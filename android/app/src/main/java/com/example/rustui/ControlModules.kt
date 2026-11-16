package com.example.rustui

import android.app.AlertDialog
import android.app.DatePickerDialog
import android.app.TimePickerDialog
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.Color
import android.view.View
import android.webkit.WebView
import android.webkit.WebViewClient
import android.widget.ProgressBar
import android.widget.SeekBar
import android.widget.Switch
import org.json.JSONArray
import org.json.JSONObject
import java.util.Calendar

/*
 * Dialogs, pickers, and the switch / slider / spinner / webview views.
 * Contracts: runtime::platform::{dialog, picker} and runtime::platform::controls.
 */

private fun JSONArray?.strings(): List<String> =
    if (this == null) emptyList() else (0 until length()).map { optString(it) }

class DialogModule(private val context: ModuleContext) : CheetahApiModule {
    override fun invoke(method: String, args: JSONObject): Boolean {
        val activity = context.activity ?: return false
        val id = args.optString("id")
        val result = { index: Int ->
            context.emit("dialog", "result", JSONObject().put("id", id).put("index", index))
        }
        when (method) {
            "alert" -> {
                val buttons = args.optJSONArray("buttons").strings().ifEmpty { listOf("OK") }
                val builder = AlertDialog.Builder(activity)
                    .setTitle(args.optString("title"))
                    .setMessage(args.optString("message").ifEmpty { null })
                    .setOnCancelListener { result(-1) }
                // Android shows at most three buttons: last is positive,
                // first negative, a middle one neutral.
                buttons.forEachIndexed { index, label ->
                    when {
                        index == buttons.lastIndex -> builder.setPositiveButton(label) { _, _ -> result(index) }
                        index == 0 -> builder.setNegativeButton(label) { _, _ -> result(index) }
                        else -> builder.setNeutralButton(label) { _, _ -> result(index) }
                    }
                }
                builder.show()
            }
            "actionSheet" -> {
                val options = args.optJSONArray("options").strings()
                AlertDialog.Builder(activity)
                    .setTitle(args.optString("title").ifEmpty { null })
                    .setItems(options.toTypedArray()) { _, index -> result(index) }
                    .setNegativeButton(android.R.string.cancel) { _, _ -> result(-1) }
                    .setOnCancelListener { result(-1) }
                    .show()
            }
            else -> return false
        }
        return true
    }
}

class PickerModule(private val context: ModuleContext) : CheetahApiModule {
    override fun invoke(method: String, args: JSONObject): Boolean {
        if (method != "date") return false
        val activity = context.activity ?: return false
        val id = args.optString("id")
        val mode = args.optString("mode", "date")
        val calendar = parse(args.optString("value"))
        val cancel = {
            context.emit("picker", "result", JSONObject().put("id", id).put("cancelled", true))
        }
        val done = { value: String ->
            context.emit("picker", "result", JSONObject().put("id", id).put("value", value))
        }
        val pickTime = { date: String? ->
            TimePickerDialog(
                activity,
                { _, hour, minute ->
                    val time = "%02d:%02d".format(hour, minute)
                    done(if (date == null) time else "${date}T$time")
                },
                calendar.get(Calendar.HOUR_OF_DAY),
                calendar.get(Calendar.MINUTE),
                android.text.format.DateFormat.is24HourFormat(activity),
            ).apply {
                setOnCancelListener { cancel() }
                show()
            }
        }
        if (mode == "time") {
            pickTime(null)
        } else {
            DatePickerDialog(
                activity,
                { _, year, month, day ->
                    val date = "%04d-%02d-%02d".format(year, month + 1, day)
                    if (mode == "datetime") pickTime(date) else done(date)
                },
                calendar.get(Calendar.YEAR),
                calendar.get(Calendar.MONTH),
                calendar.get(Calendar.DAY_OF_MONTH),
            ).apply {
                setOnCancelListener { cancel() }
                show()
            }
        }
        return true
    }

    /** `2026-11-16`, `14:30`, or `2026-11-16T14:30`; anything else is now. */
    private fun parse(value: String): Calendar {
        val calendar = Calendar.getInstance()
        val date = Regex("""(\d{4})-(\d{2})-(\d{2})""").find(value)
        if (date != null) {
            val (year, month, day) = date.destructured
            calendar.set(year.toInt(), month.toInt() - 1, day.toInt())
        }
        val time = Regex("""(\d{2}):(\d{2})""").find(value)
        if (time != null) {
            val (hour, minute) = time.destructured
            calendar.set(Calendar.HOUR_OF_DAY, hour.toInt())
            calendar.set(Calendar.MINUTE, minute.toInt())
        }
        return calendar
    }
}

private fun config(props: JSONObject): JSONObject =
    try {
        JSONObject(props.optString("nativeProps", "{}"))
    } catch (_: Exception) {
        JSONObject()
    }

private fun color(value: String?, fallback: Int): Int =
    try {
        if (value.isNullOrEmpty()) fallback else Color.parseColor(value)
    } catch (_: Exception) {
        fallback
    }

/** `native("switch")`: {"value": bool, "tint": "#hex"}; sends `change` "true"/"false". */
class SwitchViewModule(context: Context, private val host: NativeModuleHostView) : CheetahNativeModule {
    private val toggle = Switch(context)
    private var updating = false
    override val nativeView: View = toggle

    init {
        toggle.setOnCheckedChangeListener { _, checked ->
            if (!updating) host.onEvent?.invoke("change", checked.toString())
        }
    }

    override fun apply(props: JSONObject) {
        val config = config(props)
        updating = true
        toggle.isChecked = config.optBoolean("value", false)
        updating = false
        if (config.has("tint")) {
            val tint = ColorStateList.valueOf(color(config.optString("tint"), Color.GREEN))
            toggle.thumbTintList = tint
        }
    }
}

/** `native("slider")`: {"value", "min", "max", "step"}; sends `change` with the value. */
class SliderViewModule(context: Context, private val host: NativeModuleHostView) : CheetahNativeModule {
    private val bar = SeekBar(context)
    private var min = 0.0
    private var step = 0.01
    private var updating = false
    override val nativeView: View = bar

    init {
        bar.setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
            override fun onProgressChanged(seekBar: SeekBar, progress: Int, fromUser: Boolean) {
                if (fromUser && !updating) {
                    host.onEvent?.invoke("change", (min + progress * step).toString())
                }
            }

            override fun onStartTrackingTouch(seekBar: SeekBar) = Unit
            override fun onStopTrackingTouch(seekBar: SeekBar) = Unit
        })
    }

    override fun apply(props: JSONObject) {
        val config = config(props)
        min = config.optDouble("min", 0.0)
        val max = config.optDouble("max", 1.0)
        step = config.optDouble("step", (max - min) / 100.0).takeIf { it > 0 } ?: 0.01
        updating = true
        bar.max = ((max - min) / step).toInt().coerceAtLeast(1)
        bar.progress = ((config.optDouble("value", min) - min) / step).toInt()
        updating = false
    }
}

/** `native("spinner")`: {"color": "#hex"}. */
class SpinnerViewModule(context: Context, @Suppress("UNUSED_PARAMETER") host: NativeModuleHostView) :
    CheetahNativeModule {
    private val spinner = ProgressBar(context).apply { isIndeterminate = true }
    override val nativeView: View = spinner

    override fun apply(props: JSONObject) {
        val config = config(props)
        if (config.has("color")) {
            spinner.indeterminateTintList = ColorStateList.valueOf(color(config.optString("color"), Color.WHITE))
        }
    }
}

/**
 * `native("webview")`: {"url"} or {"html"}; sends `load` with the URL when a
 * page finishes. Links stay inside the view.
 */
class WebViewModule(context: Context, private val host: NativeModuleHostView) : CheetahNativeModule {
    private val web = WebView(context)
    private var loaded: String? = null
    override val nativeView: View = web

    init {
        @Suppress("SetJavaScriptEnabled")
        web.settings.javaScriptEnabled = true
        web.settings.domStorageEnabled = true
        web.webViewClient = object : WebViewClient() {
            override fun onPageFinished(view: WebView, url: String) {
                host.onEvent?.invoke("load", url)
            }
        }
    }

    override fun apply(props: JSONObject) {
        val config = config(props)
        val url = config.optString("url")
        val html = config.optString("html")
        val source = url.ifEmpty { html }
        if (source == loaded) return
        loaded = source
        if (url.isNotEmpty()) {
            web.loadUrl(url)
        } else {
            web.loadDataWithBaseURL(null, html, "text/html", "utf-8", null)
        }
    }

    override fun destroy() {
        web.destroy()
    }
}
