//! Notifikasi sistem Windows (toast) saat balasan selesai dan jendela tidak
//! fokus.
//!
//! Padanan `Notifications.kt` di Android, tapi jalurnya berbeda: Android punya
//! `NotificationManager` milik aplikasi sendiri, sedangkan WinRT toast butuh
//! **AppUserModelID** yang terdaftar. Tanpa pendaftaran, Windows memakai AUMID
//! PowerShell sebagai cadangan — toast tetap muncul, tapi bernama
//! "Windows PowerShell", dan itu membingungkan (pengguna melihat nama program
//! lain setiap kali agen selesai bekerja).
//!
//! Karena itu [`register_app_id`] menulis dua nilai registry di
//! `HKCU\Software\Classes\AppUserModelId\<id>`. Pendaftaran bersifat
//! *best effort*: kalau gagal (kebijakan perusahaan, registry dikunci), toast
//! tetap dikirim memakai AUMID bawaan — lebih baik muncul dengan nama salah
//! daripada tidak muncul sama sekali.

// Beberapa item di bawah hanya dipakai lewat FFI/UI Windows; pada build
// Linux (CI) jalur itu tidak dikompilasi. Pembatasan ini sengaja
// HANYA untuk non-Windows: kalau ada kode mati saat build Windows,
// warningnya tetap muncul di sana.
#![cfg_attr(not(windows), allow(dead_code))]

#[cfg(windows)]
use std::time::Duration;

/// AUMID aplikasi. Bentuk `Company.Product.Subsystem` seperti kebiasaan
/// Windows; stabil antar versi supaya setelan Notifikasi milik pengguna
/// (jika suatu saat ada) tidak perlu diperbarui.
pub const APP_ID: &str = "ArkaAgent.Desktop";

/// Nama yang tampil di judul toast & Action Center.
pub const DISPLAY_NAME: &str = "Arka Agent";

/// Berapa lama toast bertahan. Windows 10/11 mengabaikan angka ini untuk
/// sebagian skenario, jadi nilainya juga bukan janji — hanya petunjuk.
pub const TOAST_SECS: u64 = 6;

/// Hasil pendaftaran, untuk dilaporkan sekali di log (bukan setiap frame).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Registered {
    /// AUMID terdaftar: toast memakai nama aplikasi sendiri.
    Ok,
    /// Registry menolak; toast tetap dikirim dengan AUMID PowerShell.
    Failed(String),
    /// Platform ini tidak punya toast (dipakai saat mengembangkan di Linux).
    Unsupported,
}

/// Daftarkan AUMID. Panggil sekali saat start.
pub fn register_app_id() -> Registered {
    #[cfg(windows)]
    {
        match win::daftar(APP_ID, DISPLAY_NAME) {
            Ok(()) => Registered::Ok,
            Err(e) => Registered::Failed(e.to_string()),
        }
    }
    #[cfg(not(windows))]
    {
        Registered::Unsupported
    }
}

/// Kirim satu toast. Tidak pernah panic: semua kegagalan jadi `Err`.
pub fn toast(title: &str, body: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use notify_rust::Notification;
        let build = |app: Option<&str>| {
            let mut n = Notification::new();
            n.summary(title)
                .body(body)
                .appname(DISPLAY_NAME)
                .timeout(Duration::from_secs(TOAST_SECS));
            if let Some(id) = app {
                n.app_id(id);
            }
            n.show().map(|_| ())
        };
        match build(Some(APP_ID)) {
            Ok(()) => return Ok(()),
            // AUMID belum terdaftar (mis. registry dikunci): kirim ulang tanpa
            // app_id supaya WinRT memakai cadangan PowerShell. Ini jalur yang
            // membuat notifikasi tetap berguna di mesin yang dibatasi.
            Err(first) => match build(None) {
                Ok(()) => Ok(()),
                Err(e) => Err(format!("{first}; cadangan: {e}")),
            },
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (title, body);
        Err("notifikasi sistem hanya tersedia di Windows".into())
    }
}

/// Potong isi balasan menjadi ringkasan satu baris untuk toast.
///
/// Action Center membungkus teks panjang menjadi toast raksasa; yang berguna
/// bagi pengguna hanya awal kalimat (atau judul sesi). Baris baru diratakan
/// supaya tidak ada toast yang tingginya menutupi setengah layar.
pub fn ringkas(text: &str, max_chars: usize) -> String {
    let flat: String = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if flat.chars().count() <= max_chars {
        return flat;
    }
    let mut out: String = flat.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(windows)]
mod win {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_WRITE, REG_SZ,
    };

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    /// Tulis `DisplayName` (dan `IconUri` bila ada) untuk AUMID ini.
    pub fn daftar(app_id: &str, nama: &str) -> std::io::Result<()> {
        let sub = format!("Software\\Classes\\AppUserModelId\\{app_id}");
        let mut hkey: HKEY = std::ptr::null_mut();
        // SAFETY: semua pointer menunjuk ke data lokal yang hidup selama
        // panggilan; `hkey` ditutup sebelum fungsi kembali.
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                wide(&sub).as_ptr(),
                0,
                std::ptr::null(),
                0,
                KEY_WRITE,
                std::ptr::null(),
                &mut hkey,
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            return Err(std::io::Error::from_raw_os_error(rc as i32));
        }

        let mut result = Ok(());
        for (key, value) in [("DisplayName", nama)] {
            let v = wide(value);
            // SAFETY: `hkey` terbuka untuk tulis; buffer UTF-16 hidup selama
            // panggilan dan panjangnya dihitung dalam byte (len * 2).
            let rc = unsafe {
                RegSetValueExW(
                    hkey,
                    wide(key).as_ptr(),
                    0,
                    REG_SZ,
                    v.as_ptr().cast::<u8>(),
                    (v.len() * 2) as u32,
                )
            };
            if rc != 0 {
                result = Err(std::io::Error::from_raw_os_error(rc as i32));
                break;
            }
        }
        unsafe { RegCloseKey(hkey) };
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ringkas_meratakan_baris_baru() {
        assert_eq!(ringkas("halo\ndunia\n", 40), "halo dunia");
        assert_eq!(ringkas("  satu  \n\n  dua ", 40), "satu dua");
        assert_eq!(ringkas("", 40), "");
    }

    #[test]
    fn ringkas_memotong_dengan_elipsis() {
        let s = ringkas(&"a".repeat(100), 10);
        assert_eq!(s.chars().count(), 10, "total tetap <= batas: {s}");
        assert!(s.ends_with('\u{2026}'));
    }

    #[test]
    fn ringkas_menghitung_karakter_bukan_byte() {
        // Emoji 4 byte: kalau dipotong per byte, hasilnya karakter rusak.
        let s = ringkas("🚀🚀🚀🚀🚀", 4);
        assert_eq!(s.chars().count(), 4);
        assert!(s.starts_with("🚀"));
    }

    #[test]
    fn app_id_stabil() {
        // Nilai ini juga dipakai registry; mengubahnya membuat toast lama
        // menggantung di Action Center sebagai aplikasi tak dikenal.
        assert_eq!(APP_ID, "ArkaAgent.Desktop");
        assert_eq!(DISPLAY_NAME, "Arka Agent");
    }

    #[test]
    fn tanpa_windows_toast_mengembalikan_err_bukan_panic() {
        #[cfg(not(windows))]
        assert!(toast("a", "b").is_err());
        #[cfg(not(windows))]
        assert_eq!(register_app_id(), Registered::Unsupported);
    }
}
