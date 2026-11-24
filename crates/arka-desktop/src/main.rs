//! Titik masuk Arka Agent untuk Windows.
//!
//! UI egui menyusul di M3; sampai itu binary ini dipakai sebagai **alat bukti**:
//! kalau `--status` / `--exec` benar di Windows, berarti lapisan shell (M2)
//! siap dipakai UI. CI `windows-latest` menjalankan perintah yang sama sebagai
//! *smoke test*, jadi klaim "sudah jalan" tidak berhenti di unit test.
//!
//! ```
//! arka-agent --status              ringkasan mesin (JSON)
//! arka-agent --diagnostic          tes cepat (versi, encoding, tools)
//! arka-agent --prepare             buat workspace + penanda
//! arka-agent --exec "<perintah>"   jalankan perintah PowerShell di workspace
//! arka-agent --print-script "<p>"  naskah + base64, tanpa eksekusi (dry run)
//! arka-agent --selftest            bukti perilaku (UTF-8, exit code, Stop)
//! ```

mod encode;
mod pump;
mod pwsh;
mod script;
mod selftest;
mod sys;

use arka_core::events::Events;
use arka_core::shell;
use arka_core::{paths, secret};
use std::sync::atomic::AtomicBool;

/// Events untuk CLI: progres & output ditulis apa adanya ke terminal.
struct CliEvents;

impl Events for CliEvents {
    fn delta(&self, t: &str) {
        print!("{t}");
    }
    fn done(&self, _: &str) {}
    fn error(&self, m: &str) {
        eprintln!("error: {m}");
    }
    fn tool(&self, label: &str, display: &str) {
        println!("\n[{label}]\n{display}");
    }
    fn approval(&self, _: &str, _: &str, _: &str, _: &str) {}
    fn question(&self, _: &str, _: &str, _: &[String], _: &[serde_json::Value]) {}
    fn progress(&self, percent: i32, label: &str) {
        println!("[{percent:>3}%] {label}");
    }
    fn output(&self, text: &str) {
        print!("{text}");
    }
}

/// Siapkan path, shell, dan secret store — urutan ini wajib sebelum apa pun.
///
/// `AppPaths` harus lebih dulu karena `PwshShell::new` membaca workspace.
fn bootstrap() -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Arka Agent versi desktop hanya berjalan di Windows".into());
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let data = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home.join(".arka-agent"));
    paths::init(paths::AppPaths::windows(home, data))?;
    shell::init(Box::new(pwsh::PwshShell::new()?));
    // Enkripsi API key (DPAPI) menyusul di M5; sementara ini nilai disimpan
    // apa adanya supaya `provider::save` tetap jalan.
    secret::init(Box::new(secret::NoopSecretStore));
    Ok(())
}

fn print_help() {
    println!(
        "Arka Agent (Windows) — lapisan shell\n\
         \n\
         Pemakaian: arka-agent <perintah> [argumen]\n\
         \n\
         --status              ringkasan mesin sebagai JSON\n\
         --diagnostic          tes cepat PowerShell & tools\n\
         --prepare             siapkan folder workspace\n\
         --exec \"<perintah>\"    jalankan perintah di workspace\n\
         --print-script \"<p>\"  tampilkan naskah PowerShell + base64 (tanpa eksekusi)\n\
         --selftest            uji perilaku: UTF-8, exit code, Stop mematikan pohon proses"
    );
}

fn print_script(command: Option<&String>) -> i32 {
    let Some(command) = command else {
        eprintln!("--print-script butuh satu argumen perintah");
        return 2;
    };
    let full = script::build(command);
    let encoded = encode::encode(&full);
    println!("--- naskah PowerShell ---\n{full}");
    println!("--- base64 ({} karakter) ---", encoded.len());
    match encode::decode(&encoded) {
        Ok(back) if back == full => println!("round-trip base64: OK"),
        Ok(_) => {
            eprintln!("round-trip base64: BERBEDA");
            return 1;
        }
        Err(e) => {
            eprintln!("round-trip base64 gagal: {e}");
            return 1;
        }
    }
    0
}

fn run(args: &[String]) -> i32 {
    let label = args.first().map(String::as_str).unwrap_or("--help");
    match label {
        "--help" | "-h" | "" => {
            print_help();
            0
        }
        "--print-script" => print_script(args.get(1)),
        "--status" | "--diagnostic" | "--prepare" | "--exec" | "--selftest" => {
            if let Err(e) = bootstrap() {
                eprintln!("{e}");
                return 2;
            }
            let sh = match shell::active() {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("{e}");
                    return 2;
                }
            };
            match label {
                "--status" => match sh.status() {
                    Ok(v) => {
                        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
                        0
                    }
                    Err(e) => {
                        eprintln!("status gagal: {e}");
                        1
                    }
                },
                "--diagnostic" => match sh.diagnostic() {
                    Ok(t) => {
                        println!("{t}");
                        0
                    }
                    Err(e) => {
                        eprintln!("diagnostic gagal: {e}");
                        1
                    }
                },
                "--selftest" => selftest::run(sh),
                "--prepare" => {
                    let cancel = AtomicBool::new(false);
                    match sh.prepare(&CliEvents, &cancel) {
                        Ok(()) => 0,
                        Err(e) => {
                            eprintln!("prepare gagal: {e}");
                            1
                        }
                    }
                }
                _ => {
                    let Some(cmd) = args.get(1) else {
                        eprintln!("--exec butuh satu argumen perintah");
                        return 2;
                    };
                    let cancel = AtomicBool::new(false);
                    match sh.exec(cmd, &cancel, &mut |chunk| print!("{chunk}")) {
                        Ok(outcome) => {
                            println!("\n{}", outcome.report());
                            // Exit code diteruskan supaya CI bisa memeriksa hasil.
                            if outcome.code == 0 {
                                0
                            } else {
                                1
                            }
                        }
                        Err(e) => {
                            eprintln!("exec gagal: {e}");
                            1
                        }
                    }
                }
            }
        }
        other => {
            eprintln!("perintah tidak dikenal: {other}\n");
            print_help();
            2
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = run(&args);
    std::process::exit(code);
}
