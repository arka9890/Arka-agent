//! State global UI — meniru `MainViewModel.kt`.
//!
//! Yang disimpan di sini hanya **state tampilan**. Data sungguhan (riwayat
//! chat, provider, setelan) milik `arka_core`; M4 menyambungkannya lewat
//! `AppState::refresh_sessions` / `open_session` / `send`.

use std::path::PathBuf;

use arka_core::file::FileInfo;
use arka_core::paths::AppPaths;

use crate::preview::PreviewState;

use crate::pwsh::Detected;

use super::engine::Engine;
use super::history::History;
use super::jobs::NetJob;
use super::term_job::{ExitLine, Job, JobState, StatsJob, TaskKind, WsStats};
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

/// Satu baris daftar provider di tab Provider — beda dari [`ProviderRow`] yang
/// hanya dipakai dropdown model chat: di sini ada status kunci & statistik.
#[derive(Clone, Debug, PartialEq)]
pub struct ProvRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub base_url: String,
    pub model: String,
    pub is_default: bool,
    /// Sudah ada kunci tersimpan (bukan nilainya — layar ini sering terlihat
    /// orang lain, jadi nilai asli tidak pernah ditampilkan di daftar).
    pub has_key: bool,
    /// Ada kunci tetapi DPAPI mesin ini tidak bisa membukanya.
    pub key_unreadable: bool,
    pub total_tokens: i64,
    pub total_requests: i64,
}

/// Isi form tambah/ubah provider (padanan `ProviderDialog` di Android).
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderForm {
    /// Kosong = provider baru.
    pub id: String,
    pub name: String,
    /// `openai` | `anthropic`
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    /// Model yang dicentang pengguna.
    pub models: Vec<String>,
    pub is_default: bool,
    /// Model yang ditambahkan manual (di luar daftar server).
    pub manual_model: String,
    /// Kunci tersimpan tidak bisa dibuka DPAPI di mesin ini.
    pub key_unreadable: bool,
    /// Tampilkan API key sebagai teks (tombol mata).
    pub reveal_key: bool,
    /// Hasil tes koneksi terakhir.
    pub test_result: Option<String>,
    /// Daftar model dari server (hasil "Cek dari server" atau saat dibuka).
    pub available: Vec<String>,
    /// Kesalahan validasi di dalam form (nama/URL kosong).
    pub error: Option<String>,
}

impl Default for ProviderForm {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            kind: "openai".into(),
            base_url: String::new(),
            api_key: String::new(),
            model: String::new(),
            models: Vec::new(),
            is_default: false,
            manual_model: String::new(),
            key_unreadable: false,
            reveal_key: false,
            test_result: None,
            available: Vec::new(),
            error: None,
        }
    }
}

impl ProviderForm {
    /// Form dari provider tersimpan (kunci sudah dibuka `provider::get`).
    pub fn from_provider(p: &arka_core::provider::Provider) -> Self {
        let mut models = p.models.clone();
        if !p.model.trim().is_empty() && !models.iter().any(|m| m == p.model.trim()) {
            models.insert(0, p.model.trim().to_string());
        }
        models.sort();
        models.dedup();
        Self {
            id: p.id.clone(),
            name: p.name.clone(),
            kind: if p.kind == "anthropic" { "anthropic".into() } else { "openai".into() },
            base_url: p.base_url.clone(),
            api_key: p.api_key.clone(),
            model: p.model.clone(),
            models,
            is_default: p.is_default,
            key_unreadable: p.key_unreadable,
            ..Default::default()
        }
    }

    /// URL yang benar-benar dipakai: kalau kolom kosong, pakai bawaan protokol.
    /// Tanpa ini provider baru langsung gagal karena field kosong.
    pub fn resolved_url(&self) -> String {
        let u = self.base_url.trim();
        if !u.is_empty() {
            return u.trim_end_matches('/').to_string();
        }
        if self.kind == "anthropic" {
            "https://api.anthropic.com".into()
        } else {
            "https://api.openai.com/v1".into()
        }
    }

    /// Semua model yang bisa dipilih: dari server + yang sudah dicentang +
    /// model utama, urut & tanpa duplikat.
    pub fn all_models(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .available
            .iter()
            .chain(self.models.iter())
            .chain(std::iter::once(&self.model))
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// `None` = boleh disimpan.
    pub fn validate(&self, lang: Lang) -> Option<String> {
        if self.name.trim().is_empty() {
            return Some(tr(lang, "Nama provider wajib diisi.", "Provider name is required.", "プロバイダー名は必須です。").to_string());
        }
        if self.kind != "anthropic" && self.kind != "openai" {
            return Some(tr(lang, "Jenis provider tidak dikenal.", "Unknown provider type.", "不明なプロバイダー種別です。").to_string());
        }
        None
    }

    /// Bentuk yang dikirim ke `provider::save`.
    pub fn to_provider(&self) -> arka_core::provider::Provider {
        let mut models = self.models.clone();
        if !self.model.trim().is_empty() && !models.iter().any(|m| m.trim() == self.model.trim()) {
            models.push(self.model.trim().to_string());
        }
        models.sort();
        models.dedup();
        arka_core::provider::Provider {
            id: self.id.clone(),
            name: self.name.trim().to_string(),
            kind: if self.kind == "anthropic" { "anthropic".into() } else { "openai".into() },
            base_url: self.resolved_url(),
            api_key: self.api_key.clone(),
            model: self.model.trim().to_string(),
            models,
            is_default: self.is_default,
            created_at: 0,
            key_unreadable: self.key_unreadable,
        }
    }
}

/// State tab Provider.
#[derive(Default)]
pub struct ProviderState {
    pub rows: Vec<ProvRow>,
    /// Form yang sedang terbuka; `None` = sedang melihat daftar.
    pub form: Option<ProviderForm>,
    /// Pekerjaan jaringan (tes koneksi / ambil daftar model).
    pub job: Option<NetJob>,
    /// Provider yang menunggu konfirmasi hapus: (id, nama).
    pub confirm_delete: Option<(String, String)>,
    /// Kesalahan terakhir dari database (mis. simpan gagal).
    pub error: Option<String>,
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

/// State interaksi tab File (M6).
///
/// Dipisah dari [`FilesCache`] karena daftar berkas berasal dari disk
/// (di-cache), sedangkan dialog/rename/preview murni state UI.
pub struct FilesUi {
    /// Folder yang sedang dibuka, relatif terhadap workspace (`""` = akar).
    pub dir: String,
    /// Berkas yang pratinjaunya terbuka.
    pub preview: Option<PreviewState>,
    /// Rename langsung di baris: `(path relatif, teks yang sedang diketik)`.
    pub rename: Option<(String, String)>,
    /// Isi kotak "Folder baru"; `Some` = dialog terbuka.
    pub new_folder: Option<String>,
    /// Konfirmasi sebelum dipindahkan ke sampah.
    pub confirm_trash: Option<FileInfo>,
    /// Folder yang sedang dijelajah di pemilih impor bawaan. `Some` = terbuka.
    ///
    /// Pemilih ini dipakai bila platform tidak punya dialog OS (Linux saat
    /// pengembangan) — dan tetap berguna sebagai uji CI karena bisa diklik.
    pub picker_dir: Option<PathBuf>,
}

impl Default for FilesUi {
    fn default() -> Self {
        Self {
            dir: String::new(),
            preview: None,
            rename: None,
            new_folder: None,
            confirm_trash: None,
            picker_dir: None,
        }
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
    /// Interaksi tab File (folder aktif, pratinjau, dialog).
    pub files_ui: FilesUi,
    /// Hasil deteksi PowerShell, disimpan supaya tab Terminal tidak
    /// menjalankan proses baru di setiap frame.
    pub shell: ShellProbe,
    /// State tab Chat (pesan, streaming, provider, dialog).
    pub chat: ChatState,
    /// State tab Terminal (kotak perintah, riwayat, output, status mesin).
    pub term: TermState,
    /// State tab Provider (daftar, form, pekerjaan jaringan).
    pub provider: ProviderState,
    /// Kegagalan database terakhir; ditampilkan supaya "tidak ada sesi" tidak
    /// terlihat sama dengan "database rusak".
    pub db_error: Option<String>,
    /// Saluran hasil deteksi PowerShell yang berjalan di thread latar.
    ///
    /// Deteksi **menjalankan proses** (`pwsh -v`, atau `powershell.exe`), jadi
    /// memanggilnya dari `draw` membekukan jendela selama proses itu hidup.
    /// Pola di sini sama dengan pekerjaan lain: thread + saluran + `poll`.
    pub shell_probe_rx: Option<std::sync::mpsc::Receiver<Result<Detected, String>>>,
}

/// Status mesin yang diambil sekali lewat PowerShell (bukan tiap frame).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InfoState {
    #[default]
    Unknown,
    Ready(Vec<(String, String)>),
    Missing(String),
}

impl InfoState {
    pub fn get(&self, key: &str) -> Option<&str> {
        match self {
            InfoState::Ready(pairs) => pairs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
            _ => None,
        }
    }
}

/// Semua state tab Terminal.
///
/// Sengaja dipisah dari [`ChatState`]: `chat.terminal_log` adalah catatan
/// perintah yang dijalankan **agent**, sedangkan output di sini adalah
/// perintah yang **diketik pengguna** dan berjalan di thread terpisah.
#[derive(Default)]
pub struct TermState {
    /// Isi kotak perintah.
    pub input: String,
    pub history: History,
    /// Output perintah manual (terus bertambah selama proses berjalan).
    pub output: String,
    /// Perintah pengguna yang sedang berjalan; `None` = tidak ada.
    pub job: Option<Job>,
    /// Baris exit perintah terakhir.
    pub exit: Option<ExitLine>,
    /// Pengambilan status mesin (versi/OS/arch/policy).
    pub info_job: Option<Job>,
    /// Tombol "Tes shell" yang sedang berjalan.
    pub diag_job: Option<Job>,
    pub info: InfoState,
    /// Info sudah pernah dicoba (berhasil **atau** gagal). Tanpa ini, setiap
    /// frame akan mencoba menjalankan PowerShell lagi.
    pub info_attempted: bool,
    /// Hasil tombol "Tes shell".
    pub diagnostic: Option<String>,
    pub ws: Option<WsStats>,
    pub stats_job: Option<StatsJob>,
    pub focus_input: bool,
    /// Dropdown riwayat perintah sedang terbuka.
    pub show_history: bool,
}

impl TermState {
    pub fn running(&self) -> bool {
        self.job.as_ref().is_some_and(Job::running)
    }
}

/// Hasil pemeriksaan PowerShell.
#[derive(Clone, Debug, Default)]
pub enum ShellProbe {
    /// Belum diperiksa sama sekali.
    #[default]
    Unknown,
    /// Pemeriksaan sedang berjalan di thread latar.
    Probing,
    Ready(Detected),
    Missing(String),
}

impl ShellProbe {
    pub fn checked(&self) -> bool {
        // `Probing` sudah "checked": tanpa ini `ensure_shell_probe` akan
        // memulai pemeriksaan baru di setiap frame.
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
            files_ui: FilesUi::default(),
            shell: ShellProbe::Unknown,
            chat: ChatState {
                auto_scroll: persisted.auto_scroll,
                ..Default::default()
            },
            term: TermState::default(),
            provider: ProviderState::default(),
            db_error: None,
            shell_probe_rx: None,
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
    pub fn ensure_shell_probe(&mut self, ctx: &eframe::egui::Context) {
        // Ambil hasil yang sudah siap lebih dulu.
        if let Some(rx) = &self.shell_probe_rx {
            match rx.try_recv() {
                Ok(Ok(d)) => {
                    self.shell = ShellProbe::Ready(d);
                    self.shell_probe_rx = None;
                }
                Ok(Err(e)) => {
                    self.shell = ShellProbe::Missing(e);
                    self.shell_probe_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Thread mati tanpa mengirim hasil (panic). Tanpa ini,
                    // kartu status berhenti di "Memeriksa…" selamanya.
                    self.shell =
                        ShellProbe::Missing("Deteksi shell berhenti tanpa hasil".to_string());
                    self.shell_probe_rx = None;
                }
            }
        }
        if !self.shell.checked() && self.shell_probe_rx.is_none() {
            self.start_shell_probe(ctx.clone());
        }
    }

    /// Deteksi ulang (tombol "Periksa ulang") — tetap di thread latar.
    pub fn refresh_shell_probe(&mut self, ctx: &eframe::egui::Context) {
        self.shell_probe_rx = None;
        self.shell = ShellProbe::Unknown;
        self.start_shell_probe(ctx.clone());
    }

    fn start_shell_probe(&mut self, ctx: eframe::egui::Context) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.shell = ShellProbe::Probing;
        self.shell_probe_rx = Some(rx);
        std::thread::spawn(move || {
            // `detect()` bisa memakan ratusan milidetik (menjalankan proses).
            // Inilah alasan jalur ini tidak boleh dipanggil dari `draw`.
            let _ = tx.send(crate::pwsh::detect());
            ctx.request_repaint();
        });
    }

    /// Folder kerja agent. Di Windows: `%USERPROFILE%\ArkaAgent\workspace`.
    pub fn workspace(&self) -> PathBuf {
        self.paths.workspace.clone()
    }

    // ── Terminal ───────────────────────────────────────────────────────────

    /// Batas output perintah manual: lebih besar dari `Limits::default()`
    /// karena pengguna sendiri yang menjalankan `dir /s`, bukan model.
    fn term_limits() -> crate::pump::Limits {
        crate::pump::Limits {
            max_chars: crate::pwsh::RAW_OUTPUT_CAP,
            timeout: crate::pwsh::DEFAULT_TIMEOUT,
        }
    }

    /// Shell untuk eksekusi manual, atau pesan galat bila tidak ada.
    fn term_shell(
        &mut self,
        ctx: &eframe::egui::Context,
    ) -> Result<crate::pwsh::PwshShell, String> {
        self.ensure_shell_probe(ctx);
        match &self.shell {
            ShellProbe::Ready(d) => {
                // Workspace mungkin belum pernah dibuat (mis. pengguna baru
                // menghapusnya). Tanpa ini PowerShell gagal dengan pesan
                // "directory does not exist" yang membingungkan.
                let ws = self.workspace();
                std::fs::create_dir_all(&ws)
                    .map_err(|e| format!("Gagal membuat workspace {}: {e}", ws.display()))?;
                Ok(crate::pwsh::PwshShell::with(d.program.clone(), ws, Self::term_limits()))
            }
            ShellProbe::Missing(e) => Err(e.clone()),
            ShellProbe::Probing => Err("Shell masih diperiksa".into()),
            ShellProbe::Unknown => Err("Shell belum diperiksa".into()),
        }
    }

    /// Jalankan isi kotak perintah di thread latar.
    pub fn run_terminal(&mut self, ctx: &eframe::egui::Context) {
        let cmd = self.term.input.trim().to_string();
        if cmd.is_empty() {
            return;
        }
        if self.term.running() {
            self.notify(self.tr(
                "Masih ada perintah berjalan. Tekan Stop dulu.",
                "A command is still running. Press Stop first.",
                "まだ実行中のコマンドがあります。先に Stop を押してください。",
            ));
            return;
        }
        match self.term_shell(ctx) {
            Ok(shell) => {
                // Riwayat dicatat **sebelum** dijalankan: perintah yang gagal
                // spawn pun tetap layak diketik ulang dengan panah ↑.
                self.term.history.push(&cmd);
                self.term.output.clear();
                self.term.exit = None;
                self.term.input.clear();
                self.term.focus_input = true;
                self.term.show_history = false;
                self.term.job = Some(super::term_job::start(TaskKind::Run, cmd, shell, ctx.clone()));
            }
            Err(e) => {
                self.term.output = format!("! {e}");
                self.term.exit = None;
            }
        }
    }

    /// Minta perintah yang sedang berjalan berhenti (tombol Stop).
    pub fn stop_terminal(&mut self) {
        if let Some(job) = &self.term.job {
            job.cancel();
        }
    }

    /// Ambil status mesin sekali saja (setiap panggilan = satu proses).
    pub fn ensure_term_info(&mut self, ctx: &eframe::egui::Context) {
        if self.term.info_attempted || self.term.info_job.is_some() {
            return;
        }
        // Tanpa shell, tidak ada yang bisa ditanyakan — biarkan `Unknown`
        // supaya UI menampilkan "—" alih-alih pesan galat kedua.
        if !matches!(self.shell, ShellProbe::Ready(_)) {
            return;
        }
        self.term.info_attempted = true;
        match self.term_shell(ctx) {
            Ok(shell) => {
                self.term.info_job =
                    Some(super::term_job::start(TaskKind::Info, String::new(), shell, ctx.clone()));
            }
            Err(e) => self.term.info = InfoState::Missing(e),
        }
    }

    /// Tombol "Tes shell": satu proses yang hasilnya ditampilkan apa adanya.
    pub fn run_terminal_diagnostic(&mut self, ctx: &eframe::egui::Context) {
        if self.term.diag_job.is_some() {
            return;
        }
        match self.term_shell(ctx) {
            Ok(shell) => {
                self.term.diagnostic = None;
                self.term.diag_job = Some(super::term_job::start(
                    TaskKind::Diagnostic,
                    String::new(),
                    shell,
                    ctx.clone(),
                ));
            }
            Err(e) => self.term.diagnostic = Some(format!("! {e}")),
        }
    }

    /// Hitung isi workspace sekali (rekursif → harus di thread latar).
    pub fn ensure_workspace_stats(&mut self, ctx: &eframe::egui::Context) {
        if self.term.ws.is_some() || self.term.stats_job.is_some() {
            return;
        }
        self.term.stats_job = Some(super::term_job::start_stats(self.workspace(), ctx.clone()));
    }

    /// Pindahkan hasil pekerjaan terminal yang sudah selesai ke state tampilan.
    ///
    /// Dipanggil sekali per frame dari layar Terminal. `Job` sendiri yang
    /// mengakumulasi output selama berjalan; setelah selesai isinya dipindah ke
    /// [`TermState::output`] supaya `Job` (dan salurannya) bisa dilepas.
    pub fn poll_terminal(&mut self) {
        // Perintah pengguna.
        let mut finished: Option<(String, JobState, Option<ExitLine>)> = None;
        if let Some(job) = self.term.job.as_mut() {
            job.poll();
            if !job.running() {
                finished = Some((job.output.clone(), job.state.clone(), job.exit));
            }
        }
        if let Some((output, state, exit)) = finished {
            self.term.output = output;
            self.term.exit = exit;
            self.term.job = None;
            if let JobState::Failed(e) = state {
                if !self.term.output.is_empty() {
                    self.term.output.push_str("\n\n");
                }
                self.term.output.push_str(&format!("! {e}"));
                self.term.exit = None;
            }
        }

        // Status mesin.
        let mut info: Option<(Option<Vec<(String, String)>>, JobState)> = None;
        if let Some(job) = self.term.info_job.as_mut() {
            job.poll();
            if !job.running() {
                info = Some((job.info.clone(), job.state.clone()));
            }
        }
        if let Some((pairs, state)) = info {
            self.term.info_job = None;
            self.term.info = match (pairs, state) {
                (Some(v), _) => InfoState::Ready(v),
                (None, JobState::Failed(e)) => InfoState::Missing(e),
                _ => InfoState::Unknown,
            };
        }

        // Diagnostik.
        let mut diag: Option<(Option<String>, JobState)> = None;
        if let Some(job) = self.term.diag_job.as_mut() {
            job.poll();
            if !job.running() {
                diag = Some((job.diagnostic.clone(), job.state.clone()));
            }
        }
        if let Some((text, state)) = diag {
            self.term.diag_job = None;
            self.term.diagnostic = match (text, state) {
                (Some(t), _) => Some(t),
                (None, JobState::Failed(e)) => Some(format!("! {e}")),
                _ => None,
            };
        }

        // Ukuran workspace.
        if let Some(job) = self.term.stats_job.as_mut() {
            if let Some(stats) = job.poll() {
                self.term.ws = Some(stats);
            }
        }
    }

    /// "Bersihkan": buang output & hasil tes, bukan riwayat perintah.
    pub fn clear_terminal(&mut self) {
        self.term.output.clear();
        self.term.exit = None;
        self.term.diagnostic = None;
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

// ── Tab Provider ───────────────────────────────────────────────────────────

impl AppState {
    /// Baca ulang daftar provider + statistik pemakaian.
    ///
    /// Gagal baca **tidak** dikosongkan diam-diam: pesan disimpan di
    /// [`ProviderState::error`] supaya "belum ada provider" tidak sama dengan
    /// "database rusak".
    pub fn refresh_provider_rows(&mut self) {
        let list = match arka_core::provider::list() {
            Ok(v) => v,
            Err(e) => {
                self.provider.error = Some(e);
                return;
            }
        };
        let stats = arka_core::provider::get_all_stats().unwrap_or_default();
        self.provider.rows = list
            .iter()
            .map(|p| {
                let st = stats.iter().find(|(id, _)| id == &p.id).map(|(_, s)| s.clone());
                ProvRow {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    kind: p.kind.clone(),
                    base_url: p.base_url.clone(),
                    model: p.model.clone(),
                    is_default: p.is_default,
                    has_key: !p.api_key.trim().is_empty() || p.key_unreadable,
                    key_unreadable: p.key_unreadable,
                    total_tokens: st.as_ref().map(|s| s.total_tokens).unwrap_or(0),
                    total_requests: st.as_ref().map(|s| s.total_requests).unwrap_or(0),
                }
            })
            .collect();
        self.provider.error = None;
        // Dropdown model di tab Chat memakai daftar yang sama.
        self.refresh_providers();
    }

    /// Buka form: `None` = provider baru, `Some(id)` = ubah.
    pub fn open_provider_form(&mut self, id: Option<&str>) {
        self.provider.form = Some(match id {
            None => ProviderForm::default(),
            Some(id) => match arka_core::provider::get(id) {
                Ok(p) => {
                    let mut f = ProviderForm::from_provider(&p);
                    f.available = p.models.clone();
                    f
                }
                Err(e) => {
                    self.provider.error = Some(e);
                    return;
                }
            },
        });
    }

    /// Simpan form yang terbuka. `true` = berhasil (form ditutup).
    pub fn save_provider_form(&mut self) -> bool {
        let Some(form) = self.provider.form.clone() else {
            return false;
        };
        if let Some(msg) = form.validate(self.lang) {
            if let Some(f) = self.provider.form.as_mut() {
                f.error = Some(msg);
            }
            return false;
        }
        match arka_core::provider::save(form.to_provider()) {
            Ok(_) => {
                self.provider.form = None;
                self.provider.error = None;
                self.refresh_provider_rows();
                self.notify(self.tr(
                    "Provider disimpan. API key terenkripsi (DPAPI).",
                    "Provider saved. API key encrypted (DPAPI).",
                    "プロバイダーを保存しました。APIキーは DPAPI で暗号化されます。",
                ));
                true
            }
            Err(e) => {
                if let Some(f) = self.provider.form.as_mut() {
                    f.error = Some(e);
                }
                false
            }
        }
    }

    pub fn delete_provider(&mut self, id: &str) {
        self.provider.confirm_delete = None;
        if let Err(e) = arka_core::provider::delete(id) {
            self.provider.error = Some(e);
            return;
        }
        self.refresh_provider_rows();
        self.notify(self.tr("Provider dihapus.", "Provider deleted.", "プロバイダーを削除しました。"));
    }

    pub fn set_default_provider(&mut self, id: &str) {
        if let Err(e) = arka_core::provider::set_default(id) {
            self.provider.error = Some(e);
            return;
        }
        self.refresh_provider_rows();
    }

    /// Jalankan tes koneksi / ambil daftar model di thread latar.
    ///
    /// Sengaja memakai **isi form** (bukan yang tersimpan) supaya pengguna bisa
    /// mengetes kunci sebelum menyimpannya.
    pub fn start_provider_job(&mut self, kind: super::jobs::JobKind, ctx: &eframe::egui::Context) {
        if self.provider.job.as_ref().map(|j| j.running()).unwrap_or(false) {
            return; // satu pekerjaan pada satu waktu
        }
        let Some(form) = self.provider.form.clone() else {
            return;
        };
        let mut p = form.to_provider();
        // Provider baru belum punya id; jangan sampai mengirim string kosong.
        if p.id.is_empty() {
            p.id = "draft".into();
        }
        self.provider.job = Some(super::jobs::start(kind, p, ctx.clone()));
    }

    /// Periksa hasil pekerjaan jaringan (dipanggil sekali per frame).
    pub fn poll_provider_job(&mut self) {
        let Some(job) = self.provider.job.as_mut() else {
            return;
        };
        let kind = job.kind;
        let Some(result) = job.poll() else {
            return;
        };
        self.provider.job = None;
        let lang = self.lang;
        let Some(form) = self.provider.form.as_mut() else {
            return;
        };
        match (kind, result) {
            (_, Err(e)) => form.test_result = Some(e),
            (super::jobs::JobKind::Test, Ok(super::jobs::JobDone::Message(m))) => {
                form.test_result = Some(m)
            }
            (super::jobs::JobKind::Models, Ok(super::jobs::JobDone::Models(models))) => {
                // Model yang sudah dipilih tidak dibuang: daftar server hanya
                // menambah pilihan.
                form.available = models;
                form.test_result = Some(
                    tr(lang, "Daftar model diperbarui dari server.", "Model list refreshed from server.", "モデル一覧をサーバーから更新しました。").to_string(),
                );
            }
            (_, Ok(_)) => {}
        }
    }
}
