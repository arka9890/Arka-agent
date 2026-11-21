package com.arka.agent

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.lifecycle.viewmodel.compose.viewModel
import com.arka.agent.ui.AppScreen
import com.arka.agent.ui.LocalLanguage
import com.arka.agent.ui.theme.ArkaTheme
import com.arka.agent.ui.theme.fontScaleFor

class MainActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
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
