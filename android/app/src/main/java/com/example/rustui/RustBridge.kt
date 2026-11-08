package com.example.rustui

object RustBridge {
    init {
        System.loadLibrary("runtime")
    }

    external fun nativeNew(): Long
    external fun nativeRender(handle: Long): String?
    external fun nativeEvent(handle: Long, id: Int, kind: String, payload: String): String?
    external fun nativeFree(handle: Long)
}
