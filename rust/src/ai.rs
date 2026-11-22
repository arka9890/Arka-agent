//! Klien AI: streaming chat ke provider `openai`-compatible dan `anthropic`.
//!
//! Streaming dibuat "pull" ([ChatStream::next]) supaya pemanggil JNI bisa
//! meneruskan tiap potongan ke UI sekaligus memeriksa tombol Stop tanpa
//! memaksa satu sama lain meminjam `JNIEnv`.

use crate::provider::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
    /// Pesan assistant yang meminta tool call (OpenAI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    /// Id tool call untuk pesan hasil (OpenAI).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Isi pesan berupa blok (Anthropic: tool_use / tool_result).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Value>,
}

/// Satu permintaan eksekusi perintah dari model.
#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Konteks yang disisipkan ke setiap sesi chat (lihat `chatStream`).
///
/// Tanpa ini model mengira aplikasi hanya chatbot biasa dan menjawab "aku
/// tidak punya akses ke terminal/Ubuntumu" ketika pengguna meminta pengecekan.
/// Prompt ini menjelaskan peran ganda: model adalah "otak" agent, sedangkan
/// eksekusi perintah dilakukan lewat tab Terminal di perangkat pengguna.
pub const SYSTEM_PROMPT: &str = r#"Kamu adalah Arka Agent, asisten AI di aplikasi Android Arka Agent.

Aplikasi ini memiliki workspace Ubuntu 24.04 asli yang berjalan di perangkat pengguna melalui proot. Folder kerjanya adalah /root/storage.

KAMU PUNYA TOOL bernama `run_command`. Tool ini menjalankan perintah bash di Ubuntu pengguna dan mengembalikan stdout/stderr-nya.

Aturan penggunaan tool:
- Jika pengguna meminta kamu mengecek, menjalankan, menguji, atau memverifikasi sesuatu di Ubuntu / terminal / file, LANGSUNG panggil `run_command` dengan perintah yang tepat. JANGAN menjawab bahwa kamu tidak bisa mengakses terminal, jangan mengaku hanya berjalan di server AI, dan jangan menyuruh pengguna menjalankan sendiri.
- Gunakan perintah non-interaktif. Contoh: `cat /etc/os-release`, `uname -m`, `python3 --version`, `git --version`, `ls -la /root/storage`, `which git curl wget python3`.
- Setelah tool mengembalikan hasil, analisis output itu dan jawab dalam bahasa pengguna. Kalau perintah gagal, coba perbaiki perintahnya (maksimal beberapa kali).
- Untuk pertanyaan umum yang tidak perlu dieksekusi, jawab biasa tanpa tool.
- Jangan mengaku sedang berjalan di dalam Ubuntu; katakan kamu menjalankan perintah di Ubuntu milik pengguna lewat tool Arka Agent.
- Jawab dalam bahasa pengguna (default Bahasa Indonesia). Jika ditanya siapa kamu, jawab: "Aku Arka Agent, asisten AI di aplikasi Arka Agent yang bisa menjalankan perintah di workspace Ubuntu-mu."#;

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        // Bukan timeout total: jawaban panjang bisa berjalan lama.
        .timeout(Duration::from_secs(600))
        .user_agent("ArkaAgent/1.0")
        .build()
        .map_err(|e| format!("Klien HTTP gagal dibuat: {e}"))
}

/// Susun URL endpoint dari Base URL provider.
///
/// Base URL boleh ditulis dengan atau tanpa `/v1` (kebanyakan orang menyalin
/// dari dokumentasi provider yang menyertakannya).
///
/// Perhatikan: [path] adalah path **relatif** setelah `/v1`, dan tiap protokol
/// memakai nama berbeda — OpenAI-compatible memakai `chat/completions`,
/// Anthropic memakai `messages`. Jangan samakan keduanya.
pub fn endpoint(p: &Provider, path: &str) -> String {
    let base = p.base_url.trim().trim_end_matches('/');
    // Sebagian gateway sudah menyertakan versi API di base URL, mis.
    //   OpenAI    https://api.openai.com/v1
    //   Gemini    https://generativelanguage.googleapis.com/v1beta/openai
    //   OpenRouter https://openrouter.ai/api/v1
    // Menambahkan "/v1" lagi ke URL seperti itu menghasilkan 404, jadi hanya
    // tambahkan kalau base belum punya segmen versi.
    let has_version = base.ends_with("/v1")
        || base.ends_with("/v2")
        || base.contains("/v1beta")
        || base.ends_with("/openai");
    if has_version {
        format!("{base}/{path}")
    } else {
        format!("{base}/v1/{path}")
    }
}

fn describe_http_error(status: u16, body: &str) -> String {
    let snippet: String = body.chars().take(400).collect();
    match status {
        401 | 403 => format!("API key ditolak provider (HTTP {status}). {snippet}"),
        404 => format!(
            "Endpoint atau model tidak ditemukan (HTTP 404). Periksa Base URL dan nama model. {snippet}"
        ),
        429 => format!("Kuota/rate limit provider habis (HTTP 429). {snippet}"),
        _ => format!("Provider mengembalikan HTTP {status}. {snippet}"),
    }
}

fn auth_headers(rb: reqwest::blocking::RequestBuilder, p: &Provider) -> reqwest::blocking::RequestBuilder {
    if p.kind == "anthropic" {
        rb.header("x-api-key", p.api_key.trim())
            .header("anthropic-version", "2023-06-01")
    } else {
        rb.bearer_auth(p.api_key.trim())
    }
}

/// Definisi tool yang dikirim ke provider. Satu-satunya tool: jalankan
/// perintah di workspace Ubuntu pengguna.
fn tools_schema() -> Value {
    json!([{
        "type": "function",
        "function": {
            "name": "run_command",
            "description": "Jalankan perintah bash non-interaktif di workspace Ubuntu 24.04 di perangkat pengguna. Mengembalikan stdout/stderr. Pakai tool ini setiap kali perlu mengecek atau menjalankan sesuatu di Ubuntu/terminal/file.",
            "parameters": {
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "Perintah shell yang dijalankan di /root/storage. Contoh: 'cat /etc/os-release' atau 'python3 --version'.",
                    }
                },
                "required": ["command"],
            }
        }
    }])
}

fn anthropic_tools_schema() -> Value {
    json!([{
        "name": "run_command",
        "description": "Jalankan perintah bash non-interaktif di workspace Ubuntu 24.04 di perangkat pengguna. Mengembalikan stdout/stderr. Pakai setiap kali perlu mengecek atau menjalankan sesuatu di Ubuntu/terminal/file.",
        "input_schema": {
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Perintah shell yang dijalankan di /root/storage.",
                }
            },
            "required": ["command"],
        }
    }])
}

fn message_value(anthropic: bool, m: &ChatMsg) -> Value {
    let content = if let Some(blocks) = &m.blocks {
        blocks.clone()
    } else if m.tool_calls.is_some() && m.content.is_empty() {
        Value::Null
    } else {
        json!(m.content)
    };
    let mut v = json!({"role": m.role, "content": content});
    if !anthropic {
        if let Some(tc) = &m.tool_calls {
            v["tool_calls"] = tc.clone();
        }
        if let Some(id) = &m.tool_call_id {
            v["tool_call_id"] = json!(id);
        }
    }
    v
}

fn request_body(
    anthropic: bool,
    model: &str,
    messages: &[ChatMsg],
    with_tools: bool,
    include_usage: bool,
) -> Value {
    let system = messages.iter().find(|m| m.role == "system").map(|m| m.content.clone());
    let msgs: Vec<Value> = messages.iter()
        .filter(|m| m.role != "system")
        .map(|m| message_value(anthropic, m))
        .collect();
    if anthropic {
        let mut v = json!({
            "model": model,
            "max_tokens": 4096,
            "stream": true,
            "system": system,
            "messages": msgs,
        });
        if with_tools {
            v["tools"] = anthropic_tools_schema();
        }
        v
    } else {
        let mut v = json!({"model": model, "stream": true, "messages": msgs});
        // Minta laporan token di chunk terakhir supaya statistik provider
        // bisa diisi. Endpoint lama yang tidak kenal `stream_options` akan
        // menolak request — karena itu `include_usage` dicoba terpisah
        // (lihat [ChatStream::open]).
        if include_usage {
            v["stream_options"] = json!({"include_usage": true});
        }
        if with_tools {
            v["tools"] = tools_schema();
            v["tool_choice"] = json!("auto");
        }
        v
    }
}

/// Stream jawaban model. Pakai [ChatStream::next] sampai `None`, lalu
/// [ChatStream::take_tool_calls] untuk melihat permintaan tool (kalau ada).
pub struct ChatStream {
    reader: BufReader<reqwest::blocking::Response>,
    anthropic: bool,
    finished: bool,
    calls: Vec<ToolCall>,
    /// Total token yang dilaporkan provider (kalau endpoint mengirim `usage`).
    usage_tokens: i64,
}

impl ChatStream {
    /// Kirim permintaan dan kembalikan stream yang siap dibaca.
    ///
    /// Provider yang tidak mendukung function calling ditanya ulang tanpa
    /// tools supaya chat biasa tetap berfungsi (fallback).
    pub fn open(provider: &Provider, model: &str, messages: &[ChatMsg]) -> Result<Self, String> {
        if provider.api_key.trim().is_empty() {
            return Err(format!(
                "API key untuk provider {} belum diisi. Buka tab Provider untuk mengisinya.",
                provider.name
            ));
        }
        let anthropic = provider.kind == "anthropic";
        // Path berbeda per protokol (lihat endpoint()).
        let url = if anthropic {
            endpoint(provider, "messages")
        } else {
            endpoint(provider, "chat/completions")
        };
        let mut last_error = "Gagal menghubungi provider".to_string();
        // Coba 4 kombinasi: (tools?, usage?). Provider yang menolak
        // `stream_options` akan bilang 400 — lalu dicoba tanpa usage supaya
        // chat tetap jalan (statistik token hanya kosong di kasus itu).
        for (with_tools, include_usage) in
            [(true, true), (true, false), (false, false), (false, true)]
        {
            let body = request_body(anthropic, model, messages, with_tools, include_usage);
            let resp = auth_headers(client()?.post(&url).json(&body), provider)
                .send()
                .map_err(|e| format!("Gagal menghubungi provider: {e}"))?;
            if resp.status().is_success() {
                return Ok(Self {
                    reader: BufReader::new(resp),
                    anthropic,
                    finished: false,
                    calls: Vec::new(),
                    usage_tokens: 0,
                });
            }
            let status = resp.status().as_u16();
            let text = resp.text().unwrap_or_default();
            last_error = describe_http_error(status, &text);
            // 400/422: biasanya "tools tidak didukung" atau
            // "stream_options tidak dikenal" — coba variasi berikutnya.
            if (400..=422).contains(&status) {
                continue;
            }
            return Err(last_error);
        }
        Err(last_error)
    }

    /// Ambil tool call yang terkumpul (dipanggil setelah stream habis).
    pub fn take_tool_calls(&mut self) -> Vec<ToolCall> {
        std::mem::take(&mut self.calls)
    }

    /// Total token yang dilaporkan provider untuk permintaan ini.
    /// 0 kalau provider tidak mengirim metadata `usage`.
    pub fn usage_tokens(&self) -> i64 {
        self.usage_tokens
    }

    /// Kumpulkan metadata `usage` dari chunk SSE.
    ///
    /// OpenAI mengirim `usage` di chunk terakhir kalau request memakai
    /// `stream_options.include_usage`; beberapa gateway mengirimnya di chunk
    /// pertama. Anthropic mengirim `input_tokens` di `message_start` dan
    /// `output_tokens` di `message_delta`.
    fn collect_usage(&mut self, json: &Value) {
        let mut add = 0i64;
        if let Some(total) = json.pointer("/usage/total_tokens").and_then(|v| v.as_i64()) {
            add = add.max(total);
        }
        let prompt = json.pointer("/usage/prompt_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        let completion = json.pointer("/usage/completion_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        if add == 0 && (prompt > 0 || completion > 0) {
            add = prompt + completion;
        }
        // Anthropic: message_start (input) + message_delta (output)
        let a_in = json.pointer("/message/usage/input_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        let a_out = json.pointer("/usage/output_tokens").and_then(|v| v.as_i64()).unwrap_or(0);
        if add == 0 && (a_in > 0 || a_out > 0) {
            add = a_in + a_out;
        }
        if add > 0 {
            // Jangan menimpa nilai yang lebih besar (beberapa provider mengirim
            // usage parsial di beberapa chunk).
            self.usage_tokens = self.usage_tokens.max(add);
        }
    }

    /// Potongan teks berikutnya. `None` = stream selesai.
    pub fn next(&mut self) -> Option<Result<String, String>> {
        if self.finished {
            return None;
        }
        let mut line = String::new();
        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    self.finished = true;
                    return None;
                }
                Ok(_) => {}
                Err(e) => {
                    self.finished = true;
                    return Some(Err(format!("Koneksi ke provider terputus: {e}")));
                }
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            let Some(payload) = trimmed.strip_prefix("data:") else { continue };
            let payload = payload.trim();
            if payload == "[DONE]" {
                self.finished = true;
                return None;
            }
            if payload.is_empty() {
                continue;
            }
            let Ok(json) = serde_json::from_str::<Value>(payload) else { continue };
            if let Some(err) = json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                self.finished = true;
                return Some(Err(format!("Provider menolak permintaan: {err}")));
            }
            collect_tool_calls(&mut self.calls, self.anthropic, &json);
            self.collect_usage(&json);
            let delta = if self.anthropic {
                json.pointer("/delta/text").and_then(|t| t.as_str())
            } else {
                json.pointer("/choices/0/delta/content").and_then(|t| t.as_str())
            };
            if let Some(text) = delta {
                if !text.is_empty() {
                    return Some(Ok(text.to_string()));
                }
            }
        }
    }
}

/// Kumpulkan potongan tool call dari SSE OpenAI/Anthropic.
fn collect_tool_calls(calls: &mut Vec<ToolCall>, anthropic: bool, json: &Value) {
        if anthropic {
            match json.get("type").and_then(|v| v.as_str()).unwrap_or("") {
                "content_block_start" => {
                    let Some(cb) = json.get("content_block") else { return };
                    if cb.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
                        return;
                    }
                    let id = cb.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    let name = cb.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                    // Anthropic mengirim `input: {}` di start lalu partial_json
                    // di delta. Jangan simpan `{}` itu atau argumennya rusak.
                    let input = cb.get("input")
                        .filter(|v| *v != &json!({}))
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    calls.push(ToolCall { id, name, arguments: input });
                }
                "content_block_delta" => {
                    if json.pointer("/delta/type").and_then(|v| v.as_str())
                        != Some("input_json_delta")
                    {
                        return;
                    }
                    if let (Some(part), Some(last)) = (
                        json.pointer("/delta/partial_json").and_then(|v| v.as_str()),
                        calls.last_mut(),
                    ) {
                        last.arguments.push_str(part);
                    }
                }
                _ => {}
            }
            return;
        }

        let Some(list) = json.pointer("/choices/0/delta/tool_calls").and_then(|v| v.as_array()) else {
            return;
        };
        for c in list {
            let index = c.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            while calls.len() <= index {
                calls.push(ToolCall {
                    id: String::new(),
                    name: String::new(),
                    arguments: String::new(),
                });
            }
            if let Some(id) = c.get("id").and_then(|v| v.as_str()) {
                if !id.is_empty() {
                    calls[index].id = id.to_string();
                }
            }
            if let Some(func) = c.get("function") {
                if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                    if !name.is_empty() {
                        calls[index].name = name.to_string();
                    }
                }
                if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                    calls[index].arguments.push_str(args);
                }
            }
        }
}

#[derive(Deserialize)]
struct ModelsResp {
    #[serde(default)]
    data: Vec<ModelEntry>,
}

#[derive(Deserialize)]
struct ModelEntry {
    #[serde(default)]
    id: String,
}

/// Ambil daftar model dari endpoint `/models` provider. Gagal kalau endpoint
/// tidak ada atau API key ditolak (dipakai tombol "Tes koneksi").
fn fetch_models(p: &Provider) -> Result<Vec<String>, String> {
    if p.api_key.trim().is_empty() {
        return Err("API key belum diisi.".into());
    }
    let url = endpoint(p, "models");
    let rb = auth_headers(client()?.get(&url), p);
    let resp = rb.send().map_err(|e| format!("Gagal menghubungi provider: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Endpoint model mengembalikan HTTP {}", resp.status().as_u16()));
    }
    let parsed: ModelsResp = resp.json().map_err(|e| format!("Balasan model tidak dikenali: {e}"))?;
    Ok(parsed.data.into_iter().map(|m| m.id).filter(|s| !s.is_empty()).collect())
}

/// Daftar model untuk dropdown chat/provider — **tanpa network**.
///
/// Pemanggilan lama selalu `GET /models` tiap kali dropdown dibuka sehingga
/// UI ngelag. Sekarang yang dipakai adalah daftar yang sudah tersimpan di DB
/// (diisi sekali lewat `refresh_models` / tombol manual).
pub fn list_models(p: &Provider) -> Result<Vec<String>, String> {
    let mut ids: Vec<String> = p.models.iter()
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if !p.model.trim().is_empty() {
        ids.push(p.model.trim().to_string());
    }
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Cek daftar model dari server (manual, lewat tombol refresh di UI).
/// Hasilnya digabung dengan model yang sudah dipilih pengguna.
pub fn refresh_models(p: &Provider) -> Result<Vec<String>, String> {
    let mut ids = list_models(p)?;
    ids.extend(fetch_models(p)?);
    ids.sort();
    ids.dedup();
    Ok(ids)
}

/// Tes koneksi: benar-benar memanggil API supaya hasilnya bukan sekadar
/// "URL valid". Memakai `GET /models` karena murah dan tidak membuat pesan.
pub fn test_connection(p: &Provider) -> Result<String, String> {
    if p.api_key.trim().is_empty() {
        return Err("API key belum diisi.".into());
    }
    let models = fetch_models(p)?;
    if models.is_empty() {
        return Ok("Terhubung, tetapi provider tidak mengirim daftar model.".into());
    }
    Ok(format!("Terhubung. {} model tersedia.", models.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_tool_call_fragments_are_merged() {
        let mut calls = Vec::new();
        collect_tool_calls(&mut calls, false, &json!({
            "choices": [{"delta": {"tool_calls": [{
                "index": 0, "id": "call_1", "type": "function",
                "function": {"name": "run_command", "arguments": "{\"comm"}
            }]}}]
        }));
        collect_tool_calls(&mut calls, false, &json!({
            "choices": [{"delta": {"tool_calls": [{
                "index": 0, "function": {"arguments": "and\":\"uname -a\"}"}
            }]}}]
        }));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "run_command");
        assert_eq!(calls[0].arguments, r#"{"command":"uname -a"}"#);
    }

    #[test]
    fn anthropic_tool_call_fragments_are_merged() {
        let mut calls = Vec::new();
        collect_tool_calls(&mut calls, true, &json!({
            "type": "content_block_start",
            "content_block": {"type": "tool_use", "id": "toolu_1",
                              "name": "run_command", "input": {}}
        }));
        collect_tool_calls(&mut calls, true, &json!({
            "type": "content_block_delta",
            "delta": {"type": "input_json_delta",
                      "partial_json": "{\"command\":\"cat /etc/os-release\"}"}
        }));
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "toolu_1");
        assert_eq!(calls[0].arguments, r#"{"command":"cat /etc/os-release"}"#);
    }
}
