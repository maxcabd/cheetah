import UIKit

extension UIColor {
    convenience init(hex: String) {
        var value = hex.trimmingCharacters(in: .whitespacesAndNewlines)
        if value.hasPrefix("#") {
            value.removeFirst()
        }

        var raw: UInt64 = 0
        Scanner(string: value).scanHexInt64(&raw)

        let red: CGFloat
        let green: CGFloat
        let blue: CGFloat
        let alpha: CGFloat

        if value.count == 8 {
            red = CGFloat((raw >> 24) & 0xFF) / 255.0
            green = CGFloat((raw >> 16) & 0xFF) / 255.0
            blue = CGFloat((raw >> 8) & 0xFF) / 255.0
            alpha = CGFloat(raw & 0xFF) / 255.0
        } else {
            red = CGFloat((raw >> 16) & 0xFF) / 255.0
            green = CGFloat((raw >> 8) & 0xFF) / 255.0
            blue = CGFloat(raw & 0xFF) / 255.0
            alpha = 1.0
        }

        self.init(red: red, green: green, blue: blue, alpha: alpha)
    }
}

final class InsetLabel: UILabel {
    var insets = UIEdgeInsets.zero

    override func drawText(in rect: CGRect) {
        super.drawText(in: rect.inset(by: insets))
    }
}

/// A material container whose children live in the UIVisualEffectView's
/// contentView, so labels and controls get UIKit vibrancy automatically.
final class GlassView: UIView {
    let effectView = UIVisualEffectView(effect: nil)
    var contentView: UIView { effectView.contentView }

    private var style = "regular"
    private var tint: String?
    private var interactive = false
    private var transparencyObserver: NSObjectProtocol?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        effectView.frame = bounds
        effectView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        addSubview(effectView)
        transparencyObserver = NotificationCenter.default.addObserver(
            forName: UIAccessibility.reduceTransparencyStatusDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.refresh()
        }
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    deinit {
        if let transparencyObserver {
            NotificationCenter.default.removeObserver(transparencyObserver)
        }
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        effectView.frame = bounds
        updateShape(radius: layer.cornerRadius)
    }

    func apply(style: String, tint: String?, interactive: Bool) {
        self.style = style
        self.tint = tint
        self.interactive = interactive
        refresh()
    }

    func updateShape(radius: CGFloat) {
        layer.cornerRadius = radius
        clipsToBounds = radius > 0
        effectView.layer.cornerRadius = radius
        effectView.clipsToBounds = radius > 0
    }

    private func refresh() {
        if UIAccessibility.isReduceTransparencyEnabled {
            UIView.performWithoutAnimation {
                effectView.effect = nil
                effectView.contentView.backgroundColor = .clear
                backgroundColor = UIColor(hex: "#1e1f22")
            }
            return
        }

        #if compiler(>=6.2)
        if #available(iOS 26.0, *) {
            let glassStyle: UIGlassEffect.Style = style == "clear" ? .clear : .regular
            let effect = UIGlassEffect(style: glassStyle)
            effect.isInteractive = interactive
            if let tint {
                effect.tintColor = UIColor(hex: tint)
            }
            backgroundColor = .clear
            effectView.contentView.backgroundColor = .clear
            UIView.animate(withDuration: 0.2) {
                self.effectView.effect = effect
            }
            return
        }
        #endif

        backgroundColor = .clear
        effectView.contentView.backgroundColor = tint.map { UIColor(hex: $0) } ?? .clear
        effectView.effect = UIBlurEffect(style: .systemThinMaterialDark)
    }
}

/// Cheetah draws buttons from Rust props rather than UIKit's button styles.
/// The caption is rendered directly so it does not depend on a subview
/// layout pass, and no UIButton configuration can take over the title.
final class CheetahButton: UIControl {
    var insets = UIEdgeInsets.zero {
        didSet {
            setNeedsDisplay()
        }
    }

    var title: String? {
        didSet {
            accessibilityLabel = title
            setNeedsDisplay()
        }
    }

    var titleFont: UIFont = .systemFont(ofSize: 14) {
        didSet {
            setNeedsDisplay()
        }
    }

    var titleColor: UIColor = .white {
        didSet {
            setNeedsDisplay()
        }
    }

    var titleAlignment: NSTextAlignment = .center {
        didSet {
            setNeedsDisplay()
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isAccessibilityElement = true
        accessibilityTraits = .button
        isOpaque = false
        contentMode = .redraw
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ rect: CGRect) {
        super.draw(rect)

        guard let title, !title.isEmpty else {
            return
        }

        let content = bounds.inset(by: insets)
        guard content.width > 0, content.height > 0 else {
            return
        }

        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = titleAlignment
        paragraph.lineBreakMode = .byClipping

        let attributed = NSAttributedString(
            string: title,
            attributes: [
                .font: titleFont,
                .foregroundColor: titleColor,
                .paragraphStyle: paragraph,
            ]
        )
        let size = attributed.size()
        attributed.draw(
            in: CGRect(
                x: content.minX,
                y: content.midY - size.height / 2,
                width: content.width,
                height: size.height
            )
        )
    }

    override var isHighlighted: Bool {
        didSet {
            alpha = isHighlighted ? 0.7 : 1.0
        }
    }
}

final class InsetTextField: UITextField {
    var insets = UIEdgeInsets.zero

    override func textRect(forBounds bounds: CGRect) -> CGRect {
        bounds.inset(by: insets)
    }

    override func editingRect(forBounds bounds: CGRect) -> CGRect {
        bounds.inset(by: insets)
    }

    override func placeholderRect(forBounds bounds: CGRect) -> CGRect {
        bounds.inset(by: insets)
    }
}
