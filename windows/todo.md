# TODO — Arka Agent untuk Windows (100% Rust, shell native Windows)

Status: **M0 ✅ · M1 ✅ · CI-1 ✅ (terverifikasi di GitHub Actions)** — M2 (shell PowerShell) berikutnya.
Sumber: audit menyeluruh repo `Arka-agent` per commit `cb3a4b6`
(`rust/src/*` 13 file / 4.418 baris, `android/.../*.kt` 24 file / 10.971 baris, `docs/PRD.md`, `docs/TODO.md`).

Tujuan: aplikasi Arka Agent yang sama (agent AI + workspace + skill + terminal + file manager + provider)
berjalan di Windows, **tanpa Kotlin/Java, tanpa webview, tanpa Linux/WSL, satu binary Rust** —
dalam **satu repo** yang sama.

> **Keputusan produk (dikunci):** tidak ada Ubuntu, tidak ada WSL, tidak ada proot, tidak ada VM.
> `run_command` mengeksekusi **PowerShell** native. Workspace = folder Windows biasa.
> (Rencana lama berbasis WSL diarsipkan di `windows/todo-wsl-lama.md.bak` — jangan dipakai.)

---

## 0. Keputusan arsitektur (dasar semua pekerjaan di bawah)

| # | Keputusan | Alasan |
|---|-----------|--------|
| A1 | Repo jadi **Cargo workspace**: `crates/arka-core` (portable), `crates/arka-android` (JNI shim), `crates/arka-desktop` (UI). `android/` (Kotlin) tidak diubah. | Satu sumber logika; APK tetap bisa dibangun; tidak ada duplikasi kode AI/DB/skill. |
| A2 | UI desktop **egui/eframe** (bukan Tauri/Dioxus) | Murni Rust, tanpa webview/Node, single-exe, cross-platform, mudah dibuat bertema "clay". |
| A3 | **Shell = PowerShell native**, bukan Linux. `pwsh.exe` (PowerShell 7) bila ada, fallback `powershell.exe` (5.1). | Tanpa dependensi sistem tambahan; sudah ada di setiap Windows 10/11. |
| A4 | Abstraksi `Shell` trait: `PwshShell` (Windows utama), `ProotShell` (Android, perilaku lama). Tidak ada `WslShell`. | `run_command` + tab Terminal lewat satu antarmuka; Android tidak berubah perilaku. |
| A5 | Workspace = folder Windows: default `%USERPROFILE%\ArkaAgent\workspace` (bisa diubah). Path di UI/agent/DB **selalu path Windows** (`C:\...`), tidak ada pemetaan guest↔host. | Satu ruang path, tidak ada UNC/lambda path, langsung terlihat di Explorer. |
| A6 | Eksekusi perintah pakai `pwsh -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand <base64 UTF-16LE>` | Bebas dari quoting/escaping `"`/`'`/`&`/`%`/newline; aman untuk perintah multi-baris. |
| A7 | Output dibaca **UTF-8 paksa**: preamble `[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false); $OutputEncoding=[Console]::OutputEncoding` | Tanpa ini output native tool (git/curl/python) jadi mojibake/UTF-16 di PS 5.1. |
| A8 | Batalkan perintah = **Job Object** (`AssignProcessToJobObject` + `TerminateJobObject`) | Membunuh pohon proses (termasuk anak dari `npm`, `python`, `git`), bukan hanya induk. |
| A9 | API key di Windows dienkripsi **DPAPI** (`CryptProtectData`, scope user) lewat trait `SecretStore` | Setara Android Keystore; tidak ada key di disk. |
| A10 | Data app: `%APPDATA%\ArkaAgent\arka.db`; cache/log: `%LOCALAPPDATA%\ArkaAgent`; skill: `%USERPROFILE%\.claude\skills` | Konvensi Windows + kompatibel dengan skill Claude Code yang sudah dimiliki pengguna. |
| A11 | HTTP tetap `reqwest` + `rustls` (sudah dipakai) | Tidak butuh OpenSSL di Windows. |
| A12 | **Prompt pack per platform**: system prompt + deskripsi 6 tool dipisah jadi template `prompt/android.md` & `prompt/windows.md` | Saat ini prompt di `rust/src/ai.rs` hardcode "Ubuntu/bash/proot" — versi Windows wajib menyebut PowerShell + path Windows. |

---

## 1. Inventaris fitur Android → target Windows (jangan ada yang tertinggal)

Semua baris di bawah **wajib** ada di versi Windows.

### 1.1 Chat
- [ ] Streaming token-per-token (SSE) untuk `openai`-compatible + `anthropic`
- [ ] Typing indicator 3 titik ("AI sedang mengetik…") sebelum token pertama
- [ ] Tampilan markdown + blok kode berkotak + inline `code`/**bold**/`*italic*`/link
- [ ] Tombol salin pesan; tombol Stop saat streaming
- [ ] Lampiran dari dialog file OS (gambar/video/audio/teks) + thumbnail di bubble
- [ ] Isi file teks lampiran ikut dikirim sebagai konteks (maks 20.000 char)
- [ ] `files` di `ask_user` → kartu pertanyaan dengan pratinjau maks 5 file
- [ ] Dropdown model dengan pencarian + input model manual + refresh dari server
- [ ] Sidebar riwayat sesi: buat, pilih, rename, hapus, ringkas konteks (compact)
- [ ] Auto-title sesi dari pesan pertama (maks 60 char)
- [ ] Multi-sesi paralel + badge "sedang menjawab" + lanjut saat pindah sesi
- [ ] Auto-scroll ke pesan terbaru (bisa dimatikan)

### 1.2 Agent loop (inti `rust/src/lib.rs`)
- [ ] Tool `run_command` → **PowerShell** non-interaktif di workspace, stdout+stderr, exit code
- [ ] Tool `install_skill` (URL SKILL.md / repo GitHub, deteksi rate-limit 403/429)
- [ ] Tool `delete_skill`, `edit_skill`, `set_skill_config`
- [ ] Kartu **persetujuan** untuk 4 tool skill (Izinkan/Tolak, timeout 5 menit = tolak)
- [ ] Tool `ask_user` (min 2 opsi, `file`/`files` ≤ 5, batal = indeks -1)
- [ ] Batas putaran tool (default 40, boleh diubah 1–200) + pesan saat batas tercapai
- [ ] System prompt: bawaan / kustom / reset; otomatis disisipi semua skill dari `%USERPROFILE%\.claude\skills`
- [ ] Ringkasan konteks otomatis (budget token, `summary_upto`, batch ringkasan, retry saat error context-length)
- [ ] Pemangkasan output tool lama (2 output terakhir dipertahankan)
- [ ] Statistik provider: token & jumlah request per panggilan (termasuk per putaran tool)
- [ ] Cancel per sesi (tidak mengganggu sesi lain)

### 1.3 Provider
- [ ] Daftar provider + seed awal (OpenAI, Mistral, Anthropic, Google) tanpa API key
- [ ] Form: nama, kind (`openai`/`anthropic`), base URL, API key, model utama, multi-model, default
- [ ] Tes koneksi nyata (`GET /models`) + refresh daftar model (disimpan ke DB)
- [ ] Logika `endpoint()`: dukung base URL yang sudah berisi `/v1`, `/v2`, `/v1beta`, `/openai`
- [ ] Hapus provider, set default, tampilkan statistik per provider
- [ ] API key **tidak pernah** tampil penuh setelah disimpan (mode "reveal" eksplisit)

### 1.4 File manager
- [ ] Daftar folder (folder dulu, A→Z), breadcrumb, naik/turun direktori
- [ ] Buat folder, rename, hapus (konfirmasi ke Recycle Bin bila mungkin), impor, ekspor/simpan sebagai
- [ ] Deteksi jenis: text/image/video/pdf/binary
- [ ] Pratinjau: teks ≤ 2 MB, gambar (thumbnail → fullscreen), video (thumbnail + putar), PDF (halaman 1 + jumlah halaman), binary (metadata + unduh)
- [ ] Ukuran workspace total (byte → human readable)
- [ ] Path traversal ditolak (`..`, path absolut di luar workspace, symlink/junction keluar workspace)
- [ ] Path panjang (`\\?\`) dan nama ber-spasi/karakter non-ASCII tidak memecahkan UI maupun tool

### 1.5 Terminal
- [ ] Status shell: PowerShell 5.1/7.x, versi, arsitektur OS, ExecutionPolicy, workspace path, ukuran workspace
- [ ] "Pasang/buat workspace" = buat folder + opsional periksa tool (`git`, `python`, `node`, `curl`) via `Get-Command`
- [ ] Jalankan perintah dengan output streaming, exit code, tombol Stop (Job Object terminate)
- [ ] Riwayat perintah (maks 200, dedup berurutan, clear)
- [ ] Diagnostik (`$PSVersionTable`, `[Environment]::Is64BitOperatingSystem`, `Get-Command git,python`, `Get-ChildItem`)
- [ ] Satu antrean eksekusi (mutex) supaya dua perintah tidak saling menimpa output
- [ ] Timeout per perintah (default 120 s, bisa diubah) + pesan jelas saat kena timeout

### 1.6 Setelan
- [ ] Tema: sistem/gelap/terang; ukuran font: kecil/sedang/besar
- [ ] Bahasa: Indonesia/English/日本語 (semua string UI, bukan hanya sebagian)
- [ ] Auto-scroll, notifikasi pesan baru, jalan di latar belakang
- [ ] Batas putaran tool, ringkas otomatis, batas konteks (4.000–128.000), batas output tool (1.000–20.000)
- [ ] System prompt: lihat/ubah/reset ke bawaan
- [ ] Workspace: path (pilih folder) + ukuran; tombol "buka di Explorer"
- [ ] Statistik sistem: RAM terpakai/total, disk bebas, arsitektur
- [ ] Tentang: versi, commit, lisensi

### 1.7 Shell OS (pengganti fitur Android)
- [ ] Notifikasi Windows (toast) saat jawaban selesai dan jendela tidak fokus
- [ ] Tray icon: buka/keluar, tetap hidup saat jendela ditutup (setelan "jalan di latar belakang")
- [ ] Cegah sleep saat streaming (`SetThreadExecutionState`) — pengganti wakelock/foreground service
- [ ] Satu instance aplikasi (named mutex); argumen CLI `--open <file>` untuk pratinjau

---

## 2. Struktur repo target

```
Arka-agent/
├── Cargo.toml                  # [workspace] members = crates/*
├── crates/
│   ├── arka-core/              # logika portable (tanpa JNI, tanpa UI)  ← M1 selesai
│   │   ├── Cargo.toml          # tanpa jni/flate2/tar/sha2
│   │   ├── prompt/android.md   # BARU: system prompt Android (placeholder)
│   │   ├── prompt/windows.md   # BARU: system prompt Windows (PowerShell)
│   │   ├── src/lib.rs          # hanya deklarasi modul + re-export (tanpa JNI)
│   │   ├── src/agent.rs        # BARU: agent loop (dari lib.rs chatStream)
│   │   ├── src/events.rs       # BARU: trait Events + NoopEvents
│   │   ├── src/paths.rs        # BARU: AppPaths + Platform (pengganti ctx.rs)
│   │   ├── src/prompt.rs       # BARU: prompt pack + skema tool per platform
│   │   ├── src/secret.rs       # BARU: SecretStore + NoopSecretStore
│   │   ├── src/shell.rs        # BARU: trait Shell + ExecOutcome + registry
│   │   ├── src/ui.rs           # BARU: approval/question registry + pratinjau file
│   │   ├── src/ai.rs           # ChatStream::open(..., platform)
│   │   ├── src/db.rs / session.rs / provider.rs / settings.rs
│   │   ├── src/file.rs         # + path Windows, traversal guard (M6)
│   │   ├── src/memory.rs / skill.rs / terminal.rs
│   │   └── build-android.sh    # build `--package arka-android`
│   ├── arka-android/           # cdylib JNI + proot  ← M1 selesai
│   │   ├── src/lib.rs          # 45 fungsi Java_com_arka_agent_core_Arka_* (nama TETAP)
│   │   ├── src/events.rs       # JniEvents (satu-satunya pemanggil Kotlin)
│   │   ├── src/ctx.rs          # path khusus Android (rootfs, native_lib, tmp)
│   │   ├── src/proot_manager.rs# + ProotShell (impl Shell)
│   │   └── src/jni_util.rs     # emit_str / emit_str_str / emit_int_str
│   └── arka-desktop/           # bin: arka-agent.exe  ← M3+
│       ├── src/main.rs
│       ├── src/app.rs          # state global (pengganti MainViewModel)
│       ├── src/ui/{shell,theme,widgets,chat,bubble,markdown,files,preview,terminal,providers,settings,dialogs}.rs
│       ├── src/shell_pwsh.rs   # PwshShell (M2)
│       ├── src/secret_dpapi.rs # DpapiSecretStore (M5)
│       ├── src/platform.rs     # toast, tray, sleep-block, dialog file
│       ├── src/i18n.rs
│       └── assets/{icon.ico,fonts/*.ttf}
├── android/                    # Kotlin — tidak diubah
├── docs/
└── windows/                    # todo.md, build.md, installer/
```

---

## 3. Milestone

### M0 — Fondasi workspace (½ hari) ✅ SELESAI
- [x] `Cargo.toml` root `[workspace]` + `resolver = "2"` + `[profile.release]` dipindah ke root
- [x] Pindahkan `rust/` → `crates/arka-core` via `git mv` (history terjaga); `Cargo.lock` dipindah ke root
- [x] `crate-type = ["cdylib", "rlib"]` supaya bin desktop bisa memakai logika yang sama
- [x] `cargo check --workspace` & `cargo test --workspace` hijau (15 test lulus)
- [x] `crates/arka-core/build-android.sh` diperbarui (`WORKSPACE_ROOT`, `TARGET_DIR`, `--package arka-android` setelah M1)
- [x] `.gitignore`: `rust/target/` → `target/`
- [x] `.github/workflows/android.yml` jadi **Build & Test** + job `rust` (`cargo check`/`test --locked`)
- [x] Semua referensi `rust/` di README, `docs/PRD.md`, `docs/TODO.md`, `build.gradle.kts`, `jniLibs/README.md` diperbarui
- [ ] `crates/arka-android` (pemisahan JNI) → **dilakukan di M1**, karena `proot_manager.rs` memakai `std::os::unix`
      (crate ini belum bisa dicompile untuk Windows sampai pemisahan itu selesai)
**Selesai bila:** ✅ APK masih bisa dibuild dari CI setelah pemindahan, tanpa perubahan kode Kotlin.
> ⚠️ Catatan audit: klaim di atas awalnya **tidak** terverifikasi. Job `apk` di CI lama tidak
> pernah menjalankan cargo — ia memakai `libarka_core.so` yang ter-commit di `jniLibs`
> (build sebelum `rust/` dipindah), jadi APK-nya membawa kode native lama. Diperbaiki di
> "CI-1" di bawah: sekarang `.so` selalu dibangun dari sumber pada commit yang sama.

### M1 — Core portable + prompt pack (3 hari) ✅ SELESAI
- [x] Hapus semua ketergantungan JNI dari `arka-core`: callback diganti trait
      `trait Events { delta, done, error, tool, approval, question, progress, output }` (`src/events.rs`)
- [x] `arka-android` mengimplementasikan `Events` lewat `JniEvents` (`src/events.rs` crate android,
      `RefCell<JNIEnv>` + `into_env()` supaya `JString` hasil tetap bisa dibuat)
- [x] `AppPaths { platform, data_dir, cache_dir, workspace, skills_dir, skills_config_dir, native_lib_dir?, rootfs_dir? }`
      (`src/paths.rs`), diisi lewat `AppPaths::android(...)` / `AppPaths::windows(home, data)`
- [x] `modules` dipisah: `agent.rs`, `events.rs`, `paths.rs`, `prompt.rs`, `secret.rs`, `shell.rs`, `ui.rs`
- [x] Semua kode JNI (`Java_com_arka_agent_core_Arka_*`, 45 fungsi) pindah ke `crates/arka-android/src/lib.rs`;
      `proot_manager.rs` + `jni_util.rs` + `ctx.rs` (khusus Android) juga pindah ke sana
- [x] `Shell` trait (`status`, `prepare`, `exec`, `diagnostic`) + `ProotShell` (Android) → M2 cukup menambah `PwshShell`
- [x] `SecretStore` trait + `NoopSecretStore` (`src/secret.rs`); `provider::save`/`list`/`get` sudah lewat trait
      (Android Noop → perilaku Kotlin tidak berubah). `DpapiSecretStore` menyusul di M5
- [x] `chatStream` → `agent::chat(ChatRequest, &dyn Events)`; pembatalan per sesi + registry approval/pertanyaan
      pindah ke `agent.rs` / `ui.rs`
- [x] **`prompt.rs` + `prompt/{android,windows}.md`** dengan placeholder `{{WORKSPACE}}`, `{{SKILLS_DIR}}`,
      `{{CONFIG_DIR}}`; skema tool (`tools_schema`, `anthropic_tools_schema`) ikut per platform
- [x] `ai.rs` tidak lagi punya `SYSTEM_PROMPT`/teks Ubuntu; prompt dipilih `prompt::active_system_prompt()`
- [x] `skill::system_prompt_section()` memakai path skill platform aktif (tidak lagi "di Ubuntu")
- [x] Semua test lama tetap jalan + test baru: prompt Windows bebas "Ubuntu"/"bash"/"proot"/`/root/storage`,
      placeholder habis, nama tool stabil, cancel per sesi, `SecretStore` round-trip
- [ ] `db::init` + `PRAGMA user_version` + migrasi berurutan → **dipindah ke M4** (butuh skema baru saat
      kolom UI bertambah; sekarang migrasi masih `ALTER TABLE` ad-hoc yang aman)
**Selesai bila:** ✅ `cargo test --workspace` hijau (30 test core + 1 android), APK tetap memakai crate
`arka-android` (`build-android.sh` diperbarui: build `--package arka-android`, salin
`libarka_android.so` → `libarka_core.so`), dan test membuktikan prompt Windows tidak memuat "Ubuntu"/"bash".

> **Catatan jujur:** registry `paths` masih `OnceLock` (diisi sekali saat start) — sama seperti DB.
> Yang berubah adalah isinya kini **data**, bukan kode JNI, jadi tidak ada urutan inisialisasi yang
> bergantung pada Kotlin. Kalau nanti butuh dua instance (multi-window), baru diubah menjadi parameter
> eksplisit; sekarang biayanya lebih besar daripada manfaatnya.
> Verifikasi Windows **belum** dijalankan di mesin ini: target `x86_64-pc-windows-*` dan `rustfmt`/`clippy`
> tidak terpasang di workspace Android ini. Yang terbukti hijau: `cargo check/test --workspace` di Linux
> dan tidak ada lagi `std::os::unix` maupun `jni` di `arka-core` (dicek dengan grep + dependensi crate).

### CI-1 — Pipeline build diperbaiki & diverifikasi (¼ hari) ✅ SELESAI
- [x] Job baru `rust-android`: `dtolnay/rust-toolchain` (target `aarch64-linux-android`, `armv7-linux-androideabi`) + `setup-android` (NDK 27.0.12077973)
- [x] `crates/arka-core/build-android.sh all` dijalankan di CI → memvalidasi **path baru** setelah `rust/` dipindah
- [x] Asersi `test -s jniLibs/{arm64-v8a,armeabi-v7a}/libarka_core.so` (nama file harus sama dengan `System.loadLibrary("arka_core")`)
- [x] `.so` di-upload sebagai artifact `jniLibs`, lalu job `apk` `needs: rust-android` + `download-artifact` menimpanya
- [x] Asersi isi APK: `lib/arm64-v8a/libarka_core.so` harus **tepat satu** entri (sesuai `abiFilters`
      arm64-v8a saja), dan ukurannya dibandingkan dengan `.so` hasil job `rust-android` — jadi
      .so hilang **atau** APK memuat binary basi sama-sama menggagalkan build
- [x] `ANDROID_NDK_HOME` dihitung dari `$ANDROID_HOME` saat runtime (ekspresi `${{ env.ANDROID_HOME }}` kosong karena di-set `GITHUB_ENV`)
- [x] Job baru `rust-windows` (`windows-latest`): `cargo check`/`test -p arka_core` + tes penjaga prompt → gerbang regresi kode Unix bocor ke core
- [x] Job `apk` tidak lagi memasang `ndk;...` (NDK hanya dipakai job Rust), mempercepat build Kotlin
- [x] YAML divalidasi parser (`python3 -c "import yaml; yaml.safe_load(...)"`)
**Catatan:** `cargo check --target x86_64-pc-windows-msvc` **tidak bisa** dijalankan dari Linux —
`ring` (lewat `reqwest`) mengompilasi C dan butuh `cl.exe`. Karena itu pemeriksaan Windows
dijalankan di runner `windows-latest`, bukan cross-check dari Ubuntu.
**Selesai bila:** ✅ push ke `main` menghasilkan APK yang `.so`-nya berasal dari commit itu sendiri,
dan kegagalan build native menggagalkan job `apk` (tidak ada lagi artifact "sukses" dengan lib basi).

**Bukti (verifikasi nyata, bukan asumsi):**
- Run [`45302986207`](https://github.com/arka9890/Arka-agent/actions/runs/45302986207) — commit `30dadf2`:
  3 job hijau, **job `apk` merah** di langkah asersi. Penyebabnya bukan build rusak, melainkan
  **asersi yang salah**: kode menuntut ≥2 entri `lib/*/libarka_core.so`, padahal `abiFilters` hanya
  `arm64-v8a` sehingga APK yang benar memang memuat satu `.so`. Diperbaiki di `6ff3d4e`.
- Run [`45303743348`](https://github.com/arka9890/Arka-agent/actions/runs/45303743348) — commit `6ff3d4e`:
  **4 job hijau**. Bukti dari log: `ukuran .so: di APK=3629976 byte, hasil build=3629976 byte`
  (identik → AGP tidak men-strip, dan yang terpaket memang binary baru, bukan `.so` ter-commit dari 23 Nov).
- Catatan: `armeabi-v7a` tetap dibangun job `rust-android` (memvalidasi `build-android.sh all`),
  tetapi **tidak masuk APK** karena `abiFilters` hanya `arm64-v8a`. Menambah v7a = keputusan produk
  tersendiri (butuh `libproot.so` 32-bit yang belum ada).

### M2 — Shell PowerShell (3–4 hari) ✅ selesai (bukti CI menyusul)
- [x] `trait Shell`: `status()`, `prepare(ev, cancel)`, `exec(cmd, cancel, on_chunk) -> ExecOutcome`, `diagnostic()`
      (dibuat di M1; `PwshShell` mengimplementasikan trait ini)
- [x] Crate `arka-desktop` + binary `arka-agent` (CLI bukti; UI egui di M3)
- [x] `PwshShell`:
  - [x] deteksi `pwsh.exe` (7.x) → fallback `powershell.exe` (5.1) dengan **mencoba menjalankan**,
        bukan menebak dari `PATH`; versi + edition dicatat di `status()`
  - [x] `exec`: proses tersembunyi (`CREATE_NO_WINDOW`), `-NoProfile -NonInteractive -EncodedCommand`
        (UTF-16LE → base64), `stdin = null`, cwd = workspace
  - [x] preamble UTF-8 (§A7) + `$ErrorActionPreference='Continue'` + `$ProgressPreference='SilentlyContinue'`
        + `PYTHONIOENCODING`
  - [x] stdout & stderr dibaca **paralel** di 2 thread, digabung lewat channel
  - [x] **decoder UTF-8 bertahan lintas potongan** (`Utf8Decoder`) — emoji 4 byte tidak jadi `�`
  - [x] exit code: penanda `__ARKA_EXIT__` di akhir naskah (PowerShell tidak meneruskan
        `$LASTEXITCODE` ke exit proses) → normalisasi ke int; `canceled` & `timed_out` terpisah
  - [x] cancel & timeout → `TerminateJobObject` (Job Object + `KILL_ON_JOB_CLOSE`),
        plus batas kesabaran 1 dtk kalau ada proses yang lolos dari job
  - [x] `prepare`: `create_dir_all(workspace)` + `.arka/README.txt`; deteksi tools via `Get-Command`
  - [x] `status`: PS version/edition, OS, arsitektur, ExecutionPolicy, workspace + jumlah file + ukuran
  - [x] `diagnostic`: program, PS version, OS, arch, workspace, encoding, probe emoji, tools
- [x] Guard: tolak perintah kosong & byte NUL; batas output 200.000 karakter (dipotong + ditandai)
- [x] `Mutex` global eksekusi (satu perintah sekaligus), seperti `TOOL_LOCK`
- [x] Unit test (32 test, jalan di Linux): round-trip UTF-16LE/base64 (termasuk emoji & Jepang),
      susunan naskah, parsing penanda/CRLF, deteksi `-EncodedCommand` tunggal, guard kosong/NUL,
      loop pump dengan `/bin/sh` (batal, timeout, potong output, UTF-8 dari proses nyata)
- [x] `settings::workspace_default()` mengikuti platform — tidak lagi `/root/storage` di Windows
- [x] `--selftest`: bukti di mesin Windows (UTF-8, exit code native, **Stop mematikan pohon proses**)
- [x] CI `.github/workflows/desktop.yml`: `desktop-logic` (Linux) + `desktop-windows` (smoke + selftest)
- [ ] `DpapiSecretStore` → **dipindah ke M5** (memang di sana; M2 mendaftarkan `NoopSecretStore`)
**Selesai bila:** di Windows `run_command` mengembalikan `Get-ChildItem` dari workspace, output UTF-8 benar (emoji/日本語 utuh), dan tombol Stop benar-benar mematikan `python -c "import time; time.sleep(60)"` beserta anak prosesnya.

### M3 — Kerangka UI + tema (3–4 hari)
- [ ] `eframe` window 1200×800, min 900×600, persist ukuran/posisi (`eframe::Storage`)
- [ ] `theme.rs`: port `theme/Theme.kt` (DarkClay/LightClay + 12 warna) → `Visuals` + palet clay
- [ ] `widgets.rs`: `clay_card`, `clay_pill`, `clay_button`, `section` (port `Clay.kt`)
- [ ] Font: bundle Inter/JetBrains Mono (kode), skala font 0.85/1.0/1.15 (kecil/sedang/besar)
- [ ] Shell layout: sidebar kiri (sesi, bisa collapse) + 5 tab (Chat, Provider, File, Terminal, Setelan)
- [ ] `i18n.rs`: semua string id/en/ja (port `L10n.kt`, jangan ada string hardcoded)
- [ ] State global `app.rs` meniru `MainViewModel`: settings, sessions, messages, providers, files, terminal, shell status, progress, approval, question, notice
- [ ] Snackbar/notice + dialog modal standar
**Selesai bila:** aplikasi terbuka, tema gelap/terang/ukuran font bisa diganti dan langsung terlihat, 5 tab bisa diklik.

### M4 — Chat + agent (5–7 hari) ← inti
- [ ] `bubble.rs`: bubble user/AI (warna beda), timestamp, tombol salin, lampiran
- [ ] `markdown.rs`: blok kode + inline (port `splitCodeBlocks`/`inlineMarkdown`), highlight sederhana per bahasa (opsional)
- [ ] Typing indicator 3 titik berdenyut sebelum token pertama
- [ ] Streaming: `Events::delta` → `engine.rs` buffer per sesi → UI 60 fps tanpa flicker
- [ ] Input bar: Enter kirim, Shift+Enter baris baru, tombol kirim/stop, lampiran (dialog file OS)
- [ ] `dialogs.rs`: kartu persetujuan skill & kartu `ask_user` (opsi + pratinjau 5 file)
- [ ] Dropdown model: kotak cari, daftar dari DB, input manual, tombol refresh
- [ ] Sidebar sesi: buat, pilih, rename (inline edit), hapus (konfirmasi), compact
- [ ] Tampilkan tool call di panel Terminal (format `[judul sesi] PS> perintah` + output)
- [ ] Multi-sesi paralel: badge "menjawab" + notifikasi saat selesai & jendela tidak fokus
- [ ] Auto-scroll + opsi matikan
**Selesai bila:** percakapan nyata dengan provider OpenAI/Anthropic berjalan streaming, model memanggil `run_command` (PowerShell), hasil muncul di Terminal, dan `ask_user` menampilkan pratinjau file.

### M5 — Provider + secret store (2–3 hari)
- [ ] `providers.rs`: daftar kartu + statistik token/request + badge default
- [ ] Form tambah/edit (validasi: nama & base URL wajib) + test koneksi + refresh model + multi-select model + input manual
- [ ] `secret.rs`: `DpapiSecretStore` (`CryptProtectData`/`CryptUnprotectData`, `windows` crate) → simpan blob base64, prefix `dpapi:`
- [ ] Migrasi: nilai plaintext lama otomatis dienkripsi sekali saat baca
- [ ] API key dimasker; tombol "lihat" sementara
**Selesai bila:** API key tidak terbaca sebagai plaintext di `arka.db`, dan tes koneksi berhasil.

### M6 — File manager + pratinjau (3–4 hari)
- [ ] `files.rs`: list + breadcrumb + naik/turun + ukuran + ikon per jenis
- [ ] Aksi: buat folder, rename, hapus (konfirmasi; ke Recycle Bin via `IFileOperation` bila memungkinkan), impor (`rfd::FileDialog`), ekspor ("Simpan sebagai")
- [ ] `preview.rs`: teks (≤ 2 MB), gambar (`egui_extras::image`), video/audio (thumbnail + "Buka di pemutar"), PDF (halaman 1 + jumlah halaman, `pdfium-render`), binary (metadata + unduh)
- [ ] Drag & drop dari Explorer → impor ke workspace
- [ ] Guard traversal + `\\?\` untuk path > 260 char
**Selesai bila:** file yang dibuat `run_command` langsung muncul, bisa dipratinjau, diimpor/ekspor/dihapus.

### M7 — Terminal + status shell (2–3 hari)
- [ ] Panel output monospace, auto-scroll, tombol Bersihkan
- [ ] Input perintah + riwayat (↑/↓ + dropdown, maks 200)
- [ ] Tombol Jalankan/Stop, exit code ditampilkan, "dihentikan pengguna"/"timeout"
- [ ] Kartu status shell: PowerShell versi, OS, arsitektur, workspace + ukuran, tombol "Buka di Explorer"
- [ ] Tombol "Tes shell" menampilkan hasil diagnostik
- [ ] Hint: contoh perintah PowerShell yang umum (bukan bash)
**Selesai bila:** `git clone`/`python`/`npm` dari Terminal Windows berjalan dan output terlihat real-time.

### M8 — Setelan, notifikasi, tray (2–3 hari)
- [ ] Semua kontrol §1.6 (tema, bahasa, font, auto-scroll, notifikasi, background, 3 batas angka + validasi rentang, system prompt editor, pilih workspace)
- [ ] Statistik sistem: RAM (`GlobalMemoryStatusEx`), disk bebas (`GetDiskFreeSpaceEx`), arsitektur
- [ ] `notify-rust` toast (butuh AppUserModelID agar muncul di Win10/11)
- [ ] `tray-icon`: menu Buka / Jalan di latar belakang / Keluar; close → tray bila setting aktif
- [ ] `SetThreadExecutionState(ES_CONTINUOUS|ES_SYSTEM_REQUIRED)` selama streaming
- [ ] Tentang: versi (`CARGO_PKG_VERSION`), commit hash build, lisensi MIT
**Selesai bila:** semua setelan persist antar restart dan toast muncul saat jendela minimize.

### M9 — Paket, CI, dokumentasi, rilis (3–4 hari)
- [ ] Icon + metadata exe (`winres`/`embed-resource`), manifest DPI-aware
- [ ] Installer: `cargo-wix` (MSI) **atau** NSIS; portable ZIP sebagai alternatif
- [ ] CI `windows.yml`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, build release, upload artifact
- [x] CI `android.yml` tetap hijau — kini juga membangun `.so` sendiri dan memeriksa `arka_core` di `windows-latest`
- [ ] Hapus `rust/` lama; perbarui `README.md` (2 platform), `windows/build.md` (prasyarat: **tanpa WSL**, cukup VS Build Tools), `docs/PRD.md` (bagian Windows)
- [ ] Rilis GitHub: `arka-agent-<ver>-windows-x64.msi`, `.zip`, APK
**Selesai bila:** pengguna Windows baru bisa install → app membuat workspace → chat, `run_command`, terminal, file, skill langsung jalan tanpa memasang apa pun selain aplikasinya.

---

## 4. Pemetaan file Android → file Windows (checklist migrasi)

| Android / Rust lama | Target | Catatan |
|---|---|---|
| `core/Arka.kt` | `crates/arka-android/src/lib.rs` | Nama `Java_com_arka_agent_core_Arka_*` dipertahankan |
| `core/ArkaRepo.kt` | `arka-core/src/api.rs` + `arka-desktop/src/platform.rs` (SAF → `rfd`) | `content://` hanya Android |
| `core/Models.kt` | `arka-core/src/models.rs` | Nama field JSON snake_case dipertahankan |
| `core/ProviderEditor.kt`, `core/ApiKeyEncryption.kt` | `arka-core/src/secret.rs` | Android tetap di Kotlin; Windows DPAPI |
| `ChatEngine.kt` | `arka-core/src/engine.rs` | Buffer, running set, `finished` event, per-sesi cancel |
| `MainViewModel.kt` | `arka-desktop/src/app.rs` | State + aksi (send/stop/prepare/terminal/files/settings) |
| `ui/AppScreen.kt`, `ui/components/Clay.kt`, `ui/theme/Theme.kt` | `ui/shell.rs`, `ui/widgets.rs`, `ui/theme.rs` | |
| `ui/screens/ChatScreen.kt` | `ui/chat.rs` | |
| `ui/screens/MessageBubble.kt` | `ui/bubble.rs` + `ui/markdown.rs` | |
| `ui/screens/FilesScreen.kt`, `FilePreviewDialog.kt` | `ui/files.rs`, `ui/preview.rs` | |
| `ui/screens/TerminalScreen.kt` | `ui/terminal.rs` | Status distro → status PowerShell |
| `ui/screens/ProvidersScreen.kt` | `ui/providers.rs` | |
| `ui/screens/SettingsScreen.kt` | `ui/settings.rs` | |
| `ui/screens/UiRequestDialogs.kt` | `ui/dialogs.rs` | |
| `ui/L10n.kt` | `i18n.rs` | |
| `Notifications.kt` | `arka-core/src/notify.rs` + `platform.rs` | |
| `ArkaChatService.kt`, `BatteryOptimization.kt` | `platform.rs` (tray + sleep-block) | Tidak ada padanan service di Windows |
| `MainActivity.kt` | `main.rs` | Single-instance + argumen CLI |
| `rust/src/ai.rs` (prompt + tool schema) | `arka-core/src/ai.rs` + `prompt.rs` | **Wajib dipecah per platform** |
| `rust/src/proot_manager.rs` | `arka-core/src/shell/pwsh.rs` + `arka-android/src/proot_manager.rs` | Windows: PowerShell; proot tetap Android |
| `rust/src/lib.rs` (chatStream/agent loop) | `arka-core/src/agent.rs` | Callback → `Events` trait |

---

## 5. Risiko & hal yang wajib diuji lebih awal

| Risiko | Dampak | Mitigasi |
|---|---|---|
| Agent tetap "berpikir bash" (perintah `ls`, `cat`, `apt`) karena prompt lama | Tool sering gagal | Prompt pack §A12 + contoh PS di prompt + tes otomatis larangan kata "Ubuntu"/"bash"; tambahkan catatan di system prompt bahwa `ls`/`cat` adalah alias tapi `apt` tidak ada |
| Encoding output PS 5.1 (UTF-16/codepage) | Teks rusak, pratinjau kacau | Preamble UTF-8 (§A7) + uji dengan emoji, 日本語, aksen; normalisasi `\r\n` → `\n` |
| Quoting/escaping perintah dari model | Perintah gagal aneh | Pakai `-EncodedCommand` (§A6), jangan pernah merangkai `-Command "<cmd>"` |
| Membunuh proses hanya mematikan induk | Proses zombie menahan file/lock | Job Object + `TerminateJobObject`, uji dengan proses anak |
| PowerShell 7 belum tentu ada; 5.1 punya sintaks berbeda | Fitur PS7 gagal di mesin pengguna | Deteksi versi + hanya pakai sintaks kompatibel 5.1 di preamble; tampilkan versi di tab Terminal |
| ExecutionPolicy / antivirus memblokir script | Perintah gagal "not digitally signed" | `-ExecutionPolicy Bypass` per-proses (bukan ubah sistem) + dokumentasi |
| `LongPathEnabled` mati | Path > 260 char gagal | Prefix `\\?\` internal + pesan bila masih gagal |
| PDF butuh `pdfium.dll` | Ukuran/distribusi | Bundle di installer, atau fallback "buka PDF" |
| Toast Windows butuh AppUserModelID | Notifikasi tak muncul | Shortcut Start Menu saat install + set AUMID |
| Dua proses menulis DB/workspace | Data rusak | Single-instance lock (named mutex) |
| `rusqlite bundled` butuh MSVC build tools di CI | CI gagal | Runner `windows-latest` sudah punya; dokumentasikan VS Build Tools untuk build lokal |

---

## 6. Definition of Done (v1.0 Windows)

1. Semua item §1 tercentang dan diuji manual pada Windows 10 22H2 + Windows 11 23H2.
2. `cargo test --workspace` hijau; `clippy -D warnings` bersih; `fmt` bersih.
3. CI Windows + Android hijau di commit yang sama.
4. Installer MSI/ZIP tersedia di GitHub Releases; fresh install langsung bisa chat & menjalankan perintah — **tanpa WSL, tanpa Ubuntu, tanpa langkah setup Linux**.
5. Tidak ada proses yang menggantung: setiap perintah punya timeout + Stop yang mematikan pohon proses (Job Object).
6. Prompt & UI tidak menyebut "Ubuntu"/"proot"/`/root/storage` pada build desktop (dijaga unit test).
7. Dokumentasi: `windows/build.md`, README 2 platform, PRD bagian Windows.

---

## 7. Backlog (pasca v1.0)

- [ ] Opsi "remote shell" (SSH ke mesin Linux pengguna) — kalau nanti tetap butuh Linux, tidak lewat WSL
- [ ] Pemutaran video in-app (GStreamer/libmpv)
- [ ] Syntax highlighting penuh (tree-sitter) di chat & pratinjau
- [ ] Multi-window / tab sesi
- [ ] Terminal interaktif nyata (ConPTY) untuk program yang butuh TTY (mis. `python` REPL)
- [ ] Auto-update (MSIX/`self_update`)
- [ ] Voice input
- [ ] Ekspor percakapan (Markdown/JSON)
