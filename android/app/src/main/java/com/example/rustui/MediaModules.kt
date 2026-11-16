package com.example.rustui

import android.Manifest
import android.app.Activity
import android.content.ContentValues
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.MediaStore
import android.provider.OpenableColumns
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.ConsumeParams
import com.android.billingclient.api.PendingPurchasesParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.QueryProductDetailsParams
import com.android.billingclient.api.QueryPurchasesParams
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.UUID

/*
 * Camera, photo library, documents, app files, and in-app purchases.
 * Contracts: runtime::platform::{media, files, purchases}.
 */

/** Authority of the FileProvider declared in AndroidManifest.xml. */
private fun authority(context: android.content.Context) = "${context.packageName}.cheetah.files"

/** Where picked and captured media are copied; Rust reads them with std::fs. */
private fun mediaDir(context: android.content.Context): File =
    File(context.cacheDir, "cheetah-media").apply { mkdirs() }

class MediaModule(private val context: ModuleContext) : CheetahApiModule {
    private val app = context.context.applicationContext
    private val pickRequest = RequestCodes.allocate()
    private val photoRequest = RequestCodes.allocate()
    private val documentRequest = RequestCodes.allocate()
    private val cameraPermission = RequestCodes.allocate()
    private var pendingId = ""
    private var photoFile: File? = null

    override fun invoke(method: String, args: JSONObject): Boolean {
        val activity = context.activity ?: return false
        val id = args.optString("id")
        when (method) {
            "pickImages" -> {
                pendingId = id
                val limit = args.optInt("limit", 1).coerceAtLeast(1)
                val intent = if (Build.VERSION.SDK_INT >= 33) {
                    Intent(MediaStore.ACTION_PICK_IMAGES).apply {
                        if (limit > 1) putExtra(MediaStore.EXTRA_PICK_IMAGES_MAX, limit.coerceAtMost(MediaStore.getPickImagesMaxLimit()))
                    }
                } else {
                    Intent(Intent.ACTION_GET_CONTENT).apply {
                        type = "image/*"
                        putExtra(Intent.EXTRA_ALLOW_MULTIPLE, limit > 1)
                    }
                }
                start(activity, intent, pickRequest)
            }
            "takePhoto" -> {
                pendingId = id
                takePhoto(activity)
            }
            "pickDocuments" -> {
                pendingId = id
                val types = args.optJSONArray("types")
                val mimes = if (types == null) emptyArray() else Array(types.length()) { types.optString(it) }
                val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = if (mimes.size == 1) mimes[0] else "*/*"
                    if (mimes.size > 1) putExtra(Intent.EXTRA_MIME_TYPES, mimes)
                    putExtra(Intent.EXTRA_ALLOW_MULTIPLE, true)
                }
                start(activity, intent, documentRequest)
            }
            "saveImage" -> saveImage(id, args.optString("path"))
            else -> return false
        }
        return true
    }

    private fun start(activity: Activity, intent: Intent, code: Int) {
        try {
            @Suppress("DEPRECATION")
            activity.startActivityForResult(intent, code)
        } catch (error: Exception) {
            emitFiles(pendingId, emptyList(), error.message ?: "no app can handle this")
        }
    }

    private fun takePhoto(activity: Activity) {
        // Declaring CAMERA (cheetah.toml `camera`) makes the capture intent
        // require it at runtime too.
        val declared = try {
            app.packageManager.getPackageInfo(app.packageName, PackageManager.GET_PERMISSIONS)
                .requestedPermissions?.contains(Manifest.permission.CAMERA) == true
        } catch (_: Exception) {
            false
        }
        if (declared && app.checkSelfPermission(Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
            activity.requestPermissions(arrayOf(Manifest.permission.CAMERA), cameraPermission)
            return
        }
        val file = File(mediaDir(app), "photo-${UUID.randomUUID()}.jpg")
        photoFile = file
        val uri = FileProvider.getUriForFile(app, authority(app), file)
        val intent = Intent(MediaStore.ACTION_IMAGE_CAPTURE)
            .putExtra(MediaStore.EXTRA_OUTPUT, uri)
            .addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION)
        start(activity, intent, photoRequest)
    }

    override fun onRequestPermissionsResult(requestCode: Int, grantResults: IntArray): Boolean {
        if (requestCode != cameraPermission) return false
        val activity = context.activity
        if (grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED && activity != null) {
            takePhoto(activity)
        } else {
            emitFiles(pendingId, emptyList(), "permission_denied")
        }
        return true
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?): Boolean {
        if (requestCode != pickRequest && requestCode != photoRequest && requestCode != documentRequest) {
            return false
        }
        val id = pendingId
        if (resultCode != Activity.RESULT_OK) {
            emitFiles(id, emptyList())
            return true
        }
        if (requestCode == photoRequest) {
            val file = photoFile
            emitFiles(id, if (file != null && file.length() > 0) listOf(describe(file, "image/jpeg", file.name)) else emptyList())
            return true
        }
        val uris = mutableListOf<Uri>()
        data?.clipData?.let { clip -> for (index in 0 until clip.itemCount) uris.add(clip.getItemAt(index).uri) }
        if (uris.isEmpty()) data?.data?.let { uris.add(it) }
        // Copy off the main thread; content providers can be slow.
        Thread {
            val files = uris.mapNotNull { copy(it) }
            emitFiles(id, files)
        }.start()
        return true
    }

    private fun copy(uri: Uri): JSONObject? = try {
        val resolver = app.contentResolver
        val mime = resolver.getType(uri) ?: "application/octet-stream"
        var name = "file"
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) name = cursor.getString(0) ?: name
        }
        val extension = MimeTypeMap.getSingleton().getExtensionFromMimeType(mime)
            ?: name.substringAfterLast('.', "")
        val file = File(mediaDir(app), "${UUID.randomUUID()}${if (extension.isEmpty()) "" else ".$extension"}")
        resolver.openInputStream(uri)?.use { input -> file.outputStream().use { input.copyTo(it) } }
        describe(file, mime, name)
    } catch (_: Exception) {
        null
    }

    private fun describe(file: File, mime: String, name: String) = JSONObject()
        .put("path", file.absolutePath)
        .put("name", name)
        .put("mime", mime)
        .put("size", file.length())

    private fun emitFiles(id: String, files: List<JSONObject>, error: String? = null) {
        val values = JSONObject().put("id", id).put("files", JSONArray(files))
        if (error != null) values.put("error", error)
        context.emit("media", "result", values)
    }

    private fun saveImage(id: String, path: String) {
        val result = JSONObject().put("id", id)
        try {
            val file = File(path)
            val values = ContentValues().apply {
                put(MediaStore.Images.Media.DISPLAY_NAME, file.name)
                put(MediaStore.Images.Media.MIME_TYPE, if (file.extension.equals("png", true)) "image/png" else "image/jpeg")
            }
            val uri = app.contentResolver.insert(MediaStore.Images.Media.EXTERNAL_CONTENT_URI, values)
                ?: throw IllegalStateException("cannot create the image")
            app.contentResolver.openOutputStream(uri)?.use { output -> file.inputStream().use { it.copyTo(output) } }
            result.put("ok", true)
        } catch (error: Exception) {
            result.put("ok", false).put("error", error.message ?: "failed")
        }
        context.emit("media", "saved", result)
    }
}

class FilesModule(private val context: ModuleContext) : CheetahApiModule {
    override fun invoke(method: String, args: JSONObject): Boolean {
        val app = context.context.applicationContext
        when (method) {
            "paths" -> context.emit(
                "files",
                "paths",
                JSONObject().put("documents", app.filesDir.absolutePath).put("cache", app.cacheDir.absolutePath),
            )
            "share" -> {
                val file = File(args.optString("path"))
                try {
                    val uri = FileProvider.getUriForFile(app, authority(app), file)
                    val mime = MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension) ?: "*/*"
                    val send = Intent(Intent.ACTION_SEND)
                        .setType(mime)
                        .putExtra(Intent.EXTRA_STREAM, uri)
                        .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                    val chooser = Intent.createChooser(send, null)
                    (context.activity ?: app).startActivity(
                        chooser.addFlags(if (context.activity == null) Intent.FLAG_ACTIVITY_NEW_TASK else 0),
                    )
                } catch (error: Exception) {
                    context.emit("files", "shared", JSONObject().put("ok", false).put("error", error.message ?: "failed"))
                }
            }
            else -> return false
        }
        return true
    }
}

/** Google Play Billing. Verify `token` on your server before granting anything. */
class PurchasesModule(private val context: ModuleContext) : CheetahApiModule {
    private val details = HashMap<String, ProductDetails>()
    private val queued = ArrayList<() -> Unit>()
    private var connecting = false

    private val client: BillingClient = BillingClient.newBuilder(context.context.applicationContext)
        .setListener { result, purchases -> onPurchases(result, purchases) }
        .enablePendingPurchases(PendingPurchasesParams.newBuilder().enableOneTimeProducts().build())
        .build()

    override fun invoke(method: String, args: JSONObject): Boolean {
        when (method) {
            "products" -> {
                val ids = args.optJSONArray("ids")
                val list = if (ids == null) emptyList() else (0 until ids.length()).map { ids.optString(it) }
                ready { products(list) }
            }
            "purchase" -> ready { purchase(args.optString("id")) }
            "restore" -> ready { restore() }
            "finish" -> ready { finish(args.optString("transaction"), args.optBoolean("consumable", false)) }
            else -> return false
        }
        return true
    }

    /** Run once connected to Play (connecting on first use). */
    private fun ready(action: () -> Unit) {
        if (client.isReady) {
            action()
            return
        }
        queued.add(action)
        if (connecting) return
        connecting = true
        client.startConnection(object : BillingClientStateListener {
            override fun onBillingSetupFinished(result: BillingResult) {
                connecting = false
                val actions = queued.toList()
                queued.clear()
                if (result.responseCode == BillingClient.BillingResponseCode.OK) {
                    actions.forEach { it() }
                } else {
                    context.emit("purchases", "error", JSONObject().put("error", result.debugMessage))
                }
            }

            override fun onBillingServiceDisconnected() {
                connecting = false
            }
        })
    }

    private fun products(ids: List<String>) {
        val found = JSONArray()
        var remaining = 2
        for (type in listOf(BillingClient.ProductType.INAPP, BillingClient.ProductType.SUBS)) {
            val params = QueryProductDetailsParams.newBuilder()
                .setProductList(
                    ids.map {
                        QueryProductDetailsParams.Product.newBuilder().setProductId(it).setProductType(type).build()
                    },
                )
                .build()
            client.queryProductDetailsAsync(params) { _, list ->
                synchronized(found) {
                    for (product in list) {
                        details[product.productId] = product
                        val price = product.oneTimePurchaseOfferDetails?.formattedPrice
                            ?: product.subscriptionOfferDetails?.firstOrNull()
                                ?.pricingPhases?.pricingPhaseList?.firstOrNull()?.formattedPrice
                            ?: ""
                        found.put(
                            JSONObject()
                                .put("id", product.productId)
                                .put("title", product.name)
                                .put("description", product.description)
                                .put("price", price),
                        )
                    }
                    remaining -= 1
                    if (remaining == 0) {
                        context.emit("purchases", "products", JSONObject().put("products", found))
                    }
                }
            }
        }
    }

    private fun purchase(id: String) {
        val activity = context.activity
        val product = details[id]
        if (activity == null || product == null) {
            context.emit("purchases", "purchase", JSONObject().put("id", id).put("error", "load it with purchases.products first"))
            return
        }
        val params = BillingFlowParams.ProductDetailsParams.newBuilder().setProductDetails(product)
        product.subscriptionOfferDetails?.firstOrNull()?.offerToken?.let { params.setOfferToken(it) }
        client.launchBillingFlow(
            activity,
            BillingFlowParams.newBuilder().setProductDetailsParamsList(listOf(params.build())).build(),
        )
    }

    private fun describe(purchase: Purchase): JSONObject = JSONObject()
        .put("id", purchase.products.firstOrNull() ?: "")
        .put("transaction", purchase.purchaseToken)
        .put("token", purchase.purchaseToken)
        .put("pending", purchase.purchaseState == Purchase.PurchaseState.PENDING)

    private fun onPurchases(result: BillingResult, purchases: List<Purchase>?) {
        when (result.responseCode) {
            BillingClient.BillingResponseCode.OK -> purchases.orEmpty().forEach {
                context.emit("purchases", "purchase", describe(it))
            }
            BillingClient.BillingResponseCode.USER_CANCELED ->
                context.emit("purchases", "purchase", JSONObject().put("cancelled", true))
            else -> context.emit("purchases", "purchase", JSONObject().put("error", result.debugMessage))
        }
    }

    private fun restore() {
        val all = JSONArray()
        var remaining = 2
        for (type in listOf(BillingClient.ProductType.INAPP, BillingClient.ProductType.SUBS)) {
            client.queryPurchasesAsync(QueryPurchasesParams.newBuilder().setProductType(type).build()) { _, list ->
                synchronized(all) {
                    list.forEach { all.put(describe(it)) }
                    remaining -= 1
                    if (remaining == 0) {
                        context.emit("purchases", "restore", JSONObject().put("purchases", all))
                    }
                }
            }
        }
    }

    /** Consume (buy again) or acknowledge; unacknowledged purchases refund after 3 days. */
    private fun finish(token: String, consumable: Boolean) {
        val done = { result: BillingResult ->
            context.emit(
                "purchases",
                "finished",
                JSONObject().put("transaction", token).put("ok", result.responseCode == BillingClient.BillingResponseCode.OK),
            )
        }
        if (consumable) {
            client.consumeAsync(ConsumeParams.newBuilder().setPurchaseToken(token).build()) { result, _ -> done(result) }
        } else {
            client.acknowledgePurchase(AcknowledgePurchaseParams.newBuilder().setPurchaseToken(token).build()) { done(it) }
        }
    }

    override fun dispose() {
        client.endConnection()
    }
}
