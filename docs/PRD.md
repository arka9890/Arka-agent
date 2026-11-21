# PRD (Product Requirements Document) — Arka Agent

Status: draft · Dokumen ini adalah sumber kebenaran kebutuhan produk Arka Agent.

## 1. Overview

### 1.1. Nama Proyek
**Arka Agent**

### 1.2. Deskripsi Proyek
**Arka Agent** adalah **aplikasi AI Agent untuk Android** yang memungkinkan user untuk:
- **Berinteraksi dengan AI** (seperti GPT-4, Llama, Mistral, dll.) via **API Provider**.
- **Menjalankan tools Linux** (Python, Git, Bash, dll.) **langsung di device** menggunakan **PRoot + Ubuntu 24**.
- **Mengelola file** (preview, unduh, hapus) di **workspace Ubuntu 24**.
- **Menggunakan terminal** terintegrasi untuk menjalankan perintah Linux.

Aplikasi ini **membundel PRoot binary** di APK dan **auto-download Ubuntu 24 rootfs** (~500MB) saat pertama dibuka, sehingga **tidak memerlukan aplikasi lain** (Termux/UserLAnd).

### 1.3. Tujuan Proyek
1. Memberikan **pengalaman AI Agent yang lengkap** di Android.
2. Memungkinkan **eksekusi tools Linux** (Python, Git, dll.) **tanpa server eksternal**.
3. Menyediakan **workspace terisolasi** (Ubuntu 24) untuk **AI dan tools**.
4. **Preview file** (teks, gambar, video, PDF) **langsung di chat dan tab File**.
5. **UI modern** dengan **Claymorphism + Minimal Design**.

### 1.4. Target User
| **User Type** | **Deskripsi** | **Use Case** |
|---------------|---------------|--------------|
| **Developer** | Pengembang yang ingin menjalankan script Python/Git di Android | Menjalankan script, debugging, testing |
| **AI Enthusiasts** | Pengguna yang ingin berinteraksi dengan AI lokal/remote | Chat dengan AI, panggil tools |
| **Power Users** | User yang membutuhkan terminal Linux di Android | Eksekusi perintah Linux, manajemen file |

### 1.5. Platform & Persyaratan
| **Platform** | **Detail** |
|--------------|------------|
| **OS** | Android 7.0+ (API 24+) |
| **Arsitektur** | ARM64 (default), ARMv7 (opsional) |
| **Storage** | Minimal **1GB** (untuk rootfs + tools) |
| **RAM** | Minimal **2GB** (rekomendasi 4GB+) |
| **Internet** | Wajib (untuk download rootfs dan panggil API AI) |

## 2. Fitur Utama

### 2.1. Fitur Core
| **Fitur** | **Deskripsi** | **Prioritas** | **Status** |
|-----------|---------------|---------------|------------|
| **AI Chat** | Chat dengan AI (OpenAI, Mistral, Anthropic, dll.) via API Provider | ⭐⭐⭐⭐⭐ | ✅ |
| **PRoot + Ubuntu 24** | Environment Linux untuk menjalankan tools (Python, Git, dll.) | ⭐⭐⭐⭐⭐ | ✅ |
| **Auto-Download Ubuntu 24** | Download rootfs (~500MB) saat pertama aplikasi dibuka | ⭐⭐⭐⭐⭐ | ✅ |
| **Auto-Install Tools** | Install Python, Git, curl, wget, dll. otomatis | ⭐⭐⭐⭐⭐ | ✅ |
| **File Preview (Chat)** | Preview file (teks, gambar, video) langsung di chat | ⭐⭐⭐⭐ | ✅ |
| **File Preview (Tab File)** | Preview file (teks, gambar, video, PDF) di tab File | ⭐⭐⭐⭐ | ✅ |

### 2.2. Fitur UI/UX
| **Fitur** | **Deskripsi** | **Prioritas** | **Status** |
|-----------|---------------|---------------|------------|
| **Claymorphism UI** | Desain modern dengan efek 3D (soft shadow, border radius, dll.) | ⭐⭐⭐⭐ | ✅ |
| **5 Tabs** | Chat, Provider, File, Terminal, Settings | ⭐⭐⭐⭐⭐ | ✅ |
| **Sidebar (Session History)** | Riwayat sesi chat (buat, hapus, rename) | ⭐⭐⭐⭐ | ✅ |
| **Dark/Light Mode** | Tema gelap/terang | ⭐⭐⭐ | ✅ |
| **Multi-Language** | Bahasa Indonesia/English/Japanese | ⭐⭐ | ✅ |

### 2.3. Fitur Tambahan
| **Fitur** | **Deskripsi** | **Prioritas** | **Status** |
|-----------|---------------|---------------|------------|
| **Session Management** | Kelola sesi chat (buat, hapus, rename, switch) | ⭐⭐⭐⭐ | ✅ |
| **Provider Management** | Kelola API Provider (OpenAI, Mistral, Custom) | ⭐⭐⭐⭐ | ✅ |
| **Terminal** | Terminal terintegrasi (Ubuntu 24) | ⭐⭐⭐⭐⭐ | ✅ |
| **System Stats** | Tampilkan penggunaan RAM/CPU/Storage | ⭐⭐⭐ | ✅ |
| **File Download** | Unduh file dari workspace | ⭐⭐⭐ | ✅ |
| **Syntax Highlighting** | Highlight code di chat (opsional) | ⭐⭐ | ⚠️ |
| **Command History** | Riwayat perintah terminal | ⭐⭐ | ⚠️ |

## 3. Spesifikasi Teknis

### 3.1. Arsitektur Sistem
```
┌───────────────────────────────────────────────────────────────────────────────┐
│                            Arka Agent (Android)                              │
├───────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│  ┌───────────────────────────────────────────────────────────────────────┐   │
│  │                        UI Layer (Kotlin)                              │   │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐       │   │
│  │  │  Chat   │  │ Provider│  │   File  │  │ Terminal│  │Settings │       │   │
│  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘  └─────────┘       │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                               │
│  ┌───────────────────────────────────────────────────────────────────────┐   │
│  │                     Logic Layer (Rust + JNI)                          │   │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐         │   │
│  │  │   AI Logic      │  │  PRoot Manager  │  │  File Manager  │         │   │
│  │  │ (API Provider)  │  │ (Ubuntu 24)     │  │ (Workspace)     │         │   │
│  │  └─────────────────┘  └─────────────────┘  └─────────────────┘         │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                               │
│  ┌───────────────────────────────────────────────────────────────────────┐   │
│  │                     PRoot + Ubuntu 24 Layer                           │   │
│  │  ┌───────────────────────────────────────────────────────────────┐   │   │
│  │  │  Ubuntu 24 Rootfs (Auto-Downloaded)                            │   │   │
│  │  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐            │   │   │
│  │  │  │ Python  │  │   Git   │  │  Bash   │  │  Tools  │            │   │   │
│  │  │  └─────────┘  └─────────┘  └─────────┘  └─────────┘            │   │   │
│  │  └───────────────────────────────────────────────────────────────┘   │   │
│  └───────────────────────────────────────────────────────────────────────┘   │
│                                                                               │
└───────────────────────────────────────────────────────────────────────────────┘
```

### 3.2. Teknologi yang Digunakan
| **Layer** | **Teknologi** | **Kegunaan** |
|-----------|---------------|--------------|
| **UI** | Kotlin, Jetpack Compose (opsional), Material 3 | Tampilan aplikasi |
| **Logic** | Rust + JNI | Business logic, PRoot integration |
| **Database** | SQLite (via `rusqlite`) | Penyimpanan sesi, pesan, provider, dll. |
| **PRoot** | PRoot Binary (ARM64) | Environment Linux |
| **Ubuntu 24** | Ubuntu 24 Rootfs (~500MB) | Tools (Python, Git, dll.) |
| **AI** | API Provider (OpenAI, Mistral, Anthropic) | Chat dengan AI |
| **File Preview** | Glide (gambar), ExoPlayer (video), PDFViewer (PDF) | Preview file |
| **Build** | Cargo (Rust), Gradle (Android) | Build sistem |

### 3.3. Struktur Proyek
```
arka-agent/
├── android/                          # Kotlin (UI)
│   ├── app/
│   │   ├── src/main/
│   │   │   ├── java/com/arka/agent/
│   │   │   │   ├── MainActivity.kt
│   │   │   │   ├── fragments/
│   │   │   │   │   ├── ChatFragment.kt
│   │   │   │   │   ├── ProviderFragment.kt
│   │   │   │   │   ├── FileFragment.kt
│   │   │   │   │   ├── TerminalFragment.kt
│   │   │   │   │   └── SettingsFragment.kt
│   │   │   │   ├── adapters/
│   │   │   │   │   ├── ChatAdapter.kt
│   │   │   │   │   ├── FileAdapter.kt
│   │   │   │   │   └── TerminalAdapter.kt
│   │   │   │   ├── models/
│   │   │   │   │   ├── Message.kt
│   │   │   │   │   ├── Session.kt
│   │   │   │   │   ├── FileInfo.kt
│   │   │   │   │   └── Provider.kt
│   │   │   │   └── utils/
│   │   │   │       ├── FileUtils.kt
│   │   │   │       └── Extensions.kt
│   │   │   └── res/
│   │   │       ├── layout/
│   │   │       ├── drawable/
│   │   │       └── values/
│   │   └── build.gradle
│   └── gradle.properties
│
├── rust/                             # Rust (Logic)
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                    # JNI Exports
│   │   ├── proot_manager.rs          # PRoot + Ubuntu 24
│   │   ├── ai.rs                     # AI Logic
│   │   ├── file.rs                   # File Management
│   │   ├── session.rs                # Session Management
│   │   ├── provider.rs               # Provider Management
│   │   ├── terminal.rs               # Terminal Logic
│   │   └── settings.rs               # Settings Logic
│   └── build.rs
│
├── assets/                           # Assets
│   └── proot                         # PRoot binary (ARM64)
│
├── docs/                             # Dokumentasi
│   ├── PRD.md                        # Product Requirements Document
│   ├── ARCHITECTURE.md               # Arsitektur Sistem
│   └── API.md                        # API Documentation
│
└── README.md
```

## 4. Spesifikasi Fitur

### 4.1. AI Chat
| **Fitur** | **Deskripsi** |
|-----------|---------------|
| **Pilihan Model** | Dropdown untuk pilih model AI (GPT-4, Llama 3, Mistral, dll.) |
| **Pesan AI/User** | Bubble chat dengan warna berbeda (AI: ungu, User: putih) |
| **File Preview** | Preview file (teks, gambar, video) langsung di chat |
| **Typing Indicator** | Tampilkan "AI is typing..." saat AI sedang memproses |
| **Streaming Response** | Tampilkan respons AI secara real-time (token per token) |
| **Session Management** | Simpan riwayat pesan per sesi (SQLite) |
| **Markdown Support** | Format pesan AI dengan markdown (code blocks, bold, italic) |

### 4.2. PRoot + Ubuntu 24
| **Fitur** | **Deskripsi** |
|-----------|---------------|
| **Auto-Download Rootfs** | Download Ubuntu 24 rootfs (~500MB) saat pertama aplikasi dibuka |
| **Auto-Install Tools** | Install Python 3, Git, curl, wget, bash, coreutils, dll. otomatis |
| **Environment Setup** | Setup `.bashrc`, `PATH`, `HOME`, dll. |
| **Command Execution** | Jalankan perintah Linux di Ubuntu 24 via PRoot |
| **File System Access** | Akses file di `/root/storage` (workspace) |
| **Error Handling** | Tangani error (rootfs tidak ditemukan, perintah gagal, dll.) |

### 4.3. File Preview
| **Jenis File** | **Preview di Chat** | **Preview di Tab File** | **Teknologi** |
|----------------|---------------------|-------------------------|---------------|
| **Teks** (TXT, JSON, Python, dll.) | Isi file (markdown, max 10 baris) | Full content + syntax highlighting | Rust (`std::fs`) |
| **Gambar** (PNG, JPG, GIF, WebP) | Thumbnail (200x200) + tombol "Buka" | Thumbnail + fullscreen | Glide |
| **Video** (MP4, MKV, AVI) | Thumbnail + tombol "Putar" | Thumbnail + ExoPlayer | ExoPlayer |
| **PDF** | Metadata (nama, ukuran, halaman) + tombol "Buka" | Halaman pertama | PDFViewer |
| **Binary** (ZIP, EXE, dll.) | Metadata (nama, ukuran, jenis) | Metadata + tombol "Unduh" | - |

### 4.4. Terminal
| **Fitur** | **Deskripsi** |
|-----------|---------------|
| **Command Execution** | Jalankan perintah Linux (Python, Git, Bash, dll.) |
| **Output Display** | Tampilkan output perintah di RecyclerView |
| **Command History** | Simpan riwayat perintah (opsional) |
| **Auto-Complete** | Saran perintah (opsional) |
| **Syntax Highlighting** | Highlight syntax perintah (opsional) |
| **Clear Terminal** | Bersihkan output terminal |

### 4.5. Provider Management
| **Fitur** | **Deskripsi** |
|-----------|---------------|
| **Daftar Provider** | OpenAI, Mistral, Anthropic, Google, Custom (OpenAI-Compatible) |
| **Custom Provider** | Input API Key + Base URL untuk provider custom |
| **API Key Management** | Simpan, hapus, edit API Key (enkripsi opsional) |
| **Test Connection** | Cek koneksi ke provider (ping API) |
| **Default Provider** | Set provider default untuk sesi baru |
| **Provider Stats** | Tampilkan usage (token, request, dll.) |

### 4.6. Settings
| **Fitur** | **Deskripsi** | **Opsi** |
|-----------|---------------|----------|
| **Theme** | Tema aplikasi | Dark / Light / System |
| **Language** | Bahasa aplikasi | Indonesia / English / Japanese |
| **Font Size** | Ukuran font | Small / Medium / Large |
| **Auto-Scroll Chat** | Otomatis scroll ke pesan terbaru | On / Off |
| **Notifications** | Notifikasi pesan baru | On / Off |
| **System Stats** | Tampilkan penggunaan RAM/CPU/Storage | - |
| **Workspace Path** | Ubah path workspace | Default: `/root/storage` |
| **About** | Info aplikasi (versi, changelog, lisensi) | - |

## 5. Rencana Pengembangan

### 5.1. Milestone
| **Milestone** | **Fitur** | **Target Waktu** | **Status** |
|---------------|-----------|------------------|------------|
| **M1: Core Setup** | PRoot + Ubuntu 24, AI Chat | 2 minggu | ⏳ |
| **M2: UI/UX** | Claymorphism UI, 5 Tabs, Sidebar | 2 minggu | ⏳ |
| **M3: File Preview** | Text/Image/Video Preview | 1 minggu | ⏳ |
| **M4: Terminal** | Terminal terintegrasi | 1 minggu | ⏳ |
| **M5: Provider Management** | Kelola API Provider | 1 minggu | ⏳ |
| **M6: Settings** | Theme, Language, System Stats | 1 minggu | ⏳ |
| **M7: Testing** | Testing + Bug Fixes | 2 minggu | ⏳ |
| **M8: Release** | Release v1.0 (APK) | 1 minggu | ⏳ |

### 5.2. Prioritas Fitur
| **Prioritas** | **Fitur** | **Keterangan** |
|---------------|-----------|----------------|
| **P0 (Critical)** | PRoot + Ubuntu 24, AI Chat, File Preview (Teks) | Core functionality |
| **P1 (High)** | UI Claymorphism, Terminal, File Preview (Gambar/Video) | Fitur utama |
| **P2 (Medium)** | Provider Management, Settings, Session Management | Fitur pendukung |
| **P3 (Low)** | Syntax Highlighting, Command History, Auto-Complete | Fitur opsional |

## 6. Persyaratan Sistem

### 6.1. Device Requirements
| **Persyaratan** | **Minimum** | **Recommended** | **Catatan** |
|-----------------|-------------|-----------------|-------------|
| **OS** | Android 7.0 (API 24) | Android 10+ | PRoot butuh API 24+ |
| **Arsitektur** | ARM64 | ARM64 | Default (PRoot binary ARM64) |
| **RAM** | 2GB | 4GB+ | Ubuntu 24 butuh RAM besar |
| **Storage** | 1GB | 2GB+ | Rootfs (~500MB) + Tools (~200MB) |
| **Internet** | Wajib | - | Download rootfs + panggil API AI |

### 6.2. Permission
| **Permission** | **Kegunaan** | **Status** |
|----------------|--------------|------------|
| `INTERNET` | Download rootfs, panggil API AI | ✅ Wajib |
| `READ_EXTERNAL_STORAGE` | Baca file dari storage | ✅ Wajib |
| `WRITE_EXTERNAL_STORAGE` | Tulis file ke storage | ✅ Wajib |
| `REQUEST_INSTALL_PACKAGES` | (Opsional) | ❌ Tidak diperlukan |

## 7. Dependencies

### 7.1. Android Dependencies (`build.gradle`)
```gradle
dependencies {
    // UI
    implementation 'androidx.appcompat:appcompat:1.6.1'
    implementation 'com.google.android.material:material:1.11.0'
    implementation 'androidx.recyclerview:recyclerview:1.3.2'
    implementation 'androidx.cardview:cardview:1.0.0'
    implementation 'androidx.drawerlayout:drawerlayout:1.2.0'

    // Image Loading
    implementation 'com.github.bumptech.glide:glide:4.16.0'
    annotationProcessor 'com.github.bumptech.glide:compiler:4.16.0'

    // Video Player
    implementation 'com.google.android.exoplayer:exoplayer:2.19.1'

    // PDF Viewer
    implementation 'com.github.barteksc:android-pdf-viewer:3.2.0'

    // JSON Parsing
    implementation 'com.google.code.gson:gson:2.10.1'

    // Compression (untuk extract tar.gz)
    implementation 'org.apache.commons:commons-compress:1.21'

    // Networking
    implementation 'com.squareup.okhttp3:okhttp:4.12.0'
}
```

### 7.2. Rust Dependencies (`Cargo.toml`)
```toml
[dependencies]
jni = "0.21"                  # JNI bindings
anyhow = "1.0"                # Error handling
serde = { version = "1.0", features = ["derive"] }  # Serialization
serde_json = "1.0"            # JSON parsing
tokio = { version = "1.0", features = ["full", "rt-multi-thread"] }  # Async runtime
reqwest = { version = "0.11", features = ["json"] }  # HTTP client
rusqlite = "0.30"             # SQLite
uuid = { version = "1.0", features = ["v4"] }  # UUID generator
chrono = "0.4"                 # Date/time
lazy_static = "1.4"          # Lazy initialization
regex = "1.0"                  # Regex
image = "0.24"                 # Image processing (untuk dimensions)
```

## 8. Catatan Tambahan

### 8.1. Keamanan
| **Risiko** | **Solusi** | **Status** |
|------------|------------|------------|
| **Perintah berbahaya** | Validasi perintah (whitelist tools: `python3`, `git`, `ls`, dll.) | ✅ |
| **Akses storage** | Gunakan `WRITE_EXTERNAL_STORAGE` + validasi path | ✅ |
| **API Key bocor** | Enkripsi API Key dengan AES (opsional) | ⚠️ |
| **Rootfs korup** | Verifikasi checksum rootfs (SHA-256) | ⚠️ |
| **Path Traversal** | Validasi path file (tidak boleh keluar dari `/root/storage`) | ✅ |

### 8.2. Performansi
| **Masalah** | **Solusi** | **Status** |
|-------------|------------|------------|
| **Ukuran rootfs besar** (~500MB) | Auto-download on first launch | ✅ |
| **Waktu setup lama** | Tampilkan progress bar + pesan "Sedang mengunduh Ubuntu 24..." | ✅ |
| **File besar (video/PDF)** | Gunakan thumbnail + streaming (ExoPlayer) | ✅ |
| **Memory usage** | Batasi ukuran thumbnail (200x200) | ✅ |
| **Battery drain** | Optimasi PRoot (tidak jalankan service yang tidak diperlukan) | ⚠️ |

### 8.3. Distribusi
| **Metode** | **Ukuran** | **Catatan** |
|------------|------------|-------------|
| **APK** | ~1-2MB (PRoot binary saja) | Bisa didistribusikan langsung |
| **Ubuntu 24 Rootfs** | ~500MB | Auto-download saat pertama dibuka |
| **Tools** | ~200MB | Auto-install di Ubuntu 24 |
| **Total Storage** | ~700MB - 1GB | Tergantung tools yang diinstall |
| **Play Store** | ❌ Tidak bisa | Ukuran melebihi batas (150MB) |

### 8.4. Hosting Ubuntu 24 Rootfs
| **Opsi** | **Keuntungan** | **Keterbatasan** | **Rekomendasi** |
|----------|----------------|------------------|-----------------|
| **GitHub Releases** | Gratis, mudah | Lambat untuk file besar | ✅ **Best** |
| **Google Drive** | Sederhana | Lambat, batas download | ⚠️ |
| **Server Sendiri** (VPS) | Cepat, fleksibel | Butuh biaya | ⚠️ |
| **Termux Mirror** | Cepat, terpercaya | Tidak semua file tersedia | ⚠️ |

## 9. Kesimpulan

**Arka Agent** adalah **aplikasi AI Agent untuk Android** yang **lengkap, fleksibel, dan modern**, dengan fitur-fitur utama:
- ✅ **PRoot + Ubuntu 24** (auto-download & setup)
- ✅ **AI Chat** (OpenAI, Mistral, Anthropic, dll.)
- ✅ **File Preview** (teks, gambar, video, PDF)
- ✅ **Terminal** terintegrasi
- ✅ **UI Claymorphism** (modern + minimal)

### Next Steps
1. **Implementasi Core** (PRoot + Ubuntu 24).
2. **Implementasi UI** (Claymorphism + 5 Tabs).
3. **Implementasi Fitur** (Chat, File Preview, Terminal).
4. **Testing & Bug Fixes**.
5. **Release v1.0** (APK).

### Catatan Akhir
- **Tidak butuh Play Store**: APK bisa didistribusikan langsung.
- **Tidak butuh aplikasi lain**: PRoot + Ubuntu 24 terbundel/auto-download.
- **Fleksibel**: Bisa tambah provider AI, tools, atau fitur baru.
