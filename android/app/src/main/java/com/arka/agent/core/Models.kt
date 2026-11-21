package com.arka.agent.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/** Satu sesi chat. `messageCount` hanya terisi saat daftar sesi dimuat. */
@Serializable
data class Session(
    val id: String,
    val title: String,
    @SerialName("provider_id") val providerId: String? = null,
    val model: String? = null,
    @SerialName("created_at") val createdAt: Long = 0,
    @SerialName("updated_at") val updatedAt: Long = 0,
    @SerialName("message_count") val messageCount: Int = 0,
)

/** Satu pesan. [attachments] berisi JSON `[{path,kind}]` file workspace. */
@Serializable
data class Message(
    val id: String,
    @SerialName("session_id") val sessionId: String,
    /** `user` | `assistant` | `system` */
    val role: String,
    val content: String,
    val attachments: String = "[]",
    @SerialName("created_at") val createdAt: Long = 0,
)

@Serializable
data class Provider(
    val id: String = "",
    val name: String = "",
    /** `openai` | `anthropic` */
    val kind: String = "openai",
    @SerialName("base_url") val baseUrl: String = "",
    @SerialName("api_key") val apiKey: String = "",
    /** Model utama yang dipakai chat kalau sesi tidak punya model. */
    val model: String = "",
    /** Semua model yang dipilih di tab Provider (ditampilkan di dropdown chat). */
    val models: List<String> = emptyList(),
    @SerialName("is_default") val isDefault: Boolean = false,
    @SerialName("created_at") val createdAt: Long = 0,
)

/** Satu entri di tab File. [path] relatif terhadap workspace. */
@Serializable
data class FileInfo(
    val path: String = "",
    val name: String = "",
    @SerialName("is_dir") val isDir: Boolean = false,
    val size: Long = 0,
    val modified: Long = 0,
    /** `dir` | `text` | `image` | `video` | `pdf` | `binary` */
    val kind: String = "binary",
)

/**
 * Lampiran chat yang dipilih dari file manager perangkat lewat SAF.
 *
 * [uri] berupa `content://…` — file TIDAK disalin ke workspace (PRD: pilih
 * langsung dari file manager, jangan upload dulu). [kind] dihitung dari MIME
 * saat dipilih supaya bubble chat bisa menampilkan thumbnail yang benar.
 */
data class AttachmentRef(
    val uri: String,
    val name: String,
    val kind: String,
) {
    /** JSON yang disimpan di kolom `attachments` pesan. */
    fun toJson(): String =
        """{"path":${jsonStr(uri)},"kind":${jsonStr(kind)},"name":${jsonStr(name)}}"""

    private fun jsonStr(s: String) =
        "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"") + "\""
}

@Serializable
data class DistroStatus(
    val installed: Boolean = false,
    @SerialName("proot_available") val prootAvailable: Boolean = false,
    @SerialName("size_bytes") val sizeBytes: Long = 0,
    @SerialName("rootfs_path") val rootfsPath: String = "",
    val message: String = "",
)
