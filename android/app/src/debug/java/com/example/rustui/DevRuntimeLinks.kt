package com.example.rustui

import android.os.Handler
import android.os.Looper
import android.util.Log
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/** Debug builds: connect RustHost to tools/devserver instead of the bundled runtime. */
object DevRuntimeLinks {
    fun create(host: RustHost, url: String): RuntimeLink = DevRuntimeLink(host, url)
}

/**
 * RuntimeLink over WebSocket (protocol in tools/devserver/src/main.rs).
 * Reconnects automatically; every (re)connection starts a fresh app on the
 * server, so the host clears its views and re-sends its size and insets.
 */
private class DevRuntimeLink(
    private val host: RustHost,
    private val url: String,
) : RuntimeLink {
    private val client = OkHttpClient.Builder()
        .readTimeout(0, TimeUnit.MILLISECONDS)
        .pingInterval(10, TimeUnit.SECONDS)
        .build()
    private val main = Handler(Looper.getMainLooper())

    @Volatile private var socket: WebSocket? = null
    @Volatile private var started = false
    @Volatile private var freed = false
    private var announcedWaiting = false

    override fun start() {
        if (started) return
        started = true
        connect()
    }

    private fun connect() {
        if (freed) return
        val request = Request.Builder().url(url).build()
        client.newWebSocket(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                socket = webSocket
                announcedWaiting = false
                Log.i(TAG, "connected to $url")
                main.post { if (!freed) host.onRuntimeConnected() }
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                host.onOps(bytes.toByteArray())
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                val message = JSONObject(text)
                if (message.optString("t") != "measure") return
                val items = message.getJSONArray("items")
                val count = items.length()
                val texts = Array(count) { items.getJSONArray(it).getString(0) }
                val sizes = FloatArray(count) { items.getJSONArray(it).getDouble(1).toFloat() }
                val bold = BooleanArray(count) { items.getJSONArray(it).getBoolean(2) }
                val widths = FloatArray(count) { items.getJSONArray(it).getDouble(3).toFloat() }
                val measured = host.measureTextBatch(texts, sizes, bold, widths)
                val out = JSONArray()
                for (value in measured) out.put(value.toDouble())
                webSocket.send(
                    JSONObject()
                        .put("t", "measured")
                        .put("seq", message.getLong("seq"))
                        .put("sizes", out)
                        .toString(),
                )
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = retry()

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (!announcedWaiting) {
                    Log.w(TAG, "waiting for dev server at $url (${t.message})")
                    announcedWaiting = true
                }
                retry()
            }
        })
    }

    private fun retry() {
        socket = null
        // Short: the server is usually back within moments of a rebuild.
        if (!freed) main.postDelayed({ connect() }, 200)
    }

    private fun send(json: JSONObject) {
        socket?.send(json.toString())
    }

    override fun resize(width: Float, height: Float) {
        send(JSONObject().put("t", "resize").put("w", width.toDouble()).put("h", height.toDouble()))
    }

    override fun event(id: Int, kind: String, payload: String) {
        send(JSONObject().put("t", "event").put("id", id).put("kind", kind).put("payload", payload))
    }

    override fun eventCode(id: Int, kindCode: Int, payload: String) {
        send(JSONObject().put("t", "event").put("id", id).put("code", kindCode).put("payload", payload))
    }

    override fun lifecycle(active: Boolean) {
        send(JSONObject().put("t", "lifecycle").put("active", active))
    }

    override fun free() {
        freed = true
        main.removeCallbacksAndMessages(null)
        socket?.close(1000, "host disposed")
        socket = null
        client.dispatcher.executorService.shutdown()
    }

    companion object {
        private const val TAG = "CheetahDev"
    }
}
