package com.example.rustui

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import org.json.JSONObject
import java.security.KeyStore
import java.util.concurrent.Executors
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * `secureStore` module (Rust: `runtime::secure_store`).
 *
 * Values are encrypted with an AES-256-GCM key that never leaves the Android
 * Keystore, then stored as `base64(iv):base64(ciphertext)` in a private
 * SharedPreferences file. The key name is bound as GCM associated data, so a
 * ciphertext copied to another key fails to decrypt.
 *
 * All work runs on one serial thread: calls complete in the order Rust
 * queued them.
 */
class SecureStoreApi(context: Context) {
    private val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    private val worker = Executors.newSingleThreadExecutor { runnable ->
        Thread(runnable, "cheetah-secure-store")
    }

    fun invoke(method: String, args: JSONObject, emit: (String, String) -> Unit): Boolean {
        if (method != "get" && method != "set" && method != "delete") return false
        val key = args.optString("key")
        val value = if (args.has("value") && !args.isNull("value")) args.getString("value") else null

        worker.execute {
            val result = JSONObject()
                .put("module", "secureStore")
                .put("method", "result")
                .put("op", method)
                .put("key", key)
            try {
                require(key.isNotEmpty()) { "missing_key" }
                when (method) {
                    "get" -> result.put("value", read(key) ?: JSONObject.NULL)
                    "set" -> write(key, requireNotNull(value) { "missing_value" })
                    "delete" -> prefs.edit().remove(key).commit()
                }
                result.put("ok", true)
            } catch (error: Exception) {
                result.put("ok", false).put("error", error.message ?: error.javaClass.simpleName)
            }
            emit("native", result.toString())
        }
        return true
    }

    private fun read(key: String): String? {
        val stored = prefs.getString(key, null) ?: return null
        val parts = stored.split(':', limit = 2)
        if (parts.size != 2) return null
        val iv = Base64.decode(parts[0], Base64.NO_WRAP)
        val encrypted = Base64.decode(parts[1], Base64.NO_WRAP)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(128, iv))
        cipher.updateAAD(key.toByteArray(Charsets.UTF_8))
        return String(cipher.doFinal(encrypted), Charsets.UTF_8)
    }

    private fun write(key: String, value: String) {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        cipher.updateAAD(key.toByteArray(Charsets.UTF_8))
        val encrypted = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val stored = Base64.encodeToString(cipher.iv, Base64.NO_WRAP) + ":" +
            Base64.encodeToString(encrypted, Base64.NO_WRAP)
        check(prefs.edit().putString(key, stored).commit()) { "write_failed" }
    }

    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getKey(ALIAS, null) as? SecretKey)?.let { return it }

        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private companion object {
        const val ANDROID_KEYSTORE = "AndroidKeyStore"
        const val ALIAS = "cheetah.secureStore"
        const val PREFS = "cheetah.secureStore"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
    }
}
