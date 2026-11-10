package com.example.rustui

import android.content.Context
import android.graphics.Color
import android.util.TypedValue
import android.view.View
import android.widget.EditText
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import androidx.appcompat.widget.SearchView
import org.json.JSONObject

interface CheetahNativeModule {
    val nativeView: View

    fun apply(props: JSONObject)

    fun setActive(active: Boolean) = Unit

    fun destroy() = Unit
}

/**
 * The Rust `native` leaf owns the box; the registered module owns the platform
 * control and its internal behavior. `hatch()` is the Android access-hatch
 * point for host-side integrations that need the concrete module instance.
 */
class NativeModuleHostView(context: Context) : FrameLayout(context) {
    private var module: CheetahNativeModule? = null
    private var active = false
    var onEvent: ((String, String) -> Unit)? = null

    fun configure(name: String, props: JSONObject) {
        if (module == null) {
            module = when (name) {
                "search" -> SearchNativeModule(context) { kind, payload ->
                    onEvent?.invoke(kind, payload)
                }
                "map" -> MapNativeModule(context)
                else -> null
            }
            module?.nativeView?.let {
                addView(it, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
            }
            module?.setActive(active)
        }
        module?.apply(props)
    }

    fun setActive(active: Boolean) {
        this.active = active
        module?.setActive(active)
    }

    fun hatch(): CheetahNativeModule? = module

    fun destroy() {
        module?.destroy()
        module = null
    }
}

private class MapNativeModule(context: Context) : CheetahNativeModule {
    private val map: NativeMapHost = if (BuildConfig.MAPS_API_KEY.isBlank()) {
        MapLibreHostView(context)
    } else {
        MapHostView(context)
    }

    override val nativeView: View
        get() = map as View

    override fun apply(props: JSONObject) {
        map.apply(props)
    }

    override fun setActive(active: Boolean) {
        if (active) {
            map.start()
            map.resume()
        } else {
            map.pause()
            map.stop()
        }
    }

    override fun destroy() {
        map.destroy()
    }
}

private class SearchNativeModule(
    context: Context,
    private val emit: (String, String) -> Unit,
) : CheetahNativeModule {
    private val search = SearchView(
        ContextThemeWrapper(
            context,
            com.google.android.material.R.style.Theme_MaterialComponents_DayNight_NoActionBar,
        ),
    )
    private var updating = false

    override val nativeView: View
        get() = search

    init {
        search.setIconifiedByDefault(false)
        search.isSubmitButtonEnabled = false
        search.setBackgroundColor(Color.TRANSPARENT)
        search.setOnQueryTextListener(object : SearchView.OnQueryTextListener {
            override fun onQueryTextSubmit(query: String?): Boolean {
                emit("submit", "")
                return true
            }

            override fun onQueryTextChange(newText: String?): Boolean {
                if (!updating) {
                    emit("change", newText ?: "")
                }
                return true
            }
        })

        search.findViewById<View>(androidx.appcompat.R.id.search_plate)
            ?.setBackgroundColor(Color.TRANSPARENT)
        search.findViewById<View>(androidx.appcompat.R.id.search_edit_frame)
            ?.setBackgroundColor(Color.TRANSPARENT)
        search.findViewById<EditText>(androidx.appcompat.R.id.search_src_text)?.apply {
            setTextColor(Color.WHITE)
            setHintTextColor(Color.parseColor("#a3a3a3"))
            setBackgroundColor(Color.TRANSPARENT)
            includeFontPadding = false
            setPadding(0, 0, 0, 0)
        }
    }

    override fun apply(props: JSONObject) {
        if (props.has("placeholder")) {
            search.queryHint = props.getString("placeholder")
        }
        val value = props.optString("value", "")
        if (search.query.toString() != value) {
            updating = true
            search.setQuery(value, false)
            updating = false
        }
        if (props.has("size")) {
            search.findViewById<EditText>(androidx.appcompat.R.id.search_src_text)?.setTextSize(
                TypedValue.COMPLEX_UNIT_SP,
                props.getDouble("size").toFloat(),
            )
        }
        if (props.has("color")) {
            search.findViewById<EditText>(androidx.appcompat.R.id.search_src_text)
                ?.setTextColor(Color.parseColor(props.getString("color")))
        }
    }

    override fun destroy() {
        search.setOnQueryTextListener(null)
    }
}
