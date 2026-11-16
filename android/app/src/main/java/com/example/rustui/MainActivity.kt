package com.example.rustui

import android.app.Activity
import android.graphics.Color
import android.os.Build
import android.os.Bundle
import android.view.WindowManager
import android.widget.FrameLayout

class MainActivity : Activity() {
    private lateinit var host: RustHost

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val edgeToEdge = Build.VERSION.SDK_INT >= Build.VERSION_CODES.R
        if (edgeToEdge) {
            window.setDecorFitsSystemWindows(false)
            window.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_ADJUST_NOTHING)
            window.statusBarColor = Color.TRANSPARENT
            window.navigationBarColor = Color.TRANSPARENT
        }

        val root = FrameLayout(this).apply {
            setBackgroundColor(Color.parseColor("#313338"))
            fitsSystemWindows = !edgeToEdge
        }
        setContentView(root)

        host = RustHost(
            this,
            root,
            edgeToEdge,
            intent.getStringExtra("app") ?: "chat",
            intent.getStringExtra("devServer"),
        )
        root.setOnApplyWindowInsetsListener { _, insets ->
            if (edgeToEdge) {
                host.onWindowInsets(insets)
            }
            insets
        }
        root.requestApplyInsets()

        root.post { host.start() }
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (::host.isInitialized) {
            host.onRequestPermissionsResult(requestCode, grantResults)
        }
    }

    // Without android:enableOnBackInvokedCallback this is still delivered on
    // API 33+, which keeps one code path down to minSdk 24.
    @Deprecated("Deprecated in Android")
    override fun onBackPressed() {
        if (::host.isInitialized && host.onBackPressed()) return
        @Suppress("DEPRECATION")
        super.onBackPressed()
    }

    override fun onStart() {
        super.onStart()
        if (::host.isInitialized) {
            host.setActive(true)
        }
    }

    override fun onStop() {
        if (::host.isInitialized) {
            host.setActive(false)
        }
        super.onStop()
    }

    override fun onDestroy() {
        host.dispose()
        super.onDestroy()
    }
}
