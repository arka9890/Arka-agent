//! Jembatan agent loop → UI, tanpa JNI.
//!
//! Dulu `chatStream` memanggil method Kotlin langsung lewat `JNIEnv`, sehingga
//! seluruh logika agent terikat Android. Sekarang agent hanya bicara lewat
//! trait [`Events`]: Android mengimplementasikannya dengan JNI, Windows dengan
//! channel/`egui` context. Crate core tidak lagi tahu soal JNI.

use serde_json::Value;
use std::sync::{mpsc, Arc, Mutex};

/// Kejadian UI yang ditunda, supaya thread sub agent bisa mengirimkannya
/// tanpa memakai `JNIEnv`/state UI dari thread yang salah.
pub enum UiEvent {
    Delta(String),
    Done(String),
    Error(String),
    Tool(String, String),
    Approval {
        id: String,
        action: String,
        target: String,
        detail: String,
    },
    Question {
        id: String,
        question: String,
        options: Vec<String>,
        files: Vec<Value>,
    },
    Progress(i32, String),
    Output(String),
}

/// `Events` yang bisa dikirim ke thread lain. Pemanggil utama menyalurkan
/// isinya kembali ke `Events` asli lewat [`drain`].
#[derive(Clone)]
pub struct ChannelEvents {
    tx: Arc<Mutex<mpsc::Sender<UiEvent>>>,
}

impl ChannelEvents {
    pub fn new() -> (Self, mpsc::Receiver<UiEvent>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx: Arc::new(Mutex::new(tx)) }, rx)
    }

    fn send(&self, event: UiEvent) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(event);
        }
    }
}

impl Events for ChannelEvents {
    fn delta(&self, text: &str) { self.send(UiEvent::Delta(text.to_string())); }
    fn done(&self, text: &str) { self.send(UiEvent::Done(text.to_string())); }
    fn error(&self, message: &str) { self.send(UiEvent::Error(message.to_string())); }
    fn tool(&self, label: &str, display: &str) {
        self.send(UiEvent::Tool(label.to_string(), display.to_string()));
    }
    fn approval(&self, id: &str, action: &str, target: &str, detail: &str) {
        self.send(UiEvent::Approval {
            id: id.to_string(),
            action: action.to_string(),
            target: target.to_string(),
            detail: detail.to_string(),
        });
    }
    fn question(&self, id: &str, question: &str, options: &[String], files: &[Value]) {
        self.send(UiEvent::Question {
            id: id.to_string(),
            question: question.to_string(),
            options: options.to_vec(),
            files: files.to_vec(),
        });
    }
    fn progress(&self, percent: i32, label: &str) {
        self.send(UiEvent::Progress(percent, label.to_string()));
    }
    fn output(&self, text: &str) {
        self.send(UiEvent::Output(text.to_string()));
    }
}

/// Salin semua kejadian yang tertunda ke UI asli. Dipanggil di thread utama
/// setelah thread sub agent selesai.
pub fn drain(rx: &mpsc::Receiver<UiEvent>, ev: &dyn Events) {
    while let Ok(event) = rx.try_recv() {
        match event {
            UiEvent::Delta(t) => ev.delta(&t),
            UiEvent::Done(t) => ev.done(&t),
            UiEvent::Error(t) => ev.error(&t),
            UiEvent::Tool(l, d) => ev.tool(&l, &d),
            UiEvent::Approval { id, action, target, detail } => {
                ev.approval(&id, &action, &target, &detail)
            }
            UiEvent::Question { id, question, options, files } => {
                ev.question(&id, &question, &options, &files)
            }
            UiEvent::Progress(p, l) => ev.progress(p, &l),
            UiEvent::Output(t) => ev.output(&t),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Collector(Mutex<Vec<String>>);

    impl Events for Collector {
        fn delta(&self, _: &str) {}
        fn done(&self, _: &str) {}
        fn error(&self, _: &str) {}
        fn tool(&self, label: &str, _: &str) {
            self.0.lock().unwrap().push(label.to_string());
        }
        fn approval(&self, _: &str, _: &str, _: &str, _: &str) {}
        fn question(&self, _: &str, _: &str, _: &[String], _: &[Value]) {}
        fn progress(&self, _: i32, _: &str) {}
        fn output(&self, _: &str) {}
    }

    #[test]
    fn channel_events_relay_to_main_thread() {
        let (chan, rx) = ChannelEvents::new();
        chan.tool("sub1", "hasil");
        chan.progress(50, "Sub agent: jalan");
        let collector = Collector(Mutex::new(Vec::new()));
        drain(&rx, &collector);
        assert_eq!(collector.0.lock().unwrap().len(), 1);
        assert_eq!(collector.0.lock().unwrap()[0], "sub1");
    }
}
