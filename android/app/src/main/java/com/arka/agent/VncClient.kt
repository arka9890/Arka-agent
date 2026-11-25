package com.arka.agent

import android.graphics.Bitmap
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.nio.charset.StandardCharsets
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.max

/** Satu frame yang siap digambar Compose. Revision naik tiap update. */
data class VncFrame(val bitmap: Bitmap, val revision: Long)

/**
 * Klien RFB 3.8 minimalis untuk desktop Ubuntu di proot.
 *
 * Sengaja ditulis sendiri: tidak ada dependency VNC yang bisa diandalkan
 * tersedia di Maven, dan server yang kita pakai (Xvnc) bisa dipaksa memakai
 * encoding Raw + CopyRect lewat SetEncodings dari klien. Itu cukup untuk
 * desktop penuh dan jauh lebih ringan daripada membundel noVNC + websockify.
 *
 * Semua baca/socket terjadi di thread sendiri; UI hanya menerima frame dan
 * mengirim input lewat method [pointer], [key], [typeText].
 */
class VncClient(
    private val host: String,
    private val port: Int,
    private val onFrame: (VncFrame) -> Unit,
    private val onState: (String) -> Unit,
) {
    private val stopped = AtomicBoolean(false)
    private val sendLock = Any()
    @Volatile private var output: DataOutputStream? = null
    @Volatile private var socket: Socket? = null
    @Volatile private var thread: Thread? = null

    @Volatile private var width = 0
    @Volatile private var height = 0
    @Volatile private var pixels = IntArray(0)
    @Volatile private var revision = 0L

    // Pixel format dari ServerInit, dipakai saat decode Raw.
    @Volatile private var bytesPerPixel = 4
    @Volatile private var bigEndian = false
    @Volatile private var rShift = 16
    @Volatile private var gShift = 8
    @Volatile private var bShift = 0
    @Volatile private var rMax = 255
    @Volatile private var gMax = 255
    @Volatile private var bMax = 255

    fun start() {
        if (thread != null) return
        stopped.set(false)
        thread = Thread({ run() }, "arka-vnc-reader").apply { isDaemon = true; start() }
    }

    fun stop() {
        stopped.set(true)
        try { socket?.close() } catch (_: Exception) {}
        output = null
        thread?.interrupt()
        thread = null
    }

    // ------------------------------------------------------------ handshake

    private fun run() {
        try {
            onState("Menghubungkan ke desktop…")
            val s = Socket()
            socket = s
            s.tcpNoDelay = true
            s.connect(InetSocketAddress(host, port), 8_000)
            // Jangan pernah menunggu selamanya: kalau server mengirim data
            // setengah, UI harus menampilkan error, bukan "Menghubungkan…"
            // selama-lamanya.
            s.soTimeout = 20_000
            val input = DataInputStream(s.getInputStream())
            val out = DataOutputStream(s.getOutputStream())
            output = out

            val hello = ByteArray(12)
            input.readFully(hello)
            val version = String(hello, StandardCharsets.US_ASCII).trim()
            out.write("RFB 003.008\n".toByteArray(StandardCharsets.US_ASCII))
            out.flush()

            onState("Negosiasi keamanan…")
            val oldProtocol = version.contains("003.003")
            val count = input.readUnsignedByte()
            if (count == 0) throw IllegalStateException("Server tidak menawarkan security type")
            val types = ByteArray(count)
            input.readFully(types)
            // Xvnc kita jalankan dengan -SecurityTypes None. Kalau nanti ada
            // password, jangan menebak: beri pesan yang jujur ke pengguna.
            if (!types.contains(1)) {
                throw IllegalStateException("Server meminta password; sesi desktop perlu di-start ulang")
            }
            out.write(1) // SecurityType = None
            out.flush()
            if (!oldProtocol) {
                if (input.readInt() != 0) throw IllegalStateException("Negosiasi keamanan gagal")
            }

            out.write(1) // ClientInit: shared session
            out.flush()

            onState("Menerima info layar…")
            // ServerInit header = 24 byte:
            //   width(2), height(2), pixelFormat(16), nameLength(4)
            // diikuti nameLength byte nama.asal. NameLength ADA DI DALAM
            // header (offset 20) — jangan membaca 4 byte lagi dari stream.
            val init = ByteArray(24)
            input.readFully(init)
            width = u16(init, 0)
            height = u16(init, 2)
            bytesPerPixel = max(1, (init[4].toInt() and 0xff) / 8)
            bigEndian = (init[6].toInt() and 0xff) != 0
            rMax = u16(init, 8)
            gMax = u16(init, 10)
            bMax = u16(init, 12)
            rShift = init[14].toInt() and 0xff
            gShift = init[15].toInt() and 0xff
            bShift = init[16].toInt() and 0xff
            val nameLen = u32(init, 20) and 0xFFFF
            if (nameLen > 0 && nameLen < 65_536) skipFully(input, nameLen.toLong())

            pixels = IntArray(width * height)

            // Minta encoding sederhana yang kita mengerti. Tanpa ini Xvnc
            // mengirim Tight/ZRLE yang tidak bisa didekode viewer ini.
            send { o ->
                o.writeByte(2) // SetEncodings
                o.writeByte(0)
                o.writeShort(2)
                o.writeInt(0) // Raw
                o.writeInt(1) // CopyRect
            }
            requestUpdate(false)
            onState("Terhubung · ${width}×${height}")

            while (!stopped.get()) {
                // Timeout saat idle bukan error: desktop yang tidak berubah
                // memang bisa lama tidak mengirim pesan. Minta update lagi.
                val type = try {
                    input.read()
                } catch (e: java.net.SocketTimeoutException) {
                    requestUpdate(true)
                    continue
                }
                if (type < 0) break
                when (type) {
                    0 -> readFramebufferUpdate(input)
                    1 -> { // SetColourMapEntries: 3 padding, first, count, RGB tiap entry
                        skipFully(input, 3)
                        input.readUnsignedShort() // first colour
                        val n = input.readUnsignedShort()
                        skipFully(input, n.toLong() * 6)
                    }
                    2 -> Unit // Bell
                    3 -> { // ServerCutText
                        skipFully(input, 3)
                        skipFully(input, readU32(input).toLong())
                    }
                    else -> throw IllegalStateException("Pesan RFB tidak dikenal: $type")
                }
            }
        } catch (e: Exception) {
            if (!stopped.get()) onState("Gagal: ${e.message ?: e.javaClass.simpleName}")
        } finally {
            output = null
            try { socket?.close() } catch (_: Exception) {}
            socket = null
            if (!stopped.get()) onState("Terputus")
        }
    }

    private fun readFramebufferUpdate(input: DataInputStream) {
        skipFully(input, 1) // padding
        val rectangles = input.readUnsignedShort()
        var changed = false
        repeat(rectangles) {
            if (stopped.get()) return
            val x = input.readUnsignedShort()
            val y = input.readUnsignedShort()
            val w = input.readUnsignedShort()
            val h = input.readUnsignedShort()
            val encoding = input.readInt()
            when (encoding) {
                0 -> { // Raw
                    val buf = ByteArray(w * h * bytesPerPixel)
                    input.readFully(buf)
                    var off = 0
                    for (row in 0 until h) {
                        val dy = y + row
                        if (dy !in 0 until height) {
                            off += w * bytesPerPixel
                            continue
                        }
                        for (col in 0 until w) {
                            val dx = x + col
                            val raw = if (bytesPerPixel == 4) {
                                if (bigEndian) readIntBE(buf, off) else readIntLE(buf, off)
                            } else {
                                var v = 0
                                for (k in 0 until bytesPerPixel) {
                                    v = (v shl 8) or (buf[off + k].toInt() and 0xff)
                                }
                                v
                            }
                            off += bytesPerPixel
                            if (dx !in 0 until width) continue
                            val r = (raw shr rShift) and rMax
                            val g = (raw shr gShift) and gMax
                            val b = (raw shr bShift) and bMax
                            pixels[dy * width + dx] =
                                (0xFF shl 24) or (r shl 16) or (g shl 8) or b
                        }
                    }
                    changed = true
                }
                1 -> { // CopyRect: 4 byte srcX/srcY
                    val sx = input.readUnsignedShort()
                    val sy = input.readUnsignedShort()
                    for (row in 0 until h) {
                        val dy = y + row
                        val from = sy + row
                        if (dy !in 0 until height || from !in 0 until height) continue
                        for (col in 0 until w) {
                            val dx = x + col
                            val src = sx + col
                            if (dx in 0 until width && src in 0 until width) {
                                pixels[dy * width + dx] = pixels[from * width + src]
                            }
                        }
                    }
                    changed = true
                }
                else -> throw IllegalStateException("Encoding RFB $encoding tidak didukung")
            }
        }
        if (changed && !stopped.get()) publishFrame()
        // Minta update berikutnya supaya server terus mengirim perubahan.
        requestUpdate(true)
    }

    private fun publishFrame() {
        val bmp = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        bmp.setPixels(pixels.copyOf(), 0, width, 0, 0, width, height)
        onFrame(VncFrame(bmp, ++revision))
    }

    private fun requestUpdate(incremental: Boolean) {
        send { o ->
            o.writeByte(3)
            o.writeByte(if (incremental) 1 else 0)
            o.writeShort(0)
            o.writeShort(0)
            o.writeShort(width)
            o.writeShort(height)
        }
    }

    // ---------------------------------------------------------------- input

    /** buttonMask: bit 0 = klik kiri, bit 1 = tengah, bit 2 = kanan. */
    fun pointer(x: Int, y: Int, buttonMask: Int) {
        if (width == 0) return
        send { o ->
            o.writeByte(5)
            o.writeByte(0)
            o.writeShort(buttonMask)
            o.writeShort(x.coerceIn(0, width - 1))
            o.writeShort(y.coerceIn(0, height - 1))
        }
    }

    fun key(keysym: Int, down: Boolean) {
        if (keysym == 0) return
        send { o ->
            o.writeByte(4)
            o.writeByte(if (down) 1 else 0)
            o.writeShort(0)
            o.writeInt(keysym)
        }
    }

    /** Ketik teks dari kolom input (fallback kalau keyboard fisik tidak ada). */
    fun typeText(text: String) {
        for (ch in text) {
            val sym = when {
                ch == '\n' -> 0xFF0D
                ch == '\t' -> 0xFF09
                ch == '\b' -> 0xFF08
                ch.code in 0x20..0x7E -> ch.code
                else -> 0x01000000 + ch.code // Unicode keysym X11
            }
            key(sym, true)
            key(sym, false)
        }
    }

    fun keyPress(keysym: Int) {
        key(keysym, true)
        key(keysym, false)
    }

    private inline fun send(block: (DataOutputStream) -> Unit) {
        synchronized(sendLock) {
            output?.let { out ->
                try {
                    block(out)
                    out.flush()
                } catch (_: Exception) {}
            }
        }
    }
}

private fun u16(b: ByteArray, i: Int): Int =
    ((b[i].toInt() and 0xff) shl 8) or (b[i + 1].toInt() and 0xff)

private fun u32(b: ByteArray, i: Int): Long =
    ((b[i].toLong() and 0xff) shl 24) or
        ((b[i + 1].toLong() and 0xff) shl 16) or
        ((b[i + 2].toLong() and 0xff) shl 8) or
        (b[i + 3].toLong() and 0xff)

private fun readU32(input: DataInputStream): Long {
    val b = ByteArray(4)
    input.readFully(b)
    return ((b[0].toLong() and 0xff) shl 24) or
        ((b[1].toLong() and 0xff) shl 16) or
        ((b[2].toLong() and 0xff) shl 8) or
        (b[3].toLong() and 0xff)
}

private fun skipFully(input: DataInputStream, n: Long) {
    var sisa = n
    val buf = ByteArray(4096)
    while (sisa > 0) {
        val ambil = input.read(buf, 0, minOf(buf.size.toLong(), sisa).toInt())
        if (ambil < 0) throw java.io.EOFException()
        sisa -= ambil
    }
}

private fun readIntBE(b: ByteArray, i: Int): Int =
    ((b[i].toInt() and 0xff) shl 24) or
        ((b[i + 1].toInt() and 0xff) shl 16) or
        ((b[i + 2].toInt() and 0xff) shl 8) or
        (b[i + 3].toInt() and 0xff)

private fun readIntLE(b: ByteArray, i: Int): Int =
    (b[i].toInt() and 0xff) or
        ((b[i + 1].toInt() and 0xff) shl 8) or
        ((b[i + 2].toInt() and 0xff) shl 16) or
        ((b[i + 3].toInt() and 0xff) shl 24)
