//! Membaca stdout+stderr proses anak secara paralel sambil memantau
//! batal/timeout, dengan decoding UTF-8 yang **aman di batas potongan**.
//!
//! Modul ini tidak menyentuh Windows sama sekali: semua yang butuh OS masuk
//! lewat parameter (`pipes`, `kill`). Itulah yang membuat perilakunya bisa
//! diuji di Linux/CI memakai `/bin/sh`, bukan hanya "dipercaya" begitu saja.
//!
//! Tiga masalah yang diselesaikan di sini:
//! 1. **Deadlock pipe** — kalau hanya satu pipe dibaca, pipe satunya penuh
//!    (±4 KB di Windows) dan proses anak membeku selamanya. Karena itu dua
//!    pipe dibaca di dua thread, hasilnya digabung lewat channel.
//! 2. **Karakter multibyte terpecah** — satu emoji (4 byte) bisa datang
//!    sebagai dua potongan `read(4096)`. `String::from_utf8_lossy` per potongan
//!    akan merusaknya jadi `�`. [`Utf8Decoder`] menyimpan sisa byte sampai
//!    lengkap.
//! 3. **Batal/timeout yang tidak benar-benar mematikan** — loop memantau
//!    `cancel` dan batas waktu tiap ±40 ms lalu memanggil `kill`.

use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

/// Seberapa sering pemantauan batal/timeout dilakukan.
const TICK: Duration = Duration::from_millis(40);
/// Setelah `kill` dipanggil, tunggu paling lama selama ini agar pipe tertutup.
///
/// Di Windows job object mematikan seluruh pohon proses sehingga pipe langsung
/// EOF; batas ini hanya jaring pengaman kalau ada proses yang lolos dari job
/// (mis. anak yang lahir sebelum penempelan). Tanpa batas ini, satu cucu yang
/// masih hidup bisa membuat agent menggantung selamanya.
const GRACE_AFTER_KILL: Duration = Duration::from_secs(1);

/// Decoder UTF-8 yang menahan byte sisa antar-potongan.
#[derive(Default, Debug)]
pub struct Utf8Decoder {
    carry: Vec<u8>,
}

impl Utf8Decoder {
    /// Umpan byte potongan; kembalikan teks yang sudah pasti lengkap.
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.carry.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.carry) {
                Ok(s) => {
                    out.push_str(s);
                    self.carry.clear();
                    return out;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    if valid > 0 {
                        out.push_str(std::str::from_utf8(&self.carry[..valid]).unwrap_or(""));
                    }
                    match e.error_len() {
                        // Urutan belum lengkap di ujung buffer: simpan, tunggu.
                        None => {
                            self.carry.drain(..valid);
                            return out;
                        }
                        // Byte benar-benar tidak valid: ganti, lewati.
                        Some(bad) => {
                            out.push('\u{FFFD}');
                            self.carry.drain(..valid + bad);
                            if self.carry.is_empty() {
                                return out;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Byte sisa saat proses berakhir (mis. urutan terpotong di ujung).
    pub fn flush(&mut self) -> String {
        let out = String::from_utf8_lossy(&self.carry).to_string();
        self.carry.clear();
        out
    }
}

/// Batas yang dipakai satu eksekusi.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Jumlah karakter maksimum yang disimpan/diteruskan. Kelebihannya dibuang
    /// supaya satu perintah beroutput raksasa tidak menghabiskan memori.
    pub max_chars: usize,
    /// `None` = tanpa batas waktu.
    pub timeout: Option<Duration>,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_chars: 40_000,
            timeout: None,
        }
    }
}

/// Ringkasan hasil pembacaan.
#[derive(Clone, Debug, Default)]
pub struct Collected {
    pub output: String,
    pub canceled: bool,
    pub timed_out: bool,
    pub truncated: bool,
}

fn absorb(
    chunk: &str,
    output: &mut String,
    truncated: &mut bool,
    max: usize,
    on_chunk: &mut dyn FnMut(&str),
) {
    if *truncated || chunk.is_empty() {
        return;
    }
    let used = output.chars().count();
    let room = max.saturating_sub(used);
    if chunk.chars().count() <= room {
        output.push_str(chunk);
        on_chunk(chunk);
    } else {
        let cut: String = chunk.chars().take(room).collect();
        if !cut.is_empty() {
            output.push_str(&cut);
            on_chunk(&cut);
        }
        *truncated = true;
    }
}

/// Baca semua pipe sampai habis, sambil memantau [cancel] dan `limits.timeout`.
///
/// `kill` dipanggil **paling banyak sekali** ketika batal/timeout terjadi;
/// pemanggil bertanggung jawab mematikan pohon proses (di Windows:
/// `TerminateJobObject`). Loop tidak menunggu lebih lama dari ±40 ms.
pub fn collect(
    pipes: Vec<Box<dyn Read + Send>>,
    cancel: &AtomicBool,
    limits: Limits,
    on_chunk: &mut dyn FnMut(&str),
    kill: &mut dyn FnMut(),
) -> Collected {
    let (tx, rx) = channel::<String>();
    let mut readers = Vec::new();
    for pipe in pipes {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let mut reader = BufReader::new(pipe);
            let mut decoder = Utf8Decoder::default();
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let text = decoder.push(&buf[..n]);
                        if !text.is_empty() && tx.send(text).is_err() {
                            break;
                        }
                    }
                }
            }
            let tail = decoder.flush();
            if !tail.is_empty() {
                let _ = tx.send(tail);
            }
        }));
    }
    // Salinan utama dibuang supaya channel tertutup saat semua thread selesai.
    drop(tx);

    let deadline = limits.timeout.map(|t| Instant::now() + t);
    let mut result = Collected::default();
    let mut killed_at: Option<Instant> = None;
    // `true` bila kita berhenti karena menyerah menunggu pipe, bukan karena
    // semua reader selesai. Saat itu thread reader sengaja tidak di-join.
    let mut abandoned = false;

    loop {
        match rx.recv_timeout(TICK) {
            Ok(chunk) => absorb(
                &chunk,
                &mut result.output,
                &mut result.truncated,
                limits.max_chars,
                on_chunk,
            ),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if !result.canceled && cancel.load(Ordering::Relaxed) {
            result.canceled = true;
        }
        if !result.timed_out {
            if let Some(d) = deadline {
                if Instant::now() >= d {
                    result.timed_out = true;
                }
            }
        }
        if (result.canceled || result.timed_out) && killed_at.is_none() {
            kill();
            killed_at = Some(Instant::now());
        }
        if let Some(at) = killed_at {
            if at.elapsed() >= GRACE_AFTER_KILL {
                // Proses sudah diminta mati tapi ada yang masih memegang pipe.
                abandoned = true;
                break;
            }
        }
    }

    if !abandoned {
        for r in readers {
            let _ = r.join();
        }
    }
    // Sisa yang masuk di antara "disconnected" dan join.
    if !abandoned {
        while let Ok(chunk) = rx.try_recv() {
            absorb(
                &chunk,
                &mut result.output,
                &mut result.truncated,
                limits.max_chars,
                on_chunk,
            );
        }
    }
    if result.truncated {
        result.output.push_str("\n… (output dipotong)");
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn decoder_keeps_emoji_split_across_chunks() {
        let text = "a🚀b日";
        let bytes = text.as_bytes();
        let mut d = Utf8Decoder::default();
        let mut out = String::new();
        // Umpan satu byte per potongan: kasus terburuk.
        for b in bytes {
            out.push_str(&d.push(&[*b]));
        }
        out.push_str(&d.flush());
        assert_eq!(out, text);
    }

    #[test]
    fn decoder_replaces_invalid_bytes_not_whole_chunk() {
        let mut d = Utf8Decoder::default();
        let out = d.push(b"ok\xFFok");
        assert!(out.starts_with("ok"));
        assert!(out.contains('\u{FFFD}'));
        assert!(out.ends_with("ok"));
    }

    #[test]
    fn decoder_is_empty_for_plain_ascii() {
        let mut d = Utf8Decoder::default();
        assert_eq!(d.push(b"hello"), "hello");
        assert_eq!(d.flush(), "");
    }

    #[cfg(unix)]
    fn sh(cmd: &str) -> std::process::Child {
        Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("sh harus tersedia")
    }

    #[cfg(unix)]
    #[test]
    fn collect_merges_stdout_and_stderr() {
        let mut child = sh("printf out; printf err 1>&2");
        let pipes: Vec<Box<dyn Read + Send>> = vec![
            Box::new(child.stdout.take().unwrap()),
            Box::new(child.stderr.take().unwrap()),
        ];
        let cancel = AtomicBool::new(false);
        let mut chunks = 0;
        let out = collect(
            pipes,
            &cancel,
            Limits::default(),
            &mut |_| chunks += 1,
            &mut || {},
        );
        let _ = child.wait();
        assert!(out.output.contains("out"));
        assert!(out.output.contains("err"));
        assert!(!out.canceled && !out.timed_out && !out.truncated);
        assert!(chunks >= 1);
    }

    #[cfg(unix)]
    #[test]
    fn collect_stops_when_canceled_and_calls_kill_once() {
        let mut child = sh("printf mulai; sleep 10; printf selesai");
        let out_pipe = child.stdout.take().unwrap();
        let err_pipe = child.stderr.take().unwrap();
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let mut kills = 0;
        let res = collect(
            vec![Box::new(out_pipe), Box::new(err_pipe)],
            &cancel,
            Limits {
                max_chars: 10_000,
                timeout: None,
            },
            &mut |_| {},
            &mut || {
                kills += 1;
                let _ = child.kill();
            },
        );
        let _ = child.wait();
        assert!(res.canceled, "harus ditandai batal");
        assert_eq!(kills, 1, "kill hanya sekali");
        assert!(res.output.contains("mulai"));
        assert!(!res.output.contains("selesai"));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "tidak boleh menunggu sleep 10"
        );
    }

    #[cfg(unix)]
    #[test]
    fn collect_marks_timeout() {
        let mut child = sh("sleep 10");
        let pipes: Vec<Box<dyn Read + Send>> = vec![
            Box::new(child.stdout.take().unwrap()),
            Box::new(child.stderr.take().unwrap()),
        ];
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        let res = collect(
            pipes,
            &cancel,
            Limits {
                max_chars: 1000,
                timeout: Some(Duration::from_millis(150)),
            },
            &mut |_| {},
            &mut || {
                let _ = child.kill();
            },
        );
        let _ = child.wait();
        assert!(res.timed_out);
        assert!(!res.canceled);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn collect_truncates_at_max_chars() {
        let mut child = sh("printf 'abcdefghij'");
        let pipes: Vec<Box<dyn Read + Send>> = vec![Box::new(child.stdout.take().unwrap())];
        let cancel = AtomicBool::new(false);
        let res = collect(
            pipes,
            &cancel,
            Limits {
                max_chars: 4,
                timeout: None,
            },
            &mut |_| {},
            &mut || {},
        );
        let _ = child.wait();
        assert!(res.truncated);
        assert!(res.output.starts_with("abcd"));
        assert!(res.output.contains("dipotong"));
    }

    #[cfg(unix)]
    #[test]
    fn collect_preserves_emoji_from_real_process() {
        // Emoji ditulis sebagai byte mentah supaya tidak bergantung locale sh.
        let mut child = sh("printf '\\360\\237\\232\\200 selesai'");
        let pipes: Vec<Box<dyn Read + Send>> = vec![Box::new(child.stdout.take().unwrap())];
        let cancel = AtomicBool::new(false);
        let res = collect(pipes, &cancel, Limits::default(), &mut |_| {}, &mut || {});
        let _ = child.wait();
        assert!(res.output.starts_with('🚀'), "output: {:?}", res.output);
    }
}
