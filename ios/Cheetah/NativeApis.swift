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
