//! Konteks aplikasi yang dipakai semua modul.
//!
//! Diisi sekali dari Kotlin ([crate::Java_com_arka_agent_core_Arka_nativeInit])
//! sebelum operasi lain dipanggil.

use std::path::PathBuf;
use std::sync::OnceLock;

pub struct Ctx {
    /// `<filesDir>` — akar semua data milik aplikasi.
    pub files_dir: PathBuf,
    /// `<applicationInfo.nativeLibraryDir>` — satu-satunya folder yang boleh
    /// dieksekusi di Android 10+ (berisi libproot.so dan kawan-kawan).
    pub native_lib_dir: PathBuf,
}

static CTX: OnceLock<Ctx> = OnceLock::new();

pub fn set_ctx(files_dir: PathBuf, native_lib_dir: PathBuf) {
    let _ = CTX.set(Ctx { files_dir, native_lib_dir });
}

pub fn ctx() -> Result<&'static Ctx, String> {
    CTX.get().ok_or_else(|| "nativeInit belum dipanggil".to_string())
}

pub fn files_dir() -> Result<PathBuf, String> {
    Ok(ctx()?.files_dir.clone())
}

/// Folder rootfs Ubuntu (`<filesDir>/distro/ubuntu/rootfs`).
pub fn rootfs_dir() -> Result<PathBuf, String> {
    Ok(ctx()?.files_dir.join("distro/ubuntu/rootfs"))
}

/// Folder workspace host (`<filesDir>/workspace`), di-bind ke `/root/storage`.
pub fn workspace_dir() -> Result<PathBuf, String> {
    let dir = ctx()?.files_dir.join("workspace");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn proot_tmp_dir() -> Result<PathBuf, String> {
    let dir = ctx()?.files_dir.join("proot-tmp");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn proot_libs_dir() -> Result<PathBuf, String> {
    let dir = ctx()?.files_dir.join("proot-libs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Cari file `lib*.so` hasil ekstraksi APK di nativeLibraryDir.
pub fn native_lib(name: &str) -> Result<Option<PathBuf>, String> {
    let dir = &ctx()?.native_lib_dir;
    let direct = dir.join(name);
    if direct.exists() {
        return Ok(Some(direct));
    }
    let stem = name.trim_start_matches("lib").trim_end_matches(".so");
    let found = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .find(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with("lib") && n.contains(stem) && !n.contains("loader")
        })
        .map(|e| e.path());
    Ok(found)
}

pub fn proot_binary() -> Result<Option<PathBuf>, String> {
    native_lib("libproot.so")
}

pub fn loader_binary() -> Result<Option<PathBuf>, String> {
    let dir = &ctx()?.native_lib_dir;
    let p = dir.join("libproot-loader.so");
    Ok(if p.exists() { Some(p) } else { None })
}
