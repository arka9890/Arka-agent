package com.arka.agent.core

import android.content.Context
import org.json.JSONObject

/**
 * Helper untuk menyimpan/membaca provider dengan API key terenkripsi.
 * 
 * - Saat saveProvider: encrypt apiKey sebelum masuk ke DB.
 * - Saat testProvider / loadModels / chatStream: decrypt apiKey sebelum dipakai.
 * - Mendukung API key plaintext lama (auto-migrasi saat pertama decrypt).
 */
object ProviderEditor {

    /**
     * Simpan provider dengan apiKey terenkripsi.
     */
    fun upsert(context: Context, provider: Provider): Provider {
        val encryptedKey = if (provider.apiKey.isBlank()) {
            ""
        } else {
            try {
                ApiKeyEncryption.encrypt(context, provider.apiKey)
            } catch (e: Exception) {
                // Jika enkripsi gagal, jangan simpan plaintext
                throw IllegalStateException("Gagal mengenkripsi API key: ${e.message}")
            }
        }
        return provider.copy(apiKey = encryptedKey)
    }

    /**
     * Ambil provider dan decrypt apiKey untuk digunakan.
     * Mendukung API key plaintext lama (auto-migrasi saat pertama decrypt).
     */
    fun reveal(context: Context, provider: Provider): Provider {
        val decryptedKey = if (provider.apiKey.isBlank()) {
            ""
        } else {
            try {
                ApiKeyEncryption.decrypt(context, provider.apiKey)
            } catch (e: Exception) {
                // Jika dekripsi gagal (misal API key lama plaintext), kembalikan apa adanya
                provider.apiKey
            }
        }
        return provider.copy(apiKey = decryptedKey)
    }

    /**
     * Enkripsi JSON provider untuk disimpan ke database.
     * Dipakai saat saveProvider.
     */
    fun encryptForStorage(context: Context, provider: Provider): String {
        val encrypted = upsert(context, provider)
        return JSONObject().apply {
            put("id", encrypted.id)
            put("name", encrypted.name)
            put("kind", encrypted.kind)
            put("base_url", encrypted.baseUrl)
            put("api_key", encrypted.apiKey)
            put("model", encrypted.model)
            put("models", org.json.JSONArray(encrypted.models))
            put("is_default", encrypted.isDefault)
        }.toString()
    }

    /**
     * Dekripsi provider dari JSON database untuk digunakan.
     * Dipakai saat testProvider, loadModels, chatStream, dll.
     */
    fun decryptFromStorage(context: Context, providerJson: String): Provider {
        val json = JSONObject(providerJson)
        val encryptedProvider = Provider(
            id = json.getString("id"),
            name = json.getString("name"),
            kind = json.getString("kind"),
            baseUrl = json.getString("base_url"),
            apiKey = json.getString("api_key"),
            model = json.getString("model"),
            models = json.getJSONArray("models").let { arr ->
                List(arr.length()) { arr.getString(it) }
            },
            isDefault = json.getBoolean("is_default")
        )
        return reveal(context, encryptedProvider)
    }
}
