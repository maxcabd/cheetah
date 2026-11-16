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
    private var locationManager: CLLocationManager?
    private var locationRequested = false
    private var canGoBack = false
    private var backGesture: UIScreenEdgePanGestureRecognizer?
    private lazy var secureStore = SecureStoreApi(emit: { [weak self] values in
        self?.emitEvent("secureStore", "result", values)
    })
    private lazy var http = HttpApi(emit: { [weak self] values in
        self?.emitEvent("http", "response", values)
    })

    init(emit: @escaping Emit, presenter: @escaping () -> UIViewController?) {
        self.emit = emit
        self.presenter = presenter
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
            return false
        }
    }

    func dispose() {
        http.dispose()
        if let backGesture {
            backGesture.view?.removeGestureRecognizer(backGesture)
        }
        backGesture = nil
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

    private func haptics(_ method: String) -> Bool {
        switch method {
        case "light":
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
        case "medium":
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
        case "heavy":
            UIImpactFeedbackGenerator(style: .heavy).impactOccurred()
        case "selection":
            UISelectionFeedbackGenerator().selectionChanged()
        case "success":
            UINotificationFeedbackGenerator().notificationOccurred(.success)
        case "warning":
            UINotificationFeedbackGenerator().notificationOccurred(.warning)
        case "error":
            UINotificationFeedbackGenerator().notificationOccurred(.error)
        default:
            return false
        }
        return true
    }

    private func location(_ method: String) -> Bool {
        guard method == "request" || method == "current" else {
            return false
        }

        let manager = locationManager ?? CLLocationManager()
        locationManager = manager
        manager.delegate = self
        locationRequested = true

        switch manager.authorizationStatus {
        case .notDetermined:
            manager.requestWhenInUseAuthorization()
        case .authorizedAlways, .authorizedWhenInUse:
            manager.requestLocation()
        case .denied, .restricted:
            emitLocationError("permission_denied")
        @unknown default:
            emitLocationError("unavailable")
        }
        return true
    }

    func locationManagerDidChangeAuthorization(_ manager: CLLocationManager) {
        guard locationRequested else {
            return
        }
        switch manager.authorizationStatus {
        case .authorizedAlways, .authorizedWhenInUse:
            manager.requestLocation()
        case .denied, .restricted:
            emitLocationError("permission_denied")
        case .notDetermined:
            break
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
