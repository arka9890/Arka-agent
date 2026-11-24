# Binary native (`jniLibs`)

Folder ini ikut dipaketkan ke APK. Sejak Android 10 aplikasi tidak boleh
mengeksekusi file dari folder datanya, jadi binary yang perlu dijalankan
(proot) harus berada di `nativeLibraryDir` — itulah alasan file di bawah
diberi nama `lib*.so`.

| File | Asal | Ukuran |
|---|---|---|
| `libarka_core.so` | hasil `crates/arka-core/build-android.sh` | — |
| `libproot.so` | paket Termux `proot_5.1.107.102_aarch64.deb` → `usr/bin/proot` | 259.832 B |
| `libproot-loader.so` | paket yang sama → `usr/libexec/proot/loader` | 18.136 B |
| `libtalloc.so` | `libtalloc_2.5.0_aarch64.deb` → `usr/lib/libtalloc.so.2.5.0` | 33.592 B |
| `libandroid-shmem.so` | `libandroid-shmem_0.7_aarch64.deb` → `usr/lib/libandroid-shmem.so` | 14.432 B |

SHA-256 (diverifikasi terhadap mirror resmi Termux):

```
aad737c5b546ee7471f133be90ef20f765aa98a303522557ceed3dff76605085  libproot.so
1da54355333cca3fb6ee10e3b82a24df24fb230c99241ce83809bf9e240609b5  libproot-loader.so
742b438c4d09e276985a61d44164c9de207b6d8cc268f3018cb3001961bcb309  libtalloc.so
84475798e07c8174dbbfaec70a827fdb02f19ffa69a589380c13e7507fd0e731  libandroid-shmem.so
```

`libproot.so` sudah di-patch pada tabel `DT_NEEDED`: `libtalloc.so.2` diubah
jadi `libtalloc.so` (panjang string sama, ditulis dengan NUL tambahan) supaya
linker Android menemukan library langsung di `nativeLibraryDir`. Tanpa patch
ini, setelah aplikasi di-update in-place symlink lama di `proot-libs`
menggantung dan proot gagal dengan `library "libtalloc.so.2" not found`.
Hash asli sebelum patch: `3621937c…f41`.

Catatan lisensi: `proot` dari Termux berlisensi GPL-2.0, `libtalloc` LGPL-3.0,
`libandroid-shmem` MIT. Sertakan sumber/lisensi ini bila APK didistribusikan.

Mengganti versi: unduh `.deb` dari mirror Termux
(`https://packages-cf.termux.dev/apt/termux-main/pool/main/...`), ambil entri
`data.tar.*`, lalu salin ke sini dengan nama di atas (dan perbarui hash di
dokumen ini).
