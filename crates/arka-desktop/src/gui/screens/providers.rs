//! Tab Provider. Kerangka M3: daftar kosong + penjelasan. Penyimpanan API key
//! (DPAPI) dan editor provider menyusul di M5.

use eframe::egui::{self, RichText, Ui};

use crate::gui::state::AppState;
use crate::gui::theme::{self, fs, ClayColors};
use crate::gui::widgets;

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(state.tr("Provider", "Providers", "プロバイダー")).size(fs(ui, 22.0)).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("+ Tambah", "+ Add", "+ 追加"),
                colors.accent,
            )
            .clicked()
            {
                state.notify(state.tr(
                    "Editor provider aktif di M5.",
                    "Provider editor lands in M5.",
                    "プロバイダー編集は M5 で利用可能になります。",
                ));
            }
        });
    });

    ui.add_space(theme::SP_MD);

    widgets::clay_card(ui, colors, None, |ui| {
        ui.label(
            RichText::new(state.tr(
                "Belum ada provider",
                "No providers yet",
                "プロバイダーがありません",
            ))
            .size(fs(ui, 16.0))
            .strong(),
        );
        ui.add_space(theme::SP_XS);
        ui.label(
            RichText::new(state.tr(
                "Tambahkan endpoint OpenAI-compatible atau Anthropic. Kunci API disimpan terenkripsi di Windows (DPAPI), bukan teks biasa.",
                "Add an OpenAI-compatible or Anthropic endpoint. API keys are stored encrypted by Windows (DPAPI), never plaintext.",
                "OpenAI 互換または Anthropic のエンドポイントを追加します。API キーは Windows (DPAPI) で暗号化して保存されます。",
            ))
            .size(fs(ui, 13.0))
            .color(colors.muted),
        );
    });

    ui.add_space(theme::SP_MD);
    widgets::section(ui, colors, &state.tr("Nanti di M5", "Planned for M5", "M5 で予定"));
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        for line in [
            state.tr("Tambah / ubah / hapus provider", "Add / edit / delete provider", "プロバイダーの追加・編集・削除"),
            state.tr("Tes koneksi + daftar model", "Connection test + model list", "接続テストとモデル一覧"),
            state.tr("Simpan kunci lewat DPAPI", "Store keys via DPAPI", "DPAPI でキーを保存"),
        ] {
            ui.label(RichText::new(format!("\u{2022} {line}")).size(fs(ui, 13.0)).color(colors.muted));
        }
    });
}
