package com.arka.agent

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.lifecycle.viewmodel.compose.viewModel
import com.arka.agent.ui.AppScreen
import com.arka.agent.ui.LocalLanguage
import com.arka.agent.ui.theme.ArkaTheme
import com.arka.agent.ui.theme.fontScaleFor

class MainActivity : ComponentActivity() {

    /**
     * Notifikasi foreground service diperlukan supaya pengguna tahu chat
     * masih berjalan saat layar dikunci. Tanpa izin ini (Android 13+), chat
     * tetap jalan tetapi notifikasinya hanya muncul di task manager.
     */
    private val requestNotificationPermission =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) !=
            PackageManager.PERMISSION_GRANTED
        ) {
            requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
        setContent {
            val vm: MainViewModel = viewModel()
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
}
