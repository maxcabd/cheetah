package com.example.rustui

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.Paint
import android.graphics.Typeface
import kotlin.math.roundToInt

/** Renders the same marker vocabulary on Google Maps and MapLibre. */
object MarkerBitmapFactory {
    private var typeface: Typeface? = null
    private val bitmaps = HashMap<String, Bitmap>()

    private fun fontAwesome(context: Context): Typeface {
        typeface?.let { return it }
        val loaded = try {
            Typeface.createFromAsset(context.assets, "fonts/FontAwesome6_Solid.ttf")
        } catch (_: Exception) {
            Typeface.DEFAULT
        }
        typeface = loaded
        return loaded
    }

    private val glyphs = mapOf(
        "coffee" to "\uF0F4",
        "hamburger" to "\uF805",
        "pizza-slice" to "\uF818",
        "fish" to "\uF578",
        "utensils" to "\uF2E7",
    )

    fun create(
        context: Context,
        kind: String,
        icon: String,
        iconColor: String,
        selected: Boolean,
    ): Bitmap {
        val density = context.resources.displayMetrics.density
        val cacheKey = "$kind|$icon|$iconColor|$selected|$density"
        bitmaps[cacheKey]?.let { return it }
        val result = kind == "result"
        val sizeDp = if (result) 40f else 56f
        val size = (sizeDp * density).roundToInt()
        val bitmap = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        val center = size / 2f
        val paint = Paint(Paint.ANTI_ALIAS_FLAG)

        paint.style = Paint.Style.FILL
        paint.color = if (result) {
            if (selected) Color.parseColor("#ef4444") else Color.parseColor("#404040")
        } else {
            Color.WHITE
        }
        canvas.drawCircle(center, center, center - 2f * density, paint)

        paint.style = Paint.Style.STROKE
        paint.strokeWidth = (if (result) 3f else 1f) * density
        paint.color = if (result) {
            if (selected) Color.WHITE else Color.parseColor("#262626")
        } else {
            Color.argb(128, 38, 38, 38)
        }
        canvas.drawCircle(center, center, center - 2f * density, paint)

        val glyphPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            typeface = fontAwesome(context)
            textSize = (if (result) 14f else 20f) * density
            color = if (result) Color.WHITE else parseColor(iconColor)
            textAlign = Paint.Align.CENTER
        }
        val glyph = glyphs[icon] ?: glyphs.getValue("utensils")
        val metrics = glyphPaint.fontMetrics
        val baseline = center - (metrics.ascent + metrics.descent) / 2f
        canvas.drawText(glyph, center, baseline, glyphPaint)
        bitmaps[cacheKey] = bitmap
        return bitmap
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
