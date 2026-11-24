//! Tab File. Di M3 ini sudah berfungsi: membaca folder kerja **sungguhan**
//! dari disk, lewat cache (`fs_model::FilesCache`) supaya tidak `read_dir`
//! 60×/detik. Pratinjau (teks/gambar/video/PDF) menyusul di M6.

use eframe::egui::{self, RichText, Ui};

use crate::gui::fs_model::human_size;
use crate::gui::state::AppState;
use crate::gui::theme::{self, fs, scale_of, ClayColors};
use crate::gui::widgets;

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    let ws = state.workspace();

    // Baca hanya bila folder berubah atau belum pernah dibaca.
    state.files.ensure(&ws);

    ui.horizontal(|ui| {
        ui.label(RichText::new(state.tr("File", "Files", "ファイル")).size(fs(ui, 22.0)).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Muat ulang", "Refresh", "更新"),
                colors.accent,
            )
            .clicked()
            {
                state.files.refresh(&ws);
            }
        });
    });

    ui.add_space(theme::SP_XS);
    ui.label(
        RichText::new(ws.display().to_string())
            .size(fs(ui, 12.0))
            .color(colors.muted)
            .monospace(),
    );

    ui.add_space(theme::SP_MD);

    // Folder kerja mungkin belum ada sebelum perintah pertama dijalankan
    // (`Shell::prepare`). Itu kondisi normal, bukan error yang menakutkan.
    if !ws.exists() {
        widgets::clay_card(ui, colors, None, |ui| {
            ui.label(
                RichText::new(state.tr(
                    "Folder kerja belum dibuat",
                    "Workspace not created yet",
                    "ワークスペースが未作成です",
                ))
                .size(fs(ui, 16.0))
                .strong(),
            );
            ui.add_space(theme::SP_XS);
            ui.label(
                RichText::new(state.tr(
                    "Folder ini dibuat saat kamu menjalankan perintah pertama.",
                    "This folder is created when you run your first command.",
                    "最初のコマンド実行時に作成されます。",
                ))
                .size(fs(ui, 13.0))
                .color(colors.muted),
            );
        });
        return;
    }

    if let Some(e) = state.files.error.clone() {
        widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
            ui.label(RichText::new(e).size(fs(ui, 13.0)).color(colors.danger));
        });
        return;
    }

    if state.files.entries.is_empty() {
        widgets::clay_card(ui, colors, None, |ui| {
            ui.label(
                RichText::new(state.tr("Folder kosong", "Folder is empty", "フォルダーは空です"))
                    .size(fs(ui, 14.0))
                    .color(colors.muted),
            );
        });
        return;
    }

    // Salinan supaya `state` tidak dipinjam dua kali di dalam closure.
    let entries = state.files.entries.clone();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for e in entries {
                let glyph = if e.is_dir { "\u{1F4C1}" } else { "\u{1F4C4}" };
                let meta = if e.is_dir {
                    String::new()
                } else {
                    human_size(e.size)
                };
                let margin = (10.0 * scale_of(ui.ctx())) as i8;

                widgets::clay_panel(ui, colors, colors.surface, 16, margin, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(glyph).size(fs(ui, 16.0)));
                        ui.label(RichText::new(&e.name).size(fs(ui, 14.0)).color(colors.on_surface));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !meta.is_empty() {
                                ui.label(RichText::new(meta).size(fs(ui, 12.0)).color(colors.muted).monospace());
                            }
                        });
                    });
                });
                ui.add_space(theme::SP_XS);
            }
        });
}
