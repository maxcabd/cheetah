import UIKit

/*
 Native modules: how platform APIs and native views plug into Cheetah.

 An API module handles calls from Rust (`NativeCall::new("weather", "fetch",
 args)`) and answers through `context.emit`, which reaches Rust as a
 `native` event: {"module":"weather","method":"result",...}.

 A view module backs `native("chart")` elements: it gets the element's
 `native_props` and reports events through its host view.

 Built-in modules are listed in `BuiltinModules`; an app's own modules live
 in `apps/<app>/native/ios`, are declared in its `cheetah.toml`, and
 `cheetah prebuild` lists them in `Generated/GeneratedModules.swift`.
 */

/// What a module can reach: Rust (emit), the screen (presenter) and the
/// host's lifecycle.
final class ModuleContext {
    private let emitNative: (String, String) -> Void
    let presenter: () -> UIViewController?

    init(emit: @escaping (String, String) -> Void, presenter: @escaping () -> UIViewController?) {
        self.emitNative = emit
        self.presenter = presenter
    }

    /// Send `{"module":module,"method":method,...values}` to Rust. Any thread.
    func emit(_ module: String, _ method: String, _ values: [String: Any] = [:]) {
        var payload = values
        payload["module"] = module
        payload["method"] = method
        guard
            let data = try? JSONSerialization.data(withJSONObject: payload),
            let json = String(data: data, encoding: .utf8)
        else {
            return
        }
        if Thread.isMainThread {
            emitNative("native", json)
        } else {
            DispatchQueue.main.async { self.emitNative("native", json) }
        }
    }

    /// The top-most view controller, for presenting system UI.
    var topController: UIViewController? {
        var controller = presenter()
        while let presented = controller?.presentedViewController {
            controller = presented
        }
        return controller
    }
}

/// An imperative native API, created on first call.
protocol CheetahApiModule: AnyObject {
    /// The `module` name Rust calls.
    static var moduleName: String { get }
    init(context: ModuleContext)
    /// Handle `method`; false for an unknown method.
    func invoke(_ method: String, args: [String: Any]) -> Bool
    /// The app received a URL (deep link, auth redirect).
    func open(url: URL) -> Bool
    func dispose()
}

extension CheetahApiModule {
    func open(url: URL) -> Bool { false }
    func dispose() {}
}

/// A native view behind `native("<name>")`.
protocol CheetahViewModule: CheetahNativeModule {
    static var viewName: String { get }
    init(owner: NativeModuleHostView)
}

enum ModuleRegistry {
    static let apis: [String: CheetahApiModule.Type] = {
        var all: [String: CheetahApiModule.Type] = [:]
        for module in BuiltinModules.apis + GeneratedModules.apis {
            all[module.moduleName] = module
        }
        return all
    }()

    static let views: [String: CheetahViewModule.Type] = {
        var all: [String: CheetahViewModule.Type] = [:]
        for module in BuiltinModules.views + GeneratedModules.views {
            all[module.viewName] = module
        }
        return all
    }()
}

/// Lazily created API module instances for one host.
final class ApiModules {
    private let context: ModuleContext
    private var instances: [String: CheetahApiModule] = [:]

    init(context: ModuleContext) {
        self.context = context
    }

    func invoke(module: String, method: String, args: [String: Any]) -> Bool {
        guard let instance = instance(module) else {
            return false
        }
        return instance.invoke(method, args: args)
    }

    /// Offer a URL to every module that is loaded (or always-on, like
    /// linking); true when one handled it.
    func open(url: URL) -> Bool {
        for name in BuiltinModules.urlHandlers {
            _ = instance(name)
        }
        return instances.values.contains { $0.open(url: url) }
    }

    func dispose() {
        instances.values.forEach { $0.dispose() }
        instances.removeAll()
    }

    private func instance(_ name: String) -> CheetahApiModule? {
        if let existing = instances[name] {
            return existing
        }
        guard let type = ModuleRegistry.apis[name] else {
            return nil
        }
        let created = type.init(context: context)
        instances[name] = created
        return created
    }
}
