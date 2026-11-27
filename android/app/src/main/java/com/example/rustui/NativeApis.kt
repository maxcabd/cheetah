package com.example.rustui

import android.app.Activity
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.pm.PackageManager
import android.location.Location
import android.location.LocationListener
import android.location.LocationManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import android.view.HapticFeedbackConstants
import android.widget.Toast
import org.json.JSONObject

/**
 * Imperative native APIs.
 *
 * Every module is a small named handler. Adding a platform API means adding one
 * branch here instead of changing the Rust wire protocol. Async APIs report
 * back through the `native` event kind:
 *
 *   {"module":"location","method":"result","ok":true,"latitude":...}
 */
class NativeApis(
    private val context: Context,
    /** (kind, payload) to Rust, for modules that emit on their own. */
    hostEmit: (String, String) -> Unit = { _, _ -> },
) {
    /** Registered modules (built-in and the app's own): see Modules.kt. */
    private val modules = ApiModules(ModuleContext(context, hostEmit))
    private val activity: Activity? = findActivity(context)
    private var locationEmit: ((String, String) -> Unit)? = null
    private var locationListener: LocationListener? = null
    private var pendingLocation = false

    private val secureStore = SecureStoreApi(context)
    private val http = HttpApi()
    private val mainHandler = Handler(Looper.getMainLooper())

    /** Set by Rust through `navigation.setCanGoBack`. */
    @Volatile
    var canGoBack: Boolean = false
        private set

    /** Background modules emit on the main thread, like location does. */
    private fun mainEmit(emit: (String, String) -> Unit): (String, String) -> Unit =
        { kind, payload ->
            if (Looper.myLooper() == Looper.getMainLooper()) {
                emit(kind, payload)
            } else {
                mainHandler.post { emit(kind, payload) }
            }
        }

    private fun navigation(method: String, args: JSONObject): Boolean {
        if (method != "setCanGoBack") return false
        canGoBack = args.optBoolean("enabled", false)
        return true
    }

    fun dispose() {
        modules.dispose()
        http.dispose()
        locationListener?.let { listener ->
            val manager = context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager
            manager?.removeUpdates(listener)
        }
        locationListener = null
        locationEmit = null
    }

    fun invoke(
        module: String,
        method: String,
        args: JSONObject,
        emit: (String, String) -> Unit,
    ): Boolean {
        if (BuildConfig.DEBUG) {
            Log.d("CheetahNative", "invoke $module.$method")
        }
        return when (module) {
            "haptics" -> haptics(method)
            "location" -> location(method, emit)
            "system" -> system(method, args, emit)
            "secureStore" -> secureStore.invoke(method, args, mainEmit(emit))
            "http" -> http.invoke(method, args, mainEmit(emit))
            "navigation" -> navigation(method, args)
            else -> modules.invoke(module, method, args)
        }
    }

    fun onActivityResult(requestCode: Int, resultCode: Int, data: android.content.Intent?): Boolean =
        modules.onActivityResult(requestCode, resultCode, data)

    fun onLink(uri: android.net.Uri): Boolean = modules.onLink(uri)

    fun onRequestPermissionsResult(requestCode: Int, grantResults: IntArray): Boolean {
        if (requestCode != LOCATION_REQUEST) {
            return modules.onRequestPermissionsResult(requestCode, grantResults)
        }
        pendingLocation = false
        val emit = locationEmit ?: return true
        if (grantResults.any { it == PackageManager.PERMISSION_GRANTED }) {
            requestLocation(emit)
        } else {
            emitLocationError(emit, "permission_denied")
        }
        return true
    }

    private fun haptics(method: String): Boolean {
        val view = activity?.window?.decorView ?: return false
        val constant = when (method) {
            "light" -> HapticFeedbackConstants.KEYBOARD_TAP
            "medium", "heavy" -> HapticFeedbackConstants.LONG_PRESS
            "selection" -> HapticFeedbackConstants.CLOCK_TICK
            "success" -> if (Build.VERSION.SDK_INT >= 30) {
                HapticFeedbackConstants.CONFIRM
            } else {
                HapticFeedbackConstants.LONG_PRESS
            }
            "warning", "error" -> if (Build.VERSION.SDK_INT >= 30) {
                HapticFeedbackConstants.REJECT
            } else {
                HapticFeedbackConstants.LONG_PRESS
            }
            else -> return false
        }
        return view.performHapticFeedback(constant)
    }

    private fun location(method: String, emit: (String, String) -> Unit): Boolean {
        if (method != "request" && method != "current") return false
        locationEmit = emit

        val fine = context.checkSelfPermission(android.Manifest.permission.ACCESS_FINE_LOCATION)
        val coarse = context.checkSelfPermission(android.Manifest.permission.ACCESS_COARSE_LOCATION)
        if (fine != PackageManager.PERMISSION_GRANTED && coarse != PackageManager.PERMISSION_GRANTED) {
            val activity = activity ?: run {
                emitLocationError(emit, "no_activity")
                return true
            }
            if (!pendingLocation) {
                pendingLocation = true
                activity.requestPermissions(
                    arrayOf(
                        android.Manifest.permission.ACCESS_FINE_LOCATION,
                        android.Manifest.permission.ACCESS_COARSE_LOCATION,
                    ),
                    LOCATION_REQUEST,
                )
            }
            return true
        }

        requestLocation(emit)
        return true
    }

    private fun requestLocation(emit: (String, String) -> Unit) {
        val manager = context.getSystemService(Context.LOCATION_SERVICE) as? LocationManager
        if (manager == null) {
            emitLocationError(emit, "unavailable")
            return
        }

        var best: Location? = null
        for (provider in listOf(
            LocationManager.GPS_PROVIDER,
            LocationManager.NETWORK_PROVIDER,
            LocationManager.PASSIVE_PROVIDER,
        )) {
            try {
                val candidate = manager.getLastKnownLocation(provider) ?: continue
                if (best == null || candidate.time > best.time) best = candidate
            } catch (_: SecurityException) {
                // Permission was revoked between the check and the read.
            }
        }
        best?.let { emitLocation(emit, it) }

        if (Build.VERSION.SDK_INT >= 30) {
            var requested = false
            val providers = listOf(LocationManager.GPS_PROVIDER, LocationManager.NETWORK_PROVIDER)
            for (provider in providers) {
                try {
                    manager.getCurrentLocation(provider, null, context.mainExecutor) { location ->
                        if (location != null) emitLocation(emit, location)
                    }
                    requested = true
                } catch (_: SecurityException) {
                    // Try the next provider.
                } catch (_: IllegalArgumentException) {
                    // Provider is absent on this device.
                }
            }
            if (!requested && best == null) emitLocationError(emit, "unavailable")
            return
        }

        val listener = object : LocationListener {
            private var settled = false

            override fun onLocationChanged(location: Location) {
                if (settled) return
                settled = true
                manager.removeUpdates(this)
                if (locationListener === this) locationListener = null
                emitLocation(emit, location)
            }

            @Deprecated("Deprecated in Android")
            override fun onStatusChanged(provider: String?, status: Int, extras: Bundle?) = Unit

            override fun onProviderEnabled(provider: String) = Unit

            override fun onProviderDisabled(provider: String) = Unit
        }

        var requested = false
        for (provider in listOf(LocationManager.NETWORK_PROVIDER, LocationManager.GPS_PROVIDER)) {
            try {
                manager.requestLocationUpdates(provider, 0L, 0f, listener, Looper.getMainLooper())
                requested = true
            } catch (_: SecurityException) {
                // Try the next provider.
            } catch (_: IllegalArgumentException) {
                // Provider is absent on this device.
            }
        }
        if (requested) {
            locationListener = listener
        } else if (best == null) {
            emitLocationError(emit, "unavailable")
        }
    }

    private fun emitLocation(emit: (String, String) -> Unit, location: Location) {
        if (BuildConfig.DEBUG) {
            Log.d("CheetahNative", "location ${location.latitude},${location.longitude}")
        }
        val payload = JSONObject()
            .put("module", "location")
            .put("method", "result")
            .put("ok", true)
            .put("latitude", location.latitude)
            .put("longitude", location.longitude)
            .put("accuracy", location.accuracy.toDouble())
        emit("native", payload.toString())
    }

    private fun emitLocationError(emit: (String, String) -> Unit, error: String) {
        if (BuildConfig.DEBUG) {
            Log.d("CheetahNative", "location error: $error")
        }
        val payload = JSONObject()
            .put("module", "location")
            .put("method", "result")
            .put("ok", false)
            .put("error", error)
        emit("native", payload.toString())
    }

    private fun system(
        method: String,
        args: JSONObject,
        emit: (String, String) -> Unit,
    ): Boolean {
        return when (method) {
        "openUrl" -> {
            val url = args.optString("url")
            if (url.isBlank()) return false
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url))
            if (activity == null) intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(intent)
            true
        }
        "share" -> {
            val text = args.optString("text")
            if (text.isBlank()) return false
            val send = Intent(Intent.ACTION_SEND).apply {
                type = "text/plain"
                putExtra(Intent.EXTRA_TEXT, text)
                args.optString("subject").takeIf { it.isNotBlank() }?.let {
                    putExtra(Intent.EXTRA_SUBJECT, it)
                }
            }
            val chooser = Intent.createChooser(send, args.optString("title", "Share"))
            if (activity == null) chooser.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            context.startActivity(chooser)
            true
        }
        "clipboard.set" -> {
            val text = args.optString("text")
            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            clipboard.setPrimaryClip(ClipData.newPlainText("cheetah", text))
            true
        }
        "clipboard.get" -> {
            val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            val text = clipboard.primaryClip
                ?.takeIf { it.itemCount > 0 }
                ?.getItemAt(0)
                ?.coerceToText(context)
                ?.toString()
                .orEmpty()
            val payload = JSONObject()
                .put("module", "system")
                .put("method", "clipboard")
                .put("text", text)
            emit("native", payload.toString())
            true
        }
        "toast" -> {
            // iOS shows `title` over `text`; Android's toast has one line.
            val text = listOf(args.optString("title"), args.optString("text"))
                .filter { it.isNotBlank() }
                .joinToString(": ")
            if (text.isNotBlank()) Toast.makeText(context, text, Toast.LENGTH_SHORT).show()
            true
        }
        "info" -> {
            val payload = JSONObject()
                .put("module", "system")
                .put("method", "info")
                .put("platform", "android")
                .put("model", Build.MODEL)
                .put("manufacturer", Build.MANUFACTURER)
                .put("release", Build.VERSION.RELEASE)
                .put("sdk", Build.VERSION.SDK_INT)
            emit("native", payload.toString())
            true
        }
        else -> false
        }
    }

    private fun findActivity(context: Context?): Activity? = when (context) {
        is Activity -> context
        is ContextWrapper -> findActivity(context.baseContext)
        else -> null
    }

    private companion object {
        const val LOCATION_REQUEST = 4101
    }
}
