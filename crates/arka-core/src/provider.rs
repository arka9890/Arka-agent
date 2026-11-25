//! Provider AI: penyimpanan kredensial + definisi protokol API.
//!
//! Hanya dua bentuk protokol yang didukung karena keduanya menaungi semua
//! provider di PRD: `openai` (OpenAI, Mistral, Google Gemini lewat endpoint
//! kompatibel, dan semua provider custom) dan `anthropic`.

use crate::db;
use rusqlite::params;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Provider {
    pub id: String,
    pub name: String,
    /// `openai` | `anthropic`
    pub kind: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    /// Semua model yang dipilih pengguna di tab Provider (JSON array di DB).
    /// [model] tetap dipakai sebagai model utama saat chat.
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub created_at: i64,
    /// Kunci tersimpan tetapi tidak bisa dibuka (mis. DPAPI milik pengguna
    /// lain / mesin lain). Hanya mungkin terjadi di Windows.
    ///
    /// Field ini **tidak** dikirim ke Kotlin saat `false` (normal), jadi bentuk
    /// JSON untuk Android tidak berubah sama sekali.
    #[serde(default, skip_serializing_if = "is_false")]
    pub key_unreadable: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Provider yang dibuat saat database masih kosong, supaya tab Provider tidak
/// kosong melompong di peluncuran pertama. API key sengaja kosong.
const SEEDS: &[(&str, &str, &str, &str)] = &[
    (
        "OpenAI",
        "openai",
        "https://api.openai.com/v1",
        "gpt-4o-mini",
    ),
    (
        "Mistral",
        "openai",
        "https://api.mistral.ai/v1",
        "mistral-small-latest",
    ),
    (
        "Anthropic",
        "anthropic",
        "https://api.anthropic.com",
        "claude-3-5-sonnet-latest",
    ),
    (
        "Google",
        "openai",
        "https://generativelanguage.googleapis.com/v1beta/openai",
        "gemini-2.0-flash",
    ),
];

pub fn seed_if_empty() -> Result<(), String> {
    db::with(|c| {
        let count: i64 = c
            .query_row("SELECT COUNT(*) FROM providers", [], |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if count > 0 {
            return Ok(());
        }
        for (i, (name, kind, base, model)) in SEEDS.iter().enumerate() {
            c.execute(
                "INSERT INTO providers (id, name, kind, base_url, api_key, model, models, is_default, created_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, '[]', ?6, ?7)",
                params![
                    uuid::Uuid::new_v4().to_string(),
                    name,
                    kind,
                    base,
                    model,
                    if i == 0 { 1 } else { 0 },
                    db::now_ms()
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    })
}

fn row_to_provider(r: &rusqlite::Row) -> rusqlite::Result<Provider> {
    Ok(Provider {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        base_url: r.get(3)?,
        api_key: r.get(4)?,
        model: r.get(5)?,
        models: parse_models(&r.get::<_, String>(8)?),
        is_default: r.get::<_, i64>(6)? != 0,
        created_at: r.get(7)?,
        key_unreadable: false,
    })
}

pub fn list() -> Result<Vec<Provider>, String> {
    db::with(|c| {
        let mut stmt = c
            .prepare(
                "SELECT id, name, kind, base_url, api_key, model, is_default, created_at, models
                 FROM providers ORDER BY is_default DESC, created_at ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_to_provider)
            .map_err(|e| e.to_string())?;
        let mut list = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        for p in &mut list {
            reveal_key(p);
        }
        Ok(list)
    })
}

pub fn get(id: &str) -> Result<Provider, String> {
    let mut p = db::with(|c| {
        c.query_row(
            "SELECT id, name, kind, base_url, api_key, model, is_default, created_at, models
             FROM providers WHERE id = ?1",
            params![id],
            row_to_provider,
        )
        .map_err(|_| "Provider tidak ditemukan".to_string())
    })?;
    reveal_key(&mut p);
    Ok(p)
}

/// Buka API key yang tersimpan (lihat [`crate::secret`]).
///
/// Android: nilai sudah didekripsi Kotlin, store Noop → tidak berubah.
/// Windows: nilai berprefix `dpapi:` dibuka DPAPI; data plaintext lama
/// dikembalikan apa adanya (migrasi dilakukan saat disimpan lagi).
fn reveal_key(p: &mut Provider) {
    if p.api_key.is_empty() {
        return;
    }
    match crate::secret::unprotect(&p.api_key) {
        Ok(plain) => p.api_key = plain,
        Err(_) => {
            // Jangan tampilkan ciphertext sebagai "kunci": UI akan
            // menampilkannya sebagai tidak terbaca dan **tidak menimpanya**
            // saat disimpan (lihat `save`).
            p.api_key.clear();
            p.key_unreadable = true;
        }
    }
}

/// Enkripsi sekali nilai API key yang masih plaintext (data dari versi lama
/// atau dari Android sebelum enkripsi ada).
///
/// Tanpa store yang mengenkripsi (Android: `NoopSecretStore`), fungsi ini
/// tidak mengubah apa pun — nilai tetap seperti semula. Aman dipanggil tiap
/// start: query-nya hanya menyentuh baris yang belum berprefix platform.
pub fn migrate_plaintext_keys() -> Result<usize, String> {
    migrate_with(crate::secret::active())
}

/// Inti [`migrate_plaintext_keys`] dengan store eksplisit, supaya bisa diuji
/// tanpa mendaftarkan store ke registry global.
fn migrate_with(store: &dyn crate::secret::SecretStore) -> Result<usize, String> {
    let rows = db::with(|c| {
        let mut stmt = c
            .prepare("SELECT id, api_key FROM providers WHERE api_key <> ''")
            .map_err(|e| e.to_string())?;
        let it = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?;
        it.collect::<Result<Vec<_>, _>>().map_err(|e| e.to_string())
    })?;

    let mut changed = 0;
    for (id, stored) in rows {
        if store.owns(&stored) {
            continue;
        }
        let protected = store.protect(&stored)?;
        if protected == stored {
            continue; // store Noop → tidak ada gunanya menulis ulang
        }
        db::with(|c| {
            c.execute(
                "UPDATE providers SET api_key = ?2 WHERE id = ?1",
                params![id, protected],
            )
            .map_err(|e| e.to_string())
            .map(|_| ())
        })?;
        changed += 1;
    }
    Ok(changed)
}

fn parse_models(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Provider default; kalau tidak ada yang ditandai, ambil yang pertama.
pub fn default_provider() -> Result<Option<Provider>, String> {
    let all = list()?;
    Ok(all
        .iter()
        .find(|p| p.is_default)
        .cloned()
        .or_else(|| all.first().cloned()))
}

/// Simpan (insert kalau `id` kosong, update kalau sudah ada).
pub fn save(mut p: Provider) -> Result<Provider, String> {
    if p.name.trim().is_empty() {
        return Err("Nama provider wajib diisi".into());
    }
    if p.base_url.trim().is_empty() {
        return Err("Base URL wajib diisi".into());
    }
    p.base_url = p.base_url.trim().trim_end_matches('/').to_string();
    if p.kind != "anthropic" {
        p.kind = "openai".into();
    }
    if p.id.trim().is_empty() {
        p.id = uuid::Uuid::new_v4().to_string();
        p.created_at = db::now_ms();
    } else {
        p.created_at = get(&p.id)
            .map(|old| old.created_at)
            .unwrap_or_else(|_| db::now_ms());
    }
    // Model utama selalu ikut masuk daftar supaya dropdown chat tidak pernah
    // kosong walaupun endpoint /models provider gagal/ tidak tersedia.
    let mut models: Vec<String> = p
        .models
        .iter()
        .map(|m| m.trim().to_string())
        .filter(|m| !m.is_empty())
        .collect();
    if !p.model.trim().is_empty() && !models.contains(&p.model.trim().to_string()) {
        models.insert(0, p.model.trim().to_string());
    }
    let models_json = serde_json::to_string(&models).unwrap_or_else(|_| "[]".into());
    // Kembalikan bentuk yang benar-benar tersimpan: tanpa ini pemanggil menerima
    // `models` lama (model utama belum ikut) — UI lalu menampilkan daftar yang
    // tidak cocok dengan database.
    p.models = models;
    // Android: Noop (Kotlin sudah mengenkripsi). Windows: DPAPI.
    //
    // Kalau kunci tersimpan tidak bisa dibuka (DPAPI mesin lain), kolom di DB
    // masih berisi ciphertext dan form menampilkannya kosong. Menyimpan form
    // seperti itu tidak boleh menghapus kuncinya — jadi nilai lama dipakai.
    let stored_key = if p.key_unreadable && p.api_key.trim().is_empty() {
        db::with(|c| {
            c.query_row(
                "SELECT api_key FROM providers WHERE id = ?1",
                params![p.id],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())
        })
        .unwrap_or_default()
    } else {
        crate::secret::protect(p.api_key.trim())?
    };
    db::with(|c| {
        c.execute(
            "INSERT INTO providers (id, name, kind, base_url, api_key, model, models, is_default, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET name=?2, kind=?3, base_url=?4, api_key=?5,
                model=?6, models=?7, is_default=?8",
            params![
                p.id,
                p.name.trim(),
                p.kind,
                p.base_url,
                stored_key,
                p.model.trim(),
                models_json,
                if p.is_default { 1 } else { 0 },
                p.created_at
            ],
        )
        .map_err(|e| e.to_string())
    })?;
    if p.is_default {
        set_default(&p.id)?;
    }
    Ok(p)
}

/// Simpan daftar model hasil refresh manual ke DB.
pub fn save_models(id: &str, models: &[String]) -> Result<(), String> {
    let json = serde_json::to_string(models).map_err(|e| e.to_string())?;
    db::with(|c| {
        c.execute(
            "UPDATE providers SET models = ?2 WHERE id = ?1",
            params![id, json],
        )
        .map_err(|e| e.to_string())
        .map(|_| ())
    })
}

pub fn set_default(id: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute("UPDATE providers SET is_default = 0", [])
            .map_err(|e| e.to_string())?;
        c.execute(
            "UPDATE providers SET is_default = 1 WHERE id = ?1",
            params![id],
        )
        .map_err(|e| e.to_string())
        .map(|_| ())
    })
}

pub fn delete(id: &str) -> Result<(), String> {
    db::with(|c| {
        c.execute("DELETE FROM providers WHERE id = ?1", params![id])
            .map_err(|e| e.to_string())
            .map(|_| ())
    })
}

// ------------------------------------------------------------ provider stats

use crate::db::{now_ms, with};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ProviderStats {
    pub total_tokens: i64,
    pub total_requests: i64,
    pub last_used_at: Option<i64>,
}

/// Catat penggunaan provider (tokens & request). Dipanggil setelah chat selesai.
pub fn record_usage(provider_id: &str, tokens: i64, requests: i64) -> Result<(), String> {
    with(|c| {
        c.execute(
            "INSERT OR IGNORE INTO provider_stats (provider_id, total_tokens, total_requests)
             VALUES (?1, 0, 0)",
            rusqlite::params![provider_id],
        )
        .map_err(|e| e.to_string())?;

        c.execute(
            "UPDATE provider_stats
             SET total_tokens = total_tokens + ?1,
                 total_requests = total_requests + ?2,
                 last_used_at = ?3
             WHERE provider_id = ?4",
            rusqlite::params![tokens, requests, now_ms(), provider_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}

/// Ambil statistik provider.
#[allow(dead_code)]
pub fn get_stats(provider_id: &str) -> Result<ProviderStats, String> {
    with(|c| {
        c.query_row(
            "SELECT total_tokens, total_requests, last_used_at FROM provider_stats WHERE provider_id = ?1",
            rusqlite::params![provider_id],
            |row| Ok(ProviderStats {
                total_tokens: row.get(0)?,
                total_requests: row.get(1)?,
                last_used_at: row.get::<_, Option<i64>>(2)?,
            }),
        ).map_err(|_| format!("Stats not found for provider {}", provider_id))
    })
}

/// Ambil statistik semua provider (join dengan providers untuk urutan).
pub fn get_all_stats() -> Result<Vec<(String, ProviderStats)>, String> {
    with(|c| {
        let mut stmt = c.prepare(
            "SELECT p.id, COALESCE(ps.total_tokens, 0), COALESCE(ps.total_requests, 0), ps.last_used_at
             FROM providers p
             LEFT JOIN provider_stats ps ON p.id = ps.provider_id
             ORDER BY p.is_default DESC, p.created_at ASC",
        ).map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    ProviderStats {
                        total_tokens: row.get(1)?,
                        total_requests: row.get(2)?,
                        last_used_at: row.get::<_, Option<i64>>(3)?,
                    },
                ))
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::{SecretStore, DPAPI_PREFIX};

    /// Store berprefix, meniru DPAPI tanpa memanggil Windows.
    struct PrefixStore;

    impl SecretStore for PrefixStore {
        fn protect(&self, plain: &str) -> Result<String, String> {
            Ok(format!("{DPAPI_PREFIX}{plain}")) // cukup untuk membuktikan jalur
        }
        fn unprotect(&self, stored: &str) -> Result<String, String> {
            Ok(stored
                .strip_prefix(DPAPI_PREFIX)
                .unwrap_or(stored)
                .to_string())
        }
        fn owns(&self, stored: &str) -> bool {
            stored.starts_with(DPAPI_PREFIX)
        }
    }

    /// Koneksi SQLite bersifat global (OnceLock), jadi test modul ini memakai
    /// database yang sama dengan test lain di binary ini. Dua hal menutup
    /// masalahnya: kunci ini (berjalan bergiliran) dan tabel yang dikosongkan
    /// di awal tiap test.
    static DB_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Database uji bersih. Kembalikan penjaga kunci supaya test tidak saling
    /// menimpa.
    fn init_db() -> std::sync::MutexGuard<'static, ()> {
        let guard = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("arka-provider-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        db::init(&dir.join("provider.db").to_string_lossy()).unwrap();
        db::with(|c| {
            c.execute("DELETE FROM providers", [])
                .map_err(|e| e.to_string())
                .map(|_| ())
        })
        .unwrap();
        guard
    }

    /// Sisipkan baris provider langsung dengan kunci apa adanya.
    fn seed_row(id: &str, api_key: &str) {
        db::with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO providers
                 (id, name, kind, base_url, api_key, model, models, is_default, created_at)
                 VALUES (?1, ?2, 'openai', 'https://contoh/v1', ?3, 'm', '[]', 0, 1)",
                params![id, format!("Uji {id}"), api_key],
            )
            .map_err(|e| e.to_string())
            .map(|_| ())
        })
        .unwrap();
    }

    fn stored_key(id: &str) -> String {
        db::with(|c| {
            c.query_row(
                "SELECT api_key FROM providers WHERE id = ?1",
                params![id],
                |r| r.get::<_, String>(0),
            )
            .map_err(|e| e.to_string())
        })
        .unwrap()
    }

    /// Inti M5: kunci plaintext lama dienkripsi **sekali**, dan kunci yang sudah
    /// terlindungi tidak dienkripsi ulang (kalau dienkripsi ulang terus, DPAPI
    /// akan menghasilkan ciphertext baru setiap start — pemborosan + menyulitkan
    /// audit).
    #[test]
    fn migrasi_plaintext_sekali_saja_dan_aman_diulang() {
        let _guard = init_db();
        seed_row("mig-1", "sk-plaintext-lama");
        seed_row("mig-2", "dpapi:sudah-terlindungi");

        let n = migrate_with(&PrefixStore).unwrap();
        assert_eq!(n, 1, "hanya satu baris plaintext yang diubah");

        assert_eq!(
            stored_key("mig-1"),
            format!("{DPAPI_PREFIX}sk-plaintext-lama")
        );
        assert_eq!(
            stored_key("mig-2"),
            "dpapi:sudah-terlindungi",
            "nilai yang sudah terlindungi tidak boleh disentuh"
        );

        // Dipanggil lagi: tidak ada yang berubah.
        assert_eq!(migrate_with(&PrefixStore).unwrap(), 0);
    }

    /// Kunci kosong bukan plaintext — jangan diberi prefix (kolom kosong harus
    /// tetap kosong supaya UI bisa membedakan "belum diisi" dari "terisi").
    #[test]
    fn migrasi_tidak_menyentuh_kunci_kosong() {
        let _guard = init_db();
        seed_row("mig-kosong", "");
        assert_eq!(migrate_with(&PrefixStore).unwrap(), 0);
        assert_eq!(stored_key("mig-kosong"), "");
    }

    /// Store Noop (Android) tidak mengubah apa pun — enkripsi di sana dilakukan
    /// Kotlin, jadi Rust tidak boleh menulis ulang kolomnya.
    #[test]
    fn migrasi_dengan_noop_tidak_mengubah_apa_pun() {
        let _guard = init_db();
        seed_row("mig-noop", "sk-tetap");
        assert_eq!(migrate_with(&crate::secret::NoopSecretStore).unwrap(), 0);
        assert_eq!(stored_key("mig-noop"), "sk-tetap");
    }

    /// Kasus yang paling mudah salah: database dibawa dari mesin lain sehingga
    /// DPAPI tidak bisa membuka kuncinya. Menyimpan form (yang kuncinya tampil
    /// kosong) **tidak boleh** menghapus kunci lama — kalau tidak, pengguna
    /// kehilangan kunci hanya karena membuka lalu menekan Simpan.
    #[test]
    fn simpan_dengan_kunci_tak_terbaca_mempertahankan_kunci_lama() {
        let _guard = init_db();
        seed_row("tak-terbaca", "dpapi:milik-mesin-lain");

        let mut p = get("tak-terbaca").unwrap();
        p.api_key.clear(); // apa yang dilihat pengguna saat DPAPI gagal
        p.key_unreadable = true;
        p.name = "Nama baru".into();
        save(p).unwrap();

        assert_eq!(
            stored_key("tak-terbaca"),
            "dpapi:milik-mesin-lain",
            "kunci lama terhapus oleh penyimpanan form"
        );
        assert_eq!(get("tak-terbaca").unwrap().name, "Nama baru");
    }

    /// Kebalikannya: kalau pengguna **mengisi** kunci baru, kunci itu yang
    /// dipakai — jangan kembali ke nilai lama.
    #[test]
    fn simpan_dengan_kunci_baru_menimpa_kunci_tak_terbaca() {
        let _guard = init_db();
        seed_row("ganti-kunci", "dpapi:lama");

        let mut p = get("ganti-kunci").unwrap();
        p.api_key = "sk-baru".into();
        p.key_unreadable = true;
        save(p).unwrap();

        assert_eq!(stored_key("ganti-kunci"), "sk-baru"); // store Noop di tes
    }

    /// `save` selalu menaruh model utama di daftar model, supaya dropdown chat
    /// tidak pernah kosong walaupun endpoint /models gagal.
    #[test]
    fn simpan_menambahkan_model_utama_ke_daftar() {
        let _guard = init_db();
        let mut p = Provider {
            id: "model-uji".into(),
            name: "Uji".into(),
            kind: "openai".into(),
            base_url: "https://contoh/v1/".into(), // garis miring di akhir
            api_key: "k".into(),
            model: "gpt-x".into(),
            models: vec!["a".into()],
            is_default: false,
            created_at: 0,
            key_unreadable: false,
        };
        p = save(p).unwrap();
        assert_eq!(p.models, vec!["gpt-x".to_string(), "a".to_string()]);
        assert_eq!(
            p.base_url, "https://contoh/v1",
            "garis miring akhir dibuang"
        );
    }

    /// Statistik ikut tercatat & terbaca ulang.
    #[test]
    fn statistik_token_terakumulasi() {
        let _guard = init_db();
        seed_row("stat-uji", "");
        record_usage("stat-uji", 100, 1).unwrap();
        record_usage("stat-uji", 250, 2).unwrap();
        let s = get_stats("stat-uji").unwrap();
        assert_eq!(s.total_tokens, 350);
        assert_eq!(s.total_requests, 3);
        assert!(s.last_used_at.is_some());
    }
}
