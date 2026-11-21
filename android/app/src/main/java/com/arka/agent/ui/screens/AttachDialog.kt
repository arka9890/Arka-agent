package com.arka.agent.ui.screens

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
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
import androidx.compose.ui.unit.dp
import com.arka.agent.MainViewModel
import com.arka.agent.ui.tr
import com.arka.agent.core.FileInfo
import com.arka.agent.ui.theme.clay

/**
 * Dialog memilih file workspace sebagai lampiran pesan.
 *
 * Navigasi folder sederhana (root → subfolder) supaya pengguna bisa memilih
 * file di kedalaman mana pun tanpa layar terpisah.
 */
@Composable
fun AttachDialog(
    vm: MainViewModel,
    onDismiss: () -> Unit,
    onPick: (String) -> Unit,
) {
    var dir by remember { mutableStateOf("") }
    var items by remember { mutableStateOf<List<FileInfo>>(emptyList()) }

    LaunchedEffect(dir) {
        items = runCatching { vm.filesNow(dir) }.getOrDefault(emptyList())
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(tr("Lampirkan file", "Attach file", "ファイルを添付")) },
        text = {
            Column {
                Text(
                    if (dir.isBlank()) "/ (workspace)" else "/$dir",
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
                LazyColumn(Modifier.heightIn(max = 320.dp)) {
                    if (dir.isNotBlank()) {
                        item(key = "..") {
                            Row(
                                Modifier
                                    .fillMaxWidth()
                                    .clickable { dir = dir.substringBeforeLast('/', "") }
                                    .padding(vertical = 10.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Icon(Icons.Outlined.Folder, null, tint = clay.muted)
                                Spacer(Modifier.width(8.dp))
                                Text("..", style = MaterialTheme.typography.bodyMedium)
                            }
                        }
                    }
                    items(items, key = { it.path }) { file ->
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .clickable {
                                    if (file.isDir) dir = file.path else onPick(file.path)
                                }
                                .padding(vertical = 10.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(
                                imageVector = if (file.isDir) {
                                    Icons.Outlined.Folder
                                } else {
                                    Icons.Outlined.Description
                                },
                                contentDescription = null,
                                tint = if (file.isDir) clay.accent else clay.muted,
                                modifier = Modifier.size(20.dp),
                            )
                            Spacer(Modifier.width(8.dp))
                            Column {
                                Text(file.name, style = MaterialTheme.typography.bodyMedium)
                                if (!file.isDir) {
                                    Text(
                                        "${file.size / 1024} KB",
                                        style = MaterialTheme.typography.labelSmall,
                                        color = clay.muted,
                                    )
                                }
                            }
                        }
                    }
                    if (items.isEmpty()) {
                        item(key = "empty") {
                            Text(
                                tr("Folder kosong", "Empty folder", "空のフォルダー"),
                                style = MaterialTheme.typography.bodySmall,
                                color = clay.muted,
                                modifier = Modifier.padding(vertical = 12.dp),
                            )
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = onDismiss) { Text(tr("Tutup", "Close", "閉じる")) }
        },
    )
}
