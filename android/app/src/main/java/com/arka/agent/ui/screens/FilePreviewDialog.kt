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
import androidx.compose.material.icons.outlined.Download
import androidx.compose.material.icons.outlined.OpenInNew
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
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
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
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
 * Semua pembacaan file dilakukan di [Dispatchers.IO]; PDF dan gambar
 * didekode dengan batas ukuran supaya file besar tidak membuat aplikasi
 * kehabisan memori.
 */
@Composable
fun FilePreviewDialog(vm: MainViewModel, file: FileInfo, onDismiss: () -> Unit) {
    val context = LocalContext.current
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
        when (file.kind) {
            "text" -> vm.readText(file.path) { result ->
                result.onSuccess { text = it }
                    .onFailure { error = it.message }
                loading = false
            }
            "image" -> {
                bitmap = withContext(Dispatchers.IO) { decodeScaled(hostFile.absolutePath) }
                if (bitmap == null) error = trFor(lang, "Gambar tidak bisa dibaca", "Image could not be read", "画像を読み込めません")
                loading = false
            }
            "pdf" -> {
                val result = withContext(Dispatchers.IO) { renderPdfFirstPage(hostFile) }
                if (result == null) error = trFor(lang, "PDF tidak bisa dibaca", "PDF could not be read", "PDFを読み込めません") else {
                    bitmap = result.first
                    pdfPages = result.second
                }
                loading = false
            }
            else -> loading = false
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Column {
                Text(file.name, style = MaterialTheme.typography.titleMedium, maxLines = 1)
                Text(
                    "${file.kind} · ${humanSize(file.size)}",
                    style = MaterialTheme.typography.labelSmall,
                    color = clay.muted,
                )
            }
        },
        text = {
            Box(Modifier.fillMaxWidth().heightIn(min = 80.dp, max = 420.dp)) {
                when {
                    loading -> CircularProgressIndicator(
                        color = clay.accent,
                        modifier = Modifier.align(Alignment.Center).size(28.dp),
                    )
                    error != null -> Text(
                        error.orEmpty(),
                        style = MaterialTheme.typography.bodySmall,
                        color = clay.danger,
                    )
                    else -> when (file.kind) {
                        "text" -> Column(
                            Modifier
                                .fillMaxSize()
                                .verticalScroll(rememberScrollState())
                                .background(clay.background, RoundedCornerShape(10.dp))
                                .padding(10.dp),
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
                                modifier = Modifier.fillMaxWidth().clip(RoundedCornerShape(10.dp)),
                                contentScale = ContentScale.Fit,
                            )
                        }
                        else -> Column {
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
        },
        confirmButton = {
            Row(horizontalArrangement = Arrangement.spacedBy(Space.xs)) {
                if (file.kind == "video" || file.kind == "binary" || file.kind == "pdf") {
                    TextButton(onClick = {
                        // Buka dengan aplikasi lain lewat FileProvider: URI
                        // content:// adalah satu-satunya yang boleh dibagikan
                        // ke aplikasi lain sejak Android 7.
                        runCatching {
                            val uri = FileProvider.getUriForFile(
                                context,
                                "${context.packageName}.files",
                                hostFile,
                            )
                            val intent = Intent(Intent.ACTION_VIEW).apply {
                                setDataAndType(uri, context.contentResolver.getType(uri) ?: "*/*")
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
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) { Text(tr("Tutup", "Close", "閉じる")) }
        },
    )
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
