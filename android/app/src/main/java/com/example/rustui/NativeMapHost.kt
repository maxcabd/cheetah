package com.example.rustui

import org.json.JSONObject

/**
 * Lifecycle and prop surface shared by the keyed Google Maps host and the
 * keyless MapLibre host.
 */
interface NativeMapHost {
    fun start()

    fun resume()

    fun pause()

    fun stop()

    fun destroy()

    fun apply(props: JSONObject)
}
