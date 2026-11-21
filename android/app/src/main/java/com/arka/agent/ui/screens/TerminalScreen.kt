package com.arka.agent.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.History
import androidx.compose.material.icons.outlined.PlayArrow
import androidx.compose.material.icons.outlined.Stop
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.arka.agent.MainViewModel
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.components.ClayPill
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay

/**
 * Tab Terminal.
 *
 * Selama Ubuntu belum terpasang, layar ini menampilkan tombol unduh + progres
 * (PRD 4.2: "Auto-Download Rootfs ... saat pertama aplikasi dibuka"), bukan
 * terminal kosong yang membingungkan.
 */
@Composable
fun TerminalScreen(vm: MainViewModel) {
    val distro by vm.distro.collectAsState()
    val lines by vm.terminalLines.collectAsState()
    val running by vm.terminalRunning.collectAsState()
    val progress by vm.installProgress.collectAsState()
    val history by vm.terminalHistory.collectAsState()

    var command by remember { mutableStateOf("") }
    var showHistory by remember { mutableStateOf(false) }
    val listState = rememberLazyListState()

    LaunchedEffect(lines.size) {
        if (lines.isNotEmpty()) listState.animateScrollToItem(lines.lastIndex)
    }

    Column(Modifier.fillMaxSize().padding(horizontal = Space.md).imePadding()) {
        Row(
            Modifier.fillMaxWidth().padding(vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text("Terminal", style = MaterialTheme.typography.titleLarge)
                Text(
                    if (distro.installed) "Ubuntu 24 · /root/storage" else "Ubuntu 24 belum terpasang",
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
            }
            ClayPill(
                text = if (distro.installed) "siap" else "belum siap",
                color = if (distro.installed) clay.success else clay.danger,
            )
            Box {
                IconButton(onClick = { showHistory = true }) {
                    Icon(Icons.Outlined.History, "Riwayat", tint = clay.muted)
                }
                DropdownMenu(expanded = showHistory, onDismissRequest = { showHistory = false }) {
                    if (history.isEmpty()) {
                        DropdownMenuItem(text = { Text("Belum ada riwayat") }, onClick = {})
                    }
                    history.reversed().take(20).forEach { cmd ->
                        DropdownMenuItem(
                            text = { Text(cmd, style = MaterialTheme.typography.bodySmall, maxLines = 1) },
                            onClick = {
                                command = cmd
                                showHistory = false
                            },
                        )
                    }
                }
            }
            IconButton(onClick = { vm.clearTerminal() }) {
                Icon(Icons.Outlined.Delete, "Bersihkan", tint = clay.muted)
            }
        }

        val install = progress
        if (install != null) {
            ClayCard(Modifier.fillMaxWidth()) {
                Text("Menyiapkan Ubuntu 24", style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.height(Space.xs))
                Text(
                    install.second,
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
                Spacer(Modifier.height(Space.sm))
                LinearProgressIndicator(
                    progress = { install.first / 100f },
                    modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(50)),
                    color = clay.accent,
                )
                Spacer(Modifier.height(Space.sm))
                TextButton(onClick = { vm.cancelInstall() }) {
                    Text("Batalkan", color = clay.danger)
                }
            }
        } else if (!distro.installed) {
            ClayCard(Modifier.fillMaxWidth()) {
                Text("Pasang Ubuntu 24", style = MaterialTheme.typography.titleSmall)
                Spacer(Modifier.height(Space.xs))
                Text(
                    "Unduh rootfs resmi Ubuntu (±30 MB) lalu pasang Python 3, Git, curl, " +
                        "wget, dan nano secara otomatis. Butuh koneksi internet.",
                    style = MaterialTheme.typography.bodySmall,
                    color = clay.muted,
                )
                Spacer(Modifier.height(Space.sm))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    IconButton(
                        onClick = { vm.installUbuntu() },
                        modifier = Modifier
                            .size(44.dp)
                            .clip(RoundedCornerShape(14.dp))
                            .background(clay.accent),
                    ) {
                        Icon(Icons.Outlined.Download, "Pasang", tint = androidx.compose.ui.graphics.Color.White)
                    }
                    Spacer(Modifier.width(Space.sm))
                    Text("Pasang sekarang", style = MaterialTheme.typography.bodyMedium)
                }
                if (!distro.prootAvailable) {
                    Spacer(Modifier.height(Space.xs))
                    Text(
                        "Catatan: APK ini dibangun tanpa binary proot, jadi perintah tidak " +
                            "akan bisa dijalankan.",
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.danger,
                    )
                }
            }
        }

        Box(
            Modifier
                .weight(1f)
                .fillMaxWidth()
                .padding(top = Space.sm)
                .clip(RoundedCornerShape(16.dp))
                .background(clay.surface)
                .padding(Space.sm),
        ) {
            LazyColumn(state = listState) {
                itemsIndexed(lines) { _, line ->
                    Text(
                        text = line,
                        style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
                        color = if (line.startsWith("$ ")) clay.accent else clay.onSurface,
                    )
                }
                if (lines.isEmpty()) {
                    item {
                        Text(
                            if (distro.installed) {
                                "Ketik perintah, mis. python3 --version"
                            } else {
                                "Pasang Ubuntu 24 dulu untuk membuka terminal."
                            },
                            style = MaterialTheme.typography.labelSmall,
                            color = clay.muted,
                        )
                    }
                }
            }
        }

        Row(
            Modifier.fillMaxWidth().padding(vertical = Space.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            OutlinedTextField(
                value = command,
                onValueChange = { command = it },
                modifier = Modifier.weight(1f),
                placeholder = { Text("perintah…", color = clay.muted) },
                singleLine = true,
                shape = RoundedCornerShape(16.dp),
                textStyle = MaterialTheme.typography.bodyMedium.copy(fontFamily = FontFamily.Monospace),
                keyboardOptions = KeyboardOptions(
                    keyboardType = KeyboardType.Ascii,
                    imeAction = ImeAction.Send,
                ),
                keyboardActions = KeyboardActions(onSend = {
                    when {
                        running -> Unit
                        !distro.installed -> vm.notify("Pasang Ubuntu 24 dulu.")
                        else -> {
                            vm.runCommand(command)
                            command = ""
                        }
                    }
                }),
            )
            Spacer(Modifier.width(Space.sm))
            IconButton(
                onClick = {
                    when {
                        running -> vm.stopCommand()
                        !distro.installed -> vm.notify("Pasang Ubuntu 24 dulu.")
                        else -> {
                            vm.runCommand(command)
                            command = ""
                        }
                    }
                },
                modifier = Modifier
                    .size(46.dp)
                    .clip(RoundedCornerShape(16.dp))
                    .background(if (running) clay.danger else clay.accent),
            ) {
                Icon(
                    imageVector = if (running) Icons.Outlined.Stop else Icons.Outlined.PlayArrow,
                    contentDescription = if (running) "Hentikan" else "Jalankan",
                    tint = androidx.compose.ui.graphics.Color.White,
                )
            }
        }
    }
}
