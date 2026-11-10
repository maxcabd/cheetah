import UIKit

protocol CheetahNativeModule: AnyObject {
    var nativeView: UIView { get }
    func apply(props: [String: Any])
}

/// Container for a semantic native module. The Rust layout owns this box;
/// the module owns the platform control and its internal behavior.
final class NativeModuleHostView: UIView {
    private(set) var module: CheetahNativeModule?
    var onEvent: ((String, String) -> Void)?

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(name: String, props: [String: Any]) {
        if module == nil {
            module = NativeModuleRegistry.make(name: name, owner: self)
            if let native = module?.nativeView {
                addSubview(native)
            }
        }
        module?.apply(props: props)
        setNeedsLayout()
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        module?.nativeView.frame = bounds
    }

    /// Access hatch for host-side integrations that need the platform object.
    func hatch<T>(_ type: T.Type) -> T? {
        module as? T
    }

    func event(_ kind: String, _ payload: String) {
        onEvent?(kind, payload)
    }
}

private enum NativeModuleRegistry {
    static func make(name: String, owner: NativeModuleHostView) -> CheetahNativeModule? {
        switch name {
        case "search":
            return SearchNativeModule(owner: owner)
        default:
            return nil
        }
    }
}

private final class SearchNativeModule: NSObject, CheetahNativeModule, UITextFieldDelegate {
    let field = UISearchTextField()
    weak var owner: NativeModuleHostView?
    private var updating = false

    var nativeView: UIView {
        field
    }

    init(owner: NativeModuleHostView) {
        self.owner = owner
        super.init()

        field.delegate = self
        field.borderStyle = .none
        field.textColor = .white
        field.tintColor = .white
        field.backgroundColor = .clear
        field.autocapitalizationType = .sentences
        field.returnKeyType = .send
        field.addTarget(self, action: #selector(textChanged(_:)), for: .editingChanged)
    }

    func apply(props: [String: Any]) {
        if let placeholder = props["placeholder"] as? String {
            field.attributedPlaceholder = NSAttributedString(
                string: placeholder,
                attributes: [.foregroundColor: UIColor(hex: "#a3a3a3")]
            )
        }
        if let value = props["value"] as? String, field.text != value {
            updating = true
            field.text = value
            updating = false
        }
        if let size = props["size"] as? NSNumber {
            field.font = .systemFont(ofSize: CGFloat(size.doubleValue))
        }
        if let color = props["color"] as? String {
            field.textColor = UIColor(hex: color)
        }
    }

    @objc private func textChanged(_ field: UITextField) {
        guard !updating else {
            return
        }
        owner?.event("change", field.text ?? "")
    }

    func textFieldShouldReturn(_ textField: UITextField) -> Bool {
        owner?.event("submit", "")
        return true
    }
}
