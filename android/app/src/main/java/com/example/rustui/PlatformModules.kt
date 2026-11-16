package com.example.rustui

import android.Manifest
import android.app.AlarmManager
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.CancellationSignal
import org.json.JSONObject

/*
 * Built-in modules for notifications, links and sign-in. Contracts (methods,
 * events) are documented on the Rust side in runtime::platform.
 */

// ---------------------------------------------------------------------------
// notifications
// ---------------------------------------------------------------------------

/** Local notifications. Push on Android needs Firebase, which isn't bundled. */
class NotificationsModule(private val context: ModuleContext) : CheetahApiModule {
    private val app = context.context.applicationContext
    private val permissionRequest = RequestCodes.allocate()

    init {
        NotificationBus.listener = { method, values -> context.emit("notifications", method, values) }
        // Opened before Rust asked about notifications (cold start).
        NotificationBus.takePending()?.let { context.emit("notifications", "opened", it) }
    }

    override fun invoke(method: String, args: JSONObject): Boolean {
        when (method) {
            "requestPermission" -> requestPermission()
            "schedule" -> schedule(args)
            "cancel" -> cancel(args.optString("id"))
            "cancelAll" -> {
                manager().cancelAll()
            }
            "registerPush" -> context.emit(
                "notifications",
                "token",
                JSONObject().put("error", "unsupported: Android push needs Firebase Cloud Messaging"),
            )
            "setBadge" -> Unit
            else -> return false
        }
        return true
    }

    private fun manager() = app.getSystemService(NotificationManager::class.java)

    private fun requestPermission() {
        val activity = context.activity
        if (Build.VERSION.SDK_INT >= 33 && activity != null &&
            app.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            activity.requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), permissionRequest)
        } else {
            emitPermission(manager().areNotificationsEnabled())
        }
    }

    override fun onRequestPermissionsResult(requestCode: Int, grantResults: IntArray): Boolean {
        if (requestCode != permissionRequest) return false
        emitPermission(grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED)
        return true
    }

    private fun emitPermission(granted: Boolean) {
        context.emit("notifications", "permission", JSONObject().put("granted", granted))
    }

    private fun schedule(args: JSONObject) {
        val id = args.optString("id", System.currentTimeMillis().toString())
        val seconds = args.optDouble("seconds", 0.0)
        val intent = Intent(app, NotificationReceiver::class.java)
            .putExtra("id", id)
            .putExtra("title", args.optString("title"))
            .putExtra("body", args.optString("body"))
            .putExtra("data", args.optJSONObject("data")?.toString() ?: "{}")
        if (seconds <= 0.0) {
            NotificationReceiver.show(app, intent)
            return
        }
        val pending = PendingIntent.getBroadcast(
            app,
            id.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val alarms = app.getSystemService(AlarmManager::class.java)
        alarms.set(AlarmManager.RTC_WAKEUP, System.currentTimeMillis() + (seconds * 1000).toLong(), pending)
    }

    private fun cancel(id: String) {
        val intent = Intent(app, NotificationReceiver::class.java)
        val pending = PendingIntent.getBroadcast(
            app,
            id.hashCode(),
            intent,
            PendingIntent.FLAG_NO_CREATE or PendingIntent.FLAG_IMMUTABLE,
        )
        if (pending != null) {
            app.getSystemService(AlarmManager::class.java).cancel(pending)
        }
        manager().cancel(id.hashCode())
    }

    override fun dispose() {
        NotificationBus.listener = null
    }
}

/** Hands notification events to the live module, or holds a tap until one exists. */
object NotificationBus {
    @Volatile
    var listener: ((String, JSONObject) -> Unit)? = null
    private var pending: JSONObject? = null

    fun post(method: String, values: JSONObject) {
        val current = listener
        if (current != null) {
            current(method, values)
        } else if (method == "opened") {
            synchronized(this) { pending = values }
        }
    }

    @Synchronized
    fun takePending(): JSONObject? = pending.also { pending = null }

    /** MainActivity: was this intent a notification tap? */
    fun handleOpen(intent: Intent?): Boolean {
        val id = intent?.getStringExtra(NotificationReceiver.OPENED_ID) ?: return false
        val data = intent.getStringExtra(NotificationReceiver.OPENED_DATA) ?: "{}"
        intent.removeExtra(NotificationReceiver.OPENED_ID)
        post("opened", JSONObject().put("id", id).put("data", parse(data)))
        return true
    }

    fun parse(json: String): Any = try {
        JSONObject(json)
    } catch (_: Exception) {
        JSONObject()
    }
}

/** Shows scheduled notifications (AlarmManager fires this). */
class NotificationReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        show(context, intent)
    }

    companion object {
        const val CHANNEL = "cheetah-default"
        const val OPENED_ID = "cheetahNotificationId"
        const val OPENED_DATA = "cheetahNotificationData"

        fun show(context: Context, intent: Intent) {
            val manager = context.getSystemService(NotificationManager::class.java)
            if (manager.getNotificationChannel(CHANNEL) == null) {
                manager.createNotificationChannel(
                    NotificationChannel(CHANNEL, "Notifications", NotificationManager.IMPORTANCE_DEFAULT),
                )
            }
            val id = intent.getStringExtra("id") ?: return
            val title = intent.getStringExtra("title") ?: ""
            val body = intent.getStringExtra("body") ?: ""
            val data = intent.getStringExtra("data") ?: "{}"
            val open = Intent(context, MainActivity::class.java)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
                .putExtra(OPENED_ID, id)
                .putExtra(OPENED_DATA, data)
            val content = PendingIntent.getActivity(
                context,
                id.hashCode(),
                open,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
            val notification = Notification.Builder(context, CHANNEL)
                .setSmallIcon(context.applicationInfo.icon)
                .setContentTitle(title)
                .setContentText(body)
                .setAutoCancel(true)
                .setContentIntent(content)
                .build()
            try {
                manager.notify(id.hashCode(), notification)
            } catch (_: SecurityException) {
                // No POST_NOTIFICATIONS permission: nothing to show.
            }
            NotificationBus.post(
                "received",
                JSONObject().put("id", id).put("title", title).put("body", body).put("data", NotificationBus.parse(data)),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// linking
// ---------------------------------------------------------------------------

class LinkingModule(private val context: ModuleContext) : CheetahApiModule {
    override fun invoke(method: String, args: JSONObject): Boolean {
        when (method) {
            "initialUrl" -> context.emit(
                "linking",
                "initial",
                JSONObject().put("url", LinkInbox.initialUri?.toString() ?: JSONObject.NULL),
            )
            "open" -> {
                val opened = try {
                    val intent = Intent(Intent.ACTION_VIEW, Uri.parse(args.optString("url")))
                    (context.activity ?: context.context).startActivity(
                        intent.addFlags(if (context.activity == null) Intent.FLAG_ACTIVITY_NEW_TASK else 0),
                    )
                    true
                } catch (_: Exception) {
                    false
                }
                context.emit("linking", "opened", JSONObject().put("ok", opened))
            }
            else -> return false
        }
        return true
    }

    override fun onLink(uri: Uri): Boolean {
        context.emit("linking", "url", JSONObject().put("url", uri.toString()))
        return true
    }
}

// ---------------------------------------------------------------------------
// auth
// ---------------------------------------------------------------------------

class AuthModule(private val context: ModuleContext) : CheetahApiModule {
    private var pendingScheme: String? = null

    override fun invoke(method: String, args: JSONObject): Boolean {
        when (method) {
            "session" -> session(args)
            "apple" -> context.emit(
                "auth",
                "apple",
                JSONObject().put("error", "unsupported: use auth.session with Apple's web sign-in on Android"),
            )
            "biometric" -> biometric(args.optString("reason", "Confirm it's you"))
            else -> return false
        }
        return true
    }

    /** Opens the browser; the provider's redirect to `scheme://` comes back as a link. */
    private fun session(args: JSONObject) {
        val scheme = args.optString("callbackScheme")
        if (scheme.isEmpty()) {
            context.emit("auth", "session", JSONObject().put("error", "callbackScheme is required"))
            return
        }
        pendingScheme = scheme
        try {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(args.optString("url")))
            (context.activity ?: context.context).startActivity(
                intent.addFlags(if (context.activity == null) Intent.FLAG_ACTIVITY_NEW_TASK else 0),
            )
        } catch (error: Exception) {
            pendingScheme = null
            context.emit("auth", "session", JSONObject().put("error", error.message ?: "cannot open browser"))
        }
    }

    override fun onLink(uri: Uri): Boolean {
        val scheme = pendingScheme ?: return false
        if (!uri.scheme.equals(scheme, ignoreCase = true)) return false
        pendingScheme = null
        context.emit("auth", "session", JSONObject().put("url", uri.toString()))
        return true
    }

    private fun biometric(reason: String) {
        if (Build.VERSION.SDK_INT < 28) {
            context.emit("auth", "biometric", JSONObject().put("ok", false).put("error", "unsupported"))
            return
        }
        val executor = context.context.mainExecutor
        try {
            val prompt = android.hardware.biometrics.BiometricPrompt.Builder(context.context)
                .setTitle(reason)
                .setNegativeButton("Cancel", executor) { _, _ ->
                    context.emit("auth", "biometric", JSONObject().put("ok", false).put("error", "cancelled"))
                }
                .build()
            prompt.authenticate(
                CancellationSignal(),
                executor,
                object : android.hardware.biometrics.BiometricPrompt.AuthenticationCallback() {
                    override fun onAuthenticationSucceeded(
                        result: android.hardware.biometrics.BiometricPrompt.AuthenticationResult?,
                    ) {
                        context.emit("auth", "biometric", JSONObject().put("ok", true))
                    }

                    override fun onAuthenticationError(code: Int, message: CharSequence?) {
                        context.emit(
                            "auth",
                            "biometric",
                            JSONObject().put("ok", false).put("error", message?.toString() ?: "error $code"),
                        )
                    }
                },
            )
        } catch (error: SecurityException) {
            context.emit(
                "auth",
                "biometric",
                JSONObject().put("ok", false).put("error", "add face_id to [permissions] in cheetah.toml"),
            )
        }
    }
}
