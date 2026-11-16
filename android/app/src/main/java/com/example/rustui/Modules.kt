package com.example.rustui

import android.app.Activity
import android.content.Context
import android.content.Intent
import android.os.Handler
import android.os.Looper
import org.json.JSONObject

/*
 * Native modules: how platform APIs and native views plug into Cheetah.
 *
 * An API module handles calls from Rust (`NativeCall::new("weather",
 * "fetch", args)`) and answers through `context.emit`, which reaches Rust as
 * a `native` event: {"module":"weather","method":"result",...}.
 *
 * A view module backs `native("chart")` elements.
 *
 * Built-in modules are listed in BuiltinModules; an app's own modules live in
 * apps/<app>/native/android, are declared in its cheetah.toml, and
 * `cheetah prebuild` lists them in GeneratedModules.kt.
 */

/** What a module can reach: Rust (emit), the activity, and its lifecycle. */
class ModuleContext(
    val context: Context,
    private val emitNative: (String, String) -> Unit,
) {
    val activity: Activity? = activityOf(context)
    private val main = Handler(Looper.getMainLooper())

    /** Send `{"module":module,"method":method,...values}` to Rust. Any thread. */
    fun emit(module: String, method: String, values: JSONObject = JSONObject()) {
        values.put("module", module).put("method", method)
        val json = values.toString()
        if (Looper.myLooper() == Looper.getMainLooper()) {
            emitNative("native", json)
        } else {
            main.post { emitNative("native", json) }
        }
    }
}

fun activityOf(context: Context?): Activity? = when (context) {
    is Activity -> context
    is android.content.ContextWrapper -> activityOf(context.baseContext)
    else -> null
}

/** The link that launched the app, for `linking.initialUrl`. */
object LinkInbox {
    @Volatile
    var initialUri: android.net.Uri? = null
}

/** An imperative native API, created on first call. */
interface CheetahApiModule {
    /** Handle [method]; false for an unknown method. */
    fun invoke(method: String, args: JSONObject): Boolean

    /** An activity result for a request this module started. */
    fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean = false

    fun onRequestPermissionsResult(requestCode: Int, grantResults: IntArray): Boolean = false

    /** The app received a link (deep link, auth redirect). */
    fun onLink(uri: android.net.Uri): Boolean = false

    fun dispose() = Unit
}

class ApiModuleSpec(val name: String, val create: (ModuleContext) -> CheetahApiModule)

class ViewModuleSpec(
    val name: String,
    val create: (Context, NativeModuleHostView) -> CheetahNativeModule,
)

object ModuleRegistry {
    val apis: Map<String, ApiModuleSpec> by lazy {
        (BuiltinModules.apis + GeneratedModules.apis).associateBy { it.name }
    }

    val views: Map<String, ViewModuleSpec> by lazy {
        (BuiltinModules.views + GeneratedModules.views).associateBy { it.name }
    }
}

/** Lazily created API module instances for one host. */
class ApiModules(private val context: ModuleContext) {
    private val instances = LinkedHashMap<String, CheetahApiModule>()

    fun invoke(module: String, method: String, args: JSONObject): Boolean =
        instance(module)?.invoke(method, args) ?: false

    fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean =
        instances.values.any { it.onActivityResult(requestCode, resultCode, data) }

    fun onRequestPermissionsResult(requestCode: Int, grantResults: IntArray): Boolean =
        instances.values.any { it.onRequestPermissionsResult(requestCode, grantResults) }

    /** Offer a link to the always-on modules and every loaded one. */
    fun onLink(uri: android.net.Uri): Boolean {
        BuiltinModules.linkHandlers.forEach { instance(it) }
        return instances.values.any { it.onLink(uri) }
    }

    fun dispose() {
        instances.values.forEach { it.dispose() }
        instances.clear()
    }

    private fun instance(name: String): CheetahApiModule? =
        instances[name] ?: ModuleRegistry.apis[name]?.create?.invoke(context)?.also {
            instances[name] = it
        }
}

/**
 * Request codes for activity results and permissions: each module takes its
 * own so results route back to the module that asked.
 */
object RequestCodes {
    private var next = 5000

    @Synchronized
    fun allocate(): Int = next++
}
