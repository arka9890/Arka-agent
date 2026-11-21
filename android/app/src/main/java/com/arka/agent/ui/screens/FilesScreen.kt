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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.CreateNewFolder
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material.icons.outlined.FolderOpen
import androidx.compose.material.icons.outlined.Image
import androidx.compose.material.icons.outlined.Movie
import androidx.compose.material.icons.outlined.PictureAsPdf
import androidx.compose.material.icons.outlined.UploadFile
import androidx.compose.material3.AlertDialog
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
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.unit.dp
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import com.arka.agent.MainViewModel
import com.arka.agent.core.FileInfo
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.components.ClayPill
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay

/**
 * Tab File: penjelajah workspace + preview.
 *
 * Preview mengikuti tabel PRD: teks (isi lengkap), gambar (fullscreen),
 * video (dibuka di pemutar sistem), PDF (metadata + buka), binary (metadata +
 * unduh).
 */
@Composable
fun FilesScreen(vm: MainViewModel) {
    val files by vm.files.collectAsState()
    val dir by vm.currentDir.collectAsState()

    var preview by remember { mutableStateOf<FileInfo?>(null) }
    var deleting by remember { mutableStateOf<FileInfo?>(null) }
    var newFolder by remember { mutableStateOf(false) }

    val picker = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenDocument(),
    ) { uri -> uri?.let { vm.importFile(it) } }

    LaunchedEffect(Unit) { vm.refreshFiles(dir) }

    Column(Modifier.fillMaxSize().padding(horizontal = Space.md)) {
        Row(
            Modifier.fillMaxWidth().padding(vertical = Space.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text("Workspace", style = MaterialTheme.typography.titleLarge)
                Text(
                    if (dir.isBlank()) "/" else "/$dir",
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
            }
            IconButton(onClick = { newFolder = true }) {
                Icon(Icons.Outlined.CreateNewFolder, "Folder baru", tint = clay.accent)
            }
            IconButton(onClick = { picker.launch(arrayOf("*/*")) }) {
                Icon(Icons.Outlined.UploadFile, "Impor file", tint = clay.accent)
            }
        }

        LazyColumn(
            verticalArrangement = Arrangement.spacedBy(Space.xs),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = Space.xl),
        ) {
            if (dir.isNotBlank()) {
                item(key = "up") {
                    Row(
                        Modifier
                            .fillMaxWidth()
                            .clip(RoundedCornerShape(14.dp))
                            .clickable { vm.refreshFiles(dir.substringBeforeLast('/', "")) }
                            .padding(Space.md),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Icon(Icons.Outlined.FolderOpen, null, tint = clay.accent)
                        Spacer(Modifier.width(Space.sm))
                        Text("Folder induk", style = MaterialTheme.typography.bodyMedium)
                    }
                }
            }
            items(files, key = { it.path }) { file ->
                Row(
                    Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(14.dp))
                        .background(clay.surface)
                        .clickable {
                            if (file.isDir) vm.refreshFiles(file.path) else preview = file
                        }
                        .padding(horizontal = Space.md, vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Icon(
                        imageVector = iconFor(file),
                        contentDescription = null,
                        tint = if (file.isDir) clay.accent else clay.muted,
                        modifier = Modifier.size(20.dp),
                    )
                    Spacer(Modifier.width(Space.sm))
                    Column(Modifier.weight(1f)) {
                        Text(file.name, style = MaterialTheme.typography.bodyMedium, maxLines = 1)
                        Text(
                            if (file.isDir) "folder" else humanSize(file.size),
                            style = MaterialTheme.typography.labelSmall,
                            color = clay.muted,
                        )
                    }
                    IconButton(
                        onClick = { deleting = file },
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
            if (files.isEmpty()) {
                item(key = "empty") {
                    ClayCard(Modifier.fillMaxWidth().padding(top = Space.md)) {
                        Text(
                            "Folder masih kosong. File yang dibuat dari Terminal " +
                                "atau dilampirkan di chat akan muncul di sini.",
                            style = MaterialTheme.typography.bodySmall,
                            color = clay.muted,
                        )
                    }
                }
            }
        }
    }

    preview?.let { file ->
        FilePreviewDialog(
            vm = vm,
            file = file,
            onDismiss = { preview = null },
        )
    }

    deleting?.let { file ->
        AlertDialog(
            onDismissRequest = { deleting = null },
            title = { Text("Hapus ${file.name}?") },
            text = {
                Text(
                    if (file.isDir) "Folder dan seluruh isinya akan dihapus." else "File akan dihapus permanen.",
                )
            },
            confirmButton = {
                TextButton(onClick = {
                    vm.deleteFile(file.path)
                    deleting = null
                }) { Text("Hapus", color = clay.danger) }
            },
            dismissButton = {
                TextButton(onClick = { deleting = null }) { Text("Batal") }
            },
        )
    }

    if (newFolder) {
        var name by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { newFolder = false },
            title = { Text("Folder baru") },
            text = {
                OutlinedTextField(
                    value = name,
                    onValueChange = { name = it },
                    label = { Text("Nama folder") },
                    singleLine = true,
                )
            },
            confirmButton = {
                TextButton(
                    onClick = {
                        if (name.isNotBlank()) vm.newFolder(name.trim())
                        newFolder = false
                    },
                ) { Text("Buat") }
            },
            dismissButton = {
                TextButton(onClick = { newFolder = false }) { Text("Batal") }
            },
        )
    }
}

private fun iconFor(file: FileInfo): ImageVector = when (file.kind) {
    "dir" -> Icons.Outlined.Folder
    "image" -> Icons.Outlined.Image
    "video" -> Icons.Outlined.Movie
    "pdf" -> Icons.Outlined.PictureAsPdf
    else -> Icons.Outlined.Description
}

fun humanSize(bytes: Long): String = when {
    bytes >= 1024 * 1024 -> "%.1f MB".format(bytes / 1024.0 / 1024.0)
    bytes >= 1024 -> "${bytes / 1024} KB"
    else -> "$bytes B"
}
