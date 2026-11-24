//! Menjalankan satu sesi chat di **thread latar**, lalu meneruskan kejadiannya
//! ke UI lewat channel.
//!
//! Padanan `ChatEngine.kt` di Android. Bedanya: di sini tidak ada
//! `viewModelScope`, jadi pemiliknya adalah thread OS biasa. UI tidak pernah
//! memblok menunggu jaringan — ia hanya memanggil [`Engine::drain`] sekali per
//! frame.
//!
//! Kenapa `mpsc` dan bukan `Arc<Mutex<...>>`: `agent::chat` memanggil
//! `Events::delta` dari dalam loop streaming yang bisa ratusan kali per detik.
//! Channel membuat penulis tidak pernah menunggu UI, dan `try_iter` di UI
//! mengambil semuanya sekaligus — satu repaint per frame, bukan per token.

use std::collections::HashSet;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use arka_core::agent::{self, ChatRequest};
use arka_core::events::Events;
use eframe::egui::Context;
use serde_json::Value;

/// Satu kejadian dari thread agent ke UI.
#[derive(Clone, Debug)]
pub enum Msg {
    Delta { session: String, text: String },
    Done { session: String, text: String },
    Error { session: String, message: String },
    Tool { session: String, label: String, display: String },
    Approval { id: String, action: String, target: String, detail: String },
    Question { id: String, question: String, options: Vec<String>, files: Vec<Value> },
    /// Sesi selesai (sukses, gagal, atau dibatalkan). Selalu dikirim terakhir.
    Finished { session: String },
}

/// Jembatan `Events` → channel. Satu-satunya bagian yang menyentuh `Msg`.
pub(crate) struct ChanEvents {
    session: String,
    tx: Sender<Msg>,
    ctx: Context,
}

impl ChanEvents {
    pub(crate) fn new(session: String, tx: Sender<Msg>, ctx: Context) -> Self {
        Self { session, tx, ctx }
    }

    fn send(&self, m: Msg) {
        // Penerima bisa sudah hilang (jendela ditutup) — bukan error.
        let _ = self.tx.send(m);
        // Bangunkan egui: tanpa ini jawaban hanya muncul saat mouse bergerak.
        self.ctx.request_repaint();
    }
}

impl Events for ChanEvents {
    fn delta(&self, text: &str) {
        self.send(Msg::Delta {
            session: self.session.clone(),
            text: text.to_string(),
        });
    }
    fn done(&self, text: &str) {
        self.send(Msg::Done {
            session: self.session.clone(),
            text: text.to_string(),
        });
    }
    fn error(&self, message: &str) {
        self.send(Msg::Error {
            session: self.session.clone(),
            message: message.to_string(),
        });
    }
    fn tool(&self, label: &str, display: &str) {
        self.send(Msg::Tool {
            session: self.session.clone(),
            label: label.to_string(),
            display: display.to_string(),
        });
    }
    fn approval(&self, id: &str, action: &str, target: &str, detail: &str) {
        self.send(Msg::Approval {
            id: id.to_string(),
            action: action.to_string(),
            target: target.to_string(),
            detail: detail.to_string(),
        });
    }
    fn question(&self, id: &str, question: &str, options: &[String], files: &[Value]) {
        self.send(Msg::Question {
            id: id.to_string(),
            question: question.to_string(),
            options: options.to_vec(),
            files: files.to_vec(),
        });
    }
    fn progress(&self, _percent: i32, _label: &str) {}
    fn output(&self, _text: &str) {}
}

/// Pemilik thread-thread chat.
pub struct Engine {
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    running: Arc<Mutex<HashSet<String>>>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            running: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Apakah sesi ini sedang menjawab. Dipakai untuk menonaktifkan tombol
    /// kirim dan menampilkan badge di sidebar.
    pub fn is_running(&self, session: &str) -> bool {
        self.running
            .lock()
            .map(|g| g.contains(session))
            .unwrap_or(false)
    }

    /// Mulai satu sesi chat di thread latar. Kembali seketika.
    pub fn start(&mut self, req: ChatRequest, ctx: Context) {
        let sid = req.session_id.clone();
        if let Ok(mut g) = self.running.lock() {
            g.insert(sid.clone());
        }

        let tx = self.tx.clone();
        let running = Arc::clone(&self.running);
        std::thread::spawn(move || {
            let ev = ChanEvents::new(sid.clone(), tx.clone(), ctx.clone());
            // Hasil akhir tidak dipakai: semua informasi sudah dikirim lewat
            // `Events` (termasuk pesan error), persis seperti jalur Android.
            let _ = agent::chat(req, &ev);
            let _ = tx.send(Msg::Finished {
                session: sid.clone(),
            });
            if let Ok(mut g) = running.lock() {
                g.remove(&sid);
            }
            ctx.request_repaint();
        });
    }

    /// Hanya untuk test: tandai sesi "sedang menjawab" tanpa thread sungguhan,
    /// supaya UI yang bergantung pada status itu bisa diperiksa.
    #[cfg(test)]
    pub fn mark_running(&self, session: &str) {
        if let Ok(mut g) = self.running.lock() {
            g.insert(session.to_string());
        }
    }

    /// Hentikan sesi ini saja. Sesi lain tidak terpengaruh.
    pub fn cancel(&self, session: &str) {
        agent::request_cancel(session);
    }

    /// Ambil semua kejadian sejak frame lalu (tidak memblok).
    pub fn drain(&self) -> Vec<Msg> {
        self.rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_diterjemahkan_menjadi_pesan() {
        let (tx, rx) = mpsc::channel();
        let ev = ChanEvents::new("sesi-1".into(), tx, Context::default());

        ev.delta("hai");
        ev.tool("[A] PS> ls", "a.txt");
        ev.error("jaringan putus");

        let msgs: Vec<Msg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 3);
        match &msgs[0] {
            Msg::Delta { session, text } => {
                assert_eq!(session, "sesi-1");
                assert_eq!(text, "hai");
            }
            other => panic!("bukan Delta: {other:?}"),
        }
        match &msgs[1] {
            Msg::Tool { label, display, .. } => {
                assert_eq!(label, "[A] PS> ls");
                assert_eq!(display, "a.txt");
            }
            other => panic!("bukan Tool: {other:?}"),
        }
        match &msgs[2] {
            Msg::Error { message, .. } => assert_eq!(message, "jaringan putus"),
            other => panic!("bukan Error: {other:?}"),
        }
    }

    #[test]
    fn drain_mengosongkan_antrean() {
        let (tx, rx) = mpsc::channel();
        let ev = ChanEvents::new("s".into(), tx, Context::default());
        ev.done("selesai");

        let msgs: Vec<Msg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 1);
        assert_eq!(rx.try_iter().count(), 0);
    }

    #[test]
    fn engine_melacak_sesi_yang_berjalan() {
        let engine = Engine::new();
        assert!(!engine.is_running("a"));
        engine.running.lock().unwrap().insert("a".into());
        assert!(engine.is_running("a"));
        assert!(!engine.is_running("b"));
        assert!(engine.drain().is_empty());
    }
}
