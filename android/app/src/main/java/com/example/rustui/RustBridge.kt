package com.example.rustui

object RustBridge {
    init {
        System.loadLibrary("runtime")
    }

    external fun nativeNew(measurer: Any): Long
    external fun nativeRender(handle: Long, width: Float, height: Float): String?
    external fun nativeEvent(handle: Long, id: Int, kind: String, payload: String): String?
    external fun nativeFree(handle: Long)
}
