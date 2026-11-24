//! Preferensi aplikasi (theme, bahasa, workspace, dll.) di tabel `settings`.
//!
//! Nilai disimpan sebagai string; UI yang memutuskan cara menafsirkannya.
//! Kunci yang tidak dikenal diabaikan supaya versi lama/tema baru tetap aman.

use crate::db;
use crate::prompt;
use rusqlite::params;
use std::collections::HashMap;

/// Workspace bawaan Android (di dalam rootfs proot). Platform lain memakai
/// [`workspace_default`], jadi konstanta ini tidak boleh dipakai langsung.
pub const DEFAULT_WORKSPACE: &str = "/root/storage";

/// Batas putaran tool bawaan (dipakai kalau pengguna belum mengubah setelan).
/// Tugas panjang (clone, build, uji) sering butuh belasan perintah; dulu 6
/// membuat model berhenti di tengah jalan.
pub const DEFAULT_MAX_TOOL_ROUNDS: usize = 40;

/// Batas aman atas: mencegah salah ketik (mis. 100000) membuat satu chat
/// berjalan sangat lama tanpa kendali. Batas waktu chat (60–7200 detik) tetap
/// menjadi pengendali durasi sebenarnya, jadi angka besar di sini aman.
pub const MAX_TOOL_ROUNDS_LIMIT: usize = 2000;

/// Anggaran token konteks bawaan. Riwayat di atas angka ini diringkas lebih
/// dulu sebelum dikirim, supaya model kecil (mis. anggota combo OmniRoute
/// 8k/32k) tidak memotong awal percakapan.
pub const DEFAULT_CONTEXT_BUDGET: usize = 24_000;
pub const CONTEXT_BUDGET_MIN: usize = 4_000;
pub const CONTEXT_BUDGET_MAX: usize = 128_000;

/// Batas karakter output tool per perintah. Output besar mempercepat
/// konteks penuh dan membuat model kehilangan instruksi awal.
pub const DEFAULT_TOOL_OUTPUT_CHARS: usize = 4_000;
pub const TOOL_OUTPUT_CHARS_MIN: usize = 1_000;
pub const TOOL_OUTPUT_CHARS_MAX: usize = 20_000;

/// Batas waktu total satu balasan agent (detik). Mencegah satu chat berjalan
/// berjam-jam tanpa bisa dibedakan dari "freeze". Masih bisa diatur pengguna.
pub const DEFAULT_CHAT_TIMEOUT_SECS: u64 = 900;
pub const CHAT_TIMEOUT_MIN: u64 = 60;
pub const CHAT_TIMEOUT_MAX: u64 = 7200;

/// Workspace bawaan: path platform aktif kalau sudah diketahui.
///
/// Kalau `paths::init` belum dipanggil (mis. unit test), tebakan diambil dari
/// platform hasil kompilasi: Windows memakai `%USERPROFILE%\ArkaAgent\workspace`,
/// Android tetap `/root/storage` di dalam proot. Dulu di sini selalu
/// `/root/storage`, sehingga UI Windows menampilkan folder Linux yang tidak ada.
fn workspace_default() -> String {
    let p = crate::paths::workspace_display();
    if !p.is_empty() {
        return p;
    }
    if cfg!(windows) {
        let home = std::env::var("USERPROFILE")
            .ok()
            .filter(|h| !h.trim().is_empty())
            .unwrap_or_else(|| "%USERPROFILE%".to_string());
        return std::path::PathBuf::from(home)
            .join("ArkaAgent")
            .join("workspace")
            .to_string_lossy()
            .to_string();
    }
    DEFAULT_WORKSPACE.to_string()
}

/// Nilai default per kunci (dipakai saat kunci belum pernah disimpan).
pub fn defaults() -> HashMap<&'static str, String> {
    HashMap::from([
        ("theme", "dark".to_string()),
        ("language", "id".to_string()),
        ("font_size", "medium".to_string()),
        ("auto_scroll", "true".to_string()),
        ("notifications", "true".to_string()),
        ("workspace_path", workspace_default()),
        ("streaming", "true".to_string()),
        ("max_tool_rounds", "40".to_string()),
        // Kosong = pakai prompt bawaan platform (`prompt::active_system_prompt`).
        ("system_prompt", "".to_string()),
        // Ringkas otomatis riwayat lama supaya konteks tidak hilang saat
        // model/router punya context window kecil.
        ("auto_summary", "true".to_string()),
        ("context_budget", "24000".to_string()),
        ("tool_output_chars", "4000".to_string()),
        ("chat_timeout_secs", "900".to_string()),
        ("context_auto", "true".to_string()),
    ])
}

pub fn set(key: &str, value: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )
        .map_err(|e| e.to_string())
        .map(|_| ())
    })
}

/// Ambil satu nilai tersimpan (bukan default). `None` kalau belum ada.
pub fn get(key: &str) -> Option<String> {
    db::with(|c| {
        c.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| {
            r.get::<_, String>(0)
        })
        .map_err(|e| e.to_string())
    })
    .ok()
}

/// Batas putaran tool untuk agent loop, sudah divalidasi.
///
/// Nilai tidak valid (kosong, bukan angka, 0) jatuh ke default supaya chat
/// tidak berhenti tanpa satu pun perintah; nilai terlalu besar dipotong ke
/// [`MAX_TOOL_ROUNDS_LIMIT`].
pub fn max_tool_rounds() -> usize {
    get("max_tool_rounds")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .filter(|n| *n > 0)
        .map(|n| n.min(MAX_TOOL_ROUNDS_LIMIT))
        .unwrap_or(DEFAULT_MAX_TOOL_ROUNDS)
}

/// System prompt yang dipakai chat: prompt kustom pengguna kalau diisi,
/// kalau tidak prompt bawaan Arka Agent.
pub fn system_prompt() -> String {
    match get("system_prompt") {
        Some(v) if !v.trim().is_empty() => v,
        _ => prompt::active_system_prompt(),
    }
}

/// Ringkas otomatis riwayat lama (default aktif).
pub fn auto_summary() -> bool {
    get("auto_summary")
        .map(|v| v.trim() != "false")
        .unwrap_or(true)
}

/// Anggaran token konteks, sudah dijepit ke rentang aman.
pub fn context_budget() -> usize {
    get("context_budget")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|n| n.clamp(CONTEXT_BUDGET_MIN, CONTEXT_BUDGET_MAX))
        .unwrap_or(DEFAULT_CONTEXT_BUDGET)
}

/// Anggaran konteks otomatis mengikuti jendela konteks model (default aktif).
///
/// Matikan kalau mau memakai angka "Batas konteks" berapa pun modelnya.
pub fn context_auto() -> bool {
    get("context_auto")
        .map(|v| v.trim() != "false")
        .unwrap_or(true)
}

/// Batas waktu total satu balasan agent, sudah dijepit (detik).
pub fn chat_timeout_secs() -> u64 {
    get("chat_timeout_secs")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map(|n| n.clamp(CHAT_TIMEOUT_MIN, CHAT_TIMEOUT_MAX))
        .unwrap_or(DEFAULT_CHAT_TIMEOUT_SECS)
}

/// Batas karakter output tool per perintah, sudah dijepit.
pub fn tool_output_chars() -> usize {
    get("tool_output_chars")
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map(|n| n.clamp(TOOL_OUTPUT_CHARS_MIN, TOOL_OUTPUT_CHARS_MAX))
        .unwrap_or(DEFAULT_TOOL_OUTPUT_CHARS)
}

/// Semua preferensi (default ditimpa nilai tersimpan) sebagai JSON.
pub fn all_json() -> Result<String, String> {
    let stored = db::with(|c| {
        let mut stmt = c.prepare("SELECT key, value FROM settings").map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<HashMap<String, String>, _>>().map_err(|e| e.to_string())
    })?;
    let mut merged: HashMap<String, String> =
        defaults().into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    merged.extend(stored);
    serde_json::to_string(&merged).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Batas putaran harus selalu masuk akal, termasuk saat DB belum siap
    /// (test ini berjalan tanpa `db::init`, jadi `get` mengembalikan `None`).
    #[test]
    fn max_tool_rounds_falls_back_to_default() {
        assert_eq!(max_tool_rounds(), DEFAULT_MAX_TOOL_ROUNDS);
        // Batas atas sengaja besar (2000); durasi sebenarnya dikendalikan
        // oleh chat_timeout_secs, bukan oleh jumlah putaran.
        assert_eq!(MAX_TOOL_ROUNDS_LIMIT, 2000);
    }

    #[test]
    fn system_prompt_falls_back_to_builtin() {
        assert!(!system_prompt().is_empty());
    }

    #[test]
    fn workspace_default_is_not_linux_path_on_windows_builds() {
        let w = workspace_default();
        if cfg!(windows) {
            assert!(!w.contains("/root/storage"), "workspace Windows: {w}");
            assert!(w.to_lowercase().contains("arkaagent"), "workspace Windows: {w}");
        } else {
            assert_eq!(w, DEFAULT_WORKSPACE);
        }
    }

    #[test]
    fn defaults_contain_agent_keys() {
        let d = defaults();
        assert_eq!(d.get("max_tool_rounds").map(String::as_str), Some("40"));
        assert_eq!(d.get("system_prompt").map(String::as_str), Some(""));
        assert_eq!(d.get("auto_summary").map(String::as_str), Some("true"));
        assert_eq!(d.get("context_budget").map(String::as_str), Some("24000"));
        assert_eq!(d.get("tool_output_chars").map(String::as_str), Some("4000"));
        assert_eq!(d.get("chat_timeout_secs").map(String::as_str), Some("900"));
        assert_eq!(d.get("context_auto").map(String::as_str), Some("true"));
        assert!(!d.get("workspace_path").unwrap().is_empty());
    }

    #[test]
    fn context_settings_fall_back_to_defaults() {
        assert!(auto_summary());
        assert_eq!(context_budget(), DEFAULT_CONTEXT_BUDGET);
        assert_eq!(tool_output_chars(), DEFAULT_TOOL_OUTPUT_CHARS);
        assert_eq!(chat_timeout_secs(), DEFAULT_CHAT_TIMEOUT_SECS);
        assert!(context_auto());
    }
}
