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

## Update / install di atas versi lama

APK dari CI dan build lokal memakai **kunci yang sama**, jadi cukup install
APK baru di atas yang lama — tidak perlu uninstall (data chat & workspace tetap
ada):

- `android/app/ci-debug.keystore` adalah kunci *debug* bersama yang sengaja
  di-commit dengan password publik (`arka1234`, alias `arkaci`). Ini bukan
  kredensial rahasia; hanya dipakai supaya signature tiap build identik.
- Kalau Anda punya keystore sendiri, taruh di `android/arkacode.keystore` dan
  buat `android/keystore.properties` sesuai contoh di
  `android/keystore.properties.example`. Kunci itu **tidak** di-commit dan
  prioritasnya lebih tinggi dari kunci CI.

Catatan: APK lama yang ditandatangani kunci *random* bawaan runner harus
di-uninstall **satu kali** sebelum bisa menerima update dari build terbaru.
Sesudah itu semua build berikutnya bisa di-update di tempat.

## Perilaku chat (agent)

Saat sesi chat dibuat, aplikasi menyisipkan pesan sistem + tool `run_command`.
Kalau Anda meminta model mengecek/menjalankan sesuatu di Ubuntu, model
memanggil tool itu; aplikasi menjalankan perintah lewat proot, hasilnya
ditampilkan di tab **Terminal**, lalu dikirim balik ke model supaya ia bisa
menjawab berdasarkan output asli. Maksimal 6 putaran tool per pesan.

Provider harus mendukung *function calling* (OpenAI-compatible atau Anthropic).
Kalau tidak, aplikasi otomatis fallback ke chat biasa tanpa tool.

Dropdown model di header chat **tidak** mengecek server setiap kali dibuka —
daftar diambil dari DB (instan). Tekan tombol **refresh** di dropdown untuk
mengecek model terbaru dari server; di tab Provider juga ada tombol
**Cek dari server**. Hasil refresh disimpan, jadi pemakaian berikutnya cepat.
