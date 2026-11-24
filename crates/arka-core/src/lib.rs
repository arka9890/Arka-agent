//! Titik masuk JNI: semua fungsi `external fun` di `com.arka.agent.core.Arka`.
//!
//! Konvensi: fungsi yang bisa gagal melempar `RuntimeException` dengan pesan
//! berbahasa Indonesia (sudah siap tampil di UI). Fungsi yang mengembalikan
//! data mengembalikan JSON, supaya sisi Kotlin tidak perlu parsing manual.
//!
//! Pembatalan: chat memakai flag **per sesi** (`cancelChat(session_id)`)
//! supaya membatalkan/memulai chat di satu sesi tidak mengganggu sesi lain;
//! terminal & instalasi tetap global karena hanya satu yang boleh jalan.

mod ai;
mod ctx;
mod db;
mod file;
mod jni_util;
mod memory;
mod proot_manager;
mod provider;
mod session;
mod settings;
mod skill;
mod terminal;

use jni_util as ju;
use serde_json::json;

use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::sys::{jboolean, jint, jlong};
use jni::JNIEnv;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

static CHAT_CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

// Permintaan persetujuan / pertanyaan dari model ke pengguna. Tool di agent
// loop mem-blok sampai UI menjawab lewat JNI, supaya model tidak pernah
// mengklaim berhasil sebelum pengguna menyetujui.
static UI_SEQ: AtomicU64 = AtomicU64::new(0);
static APPROVALS: OnceLock<Mutex<HashMap<String, mpsc::Sender<bool>>>> = OnceLock::new();
static QUESTIONS: OnceLock<Mutex<HashMap<String, mpsc::Sender<usize>>>> = OnceLock::new();

fn next_ui_id(prefix: &str) -> String {
    format!("{}-{}", prefix, UI_SEQ.fetch_add(1, Ordering::SeqCst))
}

/// Data lampiran untuk kartu pertanyaan. Path guest `/root/storage/...`
/// dipetakan ke folder workspace host supaya Android bisa membacanya.
fn file_preview(path: &str) -> Option<serde_json::Value> {
    let raw = path.trim();
    if raw.is_empty() {
        return None;
    }
    let guest = if raw.starts_with('/') {
        raw.to_string()
    } else {
        format!("/root/storage/{raw}")
    };
    let host = if let Some(rest) = guest.strip_prefix("/root/storage/") {
        ctx::workspace_dir().ok()?.join(rest)
    } else if let Some(rest) = guest.strip_prefix("/root/") {
        ctx::rootfs_dir().ok()?.join("root").join(rest)
    } else {
        std::path::PathBuf::from(&guest)
    };
    if !host.is_file() {
        return None;
    }
    let name = host.file_name()?.to_string_lossy().to_string();
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let kind = match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" => "image",
        "mp4" | "webm" | "mkv" | "3gp" | "avi" => "video",
        "mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac" => "audio",
        _ => "file",
    };
    Some(json!({
        "path": guest,
        "hostPath": host.to_string_lossy(),
        "kind": kind,
        "name": name,
    }))
}

/// Tampilkan kartu persetujuan dan tunggu jawaban (maks 5 menit → ditolak).
fn ask_approval(
    env: &mut JNIEnv,
    cb: &JObject,
    action: &str,
    target: &str,
    detail: &str,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    let id = next_ui_id("apv");
    let (tx, rx) = mpsc::channel();
    APPROVALS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id.clone(), tx);
    ju::emit_str(
        env,
        cb,
        "onApproval",
        &json!({ "id": id, "action": action, "target": target, "detail": detail }).to_string(),
    );
    let deadline = Instant::now() + Duration::from_secs(300);
    let answer = loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(v) => break Ok(v),
            Err(mpsc::RecvTimeoutError::Disconnected) => break Ok(false),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    break Ok(false);
                }
            }
        }
    };
    if let Ok(mut map) = APPROVALS.get_or_init(|| Mutex::new(HashMap::new())).lock() {
        map.remove(&id);
    }
    answer
}

/// Tampilkan pertanyaan pilihan ganda dan tunggu indeks jawaban (maks 5 menit).
fn ask_question(
    env: &mut JNIEnv,
    cb: &JObject,
    question: &str,
    options: &[String],
    files: Vec<serde_json::Value>,
    cancel: &AtomicBool,
) -> Result<Option<usize>, String> {
    let id = next_ui_id("qst");
    let (tx, rx) = mpsc::channel();
    QUESTIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|e| e.to_string())?
        .insert(id.clone(), tx);
    ju::emit_str(
        env,
        cb,
        "onQuestion",
        &json!({
            "id": id,
            "question": question,
            "options": options,
            "files": files,
            "file": files.first().cloned(),
        })
        .to_string(),
    );
    let deadline = Instant::now() + Duration::from_secs(300);
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

static EXEC_CANCEL: OnceLock<AtomicBool> = OnceLock::new();
static INSTALL_CANCEL: OnceLock<AtomicBool> = OnceLock::new();

/// Flag batal **per sesi**, bukan global.
///
/// Dulu satu flag global dipakai semua sesi: ketika pengguna pindah sesi lalu
/// mengirim chat baru, `cancelChat`/`chatStream` berikutnya menimpa flag yang
/// sama sehingga chat sesi lama ikut terbatal ("gagal") atau justru hidup lagi
/// karena flag-nya di-reset. Sekarang tiap sesi punya flag sendiri; run lama
/// yang masih berjalan menyimpan `Arc`-nya sendiri sehingga tidak terpengaruh
/// oleh run baru pada sesi yang sama.
fn new_chat_cancel(session_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let map = CHAT_CANCELS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(mut guard) = map.lock() {
        guard.insert(session_id.to_string(), flag.clone());
    }
    flag
}

/// Hapus flag setelah run selesai. Hanya dihapus kalau masih flag yang sama
/// (run baru pada sesi yang sama tidak ikut terhapus).
fn remove_chat_cancel(session_id: &str, flag: &Arc<AtomicBool>) {
    let Some(map) = CHAT_CANCELS.get() else { return };
    if let Ok(mut guard) = map.lock() {
        if guard
            .get(session_id)
            .map(|f| Arc::ptr_eq(f, flag))
            .unwrap_or(false)
        {
            guard.remove(session_id);
        }
    }
}

/// Bersihkan flag cancel otomatis lewat `Drop`, termasuk saat fungsi return
/// lebih awal karena error.
struct ChatCancelGuard {
    session_id: String,
    flag: Arc<AtomicBool>,
}

impl Drop for ChatCancelGuard {
    fn drop(&mut self) {
        remove_chat_cancel(&self.session_id, &self.flag);
    }
}

/// Set flag batal untuk run chat yang terdaftar pada [session_id].
fn request_chat_cancel(session_id: &str) {
    let Some(flag) = CHAT_CANCELS.get()
        .and_then(|m| m.lock().ok())
        .and_then(|g| g.get(session_id).cloned())
    else {
        return;
    };
    flag.store(true, Ordering::SeqCst);
}
fn exec_cancel() -> &'static AtomicBool {
    EXEC_CANCEL.get_or_init(|| AtomicBool::new(false))
}
fn install_cancel() -> &'static AtomicBool {
    INSTALL_CANCEL.get_or_init(|| AtomicBool::new(false))
}

/// Teruskan permintaan batal dari UI (flag global) ke operasi yang sedang
/// berjalan. Dipakai supaya modul lain tidak perlu tahu flag global mana yang
/// berlaku untuknya.
fn cancel_token(source: &'static AtomicBool) -> Arc<AtomicBool> {
    let token = Arc::new(AtomicBool::new(false));
    let token2 = token.clone();
    std::thread::spawn(move || {
        while !token2.load(Ordering::SeqCst) {
            if source.load(Ordering::SeqCst) {
                token2.store(true, Ordering::SeqCst);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    });
    token
}

fn fail(env: &mut JNIEnv, msg: impl AsRef<str>) {
    let _ = env.throw_new("java/lang/RuntimeException", msg.as_ref());
}

fn json_str<'a>(env: &mut JNIEnv<'a>, value: &str) -> JString<'a> {
    match env.new_string(value) {
        Ok(s) => s,
        Err(_) => JString::default(),
    }
}

/// Jalankan [f]; kalau gagal, lempar exception dan kembalikan string kosong.
fn json_result<'a>(
    env: &mut JNIEnv<'a>,
    f: impl FnOnce() -> Result<String, String>,
) -> JString<'a> {
    match f() {
        Ok(s) => json_str(env, &s),
        Err(e) => {
            fail(env, e);
            JString::default()
        }
    }
}

fn unit_result(env: &mut JNIEnv, f: impl FnOnce() -> Result<(), String>) {
    if let Err(e) = f() {
        fail(env, e);
    }
}

// ------------------------------------------------------------------ init

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_nativeInit<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    files_dir: JString<'local>,
    native_lib_dir: JString<'local>,
) {
    let files = ju::arg_str(&mut env, &files_dir);
    let libs = ju::arg_str(&mut env, &native_lib_dir);
    let result = (|| -> Result<(), String> {
        ctx::set_ctx(files.clone().into(), libs.into());
        db::init(&format!("{files}/arka.db"))?;
        provider::seed_if_empty()?;
        proot_manager::check_executable()
    })();
    if let Err(e) = result {
        fail(&mut env, e);
    }
}

// -------------------------------------------------------------- sessions

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_sessionsJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_result(&mut env, || {
        serde_json::to_string(&session::list()?).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_createSession<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    title: JString<'local>,
    provider_id: JString<'local>,
    model: JString<'local>,
) -> JString<'local> {
    let title = ju::arg_str(&mut env, &title);
    let provider_id = ju::arg_str(&mut env, &provider_id);
    let model = ju::arg_str(&mut env, &model);
    json_result(&mut env, || {
        let p = if provider_id.trim().is_empty() { None } else { Some(provider_id.as_str()) };
        let m = if model.trim().is_empty() { None } else { Some(model.as_str()) };
        let s = session::create(&title, p, m)?;
        serde_json::to_string(&s).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_renameSession<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
    title: JString<'local>,
) {
    let id = ju::arg_str(&mut env, &id);
    let title = ju::arg_str(&mut env, &title);
    unit_result(&mut env, || session::rename(&id, &title));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_deleteSession<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
) {
    let id = ju::arg_str(&mut env, &id);
    unit_result(&mut env, || session::delete(&id));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_messagesJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
) -> JString<'local> {
    let session_id = ju::arg_str(&mut env, &session_id);
    json_result(&mut env, || {
        serde_json::to_string(&session::messages(&session_id)?).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_addMessage<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
    role: JString<'local>,
    content: JString<'local>,
    attachments: JString<'local>,
) -> JString<'local> {
    let session_id = ju::arg_str(&mut env, &session_id);
    let role = ju::arg_str(&mut env, &role);
    let content = ju::arg_str(&mut env, &content);
    let attachments = ju::arg_str(&mut env, &attachments);
    json_result(&mut env, || {
        let m = session::add_message(&session_id, &role, &content, &attachments)?;
        serde_json::to_string(&m).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_autotitleSession<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
    text: JString<'local>,
) {
    let session_id = ju::arg_str(&mut env, &session_id);
    let text = ju::arg_str(&mut env, &text);
    unit_result(&mut env, || session::autotitle(&session_id, &text));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_setSessionModel<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
    provider_id: JString<'local>,
    model: JString<'local>,
) {
    let session_id = ju::arg_str(&mut env, &session_id);
    let provider_id = ju::arg_str(&mut env, &provider_id);
    let model = ju::arg_str(&mut env, &model);
    unit_result(&mut env, || session::set_model(&session_id, &provider_id, &model));
}

/// Ringkas riwayat sesi sekarang (manual, dari menu sesi di UI).
///
/// Memakai provider/model sesi; kalau providernya sudah dihapus, jatuh ke
/// provider default. Mengembalikan JSON `{ "summary": "..." }`.
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_compactSession<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
) -> JString<'local> {
    let session_id = ju::arg_str(&mut env, &session_id);
    json_result(&mut env, || {
        let s = session::get(&session_id)?;
        let p = match s.provider_id.as_deref().filter(|v| !v.trim().is_empty()) {
            Some(id) => provider::get(id).or_else(|_| {
                provider::default_provider()?
                    .ok_or_else(|| "Belum ada provider AI. Tambahkan di tab Provider.".to_string())
            })?,
            None => provider::default_provider()?
                .ok_or("Belum ada provider AI. Tambahkan di tab Provider.")?,
        };
        let model = s
            .model
            .clone()
            .filter(|m| !m.trim().is_empty())
            .unwrap_or_else(|| p.model.clone());
        let summary = memory::compact_now(&p, &model, &session_id)?;
        Ok(json!({ "summary": summary }).to_string())
    })
}

// ------------------------------------------------------------- providers

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_providersJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_result(&mut env, || {
        serde_json::to_string(&provider::list()?).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_saveProvider<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    json: JString<'local>,
) -> JString<'local> {
    let json = ju::arg_str(&mut env, &json);
    json_result(&mut env, || {
        let p: provider::Provider =
            serde_json::from_str(&json).map_err(|e| format!("Data provider tidak valid: {e}"))?;
        let saved = provider::save(p)?;
        serde_json::to_string(&saved).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_deleteProvider<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
) {
    let id = ju::arg_str(&mut env, &id);
    unit_result(&mut env, || provider::delete(&id));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_setDefaultProvider<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
) {
    let id = ju::arg_str(&mut env, &id);
    unit_result(&mut env, || provider::set_default(&id));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_testProvider<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    json: JString<'local>,
) -> JString<'local> {
    let json = ju::arg_str(&mut env, &json);
    json_result(&mut env, || {
        let p: provider::Provider =
            serde_json::from_str(&json).map_err(|e| format!("Data provider tidak valid: {e}"))?;
        ai::test_connection(&p)
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_modelsJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    provider_id: JString<'local>,
) -> JString<'local> {
    let provider_id = ju::arg_str(&mut env, &provider_id);
    json_result(&mut env, || {
        let p = provider::get(&provider_id)?;
        let models = ai::list_models(&p)?;
        serde_json::to_string(&models).map_err(|e| e.to_string())
    })
}

/// Cek daftar model dari server secara manual (tombol refresh di UI).
/// Hasilnya disimpan di DB supaya pemanggilan berikutnya instan.
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_refreshModels<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    provider_id: JString<'local>,
) -> JString<'local> {
    let provider_id = ju::arg_str(&mut env, &provider_id);
    json_result(&mut env, || {
        let p = provider::get(&provider_id)?;
        let models = ai::refresh_models(&p)?;
        provider::save_models(&provider_id, &models)?;
        serde_json::to_string(&models).map_err(|e| e.to_string())
    })
}

// -------------------------------------------------------------------- AI

fn plain_msg(role: &str, content: &str) -> ai::ChatMsg {
    ai::ChatMsg::plain(role, content)
}

/// Potong output perintah supaya tidak membengkakkan prompt berikutnya.
fn tail_chars(text: String, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text;
    }
    // M2: tandai output yang dipotong supaya model/pengguna tahu sebagian
    // hasil tidak ditampilkan, bukan sekadar "hilang".
    format!("…(terpotong, {n} karakter sebelumnya)\n{}", chars[chars.len() - max..].iter().collect::<String>(), n = chars.len() - max)
}

/// Kirim percakapan dan alirkan jawaban ke `cb.onDelta(text)`.
///
/// Kalau model memanggil tool `run_command`, perintah dijalankan di Ubuntu
/// lewat proot, hasilnya dikirim balik ke model, lalu sesi dilanjutkan sampai
/// model selesai menjawab (agent loop). `cb.onTool` dipanggil untuk setiap
/// perintah supaya UI bisa menampilkannya di tab Terminal.
///
/// Blocking: panggil dari `Dispatchers.IO`.
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_recordProviderUsage<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    provider_id: JString<'local>,
    tokens: jlong,
    requests: jlong,
) {
    let provider_id = ju::arg_str(&mut env, &provider_id);
    unit_result(&mut env, || {
        provider::record_usage(&provider_id, tokens, requests)
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_providerStatsJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_result(&mut env, || {
        let stats = provider::get_all_stats()
            .map(|v| v.into_iter().map(|(id, s)| {
                serde_json::json!({
                    "id": id,
                    "total_tokens": s.total_tokens,
                    "total_requests": s.total_requests,
                    "last_used_at": s.last_used_at
                })
            }).collect::<Vec<_>>())?;
        serde_json::to_string(&stats).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_chatStream<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
    provider_id: JString<'local>,
    model: JString<'local>,
    messages_json: JString<'local>,
    cb: JObject<'local>,
) -> JString<'local> {
    let session_id = ju::arg_str(&mut env, &session_id);
    let provider_id = ju::arg_str(&mut env, &provider_id);
    let model = ju::arg_str(&mut env, &model);
    let messages_json = ju::arg_str(&mut env, &messages_json);

    // Flag batal khusus sesi ini; run lama pada sesi yang sama tidak terganggu.
    let cancel = new_chat_cancel(&session_id);
    let _cancel_guard = ChatCancelGuard {
        session_id: session_id.clone(),
        flag: cancel.clone(),
    };

    let prepared = (|| -> Result<(provider::Provider, Vec<ai::ChatMsg>), String> {
        let p = if provider_id.trim().is_empty() {
            provider::default_provider()?.ok_or("Belum ada provider AI. Tambahkan di tab Provider.")?
        } else {
            provider::get(&provider_id)?
        };
        let msgs: Vec<ai::ChatMsg> = serde_json::from_str::<Vec<ai::ChatMsg>>(&messages_json)
            .map_err(|e| format!("Riwayat pesan tidak valid: {e}"))?;
        Ok((p, msgs))
    })();

    let (p, mut msgs) = match prepared {
        Ok(v) => v,
        Err(e) => {
            ju::emit_str(&mut env, &cb, "onError", &e);
            return JString::default();
        }
    };
    if !msgs.iter().any(|m| m.role == "system") {
        // Prompt kustom dari Setelan; kalau kosong, prompt bawaan Arka Agent.
        // Skill dari ~/.claude/skills di proot ikut disisipkan.
        let mut system_prompt = settings::system_prompt();
        let skills = skill::system_prompt_section();
        if !skills.is_empty() {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&skills);
        }
        msgs.insert(0, plain_msg("system", &system_prompt));
    }
    let model = if model.trim().is_empty() { p.model.clone() } else { model };

    // Ringkas riwayat lama sebelum dikirim. Ini pengaman untuk router/combo
    // yang memilih model dengan context window kecil: lebih baik model
    // menerima ringkasan + pesan terakhir daripada awal obrolan dipotong
    // diam-diam oleh provider.
    let budget = settings::context_budget();
    if settings::auto_summary() {
        msgs = memory::prepare(&p, &model, &session_id, msgs, budget, cancel.as_ref());
    }

    // Batas langkah agent bisa diatur di Setelan (default 40): tugas panjang
    // (clone, build, uji) sering butuh belasan perintah.
    let max_tool_rounds = settings::max_tool_rounds();
    let tool_output_chars = settings::tool_output_chars();
    let mut final_text = String::new();
    let mut rounds = 0usize;
    // Token dari setiap putaran dijumlahkan: satu jawaban bisa terdiri dari
    // puluhan panggilan provider (tiap putaran tool = 1 permintaan).
    let mut total_tokens: i64 = 0;
    // Anggaran bisa diturunkan otomatis kalau provider menolak karena
    // konteks terlalu panjang (mis. router memilih model 8k).
    let mut effective_budget = budget;
    let mut context_retries = 0usize;

    loop {
        // Output tool dari putaran sebelumnya bisa membuat konteks membengkak
        // di tengah tugas. Pangkas output paling lama, bukan pesan terbaru.
        memory::fit_tool_outputs(&mut msgs, effective_budget);
        let mut stream = match ai::ChatStream::open(&p, &model, &msgs) {
            Ok(s) => s,
            Err(e)
                if settings::auto_summary()
                    && context_retries < 3
                    && effective_budget > settings::CONTEXT_BUDGET_MIN
                    && ai::is_context_error(&e) =>
            {
                context_retries += 1;
                effective_budget =
                    (effective_budget / 2).max(settings::CONTEXT_BUDGET_MIN);
                if rounds == 0 {
                    // Belum ada tool call: urutan pesan masih aman diubah.
                    msgs = memory::prepare(
                        &p,
                        &model,
                        &session_id,
                        msgs,
                        effective_budget,
                        cancel.as_ref(),
                    );
                } else {
                    // Sudah ada tool call: jangan mengubah urutan pesan,
                    // cukup pangkas output tool lebih agresif.
                    memory::fit_tool_outputs(&mut msgs, effective_budget);
                }
                continue;
            }
            Err(e) => {
                ju::emit_str(&mut env, &cb, "onError", &e);
                return JString::default();
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
                    ju::emit_str(&mut env, &cb, "onDelta", &delta);
                }
                Err(e) => {
                    stream_error = Some(e);
                    break;
                }
            }
        }
        total_tokens += stream.usage_tokens();

        // Simpan semua teks dari tiap putaran (bukan hanya terakhir): kalau
        // ditimpa, jawaban yang tampil di stream mendadak menyusut setelah
        // reload dan terlihat seperti "model berhenti di tengah".
        if !answer.trim().is_empty() {
            final_text.push_str(&answer);
        }
        if let Some(e) = stream_error {
            if final_text.trim().is_empty() {
                ju::emit_str(&mut env, &cb, "onError", &e);
                return JString::default();
            }
            break;
        }
        let calls = stream.take_tool_calls();
        if calls.is_empty() || cancel.load(Ordering::SeqCst) {
            break;
        }
        if rounds >= max_tool_rounds {
            // Beri tahu pengguna kenapa model berhenti: batas ini bisa diubah
            // di Setelan → Agent.
            final_text.push_str(&format!(
                "\n\n_(Batas {max_tool_rounds} putaran tool tercapai. Naikkan di Setelan → Agent kalau tugas belum selesai.)_"
            ));
            break;
        }
        rounds += 1;

        for call in calls {
            let args: serde_json::Value = serde_json::from_str(&call.arguments)
                .unwrap_or_else(|_| json!({}));
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
                        match ask_approval(
                            &mut env,
                            &cb,
                            "Pasang skill",
                            &url,
                            "Unduh SKILL.md / repo dan simpan ke ~/.claude/skills di Ubuntu.",
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
                    let msg = match ask_approval(
                        &mut env,
                        &cb,
                        "Hapus skill",
                        &name,
                        "Folder skill dan konfigurasinya akan dihapus dari Ubuntu.",
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
                    let msg = match ask_approval(
                        &mut env,
                        &cb,
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
                    let msg = match ask_approval(
                        &mut env,
                        &cb,
                        "Ubah konfigurasi skill",
                        &name,
                        &format!("{key} = {value} (disimpan di ~/.config/{name}/config.json)"),
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
                        match ask_question(
                            &mut env,
                            &cb,
                            &question,
                            &options,
                            {
                                let mut paths: Vec<String> = args
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
                                    paths.push(single);
                                }
                                paths.truncate(5);
                                paths
                                    .iter()
                                    .filter_map(|p| file_preview(p))
                                    .collect()
                            },
                            &cancel,
                        ) {
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
                    let (display, output) = match proot_manager::run_capture(&command, &cancel, None) {
                        Ok((code, out)) => {
                            let full = format!("exit {code}\n{out}");
                            // Terminal boleh menampilkan lebih banyak; konteks
                            // model dibatasi supaya tidak cepat penuh.
                            (
                                tail_chars(full.clone(), 20_000),
                                tail_chars(full, tool_output_chars),
                            )
                        }
                        Err(e) => {
                            let msg = format!("Gagal menjalankan perintah: {e}");
                            (msg.clone(), msg)
                        }
                    };
                    (command, display, output)
                }
            };
            ju::emit_str_str(&mut env, &cb, "onTool", &tool_label, &display);

            if p.kind == "anthropic" {
                msgs.push(ai::ChatMsg {
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
                msgs.push(ai::ChatMsg {
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
                msgs.push(ai::ChatMsg {
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
                msgs.push(ai::ChatMsg {
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
        let msg = if cancel.load(Ordering::SeqCst) { "Dibatalkan." } else { "Provider tidak mengirim jawaban." };
        ju::emit_str(&mut env, &cb, "onError", msg);
        return JString::default();
    }

    // Statistik provider: jawaban dihitung 1 permintaan, plus tiap putaran
    // tool yang sudah dijalankan. Kalau provider tidak mengirim metadata usage
    // (provider::record_usage hanya menambah request), token-nya 0.
    let _ = provider::record_usage(&p.id, total_tokens, 1 + rounds as i64);

    match session::add_message(&session_id, "assistant", &final_text, "[]") {
        Ok(m) => {
            ju::emit_str(&mut env, &cb, "onDone", &final_text);
            json_str(&mut env, &m.id)
        }
        Err(e) => {
            ju::emit_str(&mut env, &cb, "onError", &format!("Jawaban diterima tetapi gagal disimpan: {e}"));
            JString::default()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_cancelChat<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
) {
    let session_id = ju::arg_str(&mut env, &session_id);
    request_chat_cancel(&session_id);
}

/// Jawaban kartu persetujuan dari UI (Izinkan / Tolak).
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_resolveApproval<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
    approved: jboolean,
) {
    let id = ju::arg_str(&mut env, &id);
    if let Some(map) = APPROVALS.get() {
        if let Ok(mut guard) = map.lock() {
            if let Some(tx) = guard.remove(&id) {
                let _ = tx.send(approved != 0);
            }
        }
    }
}

/// Jawaban pertanyaan pilihan ganda dari UI (indeks pilihan).
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_answerQuestion<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
    choice: jint,
) {
    let id = ju::arg_str(&mut env, &id);
    if let Some(map) = QUESTIONS.get() {
        if let Ok(mut guard) = map.lock() {
            if let Some(tx) = guard.remove(&id) {
                // choice < 0 = Batal: tidak mengirim jawaban, channel
                // tertutup → agent loop menganggap tidak ada jawaban.
                if choice >= 0 {
                    let _ = tx.send(choice as usize);
                }
            }
        }
    }
}

// ----------------------------------------------------------------- files

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_filesJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
) -> JString<'local> {
    let path = ju::arg_str(&mut env, &path);
    json_result(&mut env, || {
        serde_json::to_string(&file::list(&path)?).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_readText<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
) -> JString<'local> {
    let path = ju::arg_str(&mut env, &path);
    json_result(&mut env, || file::read_text(&path))
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_fileStat<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
) -> JString<'local> {
    let path = ju::arg_str(&mut env, &path);
    json_result(&mut env, || {
        serde_json::to_string(&file::stat(&path)?).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_deleteFile<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
) {
    let path = ju::arg_str(&mut env, &path);
    unit_result(&mut env, || file::delete(&path));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_mkdir<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
) {
    let path = ju::arg_str(&mut env, &path);
    unit_result(&mut env, || file::mkdir(&path));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_renameFile<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    from: JString<'local>,
    to: JString<'local>,
) {
    let from = ju::arg_str(&mut env, &from);
    let to = ju::arg_str(&mut env, &to);
    unit_result(&mut env, || file::rename(&from, &to));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_writeBytes<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    path: JString<'local>,
    data: JByteArray<'local>,
) -> JString<'local> {
    let path = ju::arg_str(&mut env, &path);
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(_) => {
            fail(&mut env, "Data file tidak bisa dibaca");
            return JString::default();
        }
    };
    json_result(&mut env, || {
        let info = file::write_bytes(&path, &bytes)?;
        serde_json::to_string(&info).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_workspaceSize<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jlong {
    match file::workspace_size() {
        Ok(n) => n,
        Err(e) => {
            fail(&mut env, e);
            0
        }
    }
}

// -------------------------------------------------------- distro / shell

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_distroStatusJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_result(&mut env, || {
        serde_json::to_string(&proot_manager::status()?).map_err(|e| e.to_string())
    })
}

/// Unduh + pasang Ubuntu 24; progres lewat `cb.onProgress(percent, label)`.
///
/// Blocking (unduhan + apt): panggil dari `Dispatchers.IO`.
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_installUbuntu<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    cb: JObject<'local>,
) {
    install_cancel().store(false, Ordering::SeqCst);
    let token = cancel_token(install_cancel());
    let result = proot_manager::install(&mut env, &cb, token.clone());
    token.store(true, Ordering::SeqCst);
    if let Err(e) = result {
        ju::emit_str(&mut env, &cb, "onError", &e);
    }
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_cancelInstall<'local>(_env: JNIEnv, _class: JClass) {
    install_cancel().store(true, Ordering::SeqCst);
}

/// Jalankan perintah Linux di dalam Ubuntu; output streaming ke `cb.onOutput`.
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_execCommand<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    command: JString<'local>,
    cb: JObject<'local>,
) -> jint {
    let command = ju::arg_str(&mut env, &command);
    exec_cancel().store(false, Ordering::SeqCst);
    let token = cancel_token(exec_cancel());
    let result = proot_manager::exec(&mut env, &cb, &command, token.clone());
    token.store(true, Ordering::SeqCst);
    match result {
        Ok(code) => code,
        Err(e) => {
            ju::emit_str(&mut env, &cb, "onOutput", &format!("{e}\n"));
            -1
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_cancelExec<'local>(_env: JNIEnv, _class: JClass) {
    exec_cancel().store(true, Ordering::SeqCst);
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_diagnostic<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_result(&mut env, proot_manager::diagnostic)
}

// -------------------------------------------------------------- settings

/// Prompt sistem bawaan (dipakai UI sebagai isi awal saat pengguna ingin
/// mengubahnya, dan sebagai fallback kalau setelan `system_prompt` kosong).
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_defaultSystemPrompt<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_str(&mut env, ai::SYSTEM_PROMPT)
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_settingsJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_result(&mut env, settings::all_json)
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_setSetting<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    key: JString<'local>,
    value: JString<'local>,
) {
    let key = ju::arg_str(&mut env, &key);
    let value = ju::arg_str(&mut env, &value);
    unit_result(&mut env, || settings::set(&key, &value));
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_terminalHistoryJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_result(&mut env, || {
        serde_json::to_string(&terminal::list()?).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_pushTerminalHistory<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    command: JString<'local>,
) -> JString<'local> {
    let command = ju::arg_str(&mut env, &command);
    json_result(&mut env, || {
        serde_json::to_string(&terminal::push(&command)?).map_err(|e| e.to_string())
    })
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_clearTerminalHistory<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) {
    unit_result(&mut env, terminal::clear);
}
