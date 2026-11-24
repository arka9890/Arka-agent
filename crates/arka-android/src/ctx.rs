//! Path khusus Android untuk proot (rootfs, library native, folder tmp).
//!
//! Path umum (data, workspace, skill) ada di `arka_core::paths`; modul ini
//! hanya menambah hal yang tidak ada di platform lain. Karena itu crate
//! `arka-android` tidak ikut dicompile untuk Windows.

use arka_core::paths;
use std::path::PathBuf;

pub fn files_dir() -> Result<PathBuf, String> {
    paths::data_dir()
}

pub fn rootfs_dir() -> Result<PathBuf, String> {
    paths::rootfs()
}

pub fn workspace_dir() -> Result<PathBuf, String> {
    paths::workspace()
}

pub fn native_lib_dir() -> Result<PathBuf, String> {
    paths::native_lib_dir()
}

pub fn proot_tmp_dir() -> Result<PathBuf, String> {
    let dir = paths::data_dir()?.join("proot-tmp");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn proot_libs_dir() -> Result<PathBuf, String> {
    let dir = paths::data_dir()?.join("proot-libs");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Cari file `lib*.so` hasil ekstraksi APK di nativeLibraryDir.
pub fn native_lib(name: &str) -> Result<Option<PathBuf>, String> {
    let dir = paths::native_lib_dir()?;
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
    let dir = paths::native_lib_dir()?;
    let p = dir.join("libproot-loader.so");
    Ok(if p.exists() { Some(p) } else { None })
}
