# TODO Audit Superpowers — Arka Agent

Status: audit superpowers @ 5129db5. Item dikerjakan berurutan (I dulu, lalu M).
Checklist dicentang HANYA jika CI build (`assembleDebug`) sukses di GitHub.

## Important
- [ ] I1: Hapus file `.backup` dari git track (ChatScreen.kt.backup, MessageBubble.kt.backup) + tambah `*.backup` ke .gitignore
- [ ] I2: `skill.rs` `found.truncate(20)` — beri peringatan/error eksplisit bila repo >20 SKILL.md
- [ ] I3: `skill.rs` — deteksi rate limit GitHub (403/429), pesan jelas + saran token

## Minor
- [ ] M1: `ai.rs` — satukan `tools_schema()` (OpenAI) & `anthropic_tools_schema()` (Anthropic) jadi satu builder
- [ ] M2: `lib.rs` — tambah penanda `…(terpotong)` pada `tail_chars` / `tool_output_chars`
- [ ] M3: Git history — 3 commit Revert sebelum commit final (opsional)
- [ ] M4: API Key Encryption (dari TODO.md lama) — tetap BELUM, prioritas lanjutan

## Verifikasi
- [ ] CI `assembleDebug` sukses di GitHub Actions (build & signature APK)
