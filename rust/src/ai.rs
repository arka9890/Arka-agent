//! Klien AI: streaming chat ke provider `openai`-compatible dan `anthropic`.
//!
//! Streaming dibuat "pull" ([ChatStream::next]) supaya pemanggil JNI bisa
//! meneruskan tiap potongan ke UI sekaligus memeriksa tombol Stop tanpa
//! memaksa satu sama lain meminjam `JNIEnv`.

use crate::provider::Provider;
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::time::Duration;

#[derive(Clone, Debug, Deserialize)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
}

/// Konteks yang disisipkan ke setiap sesi chat (lihat `chatStream`).
///
/// Tanpa ini model mengira aplikasi hanya chatbot biasa dan menjawab "aku
/// tidak punya akses ke terminal/Ubuntumu" ketika pengguna meminta pengecekan.
/// Prompt ini menjelaskan peran ganda: model adalah "otak" agent, sedangkan
/// eksekusi perintah dilakukan lewat tab Terminal di perangkat pengguna.
pub const SYSTEM_PROMPT: &str = r#"Kamu adalah Arka Agent, asisten AI di aplikasi Android Arka Agent.

Aplikasi ini memiliki workspace Ubuntu 24.04 yang berjalan di perangkat pengguna melalui proot. Pengguna punya tab Terminal untuk menjalankan perintah Linux, dan folder kerja /root/storage.

Peranmu:
- Kamu adalah "otak" AI dari aplikasi ini. Modelmu dijalankan di server provider AI, jadi kamu tidak bisa langsung mengeksekusi perintah, membaca/menulis file, atau menyentuh OS perangkat sendiri. Jangan mengaku sedang berjalan di dalam Ubuntu.
- Tugasmu adalah membantu pengguna memakai Ubuntu itu: berikan perintah yang siap di-copy-paste ke tab Terminal, jelaskan langkahnya, lalu minta pengguna menempelkan output-nya di chat agar kamu bisa menganalisisnya.
- Kalau pengguna meminta "cek", "coba jalankan", atau "lihat dulu" di Ubuntu, jangan hanya menjawab bahwa kamu tidak punya akses. Berikan perintah konkret (mis. `uname -a`, `cat /etc/os-release`, `python3 --version`) dan katakan dengan jelas bahwa pengguna harus menjalankannya di tab Terminal, lalu menempel hasilnya di sini.
- Kalau pengguna menempel output terminal/error, analisis dan berikan langkah perbaikan berikutnya.
- Bantu juga menulis kode, menjelaskan file di /root/storage, dan troubleshooting.
- Jawab dalam bahasa pengguna (default Bahasa Indonesia). Jika ditanya siapa kamu, jawab bahwa kamu Arka Agent, asisten AI di aplikasi Arka Agent yang membantu memakai workspace Ubuntu di perangkat pengguna. Jangan mengaku sebagai model provider tertentu."#;

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

/// Stream jawaban model. Pakai [ChatStream::next] sampai `None`.
pub struct ChatStream {
    reader: BufReader<reqwest::blocking::Response>,
    anthropic: bool,
    finished: bool,
}

impl ChatStream {
    /// Kirim permintaan dan kembalikan stream yang siap dibaca.
    pub fn open(provider: &Provider, model: &str, messages: &[ChatMsg]) -> Result<Self, String> {
        if provider.api_key.trim().is_empty() {
            return Err(format!(
                "API key untuk provider {} belum diisi. Buka tab Provider untuk mengisinya.",
                provider.name
            ));
        }
        let anthropic = provider.kind == "anthropic";
        let body = if anthropic {
            serde_json::json!({
                "model": model,
                "max_tokens": 4096,
                "stream": true,
                "system": messages.iter().find(|m| m.role == "system").map(|m| m.content.clone()),
                "messages": messages.iter()
                    .filter(|m| m.role != "system")
                    .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
                    .collect::<Vec<_>>(),
            })
        } else {
            serde_json::json!({
                "model": model,
                "stream": true,
                "messages": messages.iter()
                    .map(|m| serde_json::json!({"role": m.role, "content": m.content}))
                    .collect::<Vec<_>>(),
            })
        };
        // Bug lama: kedua protokol memakai "messages" sehingga semua provider
        // OpenAI-compatible (OpenAI, Mistral, Gemini, endpoint custom) kena
        // HTTP 404. Path yang benar berbeda per protokol.
        let url = if anthropic {
            endpoint(provider, "messages")
        } else {
            endpoint(provider, "chat/completions")
        };
        let rb = auth_headers(client()?.post(&url).json(&body), provider);
        let resp = rb.send().map_err(|e| format!("Gagal menghubungi provider: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let text = resp.text().unwrap_or_default();
            return Err(describe_http_error(status, &text));
        }
        Ok(Self { reader: BufReader::new(resp), anthropic, finished: false })
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
            let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) else { continue };
            if let Some(err) = json.get("error").and_then(|e| e.get("message")).and_then(|m| m.as_str()) {
                self.finished = true;
                return Some(Err(format!("Provider menolak permintaan: {err}")));
            }
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

/// Daftar model untuk dropdown chat/provider.
///
/// Model yang disimpan pengguna selalu ikut, dan kalau endpoint `/models`
/// gagal (offline, provider tidak menyediakannya) daftar lokal tetap dipakai.
/// Tanpa ini dropdown hanya menampilkan "Belum ada model" walaupun provider
/// sudah punya model yang dipilih.
pub fn list_models(p: &Provider) -> Result<Vec<String>, String> {
    let mut ids: Vec<String> = p.models.iter()
        .cloned()
        .filter(|s| !s.trim().is_empty())
        .collect();
    if !p.model.trim().is_empty() {
        ids.push(p.model.trim().to_string());
    }
    if let Ok(remote) = fetch_models(p) {
        ids.extend(remote);
    }
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
