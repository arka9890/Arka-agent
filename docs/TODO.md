# TODO Arka Agent - 4 Fitur (mudah ke sulit)

Status: hasil audit kode (bukan asumsi). Yang sudah `[x]` benar-benar ada di kode.

## 1. Typing Indicator - PALING MUDAH - SELESAI
- [x] Tambahkan "AI is thinking..." di chat saat menunggu first token
      -> `ChatScreen.kt`: `TypingIndicator()` (3 titik berdenyut + teks "AI sedang mengetik…"), tampil saat `thinking && streaming.isEmpty()`
- [x] Tampilkan sebelum streaming mulai
      -> `MainViewModel.send()`: `_thinking.value = true` saat kirim, state dibaca `vm.thinking.collectAsState()`
- [x] Hilangkan saat streaming selesai
      -> first token di `onDelta` -> `_thinking.value = false`; juga di `finally` & `stopChat()`

## 2. Notifikasi Pesan Baru - SELESAI
- [x] Tambahkan permission POST_NOTIFICATIONS di AndroidManifest
      -> `AndroidManifest.xml`
- [x] Buat NotificationHelper (Kotlin) - channel + show notification
      -> `Notifications.kt` (channel `arka_messages`, `newMessage()`, `ensureChannel()`, `clearAll()`)
- [x] Hook ke chat: saat AI selesai streaming, jika user bukan di tab Chat -> tampilkan
      -> `ChatEngine.finished` (SharedFlow) + `ChatEngine.setVisible()/isHidden()`;
         `AppScreen.kt` set visible hanya saat tab Chat;
         `MainViewModel.init` collect `finished` -> `Notifications.newMessage()`;
         ada toggle setting `notifications` di SettingsScreen
- [x] Request permission runtime (Android 13+)
      -> `MainActivity.onCreate` (RequestPermission launcher) + `Notifications.newMessage` cek izin

## 3. Battery Drain Optimizer - SELESAI
- [x] Audit: apa yang bikin baterai boros? (streaming, polling, wakelock)
      -> tidak ada polling/loop interval sama sekali (`grep "delay(|Timer|while(true)"` = kosong).
         Boros utama: streaming HTTP panjang + foreground service + wake lock.
         HTTP client pakai `connect_timeout` 30s dan timeout total 600s (`rust/src/ai.rs`), jadi koneksi tidak menggantung.
- [x] Streaming: pastikan connection di-close saat tidak dipakai
      -> stream dibaca via `BufReader<Response>` yang di-drop saat selesai;
         tombol Stop -> `repo.cancelChat()` set flag native, loop stream & tool berhenti;
         `ChatEngine.finish()` menghapus buffer/job dan stop foreground service saat tidak ada sesi berjalan (`ArkaChatService.stop`)
- [x] Hapus polling/interval yang tidak perlu
      -> tidak ada polling (audit bersih)
- [x] Tambahkan doze-aware: pause streaming saat screen off (opsional)
      -> `BatteryOptimization.kt` (isIgnoring/requestIntent/settingsIntent) + auto-prompt sekali di `MainViewModel.send`
         + halaman Setelan (status + tombol minta pengecualian)
      -> CATATAN: "pause streaming saat screen off" sengaja TIDAK diimplementasikan; yang dipakai justru sebaliknya
         (foreground service + wake lock supaya jawaban tetap selesai saat layar mati). Bisa diubah kalau mau.

## 4. API Key Encryption - PALING SULIT  ❌ BELUM
- [ ] Buat Kms (Key Management) - generate master key di EncryptedSharedPreferences
      -> belum ada: tidak ada `EncryptedSharedPreferences`/`MasterKey` di kode (`grep -ri encrypt` = kosong)
- [ ] AES-256-GCM encrypt/decrypt API key
      -> belum ada `Cipher`/`AES`/`GCM` sama sekali
- [ ] Update ProviderStore: encrypt saat simpan, decrypt saat baca
      -> `rust/src/provider.rs` + `rust/src/db.rs`: kolom `api_key TEXT NOT NULL` disimpan & dibaca plaintext
- [ ] Migration: jika ada API key plaintext lama, auto-encrypt saat pertama kali dibuka
      -> belum ada
