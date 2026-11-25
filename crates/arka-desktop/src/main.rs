//! Titik masuk Arka Agent untuk Windows.
//!
//! Tanpa argumen membuka **UI** (egui). Perintah CLI di bawah tetap ada
//! sebagai alat bukti & *smoke test* CI: kalau `--status` / `--exec` benar,
//! berarti lapisan shell siap dipakai UI.
//!
//! ```
//! arka-agent                       buka jendela UI
//! arka-agent --status              ringkasan mesin (JSON)
//! arka-agent --diagnostic          tes cepat (versi, encoding, tools)
//! arka-agent --prepare             buat workspace + penanda
//! arka-agent --exec "<perintah>"   jalankan perintah PowerShell di workspace
//! arka-agent --print-script "<p>"  naskah + base64, tanpa eksekusi (dry run)
//! arka-agent --open "<file>"       pratinjau berkas di jendela yang sudah ada
//! arka-agent --selftest            bukti perilaku (UTF-8, exit code, Stop)
//! ```

mod desk_notify;
mod dpapi;
mod encode;
mod gui;
mod keepawake;
mod platform;
mod preview;
mod pump;
mod pwsh;
mod script;
mod selftest;
mod single;
mod sys;
mod sysinfo;
mod tray;

use arka_core::events::Events;
use arka_core::shell;
use arka_core::{paths, secret};
use std::sync::atomic::AtomicBool;

/// Hash commit saat build, untuk tab Tentang & laporan bug.
///
/// Diambil dari `ARKA_GIT_COMMIT` yang disuntikkan `build.rs` (hash `git
/// rev-parse`) atau CI. Build tanpa git menghasilkan string kosong, dan itu
/// ditampilkan sebagai "tidak diketahui" — bukan angka palsu.
pub fn build_commit() -> &'static str {
    match option_env!("ARKA_GIT_COMMIT") {
        Some(s) if !s.is_empty() => s,
        _ => "tidak diketahui",
    }
}

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
/// Daftarkan folder aplikasi. Tidak menyentuh PowerShell, jadi aman dipanggil
/// di OS apa pun — dipakai juga saat mengembangkan UI di Linux (Xvfb).
fn bootstrap_paths() -> Result<(), String> {
    // Idempoten: `main` sekarang memanggilnya lebih awal untuk menulis
    // `pending-open.txt`, sebelum `run_ui` mendaftarkan path yang sama.
    // `paths::init` memakai OnceLock, jadi panggilan kedua harus diterima.
    if paths::get().is_ok() {
        return Ok(());
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
    let data = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| home.join(".arka-agent"));
    paths::init(paths::AppPaths::windows(home, data))
}

/// Bootstrap lengkap untuk perintah CLI yang butuh shell.
fn bootstrap() -> Result<(), String> {
    if !cfg!(windows) {
        return Err("Perintah ini hanya berjalan di Windows".into());
    }
    bootstrap_paths()?;
    shell::init(Box::new(pwsh::PwshShell::new()?));
    // DPAPI di Windows, Noop hanya saat dikembangkan di Linux.
    secret::init(dpapi::default_store());
    Ok(())
}

/// Siapkan database: folder data, skema, dan provider bawaan.
///
/// Gagal di sini **tidak** menggagalkan start: jendela tetap terbuka dan
/// pesan errornya muncul di tab Chat (`AppState::db_error`). Yang tidak boleh
/// terjadi adalah aplikasi menutup diri tanpa penjelasan.
fn init_db(paths: &paths::AppPaths) -> Result<(), String> {
    std::fs::create_dir_all(&paths.data_dir).map_err(|e| e.to_string())?;
    let db = paths.data_dir.join("arka.db");
    arka_core::db::init(&db.to_string_lossy())?;
    arka_core::provider::seed_if_empty()?;
    Ok(())
}

/// Buka jendela UI.
///
/// Shell didaftarkan secara *best effort*: kalau PowerShell tidak ada (mis.
/// saat mengembangkan UI di Linux), jendela tetap terbuka dan tab Terminal
/// menampilkan "tidak ditemukan" — bukan aplikasi yang gagal start.
fn run_ui() -> i32 {
    if let Err(e) = bootstrap_paths() {
        eprintln!("{e}");
        return 2;
    }
    if cfg!(windows) {
        match pwsh::PwshShell::new() {
            Ok(s) => shell::init(Box::new(s)),
            Err(e) => eprintln!("peringatan: PowerShell tidak terdaftar: {e}"),
        }
        // Enkripsi API key: DPAPI mengikat ciphertext ke akun Windows ini,
        // jadi salinan arka.db di mesin lain tidak bisa dibuka.
        secret::init(dpapi::default_store());
    }

    let app_paths = match paths::get() {
        Ok(p) => p.clone(),
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };

    match init_db(&app_paths) {
        Ok(()) => {
            // Data dari versi yang masih menyimpan kunci apa adanya (atau dari
            // Android) dienkripsi sekali di sini. Di Android/Noop hasilnya 0 —
            // tidak ada yang ditulis ulang.
            match arka_core::provider::migrate_plaintext_keys() {
                Ok(0) => {}
                Ok(n) => eprintln!("{n} API key dienkripsi dengan DPAPI"),
                Err(e) => eprintln!("migrasi API key gagal: {e}"),
            }
        }
        // Tidak fatal: UI tetap terbuka dan menampilkan pesan ini.
        Err(e) => eprintln!("database tidak siap: {e}"),
    }

    let opts = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("Arka Agent"),
        // `persist_window` menyimpan ukuran/posisi jendela; setelan UI
        // disimpan sendiri lewat `eframe::Storage` (lihat `gui::ArkaGui`).
        persist_window: true,
        ..Default::default()
    };

    match eframe::run_native(
        "Arka Agent",
        opts,
        Box::new(move |cc| {
            let mut gui = gui::ArkaGui::new(cc, app_paths);
            // Sambungkan ke database & buka sesi terakhir. Dipanggil di sini,
            // bukan di `draw`, supaya test UI tetap jalan tanpa database.
            gui.bootstrap_data();
            Ok(Box::new(gui))
        }),
    ) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("UI gagal dibuka: {e}");
            1
        }
    }
}

fn print_help() {
    println!(
        "{}",
        [
            "Arka Agent (Windows) — 100% Rust, shell native Windows",
            "",
            "Pemakaian: arka-agent <perintah> [argumen]",
            "",
            "  (tanpa argumen)      buka jendela UI",
            "  --status             ringkasan mesin sebagai JSON",
            "  --diagnostic         tes cepat PowerShell & tools",
            "  --prepare            siapkan folder workspace",
            "  --exec \"<perintah>\"   jalankan perintah di workspace",
            "  --print-script \"<p>\" tampilkan naskah PowerShell + base64 (tanpa eksekusi)",
            "  --open \"<file>\"       pratinjau berkas (diteruskan ke jendela yang sudah berjalan)",
            "  --selftest           uji perilaku: UTF-8, exit code, Stop mematikan pohon proses",
        ]
        .join("\n")
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
    let label = args.first().map(String::as_str).unwrap_or("");
    match label {
        "" | "--ui" => run_ui(),
        "--help" | "-h" => {
            print_help();
            0
        }
        "--print-script" => print_script(args.get(1)),
        // Sudah dipisah `single::parse_args`; sampai ke sini berarti
        // `--open` tanpa nilai (atau nilainya ikut jadi flag lain).
        "--open" => {
            eprintln!("--open butuh satu argumen path berkas");
            2
        }
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

/// Titipkan permintaan pratinjau ke folder data instance pertama.
///
/// `bootstrap_paths` aman dipanggil dua kali (lihat komentar di sana), jadi
/// baik instance kedua maupun primary bisa memakai jalur yang sama.
fn titipkan_berkas(path: &std::path::Path) -> Result<(), String> {
    bootstrap_paths()?;
    let dir = paths::data_dir()?;
    single::write_pending(&dir, path)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let launch = single::parse_args(&args);

    // Mode CLI = masih ada argumen selain `--open`. Smoke test CI (--status,
    // --exec, --selftest) sengaja tidak terikat single-instance supaya bisa
    // berjalan berdampingan dengan UI tanpa membuka jendela kedua.
    if !launch.rest.is_empty() {
        if launch.open.is_some() {
            eprintln!("peringatan: --open hanya berlaku saat membuka jendela UI");
        }
        std::process::exit(run(&launch.rest));
    }

    // Satu instance: proses kedua tidak boleh membuka jendela lagi.
    if matches!(single::claim(), single::Instance::Secondary) {
        if let Some(path) = &launch.open {
            if let Err(e) = titipkan_berkas(path) {
                eprintln!("tidak bisa meneruskan berkas: {e}");
            }
        }
        println!("Arka Agent sudah berjalan.");
        std::process::exit(0);
    }

    // Toast WinRT butuh AUMID terdaftar SEBELUM notifikasi pertama. Tanpa ini
    // Windows menamai toast itu "Windows PowerShell", bukan "Arka Agent".
    if let desk_notify::Registered::Failed(e) = desk_notify::register_app_id() {
        eprintln!("peringatan: AppUserModelID gagal didaftarkan: {e}");
    }

    // `--open` pada instance pertama memakai jalur yang sama dengan instance
    // kedua, supaya pratinjau tetap melewati guard path workspace yang ada di
    // `ArkaGui::poll_pending` (bukan langsung memuat path sembarangan).
    if let Some(path) = &launch.open {
        if let Err(e) = titipkan_berkas(path) {
            eprintln!("peringatan: --open tidak bisa disiapkan: {e}");
        }
    }

    std::process::exit(run(&launch.rest));
}
