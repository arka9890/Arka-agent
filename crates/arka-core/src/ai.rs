//! Klien AI: streaming chat ke provider `openai`-compatible dan `anthropic`.
//!
//! Streaming dibuat "pull" ([ChatStream::next]) supaya pemanggil JNI bisa
//! meneruskan tiap potongan ke UI sekaligus memeriksa tombol Stop tanpa
//! memaksa satu sama lain meminjam `JNIEnv`.

use crate::paths::Platform;
use crate::provider::Provider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader};
use std::sync::{Mutex, OnceLock};
use std::collections::HashMap;
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
    /// Metadata dari aplikasi; tidak pernah dikirim ke provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Waktu pesan dibuat (ms epoch), dipakai untuk menandai batas ringkasan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

impl ChatMsg {
    /// Pesan teks biasa tanpa tool call.
    pub fn plain(role: &str, content: &str) -> Self {
        Self {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
            blocks: None,
            id: None,
            created_at: None,
        }
    }
}

/// Satu permintaan eksekusi perintah dari model.
#[derive(Clone, Debug)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

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
    platform: Platform,
) -> Value {
    let system = messages.iter().find(|m| m.role == "system").map(|m| m.content.clone());
    let rest: Vec<Value> = messages.iter()
        .filter(|m| m.role != "system")
        .map(|m| message_value(anthropic, m))
        .collect();
    if anthropic {
        let mut v = json!({
            "model": model,
            "max_tokens": 4096,
            "stream": true,
            "system": system,
            "messages": rest,
        });
        if with_tools {
            v["tools"] = crate::prompt::anthropic_tools_schema(platform);
        }
        v
    } else {
        // OpenAI-compatible memakai pesan `system` di dalam array `messages`.
        // Sebelumnya pesan ini tersaring keluar dan tidak pernah dikirim,
        // sehingga model tidak menerima prompt sistem (termasuk ringkasan
        // konteks) sama sekali.
        let mut msgs = Vec::with_capacity(rest.len() + 1);
        if let Some(sys) = system {
            msgs.push(json!({"role": "system", "content": sys}));
        }
        msgs.extend(rest);
        let mut v = json!({"model": model, "stream": true, "messages": msgs});
        // Minta laporan token di chunk terakhir supaya statistik provider
        // bisa diisi. Endpoint lama yang tidak kenal `stream_options` akan
        // menolak request — karena itu `include_usage` dicoba terpisah
        // (lihat [ChatStream::open]).
        if include_usage {
            v["stream_options"] = json!({"include_usage": true});
        }
        if with_tools {
            v["tools"] = crate::prompt::tools_schema(platform);
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
    pub fn open(
        provider: &Provider,
        model: &str,
        messages: &[ChatMsg],
        platform: Platform,
    ) -> Result<Self, String> {
        Self::open_inner(provider, model, messages, true, platform)
    }

    /// Seperti [open], tetapi tanpa tool schema. Dipakai untuk peringkasan
    /// konteks supaya model tidak menjawab dengan memanggil tool.
    pub fn open_without_tools(
        provider: &Provider,
        model: &str,
        messages: &[ChatMsg],
    ) -> Result<Self, String> {
        Self::open_inner(provider, model, messages, false, Platform::Android)
    }

    fn open_inner(
        provider: &Provider,
        model: &str,
        messages: &[ChatMsg],
        allow_tools: bool,
        platform: Platform,
    ) -> Result<Self, String> {
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
        // Coba kombinasi (tools?, usage?). Provider yang menolak
        // `stream_options` akan bilang 400 — lalu dicoba tanpa usage supaya
        // chat tetap jalan (statistik token hanya kosong di kasus itu).
        // Kalau pemanggil tidak butuh tools (mis. peringkasan konteks),
        // cukup coba variasi usage saja.
        let combos: &[(bool, bool)] = if allow_tools {
            &[(true, true), (true, false), (false, false), (false, true)]
        } else {
            &[(false, true), (false, false)]
        };
        for &(with_tools, include_usage) in combos {
            let body = request_body(anthropic, model, messages, with_tools, include_usage, platform);
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
            // Kecuali error konteks terlalu panjang: mencoba 4 varian tidak
            // membantu dan hanya menambah latency. Biarkan agent loop yang
            // mengatur ulang anggaran lewat memory::prepare.
            if is_context_error(&last_error) {
                return Err(last_error);
            }
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

/// Apakah pesan error dari provider menandakan konteks terlalu panjang?
///
/// Dipakai untuk retry otomatis dengan anggaran token lebih kecil. Router
/// seperti combo round-robin bisa memilih model 8k walau request sebelumnya
/// dilayani model 128k, jadi batasnya baru ketahuan dari error ini.
pub fn is_context_error(message: &str) -> bool {
    let m = message.to_lowercase();
    m.contains("context length")
        || m.contains("context_length")
        || m.contains("maximum context")
        || m.contains("too many tokens")
        || m.contains("reduce the length")
        || m.contains("prompt is too long")
        || m.contains("input is too long")
        || m.contains("token limit")
        || m.contains("exceeds the maximum")
}

/// Kirim percakapan dan kembalikan seluruh teks jawaban (tanpa streaming ke
/// UI, tanpa tools). Dipakai untuk meringkas konteks lama.
pub fn complete(
    provider: &Provider,
    model: &str,
    messages: &[ChatMsg],
) -> Result<String, String> {
    let mut stream = ChatStream::open_without_tools(provider, model, messages)?;
    let mut out = String::new();
    while let Some(item) = stream.next() {
        match item {
            Ok(delta) => out.push_str(&delta),
            Err(e) => return Err(e),
        }
    }
    if out.trim().is_empty() {
        return Err("Provider tidak mengirim ringkasan.".into());
    }
    Ok(out)
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

// ---------------------------------------------------------------------------
// Perkiraan jendela konteks model
// ---------------------------------------------------------------------------
//
// Anggaran konteks dulu selalu 24.000 token berapa pun modelnya: model 8k
// kena error konteks, model 128k dipaksa meringkas riwayat padahal ruangnya
// masih lega. Dengan deteksi ini anggaran ikut kapasitas model sebenarnya.

/// Slot hasil deteksi per `base_url|model`.
///
/// `Pending` mencegah beberapa chat yang berjalan bersamaan mengulang fetch
/// ke `/models` pada waktu yang sama.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CtxSlot {
    Pending,
    Done(Option<usize>),
}

static CTX_CACHE: OnceLock<Mutex<HashMap<String, CtxSlot>>> = OnceLock::new();

fn ctx_cache() -> &'static Mutex<HashMap<String, CtxSlot>> {
    CTX_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Jendela konteks (dalam token) untuk `model` pada provider `p`.
///
/// Urutan sumber, dari yang paling bisa dipercaya:
/// 1. Endpoint `/models` — banyak gateway (OpenRouter, vLLM, LiteLLM, OmniRoute)
///    menyertakan `context_length` / `context_window` / `max_model_len`.
/// 2. Angka pada nama model (`qwen2.5-32k`, `llama-3.1-128k`).
/// 3. Tabel keluarga model yang umum (claude, gpt-4o, dll).
///
/// Hasil di-cache per proses. Kalau tidak ada perkiraan lokal sama sekali,
/// menunggu fetch paling lama 6 detik sebelum jatuh ke anggaran default.
pub fn context_window(p: &Provider, model: &str) -> Option<usize> {
    let model = model.trim();
    if model.is_empty() {
        return None;
    }
    let key = format!("{}|{model}", p.base_url.trim());

    if let Some(slot) = ctx_cache().lock().ok().and_then(|c| c.get(&key).copied()) {
        return match slot {
            CtxSlot::Done(v) => v,
            // Fetch sedang berjalan di thread lain: jangan dobel, pakai
            // perkiraan lokal saja.
            CtxSlot::Pending => local_window(model),
        };
    }

    let local = local_window(model);
    if p.api_key.trim().is_empty() {
        return local;
    }
    if let Ok(mut c) = ctx_cache().lock() {
        c.insert(key.clone(), CtxSlot::Pending);
    }
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let p = p.clone();
        let model = model.to_string();
        let key = key.clone();
        std::thread::spawn(move || {
            let found = fetch_context_window(&p, &model);
            if let Ok(mut c) = ctx_cache().lock() {
                c.insert(key, CtxSlot::Done(found));
            }
            let _ = tx.send(found);
        });
    }
    if local.is_some() {
        // Sudah ada perkiraan yang layak dipakai: jangan menahan chat
        // menunggu jaringan.
        return local;
    }
    rx.recv_timeout(Duration::from_secs(6)).unwrap_or(None)
}

/// Perkiraan tanpa jaringan: dari nama model, lalu tabel keluarga.
fn local_window(model: &str) -> Option<usize> {
    window_from_name(model).or_else(|| window_from_family(model))
}

/// Ambil angka jendela konteks dari payload `GET /models`.
fn fetch_context_window(p: &Provider, model: &str) -> Option<usize> {
    let url = endpoint(p, "models");
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let resp = auth_headers(client.get(&url), p).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let v: Value = resp.json().ok()?;
    let items = v
        .get("data")
        .and_then(Value::as_array)
        .or_else(|| v.get("models").and_then(Value::as_array))?;
    for it in items {
        let id = it
            .get("id")
            .or_else(|| it.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if id.eq_ignore_ascii_case(model) || model.ends_with(id) || id.ends_with(model) {
            if let Some(n) = window_in_value(it) {
                return Some(n);
            }
        }
    }
    // Id bisa berbeda penamaan (prefix vendor). Hanya pakai baris pertama
    // kalau dafturnya memang berisi satu model — jangan mengambil angka milik
    // model lain.
    if items.len() == 1 {
        return window_in_value(&items[0]);
    }
    None
}

/// Kunci yang umum dipakai untuk menyimpan kapasitas konteks.
const WINDOW_KEYS: [&str; 5] = [
    "context_length",
    "context_window",
    "max_context_tokens",
    "max_model_len",
    "ctx_len",
];

fn window_in_value(v: &Value) -> Option<usize> {
    for k in WINDOW_KEYS {
        if let Some(n) = v.get(k).and_then(value_to_window) {
            return Some(n);
        }
    }
    v.get("details")
        .and_then(|d| d.get("context_length"))
        .and_then(value_to_window)
}

/// Angka yang masuk akal sebagai jendela konteks (>= 1000 token).
/// `max_tokens` sengaja tidak dipakai: di sebagian besar API itu artinya
/// batas output, bukan kapasitas konteks.
fn value_to_window(v: &Value) -> Option<usize> {
    let n = match v {
        Value::Number(n) => n.as_u64()?,
        Value::String(s) => s.trim().trim_end_matches(" tokens").trim().parse().ok()?,
        _ => return None,
    };
    let n = n as usize;
    (n >= 1_000 && n <= 10_000_000).then_some(n)
}

/// Baca angka + satuan `k` / `m` dari nama model, mis. `qwen2.5-32k` → 32.000.
///
/// Sengaja tidak mengangkat `8b`/`70b`: itu jumlah parameter, bukan konteks.
fn window_from_name(model: &str) -> Option<usize> {
    let m = model.rsplit('/').next().unwrap_or(model).to_lowercase();
    let b = m.as_bytes();
    let mut i = 0;
    let mut best: Option<usize> = None;
    while i < b.len() {
        if !b[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let start = i;
        while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
            i += 1;
        }
        let Ok(num) = m[start..i].trim_end_matches('.').parse::<f64>() else {
            continue;
        };
        let unit = b.get(i).map(|c| (*c as char).to_ascii_lowercase());
        // Batas kata: `32k-instruct`, `32k`, `2m.04` — bukan `32kllm` acak.
        let after = b.get(i + 1).map(u8::is_ascii_alphanumeric).unwrap_or(false);
        let n = match unit {
            Some('k') if !after => (num * 1_000.0) as usize,
            Some('m') if !after => (num * 1_000_000.0) as usize,
            _ => continue,
        };
        if (1_000..=10_000_000).contains(&n) {
            best = Some(best.map_or(n, |b: usize| b.max(n)));
        }
    }
    best
}

/// Tabel keluarga model yang beredar umum di gateway.
///
/// Ini tebakan terakhir, jadi nilainya dibatasi anggaran aman oleh pemanggil
/// (≈60% dari sini, maksimum 128k). Kalau meleset, mekanisme retry konteks
/// yang sudah ada akan memangkas anggaran sendiri.
fn window_from_family(model: &str) -> Option<usize> {
    // Nama gateway sering berupa `vendor/model`.
    let m = model.rsplit('/').next().unwrap_or(model).to_lowercase();
    let has = |s: &str| m.contains(s);
    if has("haiku") || has("sonnet") || has("opus") || has("claude") {
        return Some(200_000);
    }
    if has("gpt-4o") || has("gpt-4.1") || has("gpt-4-turbo") || has("gpt-4.5") {
        return Some(128_000);
    }
    if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
        return Some(200_000);
    }
    if has("gemini-1.5-pro") || has("gemini-2") || has("gemini-3") {
        return Some(1_000_000);
    }
    if has("gpt-3.5") {
        return Some(16_385);
    }
    if m.starts_with("gpt-4") {
        return Some(8_192);
    }
    if has("deepseek") {
        return Some(64_000);
    }
    if has("qwen") || has("glm") {
        return Some(32_768);
    }
    if has("llama-3.1") || has("llama-3.3") || has("llama3.1") || has("llama3.3") {
        return Some(128_000);
    }
    if has("mistral") || has("mixtral") {
        return Some(32_000);
    }
    if has("gemma") || has("phi") {
        return Some(8_192);
    }
    None
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
    fn window_from_name_reads_k_and_m_suffixes() {
        assert_eq!(window_from_name("qwen2.5-32k-instruct"), Some(32_000));
        assert_eq!(window_from_name("llama-3.1-8b-instruct-128k"), Some(128_000));
        assert_eq!(window_from_name("my-model-2m"), Some(2_000_000));
        assert_eq!(window_from_name("provider/llama-3.3-70b-8k"), Some(8_000));
    }

    #[test]
    fn window_from_name_ignores_parameter_counts() {
        // `8b`/`14b` jumlah parameter, bukan jendela konteks.
        assert_eq!(window_from_name("llama-3.1-8b"), None);
        assert_eq!(window_from_name("deepseek-r1-distill-qwen-14b"), None);
        // Tanggal rilis sering berupa angka: jangan tertukar.
        assert_eq!(window_from_name("gpt-4o-2024-05-13"), None);
        assert_eq!(window_from_name("claude-3-5-sonnet-20241022"), None);
    }

    #[test]
    fn window_from_family_covers_common_models() {
        assert_eq!(
            window_from_family("anthropic/claude-3-5-sonnet-20241022"),
            Some(200_000)
        );
        assert_eq!(window_from_family("gpt-4o-mini"), Some(128_000));
        assert_eq!(window_from_family("gpt-3.5-turbo"), Some(16_385));
        assert_eq!(window_from_family("openai/o1-preview"), Some(200_000));
        assert_eq!(window_from_family("model-asing-tanpa-ket-riwayat"), None);
    }

    #[test]
    fn window_value_rejects_tiny_and_output_only_numbers() {
        // Angka terlalu kecil tidak mungkin jendela konteks.
        assert_eq!(value_to_window(&json!(500)), None);
        assert_eq!(value_to_window(&json!(4096)), Some(4_096));
        assert_eq!(value_to_window(&json!(32768)), Some(32_768));
        assert_eq!(value_to_window(&json!("131072")), Some(131_072));
        assert_eq!(value_to_window(&json!(true)), None);
        // `max_tokens` sengaja bukan kunci yang dibaca: artinya output.
        assert_eq!(window_in_value(&json!({"max_tokens": 4096})), None);
        assert_eq!(
            window_in_value(&json!({"context_length": 32768})),
            Some(32_768)
        );
    }

    #[test]
    fn openai_body_keeps_system_prompt() {
        let msgs = vec![
            ChatMsg::plain("system", "aturan penting"),
            ChatMsg::plain("user", "halo"),
        ];
        let body = request_body(false, "m", &msgs, false, false, Platform::Android);
        let arr = body["messages"].as_array().unwrap();
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[0]["content"], "aturan penting");
        assert_eq!(arr[1]["role"], "user");
    }

    #[test]
    fn anthropic_body_uses_system_field() {
        let msgs = vec![
            ChatMsg::plain("system", "aturan"),
            ChatMsg::plain("user", "halo"),
        ];
        let body = request_body(true, "m", &msgs, false, false, Platform::Android);
        assert_eq!(body["system"], "aturan");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn context_errors_are_detected() {
        assert!(is_context_error(
            "Provider mengembalikan HTTP 400. {\"error\":\"context_length_exceeded\"}"
        ));
        assert!(is_context_error("This model's maximum context length is 8192 tokens"));
        assert!(!is_context_error("API key ditolak provider (HTTP 401)"));
    }

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
