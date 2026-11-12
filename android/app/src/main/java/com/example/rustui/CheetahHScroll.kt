package com.example.rustui

import android.animation.ValueAnimator
import android.content.Context
import android.util.AttributeSet
import android.view.MotionEvent
import android.view.ViewGroup
import android.view.animation.DecelerateInterpolator
import android.widget.HorizontalScrollView
import kotlin.math.abs
import kotlin.math.min

/** Horizontal scroll view with native page snapping and carousel scaling. */
class CheetahHScroll @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : HorizontalScrollView(context, attrs) {
    var paged: Boolean = false
    var onPageChanged: ((Int) -> Unit)? = null

    private var pageAnimator: ValueAnimator? = null
    private var lastTargetX = -1

    override fun onTouchEvent(ev: MotionEvent): Boolean {
        if (ev.actionMasked == MotionEvent.ACTION_DOWN) {
            pageAnimator?.cancel()
            pageAnimator = null
        }
        val handled = super.onTouchEvent(ev)
        if (paged && ev.actionMasked == MotionEvent.ACTION_UP) {
            post { snapToPage() }
        }
        return handled
    }

    override fun onScrollChanged(l: Int, t: Int, oldl: Int, oldt: Int) {
        super.onScrollChanged(l, t, oldl, oldt)
        updateCarouselScale()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        updateCarouselScale()
    }

    fun animateToX(target: Int, duration: Long = 420L) {
        if (!paged) {
            scrollTo(target, 0)
            return
        }
        if (target == lastTargetX) return
        lastTargetX = target
        val start = scrollX
        if (start == target) return
        pageAnimator?.cancel()
        pageAnimator = ValueAnimator.ofFloat(0f, 1f).apply {
            this.duration = duration
            interpolator = DecelerateInterpolator(1.6f)
            addUpdateListener { animator ->
                val fraction = animator.animatedValue as Float
                scrollTo((start + (target - start) * fraction).toInt(), 0)
            }
            start()
        }
    }

    private fun snapToPage() {
        val pageWidth = width.coerceAtLeast(1)
        val page = ((scrollX + pageWidth / 2) / pageWidth)
        val target = page * pageWidth
        lastTargetX = target
        smoothScrollTo(target, 0)
        postDelayed({ onPageChanged?.invoke(page) }, 280)
    }

    private fun updateCarouselScale() {
        if (!paged) return
        val pageWidth = width.coerceAtLeast(1)
        val content = getChildAt(0) as? ViewGroup ?: return
        val viewportCenter = scrollX + pageWidth / 2f
        for (index in 0 until content.childCount) {
            val page = content.getChildAt(index) as? ViewGroup ?: continue
            val card = page.getChildAt(0) ?: continue
            val pageCenter = index * pageWidth + pageWidth / 2f
            val distance = min(abs(viewportCenter - pageCenter) / pageWidth, 1f)
            val scale = 1f - 0.15f * distance
            card.scaleX = scale
            card.scaleY = scale
        }
    }
}
