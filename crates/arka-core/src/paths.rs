//! Lokasi folder aplikasi, per platform, tanpa ketergantungan JNI.
//!
//! Dulu modul ini bernama `ctx` dan hanya bisa diisi dari Kotlin lewat
//! `nativeInit`; path Ubuntu/proot ikut hardcode di dalamnya. Sekarang isinya
//! satu struct [`AppPaths`] yang diisi oleh **pemanggil** (Android atau
//! desktop), sehingga crate ini bisa dipakai binary Windows tanpa JNI.
//!
//! Bagian yang hanya ada di Android (`rootfs_dir`, `native_lib_dir`) memakai
//! `Option` — di Windows nilainya `None`, dan modul yang membutuhkannya
//! (proot) tidak ikut dicompile.

use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

/// Platform tempat core ini berjalan. Dipakai untuk memilih prompt & shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Android,
    Windows,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Android => "android",
            Platform::Windows => "windows",
        }
    }

    /// Nama shell yang dipakai `run_command` di platform ini (untuk prompt).
    pub fn shell_name(self) -> &'static str {
        match self {
            Platform::Android => "bash",
            Platform::Windows => "PowerShell",
        }
    }
}

/// Semua folder yang dipakai aplikasi.
#[derive(Clone, Debug)]
pub struct AppPaths {
    pub platform: Platform,
    /// Folder data: database, unduhan.
    pub data_dir: PathBuf,
    /// Folder cache sementara.
    pub cache_dir: PathBuf,
    /// Folder kerja agent (= isi `/root/storage` di Android).
    pub workspace: PathBuf,
    /// Folder skill (`~/.claude/skills`).
    pub skills_dir: PathBuf,
    /// Folder config skill (`~/.config/<nama>/config.json`).
    pub skills_config_dir: PathBuf,
    /// Folder library native di APK — hanya Android.
    pub native_lib_dir: Option<PathBuf>,
    /// Rootfs Ubuntu — hanya Android.
    pub rootfs_dir: Option<PathBuf>,
}

impl AppPaths {
    /// Path Android: `<filesDir>` + workspace + rootfs proot.
    pub fn android(files_dir: PathBuf, native_lib_dir: PathBuf) -> Self {
        let rootfs = files_dir.join("distro/ubuntu/rootfs");
        Self {
            platform: Platform::Android,
            data_dir: files_dir.clone(),
            cache_dir: files_dir.join("downloads"),
            workspace: files_dir.join("workspace"),
            // Skill tinggal di dalam rootfs supaya `run_command` (bash) dan
            // aplikasi melihat file yang sama.
            skills_dir: rootfs.join("root/.claude/skills"),
            skills_config_dir: rootfs.join("root/.config"),
            native_lib_dir: Some(native_lib_dir),
            rootfs_dir: Some(rootfs),
        }
    }

    /// Path desktop Windows: data di `%APPDATA%`, workspace di profil user.
    ///
    /// [home] adalah `%USERPROFILE%`, [data] adalah `%APPDATA%\ArkaAgent`.
    pub fn windows(home: PathBuf, data: PathBuf) -> Self {
        Self {
            platform: Platform::Windows,
            cache_dir: home.join("ArkaAgent/cache"),
            workspace: home.join("ArkaAgent/workspace"),
            // Kompatibel dengan folder skill Claude Code yang mungkin sudah
            // dimiliki pengguna.
            skills_dir: home.join(".claude/skills"),
            skills_config_dir: home.join(".config"),
            data_dir: data,
            native_lib_dir: None,
            rootfs_dir: None,
        }
    }
}

static PATHS: OnceLock<AppPaths> = OnceLock::new();

/// Folder kerja yang dipilih pengguna di tab Setelan.
///
/// Disimpan terpisah dari [`PATHS`] karena workspace dibaca **setelah**
/// database terbuka (setelannya ada di tabel `settings`), sedangkan `PATHS`
/// harus didaftarkan lebih dulu supaya letak database diketahui. `OnceLock`
/// biasa tidak bisa diubah lagi setelah di-`set`, jadi pilihan pengguna
/// ditampung di sini tanpa mengubah urutan inisialisasi.
static WORKSPACE_OVERRIDE: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Daftarkan path sekali di awal (dipanggil `nativeInit` / `main`).
pub fn init(paths: AppPaths) -> Result<(), String> {
    PATHS
        .set(paths)
        .map_err(|_| "Path aplikasi sudah diinisialisasi".to_string())
}

pub fn get() -> Result<&'static AppPaths, String> {
    PATHS
        .get()
        .ok_or_else(|| "Path aplikasi belum diinisialisasi".to_string())
}

/// Platform yang sedang berjalan. `None` kalau [`init`] belum dipanggil.
pub fn platform() -> Option<Platform> {
    PATHS.get().map(|p| p.platform)
}

pub fn data_dir() -> Result<PathBuf, String> {
    Ok(get()?.data_dir.clone())
}

pub fn cache_dir() -> Result<PathBuf, String> {
    let dir = get()?.cache_dir.clone();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Pakai folder kerja pilihan pengguna mulai sekarang.
///
/// Nilainya juga tersimpan di tabel `settings`, jadi start berikutnya
/// memanggil ini lagi dari `main`. Berbeda dengan [`PATHS`], pilihan ini
/// **boleh berubah** saat aplikasi berjalan (tombol "Simpan" di tab Setelan).
/// `RwLock` dipakai supaya perubahan itu langsung terlihat oleh thread agent
/// yang sedang berjalan, tanpa perlu restart.
pub fn override_workspace(dir: PathBuf) {
    if let Ok(mut g) = WORKSPACE_OVERRIDE.write() {
        *g = Some(dir);
    }
}

/// Folder kerja yang sedang berlaku: pilihan pengguna kalau ada.
///
/// Dipakai juga oleh unit test yang tidak boleh mengubah workspace global
/// (test berjalan paralel dalam satu proses).
pub fn workspace_current() -> PathBuf {
    let chosen = WORKSPACE_OVERRIDE.read().ok().and_then(|g| g.clone());
    chosen.unwrap_or_else(|| get().map(|p| p.workspace.clone()).unwrap_or_default())
}

/// Lupakan pilihan workspace, kembali ke bawaan [`AppPaths`].
///
/// Dipakai saat kotak folder kerja dikosongkan (tombol Simpan) dan oleh test
/// yang menunjuk folder sementara.
pub fn clear_workspace_override() {
    if let Ok(mut g) = WORKSPACE_OVERRIDE.write() {
        *g = None;
    }
}

/// Folder kerja agent, dibuat kalau belum ada.
pub fn workspace() -> Result<PathBuf, String> {
    let dir = workspace_current();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Path workspace untuk ditampilkan di prompt/UI (tanpa membuat folder).
pub fn workspace_display() -> String {
    workspace_current().to_string_lossy().to_string()
}

/// Path workspace yang terlihat oleh AGENT saat menjalankan tool.
///
/// Di Android tool berjalan di dalam proot Ubuntu, jadi workspace yang sama
/// terlihat sebagai `/root/storage` — bukan path host `/data/user/0/...`.
/// Sebelumnya prompt menyebut path host, sehingga model mencari file di path
/// yang tidak ada di Ubuntu sementara tab File menampilkan isi workspace yang
/// sama lewat path host.
pub fn agent_workspace_display() -> String {
    match platform() {
        Some(Platform::Android) => "/root/storage".to_string(),
        _ => workspace_display(),
    }
}

/// Folder skill seperti yang terlihat agent di dalam OS tempat tool jalan.
pub fn agent_skills_dir_display() -> String {
    match platform() {
        Some(Platform::Android) => "/root/.claude/skills".to_string(),
        _ => skills_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

/// Folder konfigurasi skill seperti yang terlihat agent.
pub fn agent_config_dir_display() -> String {
    match platform() {
        Some(Platform::Android) => "/root/.config".to_string(),
        _ => skills_config_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    }
}

pub fn skills_dir() -> Result<PathBuf, String> {
    Ok(get()?.skills_dir.clone())
}

pub fn skills_config_dir() -> Result<PathBuf, String> {
    Ok(get()?.skills_config_dir.clone())
}

pub fn rootfs() -> Result<PathBuf, String> {
    get()?
        .rootfs_dir
        .clone()
        .ok_or_else(|| "Platform ini tidak memakai rootfs Ubuntu".to_string())
}

pub fn native_lib_dir() -> Result<PathBuf, String> {
    get()?
        .native_lib_dir
        .clone()
        .ok_or_else(|| "Platform ini tidak punya folder library native".to_string())
}
