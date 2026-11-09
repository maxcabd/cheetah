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
