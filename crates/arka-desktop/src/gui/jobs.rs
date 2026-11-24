//! Pekerjaan jaringan tab Provider di thread latar.
//!
//! `ai::test_connection` dan `ai::refresh_models` adalah panggilan HTTP
//! **blokir** (reqwest blocking, timeout 30 s). Kalau dipanggil dari `draw`,
//! jendela membeku sampai provider menjawab — atau sampai timeout saat API
//! key salah. Jadi keduanya berjalan di thread sendiri dan UI hanya memeriksa
//! saluran sekali per frame, pola yang sama dengan [`super::engine`].
//!
//! Satu pekerjaan pada satu waktu. Ini disengaja: menekan "Tes" berkali-kali
//! tidak boleh menumpuk permintaan ke provider yang sama.

use std::sync::mpsc::{self, Receiver, TryRecvError};

use arka_core::ai;
use arka_core::provider::Provider;
use eframe::egui::Context;

/// Apa yang sedang dikerjakan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JobKind {
    /// `GET /models` untuk membuktikan kredensial & URL benar.
    Test,
    /// `GET /models` lalu gabungkan dengan model yang sudah dipilih.
    Models,
}

impl JobKind {
    pub fn label(self, lang: Lang3) -> &'static str {
        match (self, lang) {
            (JobKind::Test, Lang3::Id) => "Menguji koneksi\u{2026}",
            (JobKind::Test, Lang3::En) => "Testing connection\u{2026}",
            (JobKind::Test, Lang3::Ja) => "\u{63a5}\u{7d9a}\u{3092}\u{78ba}\u{8a8d}\u{4e2d}\u{2026}",
            (JobKind::Models, Lang3::Id) => "Mengambil daftar model\u{2026}",
            (JobKind::Models, Lang3::En) => "Fetching model list\u{2026}",
            (JobKind::Models, Lang3::Ja) => "\u{30e2}\u{30c7}\u{30eb}\u{4e00}\u{89a7}\u{3092}\u{53d6}\u{5f97}\u{4e2d}\u{2026}",
        }
    }
}

/// Supaya modul ini tidak bergantung pada `i18n::Lang` (dan tetap bisa diuji
/// tanpa state UI), bahasanya diteruskan sebagai enum kecil.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang3 {
    Id,
    En,
    Ja,
}

/// Hasil yang dikirim thread ke UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobDone {
    /// Pesan siap tampil (hasil tes koneksi).
    Message(String),
    /// Daftar model dari server.
    Models(Vec<String>),
}

/// Pemegang saluran hasil. Pemiliknya `AppState`.
pub struct NetJob {
    pub kind: JobKind,
    rx: Receiver<Result<JobDone, String>>,
    /// Hasil sudah diambil; panggilan `poll` berikutnya mengembalikan `None`.
    done: bool,
}

impl NetJob {
    /// Ambil hasil kalau sudah ada. `None` = masih berjalan.
    pub fn poll(&mut self) -> Option<Result<JobDone, String>> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(v) => {
                self.done = true;
                Some(v)
            }
            Err(TryRecvError::Empty) => None,
            // Thread berhenti tanpa mengirim apa pun (panic). Tanpa ini, UI
            // akan menampilkan "Menguji koneksi…" selamanya.
            Err(TryRecvError::Disconnected) => {
                self.done = true;
                Some(Err("Pekerjaan berhenti tanpa hasil".into()))
            }
        }
    }

    pub fn running(&self) -> bool {
        !self.done
    }
}

/// Jalankan [kind] terhadap [provider] di thread baru.
///
/// `ctx.request_repaint()` dipanggil dari thread itu supaya hasilnya langsung
/// terlihat tanpa menunggu mouse bergerak.
pub fn start(kind: JobKind, provider: Provider, ctx: Context) -> NetJob {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let result = match kind {
            JobKind::Test => ai::test_connection(&provider).map(JobDone::Message),
            JobKind::Models => ai::refresh_models(&provider).map(JobDone::Models),
        };
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    NetJob {
        kind,
        rx,
        done: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn job_selesai(kind: JobKind, f: impl FnOnce() -> Result<JobDone, String> + Send + 'static) -> NetJob {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        NetJob {
            kind,
            rx,
            done: false,
        }
    }

    #[test]
    fn hasil_diambil_sekali_lalu_selesai() {
        let mut j = job_selesai(JobKind::Test, || Ok(JobDone::Message("ok".into())));
        let mut got = None;
        let deadline = Instant::now() + Duration::from_secs(2);
        while got.is_none() && Instant::now() < deadline {
            got = j.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, Some(Ok(JobDone::Message("ok".into()))));
        assert!(j.poll().is_none(), "hasil yang sama tidak boleh dikirim dua kali");
        assert!(!j.running());
    }

    #[test]
    fn thread_mati_tanpa_hasil_bukan_menunggu_selamanya() {
        // Thread mengirim? Tidak. Sender hilang → poll harus lapor gagal.
        let (tx, rx) = mpsc::channel::<Result<JobDone, String>>();
        drop(tx);
        let mut j = NetJob {
            kind: JobKind::Models,
            rx,
            done: false,
        };
        let got = j.poll().expect("harus ada hasil");
        assert!(got.is_err());
        assert!(j.poll().is_none());
    }

    #[test]
    fn pekerjaan_masih_berjalan_tidak_mengembalikan_apa_pun() {
        let (tx, rx) = mpsc::channel::<Result<JobDone, String>>();
        let mut j = NetJob {
            kind: JobKind::Test,
            rx,
            done: false,
        };
        assert!(j.running());
        assert!(j.poll().is_none());
        drop(tx);
    }
}
