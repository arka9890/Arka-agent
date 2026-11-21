package com.arka.agent.ui.screens

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import android.os.StatFs
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.arka.agent.BuildConfig
import com.arka.agent.MainViewModel
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.components.ClayPill
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Tab Settings: tema, bahasa, font, workspace, statistik, tentang. */
@Composable
fun SettingsScreen(vm: MainViewModel) {
    val settings by vm.settings.collectAsState()
    val distro by vm.distro.collectAsState()
    val context = LocalContext.current

    var stats by remember { mutableStateOf<SystemStats?>(null) }
    var diag by remember { mutableStateOf<String?>(null) }
    var workspaceBytes by remember { mutableStateOf(0L) }

    LaunchedEffect(Unit) {
        stats = withContext(Dispatchers.IO) { readStats(context) }
        workspaceBytes = withContext(Dispatchers.IO) {
            runCatching { vm.workspaceSizeNow() }.getOrDefault(0L)
        }
    }

    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(horizontal = Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.md),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = Space.md),
    ) {
        item {
            Text("Setelan", style = MaterialTheme.typography.titleLarge)
        }

        item {
            SettingCard("Tampilan") {
                ChoiceRow(
                    label = "Tema",
                    options = listOf("system" to "Sistem", "dark" to "Gelap", "light" to "Terang"),
                    selected = settings["theme"] ?: "system",
                    onSelect = { vm.updateSetting("theme", it) },
                )
                Spacer(Modifier.height(Space.sm))
                ChoiceRow(
                    label = "Bahasa",
                    options = listOf("id" to "Indonesia", "en" to "English", "ja" to "日本語"),
                    selected = settings["language"] ?: "id",
                    onSelect = { vm.updateSetting("language", it) },
                )
                Spacer(Modifier.height(Space.sm))
                ChoiceRow(
                    label = "Ukuran font",
                    options = listOf("small" to "Kecil", "medium" to "Sedang", "large" to "Besar"),
                    selected = settings["font_size"] ?: "medium",
                    onSelect = { vm.updateSetting("font_size", it) },
                )
            }
        }

        item {
            SettingCard("Chat") {
                ToggleRow(
                    label = "Gulir otomatis ke pesan terbaru",
                    checked = settings["auto_scroll"] != "false",
                    onChange = { vm.updateSetting("auto_scroll", it.toString()) },
                )
                ToggleRow(
                    label = "Notifikasi pesan baru",
                    checked = settings["notifications"] != "false",
                    onChange = { vm.updateSetting("notifications", it.toString()) },
                )
            }
        }

        item {
            SettingCard("Workspace") {
                InfoRow("Path di aplikasi", "/root/storage")
                InfoRow("Ukuran workspace", humanSize(workspaceBytes))
                InfoRow("Rootfs Ubuntu", humanSize(distro.sizeBytes))
                Spacer(Modifier.height(Space.sm))
                TextButton(onClick = { vm.runDiagnostic() }) { Text("Tes distro") }
                diag?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                        modifier = Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(10.dp))
                            .background(clay.background)
                            .padding(10.dp),
                    )
                }
            }
        }

        item {
            SettingCard("Sistem") {
                stats?.let { s ->
                    InfoRow("RAM terpakai", "${s.usedRamMb} / ${s.totalRamMb} MB")
                    InfoRow("Storage bebas", humanSize(s.freeStorageBytes))
                    InfoRow("CPU", s.cpuAbi)
                    InfoRow("Android", "API ${Build.VERSION.SDK_INT}")
                } ?: Text("Memuat…", style = MaterialTheme.typography.labelSmall, color = clay.muted)
            }
        }

        item {
            SettingCard("Tentang") {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Arka Agent", style = MaterialTheme.typography.titleSmall)
                    Spacer(Modifier.width(Space.sm))
                    ClayPill("v${BuildConfig.VERSION_NAME}", clay.accent)
                }
                Spacer(Modifier.height(Space.xs))
                Text(
                    "AI Agent dengan workspace Ubuntu 24 di perangkat. " +
                        "Chat AI, jalankan Python/Git, kelola file — tanpa server.",
                    style = MaterialTheme.typography.bodySmall,
                    color = clay.muted,
                )
            }
        }
    }
}

private data class SystemStats(
    val totalRamMb: Long,
    val usedRamMb: Long,
    val freeStorageBytes: Long,
    val cpuAbi: String,
)

private fun readStats(context: Context): SystemStats {
    val am = context.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager
    val info = ActivityManager.MemoryInfo()
    am.getMemoryInfo(info)
    val stat = StatFs(context.filesDir.absolutePath)
    return SystemStats(
        totalRamMb = info.totalMem / 1024 / 1024,
        usedRamMb = (info.totalMem - info.availMem) / 1024 / 1024,
        freeStorageBytes = stat.availableBytes,
        cpuAbi = Build.SUPPORTED_ABIS.firstOrNull() ?: "—",
    )
}

@Composable
private fun SettingCard(title: String, content: @Composable () -> Unit) {
    ClayCard(Modifier.fillMaxWidth()) {
        Text(title, style = MaterialTheme.typography.titleSmall, color = clay.accent)
        Spacer(Modifier.height(Space.sm))
        content()
    }
}

@Composable
private fun ChoiceRow(
    label: String,
    options: List<Pair<String, String>>,
    selected: String,
    onSelect: (String) -> Unit,
) {
    Column {
        Text(label, style = MaterialTheme.typography.bodyMedium)
        Spacer(Modifier.height(Space.xs))
        Row(horizontalArrangement = Arrangement.spacedBy(Space.xs)) {
            options.forEach { (id, text) ->
                FilterChip(
                    selected = selected == id,
                    onClick = { onSelect(id) },
                    label = { Text(text, style = MaterialTheme.typography.labelSmall) },
                )
            }
        }
    }
}

@Composable
private fun ToggleRow(label: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium)
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

@Composable
private fun InfoRow(label: String, value: String) {
    Row(
        Modifier.fillMaxWidth().padding(vertical = 2.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(label, style = MaterialTheme.typography.bodySmall, color = clay.muted)
        Text(value, style = MaterialTheme.typography.bodySmall)
    }
}
