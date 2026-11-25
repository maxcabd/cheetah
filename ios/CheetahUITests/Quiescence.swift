import ObjectiveC
import XCTest

/// XCTest waits for the app in front to go idle before every tap, also one at
/// a position. An app with an endless animation never does, and each tap then
/// takes a minute (the React Native app). Measurements here have their own
/// fixed windows, so the wait is turned off, for every app alike.
///
/// The methods are private to XCTest; if they're gone, `disable` says so.
enum Quiescence {
    private static var originals: [(method: Method, implementation: IMP)] = []

    static func disable() -> Bool {
        guard originals.isEmpty, let process = NSClassFromString("XCUIApplicationProcess") else {
            return !originals.isEmpty
        }
        let one: @convention(block) (AnyObject, Bool) -> Void = { _, _ in }
        let two: @convention(block) (AnyObject, Bool, Bool) -> Void = { _, _, _ in }
        replace(process, "waitForQuiescenceIncludingAnimationsIdle:", with: imp_implementationWithBlock(one))
        replace(
            process,
            "waitForQuiescenceIncludingAnimationsIdle:isPreEvent:",
            with: imp_implementationWithBlock(two)
        )
        return !originals.isEmpty
    }

    static func restore() {
        for (method, implementation) in originals {
            method_setImplementation(method, implementation)
        }
        originals = []
    }

    private static func replace(_ type: AnyClass, _ name: String, with implementation: IMP) {
        guard let method = class_getInstanceMethod(type, NSSelectorFromString(name)) else { return }
        originals.append((method, method_setImplementation(method, implementation)))
    }
}
