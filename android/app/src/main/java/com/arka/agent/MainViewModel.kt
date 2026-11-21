package com.arka.agent

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.arka.agent.core.ArkaRepo
import com.arka.agent.core.DistroStatus
import com.arka.agent.core.FileInfo
import com.arka.agent.core.Message
import com.arka.agent.core.Provider
import com.arka.agent.core.Session
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * State seluruh aplikasi.
 *
 * Satu ViewModel dipakai bersama lima tab karena datanya saling terkait
 * (mis. file yang dibuat dari Terminal langsung muncul di tab File, dan sesi
 * chat menentukan provider mana yang dipakai). Pemecahan per tab akan membuat
 * sinkronisasi antar-tab jauh lebih rumit daripada manfaatnya.
 */
class MainViewModel(app: Application) : AndroidViewModel(app) {

    private val repo = ArkaRepo(app)

    // ------------------------------------------------------------- state

    private val _settings = MutableStateFlow<Map<String, String>>(emptyMap())
    val settings: StateFlow<Map<String, String>> = _settings.asStateFlow()

    private val _sessions = MutableStateFlow<List<Session>>(emptyList())
    val sessions: StateFlow<List<Session>> = _sessions.asStateFlow()

    private val _currentSessionId = MutableStateFlow<String?>(null)
    val currentSessionId: StateFlow<String?> = _currentSessionId.asStateFlow()

    private val _messages = MutableStateFlow<List<Message>>(emptyList())
    val messages: StateFlow<List<Message>> = _messages.asStateFlow()

    /** Teks jawaban yang sedang dialirkan (belum tersimpan). */
    private val _streamingText = MutableStateFlow("")
    val streamingText: StateFlow<String> = _streamingText.asStateFlow()

    private val _busy = MutableStateFlow(false)
    val busy: StateFlow<Boolean> = _busy.asStateFlow()

    private val _providers = MutableStateFlow<List<Provider>>(emptyList())
    val providers: StateFlow<List<Provider>> = _providers.asStateFlow()

    private val _files = MutableStateFlow<List<FileInfo>>(emptyList())
    val files: StateFlow<List<FileInfo>> = _files.asStateFlow()

    private val _currentDir = MutableStateFlow("")
    val currentDir: StateFlow<String> = _currentDir.asStateFlow()

    private val _terminalLines = MutableStateFlow<List<String>>(emptyList())
    val terminalLines: StateFlow<List<String>> = _terminalLines.asStateFlow()

    private val _terminalHistory = MutableStateFlow<List<String>>(emptyList())
    val terminalHistory: StateFlow<List<String>> = _terminalHistory.asStateFlow()

    private val _terminalRunning = MutableStateFlow(false)
    val terminalRunning: StateFlow<Boolean> = _terminalRunning.asStateFlow()

    private val _distro = MutableStateFlow(DistroStatus())
    val distro: StateFlow<DistroStatus> = _distro.asStateFlow()

    private val _installProgress = MutableStateFlow<Pair<Int, String>?>(null)
    val installProgress: StateFlow<Pair<Int, String>?> = _installProgress.asStateFlow()

    private val _notice = MutableStateFlow<String?>(null)

    /** Pesan sekali-tampil (snackbar): error atau konfirmasi aksi. */
    val notice: StateFlow<String?> = _notice.asStateFlow()

    private var chatJob: Job? = null
    private var streamBuffer = StringBuilder()

    init {
        viewModelScope.launch {
            _settings.value = repo.settings()
            refreshSessions()
            refreshProviders()
            refreshDistro()
            refreshFiles("")
            _terminalHistory.value = repo.terminalHistory()
            ArkaApp.initError?.let { notify(it) }
            // Pilih sesi terbaru supaya pengguna langsung bisa mengetik.
            _sessions.value.firstOrNull()?.let { selectSession(it.id) }
        }
    }

    fun notify(message: String) {
        _notice.value = message
    }

    fun clearNotice() {
        _notice.value = null
    }

    // ------------------------------------------------------------- sesi

    fun refreshSessions() = viewModelScope.launch {
        _sessions.value = runCatching { repo.sessions() }.getOrElse {
            notify(it.message ?: "Gagal memuat sesi")
            emptyList()
        }
    }

    fun selectSession(id: String) {
        _currentSessionId.value = id
        viewModelScope.launch {
            _messages.value = runCatching { repo.messages(id) }.getOrElse {
                notify(it.message ?: "Gagal memuat pesan")
                emptyList()
            }
        }
    }

    fun newSession() = viewModelScope.launch {
        val provider = _providers.value.firstOrNull { it.isDefault } ?: _providers.value.firstOrNull()
        runCatching {
            val s = repo.createSession("Sesi baru", provider?.id, provider?.model)
            refreshSessions()
            selectSession(s.id)
        }.onFailure { notify(it.message ?: "Gagal membuat sesi") }
    }

    fun renameSession(id: String, title: String) = viewModelScope.launch {
        runCatching {
            repo.renameSession(id, title)
            refreshSessions()
        }.onFailure { notify(it.message ?: "Gagal mengganti nama sesi") }
    }

    fun deleteSession(id: String) = viewModelScope.launch {
        runCatching {
            repo.deleteSession(id)
            refreshSessions()
            if (_currentSessionId.value == id) {
                _currentSessionId.value = null
                _messages.value = emptyList()
                _sessions.value.firstOrNull()?.let { selectSession(it.id) }
            }
        }.onFailure { notify(it.message ?: "Gagal menghapus sesi") }
    }

    // -------------------------------------------------------------- chat

    /**
     * Kirim pesan user lalu alirkan jawaban AI.
     *
     * [attachments] adalah daftar path file workspace yang ikut dikirim; isinya
     * ditempel ke pesan sebagai konteks supaya model bisa membacanya tanpa
     * tool tambahan.
     */
    fun send(text: String, attachments: List<String> = emptyList()) {
        val body = text.trim()
        if (body.isEmpty() || _busy.value) return
        val sessionId = _currentSessionId.value ?: run {
            notify("Buat sesi baru dulu.")
            return
        }

        _busy.value = true
        streamBuffer = StringBuilder()
        _streamingText.value = ""

        chatJob = viewModelScope.launch {
            try {
                val attachmentJson = attachments.joinToString(",", "[", "]") {
                    """{"path":${quote(it)},"kind":"text"}"""
                }
                val userMsg = repo.addMessage(sessionId, "user", body, attachmentJson)
                _messages.value = _messages.value + userMsg
                repo.autotitle(sessionId, body)
                refreshSessions()

                val session = _sessions.value.firstOrNull { it.id == sessionId }
                val provider = session?.providerId?.let { pid -> _providers.value.find { it.id == pid } }
                    ?: _providers.value.firstOrNull { it.isDefault }
                    ?: _providers.value.firstOrNull()

                // Konteks file: sertakan isi file teks yang dilampirkan supaya
                // model benar-benar "melihat" file tanpa tool tambahan.
                val fileContext = attachments.mapNotNull { path ->
                    runCatching {
                        val content = repo.readText(path).take(20_000)
                        "Isi file `$path`:\n```\n$content\n```"
                    }.getOrNull()
                }.joinToString("\n\n")

                val history = (_messages.value).map { m ->
                    if (m.role == "user" && m.id == userMsg.id && fileContext.isNotEmpty()) {
                        m.copy(content = "${m.content}\n\n$fileContext")
                    } else {
                        m
                    }
                }

                val result = repo.chat(
                    sessionId = sessionId,
                    providerId = provider?.id,
                    model = session?.model ?: provider?.model,
                    history = history,
                ) { delta ->
                    streamBuffer.append(delta)
                    _streamingText.value = streamBuffer.toString()
                }

                result.onFailure { notify(it.message ?: "Gagal menghubungi AI") }
                _streamingText.value = ""
                _messages.value = repo.messages(sessionId)
                refreshSessions()
            } catch (e: Exception) {
                notify(e.message ?: "Terjadi kesalahan")
            } finally {
                _busy.value = false
            }
        }
    }

    fun stopChat() = viewModelScope.launch {
        repo.cancelChat()
        chatJob?.cancel()
    }

    private fun quote(s: String) = "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"") + "\""

    // --------------------------------------------------------- provider

    fun refreshProviders() = viewModelScope.launch {
        _providers.value = runCatching { repo.providers() }.getOrElse {
            notify(it.message ?: "Gagal memuat provider")
            emptyList()
        }
    }

    fun saveProvider(provider: Provider, onDone: () -> Unit = {}) = viewModelScope.launch {
        runCatching {
            repo.saveProvider(provider)
            refreshProviders()
            onDone()
        }.onFailure { notify(it.message ?: "Gagal menyimpan provider") }
    }

    fun deleteProvider(id: String) = viewModelScope.launch {
        runCatching {
            repo.deleteProvider(id)
            refreshProviders()
        }.onFailure { notify(it.message ?: "Gagal menghapus provider") }
    }

    fun setDefaultProvider(id: String) = viewModelScope.launch {
        runCatching {
            repo.setDefaultProvider(id)
            refreshProviders()
        }.onFailure { notify(it.message ?: "Gagal mengubah provider default") }
    }

    fun testProvider(provider: Provider, onResult: (String) -> Unit) = viewModelScope.launch {
        onResult("Menguji koneksi…")
        val message = runCatching { repo.testProvider(provider) }
            .getOrElse { it.message ?: "Gagal menguji koneksi" }
        onResult(message)
    }

    fun loadModels(providerId: String, onResult: (List<String>) -> Unit) = viewModelScope.launch {
        val models = runCatching { repo.models(providerId) }.getOrDefault(emptyList())
        onResult(models)
    }

    /** Ganti model/provider untuk sesi aktif (dipilih di header chat). */
    fun setSessionModel(providerId: String?, model: String?) = viewModelScope.launch {
        val id = _currentSessionId.value ?: return@launch
        runCatching {
            repo.setSessionModel(id, providerId, model)
            refreshSessions()
        }.onFailure { notify(it.message ?: "Gagal mengganti model") }
    }

    // ------------------------------------------------------------- file

    fun refreshFiles(path: String) = viewModelScope.launch {
        runCatching {
            _files.value = repo.files(path)
            _currentDir.value = path
        }.onFailure { notify(it.message ?: "Gagal membaca folder") }
    }

    fun deleteFile(path: String) = viewModelScope.launch {
        runCatching {
            repo.deleteFile(path)
            refreshFiles(_currentDir.value)
        }.onFailure { notify(it.message ?: "Gagal menghapus file") }
    }

    fun newFolder(name: String) = viewModelScope.launch {
        val dir = _currentDir.value
        val rel = if (dir.isBlank()) name else "$dir/$name"
        runCatching {
            repo.mkdir(rel)
            refreshFiles(dir)
        }.onFailure { notify(it.message ?: "Gagal membuat folder") }
    }

    fun importFile(uri: android.net.Uri) = viewModelScope.launch {
        runCatching {
            repo.importFromUri(uri, _currentDir.value)
            refreshFiles(_currentDir.value)
            notify("File diimpor.")
        }.onFailure { notify(it.message ?: "Gagal mengimpor file") }
    }

    fun exportFile(path: String) = viewModelScope.launch {
        runCatching { repo.exportToDownloads(path) }
            .onSuccess { notify(it) }
            .onFailure { notify(it.message ?: "Gagal mengunduh file") }
    }

    fun readText(path: String, onResult: (Result<String>) -> Unit) = viewModelScope.launch {
        onResult(runCatching { repo.readText(path) })
    }

    /** Baca satu folder tanpa mengubah state tab File (dipakai dialog lampiran). */
    suspend fun filesNow(path: String): List<FileInfo> = repo.files(path)

    suspend fun workspaceSizeNow(): Long = repo.workspaceSize()

    fun workspaceFile(path: String) = repo.workspaceFile(path)

    // --------------------------------------------------------- terminal

    fun refreshDistro() = viewModelScope.launch {
        _distro.value = runCatching { repo.distroStatus() }.getOrElse { DistroStatus() }
    }

    fun installUbuntu() {
        if (_installProgress.value != null) return
        _installProgress.value = 0 to "Menyiapkan…"
        viewModelScope.launch {
            val result = repo.installUbuntu { percent, label ->
                _installProgress.value = percent to label
            }
            _installProgress.value = null
            refreshDistro()
            result.onFailure { notify(it.message ?: "Pemasangan gagal") }
        }
    }

    fun cancelInstall() = viewModelScope.launch {
        repo.cancelInstall()
        _installProgress.value = null
    }

    fun runCommand(command: String) {
        if (command.isBlank() || _terminalRunning.value) return
        _terminalRunning.value = true
        appendTerminal("\$ $command\n")
        viewModelScope.launch {
            try {
                _terminalHistory.value = runCatching { repo.pushTerminalHistory(command) }
                    .getOrDefault(_terminalHistory.value)
                repo.exec(command) { chunk -> appendTerminal(chunk) }
                // Perintah bisa membuat/menghapus file: segarkan tab File.
                refreshFiles(_currentDir.value)
            } catch (e: Exception) {
                appendTerminal("${e.message}\n")
            } finally {
                _terminalRunning.value = false
            }
        }
    }

    fun stopCommand() = viewModelScope.launch {
        repo.cancelExec()
    }

    fun clearTerminal() {
        _terminalLines.value = emptyList()
    }

    fun clearTerminalHistory() = viewModelScope.launch {
        runCatching { repo.clearTerminalHistory() }
        _terminalHistory.value = emptyList()
    }

    fun runDiagnostic() = viewModelScope.launch {
        appendTerminal("\$ arka-diagnostic\n")
        runCatching { repo.diagnostic() }
            .onSuccess { appendTerminal("$it\n") }
            .onFailure { appendTerminal("${it.message}\n") }
    }

    private fun appendTerminal(chunk: String) {
        // Batasi riwayat tampilan supaya scroll tidak melambat setelah ribuan
        // baris output (mis. `apt upgrade`).
        val merged = (_terminalLines.value + chunk.split("\n").dropLast(1).let {
            if (chunk.endsWith("\n")) it else it + chunk.split("\n").last()
        })
        _terminalLines.value = if (merged.size > 2000) merged.takeLast(2000) else merged
    }

    // --------------------------------------------------------- settings

    fun updateSetting(key: String, value: String) = viewModelScope.launch {
        runCatching {
            repo.setSetting(key, value)
            _settings.value = repo.settings()
        }.onFailure { notify(it.message ?: "Gagal menyimpan pengaturan") }
    }
}
