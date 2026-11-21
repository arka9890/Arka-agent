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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
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
import com.arka.agent.core.Provider
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
@Composable
fun ProvidersScreen(vm: MainViewModel) {
    val providers by vm.providers.collectAsState()
    var editing by remember { mutableStateOf<Provider?>(null) }
    var deleting by remember { mutableStateOf<Provider?>(null) }

    Column(Modifier.fillMaxSize().padding(horizontal = Space.md)) {
        Row(
            Modifier.fillMaxWidth().padding(vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column {
                Text("Provider AI", style = MaterialTheme.typography.titleLarge)
                Text(
                    "API key disimpan lokal di perangkat",
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
                Text("Tambah")
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
                                    ClayPill("default", clay.success)
                                }
                            }
                            Spacer(Modifier.height(2.dp))
                            Text(
                                "${provider.kind} · ${provider.model.ifBlank { "model belum dipilih" }}",
                                style = MaterialTheme.typography.labelSmall,
                                color = clay.muted,
                            )
                            Text(
                                provider.baseUrl,
                                style = MaterialTheme.typography.labelSmall,
                                color = clay.muted,
                                maxLines = 1,
                            )
                        }
                        Column(horizontalAlignment = Alignment.End) {
                            ClayPill(
                                text = if (provider.apiKey.isBlank()) "key kosong" else "key terisi",
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
                                            "Jadikan default",
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
                                        "Ubah",
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
                                        "Hapus",
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
                        "Belum ada provider. Tekan Tambah untuk memakai [OI], Mistral, atau endpoint sendiri.",
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
            title = { Text("Hapus ${provider.name}?") },
            text = { Text("API key provider ini ikut terhapus.") },
            confirmButton = {
                TextButton(onClick = {
                    vm.deleteProvider(provider.id)
                    deleting = null
                }) { Text("Hapus", color = clay.danger) }
            },
            dismissButton = {
                TextButton(onClick = { deleting = null }) { Text("Batal") }
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

    val draft = initial.copy(
        name = name,
        kind = kind,
        baseUrl = baseUrl,
        apiKey = apiKey,
        model = model,
        isDefault = isDefault,
    )

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (initial.id.isBlank()) "Tambah provider" else "Ubah provider") },
        text = {
            Column {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Nama") },
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
                    label = { Text("Base URL") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                )
                Spacer(Modifier.height(Space.sm))
                OutlinedTextField(
                    value = apiKey,
                    onValueChange = { apiKey = it },
                    label = { Text("API key") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    modifier = Modifier.fillMaxWidth(),
                )
                Spacer(Modifier.height(Space.sm))
                OutlinedTextField(
                    value = model,
                    onValueChange = { model = it },
                    label = { Text("Model") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
                )
                Spacer(Modifier.height(Space.sm))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    FilterChip(
                        selected = isDefault,
                        onClick = { isDefault = !isDefault },
                        label = { Text("Jadikan default") },
                    )
                    Spacer(Modifier.width(Space.sm))
                    TextButton(
                        onClick = {
                            vm.testProvider(draft) { testResult = it }
                        },
                    ) { Text("Tes koneksi") }
                }
                testResult?.let {
                    Text(it, style = MaterialTheme.typography.labelSmall, color = clay.muted)
                }
                if (models.isNotEmpty()) {
                    Text(
                        "Model tersedia: ${models.take(5).joinToString(", ")}",
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                    )
                }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                vm.saveProvider(draft) { onDismiss() }
            }) { Text("Simpan") }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text("Batal") }
        },
    )
}
