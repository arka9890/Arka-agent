package com.arka.agent.core

/**
 * Jembatan ke `libarka_core.so` (Rust).
 *
 * Aturan pakai:
 *  - Setiap fungsi di sini **blocking**. Panggil dari `Dispatchers.IO`
 *    (lihat [ArkaRepo]) supaya UI tidak tersendat saat unduh rootfs / apt /
 *    panggil API.
 *  - Fungsi yang mengembalikan data mengembalikan JSON (sudah dideserialisasi
 *    oleh [ArkaRepo]); fungsi yang gagal melempar [RuntimeException] dengan
 *    pesan berbahasa Indonesia yang siap ditampilkan.
 *  - Callback (interface di dalam object ini) dipanggil dari thread native,
 *    jadi implementasinya wajib memindahkan hasil ke main thread sendiri.
 */
object Arka {

    init {
        System.loadLibrary("arka_core")
    }

    /** Inisialisasi: buka database + siapkan folder. Panggil sekali di Application. */
    external fun nativeInit(filesDir: String, nativeLibDir: String)

    // ------------------------------------------------------------- sesi
    external fun sessionsJson(): String
    external fun createSession(title: String, providerId: String, model: String): String
    external fun renameSession(id: String, title: String)
    external fun deleteSession(id: String)
    external fun messagesJson(sessionId: String): String
    external fun addMessage(
        sessionId: String,
        role: String,
        content: String,
        attachments: String,
    ): String
    external fun autotitleSession(sessionId: String, text: String)
    external fun setSessionModel(sessionId: String, providerId: String, model: String)

    /**
     * Ringkas riwayat sesi sekarang (manual). Mengembalikan JSON
     * `{ "summary": "..." }`. Memakai provider/model sesi.
     */
    external fun compactSession(sessionId: String): String

    // --------------------------------------------------------- provider
    external fun providersJson(): String
    external fun saveProvider(json: String): String
    external fun deleteProvider(id: String)
    external fun setDefaultProvider(id: String)
    external fun testProvider(json: String): String
    external fun modelsJson(providerId: String): String

    /** Cek daftar model dari server (manual). Hasilnya disimpan di DB. */
    external fun refreshModels(providerId: String): String
    /** Rekam penggunaan provider (dipakai Rust saat chat selesai). */
    external fun recordProviderUsage(providerId: String, tokens: Long, requests: Long)

    /** Statistik penggunaan semua provider (id, total_tokens, total_requests, last_used_at). */
    external fun providerStatsJson(): String


    // --------------------------------------------------------------- AI
    /**
     * Kirim percakapan dan alirkan jawaban ke [cb].
     * Mengembalikan id pesan assistant yang tersimpan ("" kalau gagal).
     */
    external fun chatStream(
        sessionId: String,
        providerId: String,
        model: String,
        messagesJson: String,
        cb: ChatCallback,
    ): String

    external fun cancelChat(sessionId: String)

    // ------------------------------------------------------------ file
    external fun filesJson(path: String): String
    external fun readText(path: String): String
    external fun fileStat(path: String): String
    external fun deleteFile(path: String)
    external fun mkdir(path: String)
    external fun renameFile(from: String, to: String)
    external fun writeBytes(path: String, data: ByteArray): String
    external fun workspaceSize(): Long

    // ---------------------------------------------------- distro/shell
    external fun distroStatusJson(): String
    external fun installUbuntu(cb: InstallCallback)
    external fun cancelInstall()
    external fun execCommand(command: String, cb: ExecCallback): Int
    external fun cancelExec()
    external fun diagnostic(): String

    // --------------------------------------------------------- setting
    external fun settingsJson(): String
    external fun setSetting(key: String, value: String)

    // ------------------------------------------------------------- skill
    external fun skillsJson(): String
    external fun importSkill(raw: String, fallbackName: String): String
    external fun deleteSkill(id: String)
    external fun setSkillEnabled(id: String, enabled: Boolean)

    /** Prompt sistem bawaan Arka Agent (teks mentah, bukan JSON). */
    external fun defaultSystemPrompt(): String

    external fun terminalHistoryJson(): String
    external fun pushTerminalHistory(command: String): String
    external fun clearTerminalHistory()

    /** Callback streaming jawaban AI (dipanggil dari thread native). */
    interface ChatCallback {
        fun onDelta(text: String)
        fun onDone(fullText: String)
        fun onError(message: String)

        /** Model meminta menjalankan perintah di Ubuntu (agent tool call). */
        fun onTool(command: String, output: String)
    }

    /** Callback progres unduh/pasang Ubuntu 24. */
    interface InstallCallback {
        fun onProgress(percent: Int, label: String)

        /** Output mentah apt-get saat memasang tools (dipanggil dari thread native). */
        fun onOutput(chunk: String)
        fun onError(message: String)
    }

    /** Callback output perintah terminal. */
    interface ExecCallback {
        fun onOutput(chunk: String)
    }
}
