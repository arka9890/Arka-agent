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
mod proot_manager;
mod provider;
mod session;
mod settings;
mod terminal;

use jni_util as ju;
use serde_json::json;

use jni::objects::{JByteArray, JClass, JObject, JString};
use jni::sys::{jint, jlong};
use jni::JNIEnv;
use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

static CHAT_CANCELS: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
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
    ai::ChatMsg {
        role: role.to_string(),
        content: content.to_string(),
        tool_calls: None,
        tool_call_id: None,
        blocks: None,
    }
}

/// Potong output perintah supaya tidak membengkakkan prompt berikutnya.
fn tail_chars(text: String, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text;
    }
    chars[chars.len() - max..].iter().collect()
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
        msgs.insert(0, plain_msg("system", ai::SYSTEM_PROMPT));
    }
    let model = if model.trim().is_empty() { p.model.clone() } else { model };

    // Batas langkah agent dinaikkan jauh: tugas panjang (clone, build, uji)
    // sering butuh belasan perintah. Dulu 6 → model berhenti di tengah jalan.
    const MAX_TOOL_ROUNDS: usize = 40;
    let mut final_text = String::new();
    let mut rounds = 0usize;

    loop {
        let mut stream = match ai::ChatStream::open(&p, &model, &msgs) {
            Ok(s) => s,
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
        if calls.is_empty() || rounds >= MAX_TOOL_ROUNDS || cancel.load(Ordering::SeqCst) {
            break;
        }
        rounds += 1;

        for call in calls {
            let args: serde_json::Value = serde_json::from_str(&call.arguments)
                .unwrap_or_else(|_| json!({}));
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("").to_string();
            if command.trim().is_empty() {
                continue;
            }
            let output = match proot_manager::run_capture(&command, &cancel, None) {
                Ok((code, out)) => format!("exit {code}\n{}", tail_chars(out, 8000)),
                Err(e) => format!("Gagal menjalankan perintah: {e}"),
            };
            ju::emit_str_str(&mut env, &cb, "onTool", &command, &output);

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
                });
                msgs.push(ai::ChatMsg {
                    role: "tool".into(),
                    content: output,
                    tool_calls: None,
                    tool_call_id: Some(call.id),
                    blocks: None,
                });
            }
        }
    }

    if final_text.trim().is_empty() {
        let msg = if cancel.load(Ordering::SeqCst) { "Dibatalkan." } else { "Provider tidak mengirim jawaban." };
        ju::emit_str(&mut env, &cb, "onError", msg);
        return JString::default();
    }

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
