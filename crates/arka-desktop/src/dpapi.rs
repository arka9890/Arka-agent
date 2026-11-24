//! Enkripsi API key dengan **Windows DPAPI** (`CryptProtectData`).
//!
//! Kenapa DPAPI dan bukan kunci sendiri: DPAPI mengikat data ke akun Windows
//! pengguna (dan opsional ke mesinnya). Tidak ada kunci yang perlu disimpan
//! aplikasi, jadi tidak ada kunci yang bisa bocor bersama `arka.db`. Salinan
//! file database di komputer lain **tidak** bisa dibuka.
//!
//! Bentuk nilai di database: `dpapi:<base64>` (lihat `DPAPI_PREFIX` di core).
//! Nilai tanpa prefix dianggap plaintext dari versi lama dan dikembalikan apa
//! adanya; `provider::migrate_plaintext_keys()` yang kemudian mengenkripsinya.
//!
//! ### Kenapa `Protector` dipisah dari `SecretStore`
//! DPAPI hanya ada di Windows, sedangkan unit test UI berjalan di Linux (CI).
//! Seluruh logika yang mudah salah — prefix, base64, teks kosong, ciphertext
//! rusak, data lama — ada di [`DpapiSecretStore`] dan diuji dengan
//! [`FakeProtector`]. Yang tersisa di jalur Windows hanyalah pemanggilan FFI
//! ±40 baris, dan itu dibuktikan oleh test di `windows-latest`.

use arka_core::secret::{SecretStore, DPAPI_PREFIX};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

/// Enkripsi/dekripsi mentah sebuah blob.
///
/// Di Linux seluruh tipe di sini hanya dipakai unit test (`FakeProtector`);
/// di Windows `default_store()` memakai [`WinProtector`] sungguhan.
#[allow(dead_code)]
pub trait Protector: Send + Sync {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String>;
    fn open(&self, blob: &[u8]) -> Result<Vec<u8>, String>;
}

/// [`SecretStore`] yang menyimpan nilai sebagai `dpapi:<base64 blob>`.
#[allow(dead_code)]
pub struct DpapiSecretStore<P: Protector> {
    protector: P,
}

#[allow(dead_code)]
impl<P: Protector> DpapiSecretStore<P> {
    pub fn new(protector: P) -> Self {
        Self { protector }
    }

    /// Nilai siap simpan (untuk dipakai test & diagnosa).
    pub fn encode(&self, plain: &str) -> Result<String, String> {
        if plain.is_empty() {
            return Ok(String::new());
        }
        let blob = self.protector.seal(plain.as_bytes())?;
        Ok(format!("{DPAPI_PREFIX}{}", B64.encode(blob)))
    }
}

#[allow(dead_code)]
impl<P: Protector> SecretStore for DpapiSecretStore<P> {
    fn protect(&self, plain: &str) -> Result<String, String> {
        self.encode(plain)
    }

    fn unprotect(&self, stored: &str) -> Result<String, String> {
        // Tanpa prefix = plaintext lama (atau nilai dari Android yang sudah
        // didekripsi Kotlin). Jangan dianggap rusak.
        let Some(rest) = stored.strip_prefix(DPAPI_PREFIX) else {
            return Ok(stored.to_string());
        };
        // Blob kosong berarti kolom kosong berprefix — bukan ciphertext rusak
        // yang perlu ditampilkan sebagai error.
        if rest.is_empty() {
            return Ok(String::new());
        }
        let blob = B64
            .decode(rest)
            .map_err(|e| format!("API key tersimpan bukan base64 yang sah: {e}"))?;
        let plain = self
            .protector
            .open(&blob)
            .map_err(|e| format!("API key tidak bisa dibuka di mesin ini: {e}"))?;
        String::from_utf8(plain)
            .map_err(|_| "API key tersimpan bukan teks UTF-8 yang sah".to_string())
    }

    fn owns(&self, stored: &str) -> bool {
        stored.starts_with(DPAPI_PREFIX)
    }
}

/// Store bawaan platform: DPAPI di Windows, Noop di tempat lain.
///
/// Noop **hanya** untuk pengembangan UI di Linux; di Windows fungsinya selalu
/// diganti DPAPI (dipanggil `main.rs`).
#[allow(dead_code)]
pub fn default_store() -> Box<dyn SecretStore> {
    #[cfg(windows)]
    {
        Box::new(DpapiSecretStore::new(WinProtector))
    }
    #[cfg(not(windows))]
    {
        Box::new(arka_core::secret::NoopSecretStore)
    }
}

// ── Jalur Windows ──────────────────────────────────────────────────────────

#[cfg(windows)]
pub struct WinProtector;

#[cfg(windows)]
impl Protector for WinProtector {
    fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
        let mut input = plain.to_vec();
        let blob = unsafe { protect(&mut input, true)? };
        Ok(blob)
    }

    fn open(&self, blob: &[u8]) -> Result<Vec<u8>, String> {
        let mut input = blob.to_vec();
        unsafe { protect(&mut input, false) }
    }
}

/// Satu jalur FFI untuk protect & unprotect.
///
/// `CryptProtectData` dan `CryptUnprotectData` punya bentuk parameter yang
/// sama (blob masuk, blob keluar, satu parameter keempat kosong), jadi
/// keduanya dilayani satu fungsi — tidak ada dua salinan kode `unsafe` yang
/// bisa menyimpang.
///
/// `CRYPTPROTECT_UI_FORBIDDEN`: DPAPI tidak boleh memunculkan dialog apa pun.
/// Aplikasi ini GUI; dialog modal dari dalam pemanggilan DB akan membekukan UI.
#[cfg(windows)]
unsafe fn protect(input: &mut [u8], seal: bool) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_mut_ptr(),
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();

    let ok = if seal {
        CryptProtectData(
            &in_blob,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    } else {
        CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };

    if ok == 0 {
        return Err(format!("kode error Windows {}", GetLastError()));
    }

    let bytes = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize).to_vec();
    // Buffer keluaran DPAPI dialokasikan dengan LocalAlloc → wajib LocalFree.
    LocalFree(out_blob.pbData as *mut core::ffi::c_void);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protector palsu: XOR 0x5A lalu tautologis — cukup untuk membuktikan
    /// nilai di DB **bukan** plaintext dan bolak-baliknya utuh.
    struct FakeProtector;

    impl Protector for FakeProtector {
        fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, String> {
            Ok(plain.iter().map(|b| b ^ 0x5A).collect())
        }
        fn open(&self, blob: &[u8]) -> Result<Vec<u8>, String> {
            Ok(blob.iter().map(|b| b ^ 0x5A).collect())
        }
    }

    /// Protector yang selalu gagal, meniru DPAPI di mesin lain.
    struct BrokenProtector;

    impl Protector for BrokenProtector {
        fn seal(&self, _plain: &[u8]) -> Result<Vec<u8>, String> {
            Err("gagal mengenkripsi".into())
        }
        fn open(&self, _blob: &[u8]) -> Result<Vec<u8>, String> {
            Err("DPAPI: kunci milik pengguna lain".into())
        }
    }

    fn store() -> DpapiSecretStore<FakeProtector> {
        DpapiSecretStore::new(FakeProtector)
    }

    #[test]
    fn nilai_tersimpan_bukan_plaintext_dan_bolak_balik_utuh() {
        let s = store();
        for key in ["sk-abc123", "  spasi di tepi  ", "🔑 emoji", "kunci-panjang-".repeat(20).as_str()] {
            let stored = s.protect(key).unwrap();
            assert!(stored.starts_with(DPAPI_PREFIX), "prefix hilang: {stored}");
            assert!(!stored.contains(key), "nilai asli masih terbaca di {stored}");
            // Bukan sekadar prefix: isinya base64 yang sah.
            let b64 = stored.strip_prefix(DPAPI_PREFIX).unwrap();
            assert!(B64.decode(b64).is_ok(), "bukan base64: {b64}");
            assert_eq!(s.unprotect(&stored).unwrap(), key);
        }
    }

    #[test]
    fn kunci_kosong_tetap_kosong_tanpa_prefix() {
        let s = store();
        assert_eq!(s.protect("").unwrap(), "");
        assert!(s.protect("").unwrap().is_empty(), "kolom kosong harus tetap kosong");
    }

    #[test]
    fn plaintext_lama_dikembalikan_apa_adanya() {
        let s = store();
        assert_eq!(s.unprotect("sk-lama-plaintext").unwrap(), "sk-lama-plaintext");
        assert!(!s.owns("sk-lama-plaintext"));
        assert!(s.owns(&s.protect("sk").unwrap()));
    }

    #[test]
    fn ciphertext_rusak_jadi_error_bukan_panic_atau_kunci_palsu() {
        let s = store();
        let err = s.unprotect("dpapi:!!bukan base64!!").unwrap_err();
        assert!(err.contains("base64"), "pesan tidak jelas: {err}");
        // Prefix sah, isi sah secara base64, tetapi protector menolak.
        let broken = DpapiSecretStore::new(BrokenProtector);
        let err = broken.unprotect("dpapi:AAAA").unwrap_err();
        assert!(err.contains("tidak bisa dibuka"), "pesan tidak jelas: {err}");
        let err = broken.protect("sk").unwrap_err();
        assert!(err.contains("gagal"), "pesan tidak jelas: {err}");
    }

    #[test]
    fn blob_kosong_berprefix_bukan_error() {
        let s = store();
        assert_eq!(s.unprotect(DPAPI_PREFIX).unwrap(), "");
    }

    /// Satu-satunya test yang benar-benar memanggil **DPAPI asli**. Hanya
    /// berjalan di Windows (runner `windows-latest`), karena DPAPI butuh akun
    /// pengguna Windows. Inilah bukti bahwa jalur FFI ±40 baris itu bekerja,
    /// bukan sekadar ter-compile.
    #[cfg(windows)]
    #[test]
    fn dpapi_asli_bolak_balik() {
        let s = DpapiSecretStore::new(WinProtector);
        for key in ["sk-asli-123", "kunci dengan spasi & emoji 🔑"] {
            let stored = s.protect(key).unwrap();
            assert!(stored.starts_with(DPAPI_PREFIX), "prefix hilang: {stored}");
            assert!(!stored.contains(key), "nilai asli masih terbaca: {stored}");
            assert_eq!(s.unprotect(&stored).unwrap(), key, "bolak-balik DPAPI gagal");
        }
        // Dua kali enkripsi menghasilkan ciphertext berbeda (salt acak DPAPI);
        // keduanya tetap bisa dibuka ke nilai yang sama.
        let a = s.protect("sk-sama").unwrap();
        let b = s.protect("sk-sama").unwrap();
        assert_ne!(a, b, "ciphertext sama — enkripsi tidak berjalan");
        assert_eq!(s.unprotect(&a).unwrap(), "sk-sama");
        assert_eq!(s.unprotect(&b).unwrap(), "sk-sama");
    }
}
