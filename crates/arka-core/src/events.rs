//! Jembatan agent loop → UI, tanpa JNI.
//!
//! Dulu `chatStream` memanggil method Kotlin langsung lewat `JNIEnv`, sehingga
//! seluruh logika agent terikat Android. Sekarang agent hanya bicara lewat
//! trait [`Events`]: Android mengimplementasikannya dengan JNI, Windows dengan
//! channel/`egui` context. Crate core tidak lagi tahu soal JNI.

use serde_json::Value;

/// Kejadian yang dikirim agent ke UI selama satu sesi chat.
///
/// Semua method `&self` supaya implementasi bisa memakai interior mutability
/// (JNI butuh `&mut JNIEnv`, UI butuh `&mut` state).
pub trait Events {
    /// Potongan teks jawaban (streaming).
    fn delta(&self, text: &str);
    /// Jawaban lengkap selesai dan sudah disimpan.
    fn done(&self, text: &str);
    /// Error yang membuat sesi berhenti.
    fn error(&self, message: &str);
    /// Satu tool dijalankan: [label] singkat, [display] output untuk terminal.
    fn tool(&self, label: &str, display: &str);
    /// Minta persetujuan pengguna (`id` dipakai untuk menjawab).
    fn approval(&self, id: &str, action: &str, target: &str, detail: &str);
    /// Pertanyaan pilihan ganda + pratinjau file.
    fn question(&self, id: &str, question: &str, options: &[String], files: &[Value]);
    /// Progres operasi panjang (0–100).
    fn progress(&self, percent: i32, label: &str);
    /// Output streaming terminal/instalasi.
    fn output(&self, text: &str);
}

/// Implementasi kosong: dipakai tes, CLI, dan mode "tanpa UI".
pub struct NoopEvents;

impl Events for NoopEvents {
    fn delta(&self, _: &str) {}
    fn done(&self, _: &str) {}
    fn error(&self, _: &str) {}
    fn tool(&self, _: &str, _: &str) {}
    fn approval(&self, _: &str, _: &str, _: &str, _: &str) {}
    fn question(&self, _: &str, _: &str, _: &[String], _: &[Value]) {}
    fn progress(&self, _: i32, _: &str) {}
    fn output(&self, _: &str) {}
}
