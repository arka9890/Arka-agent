//! Markdown ringan untuk bubble chat: blok kode + inline (tebal/miring/kode).
//!
//! Port dari `splitCodeBlocks` + `inlineMarkdown` di `MessageBubble.kt`
//! (Android). Aturan pemisahan blok **sengaja identik**, termasuk mengambil
//! nama bahasa dari baris pertama pagar kode.
//!
//! Catatan jujur soal tebal: egui 0.32 tidak punya *synthetic bold*, dan font
//! yang dibundel (InterVariable) hanya satu bobot. Jadi `**tebal**` dirender
//! dengan warna penekanan + jarak huruf sedikit lebih lebar — **bukan** bobot
//! huruf yang lebih tebal. Miring bisa asli (`TextFormat::italics` memiringkan
//! glyph). Kalau nanti ada berkas Inter SemiBold, ganti
//! [`InlineStyle::font`] lewat family baru dan hasilnya jadi tebal sungguhan
//! tanpa mengubah pemanggil.

use eframe::egui::text::{LayoutJob, TextFormat};
use eframe::egui::{Color32, FontId, Stroke};

/// Satu potongan isi pesan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// Teks biasa, masih boleh mengandung markdown inline.
    Text(String),
    /// Isi pagar kode. `lang` bisa kosong.
    Code { lang: String, body: String },
}

/// Pecah pesan menjadi blok teks & kode.
///
/// Sama seperti versi Kotlin: ```` ``` ```` dipakai sebagai pemisah, indeks
/// genap teks, indeks ganjil kode. Tanpa pagar sama sekali → satu blok teks.
pub fn split_code_blocks(text: &str) -> Vec<Block> {
    if !text.contains("```") {
        return vec![Block::Text(text.to_string())];
    }

    let mut out = Vec::new();
    for (i, part) in text.split("```").enumerate() {
        if i % 2 == 0 {
            if !part.is_empty() {
                out.push(Block::Text(part.to_string()));
            }
        } else {
            let lang = part.lines().next().unwrap_or("").trim().to_string();
            // Sama seperti `removePrefix(lang).trimStart('\n')`: buang bahasa,
            // lalu baris kosong di awal, lalu baris kosong di akhir.
            let rest = part.strip_prefix(lang.as_str()).unwrap_or(part);
            let body = rest.trim_start_matches('\n').trim_end_matches('\n');
            out.push(Block::Code {
                lang,
                body: body.to_string(),
            });
        }
    }
    out
}

/// Gaya yang dipakai untuk satu paragraf inline.
pub struct InlineStyle {
    pub font: FontId,
    pub mono: FontId,
    pub color: Color32,
    pub strong_color: Color32,
    pub code_color: Color32,
    pub code_bg: Color32,
}

fn fmt(_s: &InlineStyle, font: &FontId, color: Color32) -> TextFormat {
    TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    }
}

/// Susun satu paragraf menjadi `LayoutJob` dengan **tebal**, *miring*, dan
/// `kode` sesuai penanda markdown.
pub fn inline_job(text: &str, s: &InlineStyle) -> LayoutJob {
    let mut job = LayoutJob::default();
    let mut plain = String::new();
    let mut i = 0usize;

    while i < text.len() {
        let rest = &text[i..];

        if let Some(after) = rest.strip_prefix("**") {
            if let Some(end) = after.find("**") {
                if !plain.is_empty() {
                    job.append(&plain, 0.0, fmt(s, &s.font, s.color));
                    plain.clear();
                }
                let mut f = fmt(s, &s.font, s.strong_color);
                // Penekanan kompensasi: tanpa face SemiBold, kerapatan huruf
                // adalah satu-satunya isyarat "lebih tebal" yang jujur.
                f.extra_letter_spacing = 0.25;
                job.append(&after[..end], 0.0, f);
                i += 2 + end + 2;
                continue;
            }
        }

        if let Some(after) = rest.strip_prefix('`') {
            if let Some(end) = after.find('`') {
                if !plain.is_empty() {
                    job.append(&plain, 0.0, fmt(s, &s.font, s.color));
                    plain.clear();
                }
                let mut f = fmt(s, &s.mono, s.code_color);
                f.background = s.code_bg;
                job.append(&after[..end], 0.0, f);
                i += 1 + end + 1;
                continue;
            }
        }

        if let Some(after) = rest.strip_prefix('*') {
            if let Some(end) = after.find('*') {
                if !plain.is_empty() {
                    job.append(&plain, 0.0, fmt(s, &s.font, s.color));
                    plain.clear();
                }
                let mut f = fmt(s, &s.font, s.color);
                f.italics = true;
                job.append(&after[..end], 0.0, f);
                i += 1 + end + 1;
                continue;
            }
        }

        let ch = rest.chars().next().unwrap_or(' ');
        plain.push(ch);
        i += ch.len_utf8();
    }

    if !plain.is_empty() {
        job.append(&plain, 0.0, fmt(s, &s.font, s.color));
    }
    if job.sections.is_empty() {
        job.append("", 0.0, fmt(s, &s.font, s.color));
    }
    job
}

/// Kotak kode sederhana (satu warna latar, monospace) — padanan `CodeBlock`
/// di Kotlin. Highlight per bahasa belum ada; itu opsional di checklist M4.
pub fn code_frame(
    ui: &mut eframe::egui::Ui,
    bg: Color32,
    add_contents: impl FnOnce(&mut eframe::egui::Ui),
) {
    eframe::egui::Frame::new()
        .fill(bg)
        .corner_radius(eframe::egui::CornerRadius::same(10))
        .inner_margin(eframe::egui::Margin::same(10))
        .stroke(Stroke::NONE)
        .show(ui, add_contents);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gaya() -> InlineStyle {
        InlineStyle {
            font: FontId::proportional(15.0),
            mono: FontId::monospace(14.0),
            color: Color32::WHITE,
            strong_color: Color32::from_rgb(1, 2, 3),
            code_color: Color32::from_rgb(4, 5, 6),
            code_bg: Color32::from_rgb(7, 8, 9),
        }
    }

    #[test]
    fn tanpa_pagar_satu_blok_teks() {
        let b = split_code_blocks("halo **dunia**");
        assert_eq!(b, vec![Block::Text("halo **dunia**".into())]);
    }

    #[test]
    fn blok_kode_mengambil_bahasa_dari_baris_pertama() {
        let b = split_code_blocks("sebelum\n```rust\nfn main() {}\n```\nsesudah");
        assert_eq!(b.len(), 3);
        assert_eq!(b[0], Block::Text("sebelum\n".into()));
        assert_eq!(
            b[1],
            Block::Code {
                lang: "rust".into(),
                body: "fn main() {}".into()
            }
        );
        assert_eq!(b[2], Block::Text("\nsesudah".into()));
    }

    #[test]
    fn pagar_tanpa_bahasa_tetap_blok_kode() {
        let b = split_code_blocks("```\nls -la\n```");
        assert_eq!(
            b,
            vec![Block::Code {
                lang: String::new(),
                body: "ls -la".into()
            }]
        );
    }

    #[test]
    fn inline_membuang_penanda_dan_memakai_gaya_berbeda() {
        let s = gaya();
        let job = inline_job("**tebal** lalu `kode` dan *miring*", &s);
        let teks: String = job
            .sections
            .iter()
            .map(|x| job.text[x.byte_range.clone()].to_string())
            .collect();
        assert_eq!(teks, "tebal lalu kode dan miring");
        assert_eq!(job.sections.len(), 5);

        // Bagian tebal benar-benar memakai warna penekanan.
        assert_eq!(job.sections[0].format.color, s.strong_color);
        // Bagian miring benar-benar diminta miring.
        assert!(job.sections[4].format.italics);
        // Bagian kode memakai font monospace + latar kode.
        assert_eq!(job.sections[2].format.font_id, s.mono);
        assert_eq!(job.sections[2].format.background, s.code_bg);
    }

    #[test]
    fn penanda_menggantung_tidak_panik() {
        for t in ["**belum ditutup", "`kode", "*miring", "***", "a**b", ""] {
            let job = inline_job(t, &gaya());
            assert!(!job.sections.is_empty(), "kosong untuk {t:?}");
        }
    }
}
