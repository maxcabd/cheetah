package com.example.rustui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.text.Editable
import android.text.InputType
import android.text.StaticLayout
import android.text.TextPaint
import android.text.TextWatcher
import android.util.Log
import android.util.LruCache
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.ScrollView
import android.widget.Space
import android.widget.TextView
import androidx.appcompat.view.ContextThemeWrapper
import com.google.android.material.bottomsheet.BottomSheetBehavior
import com.google.android.material.bottomsheet.BottomSheetDialog
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.Executors
import kotlin.math.abs
import kotlin.math.roundToInt

class RustHost(
    private val context: Context,
    private val root: FrameLayout,
    private val edgeToEdge: Boolean,
    app: String = "chat",
) {
    private var handle: Long = RustBridge.nativeNewApp(
        this,
        File(context.filesDir, "cheetah.redb").absolutePath,
        app,
    )
    private val views = HashMap<Int, View>()
    private val tags = HashMap<Int, NodeTag>()
    private val density = context.resources.displayMetrics.density
    private val mainHandler = Handler(Looper.getMainLooper())
    private var lastWidth = 0f
    private var lastHeight = 0f
    private var safeTop = 0.0
    private var safeBottom = 0.0
    private var keyboardInset = 0.0
    private var lastHostInfo: String? = null
    private val pools = HashMap<String, ArrayDeque<View>>()
    private val mapHosts = mutableListOf<NativeMapHost>()
    private var isActive = false
    private val imageCache = LruCache<String, Bitmap>(24)
    private val imageLoader = Executors.newFixedThreadPool(2)
    private var sheetDialog: BottomSheetDialog? = null

    init {
        root.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
            root.post { resizeIfNeeded() }
        }
    }

    fun start() {
        if (handle == 0L) return
        if (root.width <= 0 || root.height <= 0) {
            root.post { start() }
            return
        }
        sendHostInfo()
        resizeIfNeeded()
    }

    fun setActive(active: Boolean) {
        isActive = active
        for (map in mapHosts) {
            if (active) {
                map.start()
                map.resume()
            } else {
                map.pause()
                map.stop()
            }
        }

        val current = handle
        if (current != 0L) {
            RustBridge.nativeLifecycle(current, active)
        }
    }

    fun onWindowInsets(insets: WindowInsets) {
        if (!edgeToEdge || handle == 0L || Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return
        }

        val bars = insets.getInsets(WindowInsets.Type.systemBars())
        val ime = insets.getInsets(WindowInsets.Type.ime())
        safeTop = bars.top / density.toDouble()
        safeBottom = bars.bottom / density.toDouble()
        keyboardInset = ime.bottom / density.toDouble()
        sendHostInfo()
        root.post { resizeIfNeeded() }
    }

    private fun sendHostInfo() {
        if (handle == 0L) return

        val payload = JSONObject().apply {
            put("safeTop", safeTop)
            put("safeBottom", safeBottom)
            put("keyboardInset", keyboardInset)
            put("overlayChrome", edgeToEdge)
            put("glass", false)
            put("scale", density.toDouble())
        }.toString()

        if (payload == lastHostInfo) return
        lastHostInfo = payload
        RustBridge.nativeEvent(handle, 0, "host", payload)
    }

    private fun resizeIfNeeded() {
        if (handle == 0L) return
        if (root.width <= 0 || root.height <= 0) return

        val width = (root.width - root.paddingLeft - root.paddingRight) / density
        val height = (root.height - root.paddingTop - root.paddingBottom) / density
        if (width <= 0f || height <= 0f) return
        if (width == lastWidth && height == lastHeight) return

        lastWidth = width
        lastHeight = height
        RustBridge.nativeResize(handle, width, height)
    }

    fun onOps(json: String) {
        if (handle == 0L) return
        mainHandler.post {
            if (handle != 0L) {
                applyOps(json)
            }
        }
    }

    fun measureText(text: String, size: Float, bold: Boolean, maxWidth: Float): FloatArray {
        val metrics = context.resources.displayMetrics
        val paint = TextPaint().apply {
            isAntiAlias = true
            textSize = TypedValue.applyDimension(
                TypedValue.COMPLEX_UNIT_SP,
                size,
                metrics,
            )
            typeface = if (bold) Typeface.DEFAULT_BOLD else Typeface.DEFAULT
        }
        val limitPx = if (maxWidth.isFinite() && maxWidth > 0f) {
            (maxWidth * metrics.density).toInt().coerceAtLeast(1)
        } else {
            100_000
        }
        val layout = StaticLayout.Builder
            .obtain(text, 0, text.length, paint, limitPx)
            .setIncludePad(false)
            .build()

        var lineWidth = 0f
        for (line in 0 until layout.lineCount) {
            lineWidth = maxOf(lineWidth, layout.getLineWidth(line))
        }
        if (layout.lineCount <= 1) {
            lineWidth = maxOf(lineWidth, paint.measureText(text))
        }

        return floatArrayOf(
            (lineWidth + 1f) / metrics.density,
            layout.height.toFloat() / metrics.density,
        )
    }

    fun dispose() {
        isActive = false
        for (map in mapHosts) {
            map.destroy()
        }
        mapHosts.clear()
        sheetDialog?.dismiss()
        sheetDialog = null
        imageLoader.shutdownNow()

        val current = handle
        if (current != 0L) {
            handle = 0L
            mainHandler.removeCallbacksAndMessages(null)
            RustBridge.nativeFree(current)
        }
    }

    private fun applyOps(json: String?) {
        if (json.isNullOrBlank()) return

        val ops = JSONArray(json)
        for (index in 0 until ops.length()) {
            val op = ops.getJSONObject(index)
            when (op.getString("op")) {
                "create" -> create(op)
                "update" -> update(op)
                "children" -> children(op)
                "layout" -> layout(op)
                "remove" -> remove(op.getInt("id"))
            }
        }
    }

    private fun create(op: JSONObject) {
        val id = op.getInt("id")
        val kind = op.getString("kind")
        val view = obtainView(kind)
        val tag = NodeTag(id, view, kind)

        if (view is ScrollView) {
            val content = view.getChildAt(0) as FrameLayout
            tag.container = content
            tag.content = content
            tag.scroll = view
            content.tag = tag

            view.setOnScrollChangeListener { _, _, y, _, _ ->
                val current = view.tag as? NodeTag ?: return@setOnScrollChangeListener
                if (current.pendingScrollTo != null) {
                    return@setOnScrollChangeListener
                }
                val expected = current.lastScrollTo
                val isProgrammatic = expected != null && abs(y - expected) <= 1f
                if (!isProgrammatic && !current.scrollEventPending) {
                    val generation = current.scrollGeneration
                    current.scrollEventPending = true
                    view.postOnAnimation {
                        current.scrollEventPending = false
                        if (current.scrollGeneration == generation) {
                            event(current.id, "scroll", (view.scrollY / density).toString())
                        }
                    }
                }
            }

            content.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
                applyScroll(view, tag)
            }
        } else if (view is HorizontalScrollView) {
            val content = view.getChildAt(0) as FrameLayout
            tag.container = content
            tag.content = content
        } else if (
            kind == "column" ||
            kind == "row" ||
            kind == "glass" ||
            kind == "sheet"
        ) {
            tag.container = view as? ViewGroup
        }

        tag.root = op.optBoolean("root", false)
        view.tag = tag
        views[id] = view
        tags[id] = tag

        if (tag.root) {
            root.addView(view, FrameLayout.LayoutParams(0, 0))
        }

        applyProps(view, op.getJSONObject("props"))
        if (kind == "sheet") {
            presentSheetIfNeeded(view)
        }
    }

    private fun createView(kind: String): View = when (kind) {
        "column", "row", "glass", "sheet" -> FrameLayout(context)

        "map" -> {
            val map: NativeMapHost = if (BuildConfig.MAPS_API_KEY.isBlank()) {
                MapLibreHostView(context)
            } else {
                MapHostView(context)
            }
            mapHosts.add(map)
            if (isActive) {
                map.start()
                map.resume()
            }
            map as View
        }

        "text" -> TextView(context).apply {
            setTextColor(Color.WHITE)
            includeFontPadding = false
        }

        "button" -> TextView(context).apply {
            isClickable = true
            isFocusable = false
            gravity = Gravity.CENTER
            includeFontPadding = false
            setTextColor(Color.WHITE)
            setOnClickListener {
                val node = tag as? NodeTag ?: return@setOnClickListener
                event(node.id, "click", "")
            }
        }

        "list", "scroll" -> ScrollView(context).apply {
            isFillViewport = false
            overScrollMode = View.OVER_SCROLL_NEVER
            clipToPadding = false
            addView(
                FrameLayout(context),
                FrameLayout.LayoutParams(MATCH_PARENT, WRAP_CONTENT),
            )
        }

        "hscroll" -> HorizontalScrollView(context).apply {
            isHorizontalScrollBarEnabled = false
            overScrollMode = View.OVER_SCROLL_NEVER
            clipToPadding = false
            addView(
                FrameLayout(context),
                FrameLayout.LayoutParams(WRAP_CONTENT, MATCH_PARENT),
            )
        }

        "image" -> ImageView(context).apply {
            scaleType = ImageView.ScaleType.CENTER_CROP
            setBackgroundColor(Color.parseColor("#242424"))
        }

        "input" -> EditText(context).apply {
            setSingleLine(true)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            imeOptions = EditorInfo.IME_ACTION_SEND
            setTextColor(Color.WHITE)
            setHintTextColor(Color.parseColor("#949ba4"))
            includeFontPadding = false
            setPadding(0, 0, 0, 0)

            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(
                    s: CharSequence?,
                    start: Int,
                    count: Int,
                    after: Int,
                ) = Unit

                override fun onTextChanged(
                    s: CharSequence?,
                    start: Int,
                    before: Int,
                    count: Int,
                ) = Unit

                override fun afterTextChanged(s: Editable?) {
                    val node = tag as? NodeTag ?: return
                    if (!node.updating) {
                        event(node.id, "change", s?.toString() ?: "")
                    }
                }
            })

            setOnEditorActionListener { _, actionId, _ ->
                if (actionId == EditorInfo.IME_ACTION_SEND) {
                    val node = tag as? NodeTag
                    if (node != null) {
                        event(node.id, "submit", "")
                        post { requestFocus() }
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        }

        "space" -> Space(context)

        else -> throw IllegalArgumentException("Unknown node kind: $kind")
    }

    private fun update(op: JSONObject) {
        val view = views[op.getInt("id")] ?: return
        applyProps(view, op.getJSONObject("props"))
    }

    private fun children(op: JSONObject) {
        val parentId = op.getInt("id")
        val parent = tags[parentId]?.container ?: return

        val array = op.getJSONArray("children")
        val wanted = ArrayList<Int>(array.length())
        for (index in 0 until array.length()) {
            wanted.add(array.getInt(index))
        }

        for (index in parent.childCount - 1 downTo 0) {
            val childId = (parent.getChildAt(index).tag as? NodeTag)?.id
            if (childId == null || childId !in wanted) {
                parent.removeViewAt(index)
            }
        }

        for ((index, childId) in wanted.withIndex()) {
            val child = views[childId] ?: continue
            if (tags[childId]?.kind == "sheet") {
                presentSheetIfNeeded(child)
                continue
            }
            if (index >= parent.childCount || parent.getChildAt(index) !== child) {
                (child.parent as? ViewGroup)?.removeView(child)
                parent.addView(child, index.coerceAtMost(parent.childCount))
            }
        }
    }

    private fun layout(op: JSONObject) {
        val id = op.getInt("id")
        val view = views[id] ?: return
        if (tags[id]?.kind == "sheet") return
        val params = view.layoutParams as? FrameLayout.LayoutParams
            ?: FrameLayout.LayoutParams(0, 0)
        params.width = dp(op.getDouble("width").toFloat())
        params.height = dp(op.getDouble("height").toFloat())
        params.leftMargin = dp(op.getDouble("x").toFloat())
        params.topMargin = dp(op.getDouble("y").toFloat())
        view.layoutParams = params

        if (view is ScrollView) {
            val tag = view.tag as? NodeTag
            if (tag != null) {
                view.post { applyScroll(view, tag) }
            }
        }
    }

    private fun presentSheetIfNeeded(view: View) {
        if (sheetDialog != null) return

        val dialog = BottomSheetDialog(
            ContextThemeWrapper(context, R.style.Cheetah_SheetDialog),
        )
        dialog.setContentView(
            view,
            ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, dp(230f)),
        )
        dialog.setCancelable(false)
        dialog.setCanceledOnTouchOutside(false)
        dialog.window?.setDimAmount(0f)
        dialog.behavior.isHideable = false
        dialog.behavior.isDraggable = true
        dialog.behavior.isFitToContents = false
        dialog.behavior.peekHeight = dp(230f)
        dialog.setOnShowListener {
            dialog.behavior.state = BottomSheetBehavior.STATE_COLLAPSED
        }
        dialog.show()
        dialog.behavior.state = BottomSheetBehavior.STATE_COLLAPSED
        sheetDialog = dialog
    }

    private fun remove(id: Int) {
        val view = views.remove(id) ?: return
        val tag = tags.remove(id)
        if (tag?.kind == "sheet") {
            sheetDialog?.dismiss()
            sheetDialog = null
        }
        (view.parent as? ViewGroup)?.removeView(view)
        if (view is NativeMapHost) {
            view.destroy()
            mapHosts.remove(view)
        }

        val key = tag?.kind?.let(::poolKey) ?: return
        if (key == "container" && view is ViewGroup) {
            view.removeAllViews()
        }
        view.tag = null

        val pool = pools.getOrPut(key) { ArrayDeque() }
        if (pool.size < POOL_LIMIT) {
            pool.addLast(view)
            Log.d("CheetahPool", "pool $key (${pool.size})")
        }
    }

    private fun obtainView(kind: String): View {
        val key = poolKey(kind) ?: return createView(kind)
        val recycled = pools[key]?.removeLastOrNull() ?: return createView(kind)
        resetView(recycled, kind)
        Log.d("CheetahPool", "reuse $key (${pools[key]?.size ?: 0} left)")
        return recycled
    }

    private fun poolKey(kind: String): String? = when (kind) {
        "column", "row" -> "container"
        "text" -> "text"
        "button" -> "button"
        "space" -> "space"
        else -> null
    }

    private fun resetView(view: View, kind: String) {
        when (kind) {
            "column", "row" -> {
                (view as FrameLayout).removeAllViews()
                view.background = null
                view.setPadding(0, 0, 0, 0)
            }

            "text" -> {
                resetText(view as TextView)
                view.isClickable = false
                view.setOnClickListener(null)
            }

            "button" -> {
                resetText(view as TextView)
                view.isClickable = true
                view.isFocusable = false
                view.gravity = Gravity.CENTER
            }

            "space" -> Unit
        }
    }

    private fun resetText(view: TextView) {
        view.text = ""
        view.setTextColor(Color.WHITE)
        view.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        view.setTypeface(null, Typeface.NORMAL)
        view.gravity = Gravity.START or Gravity.TOP
        view.background = null
        view.setPadding(0, 0, 0, 0)
        view.contentDescription = null
        view.includeFontPadding = false
    }

    private fun event(id: Int, kind: String, payload: String) {
        if (handle == 0L) return
        RustBridge.nativeEvent(handle, id, kind, payload)
    }

    private fun applyProps(view: View, props: JSONObject) {
        val tag = view.tag as? NodeTag ?: return

        if (view is NativeMapHost) {
            view.apply(props)
        }

        if (view is HorizontalScrollView && props.has("contentWidth")) {
            val content = tag.content
            if (content != null) {
                val width = maxOf(
                    view.width,
                    dp(props.getDouble("contentWidth").toFloat()),
                )
                content.layoutParams = FrameLayout.LayoutParams(width, MATCH_PARENT)
                content.requestLayout()
            }
        }

        if (view is ImageView && props.has("src")) {
            val source = props.getString("src")
            if (tag.imageUrl != source) {
                tag.imageUrl = source
                view.setImageDrawable(null)
                loadImage(view, source, tag)
            }
        }

        if (view is ImageView && props.has("fit")) {
            view.scaleType = if (props.getString("fit") == "contain") {
                ImageView.ScaleType.FIT_CENTER
            } else {
                ImageView.ScaleType.CENTER_CROP
            }
        }

        if (props.has("text")) {
            val text = props.getString("text")
            if (view is TextView && view.text.toString() != text) {
                if (DEBUG_SLOT_REBINDS && text.length > 10 && view.text.isNotEmpty()) {
                    Log.d("CheetahSlot", "rebind ${tag.kind} ${tag.id}")
                }
                view.text = text
            }
        }

        if (props.has("size") && view is TextView) {
            view.setTextSize(TypedValue.COMPLEX_UNIT_SP, props.getDouble("size").toFloat())
        }

        if (props.has("color") && view is TextView) {
            view.setTextColor(Color.parseColor(props.getString("color")))
        }

        if (props.has("bold") && view is TextView) {
            view.setTypeface(
                null,
                if (props.getBoolean("bold")) Typeface.BOLD else Typeface.NORMAL,
            )
        }

        if (props.has("align") && view is TextView) {
            view.gravity = when (props.getString("align")) {
                "center" -> Gravity.CENTER
                "end" -> Gravity.END or Gravity.CENTER_VERTICAL
                else -> Gravity.START or Gravity.CENTER_VERTICAL
            }
        }

        if (props.has("bg") || props.has("radius") || (tag.kind == "glass" && props.has("glassTint"))) {
            val radius = dp(props.optDouble("radius", 0.0).toFloat()).toFloat()
            var color = when {
                props.has("bg") -> parseColor(props.getString("bg"))
                tag.kind == "glass" && props.has("glassTint") ->
                    parseColor(props.getString("glassTint"))
                else -> Color.TRANSPARENT
            }
            if (tag.kind == "glass" && radius == 0f) {
                // Android has no UIGlassEffect blur here. Use an opaque
                // fallback for the large floating bars so scrolling rows
                // never ghost through the header or composer.
                color = Color.rgb(
                    Color.red(color),
                    Color.green(color),
                    Color.blue(color),
                )
            }
            view.background = GradientDrawable().apply {
                setColor(color)
                cornerRadius = radius
            }
        }

        if (view is TextView && (props.has("pad") || props.has("padH") || props.has("padV"))) {
            val base = props.optDouble("pad", 0.0).toFloat()
            val horizontal = dp(props.optDouble("padH", base.toDouble()).toFloat())
            val vertical = dp(props.optDouble("padV", base.toDouble()).toFloat())
            view.setPadding(horizontal, vertical, horizontal, vertical)
        }

        if (props.has("label")) {
            view.contentDescription = props.getString("label")
        }

        if (props.has("placeholder") && view is EditText) {
            view.hint = props.getString("placeholder")
        }

        if (props.has("value") && view is EditText) {
            val value = props.getString("value")
            if (view.text.toString() != value) {
                tag.updating = true
                view.setText(value)
                view.setSelection(view.text.length)
                tag.updating = false
            }
        }

        if (view is ScrollView && (props.has("contentInsetTop") || props.has("contentInsetBottom"))) {
            val top = if (props.has("contentInsetTop")) {
                dp(props.getDouble("contentInsetTop").toFloat())
            } else {
                view.paddingTop
            }
            val bottom = if (props.has("contentInsetBottom")) {
                dp(props.getDouble("contentInsetBottom").toFloat())
            } else {
                view.paddingBottom
            }
            view.setPadding(0, top, 0, bottom)
        }

        if (props.has("contentHeight")) {
            val content = tag.content
            if (content != null) {
                val params = content.layoutParams as? FrameLayout.LayoutParams
                    ?: FrameLayout.LayoutParams(MATCH_PARENT, 0)
                params.height = dp(props.getDouble("contentHeight").toFloat())
                content.layoutParams = params
            }
        }

        if (props.has("scrollTo") && view is ScrollView) {
            tag.pendingScrollTo = dp(props.getDouble("scrollTo").toFloat()).toFloat()
            applyScroll(view, tag)
        }
    }

    private fun loadImage(view: ImageView, source: String, tag: NodeTag) {
        imageCache.get(source)?.let {
            view.setImageBitmap(it)
            return
        }

        imageLoader.execute {
            val bitmap = try {
                if (source.startsWith("asset://")) {
                    context.assets.open(source.removePrefix("asset://")).use(BitmapFactory::decodeStream)
                } else {
                    val connection = URL(source).openConnection() as HttpURLConnection
                    connection.connectTimeout = 8_000
                    connection.readTimeout = 12_000
                    connection.setRequestProperty("User-Agent", "Cheetah/0.1")
                    try {
                        connection.inputStream.use(BitmapFactory::decodeStream)
                    } finally {
                        connection.disconnect()
                    }
                }
            } catch (_: Exception) {
                null
            } ?: return@execute

            imageCache.put(source, bitmap)
            mainHandler.post {
                if (tag.imageUrl == source && view.tag === tag) {
                    view.setImageBitmap(bitmap)
                }
            }
        }
    }

    private fun applyScroll(view: ScrollView, tag: NodeTag) {
        val target = tag.pendingScrollTo ?: return
        if (view.height <= 0) return

        tag.lastScrollTo = target
        tag.scrollGeneration++
        tag.scrollEventPending = false
        view.scrollTo(0, target.toInt())

        if (abs(view.scrollY - target) <= 1f) {
            tag.pendingScrollTo = null
        }
    }

    private fun dp(value: Float): Int = (value * density).roundToInt()

    private fun parseColor(value: String): Int {
        val hex = value.removePrefix("#")
        if (hex.length != 8) {
            return Color.parseColor(value)
        }
        val red = hex.substring(0, 2).toInt(16)
        val green = hex.substring(2, 4).toInt(16)
        val blue = hex.substring(4, 6).toInt(16)
        val alpha = hex.substring(6, 8).toInt(16)
        return Color.argb(alpha, red, green, blue)
    }

    companion object {
        private const val POOL_LIMIT = 64
        private const val DEBUG_SLOT_REBINDS = false
        private val MATCH_PARENT = ViewGroup.LayoutParams.MATCH_PARENT
        private val WRAP_CONTENT = ViewGroup.LayoutParams.WRAP_CONTENT
    }
}
