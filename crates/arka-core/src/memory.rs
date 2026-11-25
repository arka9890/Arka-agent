//! Ringkasan konteks (compaction) supaya percakapan panjang tidak kehilangan
//! awal obrolan ketika model/router punya context window kecil.
//!
//! Kenapa perlu: Arka Agent mengirim seluruh riwayat sesi tiap request. Kalau
//! router (mis. combo round-robin OmniRoute) memilih model 8k/32k, provider
//! memotong bagian awal percakapan tanpa memberi tahu aplikasi. Di sini pesan
//! lama diringkas lebih dulu menjadi catatan konteks yang padat, lalu hanya
//! ringkasan + beberapa pesan terakhir yang dikirim.
//!
//! Ringkasan disimpan di kolom `sessions.summary` / `summary_upto` supaya
//! tidak perlu dihitung ulang setiap request.

use crate::ai::{self, ChatMsg};
use crate::provider::Provider;
use crate::session;
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};

/// Jumlah pesan terakhir yang selalu dikirim utuh.
const KEEP_RECENT: usize = 6;

/// Target ukuran transkrip per panggilan perangkum. Dibuat kecil supaya
/// request peringkasan sendiri tetap muat bahkan di model 8k.
const SUMMARY_CHUNK_TOKENS: usize = 3_500;

/// Batas panjang ringkasan yang disimpan (~1.750 token).
const MAX_SUMMARY_CHARS: usize = 7_000;

/// Maksimal pesan yang diringkas per satu kali `prepare`, supaya chat tidak
/// tertahan terlalu lama kalau riwayatnya sudah sangat panjang.
const MAX_BATCH_MESSAGES: usize = 24;

/// Perkiraan kasar jumlah token.
///
/// Tidak memakai tokenizer asli (ukurannya besar dan berbeda per model); ini
/// hanya untuk memutuskan kapan harus meringkas. Sengaja dibulatkan ke atas
/// agar ringkasan dibuat lebih awal, bukan setelah provider memotong.
pub fn estimate_tokens(text: &str) -> usize {
    let mut latin = 0usize;
    let mut wide = 0usize;
    for ch in text.chars() {
        let cp = ch as u32;
        let is_wide = (0x2E80..=0x9FFF).contains(&cp)      // CJK
            || (0xAC00..=0xD7AF).contains(&cp)             // Hangul
            || (0xF900..=0xFAFF).contains(&cp)
            || (0xFF00..=0xFFEF).contains(&cp)
            || (0x3000..=0x303F).contains(&cp); // tanda baca CJK
        if is_wide {
            wide += 1;
        } else {
            latin += 1;
        }
    }
    // Teks Latin/kode ±4 karakter per token; CJK ±1,5 karakter per token.
    latin / 4 + wide * 2 / 3 + 1
}

fn msg_tokens(m: &ChatMsg) -> usize {
    let mut n = estimate_tokens(&m.content);
    if let Some(tc) = &m.tool_calls {
        n += estimate_tokens(&tc.to_string());
    }
    if let Some(b) = &m.blocks {
        n += estimate_tokens(&b.to_string());
    }
    // Overhead role/format tiap pesan.
    n + 4
}

/// Perkiraan total token satu daftar pesan.
pub fn total_tokens(msgs: &[ChatMsg]) -> usize {
    msgs.iter().map(msg_tokens).sum()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("\n… (dipotong)");
    out
}

fn render(m: &ChatMsg) -> String {
    let role = match m.role.as_str() {
        "user" => "Pengguna",
        "assistant" => "Asisten",
        "tool" => "Hasil tool",
        "system" => "Sistem",
        other => other,
    };
    let mut body = m.content.trim().to_string();
    if body.is_empty() {
        if let Some(b) = &m.blocks {
            body = b.to_string();
        } else if let Some(tc) = &m.tool_calls {
            body = tc.to_string();
        }
    }
    // Satu output tool bisa puluhan ribu karakter; cukup bagian awal untuk
    // perangkum (detail penting biasanya ada di awal + akhir).
    format!("[{role}]\n{}\n", truncate_chars(&body, 4_000))
}

const SUMMARY_SYSTEM: &str = r#"Kamu adalah perangkum konteks percakapan untuk asisten AI Arka Agent.
Tugasmu menjaga agar asisten berikutnya tidak kehilangan informasi penting saat riwayat lama dipadatkan.

Tulis ringkasan padat, faktual, dan terstruktur dalam bahasa yang sama dengan percakapan.
Pertahankan:
- tujuan dan permintaan pengguna
- keputusan atau preferensi yang sudah disepakati
- nama file, path, perintah, dan hasil penting
- masalah yang sudah dicoba beserta hasilnya
- pekerjaan yang belum selesai dan langkah berikutnya

Jangan mengarang informasi baru. Jangan menyalin seluruh percakapan; ringkas saja.
Balas HANYA dengan ringkasan berupa poin-poin, maksimal sekitar 1000 kata."#;

fn summary_prompt(previous: &str, transcript: &str) -> String {
    format!(
        "Ringkasan yang sudah ada (bisa kosong):\n{}\n\n\
         Percakapan baru yang perlu digabung ke ringkasan:\n{}\n\n\
         Tulis satu ringkasan gabungan yang ringkas. Jangan kehilangan detail \
         teknis yang masih relevan untuk melanjutkan pekerjaan.",
        if previous.trim().is_empty() {
            "(belum ada)"
        } else {
            previous.trim()
        },
        transcript,
    )
}

fn summarize_once(
    p: &Provider,
    model: &str,
    previous: &str,
    transcript: &str,
) -> Result<String, String> {
    let msgs = vec![
        ChatMsg::plain("system", SUMMARY_SYSTEM),
        ChatMsg::plain("user", &summary_prompt(previous, transcript)),
    ];
    let text = ai::complete(p, model, &msgs)?;
    Ok(truncate_chars(text.trim(), MAX_SUMMARY_CHARS))
}

/// Ringkas [msgs] per potongan kecil, lalu gabungkan dengan [previous].
fn summarize_batch(
    p: &Provider,
    model: &str,
    previous: &str,
    msgs: &[ChatMsg],
) -> Result<String, String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for m in msgs {
        let line = render(m);
        if !current.is_empty()
            && estimate_tokens(&current) + estimate_tokens(&line) > SUMMARY_CHUNK_TOKENS
        {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(&line);
    }
    if !current.trim().is_empty() {
        chunks.push(current);
    }

    let mut summary = previous.trim().to_string();
    for chunk in chunks {
        summary = summarize_once(p, model, &summary, &chunk)?;
    }
    Ok(truncate_chars(&summary, MAX_SUMMARY_CHARS))
}

fn build_system(base: &str, summary: &str) -> String {
    if summary.trim().is_empty() {
        base.to_string()
    } else {
        format!(
            "{base}\n\n## Ringkasan percakapan sebelumnya\n{summary}\n\n\
             Gunakan ringkasan di atas sebagai konteks percakapan. Jangan \
             mengulanginya kecuali diminta."
        )
    }
}

fn assemble(base: &str, summary: &str, rest: &[ChatMsg], truncated: bool) -> Vec<ChatMsg> {
    let mut sys = build_system(base, summary);
    if truncated {
        sys.push_str(
            "\n\n_(Sebagian pesan lama dipotong karena batas konteks. \
             Naikkan batas di Setelan → Agent kalau perlu.)_",
        );
    }
    let mut out = Vec::with_capacity(rest.len() + 1);
    out.push(ChatMsg::plain("system", &sys));
    out.extend(rest.iter().cloned());
    out
}

/// Siapkan pesan final untuk provider.
///
/// `msgs` harus sudah berisi system prompt. Fungsi ini:
/// 1. menyisipkan ringkasan tersimpan ke system prompt,
/// 2. membuang pesan yang sudah tercakup ringkasan (`summary_upto`),
/// 3. meringkas pesan lama kalau total token masih melebihi [budget].
///
/// Kalau peringkasan gagal (mis. jaringan), pesan tidak dibuang; chat tetap
/// dikirim apa adanya supaya pengguna tidak kehilangan balasan.
pub fn prepare(
    p: &Provider,
    model: &str,
    session_id: &str,
    msgs: Vec<ChatMsg>,
    budget: usize,
    cancel: &AtomicBool,
) -> Vec<ChatMsg> {
    let Some(system_idx) = msgs.iter().position(|m| m.role == "system") else {
        return msgs;
    };
    let mut msgs = msgs;
    let base_system = msgs.remove(system_idx).content;

    let (mut summary, mut summary_upto) = session::memory(session_id).unwrap_or_default();
    let mut rest: Vec<ChatMsg> = msgs
        .into_iter()
        .filter(|m| match m.created_at {
            // Pesan tanpa metadata (mis. hasil tool di agent loop) jangan
            // dibuang; hanya pesan yang benar-benar sudah diringkas.
            Some(t) => t > summary_upto,
            None => true,
        })
        .collect();

    let mut out = assemble(&base_system, &summary, &rest, false);
    let mut rounds = 0usize;
    while total_tokens(&out) > budget && rest.len() > KEEP_RECENT && rounds < 6 {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        rounds += 1;
        let take = (rest.len() - KEEP_RECENT).min(MAX_BATCH_MESSAGES);
        if take == 0 {
            break;
        }
        let batch: Vec<ChatMsg> = rest.drain(0..take).collect();
        let new_upto = batch
            .iter()
            .filter_map(|m| m.created_at)
            .max()
            .unwrap_or(summary_upto);

        match summarize_batch(p, model, &summary, &batch) {
            Ok(s) => {
                summary = s;
                summary_upto = new_upto;
                let _ = session::set_memory(session_id, &summary, summary_upto);
            }
            Err(e) => {
                // Jangan tandai batch ini sudah diringkas: kalau nanti jaringan
                // membaik, pesan yang sama masih bisa diringkas.
                eprintln!("ringkasan konteks gagal: {e}");
                for m in batch.into_iter().rev() {
                    rest.insert(0, m);
                }
                break;
            }
        }
        out = assemble(&base_system, &summary, &rest, false);
    }

    // Jaring pengaman: kalau masih kebesaran (ringkasan gagal atau budget
    // sangat kecil), buang pesan tertua dan beri tahu model bahwa ada yang
    // dipotong. Minimal satu pesan terakhir tetap dikirim.
    let mut truncated = false;
    while total_tokens(&out) > budget && rest.len() > 1 {
        rest.remove(0);
        truncated = true;
        out = assemble(&base_system, &summary, &rest, truncated);
    }
    if total_tokens(&out) > budget && !summary.trim().is_empty() {
        summary = truncate_chars(&summary, MAX_SUMMARY_CHARS / 2);
        out = assemble(&base_system, &summary, &rest, truncated);
    }
    out
}

fn has_tool_result(m: &ChatMsg) -> bool {
    m.blocks
        .as_ref()
        .and_then(|b| b.as_array())
        .map(|arr| {
            arr.iter()
                .any(|x| x.get("type").and_then(|v| v.as_str()) == Some("tool_result"))
        })
        .unwrap_or(false)
}

/// Kalau konteks membengkak karena output tool, ganti isi output paling lama
/// dengan catatan singkat. Struktur `tool_call_id` / blok `tool_result`
/// dipertahankan supaya protokol provider tetap valid.
///
/// Output tool terakhir tidak dipotong karena biasanya itu yang sedang
/// dianalisis model.
pub fn fit_tool_outputs(msgs: &mut [ChatMsg], budget: usize) {
    if total_tokens(msgs) <= budget {
        return;
    }
    // Pertahankan dua output tool terakhir: biasanya itu yang sedang
    // dianalisis model. Sisanya dipotong dari yang paling lama.
    let tool_indices: Vec<usize> = msgs
        .iter()
        .enumerate()
        .filter(|(_, m)| m.role == "tool" || has_tool_result(m))
        .map(|(i, _)| i)
        .collect();
    let keep_from = tool_indices.len().saturating_sub(2);

    for i in 0..msgs.len() {
        if total_tokens(msgs) <= budget {
            break;
        }
        if let Some(pos) = tool_indices.iter().position(|&idx| idx == i) {
            if pos >= keep_from {
                continue;
            }
        } else {
            continue;
        }
        if msgs[i].role == "tool" {
            let n = msgs[i].content.chars().count();
            if n > 240 {
                msgs[i].content = format!(
                    "(output perintah lama dipotong untuk menghemat konteks: {n} karakter)"
                );
            }
        } else if has_tool_result(&msgs[i]) {
            if let Some(blocks) = &mut msgs[i].blocks {
                if let Some(arr) = blocks.as_array_mut() {
                    for b in arr {
                        if b.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                            b["content"] =
                                json!("(output perintah lama dipotong untuk menghemat konteks)");
                        }
                    }
                }
            }
        }
    }
}

fn from_session_message(m: &session::Message) -> ChatMsg {
    ChatMsg {
        role: m.role.clone(),
        content: m.content.clone(),
        tool_calls: None,
        tool_call_id: None,
        blocks: None,
        id: Some(m.id.clone()),
        created_at: Some(m.created_at),
    }
}

/// Ringkas riwayat sesi sekarang juga (dipanggil dari menu sesi di UI).
/// Menyisakan [KEEP_RECENT] pesan terakhir tetap utuh.
pub fn compact_now(p: &Provider, model: &str, session_id: &str) -> Result<String, String> {
    let messages = session::messages(session_id)?;
    if messages.len() <= KEEP_RECENT {
        return Err("Percakapan masih pendek; belum perlu diringkas.".into());
    }
    let (mut summary, upto) = session::memory(session_id)?;
    let pending: Vec<ChatMsg> = messages
        .iter()
        .filter(|m| m.created_at > upto)
        .map(from_session_message)
        .collect();
    let take = pending.len().saturating_sub(KEEP_RECENT);
    if take == 0 {
        return Err("Tidak ada pesan baru yang perlu diringkas.".into());
    }
    let batch = &pending[..take];
    summary = summarize_batch(p, model, &summary, batch)?;
    let new_upto = batch
        .iter()
        .filter_map(|m| m.created_at)
        .max()
        .unwrap_or(upto);
    session::set_memory(session_id, &summary, new_upto)?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_is_positive_and_scales() {
        assert!(estimate_tokens("") >= 1);
        let short = estimate_tokens("halo");
        let long = estimate_tokens(&"halo ".repeat(100));
        assert!(long > short);
    }

    #[test]
    fn tool_outputs_are_trimmed_but_last_two_are_kept() {
        let tool = |id: &str| ChatMsg {
            role: "tool".into(),
            content: "x".repeat(20_000),
            tool_calls: None,
            tool_call_id: Some(id.into()),
            blocks: None,
            id: None,
            created_at: None,
        };
        let mut msgs = vec![
            ChatMsg::plain("system", "sys"),
            ChatMsg::plain("user", "jalankan sesuatu"),
            tool("a"),
            tool("b"),
            tool("c"),
        ];
        fit_tool_outputs(&mut msgs, 2_000);
        assert!(msgs[2]
            .content
            .starts_with("(output perintah lama dipotong"));
        // Dua output terakhir tetap utuh supaya model bisa menganalisisnya.
        assert_eq!(msgs[3].content.len(), 20_000);
        assert_eq!(msgs[4].content.len(), 20_000);
    }

    #[test]
    fn system_prompt_includes_summary() {
        let s = build_system("dasar", "pengguna ingin build APK");
        assert!(s.contains("dasar"));
        assert!(s.contains("Ringkasan percakapan sebelumnya"));
        assert!(s.contains("build APK"));
    }
}
