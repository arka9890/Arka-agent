//! `--selftest`: bukti perilaku yang tidak bisa diuji dari unit test.
//!
//! Unit test berjalan di Linux dan hanya menguji logika murni (`script`,
//! `encode`, `pump`). Tiga hal berikut baru terbukti di mesin Windows:
//!  1. PowerShell benar-benar ditemukan dan bisa menjalankan perintah;
//!  2. output UTF-8 (emoji + Jepang) tidak rusak melewati pipe;
//!  3. tombol Stop **mematikan seluruh pohon proses**, bukan hanya induknya.
//!
//! CI `windows-latest` menjalankan ini, jadi klaim "M2 selesai" punya bukti
//! mesin, bukan hanya asersi di kepala pengembang.

use arka_core::shell::Shell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Hasil satu pemeriksaan.
struct Check {
    name: &'static str,
    passed: bool,
    detail: String,
}

fn exec_plain(sh: &dyn Shell, cmd: &str) -> Result<(i32, String), String> {
    let never = AtomicBool::new(false);
    let out = sh.exec(cmd, &never, &mut |_| {})?;
    Ok((out.code, out.output))
}

pub fn run(sh: &dyn Shell) -> i32 {
    let mut checks: Vec<Check> = Vec::new();

    // 1. Mesin hidup dan perintah dasar jalan.
    match exec_plain(sh, "Write-Output 'halo dari PowerShell'") {
        Ok((code, out)) => checks.push(Check {
            name: "perintah dasar",
            passed: code == 0 && out.contains("halo dari PowerShell"),
            detail: format!("exit {code}: {}", out.trim()),
        }),
        Err(e) => checks.push(Check {
            name: "perintah dasar",
            passed: false,
            detail: e,
        }),
    }

    // 2. UTF-8 utuh (emoji 4 byte + karakter non-Latin).
    match exec_plain(sh, "Write-Output '🚀 日本語 — é ñ'") {
        Ok((code, out)) => {
            let ok = code == 0 && out.contains('🚀') && out.contains("日本語") && out.contains('ñ');
            checks.push(Check {
                name: "UTF-8 utuh",
                passed: ok,
                detail: format!("exit {code}: {}", out.trim()),
            });
        }
        Err(e) => checks.push(Check {
            name: "UTF-8 utuh",
            passed: false,
            detail: e,
        }),
    }

    // 3. Exit code program native tidak hilang (kriteria `git` gagal 128).
    match exec_plain(sh, "cmd /c exit 42") {
        Ok((code, out)) => checks.push(Check {
            name: "exit code native diteruskan",
            passed: code == 42,
            detail: format!("exit {code}: {}", out.trim()),
        }),
        Err(e) => checks.push(Check {
            name: "exit code native diteruskan",
            passed: false,
            detail: e,
        }),
    }

    // 4. Stop mematikan pohon proses (anak + cucu).
    checks.push(cancel_kills_tree(sh));

    let mut failed = 0;
    for c in &checks {
        if c.passed {
            println!("PASS  {:<32} {}", c.name, c.detail);
        } else {
            failed += 1;
            println!("FAIL  {:<32} {}", c.name, c.detail);
        }
    }
    println!("\n{} pemeriksaan, {failed} gagal", checks.len());
    if failed == 0 {
        0
    } else {
        1
    }
}

/// Jalankan perintah yang membuat proses cucu, tekan Stop, lalu pastikan
/// proses cucu itu **hilang**.
fn cancel_kills_tree(sh: &dyn Shell) -> Check {
    // `Start-Process` menghasilkan proses baru yang bukan anak langsung dari
    // shell pemanggil dalam artian pewarisan handle; kalau job object salah,
    // proses ini akan bertahan setelah induk dibunuh.
    let cmd = r#"$p = Start-Process -FilePath 'powershell.exe' -ArgumentList '-NoProfile','-Command','Start-Sleep 120' -PassThru -WindowStyle Hidden
Write-Output "CUCU=$($p.Id)"
Start-Sleep 120"#;

    let cancel = Arc::new(AtomicBool::new(false));
    let flag = cancel.clone();
    let stopper = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1200));
        flag.store(true, Ordering::Relaxed);
    });

    let started = Instant::now();
    let outcome = sh.exec(cmd, &cancel, &mut |_| {});
    let elapsed = started.elapsed();
    let _ = stopper.join();

    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            return Check {
                name: "Stop mematikan pohon proses",
                passed: false,
                detail: e,
            }
        }
    };
    if !outcome.canceled {
        return Check {
            name: "Stop mematikan pohon proses",
            passed: false,
            detail: format!("tidak ditandai batal; exit {}", outcome.code),
        };
    }
    if elapsed > Duration::from_secs(15) {
        return Check {
            name: "Stop mematikan pohon proses",
            passed: false,
            detail: format!("terlalu lama: {elapsed:?}"),
        };
    }
    let child_pid = outcome.output.lines().find_map(|l| {
        l.trim()
            .strip_prefix("CUCU=")
            .and_then(|p| p.trim().parse::<u32>().ok())
    });
    let Some(pid) = child_pid else {
        return Check {
            name: "Stop mematikan pohon proses",
            passed: false,
            detail: format!(
                "pid cucu tidak terbaca dari output: {}",
                outcome.output.trim()
            ),
        };
    };

    // Beri Windows waktu singkat untuk membersihkan tabel proses.
    std::thread::sleep(Duration::from_millis(500));
    match exec_plain(sh, &format!("if (Get-Process -Id {pid} -ErrorAction SilentlyContinue) {{ 'MASIH HIDUP' }} else {{ 'SUDAH MATI' }}")) {
        Ok((_, out)) if out.contains("SUDAH MATI") => Check {
            name: "Stop mematikan pohon proses",
            passed: true,
            detail: format!("pid {pid} hilang, {elapsed:?}"),
        },
        Ok((_, out)) => Check {
            name: "Stop mematikan pohon proses",
            passed: false,
            detail: format!("pid {pid} masih hidup: {}", out.trim()),
        },
        Err(e) => Check { name: "Stop mematikan pohon proses", passed: false, detail: e },
    }
}
