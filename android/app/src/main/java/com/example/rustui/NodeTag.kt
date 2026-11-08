package com.example.rustui

import android.view.View
import android.view.ViewGroup

class NodeTag(val id: Int, val view: View) {
    var container: ViewGroup? = view as? ViewGroup
    var root: Boolean = false
    var weight: Float = 0f
    var widthDp: Float? = null
    var heightDp: Float? = null
    var updating: Boolean = false
}
