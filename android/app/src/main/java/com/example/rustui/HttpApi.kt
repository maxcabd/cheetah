package com.example.rustui

import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.MalformedURLException
import java.net.ProtocolException
import java.net.SocketTimeoutException
import java.net.URL
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors

/**
 * `http` module (Rust: `runtime::http`).
 *
 * `request` args: `{id, method, url, headers, body?, timeoutMs}`.
 * Answers with `{"module":"http","method":"response","id",ok,status,headers,body,error?}`.
 * `ok` is transport success; 4xx/5xx still arrive with `ok: true`.
 */
class HttpApi {
    private val pool = Executors.newCachedThreadPool { runnable ->
        Thread(runnable, "cheetah-http").apply { isDaemon = true }
    }
    private val active = ConcurrentHashMap<Long, HttpURLConnection>()
    private val cancelled = ConcurrentHashMap.newKeySet<Long>()

    fun invoke(method: String, args: JSONObject, emit: (String, String) -> Unit): Boolean =
        when (method) {
            "request" -> {
                pool.execute { perform(args, emit) }
                true
            }
            "cancel" -> {
                val id = args.optLong("id", -1)
                cancelled.add(id)
                active[id]?.disconnect()
                true
            }
            else -> false
        }

    fun dispose() {
        for ((id, connection) in active) {
            cancelled.add(id)
            connection.disconnect()
        }
        pool.shutdown()
    }

    private fun perform(args: JSONObject, emit: (String, String) -> Unit) {
        val id = args.optLong("id", 0)
        val response = JSONObject()
            .put("module", "http")
            .put("method", "response")
            .put("id", id)

        if (cancelled.remove(id)) {
            emit("native", response.put("ok", false).put("error", "cancelled").toString())
            return
        }

        var connection: HttpURLConnection? = null
        try {
            connection = URL(args.getString("url")).openConnection() as HttpURLConnection
            active[id] = connection
            val timeout = args.optInt("timeoutMs", 30_000)
            connection.connectTimeout = timeout
            connection.readTimeout = timeout
            connection.instanceFollowRedirects = true
            connection.requestMethod = args.optString("method", "GET").uppercase()

            args.optJSONObject("headers")?.let { headers ->
                for (name in headers.keys()) {
                    connection.setRequestProperty(name, headers.getString(name))
                }
            }

            if (args.has("body") && !args.isNull("body")) {
                val bytes = args.getString("body").toByteArray(Charsets.UTF_8)
                connection.doOutput = true
                connection.setFixedLengthStreamingMode(bytes.size)
                connection.outputStream.use { it.write(bytes) }
            }

            val status = connection.responseCode
            val stream = if (status >= 400) connection.errorStream else connection.inputStream
            val body = stream?.use { it.readBytes().toString(Charsets.UTF_8) }.orEmpty()

            val headers = JSONObject()
            for ((name, values) in connection.headerFields) {
                if (name == null) continue
                headers.put(name.lowercase(), values.joinToString(", "))
            }

            response.put("ok", true)
                .put("status", status)
                .put("headers", headers)
                .put("body", body)
        } catch (error: Exception) {
            val code = when {
                cancelled.contains(id) -> "cancelled"
                error is SocketTimeoutException -> "timeout"
                error is MalformedURLException || error is IllegalArgumentException -> "invalid_url"
                error is ProtocolException -> "unsupported_method"
                error is IOException -> "network"
                else -> error.message ?: error.javaClass.simpleName
            }
            response.put("ok", false).put("error", code)
        } finally {
            active.remove(id)
            cancelled.remove(id)
            connection?.disconnect()
        }
        emit("native", response.toString())
    }
}
