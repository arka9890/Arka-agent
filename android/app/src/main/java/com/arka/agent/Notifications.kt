package com.arka.agent

import android.Manifest
import android.annotation.SuppressLint
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationChannelCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

/**
 * Notifikasi "pesan baru" (terpisah dari notifikasi foreground service).
 *
 * Channel-nya boleh IMPORTANCE_DEFAULT (bersuara) karena hanya dipakai
 * sesekali saat jawaban AI selesai ketika pengguna sedang tidak di tab Chat.
 * Foreground service tetap memakai channel senyap milik [ArkaChatService].
 */
object Notifications {

    private const val CHANNEL_ID = "arka_messages"
    private const val NOTIFICATION_ID = 2001

    /** Batas panjang isi notifikasi supaya tidak terpotong aneh di header. */
    private const val MAX_BODY = 180

    fun ensureChannel(context: Context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val channel = NotificationChannelCompat.Builder(
            CHANNEL_ID,
            NotificationManagerCompat.IMPORTANCE_DEFAULT,
        )
            .setName("Pesan baru")
            .setDescription("Jawaban AI saat aplikasi sedang tidak dibuka")
            .build()
        NotificationManagerCompat.from(context).createNotificationChannel(channel)
    }

    /**
     * Tampilkan notifikasi jawaban AI. Senyap kalau izin POST_NOTIFICATIONS
     * belum diberikan (Android 13+) — tidak boleh melempar SecurityException.
     */
    @SuppressLint("MissingPermission")
    fun newMessage(context: Context, sessionId: String, title: String, body: String) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        ensureChannel(context)

        // Dibuka dari notifikasi → MainActivity membuka sesi yang dimaksud.
        val open = Intent(context, MainActivity::class.java).apply {
            flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
            putExtra(MainActivity.EXTRA_SESSION_ID, sessionId)
        }
        val pending = PendingIntent.getActivity(
            context,
            sessionId.hashCode(),
            open,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        val text = body.replace(Regex("\\s+"), " ").trim().take(MAX_BODY)
        val notification = NotificationCompat.Builder(context, CHANNEL_ID)
            .setSmallIcon(R.drawable.ic_stat_chat)
            .setContentTitle(title)
            .setContentText(text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setAutoCancel(true)
            .setOnlyAlertOnce(false)
            .setContentIntent(pending)
            .setPriority(NotificationCompat.PRIORITY_DEFAULT)
            .setCategory(NotificationCompat.CATEGORY_MESSAGE)
            .build()

        runCatching {
            NotificationManagerCompat.from(context).notify(NOTIFICATION_ID + sessionId.hashCode(), notification)
        }
    }

    fun clearAll(context: Context) {
        runCatching { NotificationManagerCompat.from(context).cancelAll() }
    }
}
