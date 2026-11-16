package com.example.rustui

import android.content.Context
import android.content.Intent

/** Release builds never connect to a dev server. */
object DevRuntimeLinks {
    @Suppress("UNUSED_PARAMETER")
    fun create(host: RustHost, url: String): RuntimeLink? = null

    @Suppress("UNUSED_PARAMETER")
    fun isDevLink(intent: Intent?): Boolean = false

    @Suppress("UNUSED_PARAMETER")
    fun serverFor(context: Context, intent: Intent?): String? = null
}
