package com.arka.agent.core

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
     * Enkripsi apiKey provider untuk disimpan ke database.
     * Jika enkripsi gagal, lempar error (jangan pernah simpan plaintext).
     */
    fun upsert(provider: Provider): Provider {
        val encryptedKey = if (provider.apiKey.isBlank()) {
            ""
        } else {
            try {
                ApiKeyEncryption.encrypt(provider.apiKey)
            } catch (e: Exception) {
                throw IllegalStateException("Gagal mengenkripsi API key: ${e.message}")
            }
        }
        return provider.copy(apiKey = encryptedKey)
    }

    /**
     * Dekripsi apiKey provider untuk digunakan.
     * API key plaintext lama otomatis dikembalikan apa adanya (migrasi).
     */
    fun reveal(provider: Provider): Provider {
        val decryptedKey = if (provider.apiKey.isBlank()) {
            ""
        } else {
            try {
                ApiKeyEncryption.decrypt(provider.apiKey)
            } catch (e: Exception) {
                provider.apiKey
            }
        }
        return provider.copy(apiKey = decryptedKey)
    }

    /**
     * Enkripsi provider ke JSON untuk disimpan ke database (saveProvider).
     */
    fun encryptForStorage(provider: Provider): String {
        val encrypted = upsert(provider)
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
     * Dekripsi provider dari JSON database untuk digunakan
     * (testProvider, loadModels, chatStream, dll).
     */
    fun decryptFromStorage(providerJson: String): Provider {
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
        return reveal(encryptedProvider)
    }
}
