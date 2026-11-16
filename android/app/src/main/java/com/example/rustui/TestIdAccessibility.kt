package com.example.rustui

import android.view.View
import android.view.accessibility.AccessibilityNodeInfo

/**
 * Publishes a node's Rust `testId` as its accessibility view-id so UI
 * automation (UIAutomator `resource-id`, used by cheetah-drive) can find it
 * without it replacing the spoken content description.
 */
object TestIdAccessibility : View.AccessibilityDelegate() {
    override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(host, info)
        (host.tag as? NodeTag)?.testId?.let { info.viewIdResourceName = it }
    }
}
