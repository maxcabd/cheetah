package com.example.rustui

/** Release builds never connect to a dev server. */
object DevRuntimeLinks {
    @Suppress("UNUSED_PARAMETER")
    fun create(host: RustHost, url: String): RuntimeLink? = null
}
