import CoreLocation
import UIKit

/**
 Imperative native APIs.

 Every module is a small named handler. Adding a platform API means adding one
 branch here instead of changing the Rust wire protocol. Async APIs report back
 through the `native` event kind:

 {"module":"location","method":"result","ok":true,"latitude":...}
 */
final class NativeApis: NSObject, CLLocationManagerDelegate {
    typealias Emit = (String, String) -> Void

    private let emit: Emit
    private let presenter: () -> UIViewController?
    /// Runs named work put off until after the frame, timed.
    private let later: (String, () -> Void) -> Void
    private var locationManager: CLLocationManager?
    private var locationRequested = false
    private var creatingLocationManager = false
    private var canGoBack = false
    private var backGesture: UIScreenEdgePanGestureRecognizer?
    private lazy var secureStore = SecureStoreApi(emit: { [weak self] values in
        self?.emitEvent("secureStore", "result", values)
    })
    private lazy var http = HttpApi(emit: { [weak self] values in
        self?.emitEvent("http", "response", values)
    })
    /// Registered modules (built-in and the app's own): see Modules.swift.
    private lazy var modules = ApiModules(
        context: ModuleContext(emit: emit, presenter: presenter)
    )
    private var linkObserver: NSObjectProtocol?

    init(
        emit: @escaping Emit,
        presenter: @escaping () -> UIViewController?,
        later: @escaping (String, () -> Void) -> Void
    ) {
        self.emit = emit
        self.presenter = presenter
        self.later = later
        super.init()
        linkObserver = NotificationCenter.default.addObserver(
            forName: LinkInbox.notification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            if let url = note.object as? URL {
                _ = self?.modules.open(url: url)
            }
        }
    }

    func invoke(module: String, method: String, args: [String: Any]) -> Bool {
        switch module {
        case "haptics":
            return haptics(method)
        case "location":
            return location(method)
        case "system":
            return system(method, args: args)
        case "secureStore":
            return secureStore.invoke(method, args: args)
        case "http":
            return http.invoke(method, args: args)
        case "navigation":
            return navigation(method, args: args)
        default:
            return modules.invoke(module: module, method: method, args: args)
        }
    }

    func dispose() {
        if let linkObserver {
            NotificationCenter.default.removeObserver(linkObserver)
        }
        linkObserver = nil
        modules.dispose()
        http.dispose()
        if let backGesture {
            backGesture.view?.removeGestureRecognizer(backGesture)
        }
        backGesture = nil
        locationRequested = false
        locationManager?.stopUpdatingLocation()
        locationManager?.delegate = nil
        locationManager = nil
    }

    // MARK: Navigation

    /// Rust (`runtime::nav`) reports whether its stack has history. While it
    /// does, a left-edge swipe sends `navigation.back` back to Rust.
    private func navigation(_ method: String, args: [String: Any]) -> Bool {
        guard method == "setCanGoBack" else {
            return false
        }
        canGoBack = args["enabled"] as? Bool ?? false
        installBackGesture()
        backGesture?.isEnabled = canGoBack
        return true
    }

    private func installBackGesture() {
        guard backGesture == nil, let view = presenter()?.view else {
            return
        }
        let gesture = UIScreenEdgePanGestureRecognizer(
            target: self,
            action: #selector(handleBackGesture(_:))
        )
        gesture.edges = .left
        view.addGestureRecognizer(gesture)
        backGesture = gesture
    }

    @objc private func handleBackGesture(_ gesture: UIScreenEdgePanGestureRecognizer) {
        guard gesture.state == .ended, canGoBack, let view = gesture.view else {
            return
        }
        let distance = gesture.translation(in: view).x
        let velocity = gesture.velocity(in: view).x
        if distance > view.bounds.width * 0.3 || velocity > 500 {
            emitEvent("navigation", "back", [:])
        }
    }

    /// Kept and prepared: making one for each tap wakes the Taptic Engine
    /// while the frame that asked for it waits.
    private lazy var selectionHaptic = UISelectionFeedbackGenerator()
    private lazy var noticeHaptic = UINotificationFeedbackGenerator()
    private var impactHaptics: [UIImpactFeedbackGenerator.FeedbackStyle: UIImpactFeedbackGenerator] = [:]

    private func haptics(_ method: String) -> Bool {
        switch method {
        case "light":
            impact(.light)
        case "medium":
            impact(.medium)
        case "heavy":
            impact(.heavy)
        case "selection":
            after("selection") { self.selectionHaptic.selectionChanged() }
            after("prepare") { self.selectionHaptic.prepare() }
        case "success":
            notice(.success)
        case "warning":
            notice(.warning)
        case "error":
            notice(.error)
        default:
            return false
        }
        return true
    }

    private func impact(_ style: UIImpactFeedbackGenerator.FeedbackStyle) {
        let generator = impactHaptics[style] ?? UIImpactFeedbackGenerator(style: style)
        impactHaptics[style] = generator
        after("impact") { generator.impactOccurred() }
        after("prepare") { generator.prepare() }
    }

    private func notice(_ type: UINotificationFeedbackGenerator.FeedbackType) {
        after("notice") { self.noticeHaptic.notificationOccurred(type) }
        after("prepare") { self.noticeHaptic.prepare() }
    }

    /// After this frame's ops.
    private func after(_ name: String, _ work: @escaping () -> Void) {
        DispatchQueue.main.async { [later] in
            later("haptic \(name)", work)
        }
    }

    private func location(_ method: String) -> Bool {
        guard method == "request" || method == "current" else {
            return false
        }

        locationRequested = true
        if let locationManager {
            locate(locationManager)
        } else if !creatingLocationManager {
            // A new manager says what's allowed through its delegate; asking
            // it now would make the main thread wait for the answer. Made
            // after the frame: making one took up to 114 ms. It must be made
            // on the main thread, whose run loop calls its delegate.
            creatingLocationManager = true
            DispatchQueue.main.async { [weak self, later] in
                later("location manager") {
                    guard let self else { return }
                    self.creatingLocationManager = false
                    // Not if the host was torn down meanwhile.
                    guard self.locationRequested else { return }
                    let manager = CLLocationManager()
                    self.locationManager = manager
                    manager.delegate = self
                }
            }
        }
        return true
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        guard locationRequested else {
            return
        }
        locate(manager)
    }

    /// Find the location, or ask to be allowed to; the delegate hears the
    /// answer.
    private func locate(_ manager: CLLocationManager) {
        switch manager.authorizationStatus {
        case .authorizedAlways, .authorizedWhenInUse:
            manager.requestLocation()
        case .denied, .restricted:
            emitLocationError("permission_denied")
        case .notDetermined:
            manager.requestWhenInUseAuthorization()
        @unknown default:
            emitLocationError("unavailable")
        }
    }

    func locationManager(_ manager: CLLocationManager, didUpdateLocations locations: [CLLocation]) {
        guard let location = locations.last else {
            return
        }
        emitEvent("location", "result", [
            "ok": true,
            "latitude": location.coordinate.latitude,
            "longitude": location.coordinate.longitude,
            "accuracy": location.horizontalAccuracy,
        ])
    }

    func locationManager(_ manager: CLLocationManager, didFailWithError error: Error) {
        emitLocationError(error.localizedDescription)
    }

    private func emitLocationError(_ error: String) {
        emitEvent("location", "result", [
            "ok": false,
            "error": error,
        ])
    }

    private func system(_ method: String, args: [String: Any]) -> Bool {
        switch method {
        case "openUrl":
            guard
                let raw = args["url"] as? String,
                let url = URL(string: raw)
            else {
                return false
            }
            UIApplication.shared.open(url)
            return true
        case "share":
            guard let text = args["text"] as? String, !text.isEmpty else {
                return false
            }
            let controller = UIActivityViewController(
                activityItems: [text],
                applicationActivities: nil
            )
            presentShare(controller)
            return true
        case "toast":
            // {"title": "Saved", "text": "Pizza Paradise", "icon": "bookmark.fill"}
            let title = args["title"] as? String ?? ""
            let text = args["text"] as? String ?? ""
            guard !(title.isEmpty && text.isEmpty), let window = presenter()?.view.window else {
                return false
            }
            Toast.show(in: window, title: title, text: text, icon: args["icon"] as? String)
            return true
        case "clipboard.set":
            UIPasteboard.general.string = args["text"] as? String ?? ""
            return true
        case "clipboard.get":
            emitEvent("system", "clipboard", [
                "text": UIPasteboard.general.string ?? "",
            ])
            return true
        case "info":
            let device = UIDevice.current
            emitEvent("system", "info", [
                "platform": "ios",
                "model": device.model,
                "name": device.name,
                "system": device.systemName,
                "release": device.systemVersion,
            ])
            return true
        default:
            return false
        }
    }

    private func presentShare(_ controller: UIActivityViewController) {
        guard let presenter = topPresenter() else {
            return
        }
        if let popover = controller.popoverPresentationController {
            popover.sourceView = presenter.view
            popover.sourceRect = CGRect(
                x: presenter.view.bounds.midX,
                y: presenter.view.bounds.midY,
                width: 0,
                height: 0
            )
            popover.permittedArrowDirections = []
        }
        presenter.present(controller, animated: true)
    }

    private func topPresenter() -> UIViewController? {
        var controller = presenter()
        while let presented = controller?.presentedViewController {
            controller = presented
        }
        return controller
    }

    private func emitEvent(_ module: String, _ method: String, _ values: [String: Any]) {
        var payload = values
        payload["module"] = module
        payload["method"] = method
        guard
            let data = try? JSONSerialization.data(withJSONObject: payload),
            let json = String(data: data, encoding: .utf8)
        else {
            return
        }
        emit("native", json)
    }
}

/// `system.toast`: a capsule under the Dynamic Island, the way iOS confirms
/// a quick action (an SF Symbol, a bold title and a line of text). Liquid
/// Glass on iOS 26, a blur before it. It slides down, stays a moment and
/// goes; a new one replaces it, and a tap or a swipe up sends it off early.
/// It sits on the window, so it shows over pages too.
private final class Toast: UIView {
    private static weak var current: Toast?
    private static let shown: TimeInterval = 2.2

    private let effectView = UIVisualEffectView(effect: nil)
    private var hideWork: DispatchWorkItem?

    static func show(in window: UIWindow, title: String, text: String, icon: String?) {
        current?.hide()
        let toast = Toast(title: title, text: text, icon: icon)
        current = toast
        window.addSubview(toast)
        NSLayoutConstraint.activate([
            toast.centerXAnchor.constraint(equalTo: window.centerXAnchor),
            toast.topAnchor.constraint(equalTo: window.safeAreaLayoutGuide.topAnchor, constant: 6),
            toast.leadingAnchor.constraint(greaterThanOrEqualTo: window.leadingAnchor, constant: 24),
            toast.trailingAnchor.constraint(lessThanOrEqualTo: window.trailingAnchor, constant: -24),
        ])
        window.layoutIfNeeded()
        toast.appear()
        let announcement = [title, text].filter { !$0.isEmpty }.joined(separator: ", ")
        UIAccessibility.post(notification: .announcement, argument: announcement)
    }

    private init(title: String, text: String, icon: String?) {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        isAccessibilityElement = true
        accessibilityLabel = [title, text].filter { !$0.isEmpty }.joined(separator: ", ")
        accessibilityTraits = .staticText

        layer.shadowColor = UIColor.black.cgColor
        layer.shadowOpacity = 0.25
        layer.shadowRadius = 16
        layer.shadowOffset = CGSize(width: 0, height: 6)

        effectView.translatesAutoresizingMaskIntoConstraints = false
        effectView.clipsToBounds = true
        effectView.layer.cornerCurve = .continuous
        addSubview(effectView)
        effectView.effect = Self.background()
        if UIAccessibility.isReduceTransparencyEnabled {
            effectView.effect = nil
            effectView.backgroundColor = UIColor(white: 0.14, alpha: 1)
        }

        let lines = UIStackView()
        lines.axis = .vertical
        lines.spacing = 1
        if !title.isEmpty {
            let label = UILabel()
            label.text = title
            label.font = .systemFont(ofSize: 15, weight: .semibold)
            label.textColor = .white
            lines.addArrangedSubview(label)
        }
        if !text.isEmpty {
            let label = UILabel()
            label.text = text
            label.font = .systemFont(ofSize: 13, weight: .regular)
            label.textColor = UIColor(white: 1, alpha: title.isEmpty ? 1 : 0.7)
            label.lineBreakMode = .byTruncatingTail
            lines.addArrangedSubview(label)
        }

        let row = UIStackView()
        row.axis = .horizontal
        row.alignment = .center
        row.spacing = 12
        row.translatesAutoresizingMaskIntoConstraints = false
        if let icon, let image = UIImage(systemName: icon) {
            let view = UIImageView(image: image)
            view.tintColor = .white
            view.preferredSymbolConfiguration = UIImage.SymbolConfiguration(pointSize: 18, weight: .semibold)
            view.setContentHuggingPriority(.required, for: .horizontal)
            row.addArrangedSubview(view)
        }
        row.addArrangedSubview(lines)
        effectView.contentView.addSubview(row)

        NSLayoutConstraint.activate([
            effectView.topAnchor.constraint(equalTo: topAnchor),
            effectView.bottomAnchor.constraint(equalTo: bottomAnchor),
            effectView.leadingAnchor.constraint(equalTo: leadingAnchor),
            effectView.trailingAnchor.constraint(equalTo: trailingAnchor),
            row.topAnchor.constraint(equalTo: effectView.contentView.topAnchor, constant: 10),
            row.bottomAnchor.constraint(equalTo: effectView.contentView.bottomAnchor, constant: -10),
            row.leadingAnchor.constraint(equalTo: effectView.contentView.leadingAnchor, constant: 18),
            row.trailingAnchor.constraint(equalTo: effectView.contentView.trailingAnchor, constant: -22),
            heightAnchor.constraint(greaterThanOrEqualToConstant: 52),
        ])

        addGestureRecognizer(UITapGestureRecognizer(target: self, action: #selector(dismissNow)))
        let swipe = UISwipeGestureRecognizer(target: self, action: #selector(dismissNow))
        swipe.direction = .up
        addGestureRecognizer(swipe)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not used")
    }

    private static func background() -> UIVisualEffect {
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            let glass = UIGlassEffect(style: .regular)
            glass.tintColor = UIColor(white: 0, alpha: 0.35)
            return glass
        }
        #endif
        return UIBlurEffect(style: .systemThickMaterialDark)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        effectView.layer.cornerRadius = bounds.height / 2
        layer.shadowPath = UIBezierPath(roundedRect: bounds, cornerRadius: bounds.height / 2).cgPath
    }

    private var offscreen: CGAffineTransform {
        CGAffineTransform(translationX: 0, y: -(frame.maxY + 20)).scaledBy(x: 0.9, y: 0.9)
    }

    private func appear() {
        transform = offscreen
        alpha = 0
        UIView.animate(
            withDuration: 0.5, delay: 0, usingSpringWithDamping: 0.78,
            initialSpringVelocity: 0.4, options: [.allowUserInteraction]
        ) {
            self.transform = .identity
            self.alpha = 1
        }
        let work = DispatchWorkItem { [weak self] in self?.hide() }
        hideWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + Self.shown, execute: work)
    }

    @objc private func dismissNow() {
        hide()
    }

    func hide() {
        hideWork?.cancel()
        hideWork = nil
        isUserInteractionEnabled = false
        UIView.animate(withDuration: 0.3, delay: 0, options: [.curveEaseIn]) {
            self.transform = self.offscreen
            self.alpha = 0
        } completion: { _ in
            self.removeFromSuperview()
        }
    }
}
