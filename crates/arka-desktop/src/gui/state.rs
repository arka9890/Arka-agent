//! State global UI — meniru `MainViewModel.kt`.
//!
//! Yang disimpan di sini hanya **state tampilan**. Data sungguhan (riwayat
//! chat, provider, setelan) milik `arka_core`; M4 menyambungkannya lewat
//! `AppState::refresh_sessions` / `open_session` / `send`.

use std::path::PathBuf;

use arka_core::paths::AppPaths;

use crate::pwsh::Detected;

use super::engine::Engine;
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionRow {
    pub id: String,
    pub title: String,
    pub subtitle: String,
}

/// Satu pesan yang siap digambar `bubble.rs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MsgView {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: i64,
    pub attachments: Vec<String>,
}

impl MsgView {
    pub fn from_core(m: arka_core::session::Message) -> Self {
        Self {
            id: m.id,
            role: m.role,
            content: m.content,
            created_at: m.created_at,
            attachments: parse_attachments(&m.attachments),
        }
    }

    pub fn is_user(&self) -> bool {
        self.role == "user"
    }
}

/// Ambil nama file dari kolom `attachments` (`[{path,kind,name}]`).
///
/// Dibuat toleran: kalau JSON-nya rusak (versi lama, kolom kosong), tampilkan
/// kosong — bukan membuat seluruh riwayat gagal dibaca.
pub fn parse_attachments(raw: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return Vec::new();
    };
    let Some(arr) = v.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .map(|x| {
            x.get("name")
                .and_then(|n| n.as_str())
                .or_else(|| x.get("path").and_then(|p| p.as_str()))
                .unwrap_or("?")
                .to_string()
        })
        .collect()
}

/// Lampiran yang menempel di kotak pesan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub path: String,
    pub name: String,
    /// `text` | `image` | `video` | `audio` | `file`
    pub kind: String,
}

impl Attachment {
    /// JSON yang disimpan di kolom `attachments` — bentuknya sama dengan
    /// `AttachmentRef.toJson()` di Android.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({ "path": self.path, "kind": self.kind, "name": self.name })
    }
}

/// Satu provider untuk dropdown model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderRow {
    pub id: String,
    pub name: String,
    pub model: String,
    pub models: Vec<String>,
}

/// Kartu persetujuan tool skill yang sedang menunggu jawaban.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApprovalCard {
    pub id: String,
    pub action: String,
    pub target: String,
    pub detail: String,
}

/// Kartu pertanyaan pilihan ganda (`ask_user`).
#[derive(Clone, Debug, PartialEq)]
pub struct QuestionCard {
    pub id: String,
    pub question: String,
    pub options: Vec<String>,
    pub files: Vec<serde_json::Value>,
}

/// Semua state tab Chat.
pub struct ChatState {
    pub messages: Vec<MsgView>,
    /// Jawaban yang sedang mengalir (buffer per sesi aktif).
    pub streaming: String,
    pub providers: Vec<ProviderRow>,
    pub provider_id: String,
    pub model: String,
    /// Teks pencarian di dropdown model.
    pub model_query: String,
    pub auto_scroll: bool,
    pub approval: Option<ApprovalCard>,
    pub question: Option<QuestionCard>,
    pub attachments: Vec<Attachment>,
    pub engine: Engine,
    /// Log perintah agent, ditampilkan di tab Terminal.
    pub terminal_log: String,
    /// Sesi yang judulnya sedang diubah langsung di sidebar.
    pub rename: Option<(String, String)>,
    pub focus_input: bool,
    /// Daftar sesi yang selesai saat jendela tidak fokus → badge "baru".
    pub unread: std::collections::HashSet<String>,
    /// Folder yang sedang dibuka di pemilih berkas lampiran.
    pub pick_dir: PathBuf,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            streaming: String::new(),
            providers: Vec::new(),
            provider_id: String::new(),
            model: String::new(),
            model_query: String::new(),
            auto_scroll: true,
            approval: None,
            question: None,
            attachments: Vec::new(),
            engine: Engine::new(),
            terminal_log: String::new(),
            rename: None,
            focus_input: false,
            unread: std::collections::HashSet::new(),
            pick_dir: PathBuf::new(),
        }
    }
}

/// Yang disimpan permanen antar-jalankan (`eframe::Storage`).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Persisted {
    pub theme: ThemeMode,
    pub font_scale: FontScale,
    pub lang: Lang,
    pub sidebar_open: bool,
    pub tab: TabSerde,
    #[serde(default = "ya")]
    pub auto_scroll: bool,
}

fn ya() -> bool {
    true
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            theme: ThemeMode::System,
            font_scale: FontScale::Medium,
            lang: Lang::Id,
            sidebar_open: true,
            tab: TabSerde(Tab::Chat),
            auto_scroll: true,
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
    pub active_session: Option<String>,
    /// Isi kotak pesan yang belum dikirim.
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
    /// State tab Chat (pesan, streaming, provider, dialog).
    pub chat: ChatState,
    /// Kegagalan database terakhir; ditampilkan supaya "tidak ada sesi" tidak
    /// terlihat sama dengan "database rusak".
    pub db_error: Option<String>,
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
pub enum Modal {
    #[default]
    None,
    About,
    ConfirmDelete(String),
    /// Pemilih berkas lampiran (di dalam aplikasi).
    PickFile,
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
            chat: ChatState {
                auto_scroll: persisted.auto_scroll,
                ..Default::default()
            },
            db_error: None,
        }
    }

    pub fn persist(&self) -> Persisted {
        Persisted {
            theme: self.theme,
            font_scale: self.font_scale,
            lang: self.lang,
            sidebar_open: self.sidebar_open,
            tab: TabSerde(self.tab),
            auto_scroll: self.chat.auto_scroll,
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

    // ── Data (arka_core) ────────────────────────────────────────────────────

    /// Baca ulang daftar sesi & provider dari database.
    ///
    /// Kegagalan **tidak** membuat UI panik: pesannya disimpan di
    /// [`AppState::db_error`] supaya terlihat di layar. Ini penting karena
    /// test UI berjalan tanpa database — dan harus tetap bisa menggambar.
    pub fn refresh_sessions(&mut self) {
        match arka_core::session::list() {
            Ok(list) => {
                self.sessions = list
                    .into_iter()
                    .map(|s| SessionRow {
                        subtitle: format!(
                            "{} pesan \u{00B7} {}",
                            s.message_count,
                            s.model.as_deref().unwrap_or("default")
                        ),
                        id: s.id,
                        title: s.title,
                    })
                    .collect();
                self.db_error = None;
            }
            Err(e) => self.db_error = Some(e),
        }
    }

    /// Baca ulang provider + pilih default kalau belum ada yang dipilih.
    pub fn refresh_providers(&mut self) {
        let Ok(list) = arka_core::provider::list() else {
            return;
        };
        self.chat.providers = list
            .iter()
            .map(|p| ProviderRow {
                id: p.id.clone(),
                name: p.name.clone(),
                model: p.model.clone(),
                models: p.models.clone(),
            })
            .collect();

        if self.chat.providers.iter().all(|p| p.id != self.chat.provider_id) {
            let def = list.iter().find(|p| p.is_default).or_else(|| list.first());
            match def {
                Some(p) => {
                    self.chat.provider_id = p.id.clone();
                    if self.chat.model.is_empty() {
                        self.chat.model = p.model.clone();
                    }
                }
                None => {
                    self.chat.provider_id.clear();
                }
            }
        }
    }

    /// Buka sesi: muat riwayat, provider, dan model yang dipakai sesi itu.
    pub fn open_session(&mut self, id: String) {
        self.active_session = Some(id.clone());
        self.chat.streaming.clear();
        self.chat.unread.remove(&id);
        self.chat.messages = match arka_core::session::messages(&id) {
            Ok(v) => v.into_iter().map(MsgView::from_core).collect(),
            Err(e) => {
                self.db_error = Some(e);
                Vec::new()
            }
        };
        if let Ok(s) = arka_core::session::get(&id) {
            if let Some(pid) = s.provider_id {
                if self.chat.providers.iter().any(|p| p.id == pid) {
                    self.chat.provider_id = pid;
                }
            }
            if let Some(m) = s.model {
                self.chat.model = m;
            }
        }
    }

    /// Baca ulang pesan sesi yang sedang dibuka dari database.
    ///
    /// Dipanggil saat agent selesai: teks final sudah disimpan oleh core, dan
    /// buffer streaming hanyalah salinan sementara.
    pub fn reload_messages(&mut self) {
        self.chat.streaming.clear();
        let Some(id) = self.active_session.clone() else {
            return;
        };
        match arka_core::session::messages(&id) {
            Ok(v) => self.chat.messages = v.into_iter().map(MsgView::from_core).collect(),
            Err(e) => self.db_error = Some(e),
        }
    }

    pub fn create_session(&mut self) {
        let provider = if self.chat.provider_id.is_empty() {
            None
        } else {
            Some(self.chat.provider_id.clone())
        };
        let model = if self.chat.model.is_empty() {
            None
        } else {
            Some(self.chat.model.clone())
        };
        match arka_core::session::create(
            &self.tr("Sesi baru", "New session", "新しいセッション"),
            provider.as_deref(),
            model.as_deref(),
        ) {
            Ok(s) => {
                self.refresh_sessions();
                self.open_session(s.id);
                self.chat.focus_input = true;
            }
            Err(e) => self.notify(e),
        }
    }

    pub fn delete_session(&mut self, id: &str) {
        if let Err(e) = arka_core::session::delete(id) {
            self.notify(e);
            return;
        }
        if self.active_session.as_deref() == Some(id) {
            self.active_session = None;
            self.chat.messages.clear();
            self.chat.streaming.clear();
        }
        self.refresh_sessions();
    }

    pub fn rename_session(&mut self, id: &str, title: &str) {
        let t = title.trim();
        if t.is_empty() {
            return;
        }
        if let Err(e) = arka_core::session::rename(id, t) {
            self.notify(e);
            return;
        }
        self.refresh_sessions();
    }

    /// Judul sesi yang sedang dibuka (untuk header chat).
    pub fn active_title(&self) -> Option<String> {
        let id = self.active_session.as_ref()?;
        self.sessions
            .iter()
            .find(|s| &s.id == id)
            .map(|s| s.title.clone())
    }
}
