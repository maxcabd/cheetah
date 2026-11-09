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

enum DebugLog {
    private static let url = FileManager.default
        .urls(for: .documentDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("cheetah-debug.log")

    static func reset() {
        try? FileManager.default.removeItem(at: url)
    }

    static func write(_ message: String) {
        guard let data = (message + "\n").data(using: .utf8) else {
            return
        }

        if let handle = try? FileHandle(forWritingTo: url) {
            defer { try? handle.close() }
            handle.seekToEndOfFile()
            handle.write(data)
        } else {
            try? data.write(to: url)
        }
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
