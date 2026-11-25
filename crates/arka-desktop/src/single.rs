//! Satu instance aplikasi + argumen `--open <file>` untuk pratinjau.
//!
//! Windows: named mutex `Local\...` (per sesi login pengguna). Kalau mutex
//! sudah ada, proses kedua **tidak** membuka jendela kedua; ia hanya
//! melaporkan "sudah berjalan" dan (bila ada `--open`) menuliskan path ke
//! file `pending-open.txt` di folder data supaya instance pertama bisa
//! mempratinjau berkas itu. Ini sengaja sederhana: IPC penuh (named pipe)
//! belum diperlukan untuk satu berkas, dan jalur ini bisa diuji di Linux
//! karena parser argumennya murni.

// Beberapa item di bawah hanya dipakai lewat FFI/UI Windows; pada build
// Linux (CI) jalur itu tidak dikompilasi. Pembatasan ini sengaja
// HANYA untuk non-Windows: kalau ada kode mati saat build Windows,
// warningnya tetap muncul di sana.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::{Path, PathBuf};

pub const MUTEX_NAME: &str = "Local\\ArkaAgent.Desktop.SingleInstance";
pub const PENDING_FILE: &str = "pending-open.txt";

/// Hasil pemeriksaan instance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instance {
    /// Proses ini boleh membuka jendela.
    Primary,
    /// Instance lain sudah berjalan; jangan lanjutkan `run_native`.
    Secondary,
    /// Platform tanpa named mutex (Linux/CI): selalu izinkan.
    Unsupported,
}

/// Apa yang diminta lewat baris perintah.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Launch {
    /// `--open <file>`: pratinjau berkas ini saat UI terbuka.
    pub open: Option<PathBuf>,
    /// Argumen lain diteruskan apa adanya ke `run()` di `main.rs`.
    pub rest: Vec<String>,
}

/// Pisahkan `--open` dari argumen lain. Tidak memvalidasi path: berkas boleh
/// belum ada (pengguna salah ketik) dan UI yang akan menampilkan pesannya.
/// Path relatif yang diawali `-` harus ditulis `./-berkas.txt` supaya tidak
/// tertukar dengan flag.
pub fn parse_args(args: &[String]) -> Launch {
    let mut out = Launch::default();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--open" {
            // Argumen berikutnya yang diawali `-` adalah flag lain, bukan
            // nilai: `--open --status` berarti `--open` TANPA berkas, dan
            // `--status` tetap diteruskan. Tanpa pemeriksaan ini parser
            // diam-diam memakan flag itu sebagai nama berkas.
            let ada_nilai = args.get(i + 1).is_some_and(|v| !v.starts_with('-'));
            if ada_nilai {
                out.open = Some(PathBuf::from(&args[i + 1]));
                i += 2;
                continue;
            }
            // Biarkan `--open` apa adanya di `rest` supaya `main.rs` bisa
            // melaporkan argumen yang tidak dikenal, bukan menghilangkannya.
            out.rest.push(args[i].clone());
            i += 1;
            continue;
        }
        out.rest.push(args[i].clone());
        i += 1;
    }
    out
}

/// Tulis permintaan pratinjau untuk instance pertama. Mengembalikan `Err`
/// hanya kalau folder data tidak bisa ditulis — bukan kalau berkasnya hilang.
pub fn write_pending(data_dir: &Path, path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    // Teks UTF-8 (lossy untuk nama yang tidak bisa direpresentasikan) supaya
    // pembacanya tidak perlu tahu encoding `OsStr` tiap platform. Path Windows
    // pada umumnya tetap utuh; karakter aneh menjadi U+FFFD dan UI menampilkan
    // "berkas tidak ditemukan" alih-alih gagal membaca file pending selamanya.
    std::fs::write(
        data_dir.join(PENDING_FILE),
        path.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())
}

/// Ambil & hapus permintaan pratinjau (dipanggil instance pertama tiap beberapa detik).
pub fn take_pending(data_dir: &Path) -> Option<PathBuf> {
    let f = data_dir.join(PENDING_FILE);
    let bytes = std::fs::read(&f).ok()?;
    let _ = std::fs::remove_file(&f);
    let text = String::from_utf8(bytes).ok()?;
    let t = text.trim();
    if t.is_empty() {
        None
    } else {
        Some(PathBuf::from(t))
    }
}

/// Cek named mutex. Handle sengaja **tidak** pernah ditutup: kalau ditutup,
/// instance kedua akan mengira aplikasi sudah mati. Proses Windows menutup
/// semua handle saat keluar dengan sendirinya.
pub fn claim() -> Instance {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::System::Threading::CreateMutexW;

        let name: Vec<u16> = std::ffi::OsStr::new(MUTEX_NAME)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: pointer ke buffer UTF-16 lokal yang hidup selama panggilan;
        // kami tidak meminta kepemilikan awal (false) karena yang dibutuhkan
        // hanya keberadaan objeknya.
        let h = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if h.is_null() {
            // Gagal membuat mutex (kebijakan ketat): jangan mengunci pengguna
            // keluar dari aplikasinya sendiri → anggap primary.
            return Instance::Primary;
        }
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            return Instance::Secondary;
        }
        // Handle dileak dengan sengaja (lihat komentar fungsi).
        std::mem::forget(h);
        Instance::Primary
    }
    #[cfg(not(windows))]
    {
        Instance::Unsupported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_diambil_terpisah_dari_argumen_lain() {
        let a: Vec<String> = ["--status", "--open", "C:/a/b.txt", "--exec", "x"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let l = parse_args(&a);
        assert_eq!(l.open, Some(PathBuf::from("C:/a/b.txt")));
        assert_eq!(l.rest, vec!["--status", "--exec", "x"]);
    }

    #[test]
    fn open_tanpa_nilai_tidak_memakan_argumen_berikut() {
        let a: Vec<String> = ["--open", "--status"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let l = parse_args(&a);
        assert_eq!(l.open, None);
        assert_eq!(l.rest, vec!["--open", "--status"]);
    }

    #[test]
    fn pending_hanya_dibaca_sekali() {
        let dir = std::env::temp_dir().join(format!("arka-single-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let f = PathBuf::from("C:/ kerja/🚀.txt");
        write_pending(&dir, &f).unwrap();
        assert_eq!(take_pending(&dir), Some(f));
        assert_eq!(take_pending(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tanpa_windows_tidak_pernah_mengunci() {
        if !cfg!(windows) {
            assert_eq!(claim(), Instance::Unsupported);
        }
    }
}
