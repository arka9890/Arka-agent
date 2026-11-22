package com.arka.agent.core

import android.content.Context
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.OpenableColumns
import com.arka.agent.core.Arka
import com.arka.agent.core.FileInfo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive
import java.io.File
import java.io.FileOutputStream

class ArkaRepo(
    private val appContext: Context,
    private val json: Json,
) {

    // ------------------------------------------------------------- sesi
    suspend fun sessions(): List<Session> = io {
        val arr = json.parseToJsonElement(Arka.sessionsJson()).jsonArray
        arr.mapNotNull { json.decodeFromJsonElement(Session.serializer(), it) }
    }

    suspend fun createSession(title: String, providerId: String, model: String): String = io {
        Arka.createSession(title, providerId, model)
    }

    suspend fun renameSession(id: String, title: String) = io { Arka.renameSession(id, title) }

    suspend fun deleteSession(id: String) = io { Arka.deleteSession(id) }

    suspend fun messages(sessionId: String): List<Message> = io {
        val arr = json.parseToJsonElement(Arka.messagesJson(sessionId)).jsonArray
        arr.mapNotNull { json.decodeFromJsonElement(Message.serializer(), it) }
    }

    suspend fun addMessage(sessionId: String, role: String, content: String, attachments: String = "[]"): Message = io {
        val result = Arka.addMessage(sessionId, role, content, attachments)
        json.decodeFromJsonElement(Message.serializer(), json.parseToJsonElement(result))
    }

    suspend fun autotitleSession(sessionId: String, text: String) = io {
        Arka.autotitleSession(sessionId, text)
    }

    suspend fun setSessionModel(sessionId: String, providerId: String, model: String) = io {
        Arka.setSessionModel(sessionId, providerId, model)
    }

    // --------------------------------------------------------- provider
    suspend fun providers(): List<Provider> = io {
        val arr = json.parseToJsonElement(Arka.providersJson()).jsonArray
        arr.mapNotNull { json.decodeFromJsonElement(Provider.serializer(), it) }
    }

    suspend fun saveProvider(provider: Provider): Provider = io {
        // Enkripsi API key sebelum disimpan
        val encrypted = ProviderEditor.encryptForStorage(appContext, provider)
        val saved = Arka.saveProvider(encrypted)
        json.decodeFromJsonElement(Provider.serializer(), json.parseToJsonElement(saved))
    }

    suspend fun deleteProvider(id: String) = io { Arka.deleteProvider(id) }

    suspend fun setDefaultProvider(id: String) = io { Arka.setDefaultProvider(id) }

    suspend fun testProvider(provider: Provider): String = io {
        // Dekripsi API key untuk test connection
        val decrypted = ProviderEditor.decryptFromStorage(appContext, json.encodeToString(Provider.serializer(), provider))
        Arka.testProvider(json.encodeToString(Provider.serializer(), decrypted))
    }

    suspend fun models(providerId: String): List<String> = io {
        val arr = json.parseToJsonElement(Arka.modelsJson(providerId)).jsonArray
        arr.mapNotNull { it.jsonPrimitive.content }
    }

    suspend fun refreshModels(providerId: String): List<String> = io {
        val arr = json.parseToJsonElement(Arka.refreshModels(providerId)).jsonArray
        arr.mapNotNull { it.jsonPrimitive.content }
    }

    suspend fun providerStats(): List<ProviderStats> = io {
        val arr = json.parseToJsonElement(Arka.providerStatsJson()).jsonArray
        arr.mapNotNull { json.decodeFromJsonElement(ProviderStats.serializer(), it) }
    }

    // --------------------------------------------------------------- AI
    suspend fun chatStream(
        sessionId: String,
        providerId: String,
        model: String,
        messagesJson: String,
        onDelta: (String) -> Unit,
        onDone: (String) -> Unit,
        onError: (String) -> Unit,
        onTool: (String, String) -> Unit,
    ): String = io {
        // Ambil provider dan decrypt API key
        val provider = providers().find { it.id == providerId }
            ?: throw IllegalStateException("Provider tidak ditemukan")
        val decryptedProvider = ProviderEditor.reveal(appContext, provider)

        Arka.chatStream(
            sessionId,
            providerId,
            model,
            messagesJson,
            object : Arka.ChatCallback {
                override fun onDelta(text: String) = onDelta(text)
                override fun onDone(fullText: String) = onDone(fullText)
                override fun onError(message: String) = onError(message)
                override fun onTool(command: String, output: String) = onTool(command, output)
            },
        )
    }

    suspend fun cancelChat(sessionId: String) = io { Arka.cancelChat(sessionId) }

    // ------------------------------------------------------------ file
    suspend fun files(path: String): FileInfo = io {
        val arr = json.parseToJsonElement(Arka.filesJson(path)).jsonArray
        json.decodeFromJsonElement(FileInfo.serializer(), arr[0])
    }

    suspend fun readText(path: String): String = io { Arka.readText(path) }

    suspend fun statUri(uri: String): FileInfo = io {
        statUriSync(uri)
    }

    private fun statUriSync(uri: String): FileInfo {
        val u = Uri.parse(uri)
        var name = u.lastPathSegment?.substringAfterLast('/') ?: "berkas"
        var size = -1L
        appContext.contentResolver.query(u, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)
            ?.use { c ->
                if (c.moveToFirst()) {
                    val di = c.getColumnIndex(OpenableColumns.DISPLAY_NAME)
                    if (di >= 0 && !c.isNull(di)) name = c.getString(di)
                    val si = c.getColumnIndex(OpenableColumns.SIZE)
                    if (si >= 0 && !c.isNull(si)) size = c.getLong(si)
                }
            }
        val mime = appContext.contentResolver.getType(u).orEmpty()
        return FileInfo(
            path = uri,
            name = name,
            isDir = false,
            size = size.coerceAtLeast(0),
            modified = 0,
            kind = kindOf(mime, name),
        )
    }

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

    suspend fun defaultSystemPrompt(): String = io { Arka.defaultSystemPrompt() }

    suspend fun terminalHistory(): List<String> = io {
        json.decodeFromString(Arka.terminalHistoryJson())
    }

    suspend fun pushTerminalHistory(command: String): List<String> = io {
        json.decodeFromString(Arka.pushTerminalHistory(command))
    }

    suspend fun clearTerminalHistory() = io { Arka.clearTerminalHistory() }

    private suspend fun <T> io(block: () -> T): T = withContext(Dispatchers.IO) { block() }

    private fun JsonPrimitive.contentOrNull: String? =
        if (isString) content else null
}
