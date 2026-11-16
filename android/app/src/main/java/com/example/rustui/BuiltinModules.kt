package com.example.rustui

/** Modules that ship with Cheetah. App modules come from GeneratedModules. */
object BuiltinModules {
    val apis: List<ApiModuleSpec> = listOf(
        ApiModuleSpec("notifications") { NotificationsModule(it) },
        ApiModuleSpec("linking") { LinkingModule(it) },
        ApiModuleSpec("auth") { AuthModule(it) },
        ApiModuleSpec("dialog") { DialogModule(it) },
        ApiModuleSpec("picker") { PickerModule(it) },
    )
    val views: List<ViewModuleSpec> = listOf(
        ViewModuleSpec("switch") { context, host -> SwitchViewModule(context, host) },
        ViewModuleSpec("slider") { context, host -> SliderViewModule(context, host) },
        ViewModuleSpec("spinner") { context, host -> SpinnerViewModule(context, host) },
        ViewModuleSpec("webview") { context, host -> WebViewModule(context, host) },
    )

    /** Modules created up front so they see every incoming link. */
    val linkHandlers: List<String> = listOf("linking")
}
