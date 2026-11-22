package com.arka.agent

import android.annotation.SuppressLint
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings

/**
 * Pengecualian dari optimasi baterai (Doze / App Standby / OEM optimizer).
 *
 * Foreground service + wake lock sudah cukup di Android "stock", tetapi ROM
 * Xiaomi/OPPO/Vivo sering tetap membekukan jaringan saat layar mati. Satu-
 * satunya cara resmi adalah meminta pengguna mengecualikan aplikasi lewat
 * dialog sistem [requestIntent].
 */
object BatteryOptimization {

    /** true kalau aplikasi sudah dikecualikan (atau Android < 6, tak ada Doze). */
    fun isIgnoring(context: Context): Boolean {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.M) return true
        val manager = context.getSystemService(Context.POWER_SERVICE) as android.os.PowerManager
        return manager.isIgnoringBatteryOptimizations(context.packageName)
    }

    /** Dialog sistem: "Izinkan Arka Agent mengabaikan optimasi baterai?" */
    @SuppressLint("BatteryLife")
    fun requestIntent(context: Context): Intent =
        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS)
            .setData(Uri.parse("package:${context.packageName}"))

    /** Halaman daftar aplikasi yang dikecualikan (fallback kalau dialog gagal). */
    fun settingsIntent(): Intent = Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)

    /** Halaman info aplikasi, berguna untuk mengaktifkan Auto-start di ROM OEM. */
    fun appDetailsIntent(context: Context): Intent =
        Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS)
            .setData(Uri.parse("package:${context.packageName}"))
}
