package com.arka.agent

import android.app.Application
import com.arka.agent.core.Arka

class ArkaApp : Application() {

    override fun onCreate() {
        super.onCreate()
        // Inisialisasi native (database + folder). Gagal di sini berarti
        // aplikasi tidak bisa dipakai sama sekali, jadi kegagalannya disimpan
        // dan ditampilkan di UI alih-alih membuat aplikasi crash saat start.
        initError = runCatching {
            Arka.nativeInit(filesDir.absolutePath, applicationInfo.nativeLibraryDir)
        }.exceptionOrNull()?.message

        // Job chat hidup di scope Application, bukan di ViewModel, supaya
        // tidak ikut mati saat Activity dibuat ulang.
        ChatEngine.init(this)
    }

    companion object {
        /** Pesan error inisialisasi native; null kalau normal. */
        var initError: String? = null
    }
}
