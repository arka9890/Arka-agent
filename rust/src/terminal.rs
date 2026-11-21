//! Riwayat perintah terminal (maksimum [MAX_ITEMS], terbaru di akhir).

use crate::db;
use rusqlite::params;

const MAX_ITEMS: i64 = 200;

/// Riwayat terlama → terbaru.
pub fn list() -> Result<Vec<String>, String> {
    db::with(|c| {
        let mut stmt = c
            .prepare("SELECT command FROM terminal_history ORDER BY id ASC")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
}

/// Catat satu perintah; perintah identik dengan entri terakhir diabaikan.
pub fn push(command: &str) -> Result<Vec<String>, String> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return list();
    }
    db::with(|c| {
        let last: Option<String> = c
            .query_row("SELECT command FROM terminal_history ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .ok();
        if last.as_deref() == Some(cmd) {
            return Ok(());
        }
        c.execute(
            "INSERT INTO terminal_history (command, created_at) VALUES (?1, ?2)",
            params![cmd, db::now_ms()],
        )
        .map_err(|e| e.to_string())?;
        // Batasi ukuran riwayat supaya database tidak tumbuh tanpa batas.
        c.execute(
            "DELETE FROM terminal_history WHERE id NOT IN
             (SELECT id FROM terminal_history ORDER BY id DESC LIMIT ?1)",
            params![MAX_ITEMS],
        )
        .map_err(|e| e.to_string())
        .map(|_| ())
    })?;
    list()
}

pub fn clear() -> Result<(), String> {
    db::with(|c| {
        c.execute("DELETE FROM terminal_history", [])
            .map_err(|e| e.to_string())
            .map(|_| ())
    })
}
