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
import java.nio.ByteBuffer
import java.nio.ByteOrder
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
    private val typefaceCache = HashMap<String, Typeface>()
    private val imageLoader = Executors.newFixedThreadPool(2)
    private var sheetDialog: BottomSheetDialog? = null
    private var perfCreateUs = 0L
    private var perfUpdateUs = 0L
    private var perfLayoutUs = 0L
    private var perfChildrenUs = 0L

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
        for (view in views.values) {
            if (view is NativeModuleHostView) {
                view.setActive(active)
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

    fun onOps(bytes: ByteArray) {
        if (handle == 0L) return
        mainHandler.post {
            if (handle != 0L) {
                val started = System.nanoTime()
                perfCreateUs = 0
                perfUpdateUs = 0
                perfLayoutUs = 0
                perfChildrenUs = 0
                applyOps(bytes)
                if (BuildConfig.DEBUG) {
                    Log.d(
                        "CheetahPerf",
                        "apply_us=${(System.nanoTime() - started) / 1000} " +
                            "create_us=${perfCreateUs / 1000} update_us=${perfUpdateUs / 1000} " +
                            "children_us=${perfChildrenUs / 1000} layout_us=${perfLayoutUs / 1000} " +
                            "op_bytes=${bytes.size}",
                    )
                }
            }
        }
    }

    fun measureTextBatch(
        texts: Array<String>,
        sizes: FloatArray,
        bold: BooleanArray,
        widths: FloatArray,
    ): FloatArray {
        val result = FloatArray(texts.size * 2)
        for (index in texts.indices) {
            val measured = measureText(
                texts[index],
                sizes.getOrElse(index) { 14f },
                bold.getOrElse(index) { false },
                widths.getOrElse(index) { 1f },
            )
            result[index * 2] = measured[0]
            result[index * 2 + 1] = measured[1]
        }
        return result
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
        for (view in views.values) {
            if (view is NativeModuleHostView) {
                view.destroy()
            }
        }
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

    private fun applyOps(bytes: ByteArray) {
        if (bytes.isEmpty()) return
        val reader = WireReader(bytes)
        if (!reader.magic()) return
        val count = reader.u32() ?: return
        for (index in 0 until count) {
            val tag = reader.u8() ?: return
            when (tag) {
                1 -> {
                    val id = reader.u32() ?: return
                    val kind = reader.string() ?: return
                    val root = reader.u8() ?: return
                    val sourceFlag = reader.u8() ?: return
                    val source = if (sourceFlag != 0) {
                        val file = reader.string() ?: return
                        val line = reader.u32() ?: return
                        val column = reader.u32() ?: return
                        JSONObject().put("file", file).put("line", line).put("column", column)
                    } else {
                        null
                    }
                    val props = reader.props() ?: return
                    val op = JSONObject()
                        .put("op", "create")
                        .put("id", id)
                        .put("kind", kind)
                        .put("root", root != 0)
                        .put("props", props)
                    if (source != null) op.put("source", source)
                    create(op)
                }
                2 -> {
                    val id = reader.u32() ?: return
                    val props = reader.props() ?: return
                    update(JSONObject().put("op", "update").put("id", id).put("props", props))
                }
                3 -> {
                    val id = reader.u32() ?: return
                    val childCount = reader.u32() ?: return
                    val children = JSONArray()
                    for (child in 0 until childCount) {
                        children.put(reader.u32() ?: return)
                    }
                    children(JSONObject().put("op", "children").put("id", id).put("children", children))
                }
                4 -> {
                    val id = reader.u32() ?: return
                    val x = reader.f32() ?: return
                    val y = reader.f32() ?: return
                    val width = reader.f32() ?: return
                    val height = reader.f32() ?: return
                    layout(
                        JSONObject()
                            .put("op", "layout")
                            .put("id", id)
                            .put("x", x)
                            .put("y", y)
                            .put("width", width)
                            .put("height", height),
                    )
                }
                5 -> {
                    val id = reader.u32() ?: return
                    remove(id.toInt())
                }
                6 -> {
                    val id = reader.u32() ?: return
                    val text = reader.string() ?: return
                    (views[id.toInt()] as? TextView)?.text = text
                }
                else -> return
            }
        }
    }

    private fun create(op: JSONObject) {
        val started = System.nanoTime()
        try {
            createInner(op)
        } finally {
            perfCreateUs += System.nanoTime() - started
        }
    }

    private fun createInner(op: JSONObject) {
        val id = op.getInt("id")
        val kind = op.getString("kind")
        val view = obtainView(kind)
        val tag = NodeTag(id, view, kind)
        val props = op.getJSONObject("props")
        tag.native = props.optString("native").takeIf { it.isNotEmpty() }
        val source = op.optJSONObject("source")
        if (source != null) {
            val file = source.optString("file")
            val line = source.optInt("line")
            tag.source = "$file:$line"
            if (BuildConfig.DEBUG) {
                Log.d("CheetahSource", "${tag.kind} ${tag.id} -> ${tag.source}")
            }
        }

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
        } else if (view is CheetahHScroll) {
            val content = view.getChildAt(0) as FrameLayout
            tag.container = content
            tag.content = content
            view.onPageChanged = { page ->
                event(tag.id, "page", page.toString())
            }
        } else if (
            kind == "column" ||
            kind == "row" ||
            kind == "glass" ||
            kind == "sheet" ||
            tag.isSheet
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

        applyProps(view, props)
        if (props.optBoolean("animate", false)) {
            val target = view.alpha
            view.alpha = 0f
            view.animate()
                .alpha(target)
                .setDuration(220L)
                .start()
        }
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

        "hscroll" -> CheetahHScroll(context).apply {
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

        "native" -> NativeModuleHostView(context)

        "input" -> EditText(context).apply {
            setSingleLine(true)
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
            imeOptions = EditorInfo.IME_ACTION_SEND
            setTextColor(Color.WHITE)
            setHintTextColor(Color.parseColor("#949ba4"))
            setBackgroundColor(Color.TRANSPARENT)
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

            setOnFocusChangeListener { _, hasFocus ->
                val node = tag as? NodeTag ?: return@setOnFocusChangeListener
                event(node.id, if (hasFocus) "focus" else "blur", "")
            }
        }

        "space" -> Space(context)

        else -> throw IllegalArgumentException("Unknown node kind: $kind")
    }

    private fun update(op: JSONObject) {
        val started = System.nanoTime()
        try {
            val view = views[op.getInt("id")] ?: return
            applyProps(view, op.getJSONObject("props"))
        } finally {
            perfUpdateUs += System.nanoTime() - started
        }
    }

    private fun children(op: JSONObject) {
        val started = System.nanoTime()
        try {
            childrenInner(op)
        } finally {
            perfChildrenUs += System.nanoTime() - started
        }
    }

    private fun childrenInner(op: JSONObject) {
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
            if (tags[childId]?.isSheet == true) {
                if (tags[childId]?.kind == "sheet") {
                    presentSheetIfNeeded(child)
                }
                continue
            }
            if (index >= parent.childCount || parent.getChildAt(index) !== child) {
                (child.parent as? ViewGroup)?.removeView(child)
                parent.addView(child, index.coerceAtMost(parent.childCount))
            }
        }
    }

    private fun layout(op: JSONObject) {
        val started = System.nanoTime()
        try {
            layoutInner(op)
        } finally {
            perfLayoutUs += System.nanoTime() - started
        }
    }

    private fun layoutInner(op: JSONObject) {
        val id = op.getInt("id")
        val view = views[id] ?: return
        if (tags[id]?.isSheet == true) return
        val params = view.layoutParams as? FrameLayout.LayoutParams
            ?: FrameLayout.LayoutParams(0, 0)
        params.width = dp(op.getDouble("width").toFloat())
        params.height = dp(op.getDouble("height").toFloat())
        params.leftMargin = dp(op.getDouble("x").toFloat())
        params.topMargin = dp(op.getDouble("y").toFloat())
        planLayoutTransition(view)
        view.layoutParams = params

        if (view is ScrollView) {
            val tag = view.tag as? NodeTag
            if (tag != null) {
                view.post { applyScroll(view, tag) }
            }
        }
    }

    private fun planLayoutTransition(view: View) {
        val tag = view.tag as? NodeTag ?: return
        if (!tag.animate || view.width <= 0 || view.height <= 0) return
        val oldX = view.left
        val oldY = view.top
        view.post {
            if (view.left != oldX || view.top != oldY) {
                view.translationX = (oldX - view.left).toFloat()
                view.translationY = (oldY - view.top).toFloat()
                view.animate()
                    .translationX(0f)
                    .translationY(0f)
                    .setDuration(200L)
                    .start()
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
        dialog.window?.addFlags(android.view.WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL)
        dialog.behavior.isHideable = false
        dialog.behavior.isDraggable = true
        dialog.behavior.isFitToContents = false
        dialog.behavior.peekHeight = dp(230f)
        dialog.setOnShowListener {
            dialog.behavior.state = BottomSheetBehavior.STATE_COLLAPSED
            val sheet = dialog.findViewById<View>(
                com.google.android.material.R.id.design_bottom_sheet,
            )
            val radius = dp(28f).toFloat()
            sheet?.background = GradientDrawable().apply {
                setColor(Color.parseColor("#171717"))
                cornerRadii = floatArrayOf(radius, radius, radius, radius, 0f, 0f, 0f, 0f)
            }
            // Keep the dialog window just tall enough for the sheet so the
            // map and carousel above it remain touchable.
            dialog.window?.setLayout(
                ViewGroup.LayoutParams.MATCH_PARENT,
                dp(230f),
            )
            dialog.window?.setGravity(Gravity.BOTTOM)
        }
        dialog.show()
        dialog.behavior.state = BottomSheetBehavior.STATE_COLLAPSED
        sheetDialog = dialog
    }

    private fun remove(id: Int) {
        val view = views.remove(id) ?: return
        val tag = tags.remove(id)
        if (tag?.isSheet == true) {
            sheetDialog?.dismiss()
            sheetDialog = null
        }
        (view.parent as? ViewGroup)?.removeView(view)
        if (view is NativeMapHost) {
            view.destroy()
            mapHosts.remove(view)
        }
        if (view is NativeModuleHostView) {
            view.destroy()
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
        "image" -> "image"
        "input" -> "input"
        "native" -> "native"
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

            "image" -> {
                (view as ImageView).setImageDrawable(null)
                view.background = null
                view.scaleType = ImageView.ScaleType.CENTER_CROP
            }

            "input" -> {
                val field = view as EditText
                field.setText("")
                field.hint = null
                field.background = null
            }

            "native" -> (view as NativeModuleHostView).resetForReuse()

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
        RustBridge.nativeEventCode(handle, id, eventCode(kind), payload)
    }

    private fun eventCode(kind: String): Int = when (kind) {
        "click" -> 1
        "change" -> 2
        "submit" -> 3
        "scroll" -> 4
        "page" -> 5
        "focus" -> 6
        "blur" -> 7
        "marker" -> 8
        "host" -> 9
        "incoming" -> 10
        "ack" -> 11
        "fail" -> 12
        "flush" -> 13
        "connection" -> 14
        "select" -> 15
        else -> 0
    }

    private fun applyProps(view: View, props: JSONObject) {
        val tag = view.tag as? NodeTag ?: return
        tag.animate = props.optBoolean("animate", false)

        if (view is NativeMapHost) {
            view.apply(props)
        }

        if (view is NativeModuleHostView) {
            tag.native = props.optString("native").takeIf { it.isNotEmpty() }
            view.onEvent = { kind, payload ->
                event(tag.id, kind, payload)
            }
            view.configure(props.optString("native"), props)
            view.setActive(isActive)
        }

        if (props.optBoolean("pressable", false)) {
            view.isClickable = true
            view.setOnClickListener {
                val input = context.getSystemService(Context.INPUT_METHOD_SERVICE)
                    as? android.view.inputmethod.InputMethodManager
                input?.hideSoftInputFromWindow(view.windowToken, 0)
                event(tag.id, "click", "")
            }
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

        if (view is TextView && (props.has("font") || props.has("bold"))) {
            val bold = props.optBoolean("bold", false)
            val font = props.optString("font", "")
            val base = if (font.isNotEmpty()) {
                typeface(font)
            } else {
                Typeface.DEFAULT
            }
            view.typeface = if (bold) {
                Typeface.create(base, Typeface.BOLD)
            } else {
                base
            }
        }

        if (props.has("lines") && view is TextView) {
            val lines = props.getInt("lines")
            view.maxLines = lines
            view.ellipsize = if (lines == 1) {
                android.text.TextUtils.TruncateAt.END
            } else {
                null
            }
        }

        if (props.has("lineHeight") && view is TextView) {
            val lineHeight = dp(props.getDouble("lineHeight").toFloat()).toFloat()
            val fontSize = view.textSize
            view.setLineSpacing((lineHeight - fontSize).coerceAtLeast(0f), 1f)
        }

        if (props.has("align") && view is TextView) {
            view.gravity = when (props.getString("align")) {
                "center" -> Gravity.CENTER
                "end" -> Gravity.END or Gravity.CENTER_VERTICAL
                else -> Gravity.START or Gravity.CENTER_VERTICAL
            }
        }

        if (
            props.has("bg") ||
            props.has("radius") ||
            props.has("borderWidth") ||
            (tag.kind == "glass" && props.has("glassTint"))
        ) {
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
            val borderWidth = dp(props.optDouble("borderWidth", 0.0).toFloat())
            val borderColor = parseColor(props.optString("borderColor", "#ffffff"))
            view.background = GradientDrawable().apply {
                shape = GradientDrawable.RECTANGLE
                setColor(color)
                cornerRadius = radius
                if (borderWidth > 0) {
                    setStroke(borderWidth, borderColor)
                }
            }
        }

        if (props.has("opacity")) {
            view.alpha = props.getDouble("opacity").toFloat()
        }

        if (props.has("clip") && props.getBoolean("clip")) {
            view.clipToOutline = true
        }

        if (props.has("shadow")) {
            if (props.getBoolean("shadow")) {
                view.elevation = dp(8f).toFloat()
            } else {
                view.elevation = 0f
            }
        }

        if (view is TextView && (props.has("pad") || props.has("padH") || props.has("padV"))) {
            val base = props.optDouble("pad", 0.0).toFloat()
            val horizontal = dp(props.optDouble("padH", base.toDouble()).toFloat())
            val vertical = dp(props.optDouble("padV", base.toDouble()).toFloat())
            view.setPadding(horizontal, vertical, horizontal, vertical)
        }

        if (props.has("testId")) {
            view.contentDescription = props.getString("testId")
        } else if (props.has("label")) {
            view.contentDescription = props.getString("label")
        }

        if (props.has("placeholder") && view is EditText) {
            view.hint = props.getString("placeholder")
        }

        if (props.has("placeholderColor") && view is EditText) {
            view.setHintTextColor(parseColor(props.getString("placeholderColor")))
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

        if (view is CheetahHScroll) {
            if (props.has("paged")) {
                view.paged = props.getBoolean("paged")
            }
            if (props.has("scrollX")) {
                view.scrollTo(dp(props.getDouble("scrollX").toFloat()), 0)
            }
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

    private fun typeface(font: String): Typeface {
        typefaceCache[font]?.let { return it }
        val asset = when (font) {
            "satoshi" -> "fonts/Satoshi-Regular.otf"
            "satoshi-medium" -> "fonts/Satoshi-Medium.otf"
            "satoshi-bold" -> "fonts/Satoshi-Bold.otf"
            "brice", "brice-semibold" -> "fonts/Brice-Semi-Bold.otf"
            "brice-bold" -> "fonts/Brice-Bold.otf"
            "general" -> "fonts/GeneralSans-Regular.otf"
            "general-medium" -> "fonts/GeneralSans-Medium.otf"
            "general-semibold" -> "fonts/GeneralSans-Semibold.otf"
            "general-bold" -> "fonts/GeneralSans-Bold.otf"
            else -> return Typeface.DEFAULT
        }
        val loaded = try {
            Typeface.createFromAsset(context.assets, asset)
        } catch (_: Exception) {
            Typeface.DEFAULT
        }
        typefaceCache[font] = loaded
        return loaded
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

private class WireReader(bytes: ByteArray) {
    private val buffer = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)

    fun magic(): Boolean {
        if (buffer.remaining() < 4) return false
        val bytes = ByteArray(4)
        buffer.get(bytes)
        return bytes.contentEquals(byteArrayOf('C'.code.toByte(), 'H'.code.toByte(), 'T'.code.toByte(), '1'.code.toByte()))
    }

    fun u8(): Int? = if (buffer.remaining() >= 1) buffer.get().toInt() and 0xff else null

    fun u16(): Int? = if (buffer.remaining() >= 2) buffer.short.toInt() and 0xffff else null

    fun u32(): Long? = if (buffer.remaining() >= 4) buffer.int.toLong() and 0xffffffffL else null

    fun f32(): Float? = if (buffer.remaining() >= 4) buffer.float else null

    fun string(): String? {
        val length = u32()?.toInt() ?: return null
        if (length < 0 || buffer.remaining() < length) return null
        val bytes = ByteArray(length)
        buffer.get(bytes)
        return String(bytes, Charsets.UTF_8)
    }

    private val propNames = arrayOf("text", "size", "color", "bg", "pad", "padH", "padV", "padBottom", "radius", "weight", "width", "height", "bold", "font", "lineHeight", "lines", "borderWidth", "borderColor", "opacity", "shadow", "clip", "animate", "wrap", "placeholderColor", "align", "placeholder", "value", "label", "testId", "gap", "alignItems", "justify", "absolute", "top", "left", "contentHeight", "scrollTo", "glassStyle", "glassTint", "glassInteractive", "contentInsetTop", "contentInsetBottom", "bottom", "mapLatitude", "mapLongitude", "mapLatitudeDelta", "mapLongitudeDelta", "mapMarkers", "mapDark", "mapShowsUser", "paged", "scrollX", "src", "fit", "contentWidth", "pressable", "native", "nativeProps")

    fun props(): JSONObject? {
        val count = u32()?.toInt() ?: return null
        val props = JSONObject()
        for (index in 0 until count) {
            val keyId = u16() ?: return null
            if (keyId !in propNames.indices) return null
            val key = propNames[keyId]
            when (val type = u8() ?: return null) {
                1 -> props.put(key, string() ?: return null)
                2 -> props.put(key, f32() ?: return null)
                3 -> props.put(key, (u8() ?: return null) != 0)
                4 -> props.put(key, u32() ?: return null)
                else -> return null
            }
        }
        return props
    }
}
