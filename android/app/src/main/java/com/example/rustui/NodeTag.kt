package com.example.rustui

import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.ScrollView

class NodeTag(val id: Int, val view: View, val kind: String) {
    // Only column/row/glass/scroll nodes receive Rust children. Native leaf
    // view groups such as MapView own their internal children.
    var container: ViewGroup? = null
    var scroll: ScrollView? = null
    var content: FrameLayout? = null
    var root: Boolean = false
    var updating: Boolean = false
    var pendingScrollTo: Float? = null
    var lastScrollTo: Float? = null
    var scrollEventPending: Boolean = false
    var scrollGeneration: Int = 0
    var imageUrl: String? = null
    var source: String? = null
    var native: String? = null

    val isSheet: Boolean
        get() = kind == "sheet" || native == "sheet"
}
