package com.arka.agent.ui.screens

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.AttachFile
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Delete
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.material.icons.outlined.KeyboardArrowDown
import androidx.compose.material.icons.outlined.Menu
import androidx.compose.material.icons.outlined.MoreVert
import androidx.compose.material.icons.outlined.Refresh
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import com.arka.agent.MainViewModel
import com.arka.agent.ui.LocalLanguage
import com.arka.agent.ui.tr
import com.arka.agent.ui.trFor
import com.arka.agent.core.AttachmentRef
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
    val providers by vm.providers.collectAsState()

    var drawer by remember { mutableStateOf(false) }
    var input by remember { mutableStateOf("") }
    var attachments by remember { mutableStateOf<List<AttachmentRef>>(emptyList()) }
    var showModelMenu by remember { mutableStateOf(false) }
    var modelQuery by remember { mutableStateOf("") }
    var models by remember { mutableStateOf<List<String>>(emptyList()) }
    var refreshingModels by remember { mutableStateOf(false) }
    var modelInput by remember { mutableStateOf<String?>(null) }
    var preview by remember { mutableStateOf<FileInfo?>(null) }
    val lang = LocalLanguage.current
    val context = LocalContext.current
    val clipboard = remember {
        context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    }
    val onCopy: (String) -> Unit = { text ->
        runCatching {
            clipboard.setPrimaryClip(ClipData.newPlainText("Arka Agent", text))
            vm.notify(trFor(
                lang, "Pesan disalin.", "Message copied.", "メッセージをコピーしました。",
            ))
        }.onFailure { vm.notify(it.message ?: "Gagal menyalin") }
    }

    val current = sessions.firstOrNull { it.id == currentId }

    // Pemilih lampiran langsung ke file manager perangkat (SAF). File tidak
    // disalin ke workspace — cukup URI content:// yang dibawa ke pesan.
    val attachLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.OpenMultipleDocuments(),
    ) { uris ->
        if (uris.isNotEmpty()) {
            vm.describeUris(uris) { refs ->
                attachments = (attachments + refs).distinctBy { it.uri }
            }
        }
    }
    // Sesi lama bisa punya provider_id kosong. Pakai provider default sebagai
    // fallback supaya daftar model tetap bisa dimuat dan dipilih.
    val activeProvider = current?.providerId?.let { pid -> providers.firstOrNull { it.id == pid } }
        ?: providers.firstOrNull { it.isDefault }
        ?: providers.firstOrNull()
    val menuModels = (listOfNotNull(current?.model?.takeIf { it.isNotBlank() }) + models).distinct()
    val filteredModels = menuModels.filter { it.contains(modelQuery, ignoreCase = true) }

    LaunchedEffect(currentId, activeProvider?.id) {
        val pid = activeProvider?.id
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
                models = filteredModels,
                modelQuery = modelQuery,
                refreshingModels = refreshingModels,
                onModelQuery = { modelQuery = it },
                onRefreshModels = {
                    val pid = activeProvider?.id
                    if (pid == null) {
                        vm.notify(trFor(lang, "Pilih provider dulu.", "Choose a provider first.", "先にプロバイダーを選択してください。"))
                    } else {
                        refreshingModels = true
                        vm.refreshModels(pid) {
                            models = it
                            refreshingModels = false
                        }
                    }
                },
                onMenu = { drawer = true },
                onToggleModels = {
                    if (!showModelMenu) modelQuery = ""
                    showModelMenu = !showModelMenu
                },
                onSelectModel = { model ->
                    showModelMenu = false
                    vm.setSessionModel(activeProvider?.id ?: current?.providerId, model)
                },
                onManualModel = { modelInput = "" },
            )

            if (current == null) {
                EmptyChat(onStart = { vm.newSession() })
            } else {
                // Box dengan weight: tinggi daftar pesan dibatasi sisa ruang
                // kolom, sehingga InputBar selalu terlihat di bawah. Jangan
                // menaruh weight langsung di SelectionContainer — kalau dia
                // tidak meneruskan parent data, daftar pesan tumbuh tanpa
                // batas dan kolom input terdorong keluar layar.
                Box(
                    Modifier
                        .weight(1f)
                        .fillMaxWidth(),
                ) {
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
                                    .onFailure { vm.notify(it.message ?: trFor(lang, "File tidak ditemukan", "File not found", "ファイルが見つかりません")) }
                            }
                        },
                        onCopy = onCopy,
                        modifier = Modifier.fillMaxSize(),
                    )
                }
                InputBar(
                    input = input,
                    attachments = attachments,
                    busy = busy,
                    onInputChange = { input = it },
                    onAttach = { attachLauncher.launch(arrayOf("*/*")) },
                    onRemoveAttachment = { ref -> attachments = attachments - ref },
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
                onCompact = { id -> vm.compactSession(id) },
            )
        }

        preview?.let { file ->
            FilePreviewDialog(vm = vm, file = file, onDismiss = { preview = null })
        }

        modelInput?.let { draft ->
            var typed by remember { mutableStateOf(draft) }
            AlertDialog(
                onDismissRequest = { modelInput = null },
                title = { Text(tr("Pilih model", "Choose a model", "モデルを選択")) },
                text = {
                    OutlinedTextField(
                        value = typed,
                        onValueChange = { typed = it },
                        label = { Text(tr("Nama model", "Model name", "モデル名")) },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                },
                confirmButton = {
                    TextButton(onClick = {
                        if (typed.isNotBlank()) {
                            vm.setSessionModel(activeProvider?.id ?: current?.providerId, typed.trim())
                        }
                        modelInput = null
                    }) { Text(tr("Pakai", "Use", "使用")) }
                },
                dismissButton = {
                    TextButton(onClick = { modelInput = null }) {
                        Text(tr("Batal", "Cancel", "キャンセル"))
                    }
                },
            )
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
    modelQuery: String,
    refreshingModels: Boolean,
    onModelQuery: (String) -> Unit,
    onRefreshModels: () -> Unit,
    onMenu: () -> Unit,
    onToggleModels: () -> Unit,
    onSelectModel: (String) -> Unit,
    onManualModel: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = Space.md, vertical = Space.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onMenu) {
            Icon(Icons.Outlined.Menu, tr("Menu sesi", "Session menu", "セッションメニュー"), tint = clay.onSurface)
        }
        Spacer(Modifier.width(Space.xs))
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1)
            if (hasSession) {
                Text(
                    model ?: tr("tanpa model", "no model", "モデルなし"),
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
                        model?.substringAfterLast('/') ?: tr("Model", "Model", "モデル"),
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
                    // Daftar bisa panjang (OpenRouter ratusan model), jadi
                    // dibatasi tingginya dan tetap bisa digulir.
                    Column(
                        Modifier
                            .heightIn(max = 320.dp)
                            .verticalScroll(rememberScrollState()),
                    ) {
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .padding(horizontal = 8.dp, vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            OutlinedTextField(
                                value = modelQuery,
                                onValueChange = onModelQuery,
                                placeholder = {
                                    Text(tr("Cari model…", "Search models…", "モデルを検索…"))
                                },
                                singleLine = true,
                                textStyle = MaterialTheme.typography.labelSmall,
                                modifier = Modifier.weight(1f),
                            )
                            IconButton(onClick = onRefreshModels, enabled = !refreshingModels) {
                                Icon(
                                    Icons.Outlined.Refresh,
                                    tr("Cek model dari server", "Refresh models", "モデルを再取得"),
                                    tint = clay.accent,
                                )
                            }
                        }
                        if (models.isEmpty()) {
                            DropdownMenuItem(
                                text = {
                                    Text(
                                        if (modelQuery.isBlank()) {
                                            tr("Belum ada model", "No models yet", "モデルがありません")
                                        } else {
                                            tr("Tidak ada model cocok", "No matching models", "一致するモデルがありません")
                                        },
                                    )
                                },
                                onClick = { onToggleModels() },
                            )
                        }
                        models.forEach { m ->
                            DropdownMenuItem(
                                text = { Text(m, maxLines = 1) },
                                onClick = { onSelectModel(m) },
                            )
                        }
                        DropdownMenuItem(
                            text = {
                                Text(tr(
                                    "Ketik nama model…", "Type a model name…", "モデル名を入力…",
                                ))
                            },
                            onClick = { onManualModel() },
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
                        tr(
                            "Chat dengan AI + jalankan Python, Git, dan file di workspace " +
                                "Ubuntu 24 langsung dari perangkat.",
                            "Chat with AI + run Python, Git, and files in the Ubuntu 24 " +
                                "workspace right from your device.",
                            "AIとチャットし、Ubuntu 24ワークスペースのPythonやGit、ファイルを端末から操作できます。",
                        ),
                        style = MaterialTheme.typography.bodySmall,
                        color = clay.muted,
                    )
                    Spacer(Modifier.height(Space.md))
                    ClayPill(tr("Mulai sesi baru", "Start a new session", "新しいセッションを開始"), clay.accent, onClick = onStart)
                }
            }
            Spacer(Modifier.height(Space.lg))
            TextButton(onClick = onStart) { Text(tr("Buat sesi", "New session", "新しいセッション")) }
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
    onCopy: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val listState = rememberLazyListState()
    val lastContent = streaming.ifEmpty { messages.lastOrNull()?.content.orEmpty() }

    LaunchedEffect(messages.size, lastContent.length, autoScroll) {
        if (!autoScroll) return@LaunchedEffect
        val target = if (streaming.isNotEmpty()) messages.size else messages.lastIndex.coerceAtLeast(0)
        if (target >= 0) listState.animateScrollToItem(target)
    }

    // SelectionContainer: teks bisa dipilih/dicopy dengan long-press sistem,
    // selain tombol copy di tiap bubble.
    // PENTING: `modifier` (berisi weight(1f)) harus dipasang di sini, bukan di
    // LazyColumn. Kalau weight ada di LazyColumn, SelectionContainer tidak
    // ikut terukur dan kolom input terdorong keluar layar.
    SelectionContainer(modifier = modifier.fillMaxSize()) {
        LazyColumn(
            modifier = Modifier.fillMaxSize().padding(horizontal = Space.md),
            state = listState,
            contentPadding = PaddingValues(vertical = Space.sm),
            verticalArrangement = Arrangement.spacedBy(Space.sm),
        ) {
        items(messages, key = { it.id }) { m ->
            MessageBubble(
                message = m,
                resolveFile = resolveFile,
                onOpenFile = onOpenFile,
                onCopy = onCopy,
            )
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
                    onCopy = onCopy,
                )
            }
        }
        if (messages.isEmpty() && streaming.isEmpty()) {
            item {
                Text(
                    if (busy) tr("Berpikir…", "Thinking…", "考えています…") else tr("Kirim pesan untuk mulai.", "Send a message to start.", "メッセージを送信して始めましょう。"),
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                    modifier = Modifier.fillMaxWidth().padding(vertical = Space.lg),
                )
            }
        }
        }
    }
}

@Composable
private fun InputBar(
    input: String,
    attachments: List<AttachmentRef>,
    busy: Boolean,
    onInputChange: (String) -> Unit,
    onAttach: () -> Unit,
    onRemoveAttachment: (AttachmentRef) -> Unit,
    onSend: () -> Unit,
    onStop: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            // Keyboard (IME) sudah ditangani imePadding di Scaffold (AppScreen),
            // jadi di sini cukup padding biasa — kalau ditambah imePadding lagi,
            // kolom input melayang jauh di atas keyboard.
            .padding(horizontal = Space.md, vertical = Space.sm),
    ) {
        if (attachments.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().padding(bottom = Space.xs),
                horizontalArrangement = Arrangement.spacedBy(Space.xs),
            ) {
                attachments.take(3).forEach { ref ->
                    // Ketuk pill untuk melepas satu lampiran.
                    ClayPill(
                        text = ref.name,
                        color = clay.accent,
                        onClick = { onRemoveAttachment(ref) },
                    )
                }
                if (attachments.size > 3) {
                    ClayPill("+${attachments.size - 3}", clay.muted)
                }
                Spacer(Modifier.weight(1f))
                IconButton(onClick = { attachments.forEach { onRemoveAttachment(it) } }) {
                    Icon(Icons.Outlined.Close, tr("Hapus lampiran", "Remove attachment", "添付を削除"), tint = clay.muted)
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
                        Icon(Icons.Outlined.AttachFile, tr("Lampirkan", "Attach", "添付"), tint = clay.muted)
                    }
                    OutlinedTextField(
                        value = input,
                        onValueChange = onInputChange,
                        modifier = Modifier.weight(1f),
                        placeholder = { Text(tr("Kirim pesan…", "Type a message…", "メッセージを入力…"), color = clay.muted) },
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
                    contentDescription = if (busy) tr("Hentikan", "Stop", "停止") else tr("Kirim", "Send", "送信"),
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
    onCompact: (String) -> Unit,
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
                Text(tr("Sesi", "Sessions", "セッション"), style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f))
                IconButton(onClick = onNew) {
                    Icon(Icons.Outlined.Add, tr("Sesi baru", "New session", "新しいセッション"), tint = clay.accent)
                }
            }
            Spacer(Modifier.height(Space.sm))
            if (sessions.isEmpty()) {
                Text(tr("Belum ada sesi.", "No sessions yet.", "セッションがありません。"), style = MaterialTheme.typography.bodySmall, color = clay.muted)
            }
            LazyColumn(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(Space.xs)) {
                items(sessions, key = { it.id }) { s ->
                    SessionRow(
                        session = s,
                        active = s.id == currentId,
                        onClick = { onSelect(s.id) },
                        onRename = { newTitle -> onRename(s.id, newTitle) },
                        onDelete = { onDelete(s.id) },
                        onCompact = { onCompact(s.id) },
                    )
                }
            }
            Spacer(Modifier.height(Space.sm))
            TextButton(onClick = onNew, modifier = Modifier.fillMaxWidth()) {
                Icon(Icons.Outlined.Add, null, tint = clay.accent)
                Spacer(Modifier.width(Space.xs))
                Text(tr("Sesi baru", "New session", "新しいセッション"))
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
    onCompact: () -> Unit,
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
                    }) { Text(tr("Simpan", "Save", "保存")) }
                    TextButton(onClick = { renaming = false }) { Text(tr("Batal", "Cancel", "キャンセル")) }
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
                        "${session.messageCount} ${tr("pesan", "messages", "メッセージ")}",
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
                    text = { Text(tr("Ganti nama", "Rename", "名前を変更")) },
                    leadingIcon = { Icon(Icons.Outlined.Edit, null) },
                    onClick = {
                        menu = false
                        title = session.title
                        renaming = true
                    },
                )
                DropdownMenuItem(
                    text = { Text(tr("Ringkas konteks", "Compact context", "コンテキストを要約")) },
                    leadingIcon = { Icon(Icons.Outlined.Refresh, null) },
                    onClick = {
                        menu = false
                        onCompact()
                    },
                )
                DropdownMenuItem(
                    text = { Text(tr("Hapus", "Delete", "削除")) },
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