//! Model daftar berkas untuk tab File — **murni**, tanpa egui.
//!
//! Dipisah dari layarnya karena tiga hal mudah salah dan layak diuji tanpa
//! jendela:
//!
//! 1. urutan: folder selalu di atas, lalu nama tanpa peduli besar-kecil huruf;
//! 2. `metadata()` mengikuti tautan, `file_type()` tidak — untuk daftar berkas
//!    yang jujur, `file_type()` yang dipakai;
//! 3. kegagalan baca (folder belum ada, akses ditolak) adalah **kondisi
//!    normal**, bukan panic.

use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

/// Isi satu folder. `Err` berisi pesan siap-tampil, bukan `io::Error`, supaya
/// layar tidak perlu menerjemahkan jenis error.
pub fn read_dir(path: &Path) -> Result<Vec<FsEntry>, String> {
    let rd = fs::read_dir(path).map_err(|e| e.to_string())?;

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

    sort_entries(&mut out);
    Ok(out)
}

/// Urutan yang sama dengan file manager Android.
pub fn sort_entries(entries: &mut [FsEntry]) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            // Nama yang hanya beda besar-kecil huruf diurutkan stabil supaya
            // daftar tidak "berkedip" saat dimuat ulang.
            .then_with(|| a.name.cmp(&b.name))
    });
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

/// Cache daftar berkas: dibaca ulang **hanya** saat diminta, bukan setiap
/// frame. Tanpa cache, tab File melakukan `read_dir` 60×/detik selama jendela
/// terbuka — boros dan membuat disk berputar terus di laptop.
#[derive(Clone, Debug, Default)]
pub struct FilesCache {
    dir: Option<PathBuf>,
    pub entries: Vec<FsEntry>,
    pub error: Option<String>,
    /// `true` setelah percobaan baca pertama selesai (berhasil atau gagal).
    pub loaded: bool,
}

impl FilesCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_stale(&self, dir: &Path) -> bool {
        self.dir.as_deref() != Some(dir)
    }

    /// Muat ulang isi `dir`.
    pub fn refresh(&mut self, dir: &Path) {
        self.dir = Some(dir.to_path_buf());
        self.loaded = true;
        match read_dir(dir) {
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

    /// Muat ulang hanya bila folder kerja berubah atau belum pernah dibaca.
    pub fn ensure(&mut self, dir: &Path) -> bool {
        if self.loaded && !self.is_stale(dir) {
            return false;
        }
        self.refresh(dir);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, is_dir: bool, size: u64) -> FsEntry {
        FsEntry {
            name: name.to_owned(),
            is_dir,
            size,
        }
    }

    #[test]
    fn folder_di_atas_lalu_nama_abjad() {
        let mut list = vec![
            entry("zebra.txt", false, 1),
            entry("src", true, 0),
            entry("Alpha.md", false, 2),
            entry("beta.log", false, 3),
        ];
        sort_entries(&mut list);
        let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["src", "Alpha.md", "beta.log", "zebra.txt"]);
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
    fn folder_tidak_ada_jadi_error_bukan_panic() {
        let mut c = FilesCache::new();
        c.refresh(Path::new("C:/tidak/ada/folder/ini"));
        assert!(c.error.is_some());
        assert!(c.entries.is_empty());
        assert!(c.loaded, "percobaan gagal tetap menandai sudah dicoba");
    }

    #[test]
    fn cache_hanya_membaca_ulang_saat_folder_berubah() {
        let dir = std::env::temp_dir().join("arka-fs-model-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("satu.txt"), b"halo").unwrap();

        let mut c = FilesCache::new();
        assert!(c.ensure(&dir), "baca pertama wajib");
        assert_eq!(c.entries.len(), 1);
        assert_eq!(c.entries[0].size, 4);
        assert!(!c.ensure(&dir), "folder sama → tidak membaca ulang");

        fs::write(dir.join("dua.txt"), b"hai").unwrap();
        // Masih tidak dibaca ulang: itu memang perilaku cache, dan tombol
        // "Muat ulang" yang memaksa lewat `refresh`.
        assert!(!c.ensure(&dir));
        c.refresh(&dir);
        assert_eq!(c.entries.len(), 2);

        // Folder lain → wajib baca ulang.
        assert!(c.ensure(&std::env::temp_dir()));
        let _ = fs::remove_dir_all(&dir);
    }
}
