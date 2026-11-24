//! Menyusun naskah PowerShell: preamble → perintah pengguna → penanda exit.
//!
//! Murni (tanpa API Windows) supaya bisa diuji dari Linux/CI.
//!
//! ## Kenapa ada penanda `__ARKA_EXIT__`
//! PowerShell tidak meneruskan exit code program native ke exit code proses
//! saat memakai `-EncodedCommand`/`-Command`: proses hanya keluar `0` atau `1`
//! berdasarkan `$?`. Akibatnya `git` yang gagal dengan 128 akan terlihat
//! "berhasil" (1) — agent jadi salah menyimpulkan.
//!
//! Jadi preamble mengatur `$LASTEXITCODE = 0`, lalu setelah perintah pengguna
//! kita tulis satu baris penanda berisi nilai terakhirnya. `pump` mengambil
//! angka itu dan membuang barisnya dari output.
//!
//! Kalau perintah pengguna sendiri memanggil `exit N` atau sintaksnya tidak
//! valid, baris penanda tidak akan muncul; saat itu yang dipakai adalah exit
//! code proses apa adanya.

/// Awalan baris penanda. Jangan diubah tanpa memperbarui [parse_output].
pub const SENTINEL: &str = "__ARKA_EXIT__";

/// Bagian tetap di awal setiap naskah.
///
/// * `$ErrorActionPreference='Continue'` — kesalahan tidak menghentikan naskah
///   (perilaku mirip `bash` tanpa `set -e`), jadi agent melihat semua pesan.
/// * `$ProgressPreference='SilentlyContinue'` — bar progres `Write-Progress`
///   tidak bisa dirender di output yang di-redirect dan hanya jadi sampah.
/// * Encoding UTF-8 di tiga tempat: `[Console]::OutputEncoding` (proses anak
///   ditulis ke stdout), `$OutputEncoding` (pipe ke program native), dan
///   `PYTHONIOENCODING` (python memilih encoding sendiri).
/// * Semua penetapan dibungkus `try/catch` karena pada PowerShell 5.1 dengan
///   stdout ter-redirect sebagian properti ini bisa melempar error.
pub fn preamble() -> &'static str {
    r#"$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$env:PYTHONIOENCODING = 'utf-8'
try { [Console]::OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
try { $OutputEncoding = [System.Text.Encoding]::UTF8 } catch {}
$global:LASTEXITCODE = 0"#
}

/// Baris penanda yang ditambahkan setelah perintah pengguna.
fn sentinel_line() -> String {
    // `[string]` dipakai supaya `$null` tidak memicu error method call.
    format!("[Console]::Out.WriteLine('{SENTINEL}' + [string]$global:LASTEXITCODE)")
}

/// Naskah lengkap yang akan dikodekan.
pub fn build(command: &str) -> String {
    format!("{}\n{}\n{}", preamble(), command, sentinel_line())
}

/// Hasil pemisahan output: (exit code dari penanda, output bersih).
///
/// Normalisasi yang disengaja: CRLF → LF dan newline di ujung dibuang, supaya
/// teks yang dikirim ke model sama bentuknya dengan keluaran Linux.
pub fn parse_output(raw: &str) -> (Option<i32>, String) {
    let mut code = None;
    let mut kept: Vec<&str> = Vec::new();
    let mut parts: Vec<&str> = raw.split('\n').collect();
    // Artefak `split`: elemen kosong terakhir bila teks diakhiri newline.
    if parts.last().map(|s| s.is_empty()).unwrap_or(false) {
        parts.pop();
    }
    for line in parts {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rest) = line.trim_start().strip_prefix(SENTINEL) {
            // Baris penanda selalu dibuang, walau angkanya tidak bisa diparse
            // (`$LASTEXITCODE` kosong) — jangan sampai bocor ke output model.
            code = rest.trim().parse::<i32>().ok().or(code);
            continue;
        }
        kept.push(line);
    }
    (code, kept.join("\n"))
}

/// Normalisasi exit code proses → int. Window tidak punya sinyal, jadi
/// nilai negatif hanya bisa datang dari kita sendiri (cancel = -1, mirip
/// implementasi Android di `run_capture`).
pub fn normalize_code(process_code: Option<i32>) -> i32 {
    match process_code {
        Some(c) => c,
        // `ExitProcess` tanpa argumen → 0; None di Rust berarti proses
        // dimatikan oleh sistem (mis. crash), jadi jangan dilaporkan sukses.
        None => -2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_keeps_command_verbatim_between_preamble_and_sentinel() {
        let cmd = "Get-ChildItem -Force | Select-Object -First 5";
        let s = build(cmd);
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.last().unwrap(), &sentinel_line());
        assert!(s.contains(cmd));
        assert!(s.starts_with("$ErrorActionPreference = 'Continue'"));
    }

    #[test]
    fn build_puts_command_on_its_own_line() {
        // Kalau perintah ditempel pada baris preamble, komentar `#` di akhir
        // perintah pengguna bisa "menelan" penanda exit.
        let s = build("Write-Output 1 # komentar");
        let idx_last = s.rfind(SENTINEL).unwrap();
        assert!(s[..idx_last].contains("Write-Output 1 # komentar\n"));
    }

    #[test]
    fn parse_extracts_code_and_strips_sentinel() {
        let raw = "baris 1\r\nbaris 2\r\n__ARKA_EXIT__128";
        let (code, out) = parse_output(raw);
        assert_eq!(code, Some(128));
        assert_eq!(out, "baris 1\nbaris 2");
    }

    #[test]
    fn parse_without_sentinel_falls_back_to_none() {
        let (code, out) = parse_output("Output biasa\r\n");
        assert_eq!(code, None);
        assert_eq!(out, "Output biasa");
    }

    #[test]
    fn parse_handles_crlf_and_trailing_newline() {
        let (code, out) = parse_output("a\r\nb\r\n__ARKA_EXIT__0\r\n");
        assert_eq!(code, Some(0));
        assert_eq!(out, "a\nb");
    }

    #[test]
    fn parse_survives_null_last_exit_code() {
        // `[string]$null` = "" → tidak bisa diparse → None (pakai exit proses).
        let (code, out) = parse_output("x\n__ARKA_EXIT__");
        assert_eq!(code, None);
        assert_eq!(out, "x");
    }

    #[test]
    fn parse_keeps_output_that_merely_contains_word() {
        let (code, out) = parse_output("lihat __ARKA_EXIT__ di dalam teks\n__ARKA_EXIT__0");
        assert_eq!(code, Some(0));
        assert_eq!(out, "lihat __ARKA_EXIT__ di dalam teks");
    }

    #[test]
    fn preamble_sets_error_handling_and_utf8() {
        let p = preamble();
        assert!(p.contains("$ErrorActionPreference = 'Continue'"));
        assert!(p.contains("$ProgressPreference = 'SilentlyContinue'"));
        assert!(p.contains("OutputEncoding = [System.Text.Encoding]::UTF8"));
        assert!(p.contains("PYTHONIOENCODING"));
        assert!(p.contains("$global:LASTEXITCODE = 0"));
    }

    #[test]
    fn preamble_never_mentions_linux_tools() {
        // Gerbang sama seperti prompt: kalau ada yang menyalin preamble dari
        // versi Android, test ini merah.
        let p = preamble().to_lowercase();
        for bad in ["apt-get", "proot", "/root/storage", "bash -l"] {
            assert!(!p.contains(bad), "preamble memuat {bad}");
        }
    }

    #[test]
    fn normalize_code_maps_missing_to_negative() {
        assert_eq!(normalize_code(Some(0)), 0);
        assert_eq!(normalize_code(Some(128)), 128);
        assert_eq!(normalize_code(None), -2);
    }
}
