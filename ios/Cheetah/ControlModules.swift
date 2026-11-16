import UIKit
import WebKit

/*
 Dialogs, pickers, and the switch / slider / spinner / webview views.
 Contracts: runtime::platform::{dialog, picker} and runtime::platform::controls.
 */

private func nativeConfig(_ props: [String: Any]) -> [String: Any] {
    guard
        let text = props["nativeProps"] as? String,
        let data = text.data(using: .utf8),
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
        return [:]
    }
    return object
}

private func double(_ value: Any?, _ fallback: Double) -> Double {
    (value as? NSNumber)?.doubleValue ?? fallback
}

// MARK: - dialog

final class DialogModule: CheetahApiModule {
    static let moduleName = "dialog"
    private let context: ModuleContext

    init(context: ModuleContext) {
        self.context = context
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        guard method == "alert" || method == "actionSheet", let presenter = context.topController else {
            return false
        }
        let id = args["id"] as? String ?? ""
        let destructive = (args["destructive"] as? NSNumber)?.intValue
        let sheet = method == "actionSheet"
        let labels = (args[sheet ? "options" : "buttons"] as? [String]) ?? (sheet ? [] : ["OK"])
        let title = (args["title"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let message = (args["message"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let alert = UIAlertController(
            title: title,
            message: message,
            preferredStyle: sheet ? .actionSheet : .alert
        )
        let result = { [context = self.context] (index: Int) in
            context.emit("dialog", "result", ["id": id, "index": index])
        }
        for (index, label) in labels.enumerated() {
            let style: UIAlertAction.Style = index == destructive ? .destructive : .default
            alert.addAction(UIAlertAction(title: label, style: style) { _ in result(index) })
        }
        if sheet {
            alert.addAction(UIAlertAction(title: "Cancel", style: .cancel) { _ in result(-1) })
            // iPad presents action sheets as popovers.
            if let popover = alert.popoverPresentationController {
                popover.sourceView = presenter.view
                popover.sourceRect = CGRect(x: presenter.view.bounds.midX, y: presenter.view.bounds.maxY, width: 0, height: 0)
            }
        }
        presenter.present(alert, animated: true)
        return true
    }
}

// MARK: - picker

final class PickerModule: CheetahApiModule {
    static let moduleName = "picker"
    private let context: ModuleContext

    init(context: ModuleContext) {
        self.context = context
    }

    func invoke(_ method: String, args: [String: Any]) -> Bool {
        guard method == "date", let presenter = context.topController else {
            return false
        }
        let id = args["id"] as? String ?? ""
        let mode = args["mode"] as? String ?? "date"
        let picker = UIDatePicker()
        switch mode {
        case "time":
            picker.datePickerMode = .time
            picker.preferredDatePickerStyle = .wheels
        case "datetime":
            picker.datePickerMode = .dateAndTime
            picker.preferredDatePickerStyle = .inline
        default:
            picker.datePickerMode = .date
            picker.preferredDatePickerStyle = .inline
        }
        if let value = args["value"] as? String, let date = Self.parse(value) {
            picker.date = date
        }
        let controller = DatePickerController(picker: picker) { [context = self.context] date in
            if let date {
                context.emit("picker", "result", ["id": id, "value": Self.format(date, mode: mode)])
            } else {
                context.emit("picker", "result", ["id": id, "cancelled": true])
            }
        }
        let navigation = UINavigationController(rootViewController: controller)
        if let sheet = navigation.sheetPresentationController {
            sheet.detents = [.medium(), .large()]
        }
        presenter.present(navigation, animated: true)
        return true
    }

    private static func formatter(_ format: String) -> DateFormatter {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = format
        return formatter
    }

    static func parse(_ value: String) -> Date? {
        for format in ["yyyy-MM-dd'T'HH:mm", "yyyy-MM-dd", "HH:mm"] {
            if let date = formatter(format).date(from: value) {
                if format == "HH:mm" {
                    let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
                    return Calendar.current.date(bySettingHour: parts.hour ?? 0, minute: parts.minute ?? 0, second: 0, of: Date())
                }
                return date
            }
        }
        return nil
    }

    static func format(_ date: Date, mode: String) -> String {
        switch mode {
        case "time": return formatter("HH:mm").string(from: date)
        case "datetime": return formatter("yyyy-MM-dd'T'HH:mm").string(from: date)
        default: return formatter("yyyy-MM-dd").string(from: date)
        }
    }
}

private final class DatePickerController: UIViewController, UIAdaptivePresentationControllerDelegate {
    private let picker: UIDatePicker
    private var finish: ((Date?) -> Void)?

    init(picker: UIDatePicker, finish: @escaping (Date?) -> Void) {
        self.picker = picker
        self.finish = finish
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .systemBackground
        picker.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(picker)
        NSLayoutConstraint.activate([
            picker.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 8),
            picker.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 8),
            picker.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -8),
        ])
        navigationItem.leftBarButtonItem = UIBarButtonItem(
            barButtonSystemItem: .cancel, target: self, action: #selector(cancel)
        )
        navigationItem.rightBarButtonItem = UIBarButtonItem(
            barButtonSystemItem: .done, target: self, action: #selector(done)
        )
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        navigationController?.presentationController?.delegate = self
    }

    @objc private func cancel() {
        complete(nil)
    }

    @objc private func done() {
        complete(picker.date)
    }

    /// Swiped down: same as Cancel.
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        finish?(nil)
        finish = nil
    }

    private func complete(_ date: Date?) {
        let finish = self.finish
        self.finish = nil
        dismiss(animated: true) {
            finish?(date)
        }
    }
}

// MARK: - views

/// `native("switch")`: {"value": bool, "tint": "#hex"}; sends `change` "true"/"false".
final class SwitchViewModule: NSObject, CheetahViewModule {
    static let viewName = "switch"
    private weak var owner: NativeModuleHostView?
    private let toggle = UISwitch()
    var nativeView: UIView { toggle }

    init(owner: NativeModuleHostView) {
        self.owner = owner
        super.init()
        toggle.addTarget(self, action: #selector(changed), for: .valueChanged)
    }

    func apply(props: [String: Any]) {
        let config = nativeConfig(props)
        toggle.setOn(config["value"] as? Bool ?? false, animated: true)
        if let tint = config["tint"] as? String {
            toggle.onTintColor = UIColor(hex: tint)
        }
    }

    @objc private func changed() {
        owner?.event("change", toggle.isOn ? "true" : "false")
    }
}

/// `native("slider")`: {"value", "min", "max", "step"}; sends `change` with the value.
final class SliderViewModule: NSObject, CheetahViewModule {
    static let viewName = "slider"
    private weak var owner: NativeModuleHostView?
    private let slider = UISlider()
    private var step = 0.0
    var nativeView: UIView { slider }

    init(owner: NativeModuleHostView) {
        self.owner = owner
        super.init()
        slider.addTarget(self, action: #selector(changed), for: .valueChanged)
    }

    func apply(props: [String: Any]) {
        let config = nativeConfig(props)
        slider.minimumValue = Float(double(config["min"], 0))
        slider.maximumValue = Float(double(config["max"], 1))
        step = double(config["step"], 0)
        if !slider.isTracking {
            slider.value = Float(double(config["value"], Double(slider.minimumValue)))
        }
    }

    @objc private func changed() {
        var value = Double(slider.value)
        if step > 0 {
            value = (value / step).rounded() * step
        }
        owner?.event("change", String(value))
    }
}

/// `native("spinner")`: {"color": "#hex"}.
final class SpinnerViewModule: NSObject, CheetahViewModule {
    static let viewName = "spinner"
    private let spinner = UIActivityIndicatorView(style: .medium)
    var nativeView: UIView { spinner }

    init(owner: NativeModuleHostView) {
        super.init()
        spinner.startAnimating()
    }

    func apply(props: [String: Any]) {
        if let color = nativeConfig(props)["color"] as? String {
            spinner.color = UIColor(hex: color)
        }
    }
}

/// `native("webview")`: {"url"} or {"html"}; sends `load` with the URL when a
/// page finishes. Links stay inside the view.
final class WebViewModule: NSObject, CheetahViewModule, WKNavigationDelegate {
    static let viewName = "webview"
    private weak var owner: NativeModuleHostView?
    private let web = WKWebView()
    private var loaded: String?
    var nativeView: UIView { web }

    init(owner: NativeModuleHostView) {
        self.owner = owner
        super.init()
        web.navigationDelegate = self
    }

    func apply(props: [String: Any]) {
        let config = nativeConfig(props)
        let url = config["url"] as? String ?? ""
        let html = config["html"] as? String ?? ""
        let source = url.isEmpty ? html : url
        guard source != loaded else {
            return
        }
        loaded = source
        if let target = URL(string: url), !url.isEmpty {
            web.load(URLRequest(url: target))
        } else {
            web.loadHTMLString(html, baseURL: nil)
        }
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        owner?.event("load", webView.url?.absoluteString ?? "")
    }

    func destroy() {
        web.stopLoading()
        web.navigationDelegate = nil
    }
}
