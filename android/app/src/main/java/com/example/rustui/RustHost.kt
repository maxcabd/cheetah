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
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.Space
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject

class RustHost(
    private val context: Context,
    private val root: FrameLayout,
) {
    private var handle: Long = RustBridge.nativeNew()
    private val views = HashMap<Int, View>()
    private val tags = HashMap<Int, NodeTag>()

    fun start() {
        if (handle == 0L) return
        applyOps(RustBridge.nativeRender(handle))
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
            val inner = view.getChildAt(0) as ViewGroup
            tag.container = inner
            inner.tag = tag
        }

        tag.root = op.optBoolean("root", false)
        view.tag = tag
        views[id] = view
        tags[id] = tag

        if (tag.root) {
            root.addView(view, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
        }

        applyProps(view, op.getJSONObject("props"))
    }

    private fun createView(kind: String): View = when (kind) {
        "column" -> LinearLayout(context).apply {
            orientation = LinearLayout.VERTICAL
        }

        "row" -> LinearLayout(context).apply {
            orientation = LinearLayout.HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
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

        "scroll" -> ScrollView(context).apply {
            isFillViewport = true
            overScrollMode = View.OVER_SCROLL_NEVER
            clipToPadding = false
            addView(
                LinearLayout(context).apply {
                    orientation = LinearLayout.VERTICAL
                    gravity = Gravity.BOTTOM
                },
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

        // Drop views that are no longer part of this parent.
        for (index in parent.childCount - 1 downTo 0) {
            val childId = (parent.getChildAt(index).tag as? NodeTag)?.id
            if (childId == null || childId !in wanted) {
                parent.removeViewAt(index)
            }
        }

        // Put the remaining views into the exact order Rust asked for.
        for ((index, childId) in wanted.withIndex()) {
            val child = views[childId] ?: continue
            if (index >= parent.childCount || parent.getChildAt(index) !== child) {
                (child.parent as? ViewGroup)?.removeView(child)
                parent.addView(child, index.coerceAtMost(parent.childCount))
            }
            applyLayoutParams(parent, child)
        }

        scrollToBottom(parent)
    }

    private fun scrollToBottom(start: View) {
        var current: View? = start
        while (current != null && current !is ScrollView) {
            current = current.parent as? View
        }
        val scroll = current as? ScrollView ?: return
        scroll.post { scroll.fullScroll(View.FOCUS_DOWN) }
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
        val tag = view.tag as NodeTag

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

        if (props.has("align")) {
            val align = props.getString("align")
            if (view is LinearLayout) {
                view.gravity = when (align) {
                    "center" -> Gravity.CENTER_VERTICAL
                    "end" -> Gravity.END or Gravity.CENTER_VERTICAL
                    else -> Gravity.START or Gravity.CENTER_VERTICAL
                }
            } else if (view is TextView) {
                view.gravity = when (align) {
                    "center" -> Gravity.CENTER
                    "end" -> Gravity.END or Gravity.CENTER_VERTICAL
                    else -> Gravity.START or Gravity.CENTER_VERTICAL
                }
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

        if (props.has("pad") || props.has("padH") || props.has("padV")) {
            val base = props.optDouble("pad", 0.0).toFloat()
            val horizontal = dp(props.optDouble("padH", base.toDouble()).toFloat())
            val vertical = dp(props.optDouble("padV", base.toDouble()).toFloat())
            view.setPadding(horizontal, vertical, horizontal, vertical)
        }

        if (props.has("label")) {
            view.contentDescription = props.getString("label")
        }

        if (props.has("width")) {
            tag.widthDp = props.getDouble("width").toFloat()
        }

        if (props.has("height")) {
            tag.heightDp = props.getDouble("height").toFloat()
        }

        if (props.has("weight")) {
            tag.weight = props.getDouble("weight").toFloat()
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

        if (!tag.root) {
            (view.parent as? ViewGroup)?.let { parent -> applyLayoutParams(parent, view) }
        }
    }

    private fun applyLayoutParams(parent: ViewGroup, child: View) {
        val tag = child.tag as? NodeTag ?: return

        if (tag.root) {
            child.layoutParams = FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT)
            return
        }

        val layoutParams: ViewGroup.LayoutParams = when (parent) {
            is LinearLayout -> child.layoutParams as? LinearLayout.LayoutParams
                ?: LinearLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT)

            else -> child.layoutParams as? ViewGroup.LayoutParams
                ?: ViewGroup.LayoutParams(WRAP_CONTENT, WRAP_CONTENT)
        }

        if (parent is LinearLayout) {
            val linear = layoutParams as LinearLayout.LayoutParams
            val vertical = parent.orientation == LinearLayout.VERTICAL

            if (tag.weight > 0f) {
                if (vertical) {
                    linear.height = 0
                    linear.width = tag.widthDp?.let { dimension(it) } ?: MATCH_PARENT
                } else {
                    linear.width = 0
                    linear.height = tag.heightDp?.let { dimension(it) } ?: WRAP_CONTENT
                }
                linear.weight = tag.weight
            } else {
                linear.width = tag.widthDp?.let { dimension(it) }
                    ?: if (vertical) MATCH_PARENT else WRAP_CONTENT
                linear.height = tag.heightDp?.let { dimension(it) } ?: WRAP_CONTENT
                linear.weight = 0f
            }
        } else {
            layoutParams.width = tag.widthDp?.let { dimension(it) } ?: WRAP_CONTENT
            layoutParams.height = tag.heightDp?.let { dimension(it) } ?: WRAP_CONTENT
        }

        child.layoutParams = layoutParams
    }

    private fun dimension(value: Float): Int = when (value) {
        -1f -> MATCH_PARENT
        -2f -> WRAP_CONTENT
        else -> dp(value)
    }

    private fun dp(value: Float): Int = TypedValue.applyDimension(
        TypedValue.COMPLEX_UNIT_DIP,
        value,
        context.resources.displayMetrics,
    ).toInt()

    companion object {
        private val MATCH_PARENT = ViewGroup.LayoutParams.MATCH_PARENT
        private val WRAP_CONTENT = ViewGroup.LayoutParams.WRAP_CONTENT
    }
}
