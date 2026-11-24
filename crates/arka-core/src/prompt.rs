//! Prompt pack per platform: system prompt + deskripsi tool.
//!
//! Sebelumnya semua teks ini hardcode "Ubuntu 24.04 / proot / bash" di
//! `ai.rs`, sehingga build Windows akan menyuruh model menjalankan `apt` dan
//! `ls` di PowerShell. Template dipisah ke `prompt/*.md`, lalu placeholder
//! `{{WORKSPACE}}`, `{{SKILLS_DIR}}`, `{{CONFIG_DIR}}` diisi saat runtime
//! dengan path nyata platform tersebut.
//!
//! Unit test di bawah menjaga agar template Windows tidak pernah menyebut
//! "Ubuntu" atau "bash" — itu tanda regresi paling mudah terjadi saat
//! menyalin teks.

use crate::paths::{self, Platform};
use serde_json::{json, Value};

pub const ANDROID_SYSTEM: &str = include_str!("../prompt/android.md");
pub const WINDOWS_SYSTEM: &str = include_str!("../prompt/windows.md");

pub fn template(platform: Platform) -> &'static str {
    match platform {
        Platform::Android => ANDROID_SYSTEM,
        Platform::Windows => WINDOWS_SYSTEM,
    }
}

/// Isi placeholder pada template.
pub fn render(tpl: &str, workspace: &str, skills_dir: &str, config_dir: &str) -> String {
    tpl.replace("{{WORKSPACE}}", workspace)
        .replace("{{SKILLS_DIR}}", skills_dir)
        .replace("{{CONFIG_DIR}}", config_dir)
}

/// System prompt siap pakai untuk platform tertentu.
pub fn system_prompt(
    platform: Platform,
    workspace: &str,
    skills_dir: &str,
    config_dir: &str,
) -> String {
    render(template(platform), workspace, skills_dir, config_dir)
}

/// System prompt sesuai `paths` yang sedang aktif.
///
/// Kalau `paths` belum diinisialisasi (unit test), dipakai platform Android
/// dengan path kosong supaya tidak panik.
pub fn active_system_prompt() -> String {
    match paths::get() {
        Ok(p) => system_prompt(
            p.platform,
            &p.workspace.to_string_lossy(),
            &p.skills_dir.to_string_lossy(),
            &p.skills_config_dir.to_string_lossy(),
        ),
        Err(_) => system_prompt(Platform::Android, "", "", ""),
    }
}

/// Deskripsi tool `run_command` — inti perbedaan antar platform.
fn run_command_desc(platform: Platform) -> &'static str {
    match platform {
        Platform::Android => {
            "Jalankan perintah bash non-interaktif di workspace Ubuntu 24.04 di perangkat pengguna. \
             Mengembalikan stdout/stderr. Pakai tool ini setiap kali perlu mengecek atau menjalankan \
             sesuatu di Ubuntu/terminal/file."
        }
        Platform::Windows => {
            "Jalankan perintah PowerShell non-interaktif di PC Windows pengguna, di dalam folder \
             workspace. Mengembalikan stdout/stderr dan exit code. Pakai tool ini setiap kali perlu \
             mengecek atau menjalankan sesuatu di komputer/terminal/file. Gunakan perintah PowerShell \
             (Get-ChildItem, Get-Content, Get-Command), bukan perintah khusus Linux."
        }
    }
}

fn run_command_param_desc(platform: Platform, workspace: &str) -> String {
    match platform {
        Platform::Android => {
            format!("Perintah shell yang dijalankan di {workspace}. Contoh: 'cat /etc/os-release' atau 'python3 --version'.")
        }
        Platform::Windows => {
            format!(
                "Perintah PowerShell yang dijalankan di {workspace}. Contoh: '$PSVersionTable' atau 'Get-ChildItem -Force' atau 'python --version'."
            )
        }
    }
}

fn skills_location_desc(platform: Platform) -> &'static str {
    match platform {
        Platform::Android => "Skill tersimpan di ~/.claude/skills/<nama>/SKILL.md di Ubuntu.",
        Platform::Windows => "Skill tersimpan di %USERPROFILE%\\.claude\\skills\\<nama>\\SKILL.md.",
    }
}

/// Skema tool format OpenAI (dipakai provider `openai`-compatible).
pub fn tools_schema(platform: Platform) -> Value {
    let workspace = paths::workspace_display();
    let workspace_hint = if workspace.is_empty() {
        "workspace".to_string()
    } else {
        workspace
    };
    json!([{
        "type": "function",
        "function": {
            "name": "run_command",
            "description": run_command_desc(platform),
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": run_command_param_desc(platform, &workspace_hint),
                    }
                },
                "required": ["command"],
            }
        }
    }, {
        "type": "function",
        "function": {
            "name": "install_skill",
            "description": format!(
                "Pasang skill format Claude (SKILL.md) dari URL file .md atau repo GitHub publik. Membutuhkan persetujuan pengguna. {}",
                skills_location_desc(platform)
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL SKILL.md atau repo GitHub." }
                },
                "required": ["url"],
            }
        }
    }, {
        "type": "function",
        "function": {
            "name": "delete_skill",
            "description": "Hapus skill yang terpasang dari folder skill. Membutuhkan persetujuan pengguna.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Nama skill." }
                },
                "required": ["name"],
            }
        }
    }, {
        "type": "function",
        "function": {
            "name": "edit_skill",
            "description": "Ganti seluruh isi SKILL.md sebuah skill. Membutuhkan persetujuan pengguna.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Nama skill." },
                    "content": { "type": "string", "description": "Isi SKILL.md lengkap dengan frontmatter name/description." }
                },
                "required": ["name", "content"],
            }
        }
    }, {
        "type": "function",
        "function": {
            "name": "set_skill_config",
            "description": "Ubah satu kunci konfigurasi skill. Membutuhkan persetujuan pengguna.",
            "parameters": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Nama skill." },
                    "key": { "type": "string", "description": "Kunci config, mis. defaultMode." },
                    "value": { "type": "string", "description": "Nilai config, mis. ultra." }
                },
                "required": ["name", "key", "value"],
            }
        }
    }, {
        "type": "function",
        "function": {
            "name": "ask_user",
            "description": "Tanyakan sesuatu kepada pengguna dengan beberapa pilihan jawaban. Pakai ini kalau kamu butuh keputusan/garis pilihan sebelum melanjutkan. UI menampilkan kartu pertanyaan.",
            "parameters": {
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "Pertanyaan untuk pengguna." },
                    "options": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Daftar pilihan jawaban (minimal 2).",
                    },
                    "file": {
                        "type": "string",
                        "description": "Opsional (kompatibel): satu path file untuk preview.",
                    },
                    "files": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": format!(
                            "Opsional: daftar path file di workspace ({}) untuk ditampilkan sebagai preview (gambar/video/suara). Maksimal 5 file.",
                            workspace_hint
                        ),
                    }
                },
                "required": ["question", "options"],
            }
        }
    }])
}

/// Skema tool format Anthropic.
pub fn anthropic_tools_schema(platform: Platform) -> Value {
    let workspace = paths::workspace_display();
    let workspace_hint = if workspace.is_empty() {
        "workspace".to_string()
    } else {
        workspace
    };
    json!([{
        "name": "run_command",
        "description": run_command_desc(platform),
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": run_command_param_desc(platform, &workspace_hint),
                }
            },
            "required": ["command"],
        }
    }, {
        "name": "install_skill",
        "description": format!(
            "Pasang skill format Claude (SKILL.md) dari URL file .md atau repo GitHub publik. Membutuhkan persetujuan pengguna. {}",
            skills_location_desc(platform)
        ),
        "input_schema": {
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL SKILL.md atau repo GitHub." }
            },
            "required": ["url"],
        }
    }, {
        "name": "delete_skill",
        "description": "Hapus skill yang terpasang. Membutuhkan persetujuan pengguna.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Nama skill." }
            },
            "required": ["name"],
        }
    }, {
        "name": "edit_skill",
        "description": "Ganti seluruh isi SKILL.md sebuah skill. Membutuhkan persetujuan pengguna.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Nama skill." },
                "content": { "type": "string", "description": "Isi SKILL.md lengkap." }
            },
            "required": ["name", "content"],
        }
    }, {
        "name": "set_skill_config",
        "description": "Ubah satu kunci konfigurasi skill. Membutuhkan persetujuan pengguna.",
        "input_schema": {
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Nama skill." },
                "key": { "type": "string", "description": "Kunci config." },
                "value": { "type": "string", "description": "Nilai config." }
            },
            "required": ["name", "key", "value"],
        }
    }, {
        "name": "ask_user",
        "description": "Tanyakan sesuatu kepada pengguna dengan beberapa pilihan jawaban.",
        "input_schema": {
            "type": "object",
            "properties": {
                "question": { "type": "string", "description": "Pertanyaan untuk pengguna." },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Daftar pilihan jawaban (minimal 2).",
                },
                "file": { "type": "string", "description": "Opsional: satu path file untuk preview." },
                "files": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": format!("Opsional: daftar path file di workspace ({}) untuk preview, maksimal 5.", workspace_hint),
                }
            },
            "required": ["question", "options"],
        }
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(platform: Platform) -> String {
        system_prompt(platform, r"C:\Users\arka\ArkaAgent\workspace", r"C:\Users\arka\.claude\skills", r"C:\Users\arka\.config")
    }

    #[test]
    fn windows_prompt_never_mentions_linux_shell() {
        let p = rendered(Platform::Windows);
        for bad in ["Ubuntu", "bash", "proot", "apt-get", "/root/storage"] {
            assert!(!p.contains(bad), "prompt Windows masih memuat '{bad}'");
        }
    }

    #[test]
    fn windows_prompt_mentions_powershell_and_workspace() {
        let p = rendered(Platform::Windows);
        assert!(p.contains("PowerShell"));
        assert!(p.contains(r"C:\Users\arka\ArkaAgent\workspace"));
        assert!(p.contains("Get-Command"));
    }

    #[test]
    fn android_prompt_keeps_bash_contract() {
        let p = rendered(Platform::Android);
        assert!(p.contains("bash"));
        assert!(p.contains("Ubuntu"));
    }

    #[test]
    fn all_placeholders_are_replaced() {
        for platform in [Platform::Android, Platform::Windows] {
            let p = rendered(platform);
            assert!(!p.contains("{{"), "placeholder tersisa di {:?}", platform);
        }
    }

    #[test]
    fn tool_schema_command_description_follows_platform() {
        let win = tools_schema(Platform::Windows).to_string();
        assert!(!win.contains("Ubuntu"));
        assert!(!win.contains("bash"));
        assert!(win.contains("PowerShell"));

        let anth = anthropic_tools_schema(Platform::Windows).to_string();
        assert!(!anth.contains("Ubuntu"));
        assert!(!anth.contains("bash"));
    }

    #[test]
    fn tool_names_are_stable() {
        for platform in [Platform::Android, Platform::Windows] {
            let names: Vec<String> = tools_schema(platform)
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["function"]["name"].as_str().unwrap().to_string())
                .collect();
            assert_eq!(
                names,
                vec![
                    "run_command",
                    "install_skill",
                    "delete_skill",
                    "edit_skill",
                    "set_skill_config",
                    "ask_user"
                ]
            );
        }
    }
}
