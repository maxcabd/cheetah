package com.example.rustui

import android.view.GestureDetector
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.VelocityTracker
import android.view.View
import android.view.ViewConfiguration
import org.json.JSONObject
import kotlin.math.abs

/**
 * Reports `.gestures("pan,swipe,long_press,pinch,double_tap")` as `gesture`
 * events (payload: see runtime::gesture). Distances are points from where
 * the gesture began; velocities are points per second.
 *
 * Also does the press feedback and click for pressable views, since one
 * touch listener has to see every touch.
 */
class GestureHandler(
    private val view: View,
    private val density: Float,
    private val emit: (String) -> Unit,
) {
    var kinds: Set<String> = emptySet()
    var pressable = false

    /** Resting scale and alpha, restored after press feedback. */
    var restScale = 1f
    var restAlpha = 1f

    private val slop = ViewConfiguration.get(view.context).scaledTouchSlop
    private var downX = 0f
    private var downY = 0f
    private var panning = false
    private var velocity: VelocityTracker? = null

    private val detector = GestureDetector(
        view.context,
        object : GestureDetector.SimpleOnGestureListener() {
            override fun onDown(e: MotionEvent): Boolean = true

            override fun onSingleTapConfirmed(e: MotionEvent): Boolean {
                if ("double_tap" in kinds && pressable) view.performClick()
                return true
            }

            override fun onSingleTapUp(e: MotionEvent): Boolean {
                if ("double_tap" !in kinds && pressable) view.performClick()
                return true
            }

            override fun onDoubleTap(e: MotionEvent): Boolean {
                if ("double_tap" in kinds) send("double_tap", "ended")
                return true
            }

            override fun onLongPress(e: MotionEvent) {
                if ("long_press" in kinds && !panning) {
                    view.performHapticFeedback(android.view.HapticFeedbackConstants.LONG_PRESS)
                    send("long_press", "ended")
                }
            }

            override fun onFling(
                e1: MotionEvent?,
                e2: MotionEvent,
                velocityX: Float,
                velocityY: Float,
            ): Boolean {
                if ("swipe" !in kinds || e1 == null) return false
                val dx = (e2.rawX - e1.rawX) / density
                val dy = (e2.rawY - e1.rawY) / density
                val vx = velocityX / density
                val vy = velocityY / density
                val horizontal = abs(dx) > abs(dy)
                val distance = if (horizontal) abs(dx) else abs(dy)
                val speed = if (horizontal) abs(vx) else abs(vy)
                if (distance < 40f || speed < 300f) return false
                val direction = when {
                    horizontal && dx > 0 -> "right"
                    horizontal -> "left"
                    dy > 0 -> "down"
                    else -> "up"
                }
                send("swipe", "ended", dx, dy, vx, vy, direction = direction)
                return true
            }
        },
    )

    private val scaler = ScaleGestureDetector(
        view.context,
        object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
            private var total = 1f

            override fun onScaleBegin(detector: ScaleGestureDetector): Boolean {
                if ("pinch" !in kinds) return false
                total = 1f
                view.parent?.requestDisallowInterceptTouchEvent(true)
                send("pinch", "began", scale = total)
                return true
            }

            override fun onScale(detector: ScaleGestureDetector): Boolean {
                total *= detector.scaleFactor
                send("pinch", "changed", scale = total)
                return true
            }

            override fun onScaleEnd(detector: ScaleGestureDetector) {
                send("pinch", "ended", scale = total)
            }
        },
    )

    fun onTouch(event: MotionEvent): Boolean {
        pressFeedback(event)
        if ("pinch" in kinds) scaler.onTouchEvent(event)
        if ("pan" in kinds) pan(event)
        detector.onTouchEvent(event)
        // Consume when this view handles gestures; plain pressable views
        // keep the platform click path.
        return kinds.isNotEmpty()
    }

    private fun pan(event: MotionEvent) {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                downX = event.rawX
                downY = event.rawY
                panning = false
                velocity?.recycle()
                velocity = VelocityTracker.obtain().also { it.addMovement(event) }
            }
            MotionEvent.ACTION_MOVE -> {
                velocity?.addMovement(event)
                val dx = event.rawX - downX
                val dy = event.rawY - downY
                if (!panning && (abs(dx) > slop || abs(dy) > slop) && event.pointerCount == 1) {
                    panning = true
                    view.parent?.requestDisallowInterceptTouchEvent(true)
                    send("pan", "began", dx / density, dy / density)
                } else if (panning) {
                    velocity?.computeCurrentVelocity(1000)
                    send(
                        "pan",
                        "changed",
                        dx / density,
                        dy / density,
                        (velocity?.xVelocity ?: 0f) / density,
                        (velocity?.yVelocity ?: 0f) / density,
                    )
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (panning) {
                    velocity?.addMovement(event)
                    velocity?.computeCurrentVelocity(1000)
                    send(
                        "pan",
                        if (event.actionMasked == MotionEvent.ACTION_UP) "ended" else "cancelled",
                        (event.rawX - downX) / density,
                        (event.rawY - downY) / density,
                        (velocity?.xVelocity ?: 0f) / density,
                        (velocity?.yVelocity ?: 0f) / density,
                    )
                }
                panning = false
                velocity?.recycle()
                velocity = null
            }
        }
    }

    private fun pressFeedback(event: MotionEvent) {
        if (!pressable) return
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> view.animate()
                .scaleX(restScale * 0.97f)
                .scaleY(restScale * 0.97f)
                .alpha(restAlpha * 0.9f)
                .setDuration(90L)
                .start()
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> view.animate()
                .scaleX(restScale)
                .scaleY(restScale)
                .alpha(restAlpha)
                .setDuration(140L)
                .start()
        }
    }

    private fun send(
        type: String,
        state: String,
        dx: Float = 0f,
        dy: Float = 0f,
        vx: Float = 0f,
        vy: Float = 0f,
        scale: Float = 1f,
        direction: String = "",
    ) {
        emit(
            JSONObject()
                .put("type", type)
                .put("state", state)
                .put("dx", dx.toDouble())
                .put("dy", dy.toDouble())
                .put("vx", vx.toDouble())
                .put("vy", vy.toDouble())
                .put("scale", scale.toDouble())
                .put("direction", direction)
                .toString(),
        )
    }
}
