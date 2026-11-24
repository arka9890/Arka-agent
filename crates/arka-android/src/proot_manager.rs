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
use arka_core::events::Events;
use arka_core::shell::{ExecOutcome, Shell};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
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
pub fn install(ev: &dyn Events, cancel: &AtomicBool) -> Result<(), String> {
    if is_installed() {
        ev.progress(100, "Ubuntu 24 sudah terpasang.");
        return Ok(());
    }
    let files = ctx::files_dir()?;
    let cache_dir = files.join("downloads");
    fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;
    let archive = cache_dir.join("ubuntu-24.04-base-arm64.tar.gz");

    if !archive.is_file() || verify_sha256(&archive).unwrap_or(false) == false {
        download(ev, cancel, &archive)?;
    }
    if cancel.load(Ordering::Relaxed) {
        return Err("Dibatalkan".into());
    }

    ev.progress(70, "Membongkar Ubuntu 24…");
    extract_rootfs(&archive)?;

    ev.progress(85, "Menyiapkan environment…");
    post_install()?;

    ev.progress(90, "Memasang tools (python3, git, curl…)…");
    match install_tools(ev, cancel) {
        Ok(()) => ev.progress(100, "Ubuntu 24 siap dipakai."),
        // Tools gagal (mis. offline) tidak membatalkan pemasangan: rootfs tetap
        // berguna dan user bisa `apt install` sendiri dari Terminal.
        Err(e) => ev.progress(
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

fn download(ev: &dyn Events, cancel: &AtomicBool, dest: &Path) -> Result<(), String> {
    ev.progress(0, "Mengunduh Ubuntu 24…");
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
                ev.progress(
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

    ev.progress(66, "Memeriksa checksum rootfs…");
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
        if entry.header().entry_type().is_hard_link() {
            // Bug di perangkat tertentu: std::fs::hard_link ditolak oleh
            // filesystem /data (EXDEV/EPERM), sehingga unpack gagal di entri
            // seperti usr/bin/perl5.38.2. Hard link di rootfs hanya dipakai
            // untuk menghemat ruang, jadi salin isinya saja — hasilnya sama.
            let Some(link) = entry.link_name().map_err(|e| e.to_string())? else {
                continue;
            };
            let target_rel = link.to_path_buf();
            if target_rel.is_absolute()
                || target_rel.components().any(|c| matches!(c, std::path::Component::ParentDir))
            {
                continue;
            }
            let target = staging.join(target_rel);
            let dest = staging.join(&path);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            // Timpa file lama (mis. saat ekstraksi diulang) supaya tidak
            // gagal dengan "File exists".
            let _ = fs::remove_file(&dest);
            fs::copy(&target, &dest).map_err(|e| {
                format!("failed to copy hard link {} → {}: {}", path.display(), target.display(), e)
            })?;
            let mode = entry.header().mode().map_err(|e| e.to_string())? & 0o7777;
            fs::set_permissions(&dest, fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
            count += 1;
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

fn install_tools(ev: &dyn Events, cancel: &AtomicBool) -> Result<(), String> {
    let pkgs = TOOLS.join(" ");
    let script = format!(
        "export DEBIAN_FRONTEND=noninteractive; \
         apt-get update -qq && apt-get install -y -qq --no-install-recommends {pkgs} && \
         echo ARKA_TOOLS_OK"
    );
    let (code, out) = run_capture(&script, cancel, &mut |chunk| ev.output(chunk))?;
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

/// Pastikan salinan library ada di `proot-libs` dengan nama SONAME yang
/// dicari linker.
///
/// Dulu ini memakai symlink yang dibuat sekali. Setelah aplikasi bisa
/// di-upgrade in-place, `nativeLibraryDir` berubah (path instalasi baru)
/// sehingga symlink lama menggantung dan linker mengeluarkan:
///   CANNOT LINK EXECUTABLE libproot.so: library "libtalloc.so.2" not found
/// Selalu hapus entri lama lalu salin ulang — bukan symlink — supaya path
/// selalu segar setelah update.
fn refresh_lib(src: &Path, dest: &Path) -> Result<(), String> {
    match fs::remove_file(dest) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "Gagal menghapus {} lama: {}",
                dest.display(),
                e
            ))
        }
    }
    fs::copy(src, dest).map_err(|e| {
        format!("Gagal menyiapkan {} dari {}: {}", dest.display(), src.display(), e)
    })?;
    // Library tidak perlu executable, tapi izin 0644 membuatnya bisa dibaca
    // linker di semua perangkat.
    fs::set_permissions(dest, fs::Permissions::from_mode(0o644))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Siapkan pustaka proot sekali per proses.
///
/// Dulu `refresh_lib` (hapus lalu salin) dipanggil di **setiap** eksekusi
/// perintah. Kalau dua sesi menjalankan tool bersamaan, satu proses bisa
/// menghapus `libtalloc.so.2` yang sedang dimuat proses lain → proot crash
/// dengan error "cannot open shared object". Sekarang persiapan dilakukan
/// sekali (proses dimulai ulang setelah update aplikasi, jadi file tetap
/// segar).
static LIBS_READY: OnceLock<Result<(), String>> = OnceLock::new();

fn prepare_libs() -> Result<(), String> {
    let libs = ctx::proot_libs_dir()?;
    if let Some(talloc) = ctx::native_lib("libtalloc.so")? {
        // libproot.so membutuhkan SONAME "libtalloc.so.2", bukan "libtalloc.so".
        refresh_lib(&talloc, &libs.join("libtalloc.so.2"))?;
    }
    if let Some(shmem) = ctx::native_lib("libandroid-shmem.so")? {
        refresh_lib(&shmem, &libs.join("libandroid-shmem.so"))?;
    }
    Ok(())
}

/// Folder sementara unik untuk satu eksekusi proot.
///
/// `PROOT_TMP_DIR` dipakai proot untuk file internal. Dua perintah bersamaan
/// yang memakai satu folder berisiko menimpa file yang sama.
static RUN_SEQ: AtomicU64 = AtomicU64::new(0);

fn proot_run_tmp_dir() -> Result<PathBuf, String> {
    let seq = RUN_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = ctx::proot_tmp_dir()?.join(format!("run-{}-{}", std::process::id(), seq));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Env untuk proses proot itu sendiri (bukan untuk guest).
///
/// `tmp` WAJIB diset di sini: proot (host) membaca `PROOT_TMP_DIR` untuk
/// membuat direktori glue/f2fs sebelum rootfs dimount. Kalau hanya diset di
/// dalam guest (`env -i ...` di [proot_argv]), host proot memakai path default
/// hasil kompilasi (mis. `/data/data/com.termux/files/usr/tmp`) dan gagal.
fn proot_env(tmp: &Path) -> Result<Vec<(String, String)>, String> {
    if let Err(e) = LIBS_READY.get_or_init(prepare_libs) {
        return Err(e.clone());
    }
    let libs = ctx::proot_libs_dir()?;
    let native_dir = ctx::native_lib_dir()?;
    let ld_path = format!("{}:{}", libs.to_string_lossy(), native_dir.to_string_lossy());
    let mut env = vec![
        ("LD_LIBRARY_PATH".to_string(), ld_path),
        ("PROOT_TMP_DIR".to_string(), tmp.to_string_lossy().to_string()),
    ];
    if let Some(loader) = ctx::loader_binary()? {
        env.push(("PROOT_LOADER".to_string(), loader.to_string_lossy().to_string()));
    }
    Ok(env)
}

/// Rangkaian argumen proot untuk menjalankan [command] di dalam Ubuntu.
fn proot_argv(command: &str, tmp: &Path) -> Result<Vec<String>, String> {
    let proot = ctx::proot_binary()?.ok_or("Binary proot tidak ada di APK ini")?;
    if !is_installed() {
        return Err("Ubuntu 24 belum dipasang".into());
    }
    let rootfs = ctx::rootfs_dir()?;
    let workspace = ctx::workspace_dir()?;
    let loader = ctx::loader_binary()?.ok_or("Loader proot tidak ada di APK ini")?;

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

fn spawn_proot(command: &str, tmp: &Path) -> Result<std::process::Child, String> {
    let argv = proot_argv(command, tmp)?;
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    for (k, v) in proot_env(tmp)? {
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

/// Semua perintah proot memakai rootfs, workspace (`/root/storage`), dan
/// `/dev/shm` yang sama. Dua child yang jalan bersamaan bisa menimpa file
/// (mis. `git checkout`, `apt`, atau `main.py`) dan merusak rootfs. Karena itu
/// eksekusi diserialisasi di seluruh proses: chat tool, terminal manual,
/// diagnostic, dan install_tools antre lewat lock ini.
///
/// Lock dipegang sampai child selesai, bukan cuma saat spawn. Callback tetap
/// milik thread pemanggil (tidak dipindah), jadi aman.
static TOOL_LOCK: Mutex<()> = Mutex::new(());

/// Jalankan perintah dan kembalikan (exit code, gabungan stdout+stderr).
pub fn run_capture(
    command: &str,
    cancel: &AtomicBool,
    on_chunk: &mut dyn FnMut(&str),
) -> Result<(i32, String), String> {
    let _tool_guard = TOOL_LOCK.lock().map_err(|e| format!("Lock tool rusak: {e}"))?;
    if cancel.load(Ordering::Relaxed) {
        return Ok((-1, String::new()));
    }
    // Folder tmp unik per proses → dua tool dari sesi berbeda tidak saling
    // menimpa file internal proot. Dibersihkan setelah child selesai.
    let tmp = proot_run_tmp_dir()?;
    let result = (|| -> Result<(i32, String), String> {
    let mut child = spawn_proot(command, &tmp)?;
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
    loop {
        match rx.recv_timeout(Duration::from_millis(120)) {
            Ok(chunk) => {
                collected.push_str(&chunk);
                on_chunk(&chunk);
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
        on_chunk(&chunk);
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if cancel.load(Ordering::Relaxed) {
        return Ok((-1, collected));
    }
    Ok((status.code().unwrap_or(-1), collected))
    })();
    // Jangan hapus saat child masih hidup: proot bisa membaca file tmp-nya
    // sampai proses benar-benar keluar.
    let _ = fs::remove_dir_all(&tmp);
    result
}

/// Tes cepat: dipakai tombol "Tes distro" di Settings.
pub fn diagnostic() -> Result<String, String> {
    let cancel = AtomicBool::new(false);
    let (code, out) = run_capture(
        "cat /etc/os-release | head -2; uname -m; python3 --version 2>&1; git --version 2>&1; \
         ls -a /root/storage | head",
        &cancel,
        &mut |_| {},
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

/// Implementasi [`Shell`] untuk Android: `bash` di dalam Ubuntu (proot).
///
/// Satu-satunya penghubung antara agent loop (platform-agnostic) dan proot.
/// Di Windows padanannya `PwshShell` di crate desktop.
pub struct ProotShell;

impl Shell for ProotShell {
    fn status(&self) -> Result<serde_json::Value, String> {
        serde_json::to_value(status()?).map_err(|e| e.to_string())
    }

    fn prepare(&self, ev: &dyn Events, cancel: &AtomicBool) -> Result<(), String> {
        install(ev, cancel)
    }

    fn exec(
        &self,
        command: &str,
        cancel: &AtomicBool,
        on_chunk: &mut dyn FnMut(&str),
    ) -> Result<ExecOutcome, String> {
        if command.trim().is_empty() {
            return Err("Perintah kosong".into());
        }
        let (code, output) = run_capture(command, cancel, on_chunk)?;
        Ok(ExecOutcome {
            code,
            output,
            timed_out: false,
            canceled: cancel.load(Ordering::Relaxed),
        })
    }

    fn diagnostic(&self) -> Result<String, String> {
        diagnostic()
    }
}
