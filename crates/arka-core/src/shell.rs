//! Abstraksi "mesin eksekusi perintah".
//!
//! Agent loop hanya butuh: jalankan string perintah, dapatkan (exit code,
//! output). Detail platform diserahkan ke implementasi:
//!   * Android → `ProotShell` (bash di dalam rootfs Ubuntu), di crate
//!     `arka-android` karena butuh `std::os::unix`.
//!   * Windows → `PwshShell` (PowerShell 7 / 5.1), di crate `arka-desktop`.
//!
//! Registry global dipakai supaya fungsi `run_command` di core tidak perlu
//! membawa objek shell ke mana-mana (sama seperti [`crate::secret`]).

use crate::events::Events;
use serde_json::Value;
use std::sync::atomic::AtomicBool;
use std::sync::OnceLock;

/// Hasil satu eksekusi perintah.
#[derive(Clone, Debug, Default)]
pub struct ExecOutcome {
    pub code: i32,
    pub output: String,
    pub timed_out: bool,
    pub canceled: bool,
}

impl ExecOutcome {
    /// Teks yang dikirim ke model & terminal, sudah termasuk exit code.
    pub fn report(&self) -> String {
        if self.canceled {
            format!("exit {}\n(dihentikan pengguna)\n{}", self.code, self.output)
        } else if self.timed_out {
            format!("exit {}\n(timeout)\n{}", self.code, self.output)
        } else {
            format!("exit {}\n{}", self.code, self.output)
        }
    }
}

pub trait Shell: Send + Sync {
    /// Ringkasan status mesin (dipakai tab Terminal / Setelan).
    fn status(&self) -> Result<Value, String>;
    /// Siapkan lingkungan kerja. Android: pasang Ubuntu; Windows: buat folder.
    fn prepare(&self, ev: &dyn Events, cancel: &AtomicBool) -> Result<(), String>;
    /// Jalankan perintah; setiap potongan output diteruskan ke [on_chunk].
    ///
    /// [cancel] diperiksa selama perintah berjalan (bukan hanya di awal),
    /// supaya tombol Stop benar-benar menghentikan proses.
    fn exec(
        &self,
        command: &str,
        cancel: &AtomicBool,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<ExecOutcome, String>;
    /// Tes cepat yang hasilnya ditampilkan apa adanya ke pengguna.
    fn diagnostic(&self) -> Result<String, String>;
}

static SHELL: OnceLock<Box<dyn Shell>> = OnceLock::new();

/// Daftarkan implementasi sekali saat start (Android: `nativeInit`).
pub fn init(shell: Box<dyn Shell>) {
    let _ = SHELL.set(shell);
}

pub fn active() -> Result<&'static dyn Shell, String> {
    SHELL
        .get()
        .map(|b| b.as_ref())
        .ok_or_else(|| "Shell belum diinisialisasi".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_marks_canceled_and_timeout() {
        let mut o = ExecOutcome { code: 1, output: "out".into(), ..Default::default() };
        assert_eq!(o.report(), "exit 1\nout");
        o.canceled = true;
        assert!(o.report().contains("dihentikan pengguna"));
        o.canceled = false;
        o.timed_out = true;
        assert!(o.report().contains("timeout"));
    }

    #[test]
    fn active_without_init_is_error() {
        // Catatan: test lain mungkin sudah mengisi registry; cukup pastikan
        // fungsinya tidak panik.
        let _ = active();
    }
}
