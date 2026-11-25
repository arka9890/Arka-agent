package com.arka.agent

import android.app.Application
import android.content.Intent
import android.os.Build
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import com.arka.agent.core.ApprovalRequest
import com.arka.agent.core.ArkaRepo
import com.arka.agent.core.AttachmentRef
import com.arka.agent.core.DesktopSession
import com.arka.agent.core.DesktopStatus
import com.arka.agent.core.DistroStatus
import com.arka.agent.core.FileInfo
import com.arka.agent.core.Message
import com.arka.agent.core.Provider
import com.arka.agent.core.ProviderStats
import com.arka.agent.core.QuestionRequest
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

    /** Konteks Application untuk memulai dialog sistem dari luar Activity. */
    private val appContext: Application = app

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

    /** Status agent saat menunggu/berjalan, ditampilkan di bubble chat. */
    private val _agentStatus = MutableStateFlow("")
    val agentStatus: StateFlow<String> = _agentStatus.asStateFlow()

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

    /**
     * Nomor urut setiap kali isi terminal berubah.
     *
     * `lines.size` tidak bisa dipakai sebagai pemicu scroll: setelah cap
     * 2000 baris tercapai, ukuran selalu 2000 walaupun isinya bergeser,
     * sehingga auto-scroll berhenti dan output baru tidak terlihat.
     */
    private val _terminalRevision = MutableStateFlow(0L)
    val terminalRevision: StateFlow<Long> = _terminalRevision.asStateFlow()

    private val _distro = MutableStateFlow(DistroStatus())
    val distro: StateFlow<DistroStatus> = _distro.asStateFlow()

    /** Status paket desktop (Xvnc + XFCE) di dalam Ubuntu. */
    private val _desktopStatus = MutableStateFlow(DesktopStatus())
    val desktopStatus: StateFlow<DesktopStatus> = _desktopStatus.asStateFlow()

    /** Sesi VNC yang sedang aktif; null kalau belum dinyalakan. */
    private val _desktopSession = MutableStateFlow<DesktopSession?>(null)
    val desktopSession: StateFlow<DesktopSession?> = _desktopSession.asStateFlow()

    private val _desktopBusy = MutableStateFlow(false)
    val desktopBusy: StateFlow<Boolean> = _desktopBusy.asStateFlow()

    /** Log apt saat memasang desktop grafis. */
    private val _desktopInstallLog = MutableStateFlow("")
    val desktopInstallLog: StateFlow<String> = _desktopInstallLog.asStateFlow()

    /** Frame desktop terakhir untuk viewer VNC. */
    private val _desktopFrame = MutableStateFlow<VncFrame?>(null)
    val desktopFrame: StateFlow<VncFrame?> = _desktopFrame.asStateFlow()

    private val _desktopState = MutableStateFlow("")
    val desktopState: StateFlow<String> = _desktopState.asStateFlow()

    /** Jumlah PointerEvent yang benar-benar dikirim ke VNC (diagnostik UI). */
    private val _desktopPointerCount = MutableStateFlow(0)
    val desktopPointerCount: StateFlow<Int> = _desktopPointerCount.asStateFlow()

    /** Klien VNC dimiliki ViewModel supaya tidak putus saat tab berganti. */
    private var vncClient: VncClient? = null

    private val _installProgress = MutableStateFlow<Pair<Int, String>?>(null)
    val installProgress: StateFlow<Pair<Int, String>?> = _installProgress.asStateFlow()

    private val _diagnostic = MutableStateFlow<String?>(null)

    /** Hasil "Tes distro" terakhir (ditampilkan di tab Setelan). */
    val diagnostic: StateFlow<String?> = _diagnostic.asStateFlow()

    /** Kartu persetujuan aksi skill yang sedang menunggu pengguna. */
    private val _approval = MutableStateFlow<ApprovalRequest?>(null)
    val approval: StateFlow<ApprovalRequest?> = _approval.asStateFlow()

    /** Kartu pertanyaan pilihan ganda dari model. */
    private val _question = MutableStateFlow<QuestionRequest?>(null)
    val question: StateFlow<QuestionRequest?> = _question.asStateFlow()

    private val _notice = MutableStateFlow<String?>(null)

    /** Pesan sekali-tampil (snackbar): error atau konfirmasi aksi. */
    val notice: StateFlow<String?> = _notice.asStateFlow()

    // Job dan buffer stream dipindahkan ke ChatEngine (scope Application).
    // Kalau tetap di sini, `viewModelScope.cancel()` saat Activity dihancurkan
    // mematikan chat dan native tidak pernah diberitahu. Lihat ChatEngine.

    /** Penjaga [appendTerminal]; output datang dari thread native juga. */
    private val terminalLock = Any()
    private var terminalPending = StringBuilder()

    /** Cegah auto-install dipicu dua kali dalam satu proses. */
    private var autoInstallStarted = false

    /** Dialog pengecualian baterai hanya ditampilkan sekali per proses. */
    private var batteryPromptShown = false

    init {
        viewModelScope.launch {
            // Dimuat berurutan (bukan lewat refreshXxx yang melempar coroutine
            // baru) supaya state di bawah sudah terisi saat dipakai.
            _settings.value = runCatching { repo.settings() }.getOrDefault(emptyMap())
            _providers.value = runCatching { repo.providers() }.getOrDefault(emptyList())
            _providerStats.value = runCatching { repo.providerStats() }.getOrDefault(emptyList())
            _sessions.value = runCatching { repo.sessions() }.getOrDefault(emptyList())
            _distro.value = runCatching { repo.distroStatus() }.getOrDefault(DistroStatus())
            _desktopStatus.value = runCatching { repo.desktopStatus() }
                .getOrDefault(DesktopStatus())
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

    /** Jawab kartu persetujuan skill (Izinkan / Tolak). */
    fun resolveApproval(id: String, approved: Boolean) = viewModelScope.launch {
        _approval.value = null
        runCatching { repo.resolveApproval(id, approved) }
            .onFailure { notify(it.message ?: "Gagal menjawab persetujuan") }
    }

    /** Jawab kartu pertanyaan pilihan ganda. */
    fun answerQuestion(id: String, choice: Int) = viewModelScope.launch {
        _question.value = null
        runCatching { repo.answerQuestion(id, choice) }
            .onFailure { notify(it.message ?: "Gagal menjawab pertanyaan") }
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
        val running = ChatEngine.isRunning(id)
        _busy.value = running
        _streamingText.value = if (running) ChatEngine.buffered(id) else ""
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

    /**
     * Ringkas riwayat sesi sekarang (menu sesi). Pesan asli tidak dihapus;
     * ringkasan hanya dipakai saat mengirim konteks ke model.
     */
    fun compactSession(id: String) = viewModelScope.launch {
        if (ChatEngine.isRunning(id)) {
            notify("Hentikan chat dulu sebelum meringkas konteks.")
            return@launch
        }
        notify("Meringkas konteks…")
        runCatching { repo.compactSession(id) }
            .onSuccess {
                notify("Konteks diringkas. Pesan lama tetap tersimpan di riwayat.")
            }
            .onFailure { notify(it.message ?: "Gagal meringkas konteks") }
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
        // Sesi lain tidak terganggu; kirim ke sesi yang masih sibuk ditolak
        // dengan pesan yang jelas (dulu dibuang diam-diam → terasa freeze).
        if (ChatEngine.isRunning(sessionId)) {
            notify("Chat masih berjalan. Tekan Stop atau tunggu selesai.")
            return
        }

        _busy.value = true
        _agentStatus.value = "Menyiapkan jawaban…"
        _streamingText.value = ""
        ChatEngine.clearBuffer(sessionId)

        // Job dipilik oleh ChatEngine (scope Application), bukan viewModelScope:
        // kalau Activity dibuat ulang saat model masih menjawab, chat tidak
        // ikut terbatalkan. Foreground service menyalakan CPU + jaringan.
        val background = settings.value["background_run"] != "false"
        if (background && !batteryPromptShown && Build.VERSION.SDK_INT >= Build.VERSION_CODES.M &&
            !BatteryOptimization.isIgnoring(appContext)
        ) {
            batteryPromptShown = true
            // Dialog sistem sekali jalan. Kalau pengguna menolak, masih bisa
            // mengaktifkannya kapan saja di Setelan → Agent.
            runCatching {
                appContext.startActivity(
                    BatteryOptimization.requestIntent(appContext)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
                )
            }
        }
        var job: Job? = null
        job = ChatEngine.scope.launch {
            try {
                val attachmentJson = attachments.joinToString(",", "[", "]") { it.toJson() }
                val userMsg = repo.addMessage(sessionId, "user", body, attachmentJson)
                // Jangan tampilkan pesan dari sesi latar belakang di chat
                // sesi lain. Saat sesinya dibuka, riwayat dibaca ulang dari DB.
                if (_currentSessionId.value == sessionId) {
                    _messages.value = _messages.value + userMsg
                }
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

                // Riwayat dibaca langsung dari database, bukan dari _messages:
                // state UI bisa masih berisi pesan sesi lain kalau pengguna
                // baru pindah sesi lalu langsung mengirim. Kalau itu terjadi,
                // model menerima percakapan yang salah dan terlihat seperti
                // "kehilangan konteks".
                val history = repo.messages(sessionId).map { m ->
                    if (m.role == "user" && m.id == userMsg.id && fileContext.isNotEmpty()) {
                        m.copy(content = "${m.content}\n\n$fileContext")
                    } else {
                        m
                    }
                }

                // Terminal dipakai bersama oleh semua sesi, jadi beri label
                // supaya output dua sesi yang jalan bersamaan tetap jelas.
                val toolLabel = session?.title?.take(24) ?: sessionId.take(8)

                val result = repo.chat(
                    sessionId = sessionId,
                    providerId = provider?.id,
                    model = session?.model ?: provider?.model,
                    history = history,
                    onDelta = { delta ->
                        // Simpan ke buffer global dulu supaya UI baru yang
                        // dibuat ulang tetap bisa menampilkan jawaban yang
                        // sedang berjalan.
                        ChatEngine.onDelta(sessionId, delta)
                        // Tampilkan hanya jika sesi ini masih yang terbuka;
                        // kalau pengguna pindah sesi, jawaban tetap disimpan
                        // ke sesinya tanpa mengganggu tampilan sesi aktif.
                        if (_currentSessionId.value == sessionId) {
                            _streamingText.value = ChatEngine.buffered(sessionId)
                        }
                    },
                    onTool = { command, output ->
                        // Perintah agent ditampilkan di Terminal supaya
                        // pengguna melihat apa yang dijalankan model. Leading
                        // newline mem-flush sisa baris dari sesi sebelumnya.
                        appendTerminal("\n[$toolLabel] $ $command\n$output\n")
                        // Refresh daftar file tanpa mengubah folder yang sedang
                        // dibuka pengguna (dulu sesi latar bisa memindahkan
                        // tab File ke direktori lain).
                        ChatEngine.scope.launch {
                            runCatching { _files.value = repo.files(_currentDir.value) }
                        }
                    },
                    onApproval = { request ->
                        // Callback datang dari thread native; StateFlow aman
                        // ditulis dari thread mana pun.
                        _approval.value = request
                    },
                    onQuestion = { request -> _question.value = request },
                    onProgress = { _, label -> _agentStatus.value = label },
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
                ChatEngine.finish(sessionId, job)
                // Chat selesai: kartu yang masih menggantung tidak relevan lagi.
                _approval.value = null
                _question.value = null
                _agentStatus.value = ""
                if (_currentSessionId.value == sessionId) {
                    _busy.value = false
                    _streamingText.value = ""
                }
            }
        }
        ChatEngine.begin(sessionId, job!!, background)
    }

    /** Hentikan chat yang sedang berjalan pada sesi aktif. */
    fun stopChat() {
        val sessionId = _currentSessionId.value ?: return
        viewModelScope.launch {
            // Beritahu sisi native dulu: flag cancel dicek di loop stream dan
            // eksekusi tool, jadi perintah lama ikut berhenti.
            runCatching { repo.cancelChat(sessionId) }
            ChatEngine.cancelJob(sessionId)
            _agentStatus.value = ""
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

    // --------------------------------------------------------- desktop/VNC

    fun refreshDesktop() = viewModelScope.launch {
        _desktopStatus.value = runCatching { repo.desktopStatus() }
            .getOrDefault(DesktopStatus())
    }

    /** Pasang Xvnc + XFCE (~ratusan MB). Output apt ditampilkan di kartu. */
    fun installDesktop() {
        if (_desktopBusy.value) return
        _desktopBusy.value = true
        _desktopInstallLog.value = "Menyiapkan apt di Ubuntu…\n"
        viewModelScope.launch {
            val result = repo.installDesktop { chunk ->
                val gabung = _desktopInstallLog.value + chunk
                _desktopInstallLog.value = if (gabung.length > 6_000) {
                    gabung.takeLast(6_000)
                } else {
                    gabung
                }
            }
            result.onFailure { notify(it.message ?: "Gagal memasang desktop") }
            if (result.isSuccess) refreshDesktop()
            _desktopBusy.value = false
        }
    }

    /** Nyalakan Xvnc + XFCE, lalu simpan sesinya untuk viewer. */
    fun startDesktop() {
        if (_desktopBusy.value) return
        _desktopBusy.value = true
        viewModelScope.launch {
            val result = runCatching { repo.startDesktop() }
            result.onSuccess {
                _desktopSession.value = it
                refreshDesktop()
            }
            result.onFailure { notify(it.message ?: "Gagal menyalakan desktop") }
            _desktopBusy.value = false
        }
    }

    fun stopDesktop() {
        if (_desktopBusy.value) return
        disconnectDesktop()
        viewModelScope.launch {
            runCatching { repo.stopDesktop() }
                .onFailure { notify(it.message ?: "Gagal mematikan desktop") }
            _desktopSession.value = null
            refreshDesktop()
        }
    }

    fun connectDesktop(host: String, port: Int) {
        vncClient?.stop()
        _desktopFrame.value = null
        vncClient = VncClient(
            host,
            port,
            onFrame = { _desktopFrame.value = it },
            onState = { _desktopState.value = it },
        )
        vncClient?.start()
    }

    fun disconnectDesktop() {
        vncClient?.stop()
        vncClient = null
        _desktopFrame.value = null
        _desktopState.value = ""
    }

    fun desktopPointer(x: Int, y: Int, buttons: Int) {
        _desktopPointerCount.value += 1
        vncClient?.pointer(x, y, buttons)
    }
    fun desktopKey(keysym: Int) = vncClient?.keyPress(keysym)
    fun desktopType(text: String) = vncClient?.typeText(text)

    override fun onCleared() {
        disconnectDesktop()
        super.onCleared()
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
            flushTerminal()
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
        // Instalasi memegang TOOL_LOCK; kalau dibiarkan, perintah diam tanpa
        // output sampai instalasi selesai. Lebih baik ditolak dengan jelas.
        if (_installProgress.value != null) {
            notify("Ubuntu masih dipasang — tunggu sampai selesai.")
            return
        }
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
                // Job dibatalkan (Activity hancur): hentikan juga proses native
                // supaya TOOL_LOCK tidak terus dipegang oleh perintah yatim.
                runCatching { repo.cancelExec() }
                appendTerminal("${e.message}\n")
            } finally {
                flushTerminal()
                _terminalRunning.value = false
            }
        }
    }

    fun stopCommand() = viewModelScope.launch {
        repo.cancelExec()
    }

    fun clearTerminal() {
        synchronized(terminalLock) {
            terminalPending = StringBuilder()
            // Reset di dalam lock supaya potongan dari thread native tidak
            // menimpa hasil clear.
            _terminalLines.value = emptyList()
            _terminalRevision.value++
        }
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
        flushTerminal()
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
        _terminalRevision.value++
    }

    /**
     * Tampilkan sisa buffer yang belum punya newline.
     *
     * Tanpa ini, output seperti `printf hello` (tanpa `\n`) tertahan di
     * `terminalPending` selamanya dan layar hanya menampilkan prompt-nya.
     */
    private fun flushTerminal() {
        synchronized(terminalLock) {
            val rest = terminalPending.toString()
            terminalPending = StringBuilder()
            if (rest.isNotEmpty()) pushTerminalLines(listOf(rest))
        }
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
