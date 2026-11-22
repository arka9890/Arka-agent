package com.arka.agent.core

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Modul enkripsi API key menggunakan AES-256-GCM.
 *
 * - Master key disimpan di Android Keystore (non-ekspor, hardware-backed jika tersedia).
 * - Data dienkripsi dengan AES-256-GCM (12-byte nonce + 128-bit auth tag).
 * - Format output: `enc:v1:<base64(nonce|ciphertext|tag)>`
 * - Kompatibel dengan API key plaintext lama (auto-migrasi saat decrypt).
 */
object ApiKeyEncryption {
    private const val KEY_ALIAS = "arka_api_key_master"
    private const val KEYSTORE_PROVIDER = "AndroidKeyStore"
    private const val AES_GCM_NOPADDING = "AES/GCM/NoPadding"
    private const val TAG_LENGTH_BITS = 128
    private const val NONCE_LENGTH = 12
    private const val PREFIX = "enc:v1:"

    private val secureRandom = java.security.SecureRandom()

    @Volatile private var _cipher: Cipher? = null
    private fun getCipher(): Cipher = _cipher ?: Cipher.getInstance(AES_GCM_NOPADDING).also { _cipher = it }

    /**
     * Mendapatkan/menghasilkan master key di Android Keystore.
     */
    private fun getMasterKey(): SecretKey {
        val keyStore = KeyStore.getInstance(KEYSTORE_PROVIDER).apply { load(null) }
        if (!keyStore.containsAlias(KEY_ALIAS)) {
            KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE_PROVIDER)
                .apply {
                    init(KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                        .setKeySize(256)
                        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                        .setUserAuthenticationRequired(false)
                        .setIsStrongBoxBacked(true)
                        .build())
                }.generateKey()
        }
        return keyStore.getKey(KEY_ALIAS, null) as SecretKey
    }

    /**
     * Enkripsi plaintext API key.
     * Mengembalikan string format: `enc:v1:<base64(nonce|ciphertext|tag)>`
     */
    fun encrypt(plaintext: String): String {
        if (plaintext.isBlank()) return plaintext
        // Sudah terenkripsi?
        if (plaintext.startsWith(PREFIX)) return plaintext

        val masterKey = getMasterKey()
        val cipher = getCipher()

        val nonceArray = ByteArray(NONCE_LENGTH).also { secureRandom.nextBytes(it) }
        val spec = GCMParameterSpec(TAG_LENGTH_BITS, nonceArray)
        cipher.init(Cipher.ENCRYPT_MODE, masterKey, spec)

        val ciphertext = cipher.doFinal(plaintext.toByteArray())
        // Gabungkan: nonce | ciphertext
        val combined = ByteArray(NONCE_LENGTH + ciphertext.size)
        System.arraycopy(nonceArray, 0, combined, 0, NONCE_LENGTH)
        System.arraycopy(ciphertext, 0, combined, NONCE_LENGTH, ciphertext.size)

        return PREFIX + Base64.encodeToString(combined, Base64.NO_WRAP)
    }

    /**
     * Dekripsi ciphertext ke plaintext.
     * Jika input sudah plaintext (tidak berprefiks `enc:v1:`), dikembalikan apa adanya.
     */
    fun decrypt(ciphertext: String): String {
        if (ciphertext.isBlank() || !ciphertext.startsWith(PREFIX)) return ciphertext

        val masterKey = getMasterKey()
        val cipher = getCipher()

        val data = Base64.decode(ciphertext.substring(PREFIX.length), Base64.NO_WRAP)
        if (data.size < NONCE_LENGTH + 1) throw IllegalArgumentException("Ciphertext terlalu pendek")

        val nonceArray = ByteArray(NONCE_LENGTH)
        System.arraycopy(data, 0, nonceArray, 0, NONCE_LENGTH)
        val ciphertextBytes = ByteArray(data.size - NONCE_LENGTH)
        System.arraycopy(data, NONCE_LENGTH, ciphertextBytes, 0, ciphertextBytes.size)

        val spec = GCMParameterSpec(TAG_LENGTH_BITS, nonceArray)
        cipher.init(Cipher.DECRYPT_MODE, masterKey, spec)

        return String(cipher.doFinal(ciphertextBytes))
    }

    /**
     * Cek apakah string sudah terenkripsi.
     */
    fun isEncrypted(value: String): Boolean = value.startsWith(PREFIX)
}
