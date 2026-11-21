//! Manajemen file workspace.
//!
//! Semua path dari UI wajib lewat [resolve] supaya tidak bisa keluar dari
//! workspace (path traversal `../../data/...` ditolak).

use crate::ctx;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct FileInfo {
    /// Path relatif terhadap workspace (`""` = root).
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified: i64,
    /// `text` | `image` | `video` | `pdf` | `binary`
    pub kind: String,
}

/// Ukuran maksimum file yang boleh dibaca sebagai teks (2 MB).
const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

pub fn kind_of(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "markdown" | "json" | "yaml" | "yml" | "toml" | "ini" | "csv" | "log"
        | "xml" | "html" | "htm" | "css" | "js" | "ts" | "jsx" | "tsx" | "py" | "rs" | "kt"
        | "java" | "c" | "h" | "cpp" | "hpp" | "go" | "rb" | "php" | "sh" | "bash" | "zsh"
        | "sql" | "conf" | "gradle" | "kts" | "dockerfile" | "gitignore" | "env" | "svg" => "text",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => "image",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "3gp" => "video",
        "pdf" => "pdf",
        _ => "binary",
    }
}

/// Terjemahkan path relatif dari UI ke path host di dalam workspace.
///
/// Menolak `..` dan path absolut — satu-satunya jalan masuk ke filesystem
/// lewat modul ini.
pub fn resolve(rel: &str) -> Result<PathBuf, String> {
    let root = ctx::workspace_dir()?;
    let rel = rel.trim().trim_start_matches('/');
    let candidate = Path::new(&rel);
    if candidate
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
    {
        return Err("Path tidak boleh keluar dari workspace".into());
    }
    let full = root.join(candidate);
    // Canonicalize hanya kalau ada, supaya symlink keluar workspace juga tertolak.
    if full.exists() {
        let canon_root = root.canonicalize().map_err(|e| e.to_string())?;
        let canon = full.canonicalize().map_err(|e| e.to_string())?;
        if !canon.starts_with(&canon_root) {
            return Err("Path tidak boleh keluar dari workspace".into());
        }
        return Ok(canon);
    }
    Ok(full)
}

/// Path relatif dari UI dibangun dari `rel` yang dikirim UI, bukan dari
/// `strip_prefix` path absolut.
///
/// `resolve()` memakai `canonicalize()` (di Android `/data/user/0/...`
/// berubah jadi `/data/data/...`), sedangkan root dari `workspace_dir()`
/// tidak. Strip prefix antar keduanya gagal → semua file dapat `path = ""`
/// → key `LazyColumn` duplikat → aplikasi force-close.
fn norm_rel(rel: &str) -> String {
    rel.trim().trim_start_matches('/').to_string()
}

fn info_of(path: &Path, rel: String) -> Option<FileInfo> {
    let meta = fs::metadata(path).ok()?;
    let name = path.file_name()?.to_string_lossy().to_string();
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Some(FileInfo {
        path: rel,
        name: name.clone(),
        is_dir: meta.is_dir(),
        size: meta.len() as i64,
        modified,
        kind: if meta.is_dir() { "dir".into() } else { kind_of(&name).into() },
    })
}

/// Isi satu folder, folder dulu lalu nama A→Z.
pub fn list(rel: &str) -> Result<Vec<FileInfo>, String> {
    let dir = resolve(rel)?;
    if !dir.is_dir() {
        return Err("Bukan folder".into());
    }
    let base = norm_rel(rel);
    let mut items: Vec<FileInfo> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let child = if base.is_empty() { name } else { format!("{base}/{name}") };
            info_of(&e.path(), child)
        })
        .collect();
    items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(items)
}

pub fn read_text(rel: &str) -> Result<String, String> {
    let path = resolve(rel)?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_TEXT_BYTES {
        return Err(format!(
            "File terlalu besar untuk dipratinjau ({} MB, maksimum 2 MB).",
            meta.len() / 1024 / 1024
        ));
    }
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

pub fn delete(rel: &str) -> Result<(), String> {
    let path = resolve(rel)?;
    if path == ctx::workspace_dir()? {
        return Err("Folder workspace tidak bisa dihapus".into());
    }
    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(&path).map_err(|e| e.to_string())
    }
}

pub fn mkdir(rel: &str) -> Result<(), String> {
    let path = resolve(rel)?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

pub fn rename(from: &str, to: &str) -> Result<(), String> {
    let src = resolve(from)?;
    let dst = resolve(to)?;
    fs::rename(&src, &dst).map_err(|e| e.to_string())
}

/// Salin file dari URI storage Android (hasil SAF picker) ke workspace.
///
/// Dipakai untuk "Impor file": Kotlin membuka InputStream dari URI lalu
/// menyerahkan byte-nya lewat [write_bytes].
pub fn write_bytes(rel: &str, bytes: &[u8]) -> Result<FileInfo, String> {
    let path = resolve(rel)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    info_of(&path, norm_rel(rel)).ok_or_else(|| "File gagal ditulis".to_string())
}

pub fn stat(rel: &str) -> Result<FileInfo, String> {
    let path = resolve(rel)?;
    info_of(&path, norm_rel(rel)).ok_or_else(|| "File tidak ditemukan".into())
}

/// Ukuran total workspace dalam byte (untuk tab Settings).
pub fn workspace_size() -> Result<i64, String> {
    let root = ctx::workspace_dir()?;
    let mut total = 0i64;
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                stack.push(e.path());
            } else {
                total += meta.len() as i64;
            }
        }
    }
    Ok(total)
}
