//! Pekerjaan tab Terminal di thread latar.
//!
//! ## Kenapa tidak langsung memanggil `Shell::exec` dari `draw`
//! PowerShell adalah proses terpisah: `git clone` atau `npm install` berjalan
//! menit-menit. Memanggilnya di dalam `draw` membekukan jendela selama itu —
//! termasuk tombol Stop, sehingga pengguna tidak bisa menghentikan perintah
//! yang sedang berjalan. Pola yang dipakai sama dengan [`super::engine`] dan
//! [`super::jobs`]: thread sendiri + saluran + satu `poll` per frame.
//!
//! ## Kenapa ada trait [`Runner`]
//! Karena `PwshShell` hanya ada di Windows, sedangkan logika di berkas ini
//! (akumulasi potongan output, urutan exit code, pembatalan, batas waktu)
//! harus bisa diuji di CI Linux. Trait ini juga mencegah test tidak sengaja
//! menjalankan PowerShell sungguhan.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;

use arka_core::shell::{ExecOutcome, Shell};
use eframe::egui::Context;

use super::jobs::Lang3;

/// Jenis pekerjaan yang dijalankan di latar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskKind {
    /// Perintah yang diketik pengguna di kotak perintah.
    Run,
    /// Versi PowerShell, OS, arsitektur, ExecutionPolicy.
    Info,
    /// Tombol "Tes shell".
    Diagnostic,
}

/// Kemampuan yang dibutuhkan satu pekerjaan terminal.
///
/// Produksi: [`crate::pwsh::PwshShell`]. Tes: peniru di modul `tests`.
pub trait Runner: Send + 'static {
    fn exec(
        &self,
        cmd: &str,
        cancel: &AtomicBool,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<ExecOutcome, String>;
    fn info(&self) -> Result<Vec<(String, String)>, String>;
    fn diagnostic(&self) -> Result<String, String>;
}

impl Runner for crate::pwsh::PwshShell {
    fn exec(
        &self,
        cmd: &str,
        cancel: &AtomicBool,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<ExecOutcome, String> {
        // Wajib disebut lengkap: `self.exec(...)` akan memanggil method `Runner`
        // ini sendiri dan berputar tanpa henti.
        Shell::exec(self, cmd, cancel, on_chunk)
    }

    fn info(&self) -> Result<Vec<(String, String)>, String> {
        crate::pwsh::PwshShell::info(self)
    }

    fn diagnostic(&self) -> Result<String, String> {
        Shell::diagnostic(self)
    }
}

/// Satu kejadian dari thread ke UI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobEvent {
    /// Potongan output, dikirim saat proses masih berjalan.
    Chunk(String),
    Done {
        code: i32,
        canceled: bool,
        timed_out: bool,
    },
    Info(Vec<(String, String)>),
    Diagnostic(String),
    Failed(String),
}

/// Baris exit yang ditampilkan di bawah output.
///
/// Dipisah dari [`ExecOutcome`] supaya bisa diuji & diterjemahkan tanpa
/// menjalankan proses: yang perlu diuji hanyalah pemetaan
/// (code, canceled, timed_out) → teks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExitLine {
    pub code: i32,
    pub canceled: bool,
    pub timed_out: bool,
}

impl ExitLine {
    pub fn new(code: i32, canceled: bool, timed_out: bool) -> Self {
        Self {
            code,
            canceled,
            timed_out,
        }
    }

    /// `true` bila perintah dianggap berhasil (bukan batal, bukan timeout,
    /// exit code 0).
    pub fn ok(&self) -> bool {
        !self.canceled && !self.timed_out && self.code == 0
    }

    pub fn text(&self, lang: Lang3) -> String {
        if self.canceled {
            match lang {
                Lang3::Id => format!("exit {} \u{00B7} dihentikan pengguna", self.code),
                Lang3::En => format!("exit {} \u{00B7} stopped by user", self.code),
                Lang3::Ja => format!(
                    "exit {} \u{00B7} \u{30e6}\u{30fc}\u{30b6}\u{30fc}\u{304c}\u{505c}\u{6b62}",
                    self.code
                ),
            }
        } else if self.timed_out {
            match lang {
                Lang3::Id => format!("exit {} \u{00B7} timeout", self.code),
                Lang3::En => format!("exit {} \u{00B7} timed out", self.code),
                Lang3::Ja => format!(
                    "exit {} \u{00B7} \u{30bf}\u{30a4}\u{30e0}\u{30a2}\u{30a6}\u{30c8}",
                    self.code
                ),
            }
        } else {
            format!("exit {}", self.code)
        }
    }
}

/// Keadaan pekerjaan.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum JobState {
    #[default]
    Running,
    Done,
    Failed(String),
}

/// Pekerjaan terminal yang sedang berjalan. Pemiliknya `TermState`.
pub struct Job {
    pub kind: TaskKind,
    /// Perintah yang dijalankan (kosong untuk Info/Diagnostic).
    pub cmd: String,
    /// Output yang sudah diterima. Selalu terisi walau pekerjaan gagal —
    /// pesan galat tanpa output sebelumnya tidak bisa didiagnosis pengguna.
    pub output: String,
    pub state: JobState,
    pub exit: Option<ExitLine>,
    pub info: Option<Vec<(String, String)>>,
    pub diagnostic: Option<String>,
    rx: Receiver<JobEvent>,
    cancel: Arc<AtomicBool>,
}

impl Job {
    pub fn running(&self) -> bool {
        matches!(self.state, JobState::Running)
    }

    /// Minta proses berhenti. Tidak menunggu: status baru berubah saat
    /// thread mengirim `Done`.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    /// Ambil semua kejadian yang menunggu. `true` = ada yang berubah.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        loop {
            match self.rx.try_recv() {
                Ok(ev) => {
                    changed = true;
                    self.apply(ev);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // Thread mati (panic) tanpa mengirim hasil. Tanpa ini,
                    // tombol akan berhenti di "Berjalan…" selamanya.
                    if self.running() {
                        self.state = JobState::Failed("Proses berhenti tanpa hasil".into());
                        changed = true;
                    }
                    break;
                }
            }
        }
        changed
    }

    /// Job yang tampak "sedang berjalan" tanpa proses apa pun — **hanya untuk
    /// test UI**. Sender-nya sengaja dibocorkan (`mem::forget`) supaya saluran
    /// tidak tertutup dan `poll` tidak mengubahnya jadi `Failed`.
    #[cfg(test)]
    pub fn running_stub(kind: TaskKind, cmd: &str) -> Job {
        let (tx, rx) = mpsc::channel();
        std::mem::forget(tx);
        Job {
            kind,
            cmd: cmd.to_string(),
            output: "setengah jalan".into(),
            state: JobState::Running,
            exit: None,
            info: None,
            diagnostic: None,
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    fn apply(&mut self, ev: JobEvent) {
        match ev {
            JobEvent::Chunk(s) => self.output.push_str(&s),
            JobEvent::Done {
                code,
                canceled,
                timed_out,
            } => {
                self.exit = Some(ExitLine::new(code, canceled, timed_out));
                self.state = JobState::Done;
            }
            JobEvent::Info(v) => {
                self.info = Some(v);
                self.state = JobState::Done;
            }
            JobEvent::Diagnostic(s) => {
                self.diagnostic = Some(s);
                self.state = JobState::Done;
            }
            JobEvent::Failed(e) => self.state = JobState::Failed(e),
        }
    }
}

/// Jalankan [kind] di thread baru.
pub fn start(kind: TaskKind, cmd: String, runner: impl Runner, ctx: Context) -> Job {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel2 = cancel.clone();
    let cmd2 = cmd.clone();

    std::thread::spawn(move || {
        let tx2 = tx.clone();
        let ctx2 = ctx.clone();
        let result = match kind {
            TaskKind::Run => runner
                .exec(&cmd2, &cancel2, &mut |chunk| {
                    // Potongan output dikirim secepat proses menghasilkannya →
                    // `git clone` terlihat bergerak, bukan menunggu selesai.
                    let _ = tx2.send(JobEvent::Chunk(chunk.to_string()));
                    ctx2.request_repaint();
                })
                .map(|o| JobEvent::Done {
                    code: o.code,
                    canceled: o.canceled,
                    timed_out: o.timed_out,
                }),
            TaskKind::Info => runner.info().map(JobEvent::Info),
            TaskKind::Diagnostic => runner.diagnostic().map(JobEvent::Diagnostic),
        };
        let _ = tx.send(match result {
            Ok(ev) => ev,
            Err(e) => JobEvent::Failed(e),
        });
        ctx.request_repaint();
    });

    Job {
        kind,
        cmd,
        output: String::new(),
        state: JobState::Running,
        exit: None,
        info: None,
        diagnostic: None,
        rx,
        cancel,
    }
}

/// Ukuran & jumlah berkas workspace.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WsStats {
    pub files: usize,
    pub bytes: i64,
}

/// Hitung isi workspace di thread latar.
///
/// Ini **bukan** mikro-optimasi: `C:\Users\...\workspace` bisa berisi
/// `node_modules` puluhan ribu berkas, dan menghitungnya di `draw` akan
/// membekukan jendela setiap kali tab Terminal dibuka.
pub struct StatsJob {
    rx: Receiver<WsStats>,
    done: bool,
}

impl StatsJob {
    pub fn poll(&mut self) -> Option<WsStats> {
        if self.done {
            return None;
        }
        match self.rx.try_recv() {
            Ok(v) => {
                self.done = true;
                Some(v)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.done = true;
                None
            }
        }
    }
}

pub fn start_stats(path: PathBuf, ctx: Context) -> StatsJob {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let stats = WsStats {
            files: crate::pwsh::dir_files(&path),
            bytes: crate::pwsh::dir_size(&path),
        };
        let _ = tx.send(stats);
        ctx.request_repaint();
    });
    StatsJob { rx, done: false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Peniru `PwshShell`: tidak ada proses, perilaku ditentukan test.
    struct Fake {
        chunks: Vec<String>,
        outcome: Result<ExecOutcome, String>,
        info: Vec<(String, String)>,
        /// Tunggu sampai dibatalkan, lalu laporkan `canceled`.
        wait_for_cancel: bool,
    }

    impl Fake {
        fn ok(code: i32, output: &str) -> Self {
            Self {
                chunks: vec![output.to_string()],
                outcome: Ok(ExecOutcome {
                    code,
                    ..Default::default()
                }),
                info: Vec::new(),
                wait_for_cancel: false,
            }
        }
    }

    impl Runner for Fake {
        fn exec(
            &self,
            _cmd: &str,
            cancel: &AtomicBool,
            on_chunk: &mut dyn FnMut(&str),
        ) -> Result<ExecOutcome, String> {
            for c in &self.chunks {
                if self.wait_for_cancel {
                    let deadline = Instant::now() + Duration::from_secs(5);
                    while Instant::now() < deadline {
                        if cancel.load(Ordering::Relaxed) {
                            return Ok(ExecOutcome {
                                code: -1,
                                canceled: true,
                                ..Default::default()
                            });
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    return Err("test: pembatalan tidak pernah datang".into());
                }
                on_chunk(c);
            }
            self.outcome.clone()
        }

        fn info(&self) -> Result<Vec<(String, String)>, String> {
            Ok(self.info.clone())
        }

        fn diagnostic(&self) -> Result<String, String> {
            Ok("diagnostik".into())
        }
    }

    /// Tunggu sampai `pred` benar; kembalikan `false` kalau melewati batas.
    fn tunggu(job: &mut Job, pred: impl Fn(&Job) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            job.poll();
            if pred(job) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn potongan_output_diakumulasi_lalu_exit_code_dilaporkan() {
        let ctx = Context::default();
        let fake = Fake {
            chunks: vec!["a".into(), "b".into(), "c".into()],
            outcome: Ok(ExecOutcome {
                code: 0,
                ..Default::default()
            }),
            info: Vec::new(),
            wait_for_cancel: false,
        };
        let mut job = start(TaskKind::Run, "Get-ChildItem".into(), fake, ctx);

        assert!(tunggu(&mut job, |j| !j.running()));
        assert_eq!(job.output, "abc");
        assert_eq!(job.exit, Some(ExitLine::new(0, false, false)));
        assert!(job.exit.unwrap().ok());
    }

    #[test]
    fn exit_code_bukan_nol_bukan_berhasil() {
        let ctx = Context::default();
        let fake = Fake::ok(128, "fatal: not a git repository");
        let mut job = start(TaskKind::Run, "git status".into(), fake, ctx);

        assert!(tunggu(&mut job, |j| !j.running()));
        let exit = job.exit.unwrap();
        assert_eq!(exit.code, 128);
        assert!(!exit.ok(), "exit 128 harus terlihat sebagai gagal");
        assert_eq!(exit.text(Lang3::En), "exit 128");
    }

    #[test]
    fn pembatalan_menghentikan_perintah_dan_ditandai() {
        let ctx = Context::default();
        let fake = Fake {
            chunks: vec!["x".into()],
            outcome: Ok(ExecOutcome::default()),
            info: Vec::new(),
            wait_for_cancel: true,
        };
        let mut job = start(TaskKind::Run, "ping -t 1.1.1.1".into(), fake, ctx);

        // Tunggu sampai thread benar-benar menunggu, lalu batalkan.
        assert!(tunggu(&mut job, |j| j.running()));
        job.cancel();

        assert!(
            tunggu(&mut job, |j| !j.running()),
            "Stop harus mengakhiri pekerjaan"
        );
        let exit = job.exit.unwrap();
        assert!(exit.canceled);
        assert_eq!(exit.code, -1);
        assert!(!exit.ok());
        assert!(exit.text(Lang3::Id).contains("dihentikan pengguna"));
    }

    #[test]
    fn kegagalan_spawn_mempertahankan_output_sebelumnya() {
        let fake = Fake {
            chunks: Vec::new(),
            outcome: Err("PowerShell tidak ditemukan".into()),
            info: Vec::new(),
            wait_for_cancel: false,
        };
        let mut job = start(TaskKind::Run, "dir".into(), fake, Context::default());

        assert!(tunggu(&mut job, |j| !j.running()));
        assert_eq!(
            job.state,
            JobState::Failed("PowerShell tidak ditemukan".into())
        );
        assert_eq!(job.exit, None);
    }

    #[test]
    fn timeout_dilaporkan_sebagai_timeout_bukan_gagal() {
        let ctx = Context::default();
        let fake = Fake {
            chunks: vec!["sebagian".into()],
            outcome: Ok(ExecOutcome {
                code: -1,
                timed_out: true,
                ..Default::default()
            }),
            info: Vec::new(),
            wait_for_cancel: false,
        };
        let mut job = start(TaskKind::Run, "sleep".into(), fake, ctx);

        assert!(tunggu(&mut job, |j| !j.running()));
        assert_eq!(
            job.output, "sebagian",
            "output yang sudah keluar tidak boleh hilang"
        );
        let exit = job.exit.unwrap();
        assert!(exit.timed_out);
        assert!(!exit.ok());
        assert!(exit.text(Lang3::Id).contains("timeout"));
    }

    #[test]
    fn thread_mati_tanpa_hasil_tidak_menunggu_selamanya() {
        let (tx, rx) = mpsc::channel::<JobEvent>();
        drop(tx);
        let mut job = Job {
            kind: TaskKind::Run,
            cmd: "x".into(),
            output: String::new(),
            state: JobState::Running,
            exit: None,
            info: None,
            diagnostic: None,
            rx,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        assert!(job.poll(), "harus melaporkan perubahan");
        assert!(!job.running());
        assert!(matches!(job.state, JobState::Failed(_)));
        assert!(!job.poll(), "tidak ada perubahan kedua");
    }

    #[test]
    fn info_dan_diagnostik_lewat_jalur_yang_sama() {
        let ctx = Context::default();
        let fake = Fake {
            chunks: Vec::new(),
            outcome: Ok(ExecOutcome::default()),
            info: vec![("PSVersion".into(), "7.4.6".into())],
            wait_for_cancel: false,
        };
        let mut job = start(TaskKind::Info, String::new(), fake, ctx.clone());
        assert!(tunggu(&mut job, |j| !j.running()));
        assert_eq!(job.info.as_ref().unwrap()[0].0, "PSVersion");
        assert_eq!(job.exit, None, "Info bukan eksekusi perintah");

        let fake = Fake {
            chunks: Vec::new(),
            outcome: Ok(ExecOutcome::default()),
            info: Vec::new(),
            wait_for_cancel: false,
        };
        let mut job = start(TaskKind::Diagnostic, String::new(), fake, ctx);
        assert!(tunggu(&mut job, |j| !j.running()));
        assert_eq!(job.diagnostic.as_deref(), Some("diagnostik"));
    }

    // ── Bukti di Windows sungguhan ────────────────────────────────────────
    //
    // Test di atas memakai `Fake`, jadi mereka membuktikan logika berkas ini
    // tetapi **tidak** membuktikan bahwa `PwshShell` sungguhan benar-benar
    // mengalirkan output selagi berjalan. Tiga test di bawah hanya berjalan di
    // `windows-latest` (CI) dan memakai PowerShell yang sesungguhnya.
    #[cfg(windows)]
    mod windows {
        use super::*;

        fn shell() -> crate::pwsh::PwshShell {
            let d = crate::pwsh::detect().expect("PowerShell harus ada di windows-latest");
            let ws = std::env::temp_dir().join(format!("arka-term-{}", std::process::id()));
            std::fs::create_dir_all(&ws).unwrap();
            crate::pwsh::PwshShell::with(
                d.program,
                ws,
                crate::pump::Limits {
                    max_chars: crate::pwsh::RAW_OUTPUT_CAP,
                    timeout: Some(Duration::from_secs(60)),
                },
            )
        }

        /// Kriteria "Selesai bila" M7: output terlihat **real-time**, bukan
        /// setelah perintah selesai. Diperiksa dengan mencatat keadaan pada
        /// saat output pertama muncul.
        #[test]
        fn output_terlihat_selagi_perintah_masih_berjalan() {
            let cmd = "1..3 | ForEach-Object { \"baris-$_\"; Start-Sleep -Milliseconds 300 }";
            let mut job = start(TaskKind::Run, cmd.to_string(), shell(), Context::default());

            // Tangkap momen output pertama muncul: di situ perintah harus
            // masih berjalan (kalau tidak, output baru sampai setelah selesai).
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut saw_while_running = false;
            while Instant::now() < deadline {
                job.poll();
                if !job.output.trim().is_empty() {
                    saw_while_running = job.running();
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                saw_while_running,
                "output harus sudah terlihat sebelum perintah selesai; output={:?}",
                job.output
            );

            assert!(
                tunggu(&mut job, |j| !j.running()),
                "perintah tidak pernah selesai"
            );
            for i in 1..=3 {
                assert!(
                    job.output.contains(&format!("baris-{i}")),
                    "baris-{i} hilang dari output: {:?}",
                    job.output
                );
            }
            let exit = job.exit.unwrap();
            assert_eq!(exit.code, 0);
            assert!(exit.ok());
        }

        /// Exit code native harus sampai ke UI: `exit 7` tidak boleh tampil
        /// sebagai sukses.
        #[test]
        fn exit_code_native_sampai_ke_ui() {
            let mut job = start(
                TaskKind::Run,
                "cmd /c exit 7".to_string(),
                shell(),
                Context::default(),
            );
            assert!(tunggu(&mut job, |j| !j.running()));
            let exit = job.exit.expect("exit line harus terbentuk");
            assert_eq!(exit.code, 7);
            assert!(!exit.ok(), "exit 7 bukan sukses");
            assert!(!exit.text(Lang3::Id).contains("dihentikan"));
        }

        /// Tombol Stop pada jalur UI: bukan hanya membunuh `pwsh`, tetapi
        /// membuat `Done` benar-benar datang (kalau Job Object gagal, test ini
        /// menunggu 30 detik dan gagal karena tenggat 10 detik).
        #[test]
        fn stop_menghentikan_perintah_panjang() {
            let mut job = start(
                TaskKind::Run,
                "Start-Sleep -Seconds 30".to_string(),
                shell(),
                Context::default(),
            );
            // Biarkan proses benar-benar hidup dulu, supaya yang dibatalkan
            // bukan proses yang belum sempat dibuat.
            let t_mulai = Instant::now();
            while t_mulai.elapsed() < Duration::from_millis(300) {
                job.poll();
                std::thread::sleep(Duration::from_millis(20));
            }
            let t0 = Instant::now();
            job.cancel();
            assert!(
                tunggu(&mut job, |j| !j.running()),
                "perintah 30 detik tidak berhenti setelah Stop"
            );
            assert!(
                t0.elapsed() < Duration::from_secs(10),
                "Stop butuh {:?}, terlalu lama",
                t0.elapsed()
            );
            let exit = job.exit.expect("exit line harus terbentuk");
            assert!(
                exit.canceled,
                "harus dilaporkan sebagai dihentikan pengguna"
            );
            assert!(!exit.ok());
            assert!(exit.text(Lang3::Id).contains("dihentikan pengguna"));
        }
    }

    #[test]
    fn stats_menghitung_berkas_sungguhan_di_thread() {
        let dir = std::env::temp_dir().join(format!("arka-stats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"12345").unwrap();
        std::fs::write(dir.join("sub/b.txt"), b"67").unwrap();

        let mut job = start_stats(dir.clone(), Context::default());
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got = None;
        while got.is_none() && Instant::now() < deadline {
            got = job.poll();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, Some(WsStats { files: 2, bytes: 7 }));
        assert!(job.poll().is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
