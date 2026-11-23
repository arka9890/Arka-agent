# Arka Agent

**Status Build:** ✅ [![CI Build](https://github.com/[OWNER]/Arka-agent/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/[OWNER]/Arka-agent/actions)
**Latest Commit:** [`7b8cdf7`](https://github.com/[OWNER]/Arka-agent/commit/7b8cdf7) (Fix: Output truncation, GitHub rate limit, cleanup)
**Last Updated:** 23 November 2026

---

## 📌 Tentang
Arka Agent adalah asisten AI di perangkat Android yang menjalankan **workspace Ubuntu 24.04** asli via **proot**.
Fitur utama:
- Eksekusi perintah Bash langsung di workspace Ubuntu
- Manajemen skill (Claude Code format) dengan approval UI
- Dukungan multi-provider AI (OpenAI, Anthropic)

---

## 🛠 Fitur Terbaru
| Fitur | Status | Commit |
|---|---|---|
| Skill Management (proot) | ✅ Aktif | `5129db5` |
| Approval UI (User Confirmation) | ✅ Aktif | `5129db5` |
| Fix: Hapus file `.backup` dari git | ✅ Done | `9e8b577` |
| Fix: Deteksi rate limit GitHub | ✅ Done | `6fc4e15` |
| Fix: Output truncation marker | ✅ Done | `7b8cdf7` |

---
## 📦 Build
- **CI Status:** ✅ Sukses (GitHub Actions - `assembleDebug`)
- **Cara Build Lokal:**
  ```bash
  cd /root/storage/Arka-agent
  ./rust/build-android.sh arm64
  cd android && ./gradlew assembleDebug
  ```
- **Artifact:** `android/app/build/outputs/apk/debug/app-debug.apk`

---

## 📂 Struktur Proyek
```
Arka-agent/
├── android/          # Android App (Kotlin + Jetpack Compose)
├── rust/             # Rust Library (JNI)
├── docs/             # Dokumentasi (PRD, TODO, AUDIT)
├── .github/workflows/ # GitHub Actions (CI)
└── README.md
```

---

## 🤝 Kontribusi
1. Fork repo
2. Buat branch (`git checkout -b fitur-baru`)
3. Commit (`git commit -m "feat: deskripsi"`)
4. Push & buka PR

---

## 📜 Lisensi
MIT
