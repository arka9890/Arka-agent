//! Kartu persetujuan tool skill & kartu `ask_user`.
//!
//! Padanan `UiRequestDialogs.kt`. Tool di agent loop **memblok** sampai jawaban
//! datang, jadi dialog ini bukan hiasan: kalau pengguna tidak menjawab dalam
//! 5 menit, core menganggapnya menolak (`ui::ANSWER_TIMEOUT`).
//!
//! Pratinjau file di sini sengaja hanya teks + info. Gambar/video butuh
//! pemuat tekstur yang baru dipasang di M6; menampilkannya sekarang berarti
//! memuat berkas di dalam `update()` — persis yang membuat UI tersendat.

use eframe::egui::{self, RichText, Ui};

use super::state::{AppState, ApprovalCard, QuestionCard};
use super::theme::{self, fs, ClayColors};
use super::widgets;
use arka_core::ui;

/// Terapkan jawaban persetujuan ke core lalu tutup kartunya.
pub fn apply_approval(state: &mut AppState, approved: bool) {
    if let Some(card) = state.chat.approval.take() {
        ui::resolve_approval(&card.id, approved);
        state.notify(if approved {
            state.tr("Disetujui.", "Approved.", "承認しました。")
        } else {
            state.tr("Ditolak.", "Rejected.", "拒否しました。")
        });
    }
}

/// Terapkan pilihan jawaban `ask_user` lalu tutup kartunya.
pub fn apply_choice(state: &mut AppState, index: usize) {
    if let Some(card) = state.chat.question.take() {
        ui::answer_question(&card.id, index as i32);
    }
}

/// Tutup kartu tanpa menjawab. Core memperlakukan ini sebagai "tidak ada
/// jawaban" (tool dianggap ditolak / dilewati).
pub fn dismiss(state: &mut AppState) {
    if let Some(card) = state.chat.approval.take() {
        ui::resolve_approval(&card.id, false);
    }
    if let Some(card) = state.chat.question.take() {
        ui::answer_question(&card.id, -1);
    }
}

/// Gambar kartu yang sedang aktif (kalau ada).
pub fn show(ctx: &egui::Context, state: &mut AppState, colors: &ClayColors) {
    if let Some(card) = state.chat.approval.clone() {
        approval_window(ctx, state, colors, &card);
    }
    if let Some(card) = state.chat.question.clone() {
        question_window(ctx, state, colors, &card);
    }
}

fn approval_window(
    ctx: &egui::Context,
    state: &mut AppState,
    colors: &ClayColors,
    card: &ApprovalCard,
) {
    let mut answer: Option<bool> = None;
    egui::Window::new(state.tr("Izin diperlukan", "Permission needed", "許可が必要です"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(460.0 * theme::scale_of(ctx));
            ui.label(RichText::new(&card.action).size(fs(ui, 16.0)).strong());
            ui.add_space(theme::SP_XS);
            ui.label(
                RichText::new(&card.target)
                    .size(fs(ui, 13.0))
                    .monospace()
                    .color(colors.accent),
            );
            if !card.detail.trim().is_empty() {
                ui.add_space(theme::SP_SM);
                markdown_like(ui, colors, &card.detail);
            }
            ui.add_space(theme::SP_MD);
            ui.horizontal(|ui| {
                if widgets::clay_button(ui, colors, &state.tr("Setujui", "Approve", "承認"), true)
                    .clicked()
                {
                    answer = Some(true);
                }
                if widgets::clay_button(ui, colors, &state.tr("Tolak", "Reject", "拒否"), true)
                    .clicked()
                {
                    answer = Some(false);
                }
            });
        });

    if let Some(a) = answer {
        apply_approval(state, a);
    }
}

fn question_window(
    ctx: &egui::Context,
    state: &mut AppState,
    colors: &ClayColors,
    card: &QuestionCard,
) {
    let mut chosen: Option<usize> = None;
    let mut close = false;
    egui::Window::new(state.tr("Pertanyaan", "Question", "質問"))
        .collapsible(false)
        .resizable(true)
        .default_width(460.0 * theme::scale_of(ctx))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(520.0 * theme::scale_of(ctx));
            ui.label(RichText::new(&card.question).size(fs(ui, 15.0)));

            // Pratinjau file: maksimal 5, sesuai batas yang sama di core
            // (`ui::ask_question`) dan di Android.
            if !card.files.is_empty() {
                ui.add_space(theme::SP_SM);
                widgets::section(ui, colors, &state.tr("Berkas", "Files", "ファイル"));
                egui::ScrollArea::vertical()
                    .max_height(240.0 * theme::scale_of(ctx))
                    .show(ui, |ui| {
                        for f in card.files.iter().take(5) {
                            file_card(ui, colors, f);
                        }
                    });
            }

            ui.add_space(theme::SP_MD);
            for (i, opt) in card.options.iter().enumerate() {
                if widgets::clay_button(ui, colors, opt, true).clicked() {
                    chosen = Some(i);
                }
                ui.add_space(theme::SP_XS);
            }
            ui.add_space(theme::SP_XS);
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(state.tr("Lewati", "Skip", "スキップ"))
                            .size(fs(ui, 12.5))
                            .color(colors.muted),
                    )
                    .frame(false),
                )
                .clicked()
            {
                close = true;
            }
        });

    if let Some(i) = chosen {
        apply_choice(state, i);
    } else if close {
        dismiss(state);
    }
}

/// Satu berkas di kartu `ask_user`.
fn file_card(ui: &mut Ui, colors: &ClayColors, f: &serde_json::Value) {
    let name = f.get("name").and_then(|v| v.as_str()).unwrap_or("?");
    let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or("file");
    let host = f
        .get("hostPath")
        .and_then(|v| v.as_str())
        .or_else(|| f.get("path").and_then(|v| v.as_str()))
        .unwrap_or("");

    let icon = match kind {
        "image" => "\u{1F5BC}",
        "video" => "\u{1F3AC}",
        "audio" => "\u{1F3B5}",
        _ => "\u{1F4C4}",
    };

    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon).size(fs(ui, 16.0)));
            ui.label(RichText::new(name).size(fs(ui, 13.0)).strong());
            ui.label(RichText::new(kind).size(fs(ui, 11.0)).color(colors.muted));
        });
        if !host.is_empty() {
            ui.label(
                RichText::new(host)
                    .size(fs(ui, 10.5))
                    .monospace()
                    .color(colors.muted),
            );
        }
        // Isi teks pendek langsung ditampilkan supaya pengguna tahu apa yang
        // disetujui tanpa membuka aplikasi lain.
        if kind == "file" && !host.is_empty() {
            if let Ok(text) = std::fs::read_to_string(host) {
                let cuplikan: String = text.chars().take(400).collect();
                ui.add_space(theme::SP_XS);
                ui.label(
                    RichText::new(cuplikan)
                        .size(fs(ui, 11.5))
                        .monospace()
                        .color(colors.on_surface),
                );
            }
        }
    });
    ui.add_space(theme::SP_XS);
}

/// `detail` dari core berupa teks biasa (bisa berisi perintah panjang).
fn markdown_like(ui: &mut Ui, colors: &ClayColors, text: &str) {
    markdown::code_frame(ui, colors.background, |ui| {
        ui.set_max_width(ui.available_width());
        ui.label(
            RichText::new(text)
                .size(fs(ui, 12.0))
                .monospace()
                .color(colors.on_surface),
        );
    });
}

use super::markdown;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::i18n::Lang;
    use crate::gui::state::{ChatState, Persisted};
    use arka_core::paths::AppPaths;

    fn state() -> AppState {
        let base = std::env::temp_dir().join("arka-dialog-test");
        AppState::new(
            AppPaths::windows(base.join("home"), base.join("data")),
            Persisted::default(),
        )
    }

    #[test]
    fn menyetujui_menutup_kartu_dan_memberi_notice() {
        let mut s = state();
        s.chat = ChatState::default();
        s.chat.approval = Some(ApprovalCard {
            id: "apv-uji".into(),
            action: "Jalankan perintah".into(),
            target: "ls".into(),
            detail: String::new(),
        });
        apply_approval(&mut s, true);
        assert!(s.chat.approval.is_none());
        assert!(s.notice.is_some());
    }

    #[test]
    fn memilih_opsi_menutup_kartu() {
        let mut s = state();
        s.chat.question = Some(QuestionCard {
            id: "qst-uji".into(),
            question: "Pilih".into(),
            options: vec!["A".into(), "B".into()],
            files: Vec::new(),
        });
        apply_choice(&mut s, 1);
        assert!(s.chat.question.is_none());
    }

    #[test]
    fn dismiss_menutup_kedua_jenis_kartu() {
        let mut s = state();
        s.chat.approval = Some(ApprovalCard {
            id: "apv-x".into(),
            action: "a".into(),
            target: "b".into(),
            detail: String::new(),
        });
        s.chat.question = Some(QuestionCard {
            id: "qst-x".into(),
            question: "q".into(),
            options: vec![],
            files: vec![],
        });
        dismiss(&mut s);
        assert!(s.chat.approval.is_none());
        assert!(s.chat.question.is_none());
    }

    #[test]
    fn tidak_ada_kartu_berarti_tidak_ada_efek() {
        let mut s = state();
        apply_approval(&mut s, true);
        apply_choice(&mut s, 0);
        dismiss(&mut s);
        assert_eq!(s.lang, Lang::Id);
    }
}
