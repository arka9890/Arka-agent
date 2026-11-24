//! Pembungkus kecil untuk memanggil balik method Kotlin dari Rust.
//!
//! Semua fungsi di sini sengaja "tidak pernah gagal": kalau method Kotlin
//! melempar exception (mis. Activity sudah mati saat streaming berjalan),
//! exception dibersihkan dan proses Rust lanjut. Tanpa ini satu exception
//! yang tidak di-clear akan membuat pemanggilan JNI berikutnya ikut gagal.

use jni::objects::{JObject, JString, JValue};
use jni::JNIEnv;

/// Baca `String` dari parameter JNI. String kosong kalau tidak terbaca.
pub fn arg_str(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|v| v.into()).unwrap_or_default()
}

/// Panggil `cb.<method>(String)`.
pub fn emit_str(env: &mut JNIEnv, cb: &JObject, method: &str, arg: &str) {
    let s = match env.new_string(arg) {
        Ok(s) => s,
        Err(_) => return,
    };
    if env
        .call_method(cb, method, "(Ljava/lang/String;)V", &[JValue::Object(&s)])
        .is_err()
    {
        let _ = env.exception_clear();
    }
}

/// Panggil `cb.<method>(String, String)`.
pub fn emit_str_str(env: &mut JNIEnv, cb: &JObject, method: &str, a: &str, b: &str) {
    let a = match env.new_string(a) {
        Ok(s) => s,
        Err(_) => return,
    };
    let b = match env.new_string(b) {
        Ok(s) => s,
        Err(_) => return,
    };
    if env
        .call_method(cb, method, "(Ljava/lang/String;Ljava/lang/String;)V", &[
            JValue::Object(&a),
            JValue::Object(&b),
        ])
        .is_err()
    {
        let _ = env.exception_clear();
    }
}

/// Panggil `cb.<method>(Int, String)`.
pub fn emit_int_str(env: &mut JNIEnv, cb: &JObject, method: &str, n: i32, arg: &str) {
    let s = match env.new_string(arg) {
        Ok(s) => s,
        Err(_) => return,
    };
    if env
        .call_method(
            cb,
            method,
            "(ILjava/lang/String;)V",
            &[JValue::Int(n), JValue::Object(&s)],
        )
        .is_err()
    {
        let _ = env.exception_clear();
    }
}
