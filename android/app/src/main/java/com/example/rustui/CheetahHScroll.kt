package com.example.rustui

import android.content.Context
import android.util.AttributeSet
import android.view.MotionEvent
import android.widget.HorizontalScrollView
import kotlin.math.roundToInt

/** Horizontal scroll view with optional page snapping for native carousels. */
class CheetahHScroll @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
) : HorizontalScrollView(context, attrs) {
    var paged: Boolean = false
    var onPageChanged: ((Int) -> Unit)? = null

    override fun onTouchEvent(ev: MotionEvent): Boolean {
        val handled = super.onTouchEvent(ev)
        if (paged && ev.actionMasked == MotionEvent.ACTION_UP) {
            post { snapToPage() }
        }
        return handled
    }

    private fun snapToPage() {
        val pageWidth = width.coerceAtLeast(1)
        val page = ((scrollX + pageWidth / 2) / pageWidth)
        val target = page * pageWidth
        smoothScrollTo(target, 0)
        postDelayed({ onPageChanged?.invoke(page) }, 280)
    }
}
