//! Setelan yang **disimpan ke tabel `settings`** (bukan hanya ke storage egui).
//!
//! M3–M7 menyimpan tema/bahasa/ukuran teks lewat `eframe::Storage`. Itu cukup
//! untuk bertahan antar restart, tapi bukan tempat yang benar untuk setelan
//! yang juga dibaca oleh **agent** (batas putaran tool, anggaran konteks, batas
//! output, system prompt). Nilai-nilai itu harus ada di database yang sama
//! dengan versi Android — kalau tidak, desktop dan Android punya dua kebenaran.
//!
//! Jadi pembagiannya:
//!   * [`Prefs`] → angka & teks yang dipakai `arka_core` (tabel `settings`)
//!   * `Persisted` (eframe) → tetap dipakai sebagai **cadangan** kalau database
//!     gagal dibuka, dan untuk urusan tampilan murni (tab terakhir, sidebar).
//!
//! Semua fungsi di sini **aman tanpa database**: unit test UI berjalan tanpa
//! `db::init`, dan pembacaan jatuh ke [`defaults`] sementara penulisan diabaikan.
//! Perilaku itu disengaja — beda dengan menyembunyikan error penyimpanan saat
//! database sungguhan ada ([`save`] melaporkannya).

use arka_core::settings as core;

/// Batas satu kolom angka + nilai bawaannya.
///
/// `min`/`max` diambil dari `arka_core` supaya UI **tidak bisa** menawarkan
/// rentang yang berbeda dengan yang akan dijepit agent — angka 200.000 di
/// kolom putaran tool dulu berarti "disimpan lalu dipotong diam-diam".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumSpec {
    pub min: u64,
    pub max: u64,
    pub default: u64,
}

impl NumSpec {
    pub const ROUNDS: NumSpec = NumSpec {
        min: 1,
        max: core::MAX_TOOL_ROUNDS_LIMIT as u64,
        default: core::DEFAULT_MAX_TOOL_ROUNDS as u64,
    };
    pub const CONTEXT: NumSpec = NumSpec {
        min: core::CONTEXT_BUDGET_MIN as u64,
        max: core::CONTEXT_BUDGET_MAX as u64,
        default: core::DEFAULT_CONTEXT_BUDGET as u64,
    };
    pub const OUTPUT: NumSpec = NumSpec {
        min: core::TOOL_OUTPUT_CHARS_MIN as u64,
        max: core::TOOL_OUTPUT_CHARS_MAX as u64,
        default: core::DEFAULT_TOOL_OUTPUT_CHARS as u64,
    };
    pub const TIMEOUT: NumSpec = NumSpec {
        min: core::CHAT_TIMEOUT_MIN,
        max: core::CHAT_TIMEOUT_MAX,
        default: core::DEFAULT_CHAT_TIMEOUT_SECS,
    };
    /// Batas waktu **satu perintah PowerShell** (bukan satu balasan chat).
    pub const TOOL_TIMEOUT: NumSpec = NumSpec {
        min: core::TOOL_TIMEOUT_MIN,
        max: core::TOOL_TIMEOUT_MAX,
        default: core::DEFAULT_TOOL_TIMEOUT_SECS,
    };

    /// Periksa teks yang diketik pengguna.
    ///
    /// Kesalahannya dikembalikan sebagai **jenis**, bukan kalimat, karena
    /// kalimatnya harus tiga bahasa dan itu tugas lapisan UI (`i18n`).
    pub fn parse(&self, raw: &str) -> Result<u64, NumError> {
        let t = raw.trim();
        if t.is_empty() {
            return Err(NumError::Empty);
        }
        let n = t.parse::<u64>().map_err(|_| NumError::NotNumber)?;
        if n < self.min {
            return Err(NumError::TooSmall);
        }
        if n > self.max {
            return Err(NumError::TooLarge);
        }
        Ok(n)
    }
}

/// Jenis kesalahan isian angka (diterjemahkan di layar Setelan).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumError {
    Empty,
    NotNumber,
    TooSmall,
    TooLarge,
}

fn default_str(key: &str) -> String {
    core::defaults().get(key).cloned().unwrap_or_default()
}

/// Baca satu kunci, pakai bawaan kalau belum pernah disimpan.
fn get_or(key: &str) -> String {
    core::get(key).unwrap_or_else(|| default_str(key))
}

fn flag(key: &str) -> bool {
    match core::get(key) {
        Some(v) => v.trim() != "false",
        None => default_str(key).trim() != "false",
    }
}

/// Nilai pilihan tampilan yang sah.
///
/// Dipakai juga sebagai **validasi klik**: tombol yang menghasilkan nilai di
/// luar daftar ini tidak diterapkan. Tanpa itu, `Lang::from_key` diam-diam
/// jatuh ke bahasa Indonesia dan pengguna melihat "Bahasa tidak berubah" tanpa
/// penjelasan.
pub const THEME_KEYS: [&str; 3] = ["system", "dark", "light"];
pub const LANG_KEYS: [&str; 3] = ["id", "en", "ja"];
pub const FONT_KEYS: [&str; 3] = ["small", "medium", "large"];

/// Setelan yang hidup di tabel `settings`.
///
/// Kolom angka disimpan sebagai **teks**, bukan angka, supaya isian yang belum
/// sah (mis. pengguna sedang menghapus lalu mengetik ulang) tidak hilang saat
/// frame berikutnya digambar. Validasi terjadi saat hendak dipakai/ditulis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// `"system"` | `"dark"` | `"light"` — dibaca `MainActivity.kt` juga, jadi
    /// ejaannya tidak boleh berbeda dengan versi Android.
    pub theme: String,
    /// `"id"` | `"en"` | `"ja"`.
    pub language: String,
    /// `"small"` | `"medium"` | `"large"`.
    pub font_size: String,
    pub notifications: bool,
    pub background_run: bool,
    pub auto_summary: bool,
    pub max_tool_rounds: String,
    pub context_budget: String,
    pub tool_output_chars: String,
    pub chat_timeout_secs: String,
    pub tool_timeout_secs: String,
    /// Kosong = pakai prompt bawaan platform (`prompt::active_system_prompt`).
    pub system_prompt: String,
    pub workspace_path: String,
}

impl Default for Prefs {
    /// Bawaan = bawaan `arka_core`, bukan angka yang ditulis ulang di sini.
    fn default() -> Self {
        Self {
            theme: get_or("theme"),
            language: get_or("language"),
            font_size: get_or("font_size"),
            notifications: flag("notifications"),
            background_run: flag("background_run"),
            auto_summary: flag("auto_summary"),
            max_tool_rounds: get_or("max_tool_rounds"),
            context_budget: get_or("context_budget"),
            tool_output_chars: get_or("tool_output_chars"),
            chat_timeout_secs: get_or("chat_timeout_secs"),
            tool_timeout_secs: get_or("tool_timeout_secs"),
            system_prompt: get_or("system_prompt"),
            workspace_path: get_or("workspace_path"),
        }
    }
}

impl Prefs {
    /// Baca semua kunci dari database (atau bawaan kalau belum ada).
    pub fn load() -> Self {
        Self::default()
    }

    /// Tulis semua kunci ke database.
    ///
    /// Kolom angka yang **tidak sah** tidak ditulis: nilai lama dipertahankan.
    /// Menulis hasil jepitan akan membuat kotak teks dan isi database berbeda
    /// tanpa penjelasan — pengguna melihat "5000" padahal tersimpan "200".
    ///
    /// Tanpa database (unit test) fungsi ini tidak melakukan apa pun dan
    /// mengembalikan `Ok(())`.
    pub fn save(&self) -> Result<(), String> {
        if !arka_core::db::available() {
            return Ok(());
        }
        // Kunci tampilan hanya ditulis kalau nilainya dikenal: menyimpan
        // "gelap" (nama enum) akan membuat versi Android jatuh ke default.
        for (key, value, allowed) in [
            ("theme", self.theme.as_str(), THEME_KEYS),
            ("language", self.language.as_str(), LANG_KEYS),
            ("font_size", self.font_size.as_str(), FONT_KEYS),
        ] {
            if allowed.contains(&value) {
                core::set(key, value)?;
            }
        }
        core::set("notifications", bool_str(self.notifications))?;
        core::set("background_run", bool_str(self.background_run))?;
        core::set("auto_summary", bool_str(self.auto_summary))?;
        core::set("system_prompt", &self.system_prompt)?;
        core::set("workspace_path", &self.workspace_path)?;

        for (key, spec, raw) in [
            ("max_tool_rounds", NumSpec::ROUNDS, &self.max_tool_rounds),
            ("context_budget", NumSpec::CONTEXT, &self.context_budget),
            (
                "tool_output_chars",
                NumSpec::OUTPUT,
                &self.tool_output_chars,
            ),
            (
                "chat_timeout_secs",
                NumSpec::TIMEOUT,
                &self.chat_timeout_secs,
            ),
            (
                "tool_timeout_secs",
                NumSpec::TOOL_TIMEOUT,
                &self.tool_timeout_secs,
            ),
        ] {
            if let Ok(n) = spec.parse(raw) {
                core::set(key, &n.to_string())?;
            }
        }
        Ok(())
    }

    /// Kembalikan semua kolom ke bawaan (tombol "Kembalikan ke bawaan").
    pub fn reset(&mut self) {
        let d = Prefs {
            theme: "system".into(),
            language: "id".into(),
            font_size: "medium".into(),
            notifications: true,
            background_run: false,
            auto_summary: true,
            max_tool_rounds: NumSpec::ROUNDS.default.to_string(),
            context_budget: NumSpec::CONTEXT.default.to_string(),
            tool_output_chars: NumSpec::OUTPUT.default.to_string(),
            chat_timeout_secs: NumSpec::TIMEOUT.default.to_string(),
            tool_timeout_secs: NumSpec::TOOL_TIMEOUT.default.to_string(),
            system_prompt: String::new(),
            workspace_path: self.workspace_path.clone(),
        };
        *self = d;
    }
}

fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nilai_tampilan_hanya_yang_dikenal_android() {
        let p = Prefs::load();
        assert!(THEME_KEYS.contains(&p.theme.as_str()));
        assert!(LANG_KEYS.contains(&p.language.as_str()));
        assert!(FONT_KEYS.contains(&p.font_size.as_str()));
        // Nama enum Rust tidak boleh bocor ke database bersama.
        assert!(!THEME_KEYS.contains(&"Dark"));
    }

    #[test]
    fn reset_mengembalikan_tampilan_ke_bawaan_android() {
        let mut p = Prefs {
            theme: "dark".into(),
            language: "ja".into(),
            font_size: "large".into(),
            ..Default::default()
        };
        p.reset();
        assert_eq!(p.theme, "system");
        assert_eq!(p.language, "id");
        assert_eq!(p.font_size, "medium");
    }

    #[test]
    fn spec_menolak_isian_tidak_sah() {
        assert_eq!(NumSpec::ROUNDS.parse(""), Err(NumError::Empty));
        assert_eq!(NumSpec::ROUNDS.parse("abc"), Err(NumError::NotNumber));
        assert_eq!(NumSpec::ROUNDS.parse("0"), Err(NumError::TooSmall));
        assert_eq!(NumSpec::ROUNDS.parse("300"), Err(NumError::TooLarge));
        assert_eq!(NumSpec::ROUNDS.parse(" 40 "), Ok(40));

        assert_eq!(NumSpec::CONTEXT.parse("3999"), Err(NumError::TooSmall));
        assert_eq!(NumSpec::CONTEXT.parse("128001"), Err(NumError::TooLarge));
        assert_eq!(NumSpec::OUTPUT.parse("999"), Err(NumError::TooSmall));
        assert_eq!(NumSpec::OUTPUT.parse("20001"), Err(NumError::TooLarge));
        assert_eq!(NumSpec::TIMEOUT.parse("59"), Err(NumError::TooSmall));
        assert_eq!(NumSpec::TIMEOUT.parse("7201"), Err(NumError::TooLarge));
        assert_eq!(NumSpec::TOOL_TIMEOUT.parse("9"), Err(NumError::TooSmall));
        assert_eq!(NumSpec::TOOL_TIMEOUT.parse("3601"), Err(NumError::TooLarge));
    }

    #[test]
    fn tanpa_database_bawaan_tetap_masuk_akal() {
        let p = Prefs::load();
        // Bawaan harus selalu lolos validasi rentang UI — kalau tidak,
        // tab Setelan akan menampilkan "Terlalu kecil" tepat setelah dibuka.
        assert_eq!(NumSpec::ROUNDS.parse(&p.max_tool_rounds), Ok(40));
        assert_eq!(NumSpec::CONTEXT.parse(&p.context_budget), Ok(24_000));
        assert_eq!(NumSpec::OUTPUT.parse(&p.tool_output_chars), Ok(4_000));
        assert_eq!(NumSpec::TIMEOUT.parse(&p.chat_timeout_secs), Ok(900));
        assert_eq!(NumSpec::TOOL_TIMEOUT.parse(&p.tool_timeout_secs), Ok(120));
        assert!(p.notifications);
        assert!(!p.background_run, "tutup jendela harus benar-benar menutup");
        assert!(p.auto_summary);
        assert!(p.system_prompt.is_empty(), "kosong = prompt bawaan");
        // Tanpa DB, menyimpan bukan error — hanya tidak ada yang tersimpan.
        assert!(p.save().is_ok());
    }

    #[test]
    fn reset_hanya_mengembalikan_setelan_agent_bukan_workspace() {
        let mut p = Prefs {
            max_tool_rounds: "7".into(),
            tool_timeout_secs: "33".into(),
            notifications: false,
            background_run: true,
            system_prompt: "kustom".into(),
            workspace_path: "D:/kerja".into(),
            ..Default::default()
        };
        p.reset();
        assert_eq!(p.max_tool_rounds, "40");
        assert_eq!(p.tool_timeout_secs, "120");
        assert!(p.notifications);
        assert!(!p.background_run);
        assert!(p.system_prompt.is_empty());
        assert_eq!(
            p.workspace_path, "D:/kerja",
            "folder kerja bukan setelan agent"
        );
    }
}
