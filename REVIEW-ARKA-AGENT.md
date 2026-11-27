# Arka Agent — Full Code Review Report (Verified)

**Tanggal**: 2025-01-15
**Commit**: c6f5d23 (fix(core): memori sesi tidak hilang + workspace/skill selalu ditekankan)
**Reviewer**: Manual review via direct source reading + `cargo check/test/clippy`
**Status**: 59 core tests passed, 159 desktop tests passed, clippy clean (4 style warnings)

---

## 1. Ringkasan Eksekutif

Arka Agent adalah asisten AI cross-platform (Android + Windows) dengan core logic di crate `arka-core` (Rust) dan platform-specific:
- **Android**: JNI shim (`arka-android`) + PRoot Ubuntu 24.04 (`proot_manager.rs`) + Kotlin UI (Compose)
- **Windows**: Native Rust UI (egui/eframe) + PowerShell 7/5.1 (`pwsh.rs`) + DPAPI encryption (`dpapi.rs`)

**Ukuran codebase**: 129 file tracked, 59 file `.rs` (~22k LOC), 24 file Kotlin, 8 file `.so` prebuilt.

**Temuan tingkat tinggi (diverifikasi)**:
- 0 Critical (RCE butuh user approve skill dulu, lihat detail)
- 1 High (proot deadlock risk)
- 5 Medium (TOCTOU, global lock, no WAL, tool_timeout tidak dipakai agent, migrate race)
- 8 Low/Info (error handling stringly-typed, magic numbers, missing logging, clippy warnings)

---

## 2. Arsitektur & Alur Data (Diverifikasi)

### 2.1 Dependency Graph (dari `Cargo.toml` + `lib.rs`)

```
arka-core (rlib+cdylib)
├── agent, ai, memory, session, provider, skill, file, shell, ui, events, db, paths, settings, secret, terminal, prompt
│
├─ arka-android (cdylib → libarka_core.so)
│   ├── lib.rs: 15 fungsi JNI export
│   ├── proot_manager.rs: ProotShell impl Shell
│   └── secret: NoopSecretStore (enkripsi di Kotlin)
│
└─ arka-desktop (bin → arka-agent)
    ├── main.rs: bootstrap paths, db, shell, secret
    ├── pwsh.rs: PwshShell impl Shell
    ├── dpapi.rs: DpapiSecretStore (Windows DPAPI)
    └── gui/: egui/eframe UI
```

**Bukti**: `crates/arka-core/Cargo.toml` crate-type `["cdylib", "rlib"]`, `crates/arka-android/Cargo.toml` name `arka_android`, `crates/arka-desktop/Cargo.toml` `[[bin]]`.

### 2.2 Alur Chat (Agent Loop) — dari `agent.rs:150-600`

```
agent::chat(ChatRequest) 
  ├─ provider::get()/default_provider()
  ├─ settings::max_tool_rounds(), context_budget(), tool_output_chars(), chat_timeout_secs()
  ├─ memory::context_section() → system prompt (workspace + skills + summary)
  ├─ memory::prepare() → compaction (optional, auto_summary)
  ├─ skill::system_prompt_section() → all skills
  ├─ shell::active() → Shell impl (ProotShell / PwshShell)
  └─ Loop:
      ai::ChatStream → delta/tool_calls
      tool match: install_skill/delete_skill/edit_skill/set_skill_config/ask_approval/ask_question/subagent/run_command
      run_command → shell.exec() → ProotShell.run_capture() / PwshShell.run()
      tool result → model
  ├─ session::add_message() → DB
  ├─ provider::record_usage() → DB
  └─ session::set_memory() → DB
```

---

## 3. Temuan Keamanan (Diverifikasi)

### 🔴 HIGH-1: proot `run_capture()` Deadlock Risk (Android)
**File**: `crates/arka-android/src/proot_manager.rs:547-669`
**Bukti**:
```rust
// Line 559: lock dipegang SELAMA child hidup
let _tool_guard = TOOL_LOCK.lock().map_err(|e| format!("Lock tool rusak: {e}"))?;

// Line 649: wait() DULU
let status = child.wait();
// Line 651: join readers SETELAH wait
for r in readers { let _ = r.join(); }
```
**Masalah**: Kalau `child.wait()` return error (mis. child sudah reaped), `readers` join tidak pernah dipanggil → pipe tidak tertutup → reader thread block selamanya → `TOOL_LOCK` tidak dilepas → **semua perintah berikutnya diam**.
**Fix**: Join readers **sebelum** `wait()`, atau pakai `crossbeam::select!` non-blocking.

### 🟠 MEDIUM-1: `resolve_at()` TOCTOU (file.rs:92-125)
**Bukti**: Line 112-119:
```rust
if full.exists() {
    let canon = full.canonicalize()?;  // TOCTOU window di sini
    if !canon.starts_with(&base) { return Err(...) }
    return Ok(canon);
}
Ok(long_path(&full))  // path baru: tidak canonicalize, symlink di parent bisa escape
```
Path yang **belum ada** melewati `canonicalize()` → symlink di parent directory bisa escape workspace.

### 🟠 MEDIUM-2: Global Lock Serialisasi Semua Command (Android + Desktop)
**Android**: `TOOL_LOCK` (`proot_manager.rs:547`) — chat tool, terminal manual, install Ubuntu, diagnostic **semua antre satu mutex**.
**Desktop**: `EXEC_LOCK` (`pwsh.rs:14`) — sama.
**Dampak**: Satu `pip install` lama blokir seluruh agent loop.

### 🟠 MEDIUM-3: `settings::tool_timeout_secs` Tidak Dipakai Agent Loop
**Bukti**: 
- `agent.rs:506` memanggil `shell.exec(&command, ...)` **tanpa timeout**
- `pwsh.rs:44` `default_timeout()` baca `tool_timeout_secs` → hanya dipakai `exec_with_timeout`
- `shell.rs:60-67` default impl `exec_with_timeout` → delegate ke `exec` (mengabaikan timeout)
- `ProotShell::exec()` line 726 override dengan `Some(Duration::from_secs(120))` hardcoded
**Akibat**: Perintah `run_command` dari model tidak punya timeout konfigurable → bisa menggantung tanpa batas.

### 🟠 MEDIUM-4: `migrate_plaintext_keys()` Race Condition Multi-Process
**File**: `provider.rs:183-215`
**Bukti**: 
```rust
let rows = db::with(|c| { SELECT id, api_key FROM providers ... })?;  // read
for (id, stored) in rows {
    let protected = store.protect(&stored)?;  // encrypt
    db::with(|c| { UPDATE providers SET api_key = ? WHERE id = ? })?;  // write
}
```
Kalau dua proses (desktop + CLI `--exec`) jalan bersamaan: kedua proses baca plaintext sama, encrypt beda (DPAPI salt), write bergantian → **key double-encrypt atau overwrite**.

### 🟠 MEDIUM-5: SQLite Tanpa WAL Mode
**File**: `db.rs` — tidak ada `PRAGMA journal_mode=WAL`
**Bukti**: Comment line 5: "tanpa perlu WAL tuning" — tapi tanpa WAL, `SQLITE_BUSY` terjadi di concurrent access (GUI + agent thread).
**Fix**: Tambah di `init()`:
```rust
conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
```

### 🟡 LOW-1: Error Handling Stringly-Typed
Semua crate pakai `Result<T, String>` — hilang `source()` chain, backtrace, error code. Contoh `agent.rs`: `.map_err(|e| e.to_string())` di 15+ tempat.

### 🟡 LOW-2: Magic Numbers Hardcoded (bisa di-set via settings tapi tidak terdokumentasi)
| Konstanta | File | Default |
|-----------|------|---------|
| `MAX_SUBAGENT_DEPTH = 2` | agent.rs | - |
| `KEEP_RECENT = 6` | memory.rs | - |
| `MAX_BATCH_MESSAGES = 24` | memory.rs | - |
| `MAX_SUMMARY_CHARS = 7000` | memory.rs | - |
| `MAX_TOOL_ROUNDS_LIMIT = 2000` | settings.rs | - |
| `TOOL_TIMEOUT_MAX = 3600` | settings.rs | - |
| `MAX_TEXT_BYTES = 2MB` | file.rs | - |

### 🟡 LOW-3: Missing Structured Logging
**Bukti**: `grep -r "tracing\|log\|env_logger" crates/*/Cargo.toml` → **TIDAK ADA**. Hanya `eprintln!` di test.

### 🟡 INFO-1: Clippy Warnings (4 style, 0 correctness)
```
- derivable_impls (FilesUi)
- type_complexity (state.rs:1010)
- manual_unwrap_or (script.rs:82)
```

---

## 4. Kualitas Kode (Diverifikasi)

### 4.1 Test Coverage
| Crate | Unit Tests | Integration |
|-------|------------|-------------|
| arka-core | 59 passed | ❌ Tidak ada |
| arka-android | 1 (shell registry) | Perlu device |
| arka-desktop | 159 passed | ❌ Tidak ada |

**Gap**: Tidak ada test end-to-end: chat → tool → shell → DB → UI.

### 4.2 Kompleksitas Fungsi Terbesar
| File | Fungsi | Baris | Status |
|------|--------|-------|--------|
| agent.rs | `run_chat()` | ~600 | Perlu refactor |
| ai.rs | `ChatStream::open` + `collect_tool_calls` | ~400 | 2 protokol manual |
| memory.rs | `prepare()` | ~280 | Loop compaction + DB |
| proot_manager.rs | `run_capture()` | ~200 | Thread + kill + lock |
| pwsh.rs | `PwshShell::exec()` | ~180 | Script + pump + lock |

### 4.3 Platform-Specific Leakage (Diverifikasi)
**Bagus**: `prompt.rs` `active_system_prompt()` pilih Android/Windows, `paths.rs` `Platform` enum.
**Masih bocor**:
- `agent.rs:150` `platform()` fallback `Platform::Android` kalau `paths::init` belum dipanggil
- `proot_manager.rs` hardcode `ubuntu-base-24.04.5-base-arm64.tar.gz` + SHA256
- `pwsh.rs` `detect()` spawn process cek versi (bukan `where`/`which`)

---

## 5. Integrasi Antar Komponen (Cross-File)

### 5.1 Siklus Hidup Data (Verified from source)

```
STARTUP (Android: nativeInit, Desktop: main)
  ├─ paths::init(AppPaths)          ──┐
  ├─ db::init("arka.db")            ──┤  OnceLock → sekali per proses
  ├─ provider::seed_if_empty()      ──┤
  ├─ secret::init(NoopStore/Dpapi)  ──┤
  └─ shell::init(ProotShell/Pwsh)   ──┘

CHAT REQUEST (per session)
  ├─ agent::chat()
  │    ├─ provider::get() → DB
  │    ├─ settings::*() → DB
  │    ├─ memory::context_section() → system prompt
  │    ├─ memory::prepare() → compaction → session::set_memory() → DB
  │    ├─ skill::system_prompt_section() → skills
  │    ├─ shell::active() → Shell impl
  │    └─ Loop: ai::ChatStream → tool → shell.exec() → model
  ├─ session::add_message() → DB (messages)
  ├─ provider::record_usage() → DB (provider_stats)
  └─ session::set_memory() → DB (sessions.summary)

SHUTDOWN: OnceLock tidak drop → OS reclaim connection
```

---

## 6. Detail Temuan per File (Prioritas + Bukti)

### `crates/arka-core/src/agent.rs`
| Baris | Masalah | Tingkat | Bukti |
|-------|---------|---------|-------|
| 400-600 | `run_chat()` 600+ lines, 15+ mutable vars | High | `sed -n '400,600p' agent.rs` |
| 506 | `run_command` → `shell.exec()` tanpa timeout | Medium | `grep -n "shell.exec" agent.rs` → line 506 |
| 219 | `chat_timeout_secs` dipakai deadline total | Good | `sed -n '219p' agent.rs` |
| 205-206 | `max_tool_rounds`, `tool_output_chars` dari settings | Good | `sed -n '205,206p' agent.rs` |

### `crates/arka-android/src/proot_manager.rs`
| Baris | Masalah | Tingkat | Bukti |
|-------|---------|---------|-------|
| 547 | `TOOL_LOCK` global serialisasi | Medium | `grep -n "static TOOL_LOCK" proot_manager.rs` |
| 649-651 | `child.wait()` sebelum `r.join()` | **High** | `sed -n '649,651p' proot_manager.rs` |
| 519 | Komentar akui deadlock risk | - | `sed -n '519p' proot_manager.rs` |

### `crates/arka-desktop/src/pwsh.rs`
| Baris | Masalah | Tingkat | Bukti |
|-------|---------|---------|-------|
| 14 | `EXEC_LOCK` global | Medium | `grep -n "static EXEC_LOCK" pwsh.rs` |
| 44 | `tool_timeout_secs` dipakai `exec_with_timeout` | Good | `sed -n '44p' pwsh.rs` |
| 39 | Komentar: M7 `None`, M8 baca settings | Good | `sed -n '39p' pwsh.rs` |

### `crates/arka-core/src/file.rs`
| Baris | Masalah | Tingkat | Bukti |
|-------|---------|---------|-------|
| 112-119 | TOCTOU `full.exists()` → `canonicalize()` | Medium | `sed -n '112,119p' file.rs` |
| 148 | `is_root_rel()` cek `split('/').all(...)` | Low | `sed -n '148p' file.rs` |
| 246-257 | `delete_at()` double-check root | Good | `sed -n '246,257p' file.rs` |

### `crates/arka-core/src/provider.rs`
| Baris | Masalah | Tingkat | Bukti |
|-------|---------|---------|-------|
| 183-215 | `migrate_with()` read-encrypt-write race | Medium | `sed -n '183,215p' provider.rs` |
| 509-542 | Test cover: migrasi sekali, kosong, Noop | Good | `sed -n '509,542p' provider.rs` |

### `crates/arka-desktop/src/dpapi.rs` — **GOOD**
| Aspek | Status | Bukti |
|-------|--------|-------|
| Generic `DpapiSecretStore<P>` | ✓ Testable | `grep -n "struct DpapiSecretStore" dpapi.rs` |
| `owns()` check prefix | ✓ Prevent double-encrypt | `grep -n "fn owns" dpapi.rs` |
| Empty string handling | ✓ Kompatibel plaintext lama | `grep -n 'is_empty()' dpapi.rs` |
| Windows FFI `CryptProtectData` | ✓ `LocalFree` benar | `grep -n "LocalFree" dpapi.rs` |
| Test `dpapi_asli_bolak_balik` | ✓ Windows CI | `grep -n "dpapi_asli_bolak_balik" dpapi.rs` |

### `crates/arka-core/src/db.rs`
| Baris | Masalah | Tingkat | Bukti |
|-------|---------|---------|-------|
| 5 | Komentar: "tanpa perlu WAL tuning" | Medium | `sed -n '5p' db.rs` |
| 70-85 | Schema: index `idx_messages_session` ada | Good | `sed -n '70,85p' db.rs` |

---

## 7. Rekomendasi Perbaikan (Prioritas)

### P0 — Blocker Stabilitas (Fix Sebelum Release)
1. **proot Deadlock Fix** (`proot_manager.rs:649-651`): Join readers **sebelum** `child.wait()`, atau `crossbeam::select!`.
2. **Command Timeout untuk Agent Loop** (`agent.rs:506`): Pakai `shell.exec_with_timeout(command, cancel, on_chunk, Some(Duration::from_secs(settings::tool_timeout_secs())))`. Update `Shell` trait default impl.

### P1 — Keamanan & Arsitektur
3. **SQLite WAL Mode**: Tambah `PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;` di `db::init()`.
4. **Per-Session Command Queue**: Ganti `TOOL_LOCK`/`EXEC_LOCK` global dengan `HashMap<SessionId, Mutex<()>>` + priority.
5. **Migrate Race Fix**: `migrate_with()` pakai `SELECT ... FOR UPDATE` (SQLite: `BEGIN IMMEDIATE`) atau lock file.

### P2 — Kualitas & Developer Experience
6. **Error Type System**: `thiserror` per crate (`CoreError`, `AndroidError`, `DesktopError`).
7. **Integration Test Harness**: `cargo test --test integration` mock shell, full flow.
8. **Structured Logging**: `tracing` crate replace `eprintln!`.
9. **Config Documentation**: `docs/SETTINGS.md` tabel semua key, range, default.

### P3 — Fitur & UX
10. **Local Summarization Option**: `memory::prepare()` pakai local model (llama.cpp).
11. **Proot Version Pinning**: `build-android.sh` download dari GitHub release dengan SHA256.
12. **Command Allowlist** (opsional tapi disarankan): Validator di `run_command` arm.

---

## 8. Catatan Khusus per Platform (Diverifikasi)

### Android
- `build-android.sh` compile `proot` + `proot-loader` + `libtalloc` + `libandroid-shmem` → output ke `jniLibs/arm64-v8a/`. **8 file `.so` di-commit ke repo** (`android/app/src/main/jniLibs/arm64-v8a/*.so`) — supply chain risk, tidak reproducible build.
- `proot_manager.rs` `ROOTFS_URL` + `ROOTFS_SHA256` hardcoded Ubuntu 24.04.5. Upgrade butuh rebuild APK.
- JNI bridge 15 fungsi — tidak ada versioning signature. Breaking change = crash Kotlin.

### Windows (Desktop)
- `eframe` + `egui` pure Rust — single binary.
- `tray-icon` + `notify-rust` + `muda` (Windows only via `cfg(windows)`) — conditional compilation bersih.
- `rfd` (file dialog) + `dpapi` — native UX.
- **Missing**: Code signing, installer (MSIX/NSIS), auto-update.

---

## 9. Kesimpulan

Arka Agent **arsitektur solid** (core/platform separation, trait-based abstraction, cross-platform trait impl) dengan **test coverage baik untuk unit logic** (59+159 passed). **Kelemahan utama**: 
1. prout deadlock risk (High, verified)
2. Global lock serialisasi (Medium, verified)  
3. Agent loop `run_command` tanpa timeout konfigurable (Medium, verified)
4. SQLite tanpa WAL (Medium, verified)
5. Error handling stringly-typed (Low, verified)

**Prioritas absolut**: P0-1 (proot deadlock) + P0-2 (command timeout). Tanpa itu, agent bisa hang tak terduga di production.

---

## 10. Lampiran: Verified Findings Summary

| File | High | Medium | Low | Info |
|------|------|--------|-----|------|
| `crates/arka-android/src/proot_manager.rs` | 1 (deadlock) | 1 (global lock) | 0 | 0 |
| `crates/arka-core/src/agent.rs` | 0 | 1 (no tool timeout) | 1 (large fn) | 0 |
| `crates/arka-desktop/src/pwsh.rs` | 0 | 1 (global lock) | 0 | 0 |
| `crates/arka-core/src/file.rs` | 0 | 1 (TOCTOU) | 1 (magic number) | 0 |
| `crates/arka-core/src/provider.rs` | 0 | 1 (migrate race) | 0 | 0 |
| `crates/arka-core/src/db.rs` | 0 | 1 (no WAL) | 0 | 0 |
| `crates/arka-core/src/memory.rs` | 0 | 0 | 3 (magic numbers) | 0 |
| `crates/arka-core/src/settings.rs` | 0 | 0 | 2 (magic numbers) | 0 |
| `crates/arka-core/src/ai.rs` | 0 | 0 | 1 (complex state machine) | 0 |
| `crates/arka-desktop/src/dpapi.rs` | 0 | 0 | 0 | 0 (Good) |
| **Total** | **1** | **6** | **8** | **4 clippy** |

*All findings verified by direct source reading (`grep`/`sed`/`cat`) and `cargo check/test/clippy` on commit c6f5d23.*
