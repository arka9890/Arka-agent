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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.arka.agent.BuildConfig
import com.arka.agent.MainViewModel
import com.arka.agent.ui.tr
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
    val diag by vm.diagnostic.collectAsState()
    val install by vm.installProgress.collectAsState()
    val context = LocalContext.current

    var stats by remember { mutableStateOf<SystemStats?>(null) }
    var workspaceBytes by remember { mutableStateOf(0L) }
    var showRoundsDialog by remember { mutableStateOf(false) }
    var roundsDraft by remember { mutableStateOf("40") }
    var showPromptDialog by remember { mutableStateOf(false) }
    var promptDraft by remember { mutableStateOf("") }

    // Dimuat ulang setelah rootfs selesai dipasang supaya angka ukuran tidak
    // tertinggal di 0 B.
    LaunchedEffect(distro.installed, distro.sizeBytes) {
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
            Text(tr("Setelan", "Settings", "設定"), style = MaterialTheme.typography.titleLarge)
        }

        item {
            SettingCard(tr("Tampilan", "Appearance", "外観")) {
                ChoiceRow(
                    label = tr("Tema", "Theme", "テーマ"),
                    options = listOf("system" to tr("Sistem", "System", "システム"), "dark" to tr("Gelap", "Dark", "ダーク"), "light" to tr("Terang", "Light", "ライト")),
                    selected = settings["theme"] ?: "system",
                    onSelect = { vm.updateSetting("theme", it) },
                )
                Spacer(Modifier.height(Space.sm))
                ChoiceRow(
                    label = tr("Bahasa", "Language", "言語"),
                    options = listOf("id" to tr("Indonesia", "Indonesian", "インドネシア語"), "en" to "English", "ja" to "日本語"),
                    selected = settings["language"] ?: "id",
                    onSelect = { vm.updateSetting("language", it) },
                )
                Text(
                    tr(
                        "Perubahan bahasa langsung diterapkan pada teks antarmuka.",
                        "Language changes apply immediately to the interface.",
                        "言語の変更はすぐに画面に反映されます。",
                    ),
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
                Spacer(Modifier.height(Space.sm))
                ChoiceRow(
                    label = tr("Ukuran font", "Font size", "フォントサイズ"),
                    options = listOf("small" to tr("Kecil", "Small", "小"), "medium" to tr("Sedang", "Medium", "中"), "large" to tr("Besar", "Large", "大")),
                    selected = settings["font_size"] ?: "medium",
                    onSelect = { vm.updateSetting("font_size", it) },
                )
            }
        }

        item {
            SettingCard(tr("Chat", "Chat", "チャット")) {
                ToggleRow(
                    label = tr("Gulir otomatis ke pesan terbaru", "Auto-scroll to latest message", "最新メッセージへ自動スクロール"),
                    checked = settings["auto_scroll"] != "false",
                    onChange = { vm.updateSetting("auto_scroll", it.toString()) },
                )
                // Belum ada NotificationManager di aplikasi ini: tombol
                // sengaja dinonaktifkan supaya tidak terlihat "rusak".
                ToggleRow(
                    label = tr("Notifikasi pesan baru (belum aktif)", "New message notifications (not active)", "新着通知（未実装）"),
                    checked = settings["notifications"] != "false",
                    enabled = false,
                    onChange = { vm.updateSetting("notifications", it.toString()) },
                )
            }
        }

        item {
            SettingCard(tr("Agent", "Agent", "エージェント")) {
                // Batas putaran tool: nilai ini dipakai agent loop di Rust
                // (`settings::max_tool_rounds`). Terlalu kecil membuat tugas
                // panjang (clone/build) berhenti di tengah jalan.
                val roundsValue = settings["max_tool_rounds"]?.toIntOrNull() ?: 40
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(tr("Batas putaran tool", "Max tool rounds", "ツール呼び出しの上限"), style = MaterialTheme.typography.bodyMedium)
                        Text(
                            tr(
                                "Jumlah maksimal perintah yang dijalankan model dalam satu balasan (1–200).",
                                "Maximum commands the model may run in one reply (1–200).",
                                "1回の返信でモデルが実行できるコマンドの最大数（1〜200）。",
                            ),
                            style = MaterialTheme.typography.labelSmall,
                            color = clay.muted,
                        )
                    }
                    Spacer(Modifier.width(Space.sm))
                    TextButton(onClick = { roundsDraft = roundsValue.toString(); showRoundsDialog = true }) {
                        Text("$roundsValue")
                    }
                }
                Spacer(Modifier.height(Space.sm))
                // System prompt kustom: kosong = prompt bawaan Arka Agent.
                val customPrompt = settings["system_prompt"].orEmpty()
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.SpaceBetween,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(tr("System prompt", "System prompt", "システムプロンプト"), style = MaterialTheme.typography.bodyMedium)
                        Text(
                            if (customPrompt.isBlank()) {
                                tr("Memakai prompt bawaan Arka Agent.", "Using the built-in Arka Agent prompt.", "Arka Agentの標準プロンプトを使用中。")
                            } else {
                                tr(
                                    "Kustom · ${customPrompt.length} karakter",
                                    "Custom · ${customPrompt.length} characters",
                                    "カスタム · ${customPrompt.length}文字",
                                )
                            },
                            style = MaterialTheme.typography.labelSmall,
                            color = clay.muted,
                        )
                    }
                    Spacer(Modifier.width(Space.sm))
                    TextButton(onClick = {
                        if (customPrompt.isBlank()) {
                            // Isi awal dengan prompt bawaan supaya pengguna
                            // bisa mengubah sedikit-sedikit, bukan mulai kosong.
                            vm.loadDefaultSystemPrompt { text ->
                                promptDraft = text
                                showPromptDialog = true
                            }
                        } else {
                            promptDraft = customPrompt
                            showPromptDialog = true
                        }
                    }) { Text(tr("Ubah", "Edit", "編集")) }
                }
                if (customPrompt.isNotBlank()) {
                    TextButton(onClick = { vm.updateSetting("system_prompt", "") }) {
                        Text(tr("Kembalikan ke bawaan", "Reset to default", "標準に戻す"))
                    }
                }
            }
        }

        item {
            SettingCard(tr("Workspace", "Workspace", "ワークスペース")) {
                InfoRow(tr("Path di aplikasi", "App path", "アプリ内パス"), "/root/storage")
                InfoRow(tr("Ukuran workspace", "Workspace size", "ワークスペースのサイズ"), humanSize(workspaceBytes))
                InfoRow("Rootfs Ubuntu", humanSize(distro.sizeBytes))
                InfoRow(
                    tr("Status Ubuntu", "Ubuntu status", "Ubuntuの状態"),
                    if (distro.installed) tr("terpasang", "installed", "インストール済み") else tr("belum terpasang", "not installed", "未インストール"),
                )
                Spacer(Modifier.height(Space.sm))
                Row {
                    TextButton(onClick = { vm.runDiagnostic() }) { Text(tr("Tes distro", "Test distro", "ディストリをテスト")) }
                    if (!distro.installed) {
                        Spacer(Modifier.width(Space.xs))
                        TextButton(
                            onClick = { vm.installUbuntu() },
                            enabled = install == null,
                        ) { Text(if (install == null) tr("Pasang Ubuntu 24", "Install Ubuntu 24", "Ubuntu 24をインストール") else tr("Memasang…", "Installing…", "インストール中…")) }
                    }
                }
                install?.let { progress ->
                    Text(
                        "${progress.first}% · ${progress.second}",
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.accent,
                    )
                }
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
            SettingCard(tr("Sistem", "System", "システム")) {
                stats?.let { s ->
                    InfoRow(tr("RAM terpakai", "RAM used", "使用RAM"), "${s.usedRamMb} / ${s.totalRamMb} MB")
                    InfoRow(tr("Storage bebas", "Free storage", "空きストレージ"), humanSize(s.freeStorageBytes))
                    InfoRow("CPU", s.cpuAbi)
                    InfoRow("Android", "API ${Build.VERSION.SDK_INT}")
                } ?: Text(tr("Memuat…", "Loading…", "読み込み中…"), style = MaterialTheme.typography.labelSmall, color = clay.muted)
            }
        }

        item {
            SettingCard(tr("Tentang", "About", "アプリについて")) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text("Arka Agent", style = MaterialTheme.typography.titleSmall)
                    Spacer(Modifier.width(Space.sm))
                    ClayPill("v${BuildConfig.VERSION_NAME}", clay.accent)
                }
                Spacer(Modifier.height(Space.xs))
                Text(
                    tr(
                        "AI Agent dengan workspace Ubuntu 24 di perangkat. " +
                            "Chat AI, jalankan Python/Git, kelola file — tanpa server.",
                        "AI agent with an Ubuntu 24 workspace on your device. " +
                            "Chat with AI, run Python/Git, manage files — no server needed.",
                        "端末内にUbuntu 24ワークスペースを持つAIエージェント。AIとチャットし、Python/Gitを実行し、ファイルを管理できます。サーバー不要です。",
                    ),
                    style = MaterialTheme.typography.bodySmall,
                    color = clay.muted,
                )
            }
        }
    }

    if (showRoundsDialog) {
        val parsed = roundsDraft.trim().toIntOrNull()
        val valid = parsed != null && parsed in 1..200
        AlertDialog(
            onDismissRequest = { showRoundsDialog = false },
            title = { Text(tr("Batas putaran tool", "Max tool rounds", "ツール呼び出しの上限")) },
            text = {
                Column {
                    OutlinedTextField(
                        value = roundsDraft,
                        onValueChange = { roundsDraft = it.filter(Char::isDigit).take(3) },
                        singleLine = true,
                        label = { Text(tr("Jumlah putaran", "Number of rounds", "回数")) },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Spacer(Modifier.height(Space.xs))
                    Text(
                        tr(
                            "1–200. Default 40. Naikkan untuk tugas panjang seperti clone atau build.",
                            "1–200. Default 40. Raise it for long tasks like clone or build.",
                            "1〜200。既定値は40。cloneやbuildなど長い作業では増やしてください。",
                        ),
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                    )
                }
            },
            confirmButton = {
                TextButton(
                    enabled = valid,
                    onClick = {
                        vm.updateSetting("max_tool_rounds", parsed.toString())
                        showRoundsDialog = false
                    },
                ) { Text(tr("Simpan", "Save", "保存")) }
            },
            dismissButton = {
                TextButton(onClick = { showRoundsDialog = false }) { Text(tr("Batal", "Cancel", "キャンセル")) }
            },
        )
    }

    if (showPromptDialog) {
        AlertDialog(
            onDismissRequest = { showPromptDialog = false },
            title = { Text(tr("System prompt", "System prompt", "システムプロンプト")) },
            text = {
                Column {
                    OutlinedTextField(
                        value = promptDraft,
                        onValueChange = { promptDraft = it },
                        modifier = Modifier.fillMaxWidth().heightIn(min = 180.dp, max = 320.dp),
                        textStyle = MaterialTheme.typography.bodySmall,
                    )
                    Spacer(Modifier.height(Space.xs))
                    Text(
                        tr(
                            "Kosongkan lalu simpan untuk kembali ke prompt bawaan. Berlaku untuk chat berikutnya.",
                            "Clear and save to return to the built-in prompt. Applies to the next chat.",
                            "空にして保存すると標準プロンプトに戻ります。次のチャットから適用されます。",
                        ),
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                    )
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    vm.updateSetting("system_prompt", promptDraft.trim())
                    showPromptDialog = false
                }) { Text(tr("Simpan", "Save", "保存")) }
            },
            dismissButton = {
                Row {
                    TextButton(onClick = {
                        vm.loadDefaultSystemPrompt { promptDraft = it }
                    }) { Text(tr("Muat bawaan", "Load default", "標準を読込")) }
                    TextButton(onClick = { showPromptDialog = false }) { Text(tr("Batal", "Cancel", "キャンセル")) }
                }
            },
        )
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
private fun ToggleRow(
    label: String,
    checked: Boolean,
    enabled: Boolean = true,
    onChange: (Boolean) -> Unit,
) {
    Row(
        Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.SpaceBetween,
    ) {
        Text(
            label,
            style = MaterialTheme.typography.bodyMedium,
            color = if (enabled) clay.onSurface else clay.muted,
        )
        Switch(checked = checked, onCheckedChange = onChange, enabled = enabled)
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
