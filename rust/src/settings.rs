//! Preferensi aplikasi (theme, bahasa, workspace, dll.) di tabel `settings`.
//!
//! Nilai disimpan sebagai string; UI yang memutuskan cara menafsirkannya.
//! Kunci yang tidak dikenal diabaikan supaya versi lama/tema baru tetap aman.

use crate::ai;
use crate::db;
use rusqlite::params;
use std::collections::HashMap;

pub const DEFAULT_WORKSPACE: &str = "/root/storage";

/// Batas putaran tool bawaan (dipakai kalau pengguna belum mengubah setelan).
/// Tugas panjang (clone, build, uji) sering butuh belasan perintah; dulu 6
/// membuat model berhenti di tengah jalan.
pub const DEFAULT_MAX_TOOL_ROUNDS: usize = 40;

/// Batas aman atas: mencegah salah ketik (mis. 4000) membuat satu chat
/// berjalan sangat lama tanpa kendali.
pub const MAX_TOOL_ROUNDS_LIMIT: usize = 200;

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

/// Nilai default per kunci (dipakai saat kunci belum pernah disimpan).
pub fn defaults() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("theme", "dark"),
        ("language", "id"),
        ("font_size", "medium"),
        ("auto_scroll", "true"),
        ("notifications", "true"),
        ("workspace_path", DEFAULT_WORKSPACE),
        ("streaming", "true"),
        ("max_tool_rounds", "40"),
        // Kosong = pakai prompt bawaan (`ai::SYSTEM_PROMPT`).
        ("system_prompt", ""),
        // Ringkas otomatis riwayat lama supaya konteks tidak hilang saat
        // model/router punya context window kecil.
        ("auto_summary", "true"),
        ("context_budget", "24000"),
        ("tool_output_chars", "4000"),
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
        _ => ai::SYSTEM_PROMPT.to_string(),
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
        defaults().into_iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
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
    }

    #[test]
    fn system_prompt_falls_back_to_builtin() {
        assert_eq!(system_prompt(), ai::SYSTEM_PROMPT);
    }

    #[test]
    fn defaults_contain_agent_keys() {
        let d = defaults();
        assert_eq!(d.get("max_tool_rounds"), Some(&"40"));
        assert_eq!(d.get("system_prompt"), Some(&""));
        assert_eq!(d.get("auto_summary"), Some(&"true"));
        assert_eq!(d.get("context_budget"), Some(&"24000"));
        assert_eq!(d.get("tool_output_chars"), Some(&"4000"));
    }

    #[test]
    fn context_settings_fall_back_to_defaults() {
        assert!(auto_summary());
        assert_eq!(context_budget(), DEFAULT_CONTEXT_BUDGET);
        assert_eq!(tool_output_chars(), DEFAULT_TOOL_OUTPUT_CHARS);
    }
}
