//! State global UI — meniru `MainViewModel.kt`.
//!
//! Yang disimpan di sini hanya **state tampilan**. Data sungguhan (riwayat
//! chat, provider, setelan) tetap milik `arka_core`; M4 menyambungkannya lewat
//! `AppState::attach`. Sampai saat itu, field-nya masih placeholder yang jelas
//! ditandai `TODO(M4)` supaya tidak ada yang mengira ini sudah berfungsi.

use std::path::PathBuf;

use arka_core::paths::AppPaths;

use crate::pwsh::Detected;

use super::fs_model::FilesCache;
use super::i18n::{tr, Lang};
use super::theme::{FontScale, ThemeMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Chat,
    Provider,
    File,
    Terminal,
    Settings,
}

impl Tab {
    pub const ALL: [Tab; 5] = [
        Tab::Chat,
        Tab::Provider,
        Tab::File,
        Tab::Terminal,
        Tab::Settings,
    ];

    /// Label tiga bahasa, sama seperti `enum Tab` di `AppScreen.kt`.
    pub fn label(self, lang: Lang) -> String {
        match self {
            Tab::Chat => tr(lang, "Chat", "Chat", "チャット"),
            Tab::Provider => tr(lang, "Provider", "Provider", "プロバイダー"),
            Tab::File => tr(lang, "File", "Files", "ファイル"),
            Tab::Terminal => tr(lang, "Terminal", "Terminal", "ターミナル"),
            Tab::Settings => tr(lang, "Setelan", "Settings", "設定"),
        }
    }

    /// Ikon digambar dari glyph, bukan aset — satu lingkaran kecil dengan
    /// huruf khas per tab supaya jelas di sidebar sempit.
    pub fn glyph(self) -> &'static str {
        match self {
            Tab::Chat => "\u{1F4AC}",
            Tab::Provider => "\u{1F517}",
            Tab::File => "\u{1F4C1}",
            Tab::Terminal => "\u{25B6}",
            Tab::Settings => "\u{2699}",
        }
    }
}

/// Satu baris di drawer sesi.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionRow {
    pub id: i64,
    pub title: String,
    pub subtitle: String,
}

/// Yang disimpan permanen antar-jalankan (`eframe::Storage`).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Persisted {
    pub theme: ThemeMode,
    pub font_scale: FontScale,
    pub lang: Lang,
    pub sidebar_open: bool,
    pub tab: TabSerde,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            font_scale: FontScale::Medium,
            lang: Lang::Id,
            sidebar_open: true,
            tab: TabSerde(Tab::Chat),
        }
    }
}

/// Pembungkus supaya `Tab` bisa diserialisasi tanpa menambah derive di enum UI.
#[derive(Clone, Copy, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct TabSerde(#[serde(with = "tab_codec")] pub Tab);

mod tab_codec {
    use super::Tab;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(t: &Tab, s: S) -> Result<S::Ok, S::Error> {
        let v = match t {
            Tab::Chat => "chat",
            Tab::Provider => "provider",
            Tab::File => "file",
            Tab::Terminal => "terminal",
            Tab::Settings => "settings",
        };
        v.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Tab, D::Error> {
        let s = String::deserialize(d)?;
        Ok(match s.as_str() {
            "provider" => Tab::Provider,
            "file" => Tab::File,
            "terminal" => Tab::Terminal,
            "settings" => Tab::Settings,
            _ => Tab::Chat,
        })
    }
}

pub struct AppState {
    pub tab: Tab,
    pub theme: ThemeMode,
    pub font_scale: FontScale,
    pub lang: Lang,
    pub sidebar_open: bool,

    pub paths: AppPaths,
    pub sessions: Vec<SessionRow>,
    pub active_session: Option<i64>,
    /// Isi kotak pesan yang belum dikirim (M4 yang mengirimnya).
    pub draft: String,

    /// Pesan singkat di bawah layar (padanan `SnackbarHostState`).
    pub notice: Option<String>,
    /// Modal yang sedang terbuka (padanan dialog Compose).
    pub modal: Modal,
    /// Isi folder kerja, dibaca ulang hanya saat diminta (bukan tiap frame).
    pub files: FilesCache,
    /// Hasil deteksi PowerShell, disimpan supaya tab Terminal tidak
    /// menjalankan proses baru di setiap frame.
    pub shell: ShellProbe,
}

/// Hasil pemeriksaan PowerShell.
#[derive(Clone, Debug, Default)]
pub enum ShellProbe {
    #[default]
    Unknown,
    Ready(Detected),
    Missing(String),
}

impl ShellProbe {
    pub fn checked(&self) -> bool {
        !matches!(self, ShellProbe::Unknown)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)] // `ConfirmDelete` dipakai setelah M4 punya hapus sesi sungguhan.
pub enum Modal {
    #[default]
    None,
    About,
    ConfirmDelete(i64),
}

impl AppState {
    pub fn new(paths: AppPaths, persisted: Persisted) -> Self {
        Self {
            tab: persisted.tab.0,
            theme: persisted.theme,
            font_scale: persisted.font_scale,
            lang: persisted.lang,
            sidebar_open: persisted.sidebar_open,
            paths,
            sessions: Vec::new(),
            active_session: None,
            draft: String::new(),
            notice: None,
            modal: Modal::None,
            files: FilesCache::new(),
            shell: ShellProbe::Unknown,
        }
    }

    pub fn persist(&self) -> Persisted {
        Persisted {
            theme: self.theme,
            font_scale: self.font_scale,
            lang: self.lang,
            sidebar_open: self.sidebar_open,
            tab: TabSerde(self.tab),
        }
    }

    pub fn tr(&self, id: &str, en: &str, ja: &str) -> String {
        tr(self.lang, id, en, ja)
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some(text.into());
    }

    /// Deteksi PowerShell sekali saja (mahal: menjalankan proses).
    pub fn ensure_shell_probe(&mut self) {
        if !self.shell.checked() {
            self.refresh_shell_probe();
        }
    }

    pub fn refresh_shell_probe(&mut self) {
        self.shell = match crate::pwsh::detect() {
            Ok(d) => ShellProbe::Ready(d),
            Err(e) => ShellProbe::Missing(e),
        };
    }

    /// Folder kerja agent. Di Windows: `%USERPROFILE%\ArkaAgent\workspace`.
    pub fn workspace(&self) -> PathBuf {
        self.paths.workspace.clone()
    }
}
