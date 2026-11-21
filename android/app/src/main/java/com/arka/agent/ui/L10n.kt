package com.arka.agent.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf

/** Bahasa UI aktif, diisi MainActivity dari pengaturan `language`. */
val LocalLanguage = staticCompositionLocalOf { "id" }

/**
 * Ambil teks UI sesuai bahasa yang dipilih di Setelan.
 *
 * Pemanggil wajib menuliskan ketiga versi supaya perubahan bahasa langsung
 * terasa — tidak ada lagi teks yang "disimpan tapi tidak dipakai".
 */
@Composable
fun tr(id: String, en: String, ja: String): String = trFor(LocalLanguage.current, id, en, ja)

/**
 * Versi tanpa `@Composable`, untuk dipakai di callback biasa (onClick,
 * onFailure, LaunchedEffect) yang tidak boleh memanggil fungsi composable.
 * Panggil dengan `LocalLanguage.current` yang dibaca di scope composable.
 */
fun trFor(lang: String, id: String, en: String, ja: String): String = when (lang) {
    "en" -> en
    "ja" -> ja
    else -> id
}
