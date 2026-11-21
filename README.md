# Arka Agent

Aplikasi Android (Kotlin + Jetpack Compose) dengan workspace Ubuntu 24 di
perangkat. Chat AI, terminal, dan penjelajah file berjalan tanpa server sendiri:
rootfs Ubuntu dijalankan memakai `proot`, core logika ada di Rust
(`libarka_core.so`).

```
android/   aplikasi Compose (5 tab: Chat, Provider, File, Terminal, Setelan)
rust/      core: SQLite, provider AI, streaming chat, proot, file workspace
docs/      PRD
```

## Build

```sh
# 1. Core Rust → android/app/src/main/jniLibs/arm64-v8a/libarka_core.so
export ANDROID_NDK_HOME=/path/ke/ndk/27.0.12077973
rust/build-android.sh arm64          # atau: rust/build-android.sh all

# 2. APK
cd android && ./gradlew :app:assembleDebug
```

Prasyarat: Rust (stable) dengan target `aarch64-linux-android`, Android NDK r27,
dan JDK 17. Kalau binary proot belum ada di `jniLibs`, terminal tidak akan bisa
menjalankan perintah — lihat `android/app/src/main/jniLibs/README.md`.

## Perilaku saat pertama dibuka

1. `ArkaApp` membuka database + menyiapkan folder.
2. `MainViewModel` membaca status distro; kalau Ubuntu belum terpasang,
   rootfs (±30 MB, `ubuntu-base-24.04.5`) diunduh otomatis lalu
   `apt-get install python3 git curl wget nano unzip less ca-certificates`
   dijalankan. Progres terlihat di tab **Terminal** (dan di **Setelan**).
3. Kalau unduhan gagal (offline), percobaan otomatis diulang paling cepat
   6 jam kemudian; tombol **Pasang sekarang** / **Pasang Ubuntu 24** bisa
   dipakai kapan saja.

Rootfs disimpan di `files/distro/ubuntu/rootfs`, workspace di `files/workspace`
(di dalam Ubuntu: `/root/storage`).
