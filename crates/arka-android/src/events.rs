//! Implementasi [`arka_core::events::Events`] lewat JNI.
//!
//! Satu-satunya tempat di proyek yang memanggil balik method Kotlin. Semua
//! modul lain (agent, shell, ui) hanya melihat trait, sehingga logika yang
//! sama bisa dipakai desktop.
//!
//! `JNIEnv` membutuhkan `&mut`, sedangkan trait memakai `&self`; karena semua
//! kejadian dikirim dari thread yang sama (panggilan JNI bersifat blocking),
//! cukup dibungkus `RefCell`.

use super::jni_util as ju;
use arka_core::events::Events;
use jni::objects::JObject;
use jni::JNIEnv;
use serde_json::Value;
use std::cell::RefCell;

pub struct JniEvents<'a> {
    env: RefCell<JNIEnv<'a>>,
    cb: JObject<'a>,
}

impl<'a> JniEvents<'a> {
    pub fn new(env: JNIEnv<'a>, cb: JObject<'a>) -> Self {
        Self {
            env: RefCell::new(env),
            cb,
        }
    }

    /// Ambil kembali `JNIEnv` setelah pemakaian (mis. untuk membuat `JString`
    /// hasil). JNIEnv tidak bisa `Clone`, jadi harus dioper, bukan dipinjam.
    pub fn into_env(self) -> JNIEnv<'a> {
        self.env.into_inner()
    }
}

impl Events for JniEvents<'_> {
    fn delta(&self, text: &str) {
        ju::emit_str(&mut self.env.borrow_mut(), &self.cb, "onDelta", text);
    }

    fn done(&self, text: &str) {
        ju::emit_str(&mut self.env.borrow_mut(), &self.cb, "onDone", text);
    }

    fn error(&self, message: &str) {
        ju::emit_str(&mut self.env.borrow_mut(), &self.cb, "onError", message);
    }

    fn tool(&self, label: &str, display: &str) {
        ju::emit_str_str(
            &mut self.env.borrow_mut(),
            &self.cb,
            "onTool",
            label,
            display,
        );
    }

    fn approval(&self, id: &str, action: &str, target: &str, detail: &str) {
        let payload = serde_json::json!({
            "id": id,
            "action": action,
            "target": target,
            "detail": detail,
        })
        .to_string();
        ju::emit_str(&mut self.env.borrow_mut(), &self.cb, "onApproval", &payload);
    }

    fn question(&self, id: &str, question: &str, options: &[String], files: &[Value]) {
        let payload = serde_json::json!({
            "id": id,
            "question": question,
            "options": options,
            "files": files,
            "file": files.first().cloned(),
        })
        .to_string();
        ju::emit_str(&mut self.env.borrow_mut(), &self.cb, "onQuestion", &payload);
    }

    fn progress(&self, percent: i32, label: &str) {
        ju::emit_int_str(
            &mut self.env.borrow_mut(),
            &self.cb,
            "onProgress",
            percent,
            label,
        );
    }

    fn output(&self, text: &str) {
        ju::emit_str(&mut self.env.borrow_mut(), &self.cb, "onOutput", text);
    }
}
