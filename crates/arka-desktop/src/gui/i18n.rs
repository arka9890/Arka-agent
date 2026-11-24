//! Port `android/.../ui/L10n.kt`.
//!
//! Aturannya sama dan sengaja dipertahankan: setiap teks UI **wajib** ditulis
//! tiga bahasa di tempat pemakaian. Tidak ada tabel terpisah yang bisa "lupa
//! diisi" — kalau ada teks baru dan versi en/ja-nya belum ada, kode tidak
//! mengompilasi.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Lang {
    #[default]
    Id,
    En,
    Ja,
}

#[allow(dead_code)] // `key`/`from_key` dipakai saat bahasa disimpan ke `arka_core` (M8).
impl Lang {
    pub const ALL: [Lang; 3] = [Lang::Id, Lang::En, Lang::Ja];

    pub fn key(self) -> &'static str {
        match self {
            Lang::Id => "id",
            Lang::En => "en",
            Lang::Ja => "ja",
        }
    }

    pub fn from_key(key: &str) -> Lang {
        match key {
            "en" => Lang::En,
            "ja" => Lang::Ja,
            _ => Lang::Id,
        }
    }

    /// Bentuk ringkas untuk modul yang tidak boleh bergantung pada UI
    /// (`jobs::Lang3`).
    pub fn short(self) -> crate::gui::jobs::Lang3 {
        match self {
            Lang::Id => crate::gui::jobs::Lang3::Id,
            Lang::En => crate::gui::jobs::Lang3::En,
            Lang::Ja => crate::gui::jobs::Lang3::Ja,
        }
    }

    /// Nama bahasa ditulis dalam bahasanya sendiri (kebiasaan UI Android).
    pub fn native(self) -> &'static str {
        match self {
            Lang::Id => "Bahasa Indonesia",
            Lang::En => "English",
            Lang::Ja => "日本語",
        }
    }
}

/// Padanan `trFor(lang, id, en, ja)`.
pub fn tr(lang: Lang, id: &str, en: &str, ja: &str) -> String {
    match lang {
        Lang::En => en.to_owned(),
        Lang::Ja => ja.to_owned(),
        Lang::Id => id.to_owned(),
    }
}
