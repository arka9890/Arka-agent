//! Bagian yang benar-benar menyentuh OS: membuat proses dan mematikannya.
//!
//! Ini satu-satunya berkas yang berisi FFI Windows (±100 baris). Semua logika
//! lain (`script`, `encode`, `pump`, `pwsh`) tidak bergantung pada OS supaya
//! bisa di-`cargo check`/`test` di Linux — penting karena CI Linux tidak bisa
//! mengompilasi target Windows (`ring` butuh `cl.exe`).
//!
//! ## Kenapa Job Object, bukan `child.kill()`
//! `process kill` hanya mematikan proses induk. `pwsh -EncodedCommand` biasanya
//! menjalankan program lain (`git`, `python`, `npm`) yang punya anak lagi;
//! setelah induk mati, anak-anak itu **tetap hidup** dan kadang mengunci file.
//! Job Object dengan `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` membuat Windows
//! mematikan seluruh pohon saat `TerminateJobObject` dipanggil.
//!
//! Batasnya: penempelan ke Job Object terjadi setelah proses dibuat (Rust std
//! tidak memberi handle thread untuk `CREATE_SUSPENDED`), jadi proses cucu yang
//! lahir dalam beberapa mikrodetik pertama secara teori bisa lolos. Untuk
//! shell interaktif ini praktis tidak terjadi.

use std::path::Path;

#[cfg(windows)]
mod imp {
    use super::Path;
    use std::io::Read;
    use std::os::windows::io::AsRawHandle;
    use std::os::windows::process::CommandExt;
    use std::process::{Child, Command, ExitStatus, Stdio};
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    /// `CREATE_NO_WINDOW`: jangan tampilkan jendela konsol yang berkedip.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// Pembungkus handle Job Object; menutup handle = mematikan sisa proses.
    struct Job(HANDLE);

    impl Job {
        fn create() -> Option<Job> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if handle.is_null() {
                    return None;
                }
                let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
                // LimitFlags: Windows mematikan semua anggota job saat handle
                // terakhir ditutup → tidak ada proses yatim kalau app crash.
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                let ok = SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                );
                if ok == FALSE {
                    CloseHandle(handle);
                    return None;
                }
                Some(Job(handle))
            }
        }

        fn assign(&self, child: &Child) -> bool {
            unsafe { AssignProcessToJobObject(self.0, child.as_raw_handle() as HANDLE) != FALSE }
        }

        fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    /// Proses yang sedang berjalan + job yang menaunginya.
    pub struct Proc {
        child: Child,
        job: Option<Job>,
        /// Apakah proses berhasil dimasukkan ke job (dilaporkan di status).
        pub attached: bool,
    }

    impl Proc {
        pub fn take_pipes(&mut self) -> Vec<Box<dyn Read + Send>> {
            let mut pipes: Vec<Box<dyn Read + Send>> = Vec::new();
            if let Some(out) = self.child.stdout.take() {
                pipes.push(Box::new(out));
            }
            if let Some(err) = self.child.stderr.take() {
                pipes.push(Box::new(err));
            }
            pipes
        }

        /// Matikan pohon proses: job dulu (anak-cucu), lalu child sebagai jaring
        /// pengaman kalau job gagal dibuat.
        pub fn kill(&mut self) {
            if let Some(job) = &self.job {
                job.terminate();
            }
            let _ = self.child.kill();
        }

        pub fn wait(&mut self) -> Result<ExitStatus, String> {
            self.child
                .wait()
                .map_err(|e| format!("gagal menunggu proses: {e}"))
        }
    }

    /// Buat proses PowerShell tersembunyi dengan cwd tertentu.
    pub fn spawn(program: &Path, args: &[String], cwd: &Path) -> Result<Proc, String> {
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW);
        let child = cmd
            .spawn()
            .map_err(|e| format!("gagal menjalankan {}: {e}", program.display()))?;
        // Job dibuat **setelah** spawn karena tidak perlu ada sebelum itu;
        // kegagalan di sini tidak fatal (kill fallback tetap jalan).
        let job = Job::create();
        let attached = job.as_ref().map(|j| j.assign(&child)).unwrap_or(false);
        Ok(Proc {
            child,
            job,
            attached,
        })
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Path;
    use std::io::Read;

    /// Stub untuk Linux/macOS: cukup untuk membuat modul lain lolos
    /// `cargo check`, tapi tidak pernah dipakai (spawn selalu error).
    pub struct Proc {
        pub attached: bool,
    }

    impl Proc {
        pub fn take_pipes(&mut self) -> Vec<Box<dyn Read + Send>> {
            Vec::new()
        }
        pub fn kill(&mut self) {}
        pub fn wait(&mut self) -> Result<std::process::ExitStatus, String> {
            Err("platform ini tidak punya Windows API".into())
        }
    }

    pub fn spawn(program: &Path, _args: &[String], _cwd: &Path) -> Result<Proc, String> {
        Err(format!(
            "{} hanya bisa dijalankan di Windows (PowerShell tidak tersedia di sini)",
            program.display()
        ))
    }
}

pub use imp::{spawn, Proc};

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    #[test]
    fn spawn_reports_platform_error_instead_of_panicking() {
        let err = match spawn(Path::new("pwsh.exe"), &[], Path::new(".")) {
            Ok(_) => panic!("spawn tidak boleh berhasil di platform ini"),
            Err(e) => e,
        };
        assert!(err.contains("Windows"), "pesan: {err}");
    }
}
