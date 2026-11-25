//! Cegah Windows tidur selama agent sedang menjawab.
//!
//! Padanan wakelock/foreground service Android. Bedanya: Windows tidak punya
//! "wakelock" per proses, melainkan **per thread** lewat
//! `SetThreadExecutionState`. Karena itu semua panggilan dilakukan dari satu
//! thread khusus ("penjaga") yang hidup selama streaming; kalau dipanggil dari
//! thread agent yang lalu selesai, Windows akan menganggap permintaan itu
//! ikut hilang dan layar boleh mati lagi di tengah balasan.
//!
//! `ES_SYSTEM_REQUIRED` saja (tanpa `ES_DISPLAY_REQUIRED`): mencegah sleep,
//! tapi tidak memaksa layar tetap menyala — lebih sopan untuk pengguna yang
//! sengaja meminimkan jendela sambil menunggu jawaban panjang.

// Sebagian item (TICK, guard_loop, active) hanya dipakai pada build Windows
// atau dari unit test. Di Linux/CI jalur FFI-nya tidak dikompilasi, jadi
// jangan lempar warning dead-code untuk kode yang memang hidup di Windows.
#![cfg_attr(not(windows), allow(dead_code))]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Jumlah permintaan aktif. Dipakai sebagai *reference count* supaya dua sesi
/// chat paralel tidak saling mematikan penjaga: penjaga berhenti hanya kalau
/// **semua** sesi selesai.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

/// Thread penjaga. `None` = tidak ada yang sedang dijaga.
static GUARD: Mutex<Option<Guard>> = Mutex::new(None);

/// Berapa lama penjaga menunggu sebelum memastikan flag masih dibutuhkan.
const TICK: Duration = Duration::from_secs(20);

struct Guard {
    /// Disetel `false` saat hendak menghentikan penjaga.
    live: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Tandai satu stream dimulai. Aman dipanggil berulang kali & dari thread mana pun.
pub fn begin() {
    ACTIVE.fetch_add(1, Ordering::SeqCst);
    let mut g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
    if g.is_some() {
        return;
    }
    let live = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    // Windows: API ini harus dipanggil dari thread yang tetap hidup.
    #[cfg(windows)]
    {
        let flag = live.clone();
        std::thread::Builder::new()
            .name("arka-keepawake".into())
            .spawn(move || guard_loop(&flag))
            .ok();
    }
    *g = Some(Guard { live });
}

/// Tandai satu stream selesai. Penjaga benar-benar dimatikan saat hitungan 0.
pub fn end() {
    let prev = ACTIVE
        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| {
            Some(n.saturating_sub(1))
        })
        .unwrap_or(0);
    if prev > 1 {
        return;
    }
    let mut g = GUARD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(guard) = g.take() {
        guard.live.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

/// Jumlah stream yang masih berjalan (dipakai test & diagnostik).
pub fn active() -> usize {
    ACTIVE.load(Ordering::SeqCst)
}

#[cfg(windows)]
fn guard_loop(live: &std::sync::atomic::AtomicBool) {
    use windows_sys::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };
    // SAFETY: fungsi tanpa argumen pointer; mengembalikan 0 bila gagal, yang
    // berarti sistem tetap boleh tidur — tidak ada state yang perlu dibersihkan.
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
    }
    while live.load(std::sync::atomic::Ordering::SeqCst) && ACTIVE.load(Ordering::SeqCst) > 0 {
        std::thread::sleep(TICK);
        // Perpanjang setiap TICK: beberapa kebijakan daya menghapus flag saat
        // layar sempat mati, jadi diperbaharui selama stream masih berjalan.
        unsafe {
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
        }
    }
    unsafe {
        SetThreadExecutionState(ES_CONTINUOUS);
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
fn guard_loop(_: &std::sync::atomic::AtomicBool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hitungan_naik_turun_dan_tidak_negatif() {
        // Test ini bisa berjalan berdampingan dengan test lain yang juga
        // memakai counter global, jadi bandingkan *selisih*, bukan nilai mutlak.
        let base = active();
        begin();
        assert_eq!(active(), base + 1);
        begin();
        assert_eq!(active(), base + 2);
        end();
        assert_eq!(active(), base + 1);
        end();
        assert_eq!(active(), base);
        end();
        assert_eq!(active(), base, "tidak boleh turun di bawah nol");
    }
}
