//! Statistik mesin untuk tab Setelan: arsitektur, RAM, ruang disk.
//!
//! Dipisah dari kode UI karena tiga alasan:
//!   * `GetDiskFreeSpaceExW` di folder **jaringan** bisa memblokir beberapa
//!     detik — memanggilnya dari `draw` membekukan jendela (pola yang sama
//!     dengan deteksi PowerShell di M7);
//!   * Windows memakai FFI, Linux (CI) memakai `/proc/meminfo`, dan bagian
//!     yang bisa diuji tanpa OS adalah **parser**-nya;
//!   * semua ketiadaan data harus terwakili `None`, bukan angka palsu 0 —
//!     "0 GB bebas" dan "tidak tahu" adalah dua hal berbeda bagi pengguna.

// Beberapa item di bawah hanya dipakai lewat FFI/UI Windows; pada build
// Linux (CI) jalur itu tidak dikompilasi. Pembatasan ini sengaja
// HANYA untuk non-Windows: kalau ada kode mati saat build Windows,
// warningnya tetap muncul di sana.
#![cfg_attr(not(windows), allow(dead_code))]

use std::path::Path;

/// Ringkasan mesin. Semua field boleh kosong: UI menampilkan "—".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SysInfo {
    /// `x64`, `ARM64`, … (nama yang dikenal pengguna Windows).
    pub arch: String,
    pub os: String,
    pub ram_used_mb: Option<u64>,
    pub ram_total_mb: Option<u64>,
    /// Ruang disk tempat workspace berada.
    pub disk_free_gb: Option<u64>,
    pub disk_total_gb: Option<u64>,
}

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * 1024 * 1024;

/// Nama arsitektur seperti yang muncul di Windows ("x64", "ARM64").
///
/// `std::env::consts::ARCH` memakai nama Rust (`x86_64`); pengguna lebih
/// mengenali nama Windows, dan tab Setelan juga menampilkannya di sebelah
/// hasil `$PSVersionTable` yang memakai ejaan Windows.
pub fn arch_name(arch: &str) -> String {
    match arch {
        "x86_64" => "x64".into(),
        "aarch64" => "ARM64".into(),
        "x86" | "i686" => "x86".into(),
        "arm" => "ARM".into(),
        other => other.to_string(),
    }
}

/// Ringkasan OS yang stabil untuk ditampilkan & dilaporkan saat bug.
pub fn os_name() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Persentase RAM terpakai, dibulatkan. `None` kalau totalnya tidak diketahui
/// (atau nol, yang akan membuat pembagian gagal).
pub fn ram_percent(used_mb: Option<u64>, total_mb: Option<u64>) -> Option<u32> {
    let (u, t) = (used_mb?, total_mb?);
    if t == 0 {
        return None;
    }
    Some(((u as f64 / t as f64) * 100.0).round().clamp(0.0, 100.0) as u32)
}

/// Ubah byte → MB/GB. Dibatasi `u64::MAX` supaya angka aneh dari API (mis.
/// disk yang melaporkan `u64::MAX` untuk "tidak diketahui") tidak membuat
/// penampilan meluap.
pub fn bytes_to_mb(bytes: u64) -> u64 {
    bytes.saturating_div(MB)
}

pub fn bytes_to_gb(bytes: u64) -> u64 {
    bytes.saturating_div(GB)
}

/// Baca `/proc/meminfo` (Linux/CI). Hanya dipakai jalur non-Windows.
///
/// Contoh isi:
/// `MemTotal:       16311234 kB` / `MemAvailable:    8123456 kB`
pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let mut total_kb = None;
    let mut avail_kb = None;
    for line in text.lines() {
        let (key, rest) = line.split_once(':')?;
        let value = rest.split_whitespace().next()?.parse::<u64>().ok()?;
        match key.trim() {
            "MemTotal" => total_kb = Some(value),
            // Kernel lama tidak punya MemAvailable; MemFree lebih kecil dari
            // RAM yang benar-benar bisa dipakai, tapi lebih baik daripada nol.
            "MemAvailable" => avail_kb = Some(value),
            "MemFree" if avail_kb.is_none() => avail_kb = Some(value),
            _ => {}
        }
    }
    let total = total_kb?;
    let avail = avail_kb?;
    Some((bytes_to_mb(total * 1024), bytes_to_mb(avail * 1024)))
}

/// Ambil statistik. Aman dipanggil dari thread mana pun; **tidak** dari `draw`.
pub fn collect(workspace: &Path) -> SysInfo {
    let mut info = SysInfo {
        arch: arch_name(std::env::consts::ARCH),
        os: os_name(),
        ..Default::default()
    };

    #[cfg(windows)]
    {
        if let Some((total, avail)) = win::memory() {
            let total_mb = bytes_to_mb(total);
            let avail_mb = bytes_to_mb(avail);
            info.ram_total_mb = Some(total_mb);
            info.ram_used_mb = Some(total_mb.saturating_sub(avail_mb));
        }
        if let Some((free, total)) = win::disk(workspace) {
            info.disk_free_gb = Some(bytes_to_gb(free));
            info.disk_total_gb = Some(bytes_to_gb(total));
        }
    }

    #[cfg(not(windows))]
    {
        // CI Linux: cukup RAM. Ruang disk tidak dipakai UI Windows, dan
        // menambah pembacaan `statvfs` hanya untuk memenuhi kolom yang tidak
        // ditampilkan bukan prioritas.
        if let Ok(text) = std::fs::read_to_string("/proc/meminfo") {
            if let Some((total, avail)) = parse_meminfo(&text) {
                info.ram_total_mb = Some(total);
                info.ram_used_mb = Some(total.saturating_sub(avail));
            }
        }
        let _ = workspace;
    }

    info
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    /// Total & tersedia (byte).
    pub fn memory() -> Option<(u64, u64)> {
        // SAFETY: `MEMORYSTATUSEX` adalah struct POD; `dwLength` wajib diisi
        // sebelum panggilan, dan API hanya menulis ke pointer yang kita beri.
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 {
            return None;
        }
        Some((m.ullTotalPhys, m.ullAvailPhys))
    }

    /// Ruang disk untuk **volume** tempat `dir` berada.
    pub fn disk(dir: &Path) -> Option<(u64, u64)> {
        // API ini menerima string dengan NUL ganda; path tanpa NUL membuat
        // panggilan gagal (`ERROR_INVALID_PARAMETER`).
        let mut w: Vec<u16> = dir.as_os_str().encode_wide().collect();
        w.push(0);
        w.push(0);
        let (mut free, mut total) = (0u64, 0u64);
        // SAFETY: ketiga pointer menunjuk ke variabel lokal yang hidup selama
        // panggilan; API hanya menulis ke sana.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                w.as_ptr(),
                &mut free as *mut u64,
                &mut total as *mut u64,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            // Folder belum dibuat atau path jaringan tidak terjangkau.
            return None;
        }
        Some((free, total))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_dipakai_nama_windows() {
        assert_eq!(arch_name("x86_64"), "x64");
        assert_eq!(arch_name("aarch64"), "ARM64");
        assert_eq!(arch_name("x86"), "x86");
        assert_eq!(
            arch_name("riscv64"),
            "riscv64",
            "tidak dikenal → apa adanya"
        );
    }

    #[test]
    fn meminfo_dibaca_dari_baris_kernel_sungguhan() {
        let text = "MemTotal:       16311234 kB\n\
                    MemFree:         1234567 kB\n\
                    MemAvailable:    8123456 kB\n\
                    Buffers:          123456 kB\n";
        let (total, avail) = parse_meminfo(text).expect("harus terbaca");
        assert_eq!(total, bytes_to_mb(16_311_234 * 1024));
        assert_eq!(avail, bytes_to_mb(8_123_456 * 1024));
    }

    #[test]
    fn meminfo_tanpa_available_memakai_memfree() {
        let text = "MemTotal: 1000 kB\nMemFree: 250 kB\n";
        let (total, avail) = parse_meminfo(text).expect("harus terbaca");
        assert_eq!(total, 0, "1000 kB < 1 MB");
        assert_eq!(avail, 0);
    }

    #[test]
    fn meminfo_rusak_menghasilkan_none_bukan_panic() {
        assert_eq!(parse_meminfo(""), None);
        assert_eq!(parse_meminfo("MemTotal: banyak kB"), None);
        assert_eq!(
            parse_meminfo("MemTotal: 100 kB"),
            None,
            "tanpa MemFree/Available"
        );
    }

    #[test]
    fn persen_ram_aman_dari_pembagian_nol() {
        assert_eq!(ram_percent(Some(50), Some(100)), Some(50));
        assert_eq!(ram_percent(Some(1), Some(0)), None);
        assert_eq!(ram_percent(None, Some(100)), None);
        assert_eq!(ram_percent(Some(200), Some(100)), Some(100), "dijepit");
    }

    #[test]
    fn konversi_byte_tidak_meluap() {
        // `saturating_div` membulatkan ke BAWAH: (2^64 - 1) / 2^30 = 2^34 - 1.
        // Yang diuji di sini adalah "tidak overflow / tidak panic", bukan
        // pembulatan ke atas (UI sengaja menampilkan GB penuh, bukan 1,5 GB).
        assert_eq!(bytes_to_gb(u64::MAX), 17_179_869_183);
        assert_eq!(bytes_to_mb(3 * 1024 * 1024), 3);
        assert_eq!(bytes_to_gb(4 * GB), 4);
    }

    #[test]
    fn collect_selalu_mengisi_arch_dan_os() {
        // Tidak ada asersi angka: RAM/disk berbeda tiap mesin. Yang penting
        // fungsi ini tidak panic dan arch-nya terisi.
        let i = collect(Path::new("."));
        assert!(!i.arch.is_empty());
        assert!(!i.os.is_empty());
    }
}
