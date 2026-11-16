import UIKit

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else {
            return
        }
        // Launched by scanning a `cheetah dev` QR code.
        if let url = connectionOptions.urlContexts.first?.url {
            _ = DevRuntimeLinks.handle(url)
        }

        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = RustViewController()
        window.makeKeyAndVisible()
        self.window = window
    }

    /// A `cheetah://dev` link while running: start over on the new server.
    func scene(_ scene: UIScene, openURLContexts URLContexts: Set<UIOpenURLContext>) {
        guard let url = URLContexts.first?.url, DevRuntimeLinks.handle(url) else {
            return
        }
        window?.rootViewController = RustViewController()
    }
}
