//! PRoot + Ubuntu 24: unduh rootfs, pasang tools, jalankan perintah.
//!
//! Catatan Android yang menentukan bentuk kode ini:
//!  - Sejak Android 10 file di folder data aplikasi tidak boleh di-exec
//!    (SELinux/W^X). Karena itu binary proot + loader dibundel di APK sebagai
//!    `libproot.so` / `libproot-loader.so` dan diambil dari `nativeLibraryDir`.
//!  - proot ditautkan ke `libtalloc.so.2`, jadi perlu symlink di folder yang
//!    bisa dibaca loader.
//!  - `/dev`, `/proc`, `/sys` harus di-bind, kalau tidak `apt` gagal.

use crate::ctx;
use crate::jni_util as ju;
use jni::objects::JObject;
use jni::JNIEnv;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Rootfs resmi Ubuntu 24.04 (ubuntu-base, ±30 MB terkompresi / ±110 MB
/// terpasang). Dipilih daripada proot-distro karena ukurannya jauh lebih kecil
/// dan isinya sudah lengkap untuk apt.
pub const ROOTFS_URL: &str =
    "https://cdimage.ubuntu.com/ubuntu-base/releases/24.04/release/ubuntu-base-24.04.5-base-arm64.tar.gz";
/// SHA-256 resmi dari `SHA256SUMS` di folder yang sama.
pub const ROOTFS_SHA256: &str =
    "a91d5a93010193712d346d761372b7c9db6dfcf093893161c64ca107f05914f2";

/// Paket yang dipasang otomatis setelah rootfs siap.
pub const TOOLS: &[&str] = &["python3", "git", "curl", "wget", "nano", "unzip", "less", "ca-certificates"];

#[derive(Serialize, Clone, Debug)]
pub struct DistroStatus {
    pub installed: bool,
    pub proot_available: bool,
    pub size_bytes: i64,
    pub rootfs_path: String,
    pub message: String,
}

// --------------------------------------------------------------- status

pub fn is_installed() -> bool {
    rootfs_marker().map(|p| p.is_file()).unwrap_or(false)
}

fn rootfs_marker() -> Result<PathBuf, String> {
    Ok(ctx::rootfs_dir()?.join("etc/os-release"))
}

pub fn status() -> Result<DistroStatus, String> {
    let installed = is_installed();
    let proot_available = ctx::proot_binary()?.is_some() && ctx::loader_binary()?.is_some();
    let size = if installed { dir_size(&ctx::rootfs_dir()?) } else { 0 };
    let message = if !proot_available {
        "Binary proot tidak ada di APK ini, jadi terminal tidak bisa dipakai. \
         Pastikan libproot.so + libproot-loader.so ada di jniLibs saat build."
            .to_string()
    } else if !installed {
        "Ubuntu 24 belum dipasang. Unduh ±30 MB untuk mengaktifkan terminal dan tools.".to_string()
    } else {
        format!("Ubuntu 24 siap ({} MB).", size / (1024 * 1024))
    };
    Ok(DistroStatus {
        installed,
        proot_available,
        size_bytes: size,
        rootfs_path: ctx::rootfs_dir()?.to_string_lossy().to_string(),
        message,
    })
}

fn dir_size(path: &Path) -> i64 {
    let mut total = 0i64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let Ok(meta) = fs::symlink_metadata(e.path()) else { continue };
            if meta.is_dir() {
                stack.push(e.path());
            } else if meta.is_file() {
                total += meta.len() as i64;
            }
        }
    }
    total
}

// --------------------------------------------------------------- install

/// Unduh + ekstrak rootfs lalu jalankan `apt-get install` untuk [TOOLS].
///
/// [on_progress] menerima 0..100 dan label tahap. [cancel] di-set true oleh
/// tombol Batal di UI.
pub fn install(
    env: &mut JNIEnv,
    cb: &JObject,
    cancel: Arc<AtomicBool>,
) -> Result<(), String> {
    if is_installed() {
        ju::emit_int_str(env, cb, "onProgress", 100, "Ubuntu 24 sudah terpasang.");
        return Ok(());
    }
    let files = ctx::files_dir()?;
    let cache_dir = files.join("downloads");
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let archive = cache_dir.join("ubuntu-24.04-base-arm64.tar.gz");

    if !archive.is_file() || verify_sha256(&archive).unwrap_or(false) == false {
        download(env, cb, &cancel, &archive)?;
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Dibatalkan".into());
    }

    ju::emit_int_str(env, cb, "onProgress", 70, "Membongkar Ubuntu 24…");
    extract_rootfs(&archive)?;

    ju::emit_int_str(env, cb, "onProgress", 85, "Menyiapkan environment…");
    post_install()?;

    ju::emit_int_str(env, cb, "onProgress", 90, "Memasang tools (python3, git, curl…)…");
    match install_tools(env, cb, &cancel) {
        Ok(()) => ju::emit_int_str(env, cb, "onProgress", 100, "Ubuntu 24 siap dipakai."),
        // Tools gagal (mis. offline) tidak membatalkan pemasangan: rootfs tetap
        // berguna dan user bisa `apt install` sendiri dari Terminal.
        Err(e) => ju::emit_int_str(
            env,
            cb,
            "onProgress",
            100,
            &format!("Ubuntu 24 siap, tetapi tools gagal dipasang: {e}"),
        ),
    }
    Ok(())
}

fn verify_sha256(path: &Path) -> Result<bool, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()) == ROOTFS_SHA256)
}

fn download(env: &mut JNIEnv, cb: &JObject, cancel: &AtomicBool, dest: &Path) -> Result<(), String> {
    ju::emit_int_str(env, cb, "onProgress", 0, "Mengunduh Ubuntu 24…");
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let mut resp = client
        .get(ROOTFS_URL)
        .send()
        .map_err(|e| format!("Gagal mengunduh rootfs: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("Server rootfs mengembalikan HTTP {}", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(0);
    let part = dest.with_extension("part");
    let mut out = fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 64 * 1024];
    let mut written: u64 = 0;
    let mut last_pct = -1i32;
    loop {
        if cancel.load(Ordering::Relaxed) {
            drop(out);
            let _ = fs::remove_file(&part);
            return Err("Dibatalkan".into());
        }
        let n = resp.read(&mut buf).map_err(|e| format!("Unduhan terputus: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        written += n as u64;
        if total > 0 {
            // 0..65% untuk unduhan; sisanya ekstraksi + tools.
            let pct = ((written as f64 / total as f64) * 65.0) as i32;
            if pct != last_pct {
                last_pct = pct;
                ju::emit_int_str(
                    env,
                    cb,
                    "onProgress",
                    pct,
                    &format!(
                        "Mengunduh Ubuntu 24… {} / {} MB",
                        written / 1024 / 1024,
                        total / 1024 / 1024
                    ),
                );
            }
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);

    ju::emit_int_str(env, cb, "onProgress", 66, "Memeriksa checksum rootfs…");
    if !verify_sha256(&part)? {
        let _ = fs::remove_file(&part);
        return Err("Checksum rootfs tidak cocok — unduhan korup, silakan coba lagi.".into());
    }
    fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Ekstrak tar.gz ke staging lalu tukar atomik dengan rootfs aktif.
///
/// Ekstraksi langsung ke folder tujuan berbahaya: kalau unduhan/ekstraksi
/// gagal di tengah, rootfs setengah jadi dianggap "terpasang". Staging +
/// rename membuat rootfs hanya terlihat setelah utuh.
fn extract_rootfs(archive: &Path) -> Result<(), String> {
    let rootfs = ctx::rootfs_dir()?;
    let parent = rootfs.parent().ok_or("Path rootfs tidak valid")?.to_path_buf();
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let staging = parent.join(".staging-rootfs");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;

    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let gz = flate2::read::GzDecoder::new(BufReader::new(file));
    let mut archive_reader = tar::Archive::new(gz);
    let mut count = 0usize;
    let entries = archive_reader.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path().map_err(|e| e.to_string())?.to_path_buf();
        // Tolak entri yang mencoba keluar folder staging (tarball jahat/korup).
        if path.is_absolute() || path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            continue;
        }
        entry.unpack_in(&staging).map_err(|e| e.to_string())?;
        count += 1;
    }
    if count == 0 {
        return Err("Arsip rootfs kosong atau tidak bisa dibaca".into());
    }
    if !staging.join("etc/os-release").is_file() {
        return Err("Arsip bukan rootfs Ubuntu (etc/os-release tidak ditemukan)".into());
    }

    let _ = fs::remove_dir_all(&rootfs);
    fs::rename(&staging, &rootfs).map_err(|e| e.to_string())?;
    Ok(())
}

fn post_install() -> Result<(), String> {
    let rootfs = ctx::rootfs_dir()?;
    fs::create_dir_all(rootfs.join("root/storage")).map_err(|e| e.to_string())?;
    fs::create_dir_all(rootfs.join("tmp")).map_err(|e| e.to_string())?;
    let resolv = "nameserver 1.1.1.1\nnameserver 8.8.8.8\n";
    let hosts = "127.0.0.1 localhost\n::1 localhost\n";
    for etc in [rootfs.join("etc")] {
        fs::create_dir_all(&etc).map_err(|e| e.to_string())?;
        fs::write(etc.join("resolv.conf"), resolv).map_err(|e| e.to_string())?;
        fs::write(etc.join("hosts"), hosts).map_err(|e| e.to_string())?;
    }
    // Ubuntu base tidak membawa /root/.bashrc; buat supaya prompt terminal enak.
    let bashrc = rootfs.join("root/.bashrc");
    if !bashrc.exists() {
        fs::write(
            &bashrc,
            "export HOME=/root\nexport PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\n\
             export TERM=xterm-256color\ncd /root/storage 2>/dev/null || cd /root\n",
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn install_tools(env: &mut JNIEnv, cb: &JObject, cancel: &AtomicBool) -> Result<(), String> {
    let pkgs = TOOLS.join(" ");
    let script = format!(
        "export DEBIAN_FRONTEND=noninteractive; \
         apt-get update -qq && apt-get install -y -qq --no-install-recommends {pkgs} && \
         echo ARKA_TOOLS_OK"
    );
    let (code, out) = run_capture(&script, cancel, Some((env, cb)))?;
    if out.contains("ARKA_TOOLS_OK") {
        Ok(())
    } else {
        Err(format!("apt-get keluar dengan kode {code}: {}", tail(&out, 300)))
    }
}

fn tail(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(n);
    chars[start..].iter().collect()
}

// ---------------------------------------------------------------- run

/// Env untuk proses proot itu sendiri (bukan untuk guest).
fn proot_env() -> Result<Vec<(String, String)>, String> {
    let tmp = ctx::proot_tmp_dir()?;
    let libs = ctx::proot_libs_dir()?;
    if let Some(talloc) = ctx::native_lib("libtalloc.so")? {
        let link = libs.join("libtalloc.so.2");
        if !link.exists() {
            // Fallback ke copy: symlink bisa ditolak di beberapa perangkat.
            let _ = std::os::unix::fs::symlink(&talloc, &link)
                .or_else(|_| fs::copy(&talloc, &link).map(|_| ()));
        }
    }
    if let Some(shmem) = ctx::native_lib("libandroid-shmem.so")? {
        let link = libs.join("libandroid-shmem.so");
        if !link.exists() {
            let _ = std::os::unix::fs::symlink(&shmem, &link)
                .or_else(|_| fs::copy(&shmem, &link).map(|_| ()));
        }
    }
    let mut env = vec![
        ("PROOT_TMP_DIR".to_string(), tmp.to_string_lossy().to_string()),
        ("LD_LIBRARY_PATH".to_string(), libs.to_string_lossy().to_string()),
    ];
    if let Some(loader) = ctx::loader_binary()? {
        env.push(("PROOT_LOADER".to_string(), loader.to_string_lossy().to_string()));
    }
    Ok(env)
}

/// Rangkaian argumen proot untuk menjalankan [command] di dalam Ubuntu.
fn proot_argv(command: &str) -> Result<Vec<String>, String> {
    let proot = ctx::proot_binary()?.ok_or("Binary proot tidak ada di APK ini")?;
    if !is_installed() {
        return Err("Ubuntu 24 belum dipasang".into());
    }
    let rootfs = ctx::rootfs_dir()?;
    let workspace = ctx::workspace_dir()?;
    let loader = ctx::loader_binary()?.ok_or("Loader proot tidak ada di APK ini")?;
    let tmp = ctx::proot_tmp_dir()?;

    let mut argv: Vec<String> = vec![
        proot.to_string_lossy().to_string(),
        "--kill-on-exit".into(),
        // SELinux Android menolak link(); proot mengubahnya jadi symlink agar
        // dpkg/apt bisa memasang paket.
        "--link2symlink".into(),
        "-0".into(),
        "-r".into(),
        rootfs.to_string_lossy().to_string(),
        "-w".into(),
        "/root/storage".into(),
        "-b".into(),
        "/dev".into(),
        "-b".into(),
        "/proc".into(),
        "-b".into(),
        "/sys".into(),
        "-b".into(),
        format!("{}:/dev/shm", rootfs.join("tmp").to_string_lossy()),
        "-b".into(),
        format!("{}:/root/storage", workspace.to_string_lossy()),
    ];
    argv.push("/usr/bin/env".into());
    argv.push("-i".into());
    argv.push("HOME=/root".into());
    argv.push("PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into());
    argv.push("TERM=xterm-256color".into());
    argv.push("LANG=C.UTF-8".into());
    argv.push(format!("PROOT_LOADER={}", loader.to_string_lossy()));
    argv.push(format!("PROOT_TMP_DIR={}", tmp.to_string_lossy()));
    argv.push("/bin/bash".into());
    argv.push("-lc".into());
    argv.push(command.to_string());
    Ok(argv)
}

fn spawn_proot(command: &str) -> Result<std::process::Child, String> {
    let argv = proot_argv(command)?;
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    for (k, v) in proot_env()? {
        cmd.env(k, v);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    cmd.spawn().map_err(|e| {
        format!(
            "Gagal menjalankan proot: {e}. Pastikan libproot.so dan \
             libproot-loader.so ada di jniLibs saat build."
        )
    })
}

/// Jalankan perintah dan kembalikan (exit code, gabungan stdout+stderr).
pub fn run_capture(
    command: &str,
    cancel: &AtomicBool,
    ui: Option<(&mut JNIEnv, &JObject)>,
) -> Result<(i32, String), String> {
    let mut child = spawn_proot(command)?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Baca kedua pipe di thread terpisah: kalau hanya satu yang dibaca, proses
    // bisa deadlock ketika pipe yang lain penuh.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let mut readers = Vec::new();
    let pipes: Vec<Box<dyn Read + Send>> = [
        stdout.map(|p| Box::new(p) as Box<dyn Read + Send>),
        stderr.map(|p| Box::new(p) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .collect();
    for pipe in pipes {
        let tx = tx.clone();
        readers.push(std::thread::spawn(move || {
            let mut reader = BufReader::new(pipe);
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(String::from_utf8_lossy(&buf[..n]).to_string()).is_err() {
                            break;
                        }
                    }
                }
            }
        }));
    }
    drop(tx);

    let mut collected = String::new();
    let mut sink = ui;
    loop {
        match rx.recv_timeout(Duration::from_millis(120)) {
            Ok(chunk) => {
                collected.push_str(&chunk);
                if let Some((env, cb)) = sink.as_mut() {
                    ju::emit_str(env, cb, "onOutput", &chunk);
                }
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if cancel.load(Ordering::Relaxed) {
                    let _ = child.kill();
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    for r in readers {
        let _ = r.join();
    }
    // Drain sisa output setelah pipe tertutup.
    while let Ok(chunk) = rx.try_recv() {
        collected.push_str(&chunk);
        if let Some((env, cb)) = sink.as_mut() {
            ju::emit_str(env, cb, "onOutput", &chunk);
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Ok((-1, collected));
    }
    Ok((status.code().unwrap_or(-1), collected))
}

/// Jalankan perintah dengan output streaming ke Kotlin.
pub fn exec(env: &mut JNIEnv, cb: &JObject, command: &str, cancel: Arc<AtomicBool>) -> Result<i32, String> {
    if command.trim().is_empty() {
        return Err("Perintah kosong".into());
    }
    let (code, out) = run_capture(command, &cancel, Some((env, cb)))?;
    if code == -1 && cancel.load(Ordering::Relaxed) {
        ju::emit_str(env, cb, "onOutput", "\n^C dihentikan\n");
    } else if out.trim().is_empty() {
        ju::emit_str(env, cb, "onOutput", &format!("(perintah selesai, exit {code})\n"));
    }
    Ok(code)
}

/// Tes cepat: dipakai tombol "Tes distro" di Settings.
pub fn diagnostic() -> Result<String, String> {
    let cancel = AtomicBool::new(false);
    let (code, out) = run_capture(
        "cat /etc/os-release | head -2; uname -m; python3 --version 2>&1; git --version 2>&1; \
         ls -a /root/storage | head",
        &cancel,
        None,
    )?;
    Ok(format!("exit {code}\n{out}"))
}

/// Pastikan binary proot dari APK benar-benar executable.
///
/// File di `nativeLibraryDir` sudah 0755, tapi sebagian perangkat memakai
/// mount noexec untuk folder itu pada APK yang di-install dengan
/// `extractNativeLibs=false`. Cek ini dipanggil saat init supaya pesan
/// errornya jelas, bukan "permission denied" misterius.
pub fn check_executable() -> Result<(), String> {
    let Some(proot) = ctx::proot_binary()? else {
        return Err("Binary proot tidak ditemukan di APK.".into());
    };
    let meta = fs::metadata(&proot).map_err(|e| e.to_string())?;
    if meta.permissions().mode() & 0o111 == 0 {
        fs::set_permissions(&proot, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    Ok(())
}
