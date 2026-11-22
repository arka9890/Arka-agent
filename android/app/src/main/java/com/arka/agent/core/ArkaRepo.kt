package com.arka.agent.core

import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.OpenableColumns
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.io.FileOutputStream

/**
 * Satu-satunya pintu ke [Arka]: membungkus panggilan native dengan
 * `Dispatchers.IO` dan mengubah JSON menjadi model.
 *
 * UI tidak pernah memanggil [Arka] langsung supaya tidak ada operasi berat
 * yang tidak sengaja berjalan di main thread.
 */
class ArkaRepo(private val context: Context) {

    companion object {
        private val json = Json {
            ignoreUnknownKeys = true
            encodeDefaults = true
        }

        /** Ukuran maksimum file yang boleh diimpor ke workspace (50 MB). */
        private const val MAX_IMPORT_BYTES = 50L * 1024 * 1024
    }

    private val appContext = context.applicationContext

    // ------------------------------------------------------------- sesi

    suspend fun sessions(): List<Session> = io {
        json.decodeFromString(Arka.sessionsJson())
    }

    suspend fun createSession(title: String, providerId: String?, model: String?): Session = io {
        json.decodeFromString(
            Arka.createSession(title, providerId.orEmpty(), model.orEmpty()),
        )
    }

    suspend fun renameSession(id: String, title: String) = io { Arka.renameSession(id, title) }

    suspend fun deleteSession(id: String) = io { Arka.deleteSession(id) }

    suspend fun messages(sessionId: String): List<Message> = io {
        json.decodeFromString(Arka.messagesJson(sessionId))
    }

    suspend fun addMessage(
        sessionId: String,
        role: String,
        content: String,
        attachments: String = "[]",
    ): Message = io {
        json.decodeFromString(Arka.addMessage(sessionId, role, content, attachments))
    }

    suspend fun autotitle(sessionId: String, text: String) =
        io { Arka.autotitleSession(sessionId, text) }

    suspend fun setSessionModel(sessionId: String, providerId: String?, model: String?) =
        io { Arka.setSessionModel(sessionId, providerId.orEmpty(), model.orEmpty()) }

    /**
     * Ringkas riwayat sesi sekarang (manual). Mengembalikan teks ringkasan
     * yang baru disimpan; string kosong kalau provider tidak mengirim teks.
     */
    suspend fun compactSession(sessionId: String): String = io {
        val raw = Arka.compactSession(sessionId)
        runCatching {
            json.parseToJsonElement(raw).jsonObject["summary"]?.jsonPrimitive?.content.orEmpty()
        }.getOrDefault("")
    }

    // --------------------------------------------------------- provider

    suspend fun providers(): List<Provider> = io {
        json.decodeFromString(Arka.providersJson())
    }

    suspend fun saveProvider(provider: Provider): Provider = io {
        json.decodeFromString(Arka.saveProvider(json.encodeToString(Provider.serializer(), provider)))
    }

    suspend fun deleteProvider(id: String) = io { Arka.deleteProvider(id) }

    suspend fun setDefaultProvider(id: String) = io { Arka.setDefaultProvider(id) }

    suspend fun testProvider(provider: Provider): String = io {
        Arka.testProvider(json.encodeToString(Provider.serializer(), provider))
    }

    /** Daftar model lokal (instan, tanpa network). */
    suspend fun models(providerId: String): List<String> = io {
        json.decodeFromString(Arka.modelsJson(providerId))
    }

    /** Cek model dari server saat tombol refresh ditekan. */
    suspend fun refreshModels(providerId: String): List<String> = io {
        json.decodeFromString(Arka.refreshModels(providerId))
    }

    /** Statistik penggunaan semua provider (id, total_tokens, total_requests, last_used_at). */
    suspend fun providerStats(): List<ProviderStats> = io {
        json.decodeFromString(Arka.providerStatsJson())
    }

    // --------------------------------------------------------------- AI

    /**
     * Kirim percakapan; [onDelta] dipanggil untuk tiap potongan jawaban.
     *
     * Native memanggil callback dari thread latar, jadi di sini hasilnya
     * diteruskan apa adanya — pemanggil (ViewModel) yang memindahkan ke
     * main thread.
     */
    suspend fun chat(
        sessionId: String,
        providerId: String?,
        model: String?,
        history: List<Message>,
        onDelta: (String) -> Unit,
        onTool: (String, String) -> Unit = { _, _ -> },
    ): Result<String> = io {
        runCatching {
            val payload = json.encodeToString(
                kotlinx.serialization.builtins.ListSerializer(Message.serializer()),
                history,
            )
            var error: String? = null
            val id = Arka.chatStream(
                sessionId,
                providerId.orEmpty(),
                model.orEmpty(),
                payload,
                object : Arka.ChatCallback {
                    override fun onDelta(text: String) = onDelta(text)
                    override fun onDone(fullText: String) {}
                    override fun onError(message: String) {
                        error = message
                    }
                    override fun onTool(command: String, output: String) = onTool(command, output)
                },
            )
            error?.let { throw IllegalStateException(it) }
            id
        }
    }

    suspend fun cancelChat(sessionId: String) = io { Arka.cancelChat(sessionId) }

    // ------------------------------------------------------------ file

    suspend fun files(path: String): List<FileInfo> = io {
        json.decodeFromString(Arka.filesJson(path))
    }

    /**
     * Baca isi file teks.
     *
     * [path] bisa berupa path relatif workspace ATAU URI SAF `content://`
     * (lampiran chat tidak disalin ke workspace, jadi dibaca langsung dari
     * penyedia dokumen perangkat).
     */
    suspend fun readText(path: String): String = io {
        if (path.startsWith("content://")) readUriText(path) else Arka.readText(path)
    }

    /** Baca maksimal 64 KB pertama dari URI SAF (cukup untuk konteks model). */
    private fun readUriText(uri: String): String {
        val stream = appContext.contentResolver.openInputStream(Uri.parse(uri))
            ?: throw IllegalStateException("File tidak bisa dibaca")
        val text: String = stream.use { input ->
            val buf = java.io.ByteArrayOutputStream()
            val tmp = ByteArray(8192)
            while (buf.size() < 64 * 1024) {
                val n = input.read(tmp)
                if (n <= 0) break
                buf.write(tmp, 0, n)
            }
            buf.toString("UTF-8")
        }
        return text.take(20_000)
    }

    /**
     * Info satu file dari URI SAF: nama, ukuran, dan jenis dari MIME.
     *
     * Dipakai saat user memilih lampiran dan saat lampiran diketuk untuk
     * pratinjau — path `content://` tidak bisa distat lewat native workspace.
     */
    suspend fun statUri(uri: String): FileInfo = io { statUriSync(uri) }

    /** Versi sinkron dari [statUri]; hanya dipanggil dari dalam [io]. */
    private fun statUriSync(uri: String): FileInfo {
        val u = Uri.parse(uri)
        val resolver = appContext.contentResolver
        var name = u.lastPathSegment?.substringAfterLast('/') ?: "berkas"
        var size = -1L
        resolver.query(u, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
            ?.use { c ->
                if (c.moveToFirst()) {
                    val di = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (di >= 0 && !c.isNull(di)) name = c.getString(di)
                    val si = c.getColumnIndex(OpenableColumns.SIZE)
                    if (si >= 0 && !c.isNull(si)) size = c.getLong(si)
                }
            }
        val mime = resolver.getType(u).orEmpty()
        return FileInfo(
            path = uri,
            name = name,
            isDir = false,
            size = size.coerceAtLeast(0),
            modified = 0,
            kind = kindOf(mime, name),
        )
    }

    /**
     * Salin isi URI SAF ke folder Download publik.
     *
     * Sama seperti [exportToDownloads] memakai MediaStore (tanpa izin sejak
     * Android 10) — hanya sumbernya stream SAF, bukan file workspace.
     */
    suspend fun exportUriToDownloads(uri: String): String = io {
        val src = appContext.contentResolver.openInputStream(Uri.parse(uri))
            ?: throw IllegalStateException("File tidak bisa dibaca")
        val name = statUriSync(uri).name
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, name)
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, mimeOf(name))
                put(
                    android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                    Environment.DIRECTORY_DOWNLOADS,
                )
            }
            val target = appContext.contentResolver.insert(
                android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                values,
            ) ?: throw IllegalStateException("Gagal membuat file di Download")
            src.use { input ->
                appContext.contentResolver.openOutputStream(target)?.use { out ->
                    input.copyTo(out)
                } ?: throw IllegalStateException("Gagal menulis ke Download")
            }
        } else {
            val dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
            dir.mkdirs()
            val target = File(dir, name)
            src.use { input -> FileOutputStream(target).use { input.copyTo(it) } }
        }
        "Tersimpan di Download/$name"
    }

    /** Jenis konten lampiran SAF dari MIME + nama berkas. */
    private fun kindOf(mime: String, name: String): String = when {
        mime.startsWith("image/") -> "image"
        mime.startsWith("video/") -> "video"
        mime == "application/pdf" -> "pdf"
        mime.startsWith("text/") ||
            mime == "application/json" ||
            mime == "application/xml" ||
            mime.endsWith("+json") -> "text"
        else -> when (name.substringAfterLast('.', "").lowercase()) {
            "png", "jpg", "jpeg", "gif", "webp", "bmp" -> "image"
            "mp4", "mkv", "webm", "3gp" -> "video"
            "pdf" -> "pdf"
            "txt", "md", "log", "json", "xml", "csv", "py", "kt", "rs", "sh", "java", "js", "html", "css", "yml", "yaml", "toml" -> "text"
            else -> "binary"
        }
    }

    suspend fun fileStat(path: String): FileInfo = io {
        json.decodeFromString(Arka.fileStat(path))
    }

    suspend fun deleteFile(path: String) = io { Arka.deleteFile(path) }

    suspend fun mkdir(path: String) = io { Arka.mkdir(path) }

    suspend fun renameFile(from: String, to: String) = io { Arka.renameFile(from, to) }

    suspend fun workspaceSize(): Long = io { Arka.workspaceSize() }

    /**
     * Salin file yang dipilih user (SAF) ke workspace.
     *
     * Ukuran dibaca dari `OpenableColumns` lebih dulu supaya file raksasa
     * ditolak sebelum dibaca ke memori — `readBytes()` pada file 1 GB akan
     * menghabiskan RAM perangkat.
     */
    suspend fun importFromUri(uri: Uri, dir: String = ""): FileInfo = io {
        val resolver = appContext.contentResolver
        val name = resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }
            ?: "berkas-${System.currentTimeMillis()}"

        val size = resolver.query(uri, arrayOf(OpenableColumns.SIZE), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst() && !cursor.isNull(0)) cursor.getLong(0) else -1L
            } ?: -1L
        if (size > MAX_IMPORT_BYTES) {
            throw IllegalArgumentException(
                "File terlalu besar (${size / 1024 / 1024} MB). Maksimum 50 MB.",
            )
        }

        val bytes = resolver.openInputStream(uri)?.use { it.readBytes() }
            ?: throw IllegalStateException("File tidak bisa dibaca")
        val rel = if (dir.isBlank()) name else "$dir/$name"
        json.decodeFromString(Arka.writeBytes(rel, bytes))
    }

    /**
     * Salin file workspace ke folder Download publik supaya bisa dibuka
     * aplikasi lain. Sejak Android 10 memakai MediaStore (tanpa izin).
     */
    suspend fun exportToDownloads(path: String): String = io {
        val source = File(workspaceRoot(), path)
        if (!source.isFile) throw IllegalStateException("File tidak ditemukan")
        val name = source.name
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val values = android.content.ContentValues().apply {
                put(android.provider.MediaStore.MediaColumns.DISPLAY_NAME, name)
                put(android.provider.MediaStore.MediaColumns.MIME_TYPE, mimeOf(name))
                put(
                    android.provider.MediaStore.MediaColumns.RELATIVE_PATH,
                    Environment.DIRECTORY_DOWNLOADS,
                )
            }
            val uri = appContext.contentResolver.insert(
                android.provider.MediaStore.Downloads.EXTERNAL_CONTENT_URI,
                values,
            ) ?: throw IllegalStateException("Gagal membuat file di Download")
            appContext.contentResolver.openOutputStream(uri)?.use { out ->
                source.inputStream().use { it.copyTo(out) }
            } ?: throw IllegalStateException("Gagal menulis ke Download")
            "Tersimpan di Download/$name"
        } else {
            val dir = Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS)
            dir.mkdirs()
            val target = File(dir, name)
            source.inputStream().use { input ->
                FileOutputStream(target).use { input.copyTo(it) }
            }
            "Tersimpan di ${target.absolutePath}"
        }
    }

    /** Path host file workspace (untuk preview gambar/video). */
    fun workspaceFile(path: String): File = File(workspaceRoot(), path)

    private fun workspaceRoot(): File = File(appContext.filesDir, "workspace")

    private fun mimeOf(name: String): String = when (name.substringAfterLast('.', "").lowercase()) {
        "png" -> "image/png"
        "jpg", "jpeg" -> "image/jpeg"
        "gif" -> "image/gif"
        "webp" -> "image/webp"
        "mp4" -> "video/mp4"
        "pdf" -> "application/pdf"
        "txt", "md", "log", "json", "py", "kt", "rs", "sh" -> "text/plain"
        else -> "application/octet-stream"
    }

    // ---------------------------------------------------- distro/shell

    suspend fun distroStatus(): DistroStatus = io {
        json.decodeFromString(Arka.distroStatusJson())
    }

    /**
     * Pasang Ubuntu 24. [onProgress] dipanggil dari thread native dengan
     * persentase + label tahap.
     */
    suspend fun installUbuntu(
        onProgress: (Int, String) -> Unit,
        onOutput: (String) -> Unit = {},
    ): Result<Unit> = io {
        runCatching {
            var error: String? = null
            Arka.installUbuntu(object : Arka.InstallCallback {
                override fun onProgress(percent: Int, label: String) = onProgress(percent, label)
                override fun onOutput(chunk: String) = onOutput(chunk)
                override fun onError(message: String) {
                    error = message
                }
            })
            error?.let { throw IllegalStateException(it) }
            Unit
        }
    }

    suspend fun cancelInstall() = io { Arka.cancelInstall() }

    /**
     * Jalankan perintah di Ubuntu. Output mengalir ke [onOutput]; nilai
     * kembalian adalah exit code (-1 kalau gagal/dihentikan).
     */
    suspend fun exec(command: String, onOutput: (String) -> Unit): Int = io {
        Arka.execCommand(
            command,
            object : Arka.ExecCallback {
                override fun onOutput(chunk: String) = onOutput(chunk)
            },
        )
    }

    suspend fun cancelExec() = io { Arka.cancelExec() }

    suspend fun diagnostic(): String = io { Arka.diagnostic() }

    // --------------------------------------------------------- setting

    suspend fun settings(): Map<String, String> = io {
        json.decodeFromString(Arka.settingsJson())
    }

    suspend fun setSetting(key: String, value: String) = io { Arka.setSetting(key, value) }

    /** Prompt sistem bawaan, untuk tombol "kembalikan default" di Setelan. */
    suspend fun defaultSystemPrompt(): String = io { Arka.defaultSystemPrompt() }

    // ----------------------------------------------------------- skill

    suspend fun skills(): List<Skill> = io {
        json.decodeFromString(Arka.skillsJson())
    }

    /** Impor isi SKILL.md; mengembalikan skill yang tersimpan. */
    suspend fun importSkill(raw: String, fallbackName: String): Skill = io {
        json.decodeFromString(Arka.importSkill(raw, fallbackName))
    }

    suspend fun deleteSkill(id: String) = io { Arka.deleteSkill(id) }

    suspend fun setSkillEnabled(id: String, enabled: Boolean) = io {
        Arka.setSkillEnabled(id, enabled)
    }

    suspend fun terminalHistory(): List<String> = io {
        json.decodeFromString(Arka.terminalHistoryJson())
    }

    suspend fun pushTerminalHistory(command: String): List<String> = io {
        json.decodeFromString(Arka.pushTerminalHistory(command))
    }

    suspend fun clearTerminalHistory() = io { Arka.clearTerminalHistory() }

    private suspend fun <T> io(block: () -> T): T = withContext(Dispatchers.IO) { block() }
}
