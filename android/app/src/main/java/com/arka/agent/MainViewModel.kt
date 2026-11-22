package com.arka.agent

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.arka.agent.core.ArkaRepo
import com.arka.agent.core.AttachmentRef
import com.arka.agent.core.DistroStatus
import com.arka.agent.core.FileInfo
import com.arka.agent.core.Message
import com.arka.agent.core.Provider
import com.arka.agent.core.ProviderStats
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

    companion object {
        /** Setting penanda kapan auto-install terakhir dicoba. */
        private const val AUTO_INSTALL_KEY = "rootfs_auto_attempt"

        /** Jeda minimum sebelum auto-install boleh mencoba lagi (6 jam). */
        private const val AUTO_INSTALL_RETRY_MS = 6 * 60 * 60 * 1000L
    }

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

    private val _providerStats = MutableStateFlow<List<ProviderStats>>(emptyList())
    val providerStats: StateFlow<List<ProviderStats>> = _providerStats.asStateFlow()

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

    private val _diagnostic = MutableStateFlow<String?>(null)

    /** Hasil "Tes distro" terakhir (ditampilkan di tab Setelan). */
    val diagnostic: StateFlow<String?> = _diagnostic.asStateFlow()

    private val _notice = MutableStateFlow<String?>(null)

    /** Pesan sekali-tampil (snackbar): error atau konfirmasi aksi. */
    val notice: StateFlow<String?> = _notice.asStateFlow()

    /**
     * Job chat per sesi, bukan satu job global.
     *
     * Dulu satu `chatJob` dipakai bersama: memilih sesi lain membatalkan job
     * itu, dan `send()` menolak pesan baru selama job lama masih hidup —
     * akibatnya chat di sesi baru langsung "gagal"/terbatal. Sekarang tiap
     * sesi punya job sendiri sehingga chat di sesi berbeda berjalan paralel
     * tanpa saling mengganggu.
     */
    private val chatJobs = mutableMapOf<String, Job>()

    /** Buffer stream per sesi (jawaban yang sedang dialirkan). */
    private val streamBuffers = mutableMapOf<String, StringBuilder>()

    /** Penjaga [appendTerminal]; output datang dari thread native juga. */
    private val terminalLock = Any()
    private var terminalPending = StringBuilder()

    /** Cegah auto-install dipicu dua kali dalam satu proses. */
    private var autoInstallStarted = false

    init {
        viewModelScope.launch {
            // Dimuat berurutan (bukan lewat refreshXxx yang melempar coroutine
            // baru) supaya state di bawah sudah terisi saat dipakai.
            _settings.value = runCatching { repo.settings() }.getOrDefault(emptyMap())
            _providers.value = runCatching { repo.providers() }.getOrDefault(emptyList())
            _providerStats.value = runCatching { repo.providerStats() }.getOrDefault(emptyList())
            _sessions.value = runCatching { repo.sessions() }.getOrDefault(emptyList())
            _distro.value = runCatching { repo.distroStatus() }.getOrDefault(DistroStatus())
            _files.value = runCatching { repo.files("") }.getOrDefault(emptyList())
            _terminalHistory.value = runCatching { repo.terminalHistory() }.getOrDefault(emptyList())
            ArkaApp.initError?.let { notify(it) }
            // Pilih sesi terbaru supaya pengguna langsung bisa mengetik.
            _sessions.value.firstOrNull()?.let { selectSession(it.id) }
            autoInstallIfNeeded()
        }
    }

    /**
     * PRD 4.2: rootfs Ubuntu diunduh otomatis saat aplikasi pertama kali
     * dibuka. Dipanggil setelah status distro diketahui.
     *
     * Percobaan otomatis dibatasi (maksimal sekali per [AUTO_INSTALL_RETRY_MS])
     * supaya perangkat offline tidak mencoba mengunduh 30 MB setiap aplikasi
     * dibuka; tombol "Pasang sekarang" tetap bisa dipakai kapan saja.
     */
    private fun autoInstallIfNeeded() {
        val status = _distro.value
        if (status.installed || autoInstallStarted) return
        if (!status.prootAvailable) {
            // Tanpa binary proot rootfs tidak bisa dijalankan sama sekali.
            return
        }
        val last = _settings.value[AUTO_INSTALL_KEY]?.toLongOrNull() ?: 0L
        if (System.currentTimeMillis() - last < AUTO_INSTALL_RETRY_MS) return

        autoInstallStarted = true
        updateSetting(AUTO_INSTALL_KEY, System.currentTimeMillis().toString())
        notify("Mengunduh Ubuntu 24 (±30 MB). Progres ada di tab Terminal.")
        installUbuntu()
    }

    fun notify(message: String) {
        _notice.value = message
    }

    fun clearNotice() {
        _notice.value = null
    }

    // ------------------------------------------------------------- sesi

    fun refreshSessions() = viewModelScope.launch { reloadSessions() }

    /** Versi suspend: daftar sesi sudah terisi saat baris berikutnya berjalan. */
    private suspend fun reloadSessions() {
        _sessions.value = runCatching { repo.sessions() }.getOrElse {
            notify(it.message ?: "Gagal memuat sesi")
            emptyList()
        }
    }

    fun selectSession(id: String) {
        // Jangan batalkan chat sesi lama saat pindah sesi: streaming di sesi
        // lama boleh lanjut di latar belakang dan jawabannya tetap disimpan
        // ke sesinya sendiri. Yang kita lakukan hanya "lepas" dari tampilan
        // streaming sesi lama supaya UI sesi baru bersih.
        _currentSessionId.value = id
        // Sesi ini masih menerima jawaban di latar belakang? Tampilkan lagi
        // streaming-nya dan kunci tombol kirim jadi tombol berhenti.
        val running = chatJobs[id]?.isActive == true
        _busy.value = running
        _streamingText.value = if (running) {
            streamBuffers[id]?.toString().orEmpty()
        } else {
            ""
        }
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
            // Tunggu daftar sesi terisi dulu supaya UI tidak sempat menampilkan
            // layar "belum ada sesi" setelah menekan tombol buat sesi.
            reloadSessions()
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
            reloadSessions()
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
     * [attachments] adalah lampiran dari file manager perangkat (SAF). File
     * TIDAK disalin ke workspace: hanya URI `content://` yang disimpan, dan
     * isinya dibaca langsung dari penyedia dokumen saat dibutuhkan.
     */
    fun send(text: String, attachments: List<AttachmentRef> = emptyList()) {
        val body = text.trim()
        if (body.isEmpty()) return
        val sessionId = _currentSessionId.value ?: run {
            notify("Buat sesi baru dulu.")
            return
        }
        // Sesi yang sedang mengetik boleh mengirim lagi; sesi lain tidak
        // terganggu. Cek job milik sesi ini saja.
        if (chatJobs[sessionId]?.isActive == true) return

        _busy.value = true
        streamBuffers[sessionId] = StringBuilder()
        _streamingText.value = ""

        val job = viewModelScope.launch {
            try {
                val attachmentJson = attachments.joinToString(",", "[", "]") { it.toJson() }
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
                val fileContext = attachments.mapNotNull { att ->
                    if (att.kind != "text") return@mapNotNull null
                    runCatching {
                        val content = repo.readText(att.uri).take(20_000)
                        "Isi file `${att.name}`:\n```\n$content\n```"
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
                    onDelta = { delta ->
                        val buffer = streamBuffers.getOrPut(sessionId) { StringBuilder() }
                        buffer.append(delta)
                        // Tampilkan hanya jika sesi ini masih yang terbuka;
                        // kalau pengguna pindah sesi, jawaban tetap disimpan
                        // ke sesinya tanpa mengganggu tampilan sesi aktif.
                        if (_currentSessionId.value == sessionId) {
                            _streamingText.value = buffer.toString()
                        }
                    },
                    onTool = { command, output ->
                        // Perintah agent ditampilkan di Terminal supaya
                        // pengguna melihat apa yang dijalankan model.
                        appendTerminal("$ $command\n$output\n")
                        viewModelScope.launch { refreshFiles(_currentDir.value) }
                    },
                )

                result.onFailure { notify(it.message ?: "Gagal menghubungi AI") }
                if (_currentSessionId.value == sessionId) {
                    _streamingText.value = ""
                    _messages.value = repo.messages(sessionId)
                }
                refreshSessions()
            } catch (e: Exception) {
                // Pembatalan (stopChat) bukan error: jangan tampilkan notif.
                if (e is kotlinx.coroutines.CancellationException) throw e
                notify(e.message ?: "Terjadi kesalahan")
            } finally {
                chatJobs.remove(sessionId)
                streamBuffers.remove(sessionId)
                if (_currentSessionId.value == sessionId) {
                    _busy.value = false
                    _streamingText.value = ""
                }
            }
        }
        chatJobs[sessionId] = job
    }

    /** Hentikan chat yang sedang berjalan pada sesi aktif. */
    fun stopChat() {
        val sessionId = _currentSessionId.value ?: return
        viewModelScope.launch {
            repo.cancelChat(sessionId)
            chatJobs.remove(sessionId)?.cancel()
            if (_currentSessionId.value == sessionId) {
                _busy.value = false
                _streamingText.value = ""
            }
        }
    }

    // --------------------------------------------------------- provider

    fun refreshProviderStats() = viewModelScope.launch {
        _providerStats.value = runCatching { repo.providerStats() }.getOrElse { emptyList() }
    }

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

    /** Cek model dari server hanya saat tombol refresh ditekan. */
    fun refreshModels(providerId: String, onResult: (List<String>) -> Unit) = viewModelScope.launch {
        val models = runCatching { repo.refreshModels(providerId) }.getOrElse {
            notify(it.message ?: "Gagal mengecek model")
            runCatching { repo.models(providerId) }.getOrDefault(emptyList())
        }
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
        // Lampiran SAF (content://) disalin dari penyedia dokumen; file
        // workspace disalin dari rootfs.
        val job = if (path.startsWith("content://")) {
            runCatching { repo.exportUriToDownloads(path) }
        } else {
            runCatching { repo.exportToDownloads(path) }
        }
        job.onSuccess { notify(it) }
            .onFailure { notify(it.message ?: "Gagal mengunduh file") }
    }

    fun readText(path: String, onResult: (Result<String>) -> Unit) = viewModelScope.launch {
        onResult(runCatching { repo.readText(path) })
    }

    /** Info satu file; dipakai saat lampiran chat diketuk (buka pratinjau). */
    fun statFile(path: String, onResult: (Result<FileInfo>) -> Unit) = viewModelScope.launch {
        onResult(
            if (path.startsWith("content://")) {
                runCatching { repo.statUri(path) }
            } else {
                runCatching { repo.fileStat(path) }
            },
        )
    }

    /**
     * Bangun [AttachmentRef] dari URI yang dipilih lewat SAF.
     *
     * Nama + jenis diambil dari penyedia dokumen supaya bubble chat bisa
     * menampilkan ikon/thumbnail yang benar tanpa menyentuh workspace.
     */
    fun describeUris(
        uris: List<android.net.Uri>,
        onDone: (List<AttachmentRef>) -> Unit,
    ) = viewModelScope.launch {
        val refs = uris.mapNotNull { uri ->
            runCatching {
                val info = repo.statUri(uri.toString())
                AttachmentRef(uri = info.path, name = info.name, kind = info.kind)
            }.getOrNull()
        }
        onDone(refs)
    }

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
            val result = repo.installUbuntu(
                onProgress = { percent, label ->
                    _installProgress.value = percent to label
                },
                // Output apt-get ikut ditampilkan di tab Terminal supaya
                // pengguna tahu prosesnya benar-benar berjalan.
                onOutput = { chunk -> appendTerminal(chunk) },
            )
            _installProgress.value = null
            refreshDistro()
            result
                .onSuccess { notify("Ubuntu 24 siap dipakai.") }
                .onFailure { notify(it.message ?: "Pemasangan gagal") }
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
        synchronized(terminalLock) { terminalPending = StringBuilder() }
        _terminalLines.value = emptyList()
    }

    fun clearTerminalHistory() = viewModelScope.launch {
        runCatching { repo.clearTerminalHistory() }
        _terminalHistory.value = emptyList()
    }

    fun runDiagnostic() = viewModelScope.launch {
        _diagnostic.value = "Menjalankan tes distro…"
        appendTerminal("\$ arka-diagnostic\n")
        runCatching { repo.diagnostic() }
            .onSuccess {
                _diagnostic.value = it
                appendTerminal("$it\n")
            }
            .onFailure {
                _diagnostic.value = it.message ?: "Tes distro gagal"
                appendTerminal("${it.message}\n")
            }
    }

    /**
     * Tambahkan output mentah ke terminal.
     *
     * Output proot datang sebagai potongan sembarang (bisa di tengah baris),
     * jadi potongan disimpan dulu sampai ada newline. Tanpa buffer ini setiap
     * potongan tampil sebagai baris baru dan output `apt`/`git` jadi berantakan.
     */
    private fun appendTerminal(chunk: String) {
        synchronized(terminalLock) {
            terminalPending.append(chunk.replace("\r\n", "\n").replace('\r', '\n'))
            val text = terminalPending.toString()
            val cut = text.lastIndexOf('\n')
            if (cut < 0) {
                // Belum ada baris utuh. Potongan sangat panjang (mis. prompt
                // tanpa newline) tetap ditampilkan supaya tidak "hilang".
                if (terminalPending.length > 8192) {
                    terminalPending = StringBuilder()
                    pushTerminalLines(listOf(text))
                }
                return
            }
            terminalPending = StringBuilder(text.substring(cut + 1))
            pushTerminalLines(text.substring(0, cut).split("\n"))
        }
    }

    private fun pushTerminalLines(newLines: List<String>) {
        if (newLines.isEmpty()) return
        // Batasi riwayat tampilan supaya scroll tidak melambat setelah ribuan
        // baris output (mis. `apt upgrade`).
        val merged = _terminalLines.value + newLines
        _terminalLines.value = if (merged.size > 2000) merged.takeLast(2000) else merged
    }

    // --------------------------------------------------------- settings

    fun updateSetting(key: String, value: String) = viewModelScope.launch {
        runCatching {
            repo.setSetting(key, value)
            _settings.value = repo.settings()
        }.onFailure { notify(it.message ?: "Gagal menyimpan pengaturan") }
    }

    /** Muat prompt sistem bawaan (dipakai dialog Setelan → Agent). */
    fun loadDefaultSystemPrompt(onResult: (String) -> Unit) = viewModelScope.launch {
        onResult(runCatching { repo.defaultSystemPrompt() }.getOrDefault(""))
    }
}
