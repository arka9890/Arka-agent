package com.arka.agent.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Analytics
import androidx.compose.material.icons.outlined.CheckCircle
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp

import com.arka.agent.MainViewModel
import com.arka.agent.ui.LocalLanguage
import com.arka.agent.ui.tr
import com.arka.agent.ui.trFor
import com.arka.agent.core.Provider
import com.arka.agent.core.ProviderStats
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.components.ClayPill
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay

/**
 * Tab Provider: daftar provider + form tambah/ubah.
 *
 * API key ditampilkan sebagai pill "terisi"/"kosong" di daftar, bukan nilai
 * aslinya — layar ini sering terlihat di tempat umum.
 */
/** Ringkas angka besar: 1234567 -> "1.2M". */
private fun formatTokens(n: Long): String = when {
    n >= 1_000_000 -> String.format("%.1fM", n / 1_000_000.0)
    n >= 1_000 -> String.format("%.1fK", n / 1_000.0)
    else -> n.toString()
}

@Composable
fun ProvidersScreen(vm: MainViewModel) {
    val providers by vm.providers.collectAsState()
    // Statistik diperbarui setiap kali masuk layar ini dan setelah chat selesai,
    // karena nilainya berubah di sisi Rust saat pengguna memakai AI.
    LaunchedEffect(Unit) { vm.refreshProviderStats() }
    var editing by remember { mutableStateOf<Provider?>(null) }
    var deleting by remember { mutableStateOf<Provider?>(null) }
    val stats by vm.providerStats.collectAsState()

    Column(Modifier.fillMaxSize().padding(horizontal = Space.md)) {
        Row(
            Modifier.fillMaxWidth().padding(vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column {
                Text(tr("Provider AI", "AI Providers", "AIプロバイダー"), style = MaterialTheme.typography.titleLarge)
                Text(
                    tr("API key disimpan lokal di perangkat", "API keys are stored locally on this device", "APIキーは端末内に保存されます"),
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
            }
            Button(
                onClick = { editing = Provider() },
                shape = RoundedCornerShape(16.dp),
                colors = ButtonDefaults.buttonColors(containerColor = clay.accent),
            ) {
                Icon(Icons.Outlined.Add, null, Modifier.size(18.dp))
                Spacer(Modifier.width(6.dp))
                Text(tr("Tambah", "Add", "追加"))
            }
        }

        LazyColumn(
            verticalArrangement = Arrangement.spacedBy(Space.sm),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = Space.xl),
        ) {
            items(providers, key = { it.id }) { provider ->
                ClayCard(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clickable { editing = provider },
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(
                                    provider.name,
                                    style = MaterialTheme.typography.titleSmall,
                                )
                                if (provider.isDefault) {
                                    Spacer(Modifier.width(6.dp))
                                    ClayPill(tr("default", "default", "デフォルト"), clay.success)
                                }
                            }
                            Spacer(Modifier.height(2.dp))
                            Text(
                                "${provider.kind} · ${provider.model.ifBlank { tr("model belum dipilih", "no model selected", "モデル未選択") }}",
                                style = MaterialTheme.typography.labelSmall,
                                color = clay.muted,
                            )
                            Text(
                                provider.baseUrl,
                                style = MaterialTheme.typography.labelSmall,
                                color = clay.muted,
                                maxLines = 1,
                            )
                            // Stats
                            val pStats = stats.find { it.id == provider.id }
                            if (pStats != null && (pStats.total_tokens > 0 || pStats.total_requests > 0)) {
                                Spacer(Modifier.height(4.dp))
                                Row(verticalAlignment = Alignment.CenterVertically) {
                                    Icon(
                                        Icons.Outlined.Analytics,
                                        tr("Stats", "Stats", "統計"),
                                        tint = clay.muted,
                                        modifier = Modifier.size(14.dp),
                                    )
                                    Spacer(Modifier.width(4.dp))
                                    Text(
                                        "${formatTokens(pStats.total_tokens)} tokens · ${pStats.total_requests} req",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = clay.muted,
                                    )
                                }
                            }
                        }
                        Column(horizontalAlignment = Alignment.End) {
                            ClayPill(
                                text = if (provider.apiKey.isBlank()) tr("key kosong", "no key", "キーなし") else tr("key terisi", "key set", "キー設定済み"),
                                color = if (provider.apiKey.isBlank()) clay.danger else clay.success,
                            )
                            Row {
                                if (!provider.isDefault) {
                                    IconButton(
                                        onClick = { vm.setDefaultProvider(provider.id) },
                                        modifier = Modifier.size(36.dp),
                                    ) {
                                        Icon(
                                            Icons.Outlined.CheckCircle,
                                            tr("Jadikan default", "Set as default", "デフォルトにする"),
                                            tint = clay.muted,
                                            modifier = Modifier.size(18.dp),
                                        )
                                    }
                                }
                                IconButton(
                                    onClick = { editing = provider },
                                    modifier = Modifier.size(36.dp),
                                ) {
                                    Icon(
                                        Icons.Outlined.Edit,
                                        tr("Ubah", "Edit", "編集"),
                                        tint = clay.muted,
                                        modifier = Modifier.size(18.dp),
                                    )
                                }
                                IconButton(
                                    onClick = { deleting = provider },
                                    modifier = Modifier.size(36.dp),
                                ) {
                                    Icon(
                                        Icons.Outlined.Delete,
                                        tr("Hapus", "Delete", "削除"),
                                        tint = clay.muted,
                                        modifier = Modifier.size(18.dp),
                                    )
                                }
                            }
                        }
                    }
                }
            }
            if (providers.isEmpty()) {
                item {
                    Text(
                        tr(
                            "Belum ada provider. Tekan Tambah untuk memakai OpenAI, Anthropic, " +
                                "atau endpoint yang kompatibel.",
                            "No providers yet. Tap Add to use OpenAI, Anthropic, or a compatible endpoint.",
                            "プロバイダーがありません。「追加」を押してOpenAI、Anthropic、または互換エンドポイントを設定してください。",
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = clay.muted,
                        modifier = Modifier.padding(Space.md),
                    )
                }
            }
        }
    }

    editing?.let { provider ->
        ProviderDialog(
            vm = vm,
            initial = provider,
            onDismiss = { editing = null },
        )
    }

    deleting?.let { provider ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("${tr("Hapus", "Delete", "削除")} ${provider.name}?") },
            text = { Text(tr("API key provider ini ikut terhapus.", "This provider's API key will also be deleted.", "このプロバイダーのAPIキーも削除されます。")) },
            confirmButton = {
                TextButton(onClick = {
                    vm.deleteProvider(provider.id)
                    deleting = null
                }) { Text(tr("Hapus", "Delete", "削除"), color = clay.danger) }
            },
            dismissButton = {
                TextButton(onClick = { deleting = null }) { Text(tr("Batal", "Cancel", "キャンセル")) }
            },
        )
    }
}

@Composable
private fun ProviderDialog(vm: MainViewModel, initial: Provider, onDismiss: () -> Unit) {
    var name by remember { mutableStateOf(initial.name) }
    var kind by remember { mutableStateOf(initial.kind.ifBlank { "openai" }) }
    var baseUrl by remember { mutableStateOf(initial.baseUrl) }
    var apiKey by remember { mutableStateOf(initial.apiKey) }
    var model by remember { mutableStateOf(initial.model) }
    var isDefault by remember { mutableStateOf(initial.isDefault) }
    var testResult by remember { mutableStateOf<String?>(null) }
    var models by remember { mutableStateOf<List<String>>(emptyList()) }
    var refreshingModels by remember { mutableStateOf(false) }
    // Boleh pilih banyak model: yang pertama/paling atas jadi model utama,
    // sisanya tersedia di dropdown chat sebagai alternatif.
    var selectedModels by remember {
        mutableStateOf((initial.models + listOfNotNull(initial.model.takeIf { it.isNotBlank() })).toSet())
    }
    var manualModel by remember { mutableStateOf("") }
    val lang = LocalLanguage.current

    // Provider yang sudah tersimpan: ambil daftar modelnya sekali saat dialog
    // dibuka supaya pengguna bisa memilih tanpa mengetik nama model.
    LaunchedEffect(initial.id) {
        if (initial.id.isNotBlank()) {
            vm.loadModels(initial.id) { models = it }
        }
    }

    val allModels = (models + selectedModels.toList() + listOfNotNull(model.takeIf { it.isNotBlank() }))
        .distinct()
        .sorted()

    val draft = initial.copy(
        name = name,
        kind = kind,
        baseUrl = baseUrl,
        apiKey = apiKey,
        model = model,
        isDefault = isDefault,
    )

    // Base URL default per jenis protokol supaya provider baru tidak langsung
    // gagal karena field kosong. Dipakai tombol Tes maupun Simpan.
    val resolvedUrl = baseUrl.ifBlank {
        if (kind == "anthropic") "https://api.anthropic.com" else "https://api.openai.com/v1"
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (initial.id.isBlank()) tr("Tambah provider", "Add provider", "プロバイダーを追加") else tr("Ubah provider", "Edit provider", "プロバイダーを編集")) },
        text = {
            Column {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text(tr("Nama", "Name", "名前")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(Space.sm))
                Row(horizontalArrangement = Arrangement.spacedBy(Space.xs)) {
                    listOf("openai" to "OpenAI-compatible", "anthropic" to "Anthropic").forEach { (id, label) ->
                        FilterChip(
                            selected = kind == id,
                            onClick = { kind = id },
                            label = { Text(label, style = MaterialTheme.typography.labelSmall) },
                        )
                    }
                }
                Spacer(Modifier.height(Space.sm))
                OutlinedTextField(
                    value = baseUrl,
                    onValueChange = { baseUrl = it },
                    label = { Text(tr("Base URL", "Base URL", "ベースURL")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                )
                Spacer(Modifier.height(Space.sm))
                OutlinedTextField(
                    value = apiKey,
                    onValueChange = { apiKey = it },
                    label = { Text(tr("API key", "API key", "APIキー")) },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(Space.sm))
                OutlinedTextField(
                    value = model,
                    onValueChange = { model = it },
                    label = { Text(tr("Model", "Model", "モデル")) },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                )
                Spacer(Modifier.height(Space.sm))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    FilterChip(
                        selected = isDefault,
                        onClick = { isDefault = !isDefault },
                        label = { Text(tr("Jadikan default", "Set as default", "デフォルトにする")) },
                    )
                    Spacer(Modifier.width(Space.sm))
                    TextButton(
                        onClick = {
                            vm.testProvider(draft.copy(baseUrl = resolvedUrl)) { testResult = it }
                        },
                    ) { Text(tr("Tes koneksi", "Test connection", "接続テスト")) }
                }
                testResult?.let {
                    Text(it, style = MaterialTheme.typography.labelSmall, color = clay.muted)
                }
                Spacer(Modifier.height(Space.sm))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        tr(
                            "Model tersedia (boleh pilih lebih dari satu):",
                            "Available models (select more than one):",
                            "利用可能なモデル（複数選択可）:",
                        ),
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                        modifier = Modifier.weight(1f),
                    )
                    TextButton(
                        onClick = {
                            val pid = initial.id
                            if (pid.isBlank()) {
                                testResult = trFor(
                                    lang, "Simpan dulu provider-nya.", "Save the provider first.", "先にプロバイダーを保存してください。",
                                )
                            } else {
                                refreshingModels = true
                                vm.refreshModels(pid) {
                                    models = it
                                    refreshingModels = false
                                }
                            }
                        },
                        enabled = !refreshingModels,
                    ) {
                        Text(
                            if (refreshingModels) tr("Mengecek…", "Checking…", "確認中…")
                            else tr("Cek dari server", "Refresh", "再取得"),
                        )
                    }
                }
                // Daftar model bisa sangat panjang: pakai LazyColumn supaya
                // bisa digulir, bukan `take(6)` yang menyembunyikan sisanya.
                LazyColumn(Modifier.heightIn(max = 180.dp)) {
                    items(allModels, key = { it }) { m ->
                        FilterChip(
                            selected = m in selectedModels,
                            onClick = {
                                selectedModels = if (m in selectedModels) selectedModels - m
                                else selectedModels + m
                                if (model.isBlank()) model = m
                            },
                            label = { Text(m, style = MaterialTheme.typography.labelSmall, maxLines = 1) },
                        )
                    }
                }
                Spacer(Modifier.height(Space.xs))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    OutlinedTextField(
                        value = manualModel,
                        onValueChange = { manualModel = it },
                        label = {
                            Text(tr("Tambah model manual", "Add a model manually", "モデルを手動で追加"))
                        },
                        singleLine = true,
                        modifier = Modifier.weight(1f),
                    )
                    Spacer(Modifier.width(Space.xs))
                    TextButton(onClick = {
                        val added = manualModel.trim()
                        if (added.isNotBlank()) {
                            selectedModels = selectedModels + added
                            if (model.isBlank()) model = added
                            manualModel = ""
                        }
                    }) { Text(tr("Tambah", "Add", "追加")) }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                if (name.isBlank()) {
                    testResult = trFor(lang, "Nama provider wajib diisi.", "Provider name is required.", "プロバイダー名は必須です。")
                    return@TextButton
                }
                val finalModels = selectedModels + listOfNotNull(model.trim().takeIf { it.isNotBlank() })
                vm.saveProvider(
                    draft.copy(
                        name = name.trim(),
                        baseUrl = resolvedUrl,
                        model = model.trim(),
                        models = finalModels.toList().sorted(),
                    ),
                ) { onDismiss() }
            }) { Text(tr("Simpan", "Save", "保存")) }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(tr("Batal", "Cancel", "キャンセル")) }
        },
    )
}
