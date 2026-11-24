//! Tab Terminal. Di M3 ini menampilkan **status shell sungguhan** hasil
//! `pwsh::detect()`, disimpan di `AppState` — bukan dipanggil ulang tiap
//! frame, karena deteksi menjalankan proses PowerShell (mahal). Kotak perintah
//! interaktif menyusul di M7; mesinnya sendiri sudah selesai di M2.

use eframe::egui::{self, RichText, Ui};

use crate::gui::state::{AppState, ShellProbe};
use crate::gui::theme::{self, fs, ClayColors};
use crate::gui::widgets;

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    // Deteksi sekali; tombol "Periksa ulang" memaksa ulang (mis. setelah
    // pengguna memasang PowerShell 7 tanpa menutup aplikasi).
    state.ensure_shell_probe();

    ui.horizontal(|ui| {
        ui.label(RichText::new(state.tr("Terminal", "Terminal", "ターミナル")).size(fs(ui, 22.0)).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Periksa ulang", "Re-check", "再確認"),
                colors.accent,
            )
            .clicked()
            {
                state.refresh_shell_probe();
            }
        });
    });

    ui.add_space(theme::SP_MD);

    widgets::section(ui, colors, &state.tr("Shell", "Shell", "シェル"));
    widgets::clay_card(ui, colors, None, |ui| match &state.shell {
        ShellProbe::Ready(d) => {
            ui.horizontal(|ui| {
                widgets::clay_pill(ui, colors, &state.tr("Siap", "Ready", "準備完了"), colors.success);
                ui.label(RichText::new(d.describe()).size(fs(ui, 15.0)).strong());
            });
            ui.add_space(theme::SP_SM);
            widgets::kv(ui, colors, &state.tr("Path", "Path", "パス"), &d.program.display().to_string());
            widgets::kv(
                ui,
                colors,
                &state.tr("Keluaran", "Encoding", "出力"),
                "UTF-8 \u{00B7} -EncodedCommand",
            );
        }
        ShellProbe::Missing(e) => {
            ui.horizontal(|ui| {
                widgets::clay_pill(
                    ui,
                    colors,
                    &state.tr("Tidak ditemukan", "Not found", "見つかりません"),
                    colors.danger,
                );
                ui.label(RichText::new(e).size(fs(ui, 13.0)).color(colors.danger));
            });
        }
        ShellProbe::Unknown => {
            ui.label(
                RichText::new(state.tr("Memeriksa…", "Checking…", "確認中…"))
                    .size(fs(ui, 13.0))
                    .color(colors.muted),
            );
        }
    });

    ui.add_space(theme::SP_MD);
    widgets::section(ui, colors, &state.tr("Folder kerja", "Workspace", "ワークスペース"));
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        ui.label(
            RichText::new(state.workspace().display().to_string())
                .size(fs(ui, 13.0))
                .monospace()
                .color(colors.on_surface),
        );
        ui.add_space(theme::SP_XS);
        ui.label(
            RichText::new(state.tr(
                "Setiap perintah dijalankan di folder ini.",
                "Every command runs in this folder.",
                "すべてのコマンドはこのフォルダーで実行されます。",
            ))
            .size(fs(ui, 12.0))
            .color(colors.muted),
        );
    });

    ui.add_space(theme::SP_MD);
    ui.horizontal(|ui| {
        widgets::section(ui, colors, &state.tr("Log perintah", "Command log", "コマンドログ"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Bersihkan", "Clear", "クリア"),
                colors.accent,
            )
            .clicked()
            {
                state.chat.terminal_log.clear();
            }
        });
    });
    widgets::clay_card(ui, colors, Some(colors.background), |ui| {
        if state.chat.terminal_log.trim().is_empty() {
            ui.label(
                RichText::new(state.tr(
                    "Perintah yang dijalankan agent akan muncul di sini.",
                    "Commands the agent runs will show up here.",
                    "エージェントが実行したコマンドがここに表示されます。",
                ))
                .size(fs(ui, 12.5))
                .color(colors.muted),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .max_height(240.0 * theme::scale_of(ui.ctx()))
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(&state.chat.terminal_log)
                        .size(fs(ui, 12.0))
                        .monospace()
                        .color(colors.on_surface),
                );
            });
    });

    ui.add_space(theme::SP_MD);
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        ui.label(
            RichText::new(state.tr(
                "Kotak perintah interaktif aktif di M7. Mesin shell (PowerShell, Job Object, batas waktu) sudah selesai di M2 dan diuji di Windows.",
                "Interactive command box lands in M7. The shell engine (PowerShell, Job Objects, timeouts) is already done and tested on Windows in M2.",
                "対話的なコマンド入力は M7 で利用可能になります。シェル本体 (PowerShell、Job Object、タイムアウト) は M2 で完了済みです。",
            ))
            .size(fs(ui, 12.0))
            .color(colors.muted),
        );
    });
}
