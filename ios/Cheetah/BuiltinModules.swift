import Foundation

/// Modules that ship with Cheetah. App modules come from GeneratedModules.
enum BuiltinModules {
    static let apis: [CheetahApiModule.Type] = [
        NotificationsModule.self,
        LinkingModule.self,
        AuthModule.self,
    ]
    static let views: [CheetahViewModule.Type] = []
    /// Modules created up front so they see every incoming URL.
    static let urlHandlers: [String] = ["linking"]
}

/// URLs the app was opened with (deep links, auth redirects). SceneDelegate
/// delivers them; each host's modules get them through `NativeApis`.
enum LinkInbox {
    static let notification = Notification.Name("CheetahOpenURL")
    /// The URL that launched the app, for `linking.initialUrl`.
    static var initialURL: URL?

    static func deliver(_ url: URL) {
        NotificationCenter.default.post(name: notification, object: url)
    }
}
