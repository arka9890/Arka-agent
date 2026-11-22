package com.arka.agent

import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.os.PowerManager
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/**
 * Menjaga proses aplikasi tetap hidup selama chat AI berjalan.
 *
 * Tanpa service ini, saat layar dikunci atau aplikasi diminimize Android
 * memasukkan aplikasi ke Doze / standby dan membekukan jaringan serta CPU.
 * Akibatnya stream API terputus dan model seolah "berhenti di tengah".
 * Service ini:
 *  - menampilkan notifikasi persisten (foreground service), sehingga proses
 *    mendapat prioritas tinggi dan tidak dibunuh sistem;
 *  - memegang PARTIAL_WAKE_LOCK supaya CPU tidak suspend walau layar mati.
 *
 * Service hanya mengurus "hidupnya" proses. Logika chat tetap ada di
 * [MainViewModel]; service dimulai ketika chat dimulai dan dihentikan setelah
 * semua sesi selesai.
 */
class ArkaChatService : Service() {

    companion object {
        private const val CHANNEL_ID = "arka_chat"
        private const val NOTIFICATION_ID = 1001
        private const val WAKE_LOCK_TAG = "arka:chat"
        // Tugas panjang (clone + build + uji) bisa lebih dari 30 menit.
        // Service menerima onStartCommand setiap chat dimulai, jadi lock
        // diperbarui; timeout ini hanya jaring pengaman kalau release gagal.
        private const val WAKE_LOCK_TIMEOUT_MS = 2 * 60 * 60 * 1000L

        /** Mulai/menghentikan service dari UI. Aman dipanggil berulang. */
        fun start(context: Context) {
            ContextCompat.startForegroundService(
                context,
                Intent(context, ArkaChatService::class.java),
            )
        }

        fun stop(context: Context) {
            context.stopService(Intent(context, ArkaChatService::class.java))
        }
    }

    private var wakeLock: PowerManager.WakeLock? = null

    override fun onCreate() {
        super.onCreate()
        createChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // Pakai startForeground SEBELUM kerja lain: Android memberi waktu
        // beberapa detik sejak startForegroundService, kalau terlewat proses
        // dianggap melanggar dan service dihentikan.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(NOTIFICATION_ID, buildNotification(), ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(NOTIFICATION_ID, buildNotification())
        }
        acquireWakeLock()
        // START_NOT_STICKY: kalau proses tetap dibunuh (mis. di-swipe dari
        // recents), chat juga ikut hilang. Menjalankan service lagi tanpa chat
        // hanya akan meninggalkan notifikasi "sedang bekerja" selamanya.
        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        releaseWakeLock()
        super.onDestroy()
    }

    private fun createChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannelCompat.Builder(
            CHANNEL_ID,
            NotificationManagerCompat.IMPORTANCE_LOW,
        )
            .setName("Arka Agent")
            .setDescription("Menjaga proses chat AI tetap berjalan di latar belakang")
            .setSound(null, null)
            .build()
        NotificationManagerCompat.from(this).createNotificationChannel(channel)
    }

    private fun buildNotification(): android.app.Notification {
        val open = Intent(this, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        }
        val pending = PendingIntent.getActivity(
            this,
            0,
            open,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_chat)
            .setContentTitle("Arka Agent")
            .setContentText(getString(R.string.chat_background_notification))
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(pending)
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setCategory(NotificationCompat.CATEGORY_SERVICE)
            .build()
    }

    private fun acquireWakeLock() {
        if (wakeLock?.isHeld == true) return
        val manager = getSystemService(Context.POWER_SERVICE) as PowerManager
        wakeLock = manager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, WAKE_LOCK_TAG).apply {
            setReferenceCounted(false)
            acquire(WAKE_LOCK_TIMEOUT_MS)
        }
    }

    private fun releaseWakeLock() {
        wakeLock?.let { if (it.isHeld) it.release() }
        wakeLock = null
    }
}
