package com.arka.agent.ui.screens

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Description
import androidx.compose.material.icons.outlined.PictureAsPdf
import androidx.compose.material.icons.outlined.PlayCircle
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.arka.agent.core.Message
import com.arka.agent.ui.tr
import com.arka.agent.ui.theme.clay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import java.io.File

/** Data lampiran yang disimpan di kolom `attachments` pesan. */
private data class Attachment(val path: String, val kind: String)

private fun parseAttachments(json: String): List<Attachment> = runCatching {
    val arr = JSONArray(json)
    (0 until arr.length()).map { i ->
        val o = arr.getJSONObject(i)
        Attachment(o.optString("path"), o.optString("kind", "text"))
    }
}.getOrDefault(emptyList())

/**
 * Bubble pesan.
 *
 * User: rata kanan, warna aksen. AI: rata kiri, permukaan clay.
 * `streaming = true` menambahkan kursor berkedip di akhir teks.
 */
@Composable
fun MessageBubble(
    message: Message,
    streaming: Boolean = false,
    resolveFile: (String) -> File = { File(it) },
    onOpenFile: (String) -> Unit = {},
) {
    val isUser = message.role == "user"
    val attachments = remember(message.attachments) { parseAttachments(message.attachments) }

    Row(
        modifier = Modifier.fillMaxWidth(),
        horizontalArrangement = if (isUser) Arrangement.End else Arrangement.Start,
    ) {
        Column(
            modifier = Modifier
                .widthIn(max = 320.dp)
                .clip(
                    RoundedCornerShape(
                        topStart = 20.dp,
                        topEnd = 20.dp,
                        bottomStart = if (isUser) 20.dp else 6.dp,
                        bottomEnd = if (isUser) 6.dp else 20.dp,
                    ),
                )
                .background(if (isUser) clay.userBubble else clay.aiBubble)
                .padding(horizontal = 14.dp, vertical = 10.dp),
        ) {
            if (attachments.isNotEmpty()) {
                attachments.forEach { att ->
                    AttachmentPreview(att, resolveFile, onOpenFile)
                    Spacer(Modifier.height(6.dp))
                }
            }
            MarkdownText(
                text = message.content,
                color = if (isUser) Color.White else clay.onSurface,
                showCursor = streaming,
            )
        }
    }
}

/**
 * Preview lampiran di dalam bubble.
 *
 * Gambar dirender sebagai thumbnail (dibatasi 200x200 sesuai PRD), teks
 * ditampilkan beberapa baris pertama, video/PDF/binary hanya metadata +
 * tombol — file besar tidak boleh didekode ke memori di tengah chat.
 */
@Composable
private fun AttachmentPreview(
    att: Attachment,
    resolveFile: (String) -> File,
    onOpenFile: (String) -> Unit,
) {
    // Path lampiran relatif terhadap workspace, bukan path absolut proses —
    // tanpa resolver ini File(path) selalu "tidak ditemukan".
    val file = remember(att.path) { resolveFile(att.path) }
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(Color.Black.copy(alpha = 0.18f))
            .clickable { onOpenFile(att.path) }
            .padding(8.dp),
    ) {
        when (att.kind) {
            "image" -> ImageThumbnail(file)
            "video" -> Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.PlayCircle, "Putar video", tint = Color.White)
                Spacer(Modifier.width(8.dp))
                Text(
                    att.path.substringAfterLast('/'),
                    style = MaterialTheme.typography.bodySmall,
                    color = Color.White,
                )
            }
            "pdf" -> Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.PictureAsPdf, "PDF", tint = Color.White)
                Spacer(Modifier.width(8.dp))
                Text(
                    att.path.substringAfterLast('/'),
                    style = MaterialTheme.typography.bodySmall,
                    color = Color.White,
                )
            }
            else -> Row(verticalAlignment = Alignment.CenterVertically) {
                Icon(Icons.Outlined.Description, tr("File", "File", "ファイル"), tint = Color.White)
                Spacer(Modifier.width(8.dp))
                Column {
                    Text(
                        att.path.substringAfterLast('/'),
                        style = MaterialTheme.typography.bodySmall,
                        color = Color.White,
                    )
                    Text(
                        if (file.exists()) "${file.length() / 1024} KB" else tr(
                            "tidak ditemukan", "not found", "見つかりません",
                        ),
                        style = MaterialTheme.typography.labelSmall,
                        color = Color.White.copy(alpha = 0.7f),
                    )
                }
            }
        }
    }
}

@Composable
private fun ImageThumbnail(file: File) {
    var bitmap by remember(file.path) { mutableStateOf<android.graphics.Bitmap?>(null) }

    LaunchedEffect(file.path) {
        bitmap = withContext(Dispatchers.IO) {
            runCatching {
                val path = file.path
                if (!file.isFile) return@runCatching null
                // Decode dengan inSampleSize supaya gambar 4000px tidak
                // menghabiskan RAM hanya untuk thumbnail 200px.
                val bounds = android.graphics.BitmapFactory.Options().apply {
                    inJustDecodeBounds = true
                }
                android.graphics.BitmapFactory.decodeFile(path, bounds)
                var sample = 1
                while (bounds.outWidth / sample > 400 || bounds.outHeight / sample > 400) {
                    sample *= 2
                }
                android.graphics.BitmapFactory.decodeFile(
                    path,
                    android.graphics.BitmapFactory.Options().apply { inSampleSize = sample },
                )
            }.getOrNull()
        }
    }

    val image = bitmap
    if (image == null) {
        Text(
            tr("Gambar tidak bisa dibaca", "Image could not be read", "画像を読み込めません"),
            style = MaterialTheme.typography.labelSmall,
            color = Color.White.copy(alpha = 0.8f),
        )
    } else {
        androidx.compose.foundation.Image(
            bitmap = image.asImageBitmap(),
            contentDescription = "Lampiran gambar",
            modifier = Modifier
                .fillMaxWidth()
                .aspectRatio(image.width.toFloat() / image.height.toFloat())
                .clip(RoundedCornerShape(10.dp)),
            contentScale = ContentScale.Fit,
        )
    }
}

/**
 * Markdown minimal: blok kode (```), `inline code`, **tebal**, dan *miring*.
 *
 * Tidak memakai library markdown: PRD hanya butuh code block + bold/italic,
 * dan parser ini menghindari satu dependensi + satu Compose-renderer besar.
 */
@Composable
fun MarkdownText(
    text: String,
    color: Color,
    showCursor: Boolean = false,
) {
    val blocks = remember(text) { splitCodeBlocks(text) }
    Column {
        blocks.forEach { block ->
            if (block.isCode) {
                CodeBlock(block.text)
            } else {
                Text(
                    text = inlineMarkdown(block.text),
                    style = MaterialTheme.typography.bodyMedium,
                    color = color,
                )
            }
        }
        if (showCursor) {
            Text("▍", color = clay.accent, style = MaterialTheme.typography.bodyMedium)
        }
    }
}

private data class Block(val text: String, val isCode: Boolean, val lang: String = "")

private fun splitCodeBlocks(text: String): List<Block> {
    if (!text.contains("```")) return listOf(Block(text, false))
    val out = mutableListOf<Block>()
    val parts = text.split("```")
    parts.forEachIndexed { index, part ->
        if (index % 2 == 0) {
            if (part.isNotEmpty()) out += Block(part, false)
        } else {
            val lang = part.lineSequence().firstOrNull()?.trim().orEmpty()
            val body = part.removePrefix(lang).trimStart('\n')
            out += Block(body.trimEnd('\n'), true, lang)
        }
    }
    return out
}

@Composable
private fun CodeBlock(code: String) {
    Box(
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .clip(RoundedCornerShape(10.dp))
            .background(Color.Black.copy(alpha = 0.35f))
            .padding(10.dp),
    ) {
        Text(
            text = code,
            style = MaterialTheme.typography.bodySmall.copy(fontFamily = FontFamily.Monospace),
            color = Color(0xFFE6E6F0),
        )
    }
}

/** Terapkan **tebal**, *miring*, dan `kode` ke satu paragraf. */
private fun inlineMarkdown(text: String): AnnotatedString = buildAnnotatedString {
    var i = 0
    while (i < text.length) {
        val rest = text.substring(i)
        when {
            rest.startsWith("**") && rest.indexOf("**", 2) > 0 -> {
                val end = rest.indexOf("**", 2)
                withStyle(SpanStyle(fontWeight = FontWeight.Bold)) {
                    append(rest.substring(2, end))
                }
                i += end + 2
            }
            rest.startsWith("`") && rest.indexOf('`', 1) > 0 -> {
                val end = rest.indexOf('`', 1)
                withStyle(SpanStyle(fontFamily = FontFamily.Monospace)) {
                    append(rest.substring(1, end))
                }
                i += end + 1
            }
            rest.startsWith("*") && rest.indexOf('*', 1) > 0 -> {
                val end = rest.indexOf('*', 1)
                withStyle(SpanStyle(fontStyle = androidx.compose.ui.text.font.FontStyle.Italic)) {
                    append(rest.substring(1, end))
                }
                i += end + 1
            }
            else -> {
                append(text[i])
                i++
            }
        }
    }
}
