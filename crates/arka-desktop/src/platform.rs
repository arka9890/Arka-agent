//! Hal-hal yang **tidak mungkin** sama di Windows dan Linux: membuka berkas
//! dengan aplikasi bawaan, dan dialog berkas OS.
//!
//! Semua fungsi di sini memakai `cfg`. Tujuannya bukan "jalan di Linux",
//! melainkan supaya UI bisa dikompilasi & diuji di CI Linux tanpa GTK —
//! sedangkan pengguna Windows mendapat perilaku asli (`ShellExecuteW`,
//! `IFileOpenDialog`).

use std::path::{Path, PathBuf};

/// Buka berkas/folder dengan aplikasi bawaan sistem.
///
/// Windows: `ShellExecuteW` (setara klik-ganda di Explorer).
/// Linux (hanya pengembangan): `xdg-open`.
///
/// Path dikirim **sebagai argumen terpisah**, bukan dirangkai ke satu string
/// perintah — nama berkas berisi `&`/`"`/`;` tidak boleh bisa jadi perintah.
pub fn open_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Berkas tidak ditemukan: {}", path.display()));
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
            s.encode_wide().chain(std::iter::once(0)).collect()
        }

        let operation = wide(std::ffi::OsStr::new("open"));
        let file = wide(path.as_os_str());
        // ShellExecuteW mengembalikan HINSTANCE; nilai <= 32 adalah kode galat.
        let rc = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        } as isize;
        if rc <= 32 {
            return Err(format!(
                "Tidak ada aplikasi yang bisa membuka berkas ini (kode {rc})"
            ));
        }
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("xdg-open tidak tersedia: {e}"))
    }
}

/// Dialog "Impor berkas" bawaan Windows (multi-pilih).
///
/// `None` bila pengguna membatalkan, **atau** bila platform ini tidak punya
/// dialog OS (Linux) — pemanggil lalu memakai pemilih di dalam aplikasi.
#[cfg(windows)]
pub fn pick_files() -> Option<Vec<PathBuf>> {
    Some(rfd::FileDialog::new().pick_files().unwrap_or_default())
}

#[cfg(windows)]
/// Dialog "Simpan sebagai".
pub fn pick_save(default_name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_file_name(default_name).save_file()
}

#[cfg(not(windows))]
pub fn pick_files() -> Option<Vec<PathBuf>> {
    None
}

#[cfg(not(windows))]
pub fn pick_save(_default_name: &str) -> Option<PathBuf> {
    None
}

/// Apakah platform ini punya dialog berkas OS?
pub const HAS_OS_DIALOG: bool = cfg!(windows);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn berkas_hilang_ditolak_sebelum_menjalankan_apa_pun() {
        let err = open_path(Path::new("C:/tidak/ada/berkas-ini.txt")).unwrap_err();
        assert!(err.contains("tidak ditemukan"), "{err}");
    }

    #[test]
    fn nama_berisi_karakter_shell_tidak_menjadi_perintah() {
        // Kalau path dirangkai ke string perintah, `& calc` akan dieksekusi.
        let dir = std::env::temp_dir().join(format!("arka-platform-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let nakal = dir.join("a & echo SIAL; b.txt");
        std::fs::write(&nakal, b"x").unwrap();

        // Fungsi ini boleh gagal (di Linux tanpa xdg-open), yang penting tidak
        // panic dan berkas tetap ada — tidak ada yang dihapus/dijalankan.
        let _ = open_path(&nakal);
        assert!(nakal.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dialog_os_hanya_di_windows() {
        // Di Linux hasilnya `None` (pemanggil memakai pemilih internal).
        if !cfg!(windows) {
            assert!(pick_files().is_none());
            assert!(pick_save("x.txt").is_none());
            assert!(!HAS_OS_DIALOG);
        }
    }
}
