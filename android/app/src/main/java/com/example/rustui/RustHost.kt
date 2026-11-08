package com.example.rustui

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.Editable
import android.text.InputType
import android.text.TextWatcher
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.inputmethod.EditorInfo
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ScrollView
import android.widget.Space
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import kotlin.math.abs
import kotlin.math.roundToInt

class RustHost(
    private val context: Context,
    private val root: FrameLayout,
) {
    private var handle: Long = RustBridge.nativeNew()
    private val views = HashMap<Int, View>()
    private val tags = HashMap<Int, NodeTag>()
    private val density = context.resources.displayMetrics.density
    private var lastWidth = 0f
    private var lastHeight = 0f

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
        resizeIfNeeded()
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
        applyOps(RustBridge.nativeRender(handle, width, height))
    }

    fun dispose() {
        if (handle != 0L) {
            RustBridge.nativeFree(handle)
            handle = 0L
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
        val view = createView(kind)
        val tag = NodeTag(id, view)

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
        }

        tag.root = op.optBoolean("root", false)
        view.tag = tag
        views[id] = view
        tags[id] = tag

        if (tag.root) {
            root.addView(view, FrameLayout.LayoutParams(0, 0))
        }

        applyProps(view, op.getJSONObject("props"))
    }

    private fun createView(kind: String): View = when (kind) {
        "column", "row" -> FrameLayout(context)

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
            if (index >= parent.childCount || parent.getChildAt(index) !== child) {
                (child.parent as? ViewGroup)?.removeView(child)
                parent.addView(child, index.coerceAtMost(parent.childCount))
            }
        }
    }

    private fun layout(op: JSONObject) {
        val id = op.getInt("id")
        val view = views[id] ?: return
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

    private fun remove(id: Int) {
        val view = views.remove(id) ?: return
        (view.parent as? ViewGroup)?.removeView(view)
        tags.remove(id)
    }

    private fun event(id: Int, kind: String, payload: String) {
        if (handle == 0L) return
        applyOps(RustBridge.nativeEvent(handle, id, kind, payload))
    }

    private fun applyProps(view: View, props: JSONObject) {
        val tag = view.tag as? NodeTag ?: return

        if (props.has("text")) {
            val text = props.getString("text")
            if (view is TextView && view.text.toString() != text) {
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

        if (props.has("bg") || props.has("radius")) {
            val color = if (props.has("bg")) {
                Color.parseColor(props.getString("bg"))
            } else {
                Color.TRANSPARENT
            }
            val radius = dp(props.optDouble("radius", 0.0).toFloat()).toFloat()
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

    companion object {
        private val MATCH_PARENT = ViewGroup.LayoutParams.MATCH_PARENT
        private val WRAP_CONTENT = ViewGroup.LayoutParams.WRAP_CONTENT
    }
}
