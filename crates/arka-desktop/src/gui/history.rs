//! Riwayat perintah tab Terminal.
//!
//! Murni, tanpa UI dan tanpa shell — seluruh perilaku (dedupe, batas 200,
//! navigasi ↑/↓) diuji tanpa menjalankan proses apa pun.
//!
//! ## Kenapa menyimpan teks, bukan indeks ke riwayat sesi
//! Karena isi riwayat **berubah** saat perintah baru ditambahkan. Kalau UI
//! hanya menyimpan nomor baris, menekan ↑ setelah menjalankan perintah akan
//! menunjuk baris yang sama sekali berbeda. Yang dikembalikan di sini adalah
//! *nilai* perintahnya, bukan posisinya.

/// Batas jumlah perintah yang disimpan (sama dengan versi Android).
pub const MAX_HISTORY: usize = 200;

/// Riwayat perintah dengan kursor navigasi.
///
/// `cursor == None` berarti "sedang tidak menelusuri" — kotak input memuat
/// draft pengguna. Ini yang membuat ↑ lalu ↓ kembali ke draft kosong, bukan
/// ke perintah terakhir.
#[derive(Clone, Debug, Default)]
pub struct History {
    items: Vec<String>,
    cursor: Option<usize>,
}

impl History {
    /// Simpan satu perintah. Mengembalikan `false` kalau tidak disimpan:
    /// kosong, atau **sama dengan perintah terakhir** (menekan ↑↑↑ setelah satu
    /// perintah tidak boleh mengisi riwayat dengan salinan identik).
    pub fn push(&mut self, cmd: &str) -> bool {
        let cmd = cmd.trim();
        if cmd.is_empty() {
            return false;
        }
        if self.items.last().map(String::as_str) == Some(cmd) {
            self.cursor = None;
            return false;
        }
        self.items.push(cmd.to_string());
        // Buang yang tertua, bukan yang terbaru: pengguna lebih sering
        // mengulang perintah baru daripada perintah dari sesi jauh.
        while self.items.len() > MAX_HISTORY {
            self.items.remove(0);
        }
        self.cursor = None;
        true
    }

    /// Perintah sebelumnya (↑). `None` = riwayat kosong.
    pub fn prev(&mut self) -> Option<String> {
        if self.items.is_empty() {
            return None;
        }
        let idx = match self.cursor {
            Some(i) => i.saturating_sub(1),
            None => self.items.len() - 1,
        };
        self.cursor = Some(idx);
        Some(self.items[idx].clone())
    }

    /// Perintah berikutnya (↓). `None` = kembali ke draft (kursor dilepas).
    pub fn next(&mut self) -> Option<String> {
        let i = self.cursor?;
        if i + 1 >= self.items.len() {
            self.cursor = None;
            return None;
        }
        self.cursor = Some(i + 1);
        Some(self.items[i + 1].clone())
    }

    /// Lepaskan kursor (dipanggil setelah perintah dijalankan atau saat
    /// pengguna mengetik sendiri).
    pub fn reset(&mut self) {
        self.cursor = None;
    }

    pub fn is_navigating(&self) -> bool {
        self.cursor.is_some()
    }

    /// Terbaru lebih dulu — urutan yang diharapkan untuk dropdown riwayat.
    pub fn items_newest_first(&self) -> Vec<String> {
        self.items.iter().rev().cloned().collect()
    }

    pub fn items(&self) -> &[String] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.cursor = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kosong_dan_spasi_tidak_disimpan() {
        let mut h = History::default();
        assert!(!h.push(""));
        assert!(!h.push("   "));
        assert!(h.is_empty());
        assert_eq!(h.prev(), None);
    }

    #[test]
    fn perintah_identik_berurutan_tidak_diduplikasi() {
        let mut h = History::default();
        assert!(h.push("git status"));
        assert!(
            !h.push("git status"),
            "salinan identik tidak perlu disimpan"
        );
        assert!(h.push("git log"));
        assert!(h.push("git status"), "bukan berurutan → tetap disimpan");
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn spasi_di_tepi_dipangkas() {
        let mut h = History::default();
        h.push("  git status  ");
        assert_eq!(h.items(), &["git status".to_string()]);
    }

    #[test]
    fn batas_dua_ratus_membuang_yang_tertua() {
        let mut h = History::default();
        for i in 0..250 {
            h.push(&format!("perintah-{i}"));
        }
        assert_eq!(h.len(), MAX_HISTORY);
        assert_eq!(h.items().first().unwrap(), "perintah-50");
        assert_eq!(h.items().last().unwrap(), "perintah-249");
    }

    #[test]
    fn panah_atas_menelusuri_dari_terbaru_lalu_berhenti_di_tertua() {
        let mut h = History::default();
        h.push("a");
        h.push("b");
        h.push("c");

        assert_eq!(h.prev().as_deref(), Some("c"));
        assert_eq!(h.prev().as_deref(), Some("b"));
        assert_eq!(h.prev().as_deref(), Some("a"));
        // Sudah di tertua: tetap "a", tidak membungkus ke "c" — membungkus
        // membuat pengguna tidak sadar sudah kehabisan riwayat.
        assert_eq!(h.prev().as_deref(), Some("a"));
    }

    #[test]
    fn panah_bawah_kembali_ke_draft_bukan_membungkus() {
        let mut h = History::default();
        h.push("a");
        h.push("b");

        assert_eq!(h.prev().as_deref(), Some("b"));
        assert_eq!(h.prev().as_deref(), Some("a"));
        assert_eq!(h.next().as_deref(), Some("b"));
        assert_eq!(h.next(), None, "melewati yang terbaru = kembali ke draft");
        assert!(!h.is_navigating());
        assert_eq!(
            h.prev().as_deref(),
            Some("b"),
            "↑ sesudahnya mulai lagi dari terbaru"
        );
    }

    #[test]
    fn menambah_perintah_melepas_kursor() {
        let mut h = History::default();
        h.push("a");
        h.push("b");
        h.prev();
        assert!(h.is_navigating());
        h.push("c");
        assert!(!h.is_navigating());
        assert_eq!(h.prev().as_deref(), Some("c"));
    }

    #[test]
    fn dropdown_terbaru_lebih_dulu() {
        let mut h = History::default();
        h.push("a");
        h.push("b");
        assert_eq!(
            h.items_newest_first(),
            vec!["b".to_string(), "a".to_string()]
        );
    }
}
