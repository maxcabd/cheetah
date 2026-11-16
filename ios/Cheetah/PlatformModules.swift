import AuthenticationServices
import LocalAuthentication
import UIKit
import UserNotifications

/*
 Built-in modules for notifications, links and sign-in. Contracts (methods,
 events) are documented on the Rust side in runtime::platform.
 */

// MARK: - notifications

/// Receives notification callbacks from launch on (AppDelegate installs it),
/// hands them to the live module, and holds a tap until one exists.
final class NotificationRelay: NSObject, UNUserNotificationCenterDelegate {
    static let shared = NotificationRelay()

    var listener: ((String, [String: Any]) -> Void)?
    private var pendingOpened: [String: Any]?

    func install() {
        UNUserNotificationCenter.current().delegate = self
    }

    func post(_ method: String, _ values: [String: Any]) {
        DispatchQueue.main.async {
            if let listener = self.listener {
                listener(method, values)
            } else if method == "opened" {
                self.pendingOpened = values
            }
        }
    }

    func takePending() -> [String: Any]? {
        defer { pendingOpened = nil }
        return pendingOpened
    }

    private func values(_ notification: UNNotification) -> [String: Any] {
        let content = notification.request.content
        return [
            "id": notification.request.identifier,
            "title": content.title,
            "body": content.body,
            "data": content.userInfo["data"] ?? [String: Any](),
        ]
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        post("received", values(notification))
        completionHandler([.banner, .sound, .list])
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        var opened = values(response.notification)
        opened.removeValue(forKey: "title")
        opened.removeValue(forKey: "body")
        post("opened", opened)
        completionHandler()
    }
}

final class NotificationsModule: CheetahApiModule {
    static let moduleName = "notifications"
    private let context: ModuleContext

    init(context: ModuleContext) {
        self.context = context
        NotificationRelay.shared.listener = { [weak self] method, values in
            self?.context.emit("notifications", method, values)
        }
        if let opened = NotificationRelay.shared.takePending() {
            context.emit("notifications", "opened", opened)
        }
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        let center = UNUserNotificationCenter.current()
        switch method {
        case "requestPermission":
            center.requestAuthorization(options: [.alert, .sound, .badge]) { [context = self.context] granted, _ in
                context.emit("notifications", "permission", ["granted": granted])
            }
        case "schedule":
            schedule(args)
        case "cancel":
            let id = args["id"] as? String ?? ""
            center.removePendingNotificationRequests(withIdentifiers: [id])
            center.removeDeliveredNotifications(withIdentifiers: [id])
        case "cancelAll":
            center.removeAllPendingNotificationRequests()
            center.removeAllDeliveredNotifications()
        case "registerPush":
            UIApplication.shared.registerForRemoteNotifications()
        case "setBadge":
            center.setBadgeCount((args["count"] as? NSNumber)?.intValue ?? 0)
        default:
            return false
        }
        return true
    }

    private func schedule(_ args: [String: Any]) {
        let content = UNMutableNotificationContent()
        content.title = args["title"] as? String ?? ""
        content.body = args["body"] as? String ?? ""
        content.sound = .default
        content.userInfo = ["data": args["data"] as? [String: Any] ?? [:]]
        let seconds = (args["seconds"] as? NSNumber)?.doubleValue ?? 0
        let trigger = seconds > 0
            ? UNTimeIntervalNotificationTrigger(timeInterval: seconds, repeats: false)
            : nil
        let id = args["id"] as? String ?? UUID().uuidString
        let request = UNNotificationRequest(identifier: id, content: content, trigger: trigger)
        UNUserNotificationCenter.current().add(request) { [context = self.context] error in
            if let error {
                context.emit("notifications", "scheduled", ["id": id, "error": error.localizedDescription])
            }
        }
    }

    func dispose() {
        NotificationRelay.shared.listener = nil
    }
}

// MARK: - linking

final class LinkingModule: CheetahApiModule {
    static let moduleName = "linking"
    private let context: ModuleContext

    init(context: ModuleContext) {
        self.context = context
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        switch method {
        case "initialUrl":
            context.emit("linking", "initial", ["url": LinkInbox.initialURL.map { $0.absoluteString as Any } ?? NSNull()])
        case "open":
            guard let url = URL(string: args["url"] as? String ?? "") else {
                context.emit("linking", "opened", ["ok": false])
                return true
            }
            UIApplication.shared.open(url) { [context = self.context] ok in
                context.emit("linking", "opened", ["ok": ok])
            }
        default:
            return false
        }
        return true
    }

    func open(url: URL) -> Bool {
        context.emit("linking", "url", ["url": url.absoluteString])
        return true
    }
}

// MARK: - auth

final class AuthModule: NSObject, CheetahApiModule, ASWebAuthenticationPresentationContextProviding,
    ASAuthorizationControllerDelegate, ASAuthorizationControllerPresentationContextProviding
{
    static let moduleName = "auth"
    private let context: ModuleContext
    private var session: ASWebAuthenticationSession?

    init(context: ModuleContext) {
        self.context = context
        super.init()
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        switch method {
        case "session":
            startSession(args)
        case "apple":
            let request = ASAuthorizationAppleIDProvider().createRequest()
            request.requestedScopes = [.fullName, .email]
            let controller = ASAuthorizationController(authorizationRequests: [request])
            controller.delegate = self
            controller.presentationContextProvider = self
            controller.performRequests()
        case "biometric":
            biometric(args["reason"] as? String ?? "Confirm it's you")
        default:
            return false
        }
        return true
    }

    private var window: ASPresentationAnchor {
        context.topController?.view.window ?? ASPresentationAnchor()
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        window
    }

    func presentationAnchor(for controller: ASAuthorizationController) -> ASPresentationAnchor {
        window
    }

    private func startSession(_ args: [String: Any]) {
        guard
            let url = URL(string: args["url"] as? String ?? ""),
            let scheme = args["callbackScheme"] as? String, !scheme.isEmpty
        else {
            context.emit("auth", "session", ["error": "url and callbackScheme are required"])
            return
        }
        let session = ASWebAuthenticationSession(url: url, callbackURLScheme: scheme) { [weak self] callback, error in
            guard let self else { return }
            self.session = nil
            if let callback {
                self.context.emit("auth", "session", ["url": callback.absoluteString])
            } else if let error = error as? ASWebAuthenticationSessionError, error.code == .canceledLogin {
                self.context.emit("auth", "session", ["error": "cancelled"])
            } else {
                self.context.emit("auth", "session", ["error": error?.localizedDescription ?? "failed"])
            }
        }
        session.presentationContextProvider = self
        session.prefersEphemeralWebBrowserSession = false
        self.session = session
        if !session.start() {
            self.session = nil
            context.emit("auth", "session", ["error": "could not start"])
        }
    }

    func authorizationController(
        controller: ASAuthorizationController,
        didCompleteWithAuthorization authorization: ASAuthorization
    ) {
        guard let credential = authorization.credential as? ASAuthorizationAppleIDCredential else {
            context.emit("auth", "apple", ["error": "unexpected credential"])
            return
        }
        let text = { (data: Data?) in data.flatMap { String(data: $0, encoding: .utf8) } ?? "" }
        let name = credential.fullName.map {
            PersonNameComponentsFormatter.localizedString(from: $0, style: .default, options: [])
        } ?? ""
        context.emit("auth", "apple", [
            "user": credential.user,
            "identityToken": text(credential.identityToken),
            "authorizationCode": text(credential.authorizationCode),
            "email": credential.email ?? "",
            "name": name,
        ])
    }

    func authorizationController(controller: ASAuthorizationController, didCompleteWithError error: Error) {
        let cancelled = (error as? ASAuthorizationError)?.code == .canceled
        context.emit("auth", "apple", ["error": cancelled ? "cancelled" : error.localizedDescription])
    }

    private func biometric(_ reason: String) {
        let check = LAContext()
        var problem: NSError?
        guard check.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &problem) else {
            context.emit("auth", "biometric", ["ok": false, "error": problem?.localizedDescription ?? "unavailable"])
            return
        }
        check.evaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, localizedReason: reason) { [context = self.context] ok, error in
            var values: [String: Any] = ["ok": ok]
            if let error {
                values["error"] = error.localizedDescription
            }
            context.emit("auth", "biometric", values)
        }
    }
}
