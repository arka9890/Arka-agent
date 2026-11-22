package com.arka.agent

import android.Manifest
import android.content.pm.PackageManager
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.viewmodel.compose.viewModel
import com.arka.agent.ui.AppScreen
import com.arka.agent.ui.LocalLanguage
import com.arka.agent.ui.theme.ArkaTheme
import com.arka.agent.ui.theme.fontScaleFor

class MainActivity : ComponentActivity() {

    companion object {
        /** Intent extra dari notifikasi: sesi yang harus dibuka. */
        const val EXTRA_SESSION_ID = "com.arka.agent.extra.SESSION_ID"
    }

    /**
     * Id sesi yang diminta notifikasi. State Compose (bukan field biasa)
     * supaya `onNewIntent` saat aplikasi sudah terbuka langsung membuka sesi.
     */
    private var pendingSessionId by mutableStateOf<String?>(null)

    /**
     * Notifikasi foreground service diperlukan supaya pengguna tahu chat
     * masih berjalan saat layar dikunci. Tanpa izin ini (Android 13+), chat
     * tetap jalan tetapi notifikasinya hanya muncul di task manager.
     */
    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pendingSessionId = intent?.getStringExtra(EXTRA_SESSION_ID)
        enableEdgeToEdge()
        Notifications.ensureChannel(this)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        setContent {
            val vm: MainViewModel = viewModel()
            // Notifikasi diketuk → langsung buka sesi yang menjawab.
            LaunchedEffect(pendingSessionId) {
                pendingSessionId?.let { id ->
                    pendingSessionId = null
                    vm.selectSession(id)
                }
            }
            val prefs by vm.settings.collectAsState()
            val dark = when (prefs["theme"]) {
                "light" -> false
                "dark" -> true
                else -> androidx.compose.foundation.isSystemInDarkTheme()
            }
            CompositionLocalProvider(LocalLanguage provides (prefs["language"] ?: "id")) {
                ArkaTheme(darkTheme = dark, fontScale = fontScaleFor(prefs["font_size"])) {
                    AppScreen(vm)
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        // Aplikasi sudah terbuka (singleTop) lalu notifikasi diketuk: state
        // di atas berubah, LaunchedEffect memilih sesinya.
        setIntent(intent)
        intent.getStringExtra(EXTRA_SESSION_ID)?.let { pendingSessionId = it }
    }
}
