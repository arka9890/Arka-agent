//! Manajemen file workspace.
//!
//! Semua path dari UI wajib lewat [resolve_at] supaya tidak bisa keluar dari
//! workspace (path traversal `../../data/...` ditolak).
//!
//! Sejak M6 ada dua lapis API:
//!
//! * `*_at(root, …)` — **akar eksplisit**, tanpa state global. Ini yang dipakai
//!   UI desktop, sehingga test bisa menunjuk folder sementara sendiri tanpa
//!   `paths::init` (yang hanya boleh sekali per proses).
//! * fungsi lama (`list`, `read_text`, …) — pembungkus tipis di atas
//!   `paths::workspace()`, dipakai shim JNI Android. Perilakunya tidak berubah.

use crate::paths;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Clone, Debug)]
pub struct FileInfo {
    /// Path relatif terhadap workspace (`""` = root), selalu pakai `/`.
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified: i64,
    /// `text` | `image` | `video` | `pdf` | `binary` (atau `dir`)
    pub kind: String,
}

/// Ukuran maksimum file yang boleh dibaca sebagai teks (2 MB).
pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;

/// Folder sampah internal. Sengaja di dalam workspace supaya `rename` tetap
/// satu volume (tidak menyalin gigabyte), dan disembunyikan dari daftar UI.
pub const TRASH_DIR: &str = ".arka/trash";

/// Nama yang tidak ditampilkan di daftar akar workspace.
const HIDDEN_AT_ROOT: [&str; 1] = [".arka"];

pub fn kind_of(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "txt" | "md" | "markdown" | "json" | "yaml" | "yml" | "toml" | "ini" | "csv" | "log"
        | "xml" | "html" | "htm" | "css" | "js" | "ts" | "jsx" | "tsx" | "py" | "rs" | "kt"
        | "java" | "c" | "h" | "cpp" | "hpp" | "go" | "rb" | "php" | "sh" | "bash" | "zsh"
        | "ps1" | "psm1" | "bat" | "cmd" | "sql" | "conf" | "gradle" | "kts" | "dockerfile"
        | "gitignore" | "env" | "svg" => "text",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" => "image",
        "mp4" | "mkv" | "avi" | "mov" | "webm" | "3gp" | "mp3" | "wav" | "flac" | "ogg" | "m4a"
        | "aac" => "video",
        "pdf" => "pdf",
        _ => "binary",
    }
}

/// Panjang path maksimum yang aman tanpa awalan `\\?\` di Windows.
#[cfg(windows)]
const MAX_PATH_SAFE: usize = 240;

/// Awalan `\\?\` untuk path panjang di Windows.
///
/// Windows membatasi path klasik ke 260 karakter (`MAX_PATH`). Awalan verbatim
/// `\\?\` melewati batas itu **tanpa** perlu mengubah registry
/// (`LongPathsEnabled`), jadi satu-satunya cara yang bisa diandalkan di mesin
/// pengguna mana pun. Path yang sudah berawalan `\\?\` / `\\.\` tidak diubah,
/// dan awalan UNC harus jadi `\\?\UNC\server\share`.
///
/// Di luar Windows fungsi ini mengembalikan path apa adanya.
pub fn long_path(p: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let s = p.to_string_lossy();
        if s.len() < MAX_PATH_SAFE || s.starts_with(r"\\?\") || s.starts_with(r"\\.\") {
            return p.to_path_buf();
        }
        if let Some(rest) = s.strip_prefix(r"\\") {
            return PathBuf::from(format!(r"\\?\UNC\{rest}"));
        }
        if p.is_absolute() {
            return PathBuf::from(format!(r"\\?\{s}"));
        }
    }
    p.to_path_buf()
}

/// Terjemahkan path relatif dari UI ke path host di dalam `root`.
///
/// Menolak `..`, path absolut, dan awalan drive (`C:berkas` — di Windows
/// `resolve` bawaan Windows akan melepas awalan drive walau bukan path
/// absolut, itu jalan keluar dari workspace).
pub fn resolve_at(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = rel.trim().trim_start_matches(['/', '\\']);
    let candidate = Path::new(rel);
    if candidate.components().any(|c| {
        matches!(
            c,
            std::path::Component::ParentDir
                | std::path::Component::RootDir
                | std::path::Component::Prefix(_)
        )
    }) {
        return Err("Path tidak boleh keluar dari workspace".into());
    }
    let full = root.join(candidate);
    // Canonicalize hanya kalau ada, supaya symlink keluar workspace juga tertolak.
    if full.exists() {
        let canon_root = root
            .canonicalize()
            .map_err(|e| format!("Workspace tidak bisa dibaca: {e}"))?;
        let canon = full.canonicalize().map_err(|e| e.to_string())?;
        if !canon.starts_with(&canon_root) {
            return Err("Path tidak boleh keluar dari workspace".into());
        }
        // `canonicalize` di Windows selalu memberi awalan `\\?\`, jadi path
        // panjang di bawah sudah aman sejak titik ini.
        return Ok(canon);
    }
    // Belum ada (membuat folder/berkas baru): beri awalan sendiri.
    Ok(long_path(&full))
}

/// Path relatif dari UI dibangun dari `rel` yang dikirim UI, bukan dari
/// `strip_prefix` path absolut.
///
/// `resolve_at()` memakai `canonicalize()` (di Android `/data/user/0/...`
/// berubah jadi `/data/data/...`, di Windows `C:\…` jadi `\\?\C:\…`),
/// sedangkan root dari `workspace_dir()` tidak. Strip prefix antar keduanya
/// gagal → semua file dapat `path = ""` → key duplikat → aplikasi force-close.
fn norm_rel(rel: &str) -> String {
    rel.trim()
        .trim_start_matches(['/', '\\'])
        .replace('\\', "/")
}

/// Gabungkan path relatif dengan nama berkas, selalu dengan `/`.
pub fn join_rel(dir: &str, name: &str) -> String {
    let dir = norm_rel(dir);
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
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
pub fn list_at(root: &Path, rel: &str) -> Result<Vec<FileInfo>, String> {
    let dir = resolve_at(root, rel)?;
    if !dir.is_dir() {
        return Err("Bukan folder".into());
    }
    let base = norm_rel(rel);
    let hide_meta = base.is_empty();
    let mut items: Vec<FileInfo> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| {
            !hide_meta || !HIDDEN_AT_ROOT.contains(&e.file_name().to_string_lossy().as_ref())
        })
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            info_of(&e.path(), join_rel(&base, &name))
        })
        .collect();
    items.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(items)
}

pub fn read_text_at(root: &Path, rel: &str) -> Result<String, String> {
    let path = resolve_at(root, rel)?;
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

pub fn delete_at(root: &Path, rel: &str) -> Result<(), String> {
    let path = resolve_at(root, rel)?;
    if path == long_path(root).canonicalize().unwrap_or_else(|_| root.to_path_buf()) {
        return Err("Folder workspace tidak bisa dihapus".into());
    }
    if norm_rel(rel).is_empty() {
        return Err("Folder workspace tidak bisa dihapus".into());
    }
    if path.is_dir() {
        fs::remove_dir_all(&path).map_err(|e| e.to_string())
    } else {
        fs::remove_file(&path).map_err(|e| e.to_string())
    }
}

pub fn mkdir_at(root: &Path, rel: &str) -> Result<(), String> {
    let path = resolve_at(root, rel)?;
    fs::create_dir_all(&path).map_err(|e| e.to_string())
}

pub fn rename_at(root: &Path, from: &str, to: &str) -> Result<(), String> {
    let src = resolve_at(root, from)?;
    let dst = resolve_at(root, to)?;
    fs::rename(&src, &dst).map_err(|e| e.to_string())
}

/// Salin (rekursif) berkas/folder luar `src` ke folder `rel_dir` di workspace.
///
/// Nama bentrok tidak menimpa: dipakai `nama (1).ext` seperti Explorer. Ini
/// penting untuk drag & drop — pengguna menjatuhkan berkas yang namanya sudah
/// ada, dan data lama tidak boleh hilang.
pub fn copy_into_at(root: &Path, rel_dir: &str, src: &Path) -> Result<FileInfo, String> {
    let dir = resolve_at(root, rel_dir)?;
    if !dir.is_dir() {
        return Err("Tujuan bukan folder".into());
    }
    let name = src
        .file_name()
        .ok_or_else(|| "Berkas tanpa nama".to_string())?
        .to_string_lossy()
        .to_string();
    let final_name = unique_name(&dir, &name);
    let dst = dir.join(&final_name);
    // Menyalin folder ke dalam dirinya sendiri akan berputar tanpa akhir.
    if let Ok(src_canon) = src.canonicalize() {
        if dst.starts_with(&src_canon) {
            return Err("Folder tidak bisa disalin ke dalam dirinya sendiri".into());
        }
    }
    copy_tree(src, &dst)?;
    stat_at(root, &join_rel(rel_dir, &final_name))
}

fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    let meta = fs::metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if meta.is_dir() {
        fs::create_dir_all(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        for e in fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
            copy_tree(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst)
            .map(|_| ())
            .map_err(|e| format!("{}: {e}", src.display()))
    }
}

/// Pisahkan nama jadi batang + ekstensi. Berkas tersembunyi gaya Unix
/// (`.gitignore`) dianggap **tanpa** ekstensi, supaya hasil bentroknya
/// `.gitignore (1)`, bukan ` (1).gitignore`.
fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(0) | None => (name, ""),
        Some(i) => (&name[..i], &name[i + 1..]),
    }
}

/// Nama yang belum dipakai di `dir`: `laporan.txt` → `laporan (1).txt`.
pub fn unique_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    for n in 1..=9999 {
        let candidate = if ext.is_empty() {
            format!("{stem} ({n})")
        } else {
            format!("{stem} ({n}).{ext}")
        };
        if !dir.join(&candidate).exists() {
            return candidate;
        }
    }
    format!("{stem}-{}{}", unique_stamp(), if ext.is_empty() { String::new() } else { format!(".{ext}") })
}

fn unique_stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Pindahkan berkas/folder ke sampah internal (`.arka/trash`).
///
/// Mengembalikan path tujuan. `rename` dipakai lebih dulu supaya memindahkan
/// folder 5 GB tetap sekejap; bila gagal (beda volume / izin), isinya disalin
/// lalu sumbernya dihapus.
pub fn trash_at(root: &Path, rel: &str) -> Result<String, String> {
    if norm_rel(rel).is_empty() {
        return Err("Folder workspace tidak bisa dihapus".into());
    }
    let path = resolve_at(root, rel)?;
    let trash = root.join(TRASH_DIR);
    fs::create_dir_all(&trash).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "berkas".to_string());
    let stamp = unique_stamp();
    let mut target = trash.join(format!("{stamp}-{name}"));
    let mut n = 1;
    while target.exists() {
        target = trash.join(format!("{stamp}-{n}-{name}"));
        n += 1;
    }
    if fs::rename(&path, &target).is_err() {
        copy_tree(&path, &target)?;
        if path.is_dir() {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
        }
    }
    Ok(target.to_string_lossy().to_string())
}

/// Buang seluruh isi sampah. Mengembalikan jumlah byte yang dibebaskan.
pub fn empty_trash(root: &Path) -> Result<u64, String> {
    let trash = root.join(TRASH_DIR);
    if !trash.exists() {
        return Ok(0);
    }
    let mut freed = 0u64;
    for e in fs::read_dir(&trash).map_err(|e| e.to_string())?.flatten() {
        let meta = e.metadata();
        freed += meta.map(|m| m.len()).unwrap_or(0);
        let p = e.path();
        if p.is_dir() {
            fs::remove_dir_all(&p).map_err(|e| e.to_string())?;
        } else {
            fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(freed)
}

pub fn write_bytes_at(root: &Path, rel: &str, bytes: &[u8]) -> Result<FileInfo, String> {
    let path = resolve_at(root, rel)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    info_of(&path, norm_rel(rel)).ok_or_else(|| "File gagal ditulis".to_string())
}

pub fn stat_at(root: &Path, rel: &str) -> Result<FileInfo, String> {
    let path = resolve_at(root, rel)?;
    info_of(&path, norm_rel(rel)).ok_or_else(|| "File tidak ditemukan".into())
}

pub fn workspace_size_at(root: &Path) -> Result<i64, String> {
    let mut total = 0i64;
    let mut stack = vec![root.to_path_buf()];
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

// ── Pembungkus lama (dipakai shim JNI Android; perilaku tidak berubah) ──────

fn root() -> Result<PathBuf, String> {
    paths::workspace()
}

pub fn resolve(rel: &str) -> Result<PathBuf, String> {
    resolve_at(&root()?, rel)
}

pub fn list(rel: &str) -> Result<Vec<FileInfo>, String> {
    list_at(&root()?, rel)
}

pub fn read_text(rel: &str) -> Result<String, String> {
    read_text_at(&root()?, rel)
}

pub fn delete(rel: &str) -> Result<(), String> {
    delete_at(&root()?, rel)
}

pub fn mkdir(rel: &str) -> Result<(), String> {
    mkdir_at(&root()?, rel)
}

pub fn rename(from: &str, to: &str) -> Result<(), String> {
    rename_at(&root()?, from, to)
}

pub fn write_bytes(rel: &str, bytes: &[u8]) -> Result<FileInfo, String> {
    write_bytes_at(&root()?, rel, bytes)
}

pub fn stat(rel: &str) -> Result<FileInfo, String> {
    stat_at(&root()?, rel)
}

pub fn workspace_size() -> Result<i64, String> {
    workspace_size_at(&root()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arka-file-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn traversal_ditolak() {
        let root = tmp("traversal");
        assert!(resolve_at(&root, "../luar.txt").is_err());
        assert!(resolve_at(&root, "a/../../luar.txt").is_err());
        assert!(resolve_at(&root, "a/../../luar.txt").is_err());
        assert!(resolve_at(&root, "sub/berkas.txt").is_ok());

        // Garis miring di depan **sengaja** dibuang: UI menampilkan
        // `/sub/berkas` dan Android mengirim path dengan awalan itu. Hasilnya
        // tetap di dalam workspace — bukan `/etc/passwd` sungguhan.
        let canon_root = root.canonicalize().unwrap();
        for abs in ["/etc/passwd", r"\Windows\win.ini"] {
            let p = resolve_at(&root, abs).unwrap();
            assert!(
                p.starts_with(&canon_root),
                "{abs} keluar dari workspace: {}",
                p.display()
            );
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(windows)]
    #[test]
    fn awalan_drive_windows_ditolak() {
        // `Path::join` di Windows melepas awalan drive, jadi `C:berkas`
        // berakhir di luar workspace kalau tidak ditolak lebih dulu.
        let root = tmp("drive");
        assert!(
            resolve_at(&root, "C:berkas.txt").is_err(),
            "awalan drive harus ditolak"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn daftar_terurut_dan_folder_dulu() {
        let root = tmp("list");
        fs::create_dir_all(root.join("zeta")).unwrap();
        fs::write(root.join("Berkas.txt"), b"x").unwrap();
        fs::write(root.join("apel.md"), b"yy").unwrap();
        let list = list_at(&root, "").unwrap();
        let names: Vec<&str> = list.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["zeta", "apel.md", "Berkas.txt"]);
        assert_eq!(list[1].path, "apel.md");
        assert_eq!(list[2].size, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn metadata_arka_disembunyikan_di_akar_saja() {
        let root = tmp("hidden");
        fs::create_dir_all(root.join(".arka/trash")).unwrap();
        fs::create_dir_all(root.join("sub/.arka")).unwrap();
        let akar = list_at(&root, "").unwrap();
        assert!(akar.iter().all(|f| f.name != ".arka"), "sampah tidak boleh tampil");
        let sub = list_at(&root, "sub").unwrap();
        assert_eq!(sub.len(), 1, "di dalam subfolder namanya sah-sah saja");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn nama_bentrok_diberi_nomor() {
        let root = tmp("unique");
        fs::write(root.join("laporan.txt"), b"lama").unwrap();
        assert_eq!(unique_name(&root, "baru.txt"), "baru.txt");
        assert_eq!(unique_name(&root, "laporan.txt"), "laporan (1).txt");
        fs::write(root.join("laporan (1).txt"), b"x").unwrap();
        assert_eq!(unique_name(&root, "laporan.txt"), "laporan (2).txt");
        fs::write(root.join(".gitignore"), b"x").unwrap();
        assert_eq!(unique_name(&root, ".gitignore"), ".gitignore (1)");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn impor_tidak_menimpa_dan_rekursif() {
        let root = tmp("impor");
        let luar = tmp("impor-luar");
        fs::write(luar.join("catatan.txt"), b"baru").unwrap();
        fs::write(root.join("catatan.txt"), b"lama").unwrap();

        let info = copy_into_at(&root, "", &luar.join("catatan.txt")).unwrap();
        assert_eq!(info.name, "catatan (1).txt");
        assert_eq!(fs::read(root.join("catatan.txt")).unwrap(), b"lama");
        assert_eq!(fs::read(root.join("catatan (1).txt")).unwrap(), b"baru");

        fs::create_dir_all(luar.join("proyek/src")).unwrap();
        fs::write(luar.join("proyek/src/main.rs"), b"fn main(){}").unwrap();
        let info = copy_into_at(&root, "", &luar.join("proyek")).unwrap();
        assert!(info.is_dir);
        assert_eq!(
            fs::read(root.join("proyek/src/main.rs")).unwrap(),
            b"fn main(){}"
        );
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&luar);
    }

    #[test]
    fn impor_ke_subfolder() {
        let root = tmp("impor-sub");
        let luar = tmp("impor-sub-luar");
        fs::create_dir_all(root.join("tujuan")).unwrap();
        fs::write(luar.join("a.txt"), b"isi").unwrap();
        let info = copy_into_at(&root, "tujuan", &luar.join("a.txt")).unwrap();
        assert_eq!(info.path, "tujuan/a.txt");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&luar);
    }

    #[test]
    fn hapus_ke_sampah_bukan_permanen() {
        let root = tmp("sampah");
        fs::write(root.join("penting.txt"), b"jangan hilang").unwrap();
        let tujuan = trash_at(&root, "penting.txt").unwrap();
        assert!(!root.join("penting.txt").exists());
        assert!(
            Path::new(&tujuan).exists(),
            "berkas harus ada di sampah: {tujuan}"
        );
        assert_eq!(fs::read(&tujuan).unwrap(), b"jangan hilang");

        let freed = empty_trash(&root).unwrap();
        assert_eq!(freed, 13);
        assert!(list_at(&root, TRASH_DIR).unwrap().is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sampah_tidak_bisa_menghapus_akar() {
        let root = tmp("sampah-akar");
        assert!(trash_at(&root, "").is_err());
        assert!(trash_at(&root, ".").is_ok() || trash_at(&root, ".").is_err());
        assert!(root.exists(), "workspace tidak boleh ikut hilang");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn teks_besar_ditolak() {
        let root = tmp("besar");
        fs::write(root.join("besar.txt"), vec![b'a'; (MAX_TEXT_BYTES + 1) as usize]).unwrap();
        let err = read_text_at(&root, "besar.txt").unwrap_err();
        assert!(err.contains("terlalu besar"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn jenis_berkas_mengenali_audio_dan_skrip_ps() {
        assert_eq!(kind_of("lagu.mp3"), "video");
        assert_eq!(kind_of("skrip.ps1"), "text");
        assert_eq!(kind_of("foto.JPEG"), "image");
        assert_eq!(kind_of("data.bin"), "binary");
        assert_eq!(kind_of("tanpa-ekstensi"), "binary");
    }

    #[cfg(windows)]
    #[test]
    fn path_lebih_dari_260_karakter_bisa_dibuat() {
        // Bukti awalan `\\?\` bekerja tanpa mengubah registry.
        let root = tmp("panjang");
        let mut rel = String::new();
        for i in 0..20 {
            rel = join_rel(&rel, &format!("folder-panjang-{i:02}"));
        }
        rel = join_rel(&rel, "catatan.txt");
        assert!(rel.len() > 260, "path uji harus > 260 char, dapat {}", rel.len());
        mkdir_at(&root, rel.rsplit_once('/').unwrap().0).unwrap();
        write_bytes_at(&root, &rel, b"panjang").unwrap();
        assert_eq!(read_text_at(&root, &rel).unwrap(), "panjang");
        let _ = fs::remove_dir_all(&root);
    }
}
