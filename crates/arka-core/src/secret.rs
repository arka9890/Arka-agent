//! Enkripsi API key — abstraksi per platform.
//!
//! Android: enkripsi dilakukan Kotlin (`ApiKeyEncryption.kt` + Keystore), jadi
//! Rust mendaftarkan [`NoopSecretStore`] dan nilai lewat apa adanya.
//! Windows: crate desktop mendaftarkan `DpapiSecretStore` (M5), dan nilai
//! disimpan dengan prefix [`DPAPI_PREFIX`] supaya bisa dibedakan dari data
//! plaintext versi lama.
//!
//! Semua lewat satu registry supaya `provider.rs` tidak perlu tahu platform.

use std::sync::OnceLock;

/// Penanda nilai yang sudah dilindungi DPAPI (Windows).
pub const DPAPI_PREFIX: &str = "dpapi:";

pub trait SecretStore: Send + Sync {
    /// Lindungi nilai rahasia. Mengembalikan string siap simpan ke DB.
    fn protect(&self, plain: &str) -> Result<String, String>;
    /// Buka nilai rahasia. Nilai tanpa prefix platform dianggap plaintext
    /// (data lama) dan dikembalikan apa adanya.
    fn unprotect(&self, stored: &str) -> Result<String, String>;
}

/// Tidak melakukan apa pun (Android — enkripsi di sisi Kotlin).
pub struct NoopSecretStore;

impl SecretStore for NoopSecretStore {
    fn protect(&self, plain: &str) -> Result<String, String> {
        Ok(plain.to_string())
    }

    fn unprotect(&self, stored: &str) -> Result<String, String> {
        Ok(stored.to_string())
    }
}

static STORE: OnceLock<Box<dyn SecretStore>> = OnceLock::new();

/// Daftarkan implementasi. Aman dipanggil berkali-kali (yang pertama menang).
pub fn init(store: Box<dyn SecretStore>) {
    let _ = STORE.set(store);
}

fn store() -> &'static dyn SecretStore {
    // Tanpa registrasi (mis. unit test) perilakunya = Noop.
    STORE.get().map(|b| b.as_ref()).unwrap_or(&NoopSecretStore)
}

pub fn protect(plain: &str) -> Result<String, String> {
    store().protect(plain)
}

pub fn unprotect(stored: &str) -> Result<String, String> {
    store().unprotect(stored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_round_trip() {
        let s = NoopSecretStore;
        assert_eq!(s.protect("sk-123").unwrap(), "sk-123");
        assert_eq!(s.unprotect("sk-123").unwrap(), "sk-123");
    }

    #[test]
    fn uninitialized_registry_is_noop() {
        assert_eq!(protect("abc").unwrap(), "abc");
        assert_eq!(unprotect(DPAPI_PREFIX).unwrap(), DPAPI_PREFIX);
    }
}
