//! Provider AI: penyimpanan kredensial + definisi protokol API.
//!
//! Hanya dua bentuk protokol yang didukung karena keduanya menaungi semua
//! provider di PRD: `openai` (OpenAI, Mistral, Google Gemini lewat endpoint
//! kompatibel, dan semua provider custom) dan `anthropic`.

use crate::db;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// `openai` | `anthropic`
    pub kind: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    /// Semua model yang dipilih pengguna di tab Provider (JSON array di DB).
    /// [model] tetap dipakai sebagai model utama saat chat.
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub created_at: i64,
}

/// Provider yang dibuat saat database masih kosong, supaya tab Provider tidak
/// kosong melompong di peluncuran pertama. API key sengaja kosong.
const SEEDS: &[(&str, &str, &str, &str)] = &[
    ("OpenAI", "openai", "https://api.openai.com/v1", "gpt-4o-mini"),
    ("Mistral", "openai", "https://api.mistral.ai/v1", "mistral-small-latest"),
    ("Anthropic", "anthropic", "https://api.anthropic.com", "claude-3-5-sonnet-latest"),
    (
        "Google",
        "openai",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "gemini-2.0-flash",
    ),
];

pub fn seed_if_empty() -> Result<(), String> {
    db::with(|c| {
        let count: i64 = c
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if count > 0 {
            return Ok(());
        }
        for (i, (name, kind, base, model)) in SEEDS.iter().enumerate() {
            c.execute(
                "INSERT INTO providers (id, name, kind, base_url, api_key, model, models, is_default, created_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, '[]', ?6, ?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    name,
                    kind,
                    base,
                    model,
                    if i == 0 { 1 } else { 0 },
                    db::now_ms()
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}

fn row_to_provider(r: &rusqlite::Row) -> rusqlite::Result<Provider> {
    Ok(Provider {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        base_url: r.get(3)?,
        api_key: r.get(4)?,
        model: r.get(5)?,
        models: parse_models(&r.get::<_, String>(8)?),
        is_default: r.get::<_, i64>(6)? != 0,
        created_at: r.get(7)?,
    })
}

pub fn list() -> Result<Vec<Provider>, String> {
    db::with(|c| {
        let mut stmt = c
            .prepare(
                "SELECT id, name, kind, base_url, api_key, model, is_default, created_at, models
                 FROM providers ORDER BY is_default DESC, created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], row_to_provider).map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
}

pub fn get(id: &str) -> Result<Provider, String> {
    db::with(|c| {
        c.query_row(
            "SELECT id, name, kind, base_url, api_key, model, is_default, created_at, models
             FROM providers WHERE id = ?1",
            params![id],
            row_to_provider,
        )
        .map_err(|_| "Provider tidak ditemukan".to_string())
    })
}

fn parse_models(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Provider default; kalau tidak ada yang ditandai, ambil yang pertama.
pub fn default_provider() -> Result<Option<Provider>, String> {
    let all = list()?;
    Ok(all.iter().find(|p| p.is_default).cloned().or_else(|| all.first().cloned()))
}

/// Simpan (insert kalau `id` kosong, update kalau sudah ada).
pub fn save(mut p: Provider) -> Result<Provider, String> {
    if p.name.trim().is_empty() {
        return Err("Nama provider wajib diisi".into());
    }
    if p.base_url.trim().is_empty() {
        return Err("Base URL wajib diisi".into());
    }
    p.base_url = p.base_url.trim().trim_end_matches('/').to_string();
    if p.kind != "anthropic" {
        p.kind = "openai".into();
    }
    if p.id.trim().is_empty() {
        p.id = uuid::Uuid::new_v4().to_string();
        p.created_at = db::now_ms();
    } else {
        p.created_at = get(&p.id).map(|old| old.created_at).unwrap_or_else(|_| db::now_ms());
    }
    // Model utama selalu ikut masuk daftar supaya dropdown chat tidak pernah
    // kosong walaupun endpoint /models provider gagal/ tidak tersedia.
    let mut models: Vec<String> = p.models.iter().map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty()).collect();
    if !p.model.trim().is_empty() && !models.contains(&p.model.trim().to_string()) {
        models.insert(0, p.model.trim().to_string());
    }
    let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".into());
    db::with(|c| {
        c.execute(
            "INSERT INTO providers (id, name, kind, base_url, api_key, model, models, is_default, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET name=?2, kind=?3, base_url=?4, api_key=?5,
                model=?6, models=?7, is_default=?8",
            params![
                p.id,
                p.name.trim(),
                p.kind,
                p.base_url,
                p.api_key.trim(),
                p.model.trim(),
                models_json,
                if p.is_default { 1 } else { 0 },
                p.created_at
            ],
        )
        .map_err(|e| e.to_string())
    })?;
    if p.is_default {
        set_default(&p.id)?;
    }
    Ok(p)
}

pub fn set_default(id: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute("UPDATE providers SET is_default = 0", [])
            .map_err(|e| e.to_string())?;
        c.execute("UPDATE providers SET is_default = 1 WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())
            .map(|_| ())
    })
}

pub fn delete(id: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute("DELETE FROM providers WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())
            .map(|_| ())
    })
}
