//! Permintaan interaktif dari model ke pengguna: kartu persetujuan, pertanyaan
//! pilihan ganda, dan pratinjau file.
//!
//! Tool di agent loop **memblok** sampai UI menjawab, supaya model tidak
//! pernah mengklaim berhasil sebelum pengguna menyetujui. Jawaban masuk lewat
//! [`resolve_approval`] / [`answer_question`] (dipanggil crate JNI atau UI
//! desktop). Registry-nya di sini, bukan di lapisan JNI, supaya platform lain
//! memakai jalur yang sama.

use crate::events::Events;
use crate::paths::{self, Platform};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Batas menunggu jawaban pengguna. Lewat dari ini dianggap ditolak.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(300);

static SEQ: AtomicU64 = AtomicU64::new(0);
static APPROVALS: OnceLock<Mutex<HashMap<String, mpsc::Sender<bool>>>> = OnceLock::new();
static QUESTIONS: OnceLock<Mutex<HashMap<String, mpsc::Sender<usize>>>> = OnceLock::new();

fn next_id(prefix: &str) -> String {
    format!("{}-{}", prefix, SEQ.fetch_add(1, Ordering::SeqCst))
}

/// Tampilkan kartu persetujuan dan tunggu jawaban (timeout → ditolak).
pub fn ask_approval(
    ev: &dyn Events,
    action: &str,
    target: &str,
    detail: &str,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    let id = next_id("apv");
    let (tx, rx) = mpsc::channel();
    APPROVALS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id.clone(), tx);
    ev.approval(&id, action, target, detail);
    let answer = wait_bool(&rx, cancel, ANSWER_TIMEOUT);
    if let Some(map) = APPROVALS.get() {
        if let Ok(mut guard) = map.lock() {
            guard.remove(&id);
        }
    }
    answer
}

fn wait_bool(rx: &mpsc::Receiver<bool>, cancel: &AtomicBool, timeout: Duration) -> Result<bool, String> {
    let deadline = Instant::now() + timeout;
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(v) => return Ok(v),
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(false),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    return Ok(false);
                }
            }
        }
    }
}

/// Tampilkan pertanyaan pilihan ganda dan tunggu indeks jawaban.
///
/// `Ok(None)` = pengguna membatalkan atau waktu habis.
pub fn ask_question(
    ev: &dyn Events,
    question: &str,
    options: &[String],
    files: Vec<Value>,
    cancel: &AtomicBool,
) -> Result<Option<usize>, String> {
    let id = next_id("qst");
    let (tx, rx) = mpsc::channel();
    QUESTIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id.clone(), tx);
    ev.question(&id, question, options, &files);
    let deadline = Instant::now() + ANSWER_TIMEOUT;
    let answer = loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(index) => break Ok(Some(index)),
            Err(mpsc::RecvTimeoutError::Disconnected) => break Ok(None),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    break Ok(None);
                }
            }
        }
    };
    if let Ok(mut map) = QUESTIONS.get_or_init(|| Mutex::new(HashMap::new())).lock() {
        map.remove(&id);
    }
    answer
}

/// Jawaban kartu persetujuan dari UI.
pub fn resolve_approval(id: &str, approved: bool) {
    if let Some(map) = APPROVALS.get() {
        if let Ok(mut guard) = map.lock() {
            if let Some(tx) = guard.remove(id) {
                let _ = tx.send(approved);
            }
        }
    }
}

/// Jawaban pertanyaan dari UI. `choice < 0` = dibatalkan.
pub fn answer_question(id: &str, choice: i32) {
    if let Some(map) = QUESTIONS.get() {
        if let Ok(mut guard) = map.lock() {
            if let Some(tx) = guard.remove(id) {
                if choice >= 0 {
                    let _ = tx.send(choice as usize);
                }
            }
        }
    }
}

/// Petakan path dari model ke path nyata di disk.
///
/// * Android: `/root/storage/...` → `<filesDir>/workspace/...`,
///   `/root/...` → `<rootfs>/root/...` (path guest, karena model melihat
///   filesystem Ubuntu).
/// * Windows: path absolut dipakai apa adanya, path relatif dari workspace.
pub fn host_path(raw: &str) -> Result<PathBuf, String> {
    let p = paths::get()?;
    let raw = raw.trim();
    if raw.is_empty() {
        return Err("Path kosong".to_string());
    }
    match p.platform {
        Platform::Android => {
            let guest = if raw.starts_with('/') {
                raw.to_string()
            } else {
                format!("/root/storage/{raw}")
            };
            if let Some(rest) = guest.strip_prefix("/root/storage/") {
                Ok(p.workspace.join(rest))
            } else if let Some(rest) = guest.strip_prefix("/root/") {
                Ok(p.rootfs_dir.clone().unwrap_or_default().join("root").join(rest))
            } else {
                Ok(PathBuf::from(guest))
            }
        }
        Platform::Windows => {
            let path = PathBuf::from(raw);
            Ok(if path.is_absolute() {
                path
            } else {
                p.workspace.join(raw)
            })
        }
    }
}

/// Bangun pratinjau file untuk kartu `ask_user`.
///
/// Mengembalikan `None` kalau file tidak ada / tidak bisa dipetakan, supaya
/// kartu tetap tampil tanpa pratinjau yang rusak.
pub fn file_preview(raw: &str) -> Option<Value> {
    let host = host_path(raw).ok()?;
    if !host.is_file() {
        return None;
    }
    let name = host.file_name()?.to_string_lossy().to_string();
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let kind = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "ico" => "image",
        "mp4" | "webm" | "mkv" | "3gp" | "avi" | "mov" => "video",
        "mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac" => "audio",
        _ => "file",
    };
    Some(json!({
        "path": raw.trim(),
        "hostPath": host.to_string_lossy(),
        "kind": kind,
        "name": name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_returns_none_for_missing_file() {
        assert!(file_preview("/root/storage/tidak-ada.png").is_none());
    }

    #[test]
    fn answer_without_pending_id_is_noop() {
        resolve_approval("apv-tidak-ada", true);
        answer_question("qst-tidak-ada", 0);
    }
}
