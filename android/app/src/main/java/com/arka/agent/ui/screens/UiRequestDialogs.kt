package com.arka.agent.ui.screens

import android.graphics.BitmapFactory
import android.media.MediaMetadataRetriever
import android.media.MediaPlayer
import android.net.Uri
import android.widget.MediaController
import android.widget.VideoView
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.PlayCircle
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.FileProvider
import com.arka.agent.core.ApprovalRequest
import com.arka.agent.core.FilePreview
import com.arka.agent.core.QuestionRequest
import com.arka.agent.ui.components.ClayCard
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay
import com.arka.agent.ui.tr

/** Kartu persetujuan: model menunggu sampai pengguna menekan Izinkan/Tolak. */
@Composable
fun ApprovalCardDialog(
    request: ApprovalRequest,
    onApprove: () -> Unit,
    onReject: () -> Unit,
) {
    Dialog(
        onDismissRequest = onReject,
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color(0x99000000)),
            contentAlignment = Alignment.Center,
        ) {
            ClayCard(
                Modifier
                    .fillMaxWidth()
                    .padding(Space.lg),
            ) {
                Text(
                    tr("Persetujuan aksi", "Action approval", "操作の承認"),
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(Modifier.height(Space.sm))
                Text(
                    request.action,
                    style = MaterialTheme.typography.bodyMedium,
                    color = clay.accent,
                )
                if (request.target.isNotBlank()) {
                    Text(
                        "Target: ${request.target}",
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.muted,
                    )
                }
                if (request.detail.isNotBlank()) {
                    Spacer(Modifier.height(Space.xs))
                    Text(
                        request.detail,
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.verticalScroll(rememberScrollState()).heightIn(max = 220.dp),
                    )
                }
                Spacer(Modifier.height(Space.md))
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.End,
                ) {
                    TextButton(onClick = onReject) {
                        Text(tr("Tolak", "Reject", "拒否"))
                    }
                    Spacer(Modifier.width(Space.xs))
                    Button(onClick = onApprove) {
                        Text(tr("Izinkan", "Allow", "許可"))
                    }
                }
            }
        }
    }
}

/** Kartu pertanyaan pilihan ganda dari model. */
@Composable
fun QuestionCardDialog(
    request: QuestionRequest,
    onAnswer: (Int) -> Unit,
) {
    Dialog(
        onDismissRequest = { onAnswer(-1) },
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Box(
            Modifier
                .fillMaxSize()
                .background(Color(0x99000000)),
            contentAlignment = Alignment.Center,
        ) {
            ClayCard(
                Modifier
                    .fillMaxWidth()
                    .padding(Space.lg),
            ) {
                Text(
                    tr("Pertanyaan dari AI", "Question from AI", "AIからの質問"),
                    style = MaterialTheme.typography.titleMedium,
                )
                Spacer(Modifier.height(Space.sm))
                Text(request.question, style = MaterialTheme.typography.bodyMedium)
                val previews = request.files.ifEmpty { listOfNotNull(request.file) }
                if (previews.isNotEmpty()) {
                    Spacer(Modifier.height(Space.sm))
                    Column(
                        Modifier
                            .fillMaxWidth()
                            .heightIn(max = 420.dp)
                            .verticalScroll(rememberScrollState()),
                        verticalArrangement = Arrangement.spacedBy(Space.sm),
                    ) {
                        previews.take(5).forEach { preview ->
                            FilePreviewBlock(preview, autoplay = previews.size == 1)
                        }
                    }
                }
                Spacer(Modifier.height(Space.md))
                Column(
                    Modifier
                        .fillMaxWidth()
                        .verticalScroll(rememberScrollState()),
                    verticalArrangement = Arrangement.spacedBy(Space.xs),
                ) {
                    request.options.forEachIndexed { index, option ->
                        Text(
                            option,
                            style = MaterialTheme.typography.bodySmall,
                            color = clay.accent,
                            modifier = Modifier
                                .fillMaxWidth()
                                .clip(RoundedCornerShape(12.dp))
                                .background(clay.accentSoft)
                                .clickable { onAnswer(index) }
                                .padding(Space.sm),
                        )
                    }
                }
                Spacer(Modifier.height(Space.sm))
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                    TextButton(onClick = { onAnswer(-1) }) {
                        Text(tr("Batal", "Cancel", "キャンセル"))
                    }
                }
            }
        }
    }
}

/** Pratinjau lampiran pada kartu pertanyaan: gambar, video, suara, atau file. */
@Composable
private fun FilePreviewBlock(preview: FilePreview, autoplay: Boolean = false) {
    val context = LocalContext.current
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(clay.surface)
            .padding(Space.sm),
    ) {
        Text(preview.name, style = MaterialTheme.typography.labelSmall, color = clay.muted)
        Spacer(Modifier.height(Space.xs))
        when (preview.kind) {
            "image" -> {
                val bitmap = remember(preview.hostPath) {
                    runCatching { BitmapFactory.decodeFile(preview.hostPath) }.getOrNull()
                }
                if (bitmap != null) {
                    Image(
                        bitmap = bitmap.asImageBitmap(),
                        contentDescription = preview.name,
                        modifier = Modifier
                            .fillMaxWidth()
                            .heightIn(max = 260.dp)
                            .clip(RoundedCornerShape(10.dp)),
                        contentScale = ContentScale.Fit,
                    )
                } else {
                    Text(
                        tr("Pratinjau gambar tidak tersedia", "Image preview unavailable", "画像を表示できません"),
                        style = MaterialTheme.typography.labelSmall,
                        color = clay.danger,
                    )
                }
            }
            "video" -> VideoPreview(preview, autoplay = autoplay)
            "audio" -> {
                val uri = remember(preview.hostPath) { uriFor(context, preview.hostPath) }
                AudioPreview(uri)
            }
            else -> Text(
                tr("File: ", "File: ", "ファイル: ") + preview.name,
                style = MaterialTheme.typography.labelSmall,
                color = clay.accent,
            )
        }
    }
}

/**
 * Pratinjau video: tampilkan frame pertama + tombol putar, lalu ganti dengan
 * VideoView saat diputar. VideoView tanpa tinggi pasti bisa terukur 0 dp,
 * itulah sebabnya pratinjau lama terlihat kosong.
 */
@Composable
private fun VideoPreview(preview: FilePreview, autoplay: Boolean) {
    val context = LocalContext.current
    var playing by remember(preview.hostPath) { mutableStateOf(autoplay) }
    val uri = remember(preview.hostPath) { uriFor(context, preview.hostPath) }

    if (playing) {
        AndroidView(
            factory = { ctx ->
                VideoView(ctx).apply {
                    val controller = MediaController(ctx)
                    controller.setAnchorView(this)
                    setMediaController(controller)
                    setVideoURI(uri)
                    setOnPreparedListener { start() }
                }
            },
            modifier = Modifier
                .fillMaxWidth()
                .height(220.dp)
                .clip(RoundedCornerShape(10.dp)),
        )
    } else {
        val thumbnail = remember(preview.hostPath) {
            runCatching {
                val retriever = MediaMetadataRetriever()
                try {
                    retriever.setDataSource(preview.hostPath)
                    retriever.frameAtTime(1_000_000L) // ~1 detik, bukan frame hitam
                } finally {
                    retriever.release()
                }
            }.getOrNull()
        }
        Box(
            modifier = Modifier
                .fillMaxWidth()
                .height(220.dp)
                .clip(RoundedCornerShape(10.dp))
                .background(Color(0x99000000))
                .clickable { playing = true },
            contentAlignment = Alignment.Center,
        ) {
            if (thumbnail != null) {
                Image(
                    bitmap = thumbnail.asImageBitmap(),
                    contentDescription = preview.name,
                    modifier = Modifier.fillMaxSize(),
                    contentScale = ContentScale.Crop,
                )
            }
            Icon(
                imageVector = Icons.Outlined.PlayCircle,
                contentDescription = tr("Putar video", "Play video", "動画を再生"),
                tint = Color.White,
                modifier = Modifier.size(52.dp),
            )
        }
    }
}

@Composable
private fun AudioPreview(uri: Uri) {
    val context = LocalContext.current
    var playing by remember { mutableStateOf(false) }
    val player = remember { MediaPlayer() }
    DisposableEffect(uri) {
        runCatching {
            player.setDataSource(context, uri)
            player.prepare()
        }
        onDispose { player.release() }
    }
    Button(onClick = {
        runCatching {
            if (playing) player.pause() else player.start()
            playing = !playing
        }
    }) {
        Text(if (playing) tr("Jeda", "Pause", "一時停止") else tr("Putar suara", "Play audio", "音声を再生"))
    }
}

private fun uriFor(context: android.content.Context, hostPath: String): Uri =
    runCatching {
        FileProvider.getUriForFile(
            context,
            "${context.packageName}.files",
            java.io.File(hostPath),
        )
    }.getOrElse {
        // File di rootfs Ubuntu memang di bawah filesDir, tapi kalau path
        // belum terdaftar di file_paths.xml, jangan sampai kartu question
        // crash — VideoView masih bisa membaca file milik aplikasi sendiri.
        Uri.fromFile(java.io.File(hostPath))
    }
