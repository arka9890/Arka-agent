//! Agent loop: kirim percakapan, alirkan jawaban, jalankan tool, ulangi.
//!
//! Modul ini sebelumnya hidup di `lib.rs` dan memanggil method Kotlin langsung
//! lewat `JNIEnv`. Sekarang semua interaksi UI lewat [`Events`] dan semua
//! eksekusi perintah lewat trait [`crate::shell::Shell`], sehingga alur yang
//! sama dipakai Android (JNI + proot) dan Windows (egui + PowerShell).
//!
//! Pembatalan **per sesi**: satu chat yang dibatalkan tidak mengganggu chat
//! lain yang berjalan bersamaan.

use crate::ai::{self, ChatMsg};
use crate::events::Events;
use crate::paths::Platform;
use crate::provider::Provider as AiProvider;
use crate::shell;
use crate::ui;
use crate::{memory, paths, prompt, provider, session, settings, skill};

use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Permintaan satu putaran chat dari UI.
#[derive(Clone, Debug, Default)]
pub struct ChatRequest {
    pub session_id: String,
    pub provider_id: String,
    pub model: String,
    /// Riwayat pesan (JSON array `ai::ChatMsg`).
    pub messages_json: String,
}

static CHAT_CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

fn cancel_map() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    CHAT_CANCELS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Flag batal khusus sesi ini.
///
/// Dulu satu flag global dipakai semua sesi: chat sesi lama ikut terbatal saat
/// pengguna memulai chat baru. Sekarang tiap sesi punya flag sendiri dan run
/// lama memegang `Arc`-nya sendiri.
fn new_chat_cancel(session_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    if let Ok(mut guard) = cancel_map().lock() {
        guard.insert(session_id.to_string(), flag.clone());
    }
    flag
}

fn remove_chat_cancel(session_id: &str, flag: &Arc<AtomicBool>) {
    if let Ok(mut guard) = cancel_map().lock() {
        if guard
            .get(session_id)
            .map(|f| Arc::ptr_eq(f, flag))
            .unwrap_or(false)
        {
            guard.remove(session_id);
        }
    }
}

/// Bersihkan flag saat run selesai (termasuk lewat `?` lebih awal).
struct CancelGuard {
    session_id: String,
    flag: Arc<AtomicBool>,
}

impl Drop for CancelGuard {
    fn drop(&mut self) {
        remove_chat_cancel(&self.session_id, &self.flag);
    }
}

/// Set flag batal untuk sesi yang sedang menjawab.
pub fn request_cancel(session_id: &str) {
    let flag = cancel_map()
        .lock()
        .ok()
        .and_then(|g| g.get(session_id).cloned());
    if let Some(flag) = flag {
        flag.store(true, Ordering::SeqCst);
    }
}

/// Potong output perintah supaya tidak membengkakkan prompt berikutnya.
pub fn tail_chars(text: String, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text;
    }
    let skipped = chars.len() - max;
    format!(
        "…(terpotong, {skipped} karakter sebelumnya)\n{}",
        chars[chars.len() - max..].iter().collect::<String>()
    )
}

fn platform() -> Platform {
    paths::platform().unwrap_or(Platform::Android)
}

/// Jalankan satu sesi chat sampai model selesai (atau dibatalkan).
///
/// Error yang membuat sesi berhenti sudah dikirim lewat `ev.error(...)`;
/// nilai `Err` dipakai pemanggil untuk berhenti tanpa mengulang pesan.
pub fn chat(req: ChatRequest, ev: &dyn Events) -> Result<String, String> {
    let session_id = req.session_id.clone();
    let cancel = new_chat_cancel(&session_id);
    let _guard = CancelGuard {
        session_id: session_id.clone(),
        flag: cancel.clone(),
    };

    let (p, mut msgs) = match prepare(&req, &session_id) {
        Ok(v) => v,
        Err(e) => {
            ev.error(&e);
            return Err(e);
        }
    };
    if !msgs.iter().any(|m| m.role == "system") {
        let mut system_prompt = settings::system_prompt();
        // Skill dari folder skill ikut disisipkan ke system prompt.
        let skills = skill::system_prompt_section();
        if !skills.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&skills);
        }
        msgs.insert(0, ChatMsg::plain("system", &system_prompt));
    }
    let model = if req.model.trim().is_empty() {
        p.model.clone()
    } else {
        req.model.clone()
    };

    // Ringkas riwayat lama sebelum dikirim, supaya model kecil (combo/router
    // 8k) tidak memotong awal percakapan tanpa terlihat.
    let budget = settings::context_budget();
    if settings::auto_summary() {
        msgs = memory::prepare(&p, &model, &session_id, msgs, budget, cancel.as_ref());
    }

    let shell = match shell::active() {
        Ok(s) => s,
        Err(e) => {
            ev.error(&e);
            return Err(e);
        }
    };

    let max_tool_rounds = settings::max_tool_rounds();
    let tool_output_chars = settings::tool_output_chars();
    let mut final_text = String::new();
    let mut rounds = 0usize;
    // Token dari setiap putaran dijumlahkan: satu jawaban bisa terdiri dari
    // puluhan panggilan provider (tiap putaran tool = 1 permintaan).
    let mut total_tokens: i64 = 0;
    let mut effective_budget = budget;
    let mut context_retries = 0usize;
    let platform = platform();

    loop {
        // Output tool dari putaran sebelumnya bisa membengkakkan konteks di
        // tengah tugas. Pangkas output paling lama, bukan pesan terbaru.
        memory::fit_tool_outputs(&mut msgs, effective_budget);
        let mut stream = match ai::ChatStream::open(&p, &model, &msgs, platform) {
            Ok(s) => s,
            Err(e)
                if settings::auto_summary()
                    && context_retries < 3
                    && effective_budget > settings::CONTEXT_BUDGET_MIN
                    && ai::is_context_error(&e) =>
            {
                context_retries += 1;
                effective_budget = (effective_budget / 2).max(settings::CONTEXT_BUDGET_MIN);
                if rounds == 0 {
                    msgs = memory::prepare(
                        &p,
                        &model,
                        &session_id,
                        msgs,
                        effective_budget,
                        cancel.as_ref(),
                    );
                } else {
                    // Sudah ada tool call: urutan pesan tidak boleh diubah,
                    // cukup pangkas output tool lebih agresif.
                    memory::fit_tool_outputs(&mut msgs, effective_budget);
                }
                continue;
            }
            Err(e) => {
                ev.error(&e);
                return Err(e);
            }
        };
        let mut answer = String::new();
        let mut stream_error: Option<String> = None;
        while let Some(item) = stream.next() {
            if cancel.load(Ordering::SeqCst) {
                break;
            }
            match item {
                Ok(delta) => {
                    answer.push_str(&delta);
                    ev.delta(&delta);
                }
                Err(e) => {
                    stream_error = Some(e);
                    break;
                }
            }
        }
        total_tokens += stream.usage_tokens();

        // Simpan semua teks dari tiap putaran (bukan hanya yang terakhir):
        // kalau ditimpa, jawaban yang sudah tampil mendadak menyusut.
        if !answer.trim().is_empty() {
            final_text.push_str(&answer);
        }
        if let Some(e) = stream_error {
            if final_text.trim().is_empty() {
                ev.error(&e);
                return Err(e);
            }
            break;
        }
        let calls = stream.take_tool_calls();
        if calls.is_empty() || cancel.load(Ordering::SeqCst) {
            break;
        }
        if rounds >= max_tool_rounds {
            final_text.push_str(&format!(
                "\n\n_(Batas {max_tool_rounds} putaran tool tercapai. Naikkan di Setelan → Agent kalau tugas belum selesai.)_"
            ));
            break;
        }
        rounds += 1;

        for call in calls {
            let args: serde_json::Value =
                serde_json::from_str(&call.arguments).unwrap_or_else(|_| json!({}));
            let arg = |key: &str| {
                args.get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };

            let (tool_label, display, output) = match call.name.as_str() {
                "install_skill" => {
                    let url = arg("url");
                    let label = format!("install_skill {url}");
                    let msg = if url.trim().is_empty() {
                        "URL skill kosong.".to_string()
                    } else {
                        match ui::ask_approval(
                            ev,
                            "Pasang skill",
                            &url,
                            &format!("Unduh SKILL.md / repo dan simpan ke {}.", skill_location_hint()),
                            &cancel,
                        ) {
                            Ok(true) => match skill::install_from_url(&url) {
                                Ok(m) => m,
                                Err(e) => format!("Gagal memasang skill: {e}"),
                            },
                            Ok(false) => "Ditolak pengguna.".to_string(),
                            Err(e) => format!("Gagal meminta persetujuan: {e}"),
                        }
                    };
                    (label, msg.clone(), msg)
                }
                "delete_skill" => {
                    let name = arg("name");
                    let label = format!("delete_skill {name}");
                    let msg = match ui::ask_approval(
                        ev,
                        "Hapus skill",
                        &name,
                        "Folder skill dan konfigurasinya akan dihapus.",
                        &cancel,
                    ) {
                        Ok(true) => match skill::delete(&name) {
                            Ok(()) => format!("Skill '{name}' dihapus."),
                            Err(e) => format!("Gagal menghapus skill: {e}"),
                        },
                        Ok(false) => "Ditolak pengguna.".to_string(),
                        Err(e) => format!("Gagal meminta persetujuan: {e}"),
                    };
                    (label, msg.clone(), msg)
                }
                "edit_skill" => {
                    let name = arg("name");
                    let content = arg("content");
                    let label = format!("edit_skill {name}");
                    let msg = match ui::ask_approval(
                        ev,
                        "Ubah skill",
                        &name,
                        &format!("Timpa SKILL.md. Isi baru:\n{}", tail_chars(content.clone(), 800)),
                        &cancel,
                    ) {
                        Ok(true) => match skill::edit(&name, &content) {
                            Ok(m) => format!("Skill diperbarui: {m}."),
                            Err(e) => format!("Gagal mengubah skill: {e}"),
                        },
                        Ok(false) => "Ditolak pengguna.".to_string(),
                        Err(e) => format!("Gagal meminta persetujuan: {e}"),
                    };
                    (label, msg.clone(), msg)
                }
                "set_skill_config" => {
                    let name = arg("name");
                    let key = arg("key");
                    let value = arg("value");
                    let label = format!("set_skill_config {name} {key}={value}");
                    let config_hint = paths::get()
                        .map(|p| p.skills_config_dir.join(&name).join("config.json").to_string_lossy().to_string())
                        .unwrap_or_else(|_| format!("{name}/config.json"));
                    let msg = match ui::ask_approval(
                        ev,
                        "Ubah konfigurasi skill",
                        &name,
                        &format!("{key} = {value} (disimpan di {config_hint})"),
                        &cancel,
                    ) {
                        Ok(true) => match skill::set_config(&name, &key, &value) {
                            Ok(v) => format!("Konfigurasi tersimpan: {v}."),
                            Err(e) => format!("Gagal menyimpan konfigurasi: {e}"),
                        },
                        Ok(false) => "Ditolak pengguna.".to_string(),
                        Err(e) => format!("Gagal meminta persetujuan: {e}"),
                    };
                    (label, msg.clone(), msg)
                }
                "ask_user" => {
                    let question = arg("question");
                    let options: Vec<String> = args
                        .get("options")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    let label = format!("ask_user: {question}");
                    let msg = if options.len() < 2 {
                        "Minimal dua pilihan jawaban diperlukan.".to_string()
                    } else {
                        let mut paths_in: Vec<String> = args
                            .get("files")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        let single = arg("file");
                        if !single.trim().is_empty() {
                            paths_in.push(single);
                        }
                        paths_in.truncate(5);
                        let files = paths_in
                            .iter()
                            .filter_map(|p| ui::file_preview(p))
                            .collect();
                        match ui::ask_question(ev, &question, &options, files, &cancel) {
                            Ok(Some(index)) => format!(
                                "Pengguna memilih: {}",
                                options.get(index).cloned().unwrap_or_default()
                            ),
                            Ok(None) => "Tidak ada jawaban dari pengguna.".to_string(),
                            Err(e) => format!("Gagal bertanya: {e}"),
                        }
                    };
                    (label, msg.clone(), msg)
                }
                _ => {
                    let command = arg("command");
                    if command.trim().is_empty() {
                        continue;
                    }
                    match shell.exec(&command, &cancel, &mut |chunk| ev.output(chunk)) {
                        Ok(outcome) => {
                            let full = outcome.report();
                            // Terminal boleh menampilkan lebih banyak; konteks
                            // model dibatasi supaya tidak cepat penuh.
                            (
                                command.clone(),
                                tail_chars(full.clone(), 20_000),
                                tail_chars(full, tool_output_chars),
                            )
                        }
                        Err(e) => {
                            let msg = format!("Gagal menjalankan perintah: {e}");
                            (command.clone(), msg.clone(), msg)
                        }
                    }
                }
            };
            ev.tool(&tool_label, &display);

            if p.kind == "anthropic" {
                msgs.push(ChatMsg {
                    role: "assistant".into(),
                    content: String::new(),
                    blocks: Some(json!([{
                        "type": "tool_use",
                        "id": call.id.clone(),
                        "name": call.name,
                        "input": args,
                    }])),
                    tool_calls: None,
                    tool_call_id: None,
                    id: None,
                    created_at: None,
                });
                msgs.push(ChatMsg {
                    role: "user".into(),
                    content: String::new(),
                    blocks: Some(json!([{
                        "type": "tool_result",
                        "tool_use_id": call.id,
                        "content": output,
                    }])),
                    tool_calls: None,
                    tool_call_id: None,
                    id: None,
                    created_at: None,
                });
            } else {
                let name = if call.name.is_empty() { "run_command" } else { &call.name };
                msgs.push(ChatMsg {
                    role: "assistant".into(),
                    content: String::new(),
                    tool_calls: Some(json!([{
                        "id": call.id.clone(),
                        "type": "function",
                        "function": { "name": name, "arguments": call.arguments },
                    }])),
                    tool_call_id: None,
                    blocks: None,
                    id: None,
                    created_at: None,
                });
                msgs.push(ChatMsg {
                    role: "tool".into(),
                    content: output,
                    tool_calls: None,
                    tool_call_id: Some(call.id),
                    blocks: None,
                    id: None,
                    created_at: None,
                });
            }
        }
    }

    if final_text.trim().is_empty() {
        let msg = if cancel.load(Ordering::SeqCst) {
            "Dibatalkan."
        } else {
            "Provider tidak mengirim jawaban."
        };
        ev.error(msg);
        return Err(msg.to_string());
    }

    // Statistik provider: jawaban dihitung 1 permintaan, plus tiap putaran
    // tool yang dijalankan.
    let _ = provider::record_usage(&p.id, total_tokens, 1 + rounds as i64);

    match session::add_message(&session_id, "assistant", &final_text, "[]") {
        Ok(m) => {
            ev.done(&final_text);
            Ok(m.id)
        }
        Err(e) => {
            let msg = format!("Jawaban diterima tetapi gagal disimpan: {e}");
            ev.error(&msg);
            Err(msg)
        }
    }
}

fn skill_location_hint() -> String {
    paths::get()
        .map(|p| p.skills_dir.to_string_lossy().to_string())
        .unwrap_or_else(|_| "folder skill".to_string())
}

/// Resolve provider + riwayat pesan dari permintaan UI.
fn prepare(req: &ChatRequest, _session_id: &str) -> Result<(AiProvider, Vec<ChatMsg>), String> {
    let p = if req.provider_id.trim().is_empty() {
        provider::default_provider()?
            .ok_or("Belum ada provider AI. Tambahkan di tab Provider.")?
    } else {
        provider::get(&req.provider_id)?
    };
    let msgs: Vec<ChatMsg> = serde_json::from_str::<Vec<ChatMsg>>(&req.messages_json)
        .map_err(|e| format!("Riwayat pesan tidak valid: {e}"))?;
    Ok((p, msgs))
}

/// System prompt bawaan untuk platform aktif (dipakai UI "reset ke bawaan").
pub fn default_system_prompt() -> String {
    prompt::active_system_prompt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_chars_keeps_tail_and_marks_truncation() {
        let s = "abcdefghij".to_string();
        let t = tail_chars(s, 4);
        assert!(t.ends_with("ghij"));
        assert!(t.contains("terpotong"));
        assert_eq!(tail_chars("abc".to_string(), 10), "abc");
    }

    #[test]
    fn cancel_flags_are_per_session() {
        let a = new_chat_cancel("s-a");
        let b = new_chat_cancel("s-b");
        request_cancel("s-a");
        assert!(a.load(Ordering::SeqCst));
        assert!(!b.load(Ordering::SeqCst));
        remove_chat_cancel("s-a", &a);
        // Flag yang sudah dilepas tidak lagi bisa dibatalkan dari luar.
        request_cancel("s-a");
        assert!(a.load(Ordering::SeqCst));
    }

    #[test]
    fn cancel_guard_removes_only_its_own_flag() {
        let old = new_chat_cancel("s-x");
        remove_chat_cancel("s-x", &old);
        let new = new_chat_cancel("s-x");
        // Guard run lama tidak boleh menghapus flag run baru.
        remove_chat_cancel("s-x", &old);
        request_cancel("s-x");
        assert!(new.load(Ordering::SeqCst));
    }
}
