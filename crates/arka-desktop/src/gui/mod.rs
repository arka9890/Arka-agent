//! Kerangka UI desktop (egui/eframe). Murni Rust — tanpa WebView, tanpa
//! runtime JavaScript, satu binary.
//!
//! Susunan layar mengikuti `AppScreen.kt`: drawer sesi di kiri (padanan
//! navigasi Android), 5 tab, dan snackbar di bawah.
//!
//! `draw` sengaja **tidak** memakai `eframe::Frame` supaya bisa dijalankan di
//! dalam test tanpa GPU/jendela (`egui::Context` menghitung tata letak di CPU).
//! Itulah cara "5 tab bisa diklik" dibuktikan di CI, bukan dengan mata.

pub mod fs_model;
pub mod i18n;
pub mod screens;
pub mod state;
pub mod theme;
pub mod widgets;

#[cfg(test)]
mod tests;

use eframe::egui::{self, RichText};
use state::{AppState, Modal, Persisted};

use arka_core::paths::AppPaths;

const SIDEBAR_W: f32 = 248.0;
const NOTICE_SECS: f64 = 4.0;

pub struct ArkaGui {
    state: AppState,
    /// Teks notice yang terakhir ditampilkan, untuk mendeteksi notice baru
    /// (padanan `LaunchedEffect(notice)` di Compose).
    shown_notice: Option<String>,
    notice_until: f64,
    /// (skala, mode tema, gelap?) yang terakhir diterapkan; `theme::apply`
    /// hanya dipanggil saat salah satunya berubah — memasang ulang visuals &
    /// ukuran teks tiap frame itu sia-sia.
    applied: Option<(u32, theme::ThemeMode, bool)>,
}

impl ArkaGui {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: AppPaths) -> Self {
        theme::install_fonts(&cc.egui_ctx);

        let persisted = cc
            .storage
            .and_then(|s| eframe::get_value::<Persisted>(s, eframe::APP_KEY))
            .unwrap_or_default();

        Self::from_state(AppState::new(paths, persisted))
    }

    /// Bangun tanpa `eframe` — dipakai test.
    pub fn from_state(state: AppState) -> Self {
        Self {
            state,
            shown_notice: None,
            notice_until: 0.0,
            applied: None,
        }
    }

    // Dipakai test UI; M4 akan memakainya dari `main.rs` saat sesi disambungkan.
    #[cfg(test)]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    #[cfg(test)]
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }
}

impl eframe::App for ArkaGui {
    /// Ukuran/posisi jendela disimpan `eframe` sendiri; kita simpan setelan UI.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.state.persist());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.draw(ctx);
    }
}

impl ArkaGui {
    /// Satu frame penuh. Semua gambar ada di sini.
    pub fn draw(&mut self, ctx: &egui::Context) {
        let system_dark = ctx.input(|i| i.raw.system_theme) == Some(egui::Theme::Dark);
        let dark = self.state.theme.is_dark(system_dark);
        let scale = self.state.font_scale.factor();

        // Terapkan tema saat ukuran teks berubah, saat tema berganti, atau
        // saat Windows beralih mode terang/gelap ("Ikuti sistem").
        let key = (scale.to_bits(), self.state.theme, dark);
        if self.applied != Some(key) {
            theme::apply(ctx, scale, self.state.theme);
            self.applied = Some(key);
        }
        let colors = *self.state.theme.colors(system_dark);

        // Notice baru → mulai hitung waktu tampil.
        if self.state.notice != self.shown_notice {
            self.shown_notice = self.state.notice.clone();
            self.notice_until = ctx.input(|i| i.time) + NOTICE_SECS;
        }

        self.sidebar(ctx, &colors);
        self.top_bar(ctx, &colors);

        if self.state.notice.is_some() {
            self.notice(ctx, &colors);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(colors.background))
            .show(ctx, |ui| {
                widgets::paint_background(ui, &colors);
                ui.add_space(theme::SP_SM);
                match self.state.tab {
                    state::Tab::Chat => screens::chat::show(ui, &mut self.state, &colors),
                    state::Tab::Provider => screens::providers::show(ui, &mut self.state, &colors),
                    state::Tab::File => screens::files::show(ui, &mut self.state, &colors),
                    state::Tab::Terminal => screens::terminal::show(ui, &mut self.state, &colors),
                    state::Tab::Settings => screens::settings::show(ui, &mut self.state, &colors),
                }
            });

        self.modal(ctx, &colors);
    }

    fn sidebar(&mut self, ctx: &egui::Context, colors: &theme::ClayColors) {
        if !self.state.sidebar_open {
            return;
        }

        let mut open = true;
        egui::SidePanel::left("sidebar")
            .exact_width(SIDEBAR_W)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(colors.surface)
                    .inner_margin(egui::Margin::same(theme::SP_MD as i8)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Arka Agent").size(theme::fs(ui, 18.0)).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("\u{2039}").clicked() {
                            open = false;
                        }
                    });
                });

                ui.add_space(theme::SP_MD);
                let label = self.state.tr("Sesi", "Sessions", "セッション");
                ui.label(RichText::new(label).size(theme::fs(ui, 11.0)).color(colors.muted).strong());

                ui.add_space(theme::SP_XS);
                if ui
                    .add_sized(
                        [ui.available_width(), 34.0 * theme::scale_of(ui.ctx())],
                        egui::Button::new(
                            RichText::new(self.state.tr(
                                "+ Sesi baru",
                                "+ New session",
                                "+ 新しいセッション",
                            ))
                            .size(theme::fs(ui, 14.0)),
                        ),
                    )
                    .clicked()
                {
                    // TODO(M4): `arka_core::db::create_session`.
                    self.state.notify(self.state.tr(
                        "Sesi baru menyusul (M4)",
                        "New session comes in M4",
                        "新しいセッションは M4 で",
                    ));
                }

                ui.add_space(theme::SP_SM);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.state.sessions.is_empty() {
                            ui.label(
                                RichText::new(self.state.tr(
                                    "Belum ada sesi.",
                                    "No sessions yet.",
                                    "セッションはまだありません。",
                                ))
                                .size(theme::fs(ui, 13.0))
                                .color(colors.muted),
                            );
                        }
                        // Salinan supaya tidak meminjam `self` dua kali.
                        let rows = self.state.sessions.clone();
                        for row in rows {
                            let active = self.state.active_session == Some(row.id);
                            let resp = ui.add_sized(
                                [ui.available_width(), 44.0 * theme::scale_of(ui.ctx())],
                                egui::Button::new(
                                    RichText::new(&row.title)
                                        .size(theme::fs(ui, 14.0))
                                        .color(if active { colors.accent } else { colors.on_surface }),
                                )
                                .fill(if active { colors.accent_soft } else { colors.surface_raised }),
                            );
                            if resp.clicked() {
                                self.state.active_session = Some(row.id);
                            }
                            resp.on_hover_text(&row.subtitle);
                        }
                    });

                // Cakram status di dasar sidebar: memperlihatkan platform &
                // folder kerja yang sebenarnya dipakai.
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(theme::SP_XS);
                    let ws = self.state.workspace();
                    ui.label(
                        RichText::new(ws.display().to_string())
                            .size(theme::fs(ui, 11.0))
                            .color(colors.muted),
                    );
                    ui.label(
                        RichText::new(format!("Windows \u{00B7} {}", self.state.lang.native()))
                            .size(theme::fs(ui, 11.0))
                            .color(colors.muted),
                    );
                });
            });

        if !open {
            self.state.sidebar_open = false;
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context, colors: &theme::ClayColors) {
        egui::TopBottomPanel::top("topbar")
            .frame(
                egui::Frame::new()
                    .fill(colors.background)
                    .inner_margin(egui::Margin::symmetric(
                        theme::SP_LG as i8,
                        theme::SP_SM as i8,
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if !self.state.sidebar_open && ui.small_button("\u{2261}").clicked() {
                        self.state.sidebar_open = true;
                    }

                    // Lima tab: satu segmen aktif, sisanya tenang.
                    for tab in state::Tab::ALL {
                        let active = self.state.tab == tab;
                        let text = format!("{} {}", tab.glyph(), tab.label(self.state.lang));
                        let btn = egui::Button::new(
                            RichText::new(text).size(theme::fs(ui, 14.0)).color(if active {
                                colors.accent
                            } else {
                                colors.muted
                            }),
                        )
                        .fill(if active { colors.accent_soft } else { colors.surface });

                        if ui.add(btn).clicked() {
                            self.state.tab = tab;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let system_dark = ctx.input(|i| i.raw.system_theme) == Some(egui::Theme::Dark);
                        let dark = self.state.theme.is_dark(system_dark);
                        // Tombol ini "mengunci" lawan dari tampilan sekarang,
                        // jadi dari "Ikuti sistem" ia memilih tema eksplisit.
                        let icon = if dark { "\u{2600}" } else { "\u{1F319}" };
                        if ui
                            .add(
                                egui::Button::new(RichText::new(icon).size(theme::fs(ui, 15.0)))
                                    .fill(colors.surface),
                            )
                            .on_hover_text(self.state.tr("Ganti tema", "Toggle theme", "テーマ切替"))
                            .clicked()
                        {
                            self.state.theme = if dark {
                                theme::ThemeMode::Light
                            } else {
                                theme::ThemeMode::Dark
                            };
                        }
                    });
                });
            });
    }

    fn notice(&mut self, ctx: &egui::Context, colors: &theme::ClayColors) {
        let now = ctx.input(|i| i.time);
        if now > self.notice_until {
            self.state.notice = None;
            return;
        }

        egui::TopBottomPanel::bottom("notice")
            .frame(
                egui::Frame::new()
                    .fill(colors.surface_raised)
                    .inner_margin(egui::Margin::symmetric(
                        theme::SP_LG as i8,
                        theme::SP_SM as i8,
                    )),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some(text) = self.state.notice.clone() {
                        ui.label(RichText::new(text).size(theme::fs(ui, 13.0)));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("\u{00D7}").clicked() {
                            self.state.notice = None;
                        }
                    });
                });
            });

        // Snackbar harus hilang sendiri walau tidak ada input pengguna.
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }

    fn modal(&mut self, ctx: &egui::Context, colors: &theme::ClayColors) {
        let mut close = false;
        match self.state.modal.clone() {
            Modal::None => return,
            Modal::About => {
                egui::Window::new(self.state.tr("Tentang", "About", "情報"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(RichText::new("Arka Agent").size(theme::fs(ui, 18.0)).strong());
                        ui.label(
                            RichText::new(format!(
                                "v{} \u{00B7} Windows \u{00B7} Rust",
                                env!("CARGO_PKG_VERSION")
                            ))
                            .size(theme::fs(ui, 13.0))
                            .color(colors.muted),
                        );
                        ui.add_space(theme::SP_SM);
                        if widgets::clay_button(
                            ui,
                            colors,
                            &self.state.tr("Tutup", "Close", "閉じる"),
                            true,
                        )
                        .clicked()
                        {
                            close = true;
                        }
                    });
            }
            Modal::ConfirmDelete(id) => {
                egui::Window::new(self.state.tr("Hapus sesi", "Delete session", "セッション削除"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(
                            RichText::new(self.state.tr(
                                "Sesi beserta seluruh pesannya akan dihapus.",
                                "The session and all its messages will be deleted.",
                                "セッションと全メッセージが削除されます。",
                            ))
                            .size(theme::fs(ui, 14.0)),
                        );
                        ui.add_space(theme::SP_SM);
                        ui.horizontal(|ui| {
                            if widgets::clay_button(
                                ui,
                                colors,
                                &self.state.tr("Hapus", "Delete", "削除"),
                                true,
                            )
                            .clicked()
                            {
                                // TODO(M4): `db::delete_session(id)`.
                                self.state.sessions.retain(|s| s.id != id);
                                if self.state.active_session == Some(id) {
                                    self.state.active_session = None;
                                }
                                close = true;
                            }
                            if widgets::clay_button(
                                ui,
                                colors,
                                &self.state.tr("Batal", "Cancel", "キャンセル"),
                                true,
                            )
                            .clicked()
                            {
                                close = true;
                            }
                        });
                    });
            }
        }

        if close {
            self.state.modal = Modal::None;
        }
    }
}
