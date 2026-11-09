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

/// Cheetah draws buttons from Rust props rather than UIKit's button styles.
/// A UIControl with an explicit label keeps captions visible on every iOS
/// version and avoids UIButton configuration taking over the title.
final class CheetahButton: UIControl {
    private let label = UILabel()
    var insets = UIEdgeInsets.zero {
        didSet {
            setNeedsLayout()
        }
    }

    var title: String? {
        didSet {
            label.text = title
            accessibilityLabel = title
        }
    }

    var titleFont: UIFont = .systemFont(ofSize: 14) {
        didSet {
            label.font = titleFont
        }
    }

    var titleColor: UIColor = .white {
        didSet {
            label.textColor = titleColor
        }
    }

    var titleAlignment: NSTextAlignment = .center {
        didSet {
            label.textAlignment = titleAlignment
        }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isAccessibilityElement = true
        accessibilityTraits = .button
        label.isUserInteractionEnabled = false
        label.numberOfLines = 1
        label.textAlignment = .center
        addSubview(label)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        label.frame = bounds.inset(by: insets)
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
