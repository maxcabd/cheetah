package com.example.rustui

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.widget.FrameLayout

class MainActivity : Activity() {
    private lateinit var host: RustHost

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        val root = FrameLayout(this).apply {
            setBackgroundColor(Color.parseColor("#313338"))
            fitsSystemWindows = true
        }
        setContentView(root)

        host = RustHost(this, root)
        root.post { host.start() }
    }

    override fun onDestroy() {
        host.dispose()
        super.onDestroy()
    }
}
