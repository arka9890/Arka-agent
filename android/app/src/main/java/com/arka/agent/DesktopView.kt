package com.arka.agent

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.view.View
import kotlin.math.min

/**
 * Permukaan desktop VNC: menggambar frame dan menerima sentuhan.
 *
 * Memakai View biasa (bukan Compose Canvas + pointerInput) karena api
 `awaitPointerEventScope` berbeda antar versi Compose dan tidak stabil di
 seluruh toolchain yang dipakai CI. Di sini gambar + koordinat sentuh dihitung
 di satu tempat sehingga tidak bisa meleset saat layar dipotong.
 */
class DesktopView(context: Context) : View(context) {
    var bitmap: Bitmap? = null
        set(value) {
            field = value
            invalidate()
        }

    private val paint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)

    private fun geometry(): FloatArray {
        val bmp = bitmap ?: return floatArrayOf(0f, 0f, 0f, 0f)
        val scale = min(width.toFloat() / bmp.width, height.toFloat() / bmp.height)
        val w = bmp.width * scale
        val h = bmp.height * scale
        return floatArrayOf(scale, (width - w) / 2f, (height - h) / 2f, w)
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val bmp = bitmap ?: return
        val g = geometry()
        val h = bmp.height * g[0]
        val dst = RectF(g[1], g[2], g[1] + g[3], g[2] + h)
        canvas.drawBitmap(bmp, null, dst, paint)
    }

    // Touch sengaja ditangani di Compose (lihat DesktopViewer). Event yang
    // masuk ke AndroidView bisa direbut LazyColumn sebagai scroll, sehingga
    // jari bergerak tapi PointerEvent RFB tidak pernah dikirim.
}
