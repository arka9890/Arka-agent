package com.arka.agent

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * Pemilik job chat yang **tidak terikat pada Activity/ViewModel**.
 *
 * Alasan: `viewModelScope` dibatalkan saat Activity dihancurkan permanen
 * (mis. perubahan konfigurasi yang tidak ada di `configChanges`, atau ROM
 * agresif). Kalau chat berada di scope itu, model mendadak berhenti dan sisi
 * native tidak pernah diberitahu. Di sini job, buffer stream, dan status
 * disimpan di singleton yang di-scope ke Application, jadi chat tetap jalan
 * walau UI dibuat ulang.
 */
object ChatEngine {

    /** Scope aplikasi: tidak ikut dibatalkan saat ViewModel di-clear. */
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)

    private lateinit var app: Application

    private val jobs = mutableMapOf<String, Job>()
    private val buffers = mutableMapOf<String, StringBuilder>()

    private val _running = MutableStateFlow<Set<String>>(emptySet())
    /** Id sesi yang sedang menerima jawaban. */
    val running: StateFlow<Set<String>> = _running.asStateFlow()

    private val _streaming = MutableStateFlow<Map<String, String>>(emptyMap())
    /** Teks stream per sesi, dipakai UI saat membuka lagi sesi yang jalan. */
    val streaming: StateFlow<Map<String, String>> = _streaming.asStateFlow()

    fun init(application: Application) {
        app = application
    }

    fun isRunning(sessionId: String): Boolean = jobs[sessionId]?.isActive == true

    fun buffered(sessionId: String): String = buffers[sessionId]?.toString().orEmpty()

    /**
     * Daftarkan job baru. [background] = apakah foreground service boleh
     * dinyalakan (pengguna bisa mematikannya di Setelan → Agent).
     */
    fun begin(sessionId: String, job: Job, background: Boolean) {
        // Job bisa selesai lebih dulu kalau gagal sebelum suspension pertama.
        if (!job.isActive) return
        jobs[sessionId] = job
        buffers[sessionId] = StringBuilder()
        publish()
        if (background) ArkaChatService.start(app)
    }

    /** Tambahkan potongan jawaban dari native. */
    fun onDelta(sessionId: String, delta: String) {
        buffers.getOrPut(sessionId) { StringBuilder() }.append(delta)
        publish()
    }

    /** Reset buffer untuk sesi (dipanggil setelah pesan assistant disimpan). */
    fun clearBuffer(sessionId: String) {
        buffers[sessionId] = StringBuilder()
        publish()
    }

    /**
     * Job selesai secara normal / error. [job] dipakai supaya `finally` dari
     * chat lama tidak menghapus chat baru untuk sesi yang sama (bisa terjadi
     * kalau pengguna menekan Stop lalu langsung mengirim lagi).
     */
    fun finish(sessionId: String, job: Job? = null) {
        if (job != null) {
            val current = jobs[sessionId]
            if (current != null && current !== job) return
        }
        jobs.remove(sessionId)
        buffers.remove(sessionId)
        publish()
        if (_running.value.isEmpty()) ArkaChatService.stop(app)
    }

    /**
     * Batalkan job Kotlin. Flag cancel native dipanggil terpisah dari VM
     * (lewat `repo.cancelChat`) supaya thread JNI ikut berhenti.
     */
    fun cancelJob(sessionId: String) {
        val job = jobs.remove(sessionId)
        job?.cancel()
        finish(sessionId, job)
    }

    private fun publish() {
        _running.value = jobs.filterValues { it.isActive }.keys.toSet()
        _streaming.value = buffers.mapValues { it.value.toString() }
    }
}
