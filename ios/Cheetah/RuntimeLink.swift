import Foundation

/// How RustHost talks to the Rust runtime. Normally the runtime is linked
/// into the app (NativeRuntimeLink, C FFI). Debug builds can instead connect
/// to tools/devserver, which runs the runtime on the developer's machine
/// (DevRuntimeLink). Either way ops arrive through RustHost.onOps and text
/// is measured through RustHost's HostContext.
protocol RuntimeLink: AnyObject {
    func resize(width: Float, height: Float)
    func event(id: UInt32, kind: String, payload: String)
    func eventCode(id: UInt32, code: UInt32, payload: String)
    func lifecycle(active: Bool)
    func free()
}

final class NativeRuntimeLink: RuntimeLink {
    private var handle: Int64

    init?(
        storePath: String,
        app: String,
        ops: CheetahOpsCallback,
        measure: CheetahMeasureCallback,
        measureBatch: CheetahMeasureBatchCallback,
        context: UnsafeMutableRawPointer
    ) {
        handle = storePath.withCString { store in
            "ws://127.0.0.1:9001".withCString { gateway in
                app.withCString { appName in
                    cheetah_new_app(store, gateway, appName, ops, measure, measureBatch, context)
                }
            }
        }
        if handle == 0 {
            return nil
        }
    }

    func resize(width: Float, height: Float) {
        guard handle != 0 else { return }
        cheetah_resize(handle, width, height)
    }

    func event(id: UInt32, kind: String, payload: String) {
        guard handle != 0 else { return }
        kind.withCString { kindPointer in
            payload.withCString { payloadPointer in
                cheetah_event(handle, id, kindPointer, payloadPointer)
            }
        }
    }

    func eventCode(id: UInt32, code: UInt32, payload: String) {
        guard handle != 0 else { return }
        payload.withCString { payloadPointer in
            cheetah_event_code(handle, id, code, payloadPointer)
        }
    }

    func lifecycle(active: Bool) {
        guard handle != 0 else { return }
        cheetah_lifecycle(handle, active ? 1 : 0)
    }

    func free() {
        let current = handle
        handle = 0
        if current != 0 {
            cheetah_free(current)
        }
    }
}
