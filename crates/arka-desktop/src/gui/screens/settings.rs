//! Tab Setelan. Di M3 ini sudah **berfungsi penuh** untuk tiga hal yang
//! diminta kriteria selesai: tema, ukuran font, dan bahasa. Sisanya (timeout,
//! autosave, notifikasi, tray) menyusul di M8.

use eframe::egui::{self, RichText, Ui};

use crate::gui::i18n::Lang;
use crate::gui::state::{AppState, Modal};
use crate::gui::theme::{self, fs, ClayColors, FontScale, ThemeMode};
use crate::gui::widgets;

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    ui.label(RichText::new(state.tr("Setelan", "Settings", "設定")).size(fs(22.0)).strong());
    ui.add_space(theme::SP_MD);

    widgets::section(ui, colors, &state.tr("Tampilan", "Appearance", "外観"));
    widgets::clay_card(ui, colors, None, |ui| {
        ui.label(RichText::new(state.tr("Tema", "Theme", "テーマ")).size(fs(14.0)).strong());
        ui.add_space(theme::SP_XS);
        ui.horizontal(|ui| {
            for mode in ThemeMode::ALL {
                let label = match mode {
                    ThemeMode::System => state.tr("Ikuti sistem", "Follow system", "システムに従う"),
                    ThemeMode::Dark => state.tr("Gelap", "Dark", "ダーク"),
                    ThemeMode::Light => state.tr("Terang", "Light", "ライト"),
                };
                let active = state.theme == mode;
                let btn = egui::Button::new(RichText::new(label).size(fs(13.0)).color(if active {
                    colors.accent
                } else {
                    colors.on_surface
                }))
                .fill(if active { colors.accent_soft } else { colors.surface_raised });
                if ui.add(btn).clicked() {
                    state.theme = mode;
                }
            }
        });

        ui.add_space(theme::SP_MD);
        ui.label(RichText::new(state.tr("Ukuran teks", "Text size", "文字サイズ")).size(fs(14.0)).strong());
        ui.add_space(theme::SP_XS);
        ui.horizontal(|ui| {
            for scale in FontScale::ALL {
                let label = match scale {
                    FontScale::Small => state.tr("Kecil", "Small", "小"),
                    FontScale::Medium => state.tr("Sedang", "Medium", "中"),
                    FontScale::Large => state.tr("Besar", "Large", "大"),
                };
                let active = state.font_scale == scale;
                let btn = egui::Button::new(RichText::new(label).size(fs(13.0)).color(if active {
                    colors.accent
                } else {
                    colors.on_surface
                }))
                .fill(if active { colors.accent_soft } else { colors.surface_raised });
                if ui.add(btn).clicked() {
                    state.font_scale = scale;
                }
            }
            ui.label(
                RichText::new(format!("{}x", state.font_scale.factor()))
                    .size(fs(12.0))
                    .color(colors.muted),
            );
        });

        ui.add_space(theme::SP_MD);
        ui.label(RichText::new(state.tr("Bahasa", "Language", "言語")).size(fs(14.0)).strong());
        ui.add_space(theme::SP_XS);
        ui.horizontal(|ui| {
            for lang in Lang::ALL {
                let active = state.lang == lang;
                let btn = egui::Button::new(RichText::new(lang.native()).size(fs(13.0)).color(if active {
                    colors.accent
                } else {
                    colors.on_surface
                }))
                .fill(if active { colors.accent_soft } else { colors.surface_raised });
                if ui.add(btn).clicked() {
                    state.lang = lang;
                }
            }
        });
    });

    ui.add_space(theme::SP_MD);
    widgets::section(ui, colors, &state.tr("Lingkungan", "Environment", "環境"));
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        widgets::kv(ui, colors, &state.tr("Folder kerja", "Workspace", "ワークスペース"), &state.workspace().display().to_string());
        widgets::kv(ui, colors, &state.tr("Folder skill", "Skills folder", "スキルフォルダー"), &state.paths.skills_dir.display().to_string());
        widgets::kv(ui, colors, &state.tr("Platform", "Platform", "プラットフォーム"), &format!("{:?}", state.paths.platform));
    });

    ui.add_space(theme::SP_MD);
    widgets::section(ui, colors, &state.tr("Tentang", "About", "情報"));
    widgets::clay_card(ui, colors, None, |ui| {
        let resp = ui.add_sized(
            [180.0, 34.0],
            egui::Button::new(RichText::new(state.tr("Tentang Arka Agent", "About Arka Agent", "Arka Agent について")).size(fs(14.0)))
                .fill(colors.surface_raised),
        );
        if resp.clicked() {
            state.modal = Modal::About;
        }
        ui.add_space(theme::SP_SM);
        ui.label(
            RichText::new(format!(
                "{} \u{00B7} 100% Rust \u{00B7} egui",
                env!("CARGO_PKG_VERSION")
            ))
            .size(fs(12.0))
            .color(colors.muted),
        );
    });

    ui.add_space(theme::SP_XL);
}
