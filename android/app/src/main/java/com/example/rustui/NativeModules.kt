package com.example.rustui

import android.content.Context
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.TextView
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
    private var pendingName: String? = null
    private var pendingProps: JSONObject? = null
    private var active = false
    var onEvent: ((String, String) -> Unit)? = null

    fun configure(name: String, props: JSONObject) {
        if (module == null && name == "map" && (width <= 1 || height <= 1)) {
            pendingName = name
            pendingProps = props
            return
        }
        if (module == null) {
            materialize(name, props)
            return
        }
        module?.apply(props)
        if (name == "sheet") {
            (module as? SheetNativeModule)?.presentIfNeeded()
        }
    }

    private fun materialize(name: String, props: JSONObject) {
        module = when (name) {
            "search" -> SearchNativeModule(context) { kind, payload ->
                onEvent?.invoke(kind, payload)
            }
            "map" -> MapNativeModule(context, this)
            "sheet" -> SheetNativeModule(context, this)
            "menu" -> MenuNativeModule(context, this)
            "icon" -> IconNativeModule(context)
            "gradient" -> GradientNativeModule(context)
            else -> null
        }
        if (name != "sheet") {
            module?.nativeView?.let {
                addView(it, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
            }
        }
        module?.setActive(active)
        module?.apply(props)
        if (name == "sheet") {
            (module as? SheetNativeModule)?.presentIfNeeded()
        }
        pendingName = null
        pendingProps = null
    }

    override fun onLayout(changed: Boolean, left: Int, top: Int, right: Int, bottom: Int) {
        super.onLayout(changed, left, top, right, bottom)
        if (module == null && pendingName == "map" && width > 1 && height > 1) {
            pendingName?.let { name -> pendingProps?.let { props -> materialize(name, props) } }
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

    fun resetForReuse() {
        destroy()
        removeAllViews()
        background = null
        alpha = 1f
        translationX = 0f
        translationY = 0f
        scaleX = 1f
        scaleY = 1f
        onEvent = null
    }
}

private class MapNativeModule(
    context: Context,
    private val owner: NativeModuleHostView,
) : CheetahNativeModule {
    private val map: NativeMapHost = if (BuildConfig.MAPS_API_KEY.isBlank()) {
        MapLibreHostView(context)
    } else {
        MapHostView(context)
    }

    override val nativeView: View
        get() = map as View

    init {
        map.onMarker = { id -> owner.onEvent?.invoke("marker", id) }
    }

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

private class IconNativeModule(private val context: Context) : CheetahNativeModule {
    private val iconView = TextView(context).apply {
        gravity = Gravity.CENTER
        includeFontPadding = false
        setTextColor(Color.WHITE)
        typeface = try {
            Typeface.createFromAsset(context.assets, "fonts/FontAwesome6_Solid.ttf")
        } catch (_: Exception) {
            Typeface.DEFAULT
        }
    }

    override val nativeView: View
        get() = iconView

    override fun apply(props: JSONObject) {
        var name = ""
        var color = "#ffffff"
        if (props.has("nativeProps")) {
            val config = JSONObject(props.getString("nativeProps"))
            name = config.optString("name", name)
            color = config.optString("color", color)
        }
        if (props.has("color")) {
            color = props.getString("color")
        }
        val size = props.optDouble("size", 16.0).toFloat()
        iconView.setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
        iconView.setTextColor(glyphColor(color))
        if (name == "google") {
            iconView.typeface = Typeface.DEFAULT_BOLD
            iconView.text = "G"
        } else {
            iconView.typeface = try {
                Typeface.createFromAsset(context.assets, "fonts/FontAwesome6_Solid.ttf")
            } catch (_: Exception) {
                Typeface.DEFAULT
            }
            iconView.text = glyph(name) ?: ""
        }
        iconView.contentDescription = name
    }

    private fun glyph(name: String): String? = when (name) {
        "utensils" -> "\uF2E7"
        "arrow-up" -> "\uF062"
        "xmark" -> "\uF00D"
        "chevron-right" -> "\uF054"
        "chevron-up" -> "\uF077"
        "bookmark" -> "\uF02E"
        "share" -> "\uF064"
        "plus" -> "\u002B"
        "coffee" -> "\uF0F4"
        "hamburger" -> "\uF805"
        "fish" -> "\uF578"
        "pizza-slice" -> "\uF818"
        "car" -> "\uF1B9"
        "google" -> "\uF1A0"
        "magnifying-glass" -> "\uF002"
        "circle-xmark" -> "\uF057"
        "star" -> "\uF005"
        else -> null
    }

    private fun glyphColor(value: String): Int {
        val hex = value.removePrefix("#")
        if (hex.length == 8) {
            val red = hex.substring(0, 2).toInt(16)
            val green = hex.substring(2, 4).toInt(16)
            val blue = hex.substring(4, 6).toInt(16)
            val alpha = hex.substring(6, 8).toInt(16)
            return Color.argb(alpha, red, green, blue)
        }
        return Color.parseColor(value)
    }
}

private class GradientNativeModule(context: Context) : CheetahNativeModule {
    private val gradientView = GradientView(context)

    override val nativeView: View
        get() = gradientView

    override fun apply(props: JSONObject) {
        if (!props.has("nativeProps")) return
        gradientView.configure(JSONObject(props.getString("nativeProps")))
    }
}

private class GradientView(context: Context) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG)
    private var colors: IntArray = intArrayOf(Color.TRANSPARENT, Color.TRANSPARENT)
    private var positions: FloatArray? = null
    private var startX = 0f
    private var startY = 0f
    private var endX = 0f
    private var endY = 1f

    fun configure(config: JSONObject) {
        val array = config.optJSONArray("colors") ?: return
        colors = IntArray(array.length()) { index -> parseColor(array.optString(index)) }
        val locationArray = config.optJSONArray("locations")
        positions = if (locationArray != null) {
            FloatArray(locationArray.length()) { index ->
                locationArray.optDouble(index).toFloat()
            }
        } else {
            null
        }
        startX = config.optDouble("startX", startX.toDouble()).toFloat()
        startY = config.optDouble("startY", startY.toDouble()).toFloat()
        endX = config.optDouble("endX", endX.toDouble()).toFloat()
        endY = config.optDouble("endY", endY.toDouble()).toFloat()
        invalidate()
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        if (width <= 0 || height <= 0) return
        paint.shader = LinearGradient(
            startX * width,
            startY * height,
            endX * width,
            endY * height,
            colors,
            positions,
            Shader.TileMode.CLAMP,
        )
        canvas.drawRect(0f, 0f, width.toFloat(), height.toFloat(), paint)
    }

    private fun parseColor(value: String): Int {
        val hex = value.removePrefix("#")
        if (hex.length == 8) {
            val red = hex.substring(0, 2).toInt(16)
            val green = hex.substring(2, 4).toInt(16)
            val blue = hex.substring(4, 6).toInt(16)
            val alpha = hex.substring(6, 8).toInt(16)
            return Color.argb(alpha, red, green, blue)
        }
        return Color.parseColor(value)
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
    private var sheetHeightDp = 230f
    private var sheetRadiusDp = 28f

    override val nativeView: View
        get() = placeholder

    override fun apply(props: JSONObject) {
        if (props.has("height")) {
            sheetHeightDp = props.getDouble("height").toFloat()
        }
        if (props.has("radius")) {
            sheetRadiusDp = props.getDouble("radius").toFloat()
        }
        val current = dialog ?: return
        val density = owner.resources.displayMetrics.density
        val height = (sheetHeightDp * density).toInt()
        owner.layoutParams = (owner.layoutParams ?: FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT,
            height,
        )).apply {
            this.height = height
        }
        owner.requestLayout()
        current.window?.setLayout(ViewGroup.LayoutParams.MATCH_PARENT, height)
        current.behavior.peekHeight = height
    }

    fun presentIfNeeded() {
        if (dialog != null) return

        val density = owner.resources.displayMetrics.density
        val sheetHeight = sheetHeightDp * density
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
            val radius = sheetRadiusDp * density
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
