package com.example.rustui

object RustBridge {
    init {
        System.loadLibrary("runtime")
    }

    external fun nativeNew(host: Any, storePath: String): Long
    external fun nativeResize(handle: Long, width: Float, height: Float)
    external fun nativeEvent(handle: Long, id: Int, kind: String, payload: String)
    external fun nativeFree(handle: Long)
}
