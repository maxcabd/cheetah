package com.example.rustui

import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ScrollView

class NodeTag(val id: Int, val view: View, val kind: String) {
    var container: ViewGroup? = view as? ViewGroup
    var scroll: ScrollView? = null
    var content: FrameLayout? = null
    var root: Boolean = false
    var updating: Boolean = false
    var pendingScrollTo: Float? = null
    var lastScrollTo: Float? = null
    var scrollEventPending: Boolean = false
    var scrollGeneration: Int = 0
}
