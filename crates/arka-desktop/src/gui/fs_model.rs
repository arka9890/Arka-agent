//! Model daftar berkas untuk tab File — **murni**, tanpa egui.
//!
//! Isi folder kini dibaca lewat `arka_core::file::list_at` (akar eksplisit),
//! sehingga guard traversal, urutan folder-dulu, dan penyembunyian `.arka`
//! hanya ada di **satu** tempat dan dipakai Android juga.
//!
//! Yang tinggal di sini: cache (jangan `read_dir` 60×/detik), breadcrumb, dan
//! detail tampilan (ikon, ukuran, nama induk).

use std::path::Path;

use arka_core::file::{self, FileInfo};

/// Cache daftar berkas: dibaca ulang **hanya** saat diminta, bukan setiap
/// frame. Tanpa cache, tab File melakukan `read_dir` 60×/detik selama jendela
/// terbuka — boros dan membuat disk berputar terus di laptop.
#[derive(Clone, Debug, Default)]
pub struct FilesCache {
    dir: Option<String>,
    pub entries: Vec<FileInfo>,
    pub error: Option<String>,
    /// `true` setelah percobaan baca pertama selesai (berhasil atau gagal).
    pub loaded: bool,
}

impl FilesCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_stale(&self, dir: &str) -> bool {
        self.dir.as_deref() != Some(dir)
    }

    /// Muat ulang isi folder `rel` di dalam `root`.
    pub fn refresh(&mut self, root: &Path, rel: &str) {
        self.dir = Some(rel.to_string());
        self.loaded = true;
        // Folder kerja mungkin belum ada sebelum perintah pertama. Itu kondisi
        // normal, bukan error: `list_at` memberi pesan yang bisa ditampilkan.
        match file::list_at(root, rel) {
            Ok(list) => {
                self.entries = list;
                self.error = None;
            }
            Err(e) => {
                self.entries.clear();
                self.error = Some(e);
            }
        }
    }

    /// Tandai isi folder sudah usang (mis. agent baru menulis berkas).
    ///
    /// Tidak membaca disk di sini: pemanggilnya jalur `update()` UI, dan
    /// membaca folder di tengah frame adalah cara membuat UI tersendat.
    pub fn invalidate(&mut self) {
        self.loaded = false;
    }

    /// Muat ulang hanya bila folder berubah atau belum pernah dibaca.
    pub fn ensure(&mut self, root: &Path, rel: &str) -> bool {
        if self.loaded && !self.is_stale(rel) {
            return false;
        }
        self.refresh(root, rel);
        true
    }
}

/// Isi folder **sembarang** (bukan hanya workspace).
///
/// Dipakai pemilih lampiran chat yang menjelajah folder pengguna, sehingga
/// tidak bisa memakai `file::list_at` yang mengunci diri di dalam workspace.
/// Mengikuti `file_type()` (bukan `metadata()`), jadi tautan tidak diikuti —
/// daftar berkas yang jujur, dan tidak bisa "nyangkut" di symlink melingkar.
pub fn read_dir(path: &Path) -> Result<Vec<FsEntry>, String> {
    let rd = std::fs::read_dir(path).map_err(|e| e.to_string())?;
    let mut out: Vec<FsEntry> = rd
        .filter_map(|e| e.ok())
        .map(|e| {
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let size = if is_dir {
                0
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            };
            FsEntry {
                name: e.file_name().to_string_lossy().to_string(),
                is_dir,
                size,
            }
        })
        .collect();
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(out)
}

/// Satu entri untuk [`read_dir`] (dipakai pemilih lampiran chat).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Satu bagian breadcrumb yang bisa diklik.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crumb {
    pub label: String,
    pub rel: String,
}

/// `a/b/c` → `[Workspace, a, a/b, a/b/c]`.
///
/// Bagian pertama selalu akar workspace; UI menampilkan namanya sendiri
/// (istilah "Workspace" sudah diterjemahkan di layar).
pub fn breadcrumb(rel: &str) -> Vec<Crumb> {
    let mut out = vec![Crumb {
        label: String::new(),
        rel: String::new(),
    }];
    let mut acc = String::new();
    for seg in rel
        .split(['/', '\\'])
        .filter(|s| !s.is_empty() && *s != ".")
    {
        acc = if acc.is_empty() {
            seg.to_string()
        } else {
            format!("{acc}/{seg}")
        };
        out.push(Crumb {
            label: seg.to_string(),
            rel: acc.clone(),
        });
    }
    out
}

/// Folder induk dari path relatif. Akar mengembalikan akar (`""`).
pub fn parent(rel: &str) -> String {
    let rel = rel.trim().trim_matches(['/', '\\']);
    match rel.rsplit_once('/') {
        Some((head, _)) => head.to_string(),
        None => String::new(),
    }
}

/// Ikon per jenis. Emoji dipakai supaya tidak perlu aset gambar per tipe.
pub fn icon(kind: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "\u{1F4C1}";
    }
    match kind {
        "image" => "\u{1F5BC}",
        "video" => "\u{1F3AC}",
        "audio" => "\u{1F3B5}",
        "pdf" => "\u{1F4D5}",
        "text" => "\u{1F4C4}",
        _ => "\u{1F4E6}",
    }
}

/// Ukuran dalam satuan yang enak dibaca. `1 KiB` dipakai sebagai 1024, sama
/// seperti Explorer.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Salin satu berkas/folder workspace ke `target_dir` (dipakai "Ekspor").
///
/// Mengembalikan path tujuan. Nama bentrok di tujuan diberi nomor, jadi
/// mengekspor dua kali tidak menimpa berkas lain di komputer pengguna.
pub fn export_to(root: &Path, rel: &str, target_dir: &Path) -> Result<std::path::PathBuf, String> {
    let src = file::resolve_at(root, rel)?;
    if !target_dir.is_dir() {
        return Err(format!("Folder tujuan tidak ada: {}", target_dir.display()));
    }
    let name = src
        .file_name()
        .ok_or_else(|| "Berkas tanpa nama".to_string())?
        .to_string_lossy()
        .to_string();
    let dst = target_dir.join(file::unique_name(target_dir, &name));
    if src.is_dir() {
        // `copy_into_at` butuh akar workspace; untuk ekspor kita salin langsung.
        return Err("Folder belum bisa diekspor; pilih berkas.".into());
    }
    std::fs::copy(&src, &dst).map_err(|e| e.to_string())?;
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arka-fsmodel-{name}-{}-{}",
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
    fn breadcrumb_memecah_tiap_tingkat() {
        let c = breadcrumb("proyek/src/main.rs");
        let labels: Vec<&str> = c.iter().map(|x| x.label.as_str()).collect();
        assert_eq!(labels, ["", "proyek", "src", "main.rs"]);
        let rels: Vec<&str> = c.iter().map(|x| x.rel.as_str()).collect();
        assert_eq!(rels, ["", "proyek", "proyek/src", "proyek/src/main.rs"]);
        assert_eq!(breadcrumb("").len(), 1);
        assert_eq!(
            breadcrumb("/a//b/").len(),
            3,
            "garis miring ganda diabaikan"
        );
        // Path gaya Windows tetap dipecah benar.
        assert_eq!(breadcrumb(r"a\b").len(), 3);
    }

    #[test]
    fn induk_folder() {
        assert_eq!(parent("a/b/c.txt"), "a/b");
        assert_eq!(parent("a"), "");
        assert_eq!(parent(""), "");
    }

    #[test]
    fn ukuran_manusiawi() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(999), "999 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn ikon_per_jenis() {
        assert_eq!(icon("dir", true), "\u{1F4C1}");
        assert_eq!(icon("image", false), "\u{1F5BC}");
        assert_eq!(icon("binary", false), "\u{1F4E6}");
    }

    #[test]
    fn folder_tidak_ada_jadi_error_bukan_panic() {
        let mut c = FilesCache::new();
        c.refresh(Path::new("C:/tidak/ada/folder/ini"), "");
        assert!(c.error.is_some());
        assert!(c.entries.is_empty());
        assert!(c.loaded, "percobaan gagal tetap menandai sudah dicoba");
    }

    #[test]
    fn cache_hanya_membaca_ulang_saat_folder_berubah() {
        let dir = tmp("cache");
        fs::write(dir.join("satu.txt"), b"halo").unwrap();

        let mut c = FilesCache::new();
        assert!(c.ensure(&dir, ""), "baca pertama wajib");
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.entries[0].size, 4);
        assert_eq!(c.entries[0].kind, "text");
        assert_eq!(c.entries[0].path, "satu.txt");
        assert!(!c.ensure(&dir, ""), "folder sama → tidak membaca ulang");

        fs::write(dir.join("dua.txt"), b"hai").unwrap();
        // Masih tidak dibaca ulang: itu memang perilaku cache, dan tombol
        // "Muat ulang" yang memaksa lewat `refresh`.
        assert!(!c.ensure(&dir, ""));
        c.refresh(&dir, "");
        assert_eq!(c.entries.len(), 2);

        // Pindah folder → wajib baca ulang.
        fs::create_dir_all(dir.join("sub")).unwrap();
        assert!(c.ensure(&dir, "sub"));
        assert!(c.entries.iter().all(|e| e.path.starts_with("sub/")));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn read_dir_sembarang_terurut_folder_dulu() {
        let dir = tmp("readdir");
        std::fs::write(dir.join("b.txt"), b"xx").unwrap();
        std::fs::create_dir_all(dir.join("aaa")).unwrap();
        let list = read_dir(&dir).unwrap();
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["aaa", "b.txt"]);
        assert_eq!(list[1].size, 2);
        assert!(read_dir(&dir.join("tidak-ada")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ekspor_tidak_menimpa_berkas_di_tujuan() {
        let root = tmp("ekspor-src");
        let tujuan = tmp("ekspor-dst");
        fs::write(root.join("laporan.txt"), b"baru").unwrap();
        fs::write(tujuan.join("laporan.txt"), b"lama").unwrap();

        let dst = export_to(&root, "laporan.txt", &tujuan).unwrap();
        assert_eq!(dst.file_name().unwrap(), "laporan (1).txt");
        assert_eq!(fs::read(tujuan.join("laporan.txt")).unwrap(), b"lama");
        assert_eq!(fs::read(&dst).unwrap(), b"baru");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&tujuan);
    }
}
