//! Satu koneksi SQLite yang dipakai bersama semua modul.
//!
//! Kenapa satu koneksi? Semua operasi berasal dari thread UI (lewat JNI) dan
//! jumlahnya kecil; satu koneksi yang dilindungi `Mutex` menghindari
//! `SQLITE_BUSY` antar-koneksi tanpa perlu WAL tuning.
//!
//! Semua waktu disimpan sebagai milidetik epoch (INTEGER) supaya sorting
//! konsisten tanpa parsing tanggal.

use rusqlite::Connection;
use std::sync::{Mutex, OnceLock};

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

/// Buka (atau buat) database di [path] lalu siapkan skema.
///
/// Aman dipanggil berkali-kali: kalau sudah terbuka, path baru diabaikan.
pub fn init(path: &str) -> Result<(), String> {
    if DB.get().is_some() {
        return Ok(());
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch(SCHEMA).map_err(|e| e.to_string())?;
    // Kalau dua thread balapan, yang kalah cukup memakai koneksi pemenang.
    let _ = DB.set(Mutex::new(conn));
    Ok(())
}

/// Jalankan [f] dengan koneksi. Error kalau [init] belum dipanggil.
pub fn with<T>(f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
    let cell = DB.get().ok_or_else(|| "Database belum diinisialisasi".to_string())?;
    let conn = cell.lock().map_err(|e| e.to_string())?;
    f(&conn)
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sessions (
    id          TEXT PRIMARY KEY,
    title       TEXT NOT NULL,
    provider_id TEXT,
    model       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id         TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,
    content    TEXT NOT NULL,
    attachments TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);

CREATE TABLE IF NOT EXISTS providers (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL,
    base_url   TEXT NOT NULL,
    api_key    TEXT NOT NULL,
    model      TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS terminal_history (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    command    TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
"#;
