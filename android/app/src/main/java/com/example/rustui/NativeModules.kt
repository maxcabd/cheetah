package com.example.rustui

import android.content.Context
import android.graphics.Color
import android.graphics.drawable.GradientDrawable
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import androidx.appcompat.view.ContextThemeWrapper
import androidx.appcompat.widget.SearchView
import com.google.android.material.bottomsheet.BottomSheetBehavior
import com.google.android.material.bottomsheet.BottomSheetDialog
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
                "sheet" -> SheetNativeModule(context, this)
                "menu" -> MenuNativeModule(context, this)
                else -> null
            }
            if (name != "sheet") {
                module?.nativeView?.let {
                    addView(it, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
                }
            }
            module?.setActive(active)
        }
        module?.apply(props)
        if (name == "sheet") {
            (module as? SheetNativeModule)?.presentIfNeeded()
        }
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

private class MenuNativeModule(
    context: Context,
    private val owner: NativeModuleHostView,
) : CheetahNativeModule {
    private val button = android.widget.Button(context)
    private var items: List<String> = emptyList()

    override val nativeView: View
        get() = button

    init {
        button.background = android.graphics.drawable.ColorDrawable(Color.TRANSPARENT)
        button.isAllCaps = false
        button.minWidth = 0
        button.minimumWidth = 0
        button.setPadding(0, 0, 0, 0)
        button.gravity = Gravity.CENTER
        button.setTextColor(Color.WHITE)
        button.typeface = android.graphics.Typeface.DEFAULT_BOLD
        button.textSize = 14f
        button.setOnClickListener { showMenu() }
    }

    override fun apply(props: JSONObject) {
        if (props.has("nativeProps")) {
            val config = JSONObject(props.getString("nativeProps"))
            button.text = config.optString("title", "Menu")
            val array = config.optJSONArray("items") ?: org.json.JSONArray()
            items = (0 until array.length()).mapNotNull { array.optString(it) }
        }
        if (props.has("size")) {
            button.setTextSize(TypedValue.COMPLEX_UNIT_SP, props.getDouble("size").toFloat())
        }
        if (props.has("color")) {
            button.setTextColor(Color.parseColor(props.getString("color")))
        }
    }

    private fun showMenu() {
        val popup = android.widget.PopupMenu(button.context, button)
        items.forEachIndexed { index, item ->
            popup.menu.add(0, index, index, item)
        }
        popup.setOnMenuItemClickListener { item ->
            owner.onEvent?.invoke("select", item.title.toString())
            true
        }
        popup.show()
    }
}

private class SheetNativeModule(
    context: Context,
    private val owner: NativeModuleHostView,
) : CheetahNativeModule {
    private val placeholder = View(context)
    private var dialog: BottomSheetDialog? = null

    override val nativeView: View
        get() = placeholder

    override fun apply(props: JSONObject) = Unit

    fun presentIfNeeded() {
        if (dialog != null) return

        val density = owner.resources.displayMetrics.density
        val sheetHeight = 230f * density
        val dialog = BottomSheetDialog(
            ContextThemeWrapper(owner.context, R.style.Cheetah_SheetDialog),
        )
        dialog.setContentView(
            owner,
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, sheetHeight.toInt()),
        )
        dialog.setCancelable(false)
        dialog.setCanceledOnTouchOutside(false)
        dialog.window?.setDimAmount(0f)
        dialog.window?.addFlags(
            android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL,
        )
        dialog.behavior.isHideable = false
        dialog.behavior.isDraggable = true
        dialog.behavior.isFitToContents = false
        dialog.behavior.peekHeight = sheetHeight.toInt()
        dialog.setOnShowListener {
            dialog.behavior.state = BottomSheetBehavior.STATE_COLLAPSED
            val sheet = dialog.findViewById<View>(
                com.google.android.material.R.id.design_bottom_sheet,
            )
            val radius = 28f * density
            sheet?.background = GradientDrawable().apply {
                setColor(Color.parseColor("#171717"))
                cornerRadii = floatArrayOf(
                    radius,
                    radius,
                    radius,
                    radius,
                    0f,
                    0f,
                    0f,
                    0f,
                )
            }
            dialog.window?.setLayout(
                ViewGroup.LayoutParams.MATCH_PARENT,
                sheetHeight.toInt(),
            )
            dialog.window?.setGravity(Gravity.BOTTOM)
        }
        dialog.setOnDismissListener {
            if (this.dialog === dialog) {
                this.dialog = null
            }
        }
        dialog.show()
        dialog.behavior.state = BottomSheetBehavior.STATE_COLLAPSED
        this.dialog = dialog
    }

    override fun destroy() {
        dialog?.dismiss()
        dialog = null
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
