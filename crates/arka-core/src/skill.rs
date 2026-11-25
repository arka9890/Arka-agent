//! Skill agen berformat Claude yang disimpan **di dalam proot Ubuntu**.
//!
//! Letak di guest:
//!   ~/.claude/skills/<nama>/SKILL.md
//!   ~/.config/<nama>/config.json
//!
//! Aplikasi membaca folder ini lewat path host rootfs, lalu menyisipkan
//! instruksi + konfigurasi skill aktif ke system prompt saat chat. Model juga
//! bisa melihat/mengubahnya lewat `run_command`, jadi tidak ada perbedaan
//! antara "skill di aplikasi" dan "skill di filesystem" seperti sebelumnya.

use crate::paths;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

/// Batas total karakter instruksi skill di system prompt.
const MAX_PROMPT_CHARS: usize = 16_000;

#[derive(Serialize, Clone, Debug)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub content: String,
    #[serde(default)]
    pub config: HashMap<String, String>,
}

fn skills_root() -> Result<PathBuf, String> {
    paths::skills_dir()
}

fn config_path(name: &str) -> Result<PathBuf, String> {
    Ok(paths::skills_config_dir()?
        .join(sanitize_id(name))
        .join("config.json"))
}

fn skill_dir(name: &str) -> Result<PathBuf, String> {
    Ok(skills_root()?.join(sanitize_id(name)))
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Parser frontmatter SKILL.md (tanpa dependency YAML).
fn parse(raw: &str) -> Result<(String, String, String), String> {
    let raw = raw.trim_start_matches('\u{feff}').trim_start();
    let after = raw
        .strip_prefix("---")
        .ok_or("Skill harus diawali `---` (format SKILL.md Claude).")?;
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
        if let Some((key, value)) = front[i].split_once(':') {
            let key = key.trim().to_string();
            let value = value.trim();
            if value == ">" || value == ">-" || value == "|" || value == "|-" {
                let mut block: Vec<String> = Vec::new();
                i += 1;
                while i < front.len()
                    && (front[i].starts_with(' ')
                        || front[i].starts_with('\t')
                        || front[i].trim().is_empty())
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
    if id.is_empty() {
        "skill".to_string()
    } else {
        id
    }
}

fn read_config(name: &str) -> HashMap<String, String> {
    let Ok(path) = config_path(name) else {
        return HashMap::new();
    };
    let Ok(text) = fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
        return HashMap::new();
    };
    map.into_iter()
        .filter_map(|(k, v)| {
            Some((
                k,
                match v {
                    Value::String(s) => s,
                    other => other.to_string(),
                },
            ))
        })
        .collect()
}

/// Daftar skill dari filesystem proot. File rusak dilewati, bukan menggagalkan
/// seluruh daftar.
pub fn list() -> Vec<Skill> {
    let Ok(root) = skills_root() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<Skill> = Vec::new();
    for entry in entries.flatten() {
        let file = entry.path().join("SKILL.md");
        let Ok(raw) = fs::read_to_string(&file) else {
            continue;
        };
        let folder = entry.file_name().to_string_lossy().to_string();
        let (name, description, content) = match parse(&raw) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let name = if name.is_empty() { folder } else { name };
        out.push(Skill {
            config: read_config(&name),
            name,
            description,
            content,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Bagian system prompt yang berisi semua skill yang terpasang.
///
/// Teks pembukanya memakai path skill platform aktif (bukan hardcode
/// `~/.claude/skills` di Ubuntu) supaya build Windows tidak menyebut Linux.
pub fn system_prompt_section() -> String {
    let skills = list();
    if skills.is_empty() {
        return String::new();
    }
    let dir = paths::get()
        .map(|p| p.skills_dir.to_string_lossy().to_string())
        .unwrap_or_else(|_| "~/.claude/skills".to_string());
    let mut out = format!(
        "## Skill agen\nSkill berikut tersimpan di `{dir}`. Gunakan yang relevan dengan permintaan pengguna dan ikuti instruksinya.\n"
    );
    let mut used = 0usize;
    for s in skills {
        let block = format!(
            "\n### {}\nDeskripsi: {}\n\n{}\n",
            s.name,
            if s.description.is_empty() {
                "-"
            } else {
                &s.description
            },
            s.content
        );
        if used + block.len() > MAX_PROMPT_CHARS {
            out.push_str("\n_(Skill lain tidak dimuat karena batas panjang.)_\n");
            break;
        }
        used += block.len();
        out.push_str(&block);
        if !s.config.is_empty() {
            let mut cfg = s.config.iter().collect::<Vec<_>>();
            cfg.sort_by_key(|(k, _)| *k);
            out.push_str(&format!(
                "Konfigurasi: {}.\n",
                cfg.into_iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    out
}

fn write_skill(name: &str, raw: &str) -> Result<String, String> {
    let (parsed_name, description, _body) = parse(raw)?;
    let final_name = if parsed_name.is_empty() {
        name.to_string()
    } else {
        parsed_name
    };
    let dir = skill_dir(&final_name)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Gagal membuat folder skill: {e}"))?;
    fs::write(dir.join("SKILL.md"), raw).map_err(|e| format!("Gagal menulis SKILL.md: {e}"))?;
    Ok(format!("{} ({})", final_name, description))
}

/// Pasang skill dari URL file .md atau repo GitHub publik.
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

    // I3: deteksi rate limit GitHub (403/429) dengan pesan yang jelas.
    // Tanpa token, API GitHub membatasi ~60 request/jam per IP.
    let fetch_json = |u: &str| -> Result<Value, String> {
        let resp = client
            .get(u)
            .send()
            .map_err(|e| format!("Gagal menghubungi GitHub: {e}"))?;
        let status = resp.status();
        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return Err(
                "Rate limit GitHub API tercapai (403/429). Coba lagi dalam beberapa menit,                  atau set token GitHub (env GITHUB_TOKEN) untuk batas lebih tinggi."
                    .into(),
            );
        }
        let resp = resp
            .error_for_status()
            .map_err(|e| format!("Gagal menghubungi GitHub: {e}"))?;
        resp.json::<Value>()
            .map_err(|e| format!("Gagal membaca respons GitHub: {e}"))
    };

    let (repo, files): (Option<(String, String, String)>, Vec<String>) = if let Some(rest) =
        url.strip_prefix("https://github.com/")
    {
        let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return Err("URL GitHub tidak valid.".into());
        }
        let (owner, name) = (parts[0], parts[1]);
        let meta = fetch_json(&format!("https://api.github.com/repos/{owner}/{name}"))?;
        let branch = meta["default_branch"]
            .as_str()
            .unwrap_or("main")
            .to_string();
        let tree = fetch_json(&format!(
            "https://api.github.com/repos/{owner}/{name}/git/trees/{branch}?recursive=1"
        ))?;
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
            return Err("Repo tidak berisi SKILL.md.".into());
        }
        // I2: jangan memotong diam-diam. Proses SEMUA SKILL.md, atau beri
        // peringatan eksplisit bila melebihi batas. Di sini kita proses semua
        // agar tidak ada skill yang terlewat tanpa sepengetahuan pengguna.
        const MAX_SKILLS: usize = 50;
        if found.len() > MAX_SKILLS {
            return Err(format!(
                    "Repo berisi {} SKILL.md (lebih dari batas {}). Persempit                      sumber atau gunakan path file .md langsung.",
                    found.len(),
                    MAX_SKILLS
                ));
        }
        (Some((owner.to_string(), name.to_string(), branch)), found)
    } else {
        (None, vec![url.to_string()])
    };

    let mut installed = Vec::new();
    for path in files {
        let raw_url = match &repo {
            Some((owner, name, branch)) => {
                format!("https://raw.githubusercontent.com/{owner}/{name}/{branch}/{path}")
            }
            None => path.clone(),
        };
        let raw = client
            .get(&raw_url)
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.text())
            .map_err(|e| format!("Gagal mengunduh skill: {e}"))?;
        let fallback = path
            .rsplit('/')
            .nth(1)
            .filter(|s| !s.is_empty())
            .unwrap_or("skill")
            .to_string();
        installed.push(write_skill(&fallback, &raw)?);
    }
    Ok(format!(
        "Terpasang {} skill: {}. Berlaku pada request berikutnya.",
        installed.len(),
        installed.join(", ")
    ))
}

/// Timpa isi SKILL.md sebuah skill (nama dari frontmatter dipakai sebagai folder).
pub fn edit(name: &str, raw: &str) -> Result<String, String> {
    write_skill(name, raw)
}

/// Hapus folder skill + config-nya.
pub fn delete(name: &str) -> Result<(), String> {
    let dir = skill_dir(name)?;
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("Gagal menghapus skill: {e}"))?;
    }
    let cfg_dir = config_path(name)?;
    if let Some(parent) = cfg_dir.parent() {
        if parent.exists() {
            let _ = fs::remove_dir_all(parent);
        }
    }
    Ok(())
}

/// Set satu kunci di config.json skill.
pub fn set_config(name: &str, key: &str, value: &str) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Kunci konfigurasi wajib diisi.".into());
    }
    let path = config_path(name)?;
    let mut map = read_config(name);
    map.insert(key.to_string(), value.to_string());
    let obj: serde_json::Map<String, Value> = map
        .into_iter()
        .map(|(k, v)| (k, Value::String(v)))
        .collect();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Gagal membuat folder config: {e}"))?;
    }
    fs::write(
        &path,
        serde_json::to_string_pretty(&json!(obj)).unwrap_or_default(),
    )
    .map_err(|e| format!("Gagal menulis config: {e}"))?;
    Ok(format!("{key} = {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_claude_skill() {
        let raw = "---\nname: pdf\ndescription: Work with PDF\n---\n\n# PDF\nDo it.\n";
        let (n, d, b) = parse(raw).unwrap();
        assert_eq!(n, "pdf");
        assert_eq!(d, "Work with PDF");
        assert!(b.contains("Do it."));
    }

    #[test]
    fn rejects_bad_frontmatter() {
        assert!(parse("no frontmatter").is_err());
    }

    #[test]
    fn id_is_sanitized() {
        assert_eq!(sanitize_id("My Cool Skill!"), "my-cool-skill");
        assert_eq!(sanitize_id("???"), "skill");
    }
}
