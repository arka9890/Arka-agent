package com.arka.agent.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.awaitPointerEventScope
import androidx.compose.ui.input.pointer.awaitPointerEvent
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.arka.agent.MainViewModel
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.tr
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay

/**
 * Tab Desktop: akses Ubuntu grafis lewat VNC langsung dari aplikasi.
 *
 * Tidak ada prompt AI yang diperlukan untuk menyalakan, melihat, atau
 * mematikan desktop. Klien RFB ada di [com.arka.agent.VncClient].
 */
@Composable
fun DesktopScreen(vm: MainViewModel) {
    val status by vm.desktopStatus.collectAsState()
    val distro by vm.distro.collectAsState()
    val session by vm.desktopSession.collectAsState()
    val busy by vm.desktopBusy.collectAsState()
    val log by vm.desktopInstallLog.collectAsState()
    val frame by vm.desktopFrame.collectAsState()
    val state by vm.desktopState.collectAsState()
    var typed by remember { mutableStateOf("") }

    // Sambung ulang hanya kalau sesinya berubah. Pindah tab tidak memutus
    // koneksi karena klien dimiliki ViewModel, bukan Composable ini.
    LaunchedEffect(session?.port, session?.width) {
        if (session != null && status.host.isNotBlank()) {
            vm.connectDesktop(status.host, status.port)
        }
    }
    DisposableEffect(Unit) { onDispose { /* klien sengaja tetap hidup */ } }

    LazyColumn(
        modifier = Modifier.fillMaxSize().padding(horizontal = Space.md),
        verticalArrangement = Arrangement.spacedBy(Space.md),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(vertical = Space.md),
    ) {
        item {
            Text(tr("Desktop", "Desktop", "デスクトップ"), style = MaterialTheme.typography.titleLarge)
            Text(
                tr(
                    "Akses Ubuntu 24.04 grafis langsung dari aplikasi — tanpa lewat model AI.",
                    "Direct graphical access to Ubuntu 24.04 — no AI prompt needed.",
                    "AI を使わず Ubuntu 24.04 のデスクトップに直接アクセス。",
                ),
                style = MaterialTheme.typography.bodySmall,
                color = clay.muted,
            )
        }

        item {
            ClayCard {
                Text(tr("Status Ubuntu", "Ubuntu status", "Ubuntu の状態"), style = MaterialTheme.typography.titleMedium)
                Spacer(Modifier.height(Space.sm))
                StatusRow("Ubuntu 24", if (status.ubuntu) "siap" else "belum dipasang", status.ubuntu)
                StatusRow("Server VNC", if (status.vncInstalled) "terpasang" else "belum", status.vncInstalled)
                StatusRow("Desktop XFCE", if (status.xfceInstalled) "terpasang" else "belum", status.xfceInstalled)
                StatusRow("Sesi VNC", if (status.running) "berjalan" else "mati", status.running)
                if (!status.ubuntu) {
                    Spacer(Modifier.height(Space.sm))
                    Text(
                        distro.message.ifBlank { "Pasang Ubuntu dulu lewat tab Setelan → Sistem." },
                        style = MaterialTheme.typography.bodySmall,
                        color = clay.muted,
                    )
                }
                Spacer(Modifier.height(Space.sm))
                Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    OutlinedButton(onClick = { vm.refreshDesktop(); vm.refreshDistro() }, enabled = !busy) {
                        Text(tr("Segarkan", "Refresh", "更新"))
                    }
                    OutlinedButton(
                        onClick = { vm.installDesktop() },
                        enabled = !busy && status.ubuntu && (!status.vncInstalled || !status.xfceInstalled),
                    ) {
                        Text(if (busy) "Memasang…" else tr("Pasang desktop", "Install desktop", "導入"))
                    }
                }
            }
        }

        item {
            ClayCard {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(tr("Kontrol desktop", "Desktop control", "制御"), style = MaterialTheme.typography.titleMedium)
                    Spacer(Modifier.width(Space.sm))
                    if (busy) {
                        Text("…", color = clay.muted)
                    }
                }
                Spacer(Modifier.height(Space.sm))
                Text(
                    tr(
                        "VNC hanya mendengar di 127.0.0.1:5901 tanpa password (tidak bisa diakses dari jaringan lain).",
                        "VNC listens only on 127.0.0.1:5901 without a password (not reachable from other devices).",
                        "VNC は 127.0.0.1:5901 のみで待機し、パスワードはありません。",
                    ),
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
                Spacer(Modifier.height(Space.sm))
                Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                    if (session == null) {
                        Button(
                            onClick = { vm.startDesktop() },
                            enabled = !busy && status.ubuntu && status.vncInstalled && status.xfceInstalled,
                        ) { Text(tr("Nyalakan desktop", "Start desktop", "起動")) }
                    } else {
                        Button(onClick = { vm.stopDesktop() }) { Text(tr("Matikan", "Stop", "停止")) }
                        OutlinedButton(onClick = { vm.connectDesktop(status.host, status.port) }) {
                            Text(tr("Sambung ulang", "Reconnect", "再接続"))
                        }
                    }
                }
                if (log.isNotBlank()) {
                    Spacer(Modifier.height(Space.sm))
                    Text(
                        log.takeLast(1_500),
                        style = MaterialTheme.typography.labelSmall.copy(fontFamily = FontFamily.Monospace),
                        color = clay.muted,
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(120.dp)
                            .verticalScroll(rememberScrollState()),
                    )
                }
            }
        }

        if (session != null) {
            item {
                ClayCard(contentPadding = 10.dp) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(
                            tr("Layar Ubuntu", "Ubuntu screen", "Ubuntu 画面"),
                            style = MaterialTheme.typography.titleMedium,
                        )
                        Spacer(Modifier.width(Space.sm))
                        Text(state, style = MaterialTheme.typography.labelSmall, color = clay.muted)
                    }
                    Spacer(Modifier.height(Space.sm))
                    val bitmap = frame?.bitmap
                    Box(
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(320.dp)
                            .clip(RoundedCornerShape(14.dp))
                            .background(Color(0xFF10151B))
                            .border(1.dp, clay.shadowLight.copy(alpha = 0.2f), RoundedCornerShape(14.dp))
                            .then(
                                if (bitmap != null) {
                                    Modifier.pointerInput(bitmap) {
                                        awaitPointerEventScope {
                                            while (true) {
                                                val event = awaitPointerEvent()
                                                val change = event.changes.lastOrNull() ?: continue
                                                val bmp = frame?.bitmap ?: continue
                                                val sw = size.width.toFloat()
                                                val sh = size.height.toFloat()
                                                val scale = minOf(sw / bmp.width, sh / bmp.height)
                                                val ox = (sw - bmp.width * scale) / 2f
                                                val oy = (sh - bmp.height * scale) / 2f
                                                val px = ((change.position.x - ox) / scale).toInt()
                                                val py = ((change.position.y - oy) / scale).toInt()
                                                when (event.type) {
                                                    PointerEventType.Press -> vm.desktopPointer(px, py, 1)
                                                    PointerEventType.Move -> if (change.pressed) vm.desktopPointer(px, py, 1)
                                                    PointerEventType.Release -> vm.desktopPointer(px, py, 0)
                                                    else -> Unit
                                                }
                                                change.consume()
                                            }
                                        }
                                    }
                                } else Modifier
                            ),
                        contentAlignment = Alignment.Center,
                    ) {
                        if (bitmap == null) {
                            Text(
                                tr("Menunggu frame desktop…", "Waiting for desktop frame…", "待機中…"),
                                color = clay.muted,
                                style = MaterialTheme.typography.bodySmall,
                            )
                        } else {
                            Canvas(modifier = Modifier.fillMaxSize()) {
                                val image = bitmap.asImageBitmap()
                                val scale = minOf(size.width / image.width, size.height / image.height)
                                val w = image.width * scale
                                val h = image.height * scale
                                drawImage(
                                    image,
                                    dstOffsetX = (size.width - w) / 2f,
                                    dstOffsetY = (size.height - h) / 2f,
                                    dstSize = androidx.compose.ui.geometry.Size(w, h),
                                )
                            }
                        }
                    }
                    Spacer(Modifier.height(Space.sm))
                    Row(
                        modifier = Modifier.horizontalScroll(rememberScrollState()),
                        horizontalArrangement = Arrangement.spacedBy(6.dp),
                    ) {
                        KeyButton("Esc", 0xFF1B, vm)
                        KeyButton("Tab", 0xFF09, vm)
                        KeyButton("⌫", 0xFF08, vm)
                        KeyButton("↵", 0xFF0D, vm)
                        KeyButton("←", 0xFF51, vm)
                        KeyButton("→", 0xFF53, vm)
                        KeyButton("↑", 0xFF52, vm)
                        KeyButton("↓", 0xFF54, vm)
                    }
                    Spacer(Modifier.height(Space.sm))
                    OutlinedTextField(
                        value = typed,
                        onValueChange = { typed = it },
                        modifier = Modifier.fillMaxWidth(),
                        label = { Text(tr("Kirim teks ke desktop", "Send text to desktop", "テキストを送信")) },
                        singleLine = true,
                    )
                    Spacer(Modifier.height(Space.sm))
                    Row(horizontalArrangement = Arrangement.spacedBy(Space.sm)) {
                        Button(
                            onClick = {
                                if (typed.isNotEmpty()) {
                                    vm.desktopType(typed)
                                    typed = ""
                                }
                            },
                        ) { Text(tr("Kirim", "Send", "送信")) }
                        OutlinedButton(onClick = { vm.desktopKey(0xFF0D) }) { Text("Enter") }
                    }
                }
            }
        }
    }
}

@Composable
private fun StatusRow(label: String, value: String, ok: Boolean) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium)
        Text(value, style = MaterialTheme.typography.labelSmall, color = if (ok) clay.accent else clay.muted)
    }
}

@Composable
private fun KeyButton(label: String, keysym: Int, vm: MainViewModel) {
    OutlinedButton(
        onClick = { vm.desktopKey(keysym) },
        modifier = Modifier.size(width = 52.dp, height = 40.dp),
        contentPadding = androidx.compose.foundation.layout.PaddingValues(0.dp),
    ) { Text(label, style = MaterialTheme.typography.labelSmall) }
}
