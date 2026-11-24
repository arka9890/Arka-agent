//! Shim JNI untuk aplikasi Android.
//!
//! Crate ini **hanya** berisi:
//!  * semua fungsi `external fun` di `com.arka.agent.core.Arka`;
//!  * `JniEvents` (penerus kejadian ke Kotlin);
//!  * `proot_manager` + `ProotShell` (bagian yang butuh Linux/proot).
//!
//! Semua logika (AI, DB, agent loop, skill, file, setting) ada di
//! `arka_core`, sehingga bisa dipakai versi Windows tanpa JNI.

mod ctx;
mod events;
mod jni_util;
mod proot_manager;

use arka_core::{agent, ai, db, file, memory, paths, provider, session, settings, shell, terminal, ui, Events};
use events::JniEvents;
use jni_util as ju;
use serde_json::json;

use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::sys::{jboolean, jint, jlong};
use jni::JNIEnv;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

// Permintaan batal instalasi & eksekusi terminal (global: hanya satu yang
// boleh berjalan sekaligus).
static EXEC_CANCEL: OnceLock<AtomicBool> = OnceLock::new();
static INSTALL_CANCEL: OnceLock<AtomicBool> = OnceLock::new();

fn exec_cancel() -> &'static AtomicBool {
    EXEC_CANCEL.get_or_init(|| AtomicBool::new(false))
}
fn install_cancel() -> &'static AtomicBool {
    INSTALL_CANCEL.get_or_init(|| AtomicBool::new(false))
}

/// Teruskan permintaan batal dari UI ke operasi yang sedang berjalan.
fn cancel_token(source: &'static AtomicBool) -> std::sync::Arc<AtomicBool> {
    let token = std::sync::Arc::new(AtomicBool::new(false));
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
        paths::init(paths::AppPaths::android(files.clone().into(), libs.into()))?;
        db::init(&format!("{files}/arka.db"))?;
        provider::seed_if_empty()?;
        // Enkripsi API key tetap di Kotlin (Keystore) → store Noop di Rust.
        arka_core::secret::init(Box::new(arka_core::secret::NoopSecretStore));
        // Satu-satunya implementasi Shell di platform ini.
        shell::init(Box::new(proot_manager::ProotShell));
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
                json!({
                    "id": id,
                    "total_tokens": s.total_tokens,
                    "total_requests": s.total_requests,
                    "last_used_at": s.last_used_at
                })
            }).collect::<Vec<_>>())?;
        serde_json::to_string(&stats).map_err(|e| e.to_string())
    })
}

// -------------------------------------------------------------------- AI

/// Kirim percakapan dan alirkan jawaban ke `cb.onDelta(text)`.
///
/// Blocking: panggil dari `Dispatchers.IO`. Semua kejadian (delta, tool,
/// persetujuan, selesai) lewat [`JniEvents`]; logikanya di
/// `arka_core::agent`.
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
    // Argumen dibaca lebih dulu karena `env` kemudian dimiliki JniEvents.
    let req = agent::ChatRequest {
        session_id: ju::arg_str(&mut env, &session_id),
        provider_id: ju::arg_str(&mut env, &provider_id),
        model: ju::arg_str(&mut env, &model),
        messages_json: ju::arg_str(&mut env, &messages_json),
    };

    let events = JniEvents::new(env, cb);
    let result = agent::chat(req, &events);
    // Kembalikan JNIEnv supaya id pesan bisa dibuat sebagai JString.
    let mut env = events.into_env();
    match result {
        Ok(id) => json_str(&mut env, &id),
        // Pesan error sudah dikirim lewat events.error().
        Err(_) => JString::default(),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_cancelChat<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    session_id: JString<'local>,
) {
    let session_id = ju::arg_str(&mut env, &session_id);
    agent::request_cancel(&session_id);
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
    ui::resolve_approval(&id, approved != 0);
}

/// Jawaban pertanyaan pilihan ganda dari UI (indeks pilihan, < 0 = batal).
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_answerQuestion<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
    choice: jint,
) {
    let id = ju::arg_str(&mut env, &id);
    ui::answer_question(&id, choice);
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
        let status = shell::active()?.status()?;
        serde_json::to_string(&status).map_err(|e| e.to_string())
    })
}

/// Unduh + pasang Ubuntu 24; progres lewat `cb.onProgress(percent, label)`.
///
/// Blocking (unduhan + apt): panggil dari `Dispatchers.IO`.
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_installUbuntu<'local>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    cb: JObject<'local>,
) {
    install_cancel().store(false, Ordering::SeqCst);
    let token = cancel_token(install_cancel());
    let events = JniEvents::new(env, cb);
    let result = shell::active().and_then(|s| s.prepare(&events, token.as_ref()));
    token.store(true, Ordering::SeqCst);
    if let Err(e) = result {
        events.error(&e);
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
    let events = JniEvents::new(env, cb);
    let result = shell::active().and_then(|s| {
        // Terminal manual boleh jalan lama (`pip install`, build). Beda dengan
        // tool chat yang dibatasi 120 detik.
        s.exec_with_timeout(
            &command,
            token.as_ref(),
            &mut |chunk| events.output(chunk),
            Some(std::time::Duration::from_secs(600)),
        )
    });
    token.store(true, Ordering::SeqCst);
    match result {
        Ok(outcome) => {
            // Beri penutup yang jelas: terminal tidak boleh terlihat "menggantung"
            // ketika perintah tidak menghasilkan output.
            if outcome.canceled {
                events.output("\n^C dihentikan\n");
            } else if outcome.timed_out {
                // Tanpa penanda ini, perintah panjang (apt/build) terpotong di
                // tengah output dan terlihat seperti terminal "macet".
                events.output(&format!(
                    "\n(timeout — perintah dihentikan karena melewati batas waktu, exit {})\n",
                    outcome.code
                ));
            } else if outcome.output.trim().is_empty() && outcome.code >= 0 {
                events.output(&format!("(perintah selesai, exit {})\n", outcome.code));
            } else if outcome.code > 0 {
                events.output(&format!("\n(exit {})\n", outcome.code));
            }
            outcome.code
        }
        Err(e) => {
            events.output(&format!("{e}\n"));
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
    json_result(&mut env, || shell::active()?.diagnostic())
}

// -------------------------------------------------------------- settings

/// Prompt sistem bawaan (isi awal editor prompt di Setelan).
#[no_mangle]
pub extern "system" fn Java_com_arka_agent_core_Arka_defaultSystemPrompt<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> JString<'local> {
    json_str(&mut env, &agent::default_system_prompt())
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

#[cfg(test)]
mod tests {
    /// Pastikan crate ini tetap kompilasi & nama modul tidak berubah.
    #[test]
    fn shell_registry_is_shared_with_core() {
        // Tidak menginisialisasi apa pun (butuh Android); hanya memastikan
        // tipe trait terlihat dari crate ini.
        let _: fn() -> Result<&'static dyn arka_core::Shell, String> = arka_core::shell::active;
    }
}
