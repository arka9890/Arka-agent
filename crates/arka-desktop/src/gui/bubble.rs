//! Bubble pesan chat: padanan `MessageBubble.kt`.
//!
//! Perbedaan warna: pesan pengguna memakai `user_bubble` (accent) dengan teks
//! putih, jawaban AI memakai `ai_bubble` dengan teks `on_surface`. Keduanya
//! berbagi sudut & bayangan clay yang sama supaya tidak terlihat seperti dua
//! aplikasi berbeda.

use eframe::egui::{self, Color32, RichText, Ui};

use super::markdown::{self, InlineStyle};
use super::state::MsgView;
use super::theme::{self, fs, ClayColors};
use super::widgets;

/// Batas lebar bubble terhadap lebar layar, seperti `fillMaxWidth(0.86f)` di
/// Android.
const BUBBLE_RATIO: f32 = 0.78;

/// Jam `HH:MM` lokal dari epoch milidetik.
///
/// Tidak memakai crate tanggal: workspace ini dikerjakan tanpa jaringan, dan
/// `chrono` akan menggagalkan `cargo build --offline`. Yang dibutuhkan cuma
/// jam & menit, jadi cukup geser UTC dengan selisih zona waktu sistem —
/// tanpa algoritma kalender.
///
/// Selisih diambil **sekali** lalu di-cache. Kalau zona waktu berubah di
/// tengah sesi (perjalanan/DST), jam yang sudah tampil bisa bergeser satu jam
/// sampai aplikasi dijalankan ulang. Itu disengaja: membaca zona waktu tiap
/// pesan berarti memanggil OS ratusan kali per frame.
pub fn time_label(ms: i64) -> String {
    label_with_offset(ms, local_offset_secs())
}

/// Bagian murni (bisa diuji tanpa bergantung zona waktu mesin CI).
pub fn label_with_offset(ms: i64, offset_secs: i64) -> String {
    let local = ms + offset_secs * 1000;
    // `rem_euclid`: geseran zona waktu bisa membuat nilai negatif untuk
    // stempel waktu di sekitar epoch. Yang perlu benar hanya jam-menitnya,
    // jadi pembungkusan ke hari sebelumnya justru jawaban yang tepat.
    // `div_euclid` **dan** `rem_euclid`: pembagian biasa memotong ke arah nol,
    // jadi -4,5 jam menjadi -4 (bukan -5) dan jamnya meleset satu.
    let h = local.div_euclid(3_600_000).rem_euclid(24);
    let m = local.div_euclid(60_000).rem_euclid(60);
    format!("{h:02}:{m:02}")
}

/// Selisih waktu lokal terhadap UTC, dalam detik.
fn local_offset_secs() -> i64 {
    use std::sync::OnceLock;
    static OFF: OnceLock<i64> = OnceLock::new();
    *OFF.get_or_init(offset_from_os)
}

/// Detik sejak epoch dari tanggal sipil (algoritma Hinnant).
///
/// Ditulis terpisah dari FFI-nya supaya **bisa diuji di Linux**: bagian yang
/// tidak bisa kita jalankan di sini hanyalah dua pemanggilan Windows, bukan
/// aritmetika kalendernya.
#[cfg(any(windows, test))]
fn civil_secs(y: i64, m: i64, d: i64, h: i64, mi: i64, sec: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // 0..=399
    let mp = (m + 9) % 12; // Maret = 0
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    days * 86_400 + h * 3600 + mi * 60 + sec
}

#[cfg(windows)]
fn offset_from_os() -> i64 {
    // `GetLocalTime`/`GetSystemTime` tinggal di `SystemInformation`, bukan
    // `System::Time` — `System::Time` hanya berisi tipe waktu lama.
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};

    fn secs(t: &SYSTEMTIME) -> i64 {
        civil_secs(
            t.wYear as i64,
            t.wMonth as i64,
            t.wDay as i64,
            t.wHour as i64,
            t.wMinute as i64,
            t.wSecond as i64,
        )
    }

    // SAFETY: kedua fungsi hanya mengisi struct yang kita sediakan.
    unsafe {
        let mut local: SYSTEMTIME = std::mem::zeroed();
        let mut utc: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut local);
        GetSystemTime(&mut utc);
        secs(&local) - secs(&utc)
    }
}

/// Di luar Windows (pengembangan UI & CI Linux) tidak ada zona waktu yang
/// dipakai pengguna akhir — tampilkan UTC apa adanya.
#[cfg(not(windows))]
fn offset_from_os() -> i64 {
    0
}

fn inline_style(ui: &Ui, colors: &ClayColors) -> InlineStyle {
    InlineStyle {
        font: egui::FontId::new(fs(ui, 15.0), egui::FontFamily::Proportional),
        mono: egui::FontId::new(fs(ui, 13.5), egui::FontFamily::Monospace),
        color: colors.on_surface,
        strong_color: if ui.visuals().dark_mode {
            Color32::WHITE
        } else {
            Color32::BLACK
        },
        code_color: colors.accent,
        code_bg: colors.accent_soft.gamma_multiply(0.7),
    }
}

/// Isi jawaban AI: blok kode dirender monospace, teks lewat markdown inline.
fn markdown_body(ui: &mut Ui, colors: &ClayColors, text: &str) {
    let style = inline_style(ui, colors);
    for block in markdown::split_code_blocks(text) {
        match block {
            markdown::Block::Text(t) => {
                if t.trim().is_empty() {
                    continue;
                }
                let job = markdown::inline_job(&t, &style);
                let mut job = job;
                job.wrap.max_width = ui.available_width();
                ui.label(job);
            }
            markdown::Block::Code { lang, body } => {
                let bg = if ui.visuals().dark_mode {
                    Color32::from_black_alpha(90)
                } else {
                    colors.background
                };
                if !lang.is_empty() {
                    ui.label(
                        RichText::new(&lang)
                            .size(fs(ui, 11.0))
                            .color(colors.muted)
                            .monospace(),
                    );
                }
                markdown::code_frame(ui, bg, |ui| {
                    ui.set_max_width(ui.available_width());
                    ui.label(
                        RichText::new(&body)
                            .size(fs(ui, 13.0))
                            .monospace()
                            .color(if ui.visuals().dark_mode {
                                Color32::from_rgb(0xE6, 0xE6, 0xF0)
                            } else {
                                colors.on_surface
                            }),
                    );
                });
                ui.add_space(theme::SP_XS);
            }
        }
    }
}

fn footer(ui: &mut Ui, colors: &ClayColors, m: &MsgView, on_accent: bool) {
    ui.add_space(theme::SP_XS);
    let dim = if on_accent {
        Color32::WHITE.gamma_multiply(0.75)
    } else {
        colors.muted
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(time_label(m.created_at)).size(fs(ui, 10.5)).color(dim));
        for a in &m.attachments {
            ui.label(
                RichText::new(format!("\u{1F4C4} {a}"))
                    .size(fs(ui, 10.5))
                    .color(dim),
            );
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let resp = ui.add(
                egui::Button::new(RichText::new("\u{29C9}").size(fs(ui, 11.0)).color(dim))
                    .frame(false),
            );
            if resp.on_hover_text("Salin").clicked() {
                ui.ctx().copy_text(m.content.clone());
            }
        });
    });
}

/// Satu pesan lengkap.
pub fn show(ui: &mut Ui, colors: &ClayColors, m: &MsgView) {
    let max_w = (ui.available_width() * BUBBLE_RATIO).max(160.0);

    if m.is_user() {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
            ui.set_max_width(max_w);
            widgets::clay_panel(ui, colors, colors.user_bubble, 18, 12, |ui| {
                ui.set_max_width((max_w - 24.0).max(80.0));
                ui.add(
                    egui::Label::new(
                        RichText::new(&m.content)
                            .size(fs(ui, 15.0))
                            .color(Color32::WHITE),
                    )
                    .wrap(),
                );
                footer(ui, colors, m, true);
            });
        });
    } else {
        widgets::clay_panel(ui, colors, colors.ai_bubble, 18, 14, |ui| {
            ui.set_max_width((ui.available_width() - 4.0).max(120.0));
            markdown_body(ui, colors, &m.content);
            footer(ui, colors, m, false);
        });
    }

    ui.add_space(theme::SP_SM);
}

/// Jawaban yang sedang mengalir.
pub fn streaming(ui: &mut Ui, colors: &ClayColors, text: &str) {
    widgets::clay_panel(ui, colors, colors.ai_bubble, 18, 14, |ui| {
        ui.set_max_width((ui.available_width() - 4.0).max(120.0));
        if text.is_empty() {
            typing(ui, colors);
        } else {
            markdown_body(ui, colors, text);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("\u{258C}")
                        .size(fs(ui, 15.0))
                        .color(colors.accent),
                );
            });
        }
    });
    ui.add_space(theme::SP_SM);
}

/// Tiga titik berdenyut sebelum token pertama tiba.
///
/// Animasi memakai waktu `Context` sehingga tidak butuh timer sendiri; pemanggil
/// cukup meminta repaint selagi menunggu (`request_repaint_after`).
pub fn typing(ui: &mut Ui, colors: &ClayColors) {
    let t = ui.ctx().input(|i| i.time);
    let phase = (t * 2.4) as i32;
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("AI sedang berpikir")
                .size(fs(ui, 12.5))
                .color(colors.muted),
        );
        for i in 0..3 {
            let on = phase % 3 == i;
            let r = if on { 3.4 } else { 2.2 } * theme::scale_of(ui.ctx());
            let (rect, _) =
                ui.allocate_exact_size(egui::Vec2::splat(8.0 * theme::scale_of(ui.ctx())), egui::Sense::hover());
            let center = rect.center();
            ui.painter().circle_filled(
                center,
                r,
                if on { colors.accent } else { colors.muted.gamma_multiply(0.5) },
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diuji lewat `label_with_offset`, bukan `time_label`: hasilnya jadi
    /// deterministik walau runner CI berada di zona waktu berbeda.
    #[test]
    fn jam_diformat_jam_menit() {
        // 1970-01-01 00:00 UTC + 7 jam (WIB).
        assert_eq!(label_with_offset(0, 7 * 3600), "07:00");
        assert_eq!(label_with_offset(3_600_000, 0), "01:00");
        assert_eq!(label_with_offset(86_400_000 - 60_000, 0), "23:59");
    }

    #[test]
    fn offset_negatif_tetap_benar() {
        // UTC-5: 00:30 UTC = 19:30 hari sebelumnya.
        assert_eq!(label_with_offset(1_800_000, -5 * 3600), "19:30");
    }

    #[test]
    fn waktu_sebelum_epoch_membungkus_ke_hari_sebelumnya() {
        // 1 ms sebelum epoch = 1969-12-31 23:59:59.999 UTC.
        assert_eq!(label_with_offset(-1, 0), "23:59");
    }

    /// Aritmetika kalender diuji dengan tanggal yang nilainya sudah diketahui
    /// (dicek lewat `date -u`), jadi bukan "sepertinya benar".
    #[test]
    fn aritmetika_kalender_sama_dengan_epoch_unix() {
        assert_eq!(civil_secs(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(civil_secs(2000, 1, 1, 0, 0, 0), 946_684_800);
        assert_eq!(civil_secs(2024, 3, 1, 0, 0, 0), 1_709_251_200);
        assert_eq!(civil_secs(1969, 12, 31, 23, 59, 59), -1);
        // Tahun kabisat: 2024-02-29 ada, 2023-03-01 tetap benar.
        assert_eq!(
            civil_secs(2024, 3, 1, 0, 0, 0) - civil_secs(2024, 2, 29, 0, 0, 0),
            86_400
        );
    }

    #[test]
    fn time_label_memakai_offset_sistem_tanpa_panik() {
        let s = time_label(1_700_000_000_000);
        assert_eq!(s.len(), 5, "harus HH:MM, dapat {s:?}");
    }
}
