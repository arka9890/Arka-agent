//! `Shell` untuk Windows: PowerShell 7 (`pwsh.exe`) atau 5.1 (`powershell.exe`).
//!
//! Modul ini sengaja **tidak** memakai API Windows langsung (selain via
//! [`crate::sys`]) supaya seluruh alurnya ikut di-`cargo check` di Linux.
//!
//! ## Keputusan yang menentukan perilaku
//! * **`-EncodedCommand`, bukan `-Command`** — bebas masalah quoting, lihat
//!   [`crate::encode`].
//! * **Penanda exit** — exit code proses PowerShell tidak meneruskan exit code
//!   program native, lihat [`crate::script`].
//! * **`-NoProfile`** — profil pengguna bisa mencetak banner, mengubah
//!   `$ErrorActionPreference`, atau memuat modul yang memperlambat/mengubah
//!   output. Untuk agent, semua itu harus mati.
//! * **`-NonInteractive`** — perintah yang diam-diam menunggu input akan gagal
//!   cepat, bukan menggantung sampai timeout.
//! * **`stdin = null`** — sama seperti di atas; `apt`/`nano` versi Windows
//!   tidak boleh mengunci agent.
//! * **Satu perintah sekaligus** — dijaga `EXEC_LOCK`; dua perintah paralel di
//!   satu PowerShell besar kemungkinan saling menimpa file kerja.

use crate::encode;
use crate::pump::{self, Limits};
use crate::script;
use crate::sys;
use arka_core::events::Events;
use arka_core::shell::{ExecOutcome, Shell};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::Duration;

/// Satu eksekusi pada satu waktu (pola sama dengan `TOOL_LOCK` di Android).
static EXEC_LOCK: Mutex<()> = Mutex::new(());

/// Batas waktu bawaan untuk perintah lewat `run_command`.
/// `None` = tanpa batas; agent tetap bisa dihentikan lewat tombol Stop.
pub const DEFAULT_TIMEOUT: Option<Duration> = None;

/// Batas karakter output yang disimpan per eksekusi. Agent memotong lagi ke
/// `tool_output_chars` (bawaan 4.000) sebelum dikirim ke model; angka ini
/// hanya pagar memori supaya `dir /s` tidak menghabiskan RAM.
pub const RAW_OUTPUT_CAP: usize = 200_000;

/// Argumen `pwsh.exe`. Fungsi murni supaya bisa diuji dari Linux.
pub fn args(encoded: &str) -> Vec<String> {
    vec![
        "-NoProfile".to_string(),
        "-NonInteractive".to_string(),
        "-EncodedCommand".to_string(),
        encoded.to_string(),
    ]
}

/// Hasil deteksi PowerShell.
#[derive(Clone, Debug)]
pub struct Detected {
    pub program: PathBuf,
    pub version: String,
    pub edition: String,
    pub is_pwsh7: bool,
}

impl Detected {
    pub fn describe(&self) -> String {
        let kind = if self.is_pwsh7 { "PowerShell 7" } else { "Windows PowerShell 5.1" };
        if self.version.is_empty() {
            format!("{kind} ({})", self.program.display())
        } else {
            format!("{kind} {} ({})", self.version, self.program.display())
        }
    }
}

/// Shell PowerShell aktif.
pub struct PwshShell {
    detected: Detected,
    workspace: PathBuf,
    limits: Limits,
}

impl PwshShell {
    /// Deteksi PowerShell lalu pakai workspace dari [`arka_core::paths`].
    pub fn new() -> Result<Self, String> {
        let workspace = arka_core::paths::workspace()?;
        Ok(Self {
            detected: detect()?,
            workspace,
            limits: Limits { max_chars: RAW_OUTPUT_CAP, timeout: DEFAULT_TIMEOUT },
        })
    }

    /// Varian eksplisit untuk tes/CLI (`--exec` dengan workspace lain).
    pub fn with(program: PathBuf, workspace: PathBuf, limits: Limits) -> Self {
        Self {
            detected: Detected {
                program,
                version: String::new(),
                edition: String::new(),
                is_pwsh7: false,
            },
            workspace,
            limits,
        }
    }

    fn run(
        &self,
        script_text: &str,
        cancel: &AtomicBool,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<(String, Option<i32>, pump::Collected, bool), String> {
        let encoded = encode::encode(script_text);
        let mut proc: sys::Proc = sys::spawn(&self.detected.program, &args(&encoded), &self.workspace)?;
        let attached = proc.attached;
        let pipes = proc.take_pipes();
        let collected = pump::collect(pipes, cancel, self.limits, on_chunk, &mut || proc.kill());
        let status = proc.wait()?;
        Ok((collected.output.clone(), status.code(), collected, attached))
    }

    /// Jalankan naskah kecil dan kembalikan stdout mentah (tanpa penanda).
    fn capture(&self, script_text: &str) -> Result<String, String> {
        let full = format!("{}\n{}", script::preamble(), script_text);
        let never = AtomicBool::new(false);
        let (out, _, _, _) = self.run(&full, &never, &mut |_| {})?;
        let (_, clean) = script::parse_output(&out);
        Ok(clean)
    }

    /// Versi/OS/arsitektur/ExecutionPolicy dalam satu proses.
    ///
    /// Dipublikkan untuk tab Terminal (kartu status shell). Setiap panggilan
    /// **menjalankan satu proses PowerShell**, jadi pemanggil wajib menyimpan
    /// hasilnya — jangan memanggilnya dari `draw`.
    pub fn info(&self) -> Result<Vec<(String, String)>, String> {
        let raw = self.capture(
            r"'PSVersion=' + $PSVersionTable.PSVersion.ToString()
'PSEdition=' + $PSVersionTable.PSEdition
'OS=' + [System.Environment]::OSVersion.VersionString
'Arch=' + [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
'Policy=' + (Get-ExecutionPolicy -Scope CurrentUser).ToString()
'Home=' + $HOME"
            .trim_end()
            .trim_start(),
        )?;
        Ok(raw
            .lines()
            .filter_map(|l| {
                let l = l.trim();
                let (k, v) = l.split_once('=')?;
                Some((k.trim().to_string(), v.trim().to_string()))
            })
            .collect())
    }
}

/// Cari `pwsh.exe` (7+) lalu `powershell.exe` (5.1).
///
/// Cara paling andal: **coba jalankan**. Mencari di `PATH` sendiri sering
/// keliru (Microsoft Store alias, `SysWow64`, `dotnet global tool`), dan
/// kegagalan spawn sudah memberi tahu kita semuanya.
pub fn detect() -> Result<Detected, String> {
    let mut last: Option<String> = None;
    for (name, seven) in [("pwsh.exe", true), ("powershell.exe", false)] {
        let program = PathBuf::from(name);
        let probe = PwshShell::with(program.clone(), std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")), Limits::default());
        match probe.capture("$PSVersionTable.PSVersion.ToString(); $PSVersionTable.PSEdition") {
            Ok(out) => {
                let mut lines = out.lines().map(str::trim).filter(|l| !l.is_empty());
                let version = lines.next().unwrap_or("").to_string();
                let edition = lines.next().unwrap_or("").to_string();
                return Ok(Detected { program, version, edition, is_pwsh7: seven });
            }
            Err(e) => last = Some(e),
        }
    }
    Err(format!(
        "PowerShell tidak ditemukan. Pasang PowerShell 7 (winget install Microsoft.PowerShell) \
         atau jalankan versi Windows yang menyertakan powershell.exe. Detail: {}",
        last.unwrap_or_else(|| "tidak diketahui".into())
    ))
}

/// Total byte seluruh berkas di [path] (rekursif, tanpa mengikuti symlink).
pub fn dir_size(path: &Path) -> i64 {
    let mut total = 0i64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else { continue };
            if meta.is_dir() {
                stack.push(e.path());
            } else if meta.is_file() {
                total += meta.len() as i64;
            }
        }
    }
    total
}

/// Jumlah berkas di [path] (rekursif).
pub fn dir_files(path: &Path) -> usize {
    let mut n = 0usize;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(_) => n += 1,
                Err(_) => {}
            }
        }
    }
    n
}

impl Shell for PwshShell {
    fn status(&self) -> Result<Value, String> {
        let info = self.info().unwrap_or_default();
        let pick = |key: &str| {
            info.iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        };
        let size = dir_size(&self.workspace);
        Ok(json!({
            "shell": "powershell",
            "program": self.detected.program.to_string_lossy(),
            "version": if self.detected.version.is_empty() { pick("PSVersion") } else { self.detected.version.clone() },
            "edition": if self.detected.edition.is_empty() { pick("PSEdition") } else { self.detected.edition.clone() },
            "is_pwsh7": self.detected.is_pwsh7,
            "os": pick("OS"),
            "arch": pick("Arch"),
            "execution_policy": pick("Policy"),
            "workspace": self.workspace.to_string_lossy(),
            "workspace_exists": self.workspace.is_dir(),
            "workspace_files": dir_files(&self.workspace),
            "workspace_size_bytes": size,
            "max_chars": self.limits.max_chars,
            "timeout_secs": self.limits.timeout.map(|d| d.as_secs()),
            "message": self.detected.describe(),
        }))
    }

    fn prepare(&self, ev: &dyn Events, cancel: &AtomicBool) -> Result<(), String> {
        ev.progress(5, "Menyiapkan workspace…");
        std::fs::create_dir_all(&self.workspace)
            .map_err(|e| format!("gagal membuat workspace {}: {e}", self.workspace.display()))?;
        ev.progress(40, "Menulis penanda workspace…");
        let marker = self.workspace.join(".arka");
        std::fs::create_dir_all(&marker).map_err(|e| e.to_string())?;
        let readme = marker.join("README.txt");
        if !readme.is_file() {
            let text = format!(
                "Folder ini adalah workspace Arka Agent.\r\n\
                 Agent menjalankan perintah PowerShell di sini, jadi semua file yang dibuatnya ada di sini.\r\n\
                 Workspace: {}\r\n",
                self.workspace.display()
            );
            std::fs::write(&readme, text).map_err(|e| e.to_string())?;
        }
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Err("Dibatalkan".into());
        }
        ev.progress(70, "Memeriksa tools…");
        let found = self
            .capture(
                "foreach ($t in 'git','python','node','pwsh') { \
                 $c = Get-Command $t -ErrorAction SilentlyContinue; \
                 if ($c) { \"$t=$($c.Source)\" } else { \"$t=\" } }",
            )
            .unwrap_or_default();
        let accessible: Vec<String> = found
            .lines()
            .filter_map(|l| {
                let (k, v) = l.trim().split_once('=')?;
                if v.trim().is_empty() { None } else { Some(k.to_string()) }
            })
            .collect();
        ev.progress(
            100,
            &format!(
                "Siap. {} tools terdeteksi{}.",
                accessible.len(),
                if accessible.is_empty() { String::new() } else { format!(": {}", accessible.join(", ")) }
            ),
        );
        Ok(())
    }

    fn exec(
        &self,
        command: &str,
        cancel: &AtomicBool,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<ExecOutcome, String> {
        let command = command.trim();
        if command.is_empty() {
            return Err("Perintah kosong".into());
        }
        if command.contains('\0') {
            return Err("Perintah memuat byte NUL".into());
        }
        let _guard = EXEC_LOCK.lock().map_err(|e| format!("Lock eksekusi rusak: {e}"))?;
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(ExecOutcome { code: -1, canceled: true, ..Default::default() });
        }

        let full = script::build(command);
        let (raw, process_code, collected, _attached) = self.run(&full, cancel, on_chunk)?;
        let (sentinel_code, clean) = script::parse_output(&raw);

        // Urutan kepercayaan: batal > penanda > exit code proses.
        let (code, canceled, timed_out) = if collected.canceled {
            (-1, true, false)
        } else if collected.timed_out {
            (-1, false, true)
        } else if let Some(c) = sentinel_code {
            (c, false, false)
        } else {
            (script::normalize_code(process_code), false, false)
        };

        Ok(ExecOutcome { code, output: clean, timed_out, canceled })
    }

    fn diagnostic(&self) -> Result<String, String> {
        self.capture(
            r#"$out = @()
$out += 'program=' + [System.Diagnostics.Process]::GetCurrentProcess().Path
$out += 'ps=' + $PSVersionTable.PSVersion.ToString() + ' edition=' + $PSVersionTable.PSEdition
$out += 'os=' + [System.Environment]::OSVersion.VersionString
$out += 'arch=' + [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
$out += 'workspace=' + (Get-Location).Path
$out += 'encoding=' + [Console]::OutputEncoding.WebName
$out += 'utf8_probe=' + [char]0x1F680 + ' 日本語'
foreach ($t in 'git','python','node','pwsh','rg') {
  $c = Get-Command $t -ErrorAction SilentlyContinue
  if ($c) { $out += "tool:$t=" + $c.Source } else { $out += "tool:$t=tidak ada" }
}
$out -join [Environment]::NewLine"#,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_contain_exactly_one_encoded_command_and_no_command_flag() {
        let a = args("QUJD");
        assert_eq!(a.iter().filter(|s| s.as_str() == "-EncodedCommand").count(), 1);
        assert!(!a.iter().any(|s| s == "-Command" || s == "-c"));
        assert_eq!(a[0], "-NoProfile");
        assert_eq!(a[1], "-NonInteractive");
        assert_eq!(a.last().unwrap(), "QUJD");
    }

    #[test]
    fn args_avoid_ephemeral_flags_that_break_output() {
        // `-NoLogo` tidak cukup: tema/warna bisa menulis escape sequence yang
        // membuat output sulit dibaca model → warna dimatikan lewat naskah.
        let a = args("x");
        assert!(!a.iter().any(|s| s == "-OutputFormat"));
        assert!(script::build("ls").contains("$ErrorActionPreference"));
    }

    #[test]
    fn describe_mentions_which_powershell() {
        let d = Detected {
            program: PathBuf::from("pwsh.exe"),
            version: "7.4.6".into(),
            edition: "Core".into(),
            is_pwsh7: true,
        };
        assert!(d.describe().contains("PowerShell 7"));
        assert!(d.describe().contains("7.4.6"));
    }

    #[test]
    fn exec_rejects_empty_and_nul_before_spawning() {
        let shell = PwshShell::with(PathBuf::from("pwsh.exe"), PathBuf::from("."), Limits::default());
        let cancel = AtomicBool::new(false);
        assert_eq!(shell.exec("   ", &cancel, &mut |_| {}).unwrap_err(), "Perintah kosong");
        assert!(shell.exec("echo a\0b", &cancel, &mut |_| {}).is_err());
    }

    #[test]
    fn exec_returns_early_when_already_canceled() {
        let shell = PwshShell::with(PathBuf::from("pwsh.exe"), PathBuf::from("."), Limits::default());
        let cancel = AtomicBool::new(true);
        let out = shell.exec("echo hi", &cancel, &mut |_| {}).unwrap();
        assert!(out.canceled);
        assert_eq!(out.code, -1);
    }

    #[test]
    fn workspace_helpers_tolerate_missing_path() {
        assert_eq!(dir_size(Path::new("/tidak/ada/path/ini")), 0);
        assert_eq!(dir_files(Path::new("/tidak/ada/path/ini")), 0);
    }

    #[cfg(unix)]
    #[test]
    fn dir_size_and_files_count_real_dir() {
        let tmp = std::env::temp_dir().join(format!("arka-{}", std::process::id()));
        std::fs::create_dir_all(tmp.join("sub")).unwrap();
        std::fs::write(tmp.join("a.txt"), b"12345").unwrap();
        std::fs::write(tmp.join("sub/b.txt"), b"67").unwrap();
        assert_eq!(dir_size(&tmp), 7);
        assert_eq!(dir_files(&tmp), 2);
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
