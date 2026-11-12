package com.example.rustui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.view.Gravity
import android.view.MotionEvent
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.LinearLayout
import kotlin.math.abs
import kotlin.math.roundToInt

/** Native photo pager and progress dots for the Nightshade place card. */
class PhotoPagerView(context: Context) : FrameLayout(context) {
    private val strip = PhotoStrip(context)
    private val images = LinearLayout(context).apply {
        orientation = LinearLayout.HORIZONTAL
    }
    private val gradientPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val dotPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = Color.WHITE }
    private var bitmap: Bitmap? = null
    private var pages = 5
    private var source = ""

    init {
        strip.isHorizontalScrollBarEnabled = false
        strip.isFillViewport = true
        strip.addView(
            images,
            FrameLayout.LayoutParams(
                FrameLayout.LayoutParams.WRAP_CONTENT,
                FrameLayout.LayoutParams.MATCH_PARENT,
            ),
        )
        strip.onProgress = { invalidate() }
        addView(strip, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
    }

    fun configure(src: String, count: Int) {
        if (src == source && count == pages && bitmap != null) return
        val sourceChanged = src != source
        source = src
        pages = count.coerceAtLeast(1)
        if (bitmap == null || sourceChanged) {
            bitmap = if (cachedSource == src) {
                cachedBitmap
            } else {
                try {
                    val path = src.removePrefix("asset://")
                    val options = BitmapFactory.Options().apply {
                        inSampleSize = 2
                    }
                    context.assets.open(path).use {
                        BitmapFactory.decodeStream(it, null, options)
                    }
                } catch (_: Exception) {
                    null
                }
            }
            if (bitmap != null) {
                cachedBitmap = bitmap
                cachedSource = src
            }
        }
        rebuild()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        layoutPages(w, h)
    }


    private fun rebuild() {
        images.removeAllViews()
        for (index in 0 until pages) {
            val imageView = ImageView(context).apply {
                scaleType = ImageView.ScaleType.CENTER_CROP
                setImageBitmap(bitmap)
                contentDescription = null
            }
            images.addView(
                imageView,
                LinearLayout.LayoutParams(
                    width.coerceAtLeast(1),
                    height.coerceAtLeast(1),
                ),
            )
        }
        layoutPages(width, height)
    }

    private fun layoutPages(w: Int, h: Int) {
        if (w <= 0 || h <= 0) return
        images.layoutParams = FrameLayout.LayoutParams(w * pages, h)
        for (index in 0 until images.childCount) {
            val child = images.getChildAt(index)
            child.layoutParams = LinearLayout.LayoutParams(w, h)
        }
        images.measure(
            MeasureSpec.makeMeasureSpec(w * pages, MeasureSpec.EXACTLY),
            MeasureSpec.makeMeasureSpec(h, MeasureSpec.EXACTLY),
        )
        images.layout(0, 0, w * pages, h)
        images.requestLayout()
        strip.requestLayout()
    }

    override fun dispatchDraw(canvas: Canvas) {
        super.dispatchDraw(canvas)
        drawGradient(canvas)
        drawDots(canvas)
    }

    private fun drawGradient(canvas: Canvas) {
        val heightPx = (45f * resources.displayMetrics.density).roundToInt()
        if (height <= 0) return
        gradientPaint.shader = LinearGradient(
            0f,
            (height - heightPx).toFloat(),
            0f,
            height.toFloat(),
            intArrayOf(
                Color.parseColor("#0b0b0f00"),
                Color.parseColor("#0b0b0f59"),
                Color.parseColor("#0b0b0fb3"),
                Color.parseColor("#1f1f1f"),
            ),
            floatArrayOf(0f, 0.5f, 0.8f, 1f),
            Shader.TileMode.CLAMP,
        )
        canvas.drawRect(
            0f,
            (height - heightPx).toFloat(),
            width.toFloat(),
            height.toFloat(),
            gradientPaint,
        )
    }

    private fun drawDots(canvas: Canvas) {
        if (pages <= 1) return
        val density = resources.displayMetrics.density
        val baseRadius = 2.5f * density
        val spacing = 11f * density
        val total = spacing * (pages - 1)
        val start = width / 2f - total / 2f
        val y = height - 12f * density
        val progress = if (width > 0) strip.scrollX.toFloat() / width else 0f
        for (index in 0 until pages) {
            val distance = abs(progress - index)
            val influence = (1f - distance / 0.75f).coerceIn(0f, 1f)
            val scale = 1f + 0.55f * influence
            dotPaint.alpha = (56 + 199 * influence).roundToInt().coerceIn(0, 255)
            canvas.drawCircle(
                start + index * spacing,
                y,
                baseRadius * scale,
                dotPaint,
            )
        }
    }

    private class PhotoStrip(context: Context) : HorizontalScrollView(context) {
        var onProgress: (() -> Unit)? = null
        private var downX = 0f

        override fun onTouchEvent(ev: MotionEvent): Boolean {
            when (ev.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    downX = ev.x
                    parent?.requestDisallowInterceptTouchEvent(true)
                }
                MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                    parent?.requestDisallowInterceptTouchEvent(false)
                    if (abs(ev.x - downX) > 4f) {
                        post { snapToPage() }
                    }
                }
            }
            return super.onTouchEvent(ev)
        }

        override fun onScrollChanged(l: Int, t: Int, oldl: Int, oldt: Int) {
            super.onScrollChanged(l, t, oldl, oldt)
            if (width > 0) {
                onProgress?.invoke()
            }
        }

        private fun snapToPage() {
            if (width <= 0) return
            val page = ((scrollX + width / 2) / width)
            smoothScrollTo(page * width, 0)
        }
    }

    private companion object {
        var cachedBitmap: Bitmap? = null
        var cachedSource: String? = null
    }
}
