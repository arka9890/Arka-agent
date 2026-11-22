package com.arka.agent.ui.screens

import android.content.Intent
import android.graphics.Bitmap
import android.graphics.Color as AndroidColor
import android.graphics.pdf.PdfRenderer
import android.os.ParcelFileDescriptor
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
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
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.FileProvider
import com.arka.agent.MainViewModel
import com.arka.agent.ui.LocalLanguage
import com.arka.agent.ui.tr
import com.arka.agent.ui.trFor
import com.arka.agent.core.FileInfo
import com.arka.agent.ui.components.ClayPill
import com.arka.agent.ui.theme.Space
import com.arka.agent.ui.theme.clay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Dialog preview file di tab File.
 *
 * Full screen: seluruh layar dipakai untuk menampilkan isi file.
 * Card diberi efek tembus pandang 70% (background semi-transparan)
 * agar konten di belakangnya masih samar terlihat, memberi kesan
 * glassmorphism.
 *
 * Semua pembacaan file dilakukan di [Dispatchers.IO]; PDF dan gambar
 * didekode dengan batas ukuran supaya file besar tidak membuat aplikasi
 * kehabisan memori.
 */
@Composable
fun FilePreviewDialog(vm: MainViewModel, file: FileInfo, onDismiss: () -> Unit) {
    val context = LocalContext.current
    // Lampiran SAF berupa URI content://; path workspace lama tetap path biasa.
    val isUri = remember(file.path) { file.path.startsWith("content://") }
    var text by remember(file.path) { mutableStateOf<String?>(null) }
    var bitmap by remember(file.path) { mutableStateOf<Bitmap?>(null) }
    var pdfPages by remember(file.path) { mutableStateOf<Int?>(null) }
    var loading by remember(file.path) { mutableStateOf(true) }
    var error by remember(file.path) { mutableStateOf<String?>(null) }
    val lang = LocalLanguage.current

    val hostFile = remember(file.path) { vm.workspaceFile(file.path) }

    LaunchedEffect(file.path) {
        loading = true
        error = null
        text = null
        bitmap = null
        pdfPages = null

        // Path kosong / bukan file → jangan coba decode (dulu penyebab FC:
        // decodeFile pada direktori mengembalikan null lalu dipaksa render).
        if (file.path.isBlank()) {
            error = trFor(lang, "File tidak ditemukan", "File not found", "ファイルが見つかりません")
            loading = false
            return@LaunchedEffect
        }

        when (file.kind) {
            "text" -> vm.readText(file.path) { result ->
                result.onSuccess { text = it }
                    .onFailure { error = it.message }
                loading = false
            }
            "image" -> {
                bitmap = withContext(Dispatchers.IO) {
                    if (isUri) decodeScaledUri(context, file.path)
                    else if (hostFile.isFile) decodeScaled(hostFile.absolutePath)
                    else null
                }
                if (bitmap == null) error = trFor(lang, "Gambar tidak bisa dibaca", "Image could not be read", "画像を読み込めません")
                loading = false
            }
            "pdf" -> {
                val result = withContext(Dispatchers.IO) {
                    if (isUri) renderPdfFirstPageUri(context, file.path)
                    else if (hostFile.isFile) renderPdfFirstPage(hostFile)
                    else null
                }
                if (result == null) error = trFor(lang, "PDF tidak bisa dibaca", "PDF could not be read", "PDFを読み込めません") else {
                    bitmap = result.first
                    pdfPages = result.second
                }
                loading = false
            }
            else -> loading = false
        }
    }

    // Full-screen dialog: usePlatformDefaultWidth = false membuat
    // dialog memenuhi seluruh layar, bukan hanya 80% lebar seperti
    // AlertDialog bawaan.
    Dialog(
        onDismissRequest = onDismiss,
        properties = DialogProperties(
            usePlatformDefaultWidth = false,
            dismissOnClickOutside = true,
            dismissOnBackPress = true,
        ),
    ) {
        // Root: fillMaxSize supaya benar-benar full screen.
        Box(
            Modifier
                .fillMaxSize()
                .background(Color(0x99000000)), // scrim gelap di belakang card
        ) {
            // Card tembus pandang 70%: background clay dengan alpha 0.7f.
            Column(
                Modifier
                    .fillMaxSize()
                    .padding(12.dp)
                    .background(
                        clay.background.copy(alpha = 0.7f),
                        RoundedCornerShape(16.dp),
                    )
                    .padding(16.dp),
            ) {
                // Header: judul + tombol tutup (full screen).
                Row(
                    Modifier.fillMaxWidth(),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            file.name.ifBlank { tr("Tanpa nama", "Untitled", "無題") },
                            style = MaterialTheme.typography.titleMedium,
                            maxLines = 1,
                        )
                        Text(
                            if (file.size > 0) "${file.kind} · ${humanSize(file.size)}" else file.kind,
                            style = MaterialTheme.typography.labelSmall,
                            color = clay.muted,
                        )
                    }
                    IconButton(onClick = onDismiss) {
                        Icon(Icons.Outlined.Close, tr("Tutup", "Close", "閉じる"), tint = clay.muted)
                    }
                }

                Spacer(Modifier.height(Space.sm))

                // Konten preview: mengisi sisa layar (full screen).
                Box(
                    Modifier
                        .fillMaxSize()
                        .clip(RoundedCornerShape(10.dp)),
                ) {
                    when {
                        loading -> CircularProgressIndicator(
                            color = clay.accent,
                            modifier = Modifier.align(Alignment.Center).size(28.dp),
                        )
                        error != null -> Text(
                            error.orEmpty(),
                            style = MaterialTheme.typography.bodySmall,
                            color = clay.danger,
                            modifier = Modifier.align(Alignment.Center).padding(16.dp),
                        )
                        else -> when (file.kind) {
                            "text" -> Column(
                                Modifier
                                    .fillMaxSize()
                                    .verticalScroll(rememberScrollState())
                                    .background(Color(0x40000000), RoundedCornerShape(10.dp))
                                    .padding(12.dp),
                            ) {
                                Text(
                                    text.orEmpty(),
                                    style = MaterialTheme.typography.bodySmall
                                        .copy(fontFamily = FontFamily.Monospace),
                                )
                            }
                            "image", "pdf" -> bitmap?.let {
                                Image(
                                    bitmap = it.asImageBitmap(),
                                    contentDescription = file.name,
                                    modifier = Modifier.fillMaxSize(),
                                    contentScale = ContentScale.Fit,
                                )
                            }
                            else -> Column(
                                Modifier
                                    .fillMaxSize()
                                    .padding(16.dp),
                                verticalArrangement = Arrangement.Center,
                            ) {
                                Text(
                                    tr("File biner tidak bisa dipratinjau di aplikasi.", "Binary files cannot be previewed in the app.", "バイナリファイルはアプリ内でプレビューできません。"),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = clay.muted,
                                )
                                Spacer(Modifier.height(Space.sm))
                                Text(
                                    tr("Gunakan tombol Unduh untuk membukanya dengan aplikasi lain.", "Use Download to open it with another app.", "ダウンロードボタンで他のアプリから開いてください。"),
                                    style = MaterialTheme.typography.labelSmall,
                                    color = clay.muted,
                                )
                            }
                        }
                    }
                }

                Spacer(Modifier.height(Space.sm))

                // Footer: tombol aksi (full screen).
                Row(
                    Modifier.fillMaxWidth(),
                    horizontalArrangement = Arrangement.spacedBy(Space.xs),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    if (file.kind == "video" || file.kind == "binary" || file.kind == "pdf") {
                        TextButton(onClick = {
                            // Lampiran SAF: URI content:// sudah boleh dibagikan
                            // langsung (diberi izin baca lewat FLAG_GRANT_READ).
                            // File workspace tetap lewat FileProvider.
                            runCatching {
                                val target = if (isUri) {
                                    android.net.Uri.parse(file.path)
                                } else {
                                    FileProvider.getUriForFile(
                                        context,
                                        "${context.packageName}.files",
                                        hostFile,
                                    )
                                }
                                val mime = context.contentResolver.getType(target)
                                    ?: context.contentResolver.getType(android.net.Uri.parse(file.path))
                                    ?: "*/*"
                                val intent = Intent(Intent.ACTION_VIEW).apply {
                                    setDataAndType(target, mime)
                                    addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                                }
                                context.startActivity(intent)
                            }.onFailure { vm.notify(trFor(lang, "Tidak ada aplikasi untuk membuka file ini", "No app can open this file", "このファイルを開くアプリがありません")) }
                        }) {
                            Icon(Icons.Outlined.OpenInNew, null, Modifier.size(16.dp))
                            Spacer(Modifier.width(4.dp))
                            Text(tr("Buka", "Open", "開く"))
                        }
                    }
                    TextButton(onClick = { vm.exportFile(file.path) }) {
                        Icon(Icons.Outlined.Download, null, Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text(tr("Unduh", "Download", "ダウンロード"))
                    }
                    Spacer(Modifier.weight(1f))
                    TextButton(onClick = onDismiss) {
                        Text(tr("Tutup", "Close", "閉じる"))
                    }
                }
            }
        }
    }
}

/** Dekode gambar dengan `inSampleSize` supaya maksimal ±1000px sisi terpanjang. */
private fun decodeScaled(path: String): Bitmap? {
    val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
    android.graphics.BitmapFactory.decodeFile(path, bounds)
    if (bounds.outWidth <= 0) return null
    var sample = 1
    while (bounds.outWidth / sample > 1000 || bounds.outHeight / sample > 1000) {
        sample *= 2
    }
    return android.graphics.BitmapFactory.decodeFile(
        path,
        android.graphics.BitmapFactory.Options().apply { inSampleSize = sample },
    )
}

/**
 * Render halaman pertama PDF ke Bitmap (sesuai PRD: tab File menampilkan
 * halaman pertama). Mengembalikan bitmap + jumlah halaman.
 */
private fun renderPdfFirstPage(file: java.io.File): Pair<Bitmap, Int>? = runCatching {
    val descriptor = ParcelFileDescriptor.open(file, ParcelFileDescriptor.MODE_READ_ONLY)
    PdfRenderer(descriptor).use { renderer ->
        val pages = renderer.pageCount
        if (pages == 0) return null
        renderer.openPage(0).use { page ->
            // Skala 1.5x agar teks masih terbaca tanpa memakan memori besar.
            val width = (page.width * 1.5f).toInt().coerceAtLeast(1)
            val height = (page.height * 1.5f).toInt().coerceAtLeast(1)
            val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
            bitmap.eraseColor(AndroidColor.WHITE)
            page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
            bitmap to pages
        }
    }
}.getOrNull()

/** Versi [decodeScaled] untuk URI SAF: dua pass (bounds lalu decode). */
private fun decodeScaledUri(context: android.content.Context, uri: String): Bitmap? {
    val resolver = context.contentResolver
    val u = android.net.Uri.parse(uri)
    val bounds = android.graphics.BitmapFactory.Options().apply { inJustDecodeBounds = true }
    runCatching {
        resolver.openInputStream(u)?.use {
            android.graphics.BitmapFactory.decodeStream(it, null, bounds)
        }
    }
    if (bounds.outWidth <= 0) return null
    var sample = 1
    while (bounds.outWidth / sample > 1000 || bounds.outHeight / sample > 1000) {
        sample *= 2
    }
    val opts = android.graphics.BitmapFactory.Options().apply { inSampleSize = sample }
    return runCatching {
        resolver.openInputStream(u)?.use {
            android.graphics.BitmapFactory.decodeStream(it, null, opts)
        }
    }.getOrNull()
}

/** Versi [renderPdfFirstPage] untuk URI SAF. */
private fun renderPdfFirstPageUri(context: android.content.Context, uri: String): Pair<Bitmap, Int>? =
    runCatching {
        val descriptor = context.contentResolver.openFileDescriptor(
            android.net.Uri.parse(uri),
            "r",
        ) ?: return null
        PdfRenderer(descriptor).use { renderer ->
            val pages = renderer.pageCount
            if (pages == 0) return null
            renderer.openPage(0).use { page ->
                val width = (page.width * 1.5f).toInt().coerceAtLeast(1)
                val height = (page.height * 1.5f).toInt().coerceAtLeast(1)
                val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
                bitmap.eraseColor(AndroidColor.WHITE)
                page.render(bitmap, null, null, PdfRenderer.Page.RENDER_MODE_FOR_DISPLAY)
                bitmap to pages
            }
        }
    }.getOrNull()
