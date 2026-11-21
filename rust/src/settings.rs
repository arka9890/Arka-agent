//! Preferensi aplikasi (theme, bahasa, workspace, dll.) di tabel `settings`.
//!
//! Nilai disimpan sebagai string; UI yang memutuskan cara menafsirkannya.
//! Kunci yang tidak dikenal diabaikan supaya versi lama/tema baru tetap aman.

use crate::db;
use rusqlite::params;
use std::collections::HashMap;

pub const DEFAULT_WORKSPACE: &str = "/root/storage";

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
