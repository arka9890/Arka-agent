package com.arka.agent.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Chat
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Extension
import androidx.compose.material.icons.outlined.Hub
import androidx.compose.material.icons.outlined.Settings
import androidx.compose.material.icons.outlined.Terminal
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.DisposableEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import com.arka.agent.ChatEngine
import com.arka.agent.MainViewModel
import com.arka.agent.ui.screens.ChatScreen
import com.arka.agent.ui.screens.FilesScreen
import com.arka.agent.ui.screens.ProvidersScreen
import com.arka.agent.ui.screens.SettingsScreen
import com.arka.agent.ui.screens.SkillsScreen
import com.arka.agent.ui.screens.TerminalScreen
import com.arka.agent.ui.theme.clay

/** Tab utama: Chat, Provider, File, Skill, Terminal, Setelan. */
enum class Tab(
    val label: String,
    val labelEn: String,
    val labelJa: String,
    val icon: ImageVector,
) {
    Chat("Chat", "Chat", "チャット", Icons.Outlined.Chat),
    Provider("Provider", "Provider", "プロバイダー", Icons.Outlined.Hub),
    File("File", "Files", "ファイル", Icons.Outlined.Description),
    Skill("Skill", "Skills", "スキル", Icons.Outlined.Extension),
    Terminal("Terminal", "Terminal", "ターミナル", Icons.Outlined.Terminal),
    Settings("Setelan", "Settings", "設定", Icons.Outlined.Settings),
}

@Composable
fun AppScreen(vm: MainViewModel) {
    var tab by remember { mutableStateOf(Tab.Chat) }
    val snackbar = remember { SnackbarHostState() }
    val notice by vm.notice.collectAsState()

    LaunchedEffect(notice) {
        notice?.let {
            snackbar.showSnackbar(it)
            vm.clearNotice()
        }
    }

    Scaffold(
        // Naikkan seluruh Scaffold (konten + tab bar) di atas keyboard saat
        // IME terbuka. Kalau imePadding hanya dipasang di InputBar, tab bar
        // tetap keburu keyboard dan kolom input melayang setinggi tab bar
        // di atas keyboard. Insets yang sudah dikonsumsi di sini otomatis
        // dianggap 0 oleh imePadding anak-anak di dalamnya, jadi aman.
        modifier = Modifier.imePadding(),
        containerColor = clay.background,
        snackbarHost = { SnackbarHost(snackbar) },
        bottomBar = { ClayTabBar(current = tab, onSelect = { tab = it }) },
        contentWindowInsets = WindowInsets(0),
    ) { padding ->
        // ChatEngine hanya perlu tahu sesi mana yang *terlihat* supaya
        // jawaban di tab lain / latar belakang memicu notifikasi.
        val sessionId by vm.currentSessionId.collectAsState()
        val inChat = tab == Tab.Chat
        DisposableEffect(inChat, sessionId) {
            ChatEngine.setVisible(if (inChat) sessionId else null)
            onDispose {}
        }

        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(padding)
                .windowInsetsPadding(WindowInsets.statusBars),
        ) {
            when (tab) {
                Tab.Chat -> ChatScreen(vm)
                Tab.Provider -> ProvidersScreen(vm)
                Tab.File -> FilesScreen(vm)
                Tab.Skill -> SkillsScreen(vm)
                Tab.Terminal -> TerminalScreen(vm)
                Tab.Settings -> SettingsScreen(vm)
            }
        }
    }
}

/**
 * Tab bar bergaya clay: satu "kapsul" melayang dengan item aktif yang
 * menonjol. Ikon + label (bukan hanya ikon) supaya jelas di layar kecil.
 */
@Composable
private fun ClayTabBar(current: Tab, onSelect: (Tab) -> Unit) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .background(clay.background)
            .windowInsetsPadding(WindowInsets.navigationBars)
            .padding(horizontal = 12.dp, vertical = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        Tab.entries.forEach { entry ->
            val active = entry == current
            val label = tr(entry.label, entry.labelEn, entry.labelJa)
            Box(
                modifier = Modifier
                    .weight(1f)
                    .clip(RoundedCornerShape(18.dp))
                    .background(if (active) clay.accentSoft else clay.surface)
                    // Tanpa modifier ini item tab hanya dekorasi: tidak ada
                    // klik/ripple sehingga navbar terasa mati.
                    .selectable(
                        selected = active,
                        role = Role.Tab,
                        onClick = { onSelect(entry) },
                    )
                    .padding(vertical = 8.dp),
                contentAlignment = Alignment.Center,
            ) {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Icon(
                        imageVector = entry.icon,
                        contentDescription = label,
                        tint = if (active) clay.accent else clay.muted,
                        modifier = Modifier.height(22.dp),
                    )
                    Spacer(Modifier.height(2.dp))
                    Text(
                        text = label,
                        style = MaterialTheme.typography.labelSmall,
                        color = if (active) clay.accent else clay.muted,
                    )
                }
            }
        }
    }
}
