//! Skill agen berformat Claude (`SKILL.md`).
//!
//! File skill memakai frontmatter YAML sederhana:
//!
//! ```markdown
//! ---
//! name: pdf
//! description: Use this skill whenever the user wants to work with PDF files.
//! ---
//!
//! # PDF
//! Instruksi lengkap skill di sini...
//! ```
//!
//! Skill yang aktif disisipkan ke system prompt chat. Aplikasi tidak
//! menambah tool baru: instruksi skill dibaca model seperti instruksi lain.

use crate::db;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Batas total karakter instruksi skill yang disisipkan ke system prompt.
/// Mencegah skill besar menghabiskan anggaran konteks (lihat `context_budget`).
const MAX_PROMPT_CHARS: usize = 16_000;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Isi `SKILL.md` setelah frontmatter (instruksi untuk model).
    pub content: String,
    pub enabled: bool,
    pub created_at: i64,
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 &&
        ((s.starts_with('"') && s.ends_with('"')) ||
         (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parsing frontmatter tanpa dependensi YAML: cukup untuk `key: value`,
/// string berkutip, dan blok `>` / `|` yang dipakai SKILL.md Claude.
fn parse(raw: &str) -> Result<(String, String, String), String> {
    let raw = raw.trim_start_matches('\u{feff}').trim_start();
    let after = raw
        .strip_prefix("---")
        .ok_or("Skill harus diawali frontmatter `---` (format SKILL.md Claude).")?;
    let after = after.strip_prefix('\r').unwrap_or(after);
    let after = after.strip_prefix('\n').unwrap_or(after);

    let mut front: Vec<String> = Vec::new();
    let mut body = String::new();
    let mut closed = false;
    for line in after.split_inclusive('\n') {
        let clean = line.trim_end_matches(['\r', '\n']);
        if clean.trim() == "---" {
            closed = true;
            continue;
        }
        if closed {
            body.push_str(line);
        } else {
            front.push(clean.to_string());
        }
    }
    if !closed {
        return Err("Frontmatter skill tidak ditutup dengan `---`.".into());
    }

    let mut map: HashMap<String, String> = HashMap::new();
    let mut i = 0usize;
    while i < front.len() {
        let line = &front[i];
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim();
            if value == ">" || value == ">-" || value == "|" || value == "|-" {
                let mut block: Vec<String> = Vec::new();
                i += 1;
                while i < front.len() &&
                    (front[i].starts_with(' ') || front[i].starts_with('\t') || front[i].trim().is_empty())
                {
                    block.push(front[i].trim().to_string());
                    i += 1;
                }
                let sep = if value.starts_with('|') { "\n" } else { " " };
                map.insert(key, block.join(sep).trim().to_string());
                continue;
            }
            map.insert(key, unquote(value));
        }
        i += 1;
    }

    let name = map.remove("name").unwrap_or_default().trim().to_string();
    let description = map
        .remove("description")
        .unwrap_or_default()
        .trim()
        .to_string();
    let body = body.trim().to_string();
    if body.is_empty() {
        return Err("Isi skill kosong. Tulis instruksi setelah frontmatter.".into());
    }
    Ok((name, description, body))
}

fn sanitize_id(name: &str) -> String {
    let id: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let id = id.trim_matches('-').to_string();
    if id.is_empty() { "skill".to_string() } else { id }
}

/// Simpan/perbarui skill. Nama yang sama menimpa skill lama (id = nama yang
/// sudah disanitasi), sehingga import ulang tidak membuat duplikat.
pub fn import(raw: &str, fallback_name: &str) -> Result<Skill, String> {
    let (parsed_name, description, content) = parse(raw)?;
    let name = if parsed_name.is_empty() {
        fallback_name.trim().to_string()
    } else {
        parsed_name
    };
    if name.is_empty() {
        return Err("Nama skill wajib diisi di frontmatter `name:`.".into());
    }
    let skill = Skill {
        id: sanitize_id(&name),
        name,
        description,
        content,
        enabled: true,
        created_at: db::now_ms(),
    };
    db::with(|c| {
        c.execute(
            "INSERT INTO skills (id, name, description, content, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name = ?2, description = ?3, content = ?4, created_at = ?5",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.content,
                skill.created_at
            ],
        )
        .map_err(|e| e.to_string())
        .map(|_| ())
    })?;
    Ok(skill)
}

pub fn list() -> Result<Vec<Skill>, String> {
    db::with(|c| {
        let mut stmt = c.prepare(
            "SELECT id, name, description, content, enabled, created_at
             FROM skills ORDER BY created_at DESC",
        )
        .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Skill {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    description: r.get(2)?,
                    content: r.get(3)?,
                    enabled: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                })
            })
            .map_err(|e| e.to_string())?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })
}

pub fn set_enabled(id: &str, enabled: bool) -> Result<(), String> {
    db::with(|c| {
        c.execute(
            "UPDATE skills SET enabled = ?2 WHERE id = ?1",
            params![id, if enabled { 1 } else { 0 }],
        )
        .map_err(|e| e.to_string())
        .and_then(|n| if n == 0 { Err("Skill tidak ditemukan".into()) } else { Ok(()) })
    })
}

pub fn delete(id: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute("DELETE FROM skills WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())
            .map(|_| ())
    })
}

/// Pasang skill dari URL.
///
/// Menerima langsung URL file `SKILL.md` (raw) **atau** URL repo GitHub
/// publik. Untuk repo, semua file bernama `SKILL.md` diambil lewat GitHub API
/// lalu dipasang satu per satu. Dipanggil dari tool agent `install_skill`,
/// sehingga pengguna cukup menempelkan link di chat.
pub fn install_from_url(url: &str) -> Result<String, String> {
    let url = url.trim().trim_end_matches('/');
    if url.is_empty() {
        return Err("URL skill kosong.".into());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .user_agent("ArkaAgent/1.0")
        .build()
        .map_err(|e| format!("Gagal membuat klien HTTP: {e}"))?;

    fn fetch_json(client: &reqwest::blocking::Client, url: &str) -> Result<Value, String> {
        client
            .get(url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json::<Value>())
            .map_err(|e| format!("Gagal menghubungi GitHub: {e}"))
    }

    let (repo, files): (Option<(String, String, String)>, Vec<String>) =
        if let Some(rest) = url.strip_prefix("https://github.com/") {
            let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
            if parts.len() < 2 {
                return Err("URL GitHub tidak valid. Contoh: https://github.com/owner/repo".into());
            }
            let (owner, name) = (parts[0], parts[1]);
            let meta = fetch_json(&client, &format!("https://api.github.com/repos/{owner}/{name}"))?;
            let branch = meta["default_branch"]
                .as_str()
                .unwrap_or("main")
                .to_string();
            let tree = fetch_json(
                &client,
                &format!(
                    "https://api.github.com/repos/{owner}/{name}/git/trees/{branch}?recursive=1"
                ),
            )?;
            let mut found: Vec<String> = Vec::new();
            if let Some(entries) = tree["tree"].as_array() {
                for entry in entries {
                    let path = entry["path"].as_str().unwrap_or("");
                    if path.to_ascii_lowercase().ends_with("skill.md") {
                        found.push(path.to_string());
                    }
                }
            }
            if found.is_empty() {
                return Err("Repo tidak berisi file SKILL.md.".into());
            }
            found.truncate(20);
            (
                Some((owner.to_string(), name.to_string(), branch)),
                found,
            )
        } else {
            (None, vec![url.to_string()])
        };

    let mut installed: Vec<String> = Vec::new();
    for path in files {
        let raw = match &repo {
            Some((owner, name, branch)) => format!(
                "https://raw.githubusercontent.com/{owner}/{name}/{branch}/{path}"
            ),
            None => path.clone(),
        };
        let text = client
            .get(&raw)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text())
            .map_err(|e| format!("Gagal mengunduh skill: {e}"))?;
        // Untuk repo, nama folder induk biasanya nama skill (mis.
        // `skills/pdf/SKILL.md` → `pdf`).
        let fallback = path
            .rsplit('/')
            .nth(1)
            .filter(|s| !s.is_empty())
            .unwrap_or("skill")
            .to_string();
        let skill = import(&text, &fallback)?;
        installed.push(skill.name);
    }
    Ok(format!(
        "Terpasang {} skill: {}. Skill dipakai pada request berikutnya.",
        installed.len(),
        installed.join(", ")
    ))
}

/// Bagian system prompt yang berisi semua skill aktif.
pub fn system_prompt_section() -> String {
    let skills = match list() {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let enabled: Vec<Skill> = skills.into_iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "## Skill agen\n\
         Skill di bawah memakai format Claude. Gunakan skill yang relevan dengan \
         permintaan pengguna dan ikuti instruksinya.\n",
    );
    let mut used = 0usize;
    for s in enabled {
        let block = format!(
            "\n### {}\nDeskripsi: {}\n\n{}\n",
            s.name,
            if s.description.is_empty() { "-" } else { &s.description },
            s.content
        );
        if used + block.len() > MAX_PROMPT_CHARS {
            out.push_str("\n_(Skill lain tidak dimuat karena batas panjang.)_\n");
            break;
        }
        used += block.len();
        out.push_str(&block);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_skill_frontmatter() {
        let raw = "---\nname: pdf\ndescription: Work with PDF files\n---\n\n# PDF\nDo the thing.\n";
        let (name, desc, body) = parse(raw).unwrap();
        assert_eq!(name, "pdf");
        assert_eq!(desc, "Work with PDF files");
        assert!(body.contains("Do the thing."));
    }

    #[test]
    fn parses_folded_description() {
        let raw = "---\nname: web\n description: >-\n   Use for web tasks\n   and research\n---\nBody.\n";
        let (_, desc, _) = parse(raw).unwrap();
        assert_eq!(desc, "Use for web tasks and research");
    }

    #[test]
    fn rejects_missing_frontmatter() {
        assert!(parse("# PDF\nDo it.").is_err());
    }

    #[test]
    fn id_is_sanitized() {
        assert_eq!(sanitize_id("My Cool Skill!"), "my-cool-skill");
        assert_eq!(sanitize_id("---"), "skill");
    }
}
