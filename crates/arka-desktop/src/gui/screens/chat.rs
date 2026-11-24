//! Tab Chat. Kerangka M3: tata letak + empty state. Streaming & bubble
//! sungguhan menyusul di M4 (`bubble.rs` + `agent.rs`).

use eframe::egui::{self, RichText, Ui};

use crate::gui::state::AppState;
use crate::gui::theme::{self, fs, ClayColors};
use crate::gui::widgets;

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    // Header layar.
    ui.horizontal(|ui| {
        ui.label(RichText::new(state.tr("Chat", "Chat", "チャット")).size(fs(ui, 22.0)).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("+ Sesi baru", "+ New session", "+ 新しいセッション"),
                colors.accent,
            )
            .clicked()
            {
                state.notify(state.tr("Sesi baru menyusul (M4)", "New session comes in M4", "新しいセッションは M4 で"));
            }
        });
    });

    ui.add_space(theme::SP_MD);

    // Area pesan: masih kosong → tampilkan empty state.
    let footer_h = 96.0;
    let avail = (ui.available_height() - footer_h).max(120.0);
    egui::ScrollArea::vertical()
        .max_height(avail)
        .auto_shrink([false, false])
        .show(ui, |ui| empty_state(ui, state, colors));

    // Input bar (belum mengirim apa pun — M4).
    ui.add_space(theme::SP_SM);
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        ui.horizontal(|ui| {
            let button_w = 96.0;
            let w = (ui.available_width() - button_w - theme::SP_MD).max(120.0);
            let hint = state.tr("Tulis pesan…", "Type a message…", "メッセージを入力…");
            ui.add_sized(
                [w, 40.0],
                egui::TextEdit::singleline(&mut state.draft)
                    .hint_text(hint)
                    .desired_width(w),
            );
            if widgets::clay_button(ui, colors, &state.tr("Kirim", "Send", "送信"), true).clicked() {
                state.notify(state.tr(
                    "Pengiriman pesan aktif di M4.",
                    "Sending messages lands in M4.",
                    "メッセージ送信は M4 で利用可能になります。",
                ));
            }
        });
    });
}

fn empty_state(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    ui.add_space(theme::SP_XL);
    widgets::clay_card(ui, colors, None, |ui| {
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("\u{1F44B}").size(fs(ui, 40.0)));
            ui.add_space(theme::SP_SM);
            ui.label(
                RichText::new(state.tr("Halo, aku Arka", "Hi, I'm Arka", "こんにちは、Arka です"))
                    .size(fs(ui, 20.0))
                    .strong(),
            );
            ui.add_space(theme::SP_XS);
            ui.label(
                RichText::new(state.tr(
                    "Aku bisa menjalankan perintah PowerShell di PC ini.",
                    "I can run PowerShell commands on this PC.",
                    "この PC で PowerShell コマンドを実行できます。",
                ))
                .size(fs(ui, 14.0))
                .color(colors.muted),
            );
            ui.add_space(theme::SP_LG);
        });

        // Contoh perintah — membantu pengguna tahu apa yang bisa dilakukan.
        let samples = [
            state.tr("Cek versi Python", "Check Python version", "Python のバージョンを確認"),
            state.tr("Daftar isi folder kerja", "List workspace files", "ワークスペースの一覧"),
            state.tr("Lihat penggunaan disk", "Show disk usage", "ディスク使用量を表示"),
        ];
        ui.horizontal_wrapped(|ui| {
            for s in samples {
                if widgets::clay_pill_button(ui, colors, &s, colors.accent).clicked() {
                    state.notify(state.tr(
                        "Contoh perintah aktif setelah M4.",
                        "Sample commands work after M4.",
                        "サンプルコマンドは M4 以降で動作します。",
                    ));
                }
            }
        });
    });
}
