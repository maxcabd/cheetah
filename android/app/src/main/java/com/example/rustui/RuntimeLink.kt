package com.example.rustui

/**
 * How RustHost talks to the Rust runtime. Normally the runtime is compiled
 * into the app (NativeRuntimeLink, JNI). Debug builds can instead connect to
 * tools/devserver, which runs the runtime on the developer's machine
 * (DevRuntimeLinks, debug source set only).
 *
 * Either way the runtime calls back into RustHost.onOps and
 * RustHost.measureTextBatch.
 */
interface RuntimeLink {
    /** Called once the host can render (views and window are ready). */
    fun start() {}
    fun resize(width: Float, height: Float)
    fun event(id: Int, kind: String, payload: String)
    fun eventCode(id: Int, kindCode: Int, payload: String)
    fun lifecycle(active: Boolean)
    fun free()
}

class NativeRuntimeLink private constructor(private var handle: Long) : RuntimeLink {
    override fun resize(width: Float, height: Float) {
        if (handle != 0L) RustBridge.nativeResize(handle, width, height)
    }

    override fun event(id: Int, kind: String, payload: String) {
        if (handle != 0L) RustBridge.nativeEvent(handle, id, kind, payload)
    }

    override fun eventCode(id: Int, kindCode: Int, payload: String) {
        if (handle != 0L) RustBridge.nativeEventCode(handle, id, kindCode, payload)
    }

    override fun lifecycle(active: Boolean) {
        if (handle != 0L) RustBridge.nativeLifecycle(handle, active)
    }

    override fun free() {
        val current = handle
        handle = 0L
        if (current != 0L) RustBridge.nativeFree(current)
    }

    companion object {
        fun create(host: RustHost, storePath: String, app: String): NativeRuntimeLink? {
            val handle = RustBridge.nativeNewApp(host, storePath, app)
            return if (handle != 0L) NativeRuntimeLink(handle) else null
        }
    }
}
