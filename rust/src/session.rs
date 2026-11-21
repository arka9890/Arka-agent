//! Sesi chat + pesan.
//!
//! Sesi menyimpan provider & model yang dipakai saat itu, sehingga mengganti
//! provider default tidak mengubah riwayat sesi lama.

use crate::db;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Session {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    /// Jumlah pesan; diisi hanya oleh [list].
    #[serde(default)]
    pub message_count: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    /// `user` | `assistant` | `system`
    pub role: String,
    pub content: String,
    /// JSON array `[{path, kind}]` — file workspace yang dilampirkan.
    #[serde(default)]
    pub attachments: String,
    pub created_at: i64,
}

/// Semua sesi, terbaru dulu.
pub fn list() -> Result<Vec<Session>, String> {
    db::with(|c| {
        let mut stmt = c
            .prepare(
                "SELECT s.id, s.title, s.provider_id, s.model, s.created_at, s.updated_at,
                        (SELECT COUNT(*) FROM messages m WHERE m.session_id = s.id)
                 FROM sessions s ORDER BY s.updated_at DESC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Session {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    provider_id: r.get(2)?,
                    model: r.get(3)?,
                    created_at: r.get(4)?,
                    updated_at: r.get(5)?,
                    message_count: r.get(6)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
}

pub fn create(title: &str, provider_id: Option<&str>, model: Option<&str>) -> Result<Session, String> {
    let now = db::now_ms();
    let s = Session {
        id: uuid::Uuid::new_v4().to_string(),
        title: if title.trim().is_empty() { "Sesi baru".to_string() } else { title.trim().to_string() },
        provider_id: provider_id.map(|v| v.to_string()),
        model: model.map(|v| v.to_string()),
        created_at: now,
        updated_at: now,
        message_count: 0,
    };
    db::with(|c| {
        c.execute(
            "INSERT INTO sessions (id, title, provider_id, model, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![s.id, s.title, s.provider_id, s.model, s.created_at, s.updated_at],
        )
        .map_err(|e| e.to_string())
    })?;
    Ok(s)
}

pub fn rename(id: &str, title: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute(
            "UPDATE sessions SET title = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, title.trim(), db::now_ms()],
        )
        .map_err(|e| e.to_string())
        .and_then(|n| if n == 0 { Err("Sesi tidak ditemukan".into()) } else { Ok(()) })
    })
}

pub fn delete(id: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute("DELETE FROM messages WHERE session_id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        c.execute("DELETE FROM sessions WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// Pesan satu sesi, terlama dulu.
pub fn messages(session_id: &str) -> Result<Vec<Message>, String> {
    db::with(|c| {
        let mut stmt = c
            .prepare(
                "SELECT id, session_id, role, content, attachments, created_at
                 FROM messages WHERE session_id = ?1 ORDER BY created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![session_id], |r| {
                Ok(Message {
                    id: r.get(0)?,
                    session_id: r.get(1)?,
                    role: r.get(2)?,
                    content: r.get(3)?,
                    attachments: r.get(4)?,
                    created_at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
}

pub fn add_message(
    session_id: &str,
    role: &str,
    content: &str,
    attachments: &str,
) -> Result<Message, String> {
    let m = Message {
        id: uuid::Uuid::new_v4().to_string(),
        session_id: session_id.to_string(),
        role: role.to_string(),
        content: content.to_string(),
        attachments: if attachments.is_empty() { "[]".into() } else { attachments.to_string() },
        created_at: db::now_ms(),
    };
    db::with(|c| {
        c.execute(
            "INSERT INTO messages (id, session_id, role, content, attachments, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![m.id, m.session_id, m.role, m.content, m.attachments, m.created_at],
        )
        .map_err(|e| e.to_string())?;
        c.execute(
            "UPDATE sessions SET updated_at = ?2 WHERE id = ?1",
            params![session_id, m.created_at],
        )
        .map_err(|e| e.to_string())
    })?;
    Ok(m)
}

/// Ubah judul sesi otomatis dari pesan user pertama (dipotong 60 karakter).
pub fn autotitle(session_id: &str, text: &str) -> Result<(), String> {
    let title: String = text.trim().lines().next().unwrap_or("").chars().take(60).collect();
    if title.is_empty() {
        return Ok(());
    }
    db::with(|c| {
        c.execute(
            "UPDATE sessions SET title = ?2 WHERE id = ?1 AND (title = 'Sesi baru' OR title = '')",
            params![session_id, title],
        )
        .map_err(|e| e.to_string())
        .map(|_| ())
    })
}

/// Ganti provider/model yang dipakai sesi (dipilih dari dropdown di chat).
pub fn set_model(session_id: &str, provider_id: &str, model: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute(
            "UPDATE sessions SET provider_id = ?2, model = ?3, updated_at = ?4 WHERE id = ?1",
            params![
                session_id,
                if provider_id.trim().is_empty() { None } else { Some(provider_id) },
                if model.trim().is_empty() { None } else { Some(model) },
                db::now_ms()
            ],
        )
        .map_err(|e| e.to_string())
        .and_then(|n| if n == 0 { Err("Sesi tidak ditemukan".into()) } else { Ok(()) })
    })
}

