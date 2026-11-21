package com.arka.agent.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.AttachFile
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.KeyboardArrowDown
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.Send
import androidx.compose.material.icons.outlined.Stop
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import com.arka.agent.MainViewModel
import com.arka.agent.core.FileInfo
import com.arka.agent.core.Message
import com.arka.agent.core.Session
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.components.ClayPill
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay

/**
 * Tab Chat.
 *
 * Layar saat belum ada sesi: ajakan mulai. Saat sesi aktif: riwayat pesan,
 * kolom input, lampiran file, dan pemilih model di kanan atas. Sidebar kiri
 * berisi daftar sesi (⋮: ganti nama / hapus) dan tombol sesi baru.
 */
@Composable
fun ChatScreen(vm: MainViewModel) {
    val sessions by vm.sessions.collectAsState()
    val currentId by vm.currentSessionId.collectAsState()
    val messages by vm.messages.collectAsState()
    val streaming by vm.streamingText.collectAsState()
    val busy by vm.busy.collectAsState()
    val settings by vm.settings.collectAsState()

    var drawer by remember { mutableStateOf(false) }
    var input by remember { mutableStateOf("") }
    var attachments by remember { mutableStateOf<List<String>>(emptyList()) }
    var showAttach by remember { mutableStateOf(false) }
    var showModelMenu by remember { mutableStateOf(false) }
    var models by remember { mutableStateOf<List<String>>(emptyList()) }
    var preview by remember { mutableStateOf<FileInfo?>(null) }

    val current = sessions.firstOrNull { it.id == currentId }

    LaunchedEffect(currentId, current?.providerId) {
        val pid = current?.providerId
        if (pid != null) {
            vm.loadModels(pid) { models = it }
        } else {
            models = emptyList()
        }
    }

    Box(Modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            ChatTopBar(
                title = current?.title ?: "Arka Agent",
                model = current?.model,
                hasSession = current != null,
                showModelMenu = showModelMenu,
                models = models,
                onMenu = { drawer = true },
                onToggleModels = { showModelMenu = !showModelMenu },
                onSelectModel = { model ->
                    showModelMenu = false
                    vm.setSessionModel(current?.providerId, model)
                },
            )

            if (current == null) {
                EmptyChat(onStart = { vm.newSession() })
            } else {
                MessageList(
                    messages = messages,
                    streaming = streaming,
                    busy = busy,
                    autoScroll = settings["auto_scroll"] != "false",
                    resolveFile = vm::workspaceFile,
                    onOpenFile = { path ->
                        // Lampiran diketuk → buka dialog pratinjau yang sama
                        // dengan tab File (gambar, teks, PDF, video).
                        vm.statFile(path) { result ->
                            result.onSuccess { preview = it }
                                .onFailure { vm.notify(it.message ?: "File tidak ditemukan") }
                        }
                    },
                    modifier = Modifier.weight(1f),
                )
                InputBar(
                    input = input,
                    attachments = attachments,
                    busy = busy,
                    onInputChange = { input = it },
                    onAttach = { showAttach = true },
                    onRemoveAttachment = { path -> attachments = attachments - path },
                    onSend = {
                        val text = input
                        val files = attachments
                        vm.send(text, files)
                        input = ""
                        attachments = emptyList()
                    },
                    onStop = { vm.stopChat() },
                )
            }
        }

        if (drawer) {
            SessionDrawer(
                sessions = sessions,
                currentId = currentId,
                onDismiss = { drawer = false },
                onSelect = {
                    vm.selectSession(it)
                    drawer = false
                },
                onNew = {
                    vm.newSession()
                    drawer = false
                },
                onRename = { id, title -> vm.renameSession(id, title) },
                onDelete = { id -> vm.deleteSession(id) },
            )
        }

        if (showAttach) {
            AttachDialog(
                vm = vm,
                onDismiss = { showAttach = false },
                onPick = { path ->
                    attachments = (attachments + path).distinct()
                    showAttach = false
                },
            )
        }

        preview?.let { file ->
            FilePreviewDialog(vm = vm, file = file, onDismiss = { preview = null })
        }
    }
}

@Composable
private fun ChatTopBar(
    title: String,
    model: String?,
    hasSession: Boolean,
    showModelMenu: Boolean,
    models: List<String>,
    onMenu: () -> Unit,
    onToggleModels: () -> Unit,
    onSelectModel: (String) -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Space.md, vertical = Space.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onMenu) {
            Icon(Icons.Outlined.Menu, "Menu sesi", tint = clay.onSurface)
        }
        Spacer(Modifier.width(Space.xs))
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1)
            if (hasSession) {
                Text(
                    model ?: "tanpa model",
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
            }
        }
        if (hasSession) {
            Box {
                Row(
                    Modifier
                        .clip(RoundedCornerShape(50))
                        .background(clay.surface)
                        .clickable { onToggleModels() }
                        .padding(horizontal = 12.dp, vertical = 6.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        model?.substringAfterLast('/') ?: "Model",
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.accent,
                        maxLines = 1,
                    )
                    Spacer(Modifier.width(Space.xs))
                    Icon(
                        Icons.Outlined.KeyboardArrowDown,
                        null,
                        tint = clay.accent,
                        modifier = Modifier.size(16.dp),
                    )
                }
                DropdownMenu(expanded = showModelMenu, onDismissRequest = { onToggleModels() }) {
                    if (models.isEmpty()) {
                        // Menu tetap bisa ditutup walau daftar kosong.
                        DropdownMenuItem(
                            text = { Text("Belum ada model") },
                            onClick = { onToggleModels() },
                        )
                    }
                    models.forEach { m ->
                        DropdownMenuItem(
                            text = { Text(m, maxLines = 1) },
                            onClick = { onSelectModel(m) },
                        )
                    }
                }
            }
        }
    }
}

@Composable
private fun EmptyChat(onStart: () -> Unit) {
    Box(Modifier.fillMaxSize().padding(Space.xl), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            ClayCard {
                Column(horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("Arka Agent", style = MaterialTheme.typography.titleLarge)
                    Spacer(Modifier.height(Space.sm))
                    Text(
                        "Chat dengan AI + jalankan Python, Git, dan file di workspace " +
                            "Ubuntu 24 langsung dari perangkat.",
                        style = MaterialTheme.typography.bodySmall,
                        color = clay.muted,
                    )
                    Spacer(Modifier.height(Space.md))
                    ClayPill("Mulai sesi baru", clay.accent, onClick = onStart)
                }
            }
            Spacer(Modifier.height(Space.lg))
            TextButton(onClick = onStart) { Text("Buat sesi") }
        }
    }
}

@Composable
private fun MessageList(
    messages: List<Message>,
    streaming: String,
    busy: Boolean,
    autoScroll: Boolean,
    resolveFile: (String) -> java.io.File,
    onOpenFile: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val listState = rememberLazyListState()
    val lastContent = streaming.ifEmpty { messages.lastOrNull()?.content.orEmpty() }

    LaunchedEffect(messages.size, lastContent.length, autoScroll) {
        if (!autoScroll) return@LaunchedEffect
        val target = if (streaming.isNotEmpty()) messages.size else messages.lastIndex.coerceAtLeast(0)
        if (target >= 0) listState.animateScrollToItem(target)
    }

    LazyColumn(
        modifier = modifier.fillMaxSize().padding(horizontal = Space.md),
        state = listState,
        contentPadding = PaddingValues(vertical = Space.sm),
        verticalArrangement = Arrangement.spacedBy(Space.sm),
    ) {
        items(messages, key = { it.id }) { m ->
            MessageBubble(message = m, resolveFile = resolveFile, onOpenFile = onOpenFile)
        }
        if (streaming.isNotEmpty()) {
            item(key = "streaming") {
                MessageBubble(
                    message = Message(
                        id = "streaming",
                        sessionId = "",
                        role = "assistant",
                        content = streaming,
                    ),
                    streaming = true,
                    resolveFile = resolveFile,
                    onOpenFile = onOpenFile,
                )
            }
        }
        if (messages.isEmpty() && streaming.isEmpty()) {
            item {
                Text(
                    if (busy) "Berpikir…" else "Kirim pesan untuk mulai.",
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                    modifier = Modifier.fillMaxWidth().padding(vertical = Space.lg),
                )
            }
        }
    }
}

@Composable
private fun InputBar(
    input: String,
    attachments: List<String>,
    busy: Boolean,
    onInputChange: (String) -> Unit,
    onAttach: () -> Unit,
    onRemoveAttachment: (String) -> Unit,
    onSend: () -> Unit,
    onStop: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .navigationBarsPadding()
            .imePadding()
            .padding(horizontal = Space.md, vertical = Space.sm),
    ) {
        if (attachments.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().padding(bottom = Space.xs),
                horizontalArrangement = Arrangement.spacedBy(Space.xs),
            ) {
                attachments.take(3).forEach { path ->
                    // Ketuk pill untuk melepas satu lampiran.
                    ClayPill(
                        text = path.substringAfterLast('/'),
                        color = clay.accent,
                        onClick = { onRemoveAttachment(path) },
                    )
                }
                if (attachments.size > 3) {
                    ClayPill("+${attachments.size - 3}", clay.muted)
                }
                Spacer(Modifier.weight(1f))
                IconButton(onClick = { attachments.forEach { onRemoveAttachment(it) } }) {
                    Icon(Icons.Outlined.Close, "Hapus lampiran", tint = clay.muted)
                }
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            ClayCard(
                Modifier.weight(1f),
                shape = RoundedCornerShape(20.dp),
                elevation = 2.dp,
                contentPadding = 0.dp,
            ) {
                Row(
                    Modifier.padding(horizontal = Space.sm, vertical = Space.xs),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    IconButton(onClick = onAttach) {
                        Icon(Icons.Outlined.AttachFile, "Lampirkan", tint = clay.muted)
                    }
                    OutlinedTextField(
                        value = input,
                        onValueChange = onInputChange,
                        modifier = Modifier.weight(1f),
                        placeholder = { Text("Kirim pesan…", color = clay.muted) },
                        textStyle = MaterialTheme.typography.bodyMedium,
                        maxLines = 4,
                        // Enter di keyboard = kirim (Shift+Enter tetap baris baru
                        // di keyboard fisik).
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
                        keyboardActions = KeyboardActions(onSend = {
                            if (!busy && input.isNotBlank()) onSend()
                        }),
                    )
                }
            }
            Spacer(Modifier.width(Space.sm))
            Box(
                Modifier
                    .size(46.dp)
                    .clip(RoundedCornerShape(16.dp))
                    .background(if (busy) clay.danger else clay.accent)
                    .clickable { if (busy) onStop() else onSend() },
                contentAlignment = Alignment.Center,
            ) {
                Icon(
                    imageVector = if (busy) Icons.Outlined.Stop else Icons.Outlined.Send,
                    contentDescription = if (busy) "Hentikan" else "Kirim",
                    tint = Color.White,
                )
            }
        }
    }
}

@Composable
private fun SessionDrawer(
    sessions: List<Session>,
    currentId: String?,
    onDismiss: () -> Unit,
    onSelect: (String) -> Unit,
    onNew: () -> Unit,
    onRename: (String, String) -> Unit,
    onDelete: (String) -> Unit,
) {
    Box(Modifier.fillMaxSize()) {
        // Latar tembus pandang: klik di luar panel menutup.
        Box(
            Modifier
                .fillMaxSize()
                .background(Color.Black.copy(alpha = 0.35f))
                .clickable(onClick = onDismiss),
        )
        ClayCard(
            Modifier
                .widthIn(max = 320.dp)
                .fillMaxHeight()
                .padding(Space.sm),
            shape = RoundedCornerShape(bottomStart = 24.dp, bottomEnd = 24.dp),
            contentPadding = Space.md,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text("Sesi", style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                IconButton(onClick = onNew) {
                    Icon(Icons.Outlined.Add, "Sesi baru", tint = clay.accent)
                }
            }
            Spacer(Modifier.height(Space.sm))
            if (sessions.isEmpty()) {
                Text("Belum ada sesi.", style = MaterialTheme.typography.bodySmall, color = clay.muted)
            }
            LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                items(sessions, key = { it.id }) { s ->
                    SessionRow(
                        session = s,
                        active = s.id == currentId,
                        onClick = { onSelect(s.id) },
                        onRename = { newTitle -> onRename(s.id, newTitle) },
                        onDelete = { onDelete(s.id) },
                    )
                }
            }
            Spacer(Modifier.height(Space.sm))
            TextButton(onClick = onNew, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Outlined.Add, null, tint = clay.accent)
                Spacer(Modifier.width(Space.xs))
                Text("Sesi baru")
            }
        }
    }
}

@Composable
private fun SessionRow(
    session: Session,
    active: Boolean,
    onClick: () -> Unit,
    onRename: (String) -> Unit,
    onDelete: () -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    var renaming by remember { mutableStateOf(false) }
    var title by remember(session.title) { mutableStateOf(session.title) }

    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(14.dp))
            .background(if (active) clay.accentSoft else Color.Transparent)
            .clickable { onClick() }
            .padding(horizontal = Space.sm, vertical = Space.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            if (renaming) {
                OutlinedTextField(
                    value = title,
                    onValueChange = { title = it },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
                Row {
                    TextButton(onClick = {
                        if (title.isNotBlank()) onRename(title.trim())
                        renaming = false
                    }) { Text("Simpan") }
                    TextButton(onClick = { renaming = false }) { Text("Batal") }
                }
            } else {
                Text(
                    session.title,
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (active) clay.accent else clay.onSurface,
                    maxLines = 1,
                )
                if (session.messageCount > 0) {
                    Text(
                        "${session.messageCount} pesan",
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                    )
                }
            }
        }
        Box {
            IconButton(onClick = { menu = true }) {
                Icon(Icons.Outlined.MoreVert, "Aksi", tint = clay.muted)
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(
                    text = { Text("Ganti nama") },
                    leadingIcon = { Icon(Icons.Outlined.Edit, null) },
                    onClick = {
                        menu = false
                        title = session.title
                        renaming = true
                    },
                )
                DropdownMenuItem(
                    text = { Text("Hapus") },
                    leadingIcon = { Icon(Icons.Outlined.Delete, null, tint = clay.danger) },
                    onClick = {
                        menu = false
                        onDelete()
                    },
                )
            }
        }
    }
}