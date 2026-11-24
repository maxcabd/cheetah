import UIKit

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {}

final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let scene = scene as? UIWindowScene else { return }
        let window = UIWindow(windowScene: scene)
        window.overrideUserInterfaceStyle = .dark
        window.rootViewController = MapScreen()
        window.makeKeyAndVisible()
        self.window = window
    }
}

/// The demo's sample places (apps/nightshade api/models.rs).
struct Place {
    let id: String
    let name: String
    let address: String
    let latitude: Double
    let longitude: Double
    let detail: String
    let hours: String
    let open: Bool

    static let samples = [
        Place(
            id: "java", name: "Java Haven", address: "Cedarwood Drive",
            latitude: 37.7793, longitude: -122.4192,
            detail: "280m • 1 min • $$", hours: "Closes at 6:00 PM", open: true
        ),
        Place(
            id: "pizza", name: "Pizza Paradise", address: "Market Street",
            latitude: 37.7849, longitude: -122.4094,
            detail: "1.2km • 2 min • $$", hours: "Closes at 11:00 PM", open: true
        ),
        Place(
            id: "sushi", name: "Sushi Zen", address: "Hayes Valley",
            latitude: 37.7694, longitude: -122.4862,
            detail: "700m • 1 min • $$$", hours: "Closes at 9:30 PM", open: true
        ),
        Place(
            id: "burger", name: "Burger Joint", address: "Mission Street",
            latitude: 37.7599, longitude: -122.4148,
            detail: "520m • 1 min • $", hours: "Opens at 10:30 AM", open: false
        ),
    ]
}

/// Sizes and colors of Nightshade's theme.view.
enum Theme {
    static let card = UIColor(red: 0x1f / 255, green: 0x1f / 255, blue: 0x1f / 255, alpha: 1)
    static let chip = UIColor(red: 0x24 / 255, green: 0x24 / 255, blue: 0x24 / 255, alpha: 1)
    static let sheet = UIColor(red: 0x17 / 255, green: 0x17 / 255, blue: 0x17 / 255, alpha: 1)
    static let muted = UIColor(red: 0xa3 / 255, green: 0xa3 / 255, blue: 0xa3 / 255, alpha: 1)
    static let green = UIColor(red: 0x4a / 255, green: 0xde / 255, blue: 0x80 / 255, alpha: 1)
    static let red = UIColor(red: 0xf8 / 255, green: 0x71 / 255, blue: 0x71 / 255, alpha: 1)

    static let cardHeight: CGFloat = 280
    static let cardHeader: CGFloat = 148
    static let carouselBottom: CGFloat = 224
    static let cardFraction: CGFloat = 0.78
    static let cardGap: CGFloat = 10
    static let resultSheet: CGFloat = 120
    static let idleSheet: CGFloat = 320

    static func font(_ name: String, _ size: CGFloat) -> UIFont {
        UIFont(name: name, size: size) ?? .systemFont(ofSize: size, weight: .medium)
    }

    static func label(_ text: String, _ font: UIFont, _ color: UIColor = .white) -> UILabel {
        let label = UILabel()
        label.text = text
        label.font = font
        label.textColor = color
        return label
    }

    static func icon(_ name: String, _ size: CGFloat, _ color: UIColor = .white) -> UIImageView {
        let view = UIImageView(
            image: UIImage(
                systemName: name,
                withConfiguration: UIImage.SymbolConfiguration(pointSize: size, weight: .medium)
            )
        )
        view.tintColor = color
        view.contentMode = .center
        return view
    }

    /// Liquid Glass on iOS 26, a blur before, like Cheetah's `glass`.
    static func glass(radius: CGFloat) -> UIVisualEffectView {
        let view: UIVisualEffectView
        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            let effect = UIGlassEffect()
            effect.isInteractive = true
            view = UIVisualEffectView(effect: effect)
        } else {
            view = UIVisualEffectView(effect: UIBlurEffect(style: .systemUltraThinMaterialDark))
        }
        #else
        view = UIVisualEffectView(effect: UIBlurEffect(style: .systemUltraThinMaterialDark))
        #endif
        view.layer.cornerRadius = radius
        view.clipsToBounds = true
        return view
    }

    static func glassButton(_ symbol: String, size: CGFloat, label: String? = nil) -> UIView {
        let view = glass(radius: size / 2)
        view.frame.size = CGSize(width: size, height: size)
        let icon = icon(symbol, size * 0.42)
        icon.frame = view.bounds
        icon.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        view.contentView.addSubview(icon)
        if let label {
            view.isAccessibilityElement = true
            view.accessibilityTraits = .button
            view.accessibilityLabel = label
        }
        return view
    }
}

/// A tap handled by a closure.
final class Tap: UITapGestureRecognizer {
    private let run: () -> Void

    init(_ run: @escaping () -> Void) {
        self.run = run
        super.init(target: nil, action: nil)
        addTarget(self, action: #selector(fire))
    }

    @objc private func fire() {
        run()
    }
}
