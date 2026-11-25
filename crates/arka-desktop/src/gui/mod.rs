//! Kerangka UI desktop (egui/eframe). Murni Rust — tanpa WebView, tanpa
//! runtime JavaScript, satu binary.
//!
//! Susunan layar mengikuti `AppScreen.kt`: drawer sesi di kiri, 5 tab, dan
//! snackbar di bawah.
//!
//! `draw` sengaja **tidak** memakai `eframe::Frame` supaya bisa dijalankan di
//! dalam test tanpa GPU/jendela (`egui::Context` menghitung tata letak di CPU).

pub mod bubble;
pub mod dialogs;
pub mod engine;
pub mod fs_model;
pub mod history;
pub mod i18n;
pub mod jobs;
pub mod markdown;
pub mod prefs;
pub mod screens;
pub mod state;
pub mod term_job;
pub mod theme;
pub mod widgets;

#[cfg(test)]
mod tests;

use eframe::egui::{self, RichText};
use state::{AppState, Modal, Persisted};

use arka_core::paths::AppPaths;

use crate::tray::{Action as TrayAction, Tray};
use engine::Msg;

const SIDEBAR_W: f32 = 248.0;
const NOTICE_SECS: f64 = 4.0;

pub struct ArkaGui {
    state: AppState,
    /// Ikon tray Windows (tidak ada di Linux/CI, tapi tipenya tetap ada supaya
    /// kode UI tidak perlu `cfg` di mana-mana).
    tray: Option<Tray>,
    /// Jendela sedang disembunyikan ke tray (bukan ditutup).
    hidden: bool,
    /// Minta keluar sungguhan walau `background_run` aktif (menu "Keluar").
    force_quit: bool,
    /// Waktu terakhir `--open` diperiksa (instance kedua menulis file di data).
    pending_checked: f64,
    /// Teks notice yang terakhir ditampilkan, untuk mendeteksi notice baru.
    shown_notice: Option<String>,
    notice_until: f64,
    /// (skala, mode tema, gelap?) yang terakhir diterapkan.
    applied: Option<(u32, theme::ThemeMode, bool)>,
}

impl ArkaGui {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: AppPaths) -> Self {
        theme::install_fonts(&cc.egui_ctx);

        let persisted = cc
            .storage
            .and_then(|s| eframe::get_value::<Persisted>(s, eframe::APP_KEY))
            .unwrap_or_default();

        let mut gui = Self::from_state(AppState::new(paths, persisted));
        gui.tray = match Tray::new(gui.state.prefs.background_run) {
            Ok(t) => Some(t),
            Err(e) => {
                eprintln!("peringatan: ikon tray tidak tersedia: {e}");
                None
            }
        };
        gui
    }

    /// Bangun tanpa `eframe` — dipakai test.
    pub fn from_state(state: AppState) -> Self {
        Self {
            state,
            tray: None,
            hidden: false,
            force_quit: false,
            pending_checked: 0.0,
            shown_notice: None,
            notice_until: 0.0,
            applied: None,
        }
    }

    /// Sambungkan ke database: muat sesi & provider, buat sesi pertama bila
    /// database masih kosong.
    ///
    /// **Tidak** dipanggil dari `draw`: test UI berjalan tanpa database dan
    /// harus tetap bisa menggambar. `main.rs` memanggilnya sekali saat start.
    pub fn bootstrap_data(&mut self) {
        // Terapkan pilihan workspace & setelan sebelum sesi/provider dibaca:
        // prompt agent dan `run_command` harus melihat folder yang sama dengan
        // yang tertera di tab Setelan sejak frame pertama.
        self.state.restore_display();
        apply_prefs(&mut self.state);
        if let Err(e) = self.state.apply_workspace() {
            self.state.notice = Some(e);
        }
        self.state.refresh_providers();
        // Statistik token & status kunci ikut dibaca supaya tab Provider tidak
        // kosong saat pertama dibuka.
        self.state.refresh_provider_rows();
        self.state.refresh_sessions();
        if self.state.db_error.is_some() {
            return;
        }
        if self.state.sessions.is_empty() {
            self.state.create_session();
        } else if self.state.active_session.is_none() {
            if let Some(first) = self.state.sessions.first() {
                let id = first.id.clone();
                self.state.open_session(id);
            }
        }
    }

    #[cfg(test)]
    pub fn state(&self) -> &AppState {
        &self.state
    }

    #[cfg(test)]
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Keadaan "jendela disembunyikan ke tray" (hanya untuk test).
    #[cfg(test)]
    pub fn hidden_for_test(&self) -> bool {
        self.hidden
    }

    /// Padanan klik menu tray "Buka" (hanya untuk test).
    #[cfg(test)]
    pub fn show_window_for_test(&mut self, ctx: &egui::Context) {
        self.show_window(ctx);
    }
}

/// Terapkan setelan ke pembaca global sesudah `Prefs::save`.
///
/// `Prefs` menulis ke database; di sini kita menyelaraskan auto-scroll yang
/// dibaca di luar UI. Centang menu tray disamakan di `poll_tray` tiap frame
/// (setelan bisa berubah dari tab Setelan), dan notifikasi dibaca langsung
/// dari `arka_core::settings` saat toast dikirim.
pub fn apply_prefs(state: &mut AppState) {
    state.chat.auto_scroll = arka_core::settings::auto_scroll();
    let p = state.prefs.clone();
    if let Err(e) = p.save() {
        state.notice = Some(e);
    }
}

impl ArkaGui {
    /// Satu langkah aplikasi: tray, instance kedua, tombol X, lalu gambar.
    ///
    /// Dipisah dari `eframe::App::update` supaya unit test bisa menjalankan
    /// jalur yang sama (termasuk `handle_close`) tanpa jendela sungguhan.
    pub fn step(&mut self, ctx: &egui::Context) {
        self.poll_tray(ctx);
        self.poll_pending(ctx);
        self.handle_close(ctx);
        self.draw(ctx);
    }

    /// Menu tray & klik ikon. Tidak pernah memblok: `try_recv` sekali per frame.
    fn poll_tray(&mut self, ctx: &egui::Context) {
        let Some(tray) = &self.tray else { return };
        tray.set_background(self.state.prefs.background_run);
        let actions = tray.poll();
        for action in actions {
            match action {
                TrayAction::Show => self.show_window(ctx),
                TrayAction::ToggleBackground(on) => {
                    self.state.prefs.background_run = on;
                    let _ = self.state.prefs.save();
                }
                TrayAction::Quit => {
                    self.force_quit = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Tampilkan kembali jendela yang disembunyikan ke tray.
    fn show_window(&mut self, ctx: &egui::Context) {
        self.hidden = false;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        ctx.request_repaint();
    }

    /// `--open <file>` dari instance kedua: tulis path ke `pending-open.txt`,
    /// instance pertama memeriksanya tiap 2 detik (bukan tiap frame) lalu
    /// membuka pratinjau di tab File.
    fn poll_pending(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if now - self.pending_checked < 2.0 {
            return;
        }
        self.pending_checked = now;
        let Some(path) = crate::single::take_pending(&self.state.paths.data_dir) else {
            return;
        };
        let root = self.state.workspace();
        let rel = path
            .strip_prefix(&root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        match arka_core::file::stat_at(&root, &rel) {
            Ok(info) => {
                self.state.tab = state::Tab::File;
                self.state.files_ui.preview =
                    Some(crate::preview::PreviewState::load(&root, &info, ctx));
                self.show_window(ctx);
            }
            Err(e) => self.state.notify(e),
        }
    }

    /// Tombol X: sembunyikan ke tray bila "jalan di latar belakang" aktif.
    fn handle_close(&mut self, ctx: &egui::Context) {
        let closing = ctx.input(|i| i.viewport().close_requested());
        if !closing {
            return;
        }
        if self.force_quit || !self.state.prefs.background_run {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        self.hidden = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
}

impl eframe::App for ArkaGui {
    /// Ukuran/posisi jendela disimpan `eframe` sendiri; kita simpan setelan UI.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.state.persist());
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.step(ctx);
    }
}

impl ArkaGui {
    /// Satu frame penuh. Semua gambar ada di sini.
    pub fn draw(&mut self, ctx: &egui::Context) {
        // Kejadian dari thread agent diproses **sebelum** menggambar, supaya
        // frame ini sudah menampilkan token yang baru tiba.
        self.poll_engine(ctx);

        let system_dark = ctx.input(|i| i.raw.system_theme) == Some(egui::Theme::Dark);
        let dark = self.state.theme.is_dark(system_dark);
        let scale = self.state.font_scale.factor();

        let key = (scale.to_bits(), self.state.theme, dark);
        if self.applied != Some(key) {
            theme::apply(ctx, scale, self.state.theme);
            self.applied = Some(key);
        }
        let colors = *self.state.theme.colors(system_dark);

        if self.state.notice != self.shown_notice {
            self.shown_notice = self.state.notice.clone();
            self.notice_until = ctx.input(|i| i.time) + NOTICE_SECS;
        }

        self.drop_files(ctx);

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

        self.drop_hint(ctx, &colors);

        // Kartu persetujuan/pertanyaan menggantung di atas semuanya.
        dialogs::show(ctx, &mut self.state, &colors);

        self.modal(ctx, &colors);
    }

    // ── Seret & lepas dari Explorer ─────────────────────────────────────────

    /// Berkas yang dijatuhkan ke jendela: di tab File masuk ke folder yang
    /// sedang dibuka; di tab lain jadi lampiran pesan yang sedang ditulis.
    ///
    /// Impor lewat jalur yang sama dengan tombol "Impor"
    /// ([`screens::files::import_now`]) supaya guard traversal, nama bentrok,
    /// dan penyegaran cache tidak punya dua versi.
    fn drop_files(&mut self, ctx: &egui::Context) {
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        if dropped.is_empty() {
            return;
        }
        if self.state.tab == state::Tab::File {
            screens::files::import_now(&mut self.state, &dropped);
            return;
        }
        let n = dropped.len();
        for p in dropped {
            self.state
                .chat
                .attachments
                .push(screens::chat::attachment_from(&p));
        }
        self.state.notify(format!(
            "{} {}",
            n,
            self.state
                .tr("berkas dilampirkan", "file(s) attached", "件を添付しました")
        ));
    }

    /// Tanda terlihat saat berkas diseret di atas jendela.
    ///
    /// Tanpa ini, pengguna tidak tahu apakah aplikasi menerima seret-lepas;
    /// egui sendiri tidak menggambar apa pun.
    fn drop_hint(&self, ctx: &egui::Context, colors: &theme::ClayColors) {
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        if !hovering {
            return;
        }
        let text = if self.state.tab == state::Tab::File {
            self.state.tr(
                "Lepaskan untuk mengimpor ke folder ini",
                "Drop to import into this folder",
                "ドロップしてこのフォルダーへインポート",
            )
        } else {
            self.state.tr(
                "Lepaskan untuk melampirkan ke pesan",
                "Drop to attach to the message",
                "ドロップしてメッセージに添付",
            )
        };
        egui::Area::new(egui::Id::new("drop-hint"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .order(egui::Order::Tooltip)
            .show(ctx, |ui| {
                widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
                    ui.label(RichText::new(text).size(theme::fs(ui, 16.0)).strong());
                });
            });
    }

    // ── Kejadian agent ──────────────────────────────────────────────────────

    fn poll_engine(&mut self, ctx: &egui::Context) {
        for m in self.state.chat.engine.drain() {
            self.handle(m, ctx);
        }
    }

    fn handle(&mut self, m: Msg, ctx: &egui::Context) {
        let active = self.state.active_session.clone();
        match m {
            Msg::Delta { session, text } => {
                if active.as_deref() == Some(session.as_str()) {
                    self.state.chat.streaming.push_str(&text);
                }
            }
            Msg::Done { session, text } => {
                if active.as_deref() == Some(session.as_str()) {
                    self.state.chat.streaming = text;
                }
            }
            Msg::Error { session, message } => {
                self.state.chat.streaming.clear();
                if active.as_deref() == Some(session.as_str()) {
                    self.state.notify(message);
                }
            }
            Msg::Tool {
                session,
                label,
                display,
            } => {
                // Terminal dipakai bersama semua sesi → beri label sesi.
                self.state
                    .chat
                    .terminal_log
                    .push_str(&format!("\n[{label}] {display}\n"));
                // Agent mungkin menulis/menghapus berkas: tanda usang saja,
                // pembacaan ulang terjadi saat tab File dibuka.
                self.state.files.invalidate();
                let _ = session;
            }
            Msg::Approval {
                id,
                action,
                target,
                detail,
            } => {
                self.state.chat.approval = Some(state::ApprovalCard {
                    id,
                    action,
                    target,
                    detail,
                });
            }
            Msg::Question {
                id,
                question,
                options,
                files,
            } => {
                self.state.chat.question = Some(state::QuestionCard {
                    id,
                    question,
                    options,
                    files,
                });
            }
            Msg::Finished { session } => {
                crate::keepawake::end();
                // Muat ulang dari database: kebenaran ada di sana, bukan di
                // buffer streaming.
                if active.as_deref() == Some(session.as_str()) {
                    self.state.reload_messages();
                } else {
                    self.state.chat.unread.insert(session.clone());
                }
                self.state.refresh_sessions();

                // Jendela tidak fokus → minta perhatian OS (kedip taskbar) dan
                // tampilkan notice di dalam aplikasi.
                let focused = ctx.input(|i| i.focused);
                if !focused && self.state.prefs.notifications {
                    let body = self.state.tr(
                        "Sesi selesai menjawab.",
                        "A session finished answering.",
                        "セッションの応答が完了しました。",
                    );
                    let _ = crate::desk_notify::toast(
                        "Arka Agent",
                        &crate::desk_notify::ringkas(&body, 180),
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                        egui::UserAttentionType::Informational,
                    ));
                    self.state.notify(self.state.tr(
                        "Sesi selesai menjawab.",
                        "A session finished answering.",
                        "セッションの応答が完了しました。",
                    ));
                }
                ctx.request_repaint();
            }
        }
    }

    // ── Panel ───────────────────────────────────────────────────────────────

    fn sidebar(&mut self, ctx: &egui::Context, colors: &theme::ClayColors) {
        if !self.state.sidebar_open {
            return;
        }

        let mut open = true;
        let mut select: Option<String> = None;
        let mut delete: Option<String> = None;
        let mut start_rename: Option<(String, String)> = None;
        let mut commit_rename: Option<(String, String)> = None;

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
                    ui.label(
                        RichText::new("Arka Agent")
                            .size(theme::fs(ui, 18.0))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("\u{2039}").clicked() {
                            open = false;
                        }
                    });
                });

                ui.add_space(theme::SP_MD);
                let label = self.state.tr("Sesi", "Sessions", "セッション");
                ui.label(
                    RichText::new(label)
                        .size(theme::fs(ui, 11.0))
                        .color(colors.muted)
                        .strong(),
                );

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
                    self.state.create_session();
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

                        let rows = self.state.sessions.clone();
                        let renaming = self.state.chat.rename.clone();
                        for row in rows {
                            let active =
                                self.state.active_session.as_deref() == Some(row.id.as_str());
                            let busy = self.state.chat.engine.is_running(&row.id);
                            let unread = self.state.chat.unread.contains(&row.id);

                            // Ubah judul langsung di baris (inline edit).
                            if let Some((rid, text)) = &renaming {
                                if rid == &row.id {
                                    let mut buf = text.clone();
                                    let resp = ui.add_sized(
                                        [ui.available_width(), 34.0 * theme::scale_of(ui.ctx())],
                                        egui::TextEdit::singleline(&mut buf).hint_text(
                                            self.state.tr(
                                                "Judul sesi",
                                                "Session title",
                                                "セッション名",
                                            ),
                                        ),
                                    );
                                    resp.request_focus();
                                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                    if enter || resp.lost_focus() {
                                        commit_rename = Some((row.id.clone(), buf));
                                    } else {
                                        // Simpan teks sementara supaya ketikan
                                        // tidak hilang tiap frame.
                                        self.state.chat.rename = Some((row.id.clone(), buf));
                                    }
                                    continue;
                                }
                            }

                            let title = if busy {
                                format!("\u{23F3} {}", row.title)
                            } else if unread {
                                format!("\u{2022} {}", row.title)
                            } else {
                                row.title.clone()
                            };

                            let resp = ui.add_sized(
                                [ui.available_width(), 44.0 * theme::scale_of(ui.ctx())],
                                egui::Button::new(
                                    RichText::new(title).size(theme::fs(ui, 14.0)).color(
                                        if active {
                                            colors.accent
                                        } else {
                                            colors.on_surface
                                        },
                                    ),
                                )
                                .fill(if active {
                                    colors.accent_soft
                                } else {
                                    colors.surface_raised
                                }),
                            );
                            if resp.clicked() {
                                select = Some(row.id.clone());
                            }
                            let resp = resp.on_hover_text(&row.subtitle);

                            // Klik kanan: ubah judul / hapus.
                            resp.context_menu(|ui| {
                                if ui
                                    .button(self.state.tr("Ubah judul", "Rename", "名前を変更"))
                                    .clicked()
                                {
                                    start_rename = Some((row.id.clone(), row.title.clone()));
                                    ui.close();
                                }
                                if ui
                                    .button(self.state.tr("Hapus", "Delete", "削除"))
                                    .clicked()
                                {
                                    delete = Some(row.id.clone());
                                    ui.close();
                                }
                            });
                        }
                    });

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
        if let Some((id, text)) = start_rename {
            if let Some(row) = self.state.sessions.iter().find(|s| s.id == id) {
                self.state.chat.rename = Some((row.id.clone(), text));
            }
        }
        if let Some((id, text)) = commit_rename {
            self.state.chat.rename = None;
            self.state.rename_session(&id, &text);
        }
        if let Some(id) = select {
            self.state.open_session(id);
        }
        if let Some(id) = delete {
            self.state.modal = Modal::ConfirmDelete(id);
        }
    }

    fn top_bar(&mut self, ctx: &egui::Context, colors: &theme::ClayColors) {
        egui::TopBottomPanel::top("topbar")
            .frame(egui::Frame::new().fill(colors.background).inner_margin(
                egui::Margin::symmetric(theme::SP_LG as i8, theme::SP_SM as i8),
            ))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if !self.state.sidebar_open && ui.small_button("\u{2261}").clicked() {
                        self.state.sidebar_open = true;
                    }

                    for tab in state::Tab::ALL {
                        let active = self.state.tab == tab;
                        let text = format!("{} {}", tab.glyph(), tab.label(self.state.lang));
                        let btn = egui::Button::new(
                            RichText::new(text)
                                .size(theme::fs(ui, 14.0))
                                .color(if active { colors.accent } else { colors.muted }),
                        )
                        .fill(if active {
                            colors.accent_soft
                        } else {
                            colors.surface
                        });

                        if ui.add(btn).clicked() {
                            self.state.tab = tab;
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let system_dark =
                            ctx.input(|i| i.raw.system_theme) == Some(egui::Theme::Dark);
                        let dark = self.state.theme.is_dark(system_dark);
                        let icon = if dark { "\u{2600}" } else { "\u{1F319}" };
                        if ui
                            .add(
                                egui::Button::new(RichText::new(icon).size(theme::fs(ui, 15.0)))
                                    .fill(colors.surface),
                            )
                            .on_hover_text(self.state.tr(
                                "Ganti tema",
                                "Toggle theme",
                                "テーマ切替",
                            ))
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
            .frame(egui::Frame::new().fill(colors.surface_raised).inner_margin(
                egui::Margin::symmetric(theme::SP_LG as i8, theme::SP_SM as i8),
            ))
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
                        ui.label(
                            RichText::new("Arka Agent")
                                .size(theme::fs(ui, 18.0))
                                .strong(),
                        );
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
            Modal::PickFile => {
                let dir = self.state.chat.pick_dir.clone();
                let mut nav: Option<std::path::PathBuf> = None;
                let mut attach: Option<std::path::PathBuf> = None;

                egui::Window::new(self.state.tr(
                    "Lampirkan berkas",
                    "Attach a file",
                    "ファイルを添付",
                ))
                .collapsible(false)
                .resizable(true)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(520.0 * theme::scale_of(ctx));
                    ui.horizontal(|ui| {
                        if ui.small_button("\u{2B06}").clicked() {
                            if let Some(parent) = dir.parent() {
                                nav = Some(parent.to_path_buf());
                            }
                        }
                        ui.label(
                            RichText::new(dir.display().to_string())
                                .size(theme::fs(ui, 11.5))
                                .monospace()
                                .color(colors.muted),
                        );
                    });
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .max_height(320.0 * theme::scale_of(ctx))
                        .auto_shrink([false, false])
                        .show(ui, |ui| match fs_model::read_dir(&dir) {
                            Ok(entries) => {
                                if entries.is_empty() {
                                    ui.label(
                                        RichText::new(self.state.tr(
                                            "Folder kosong.",
                                            "Empty folder.",
                                            "空のフォルダーです。",
                                        ))
                                        .size(theme::fs(ui, 12.0))
                                        .color(colors.muted),
                                    );
                                }
                                for e in entries {
                                    let label = if e.is_dir {
                                        format!("\u{1F4C1} {}", e.name)
                                    } else {
                                        format!("\u{1F4C4} {}", e.name)
                                    };
                                    let resp = ui.add_sized(
                                        [ui.available_width(), 30.0 * theme::scale_of(ctx)],
                                        egui::Button::new(
                                            RichText::new(label).size(theme::fs(ui, 13.0)),
                                        )
                                        .fill(colors.surface_raised),
                                    );
                                    if resp.clicked() {
                                        if e.is_dir {
                                            nav = Some(dir.join(&e.name));
                                        } else {
                                            attach = Some(dir.join(&e.name));
                                        }
                                    }
                                    if !e.is_dir {
                                        resp.on_hover_text(fs_model::human_size(e.size));
                                    }
                                }
                            }
                            Err(err) => {
                                ui.label(
                                    RichText::new(err)
                                        .size(theme::fs(ui, 12.0))
                                        .color(colors.danger),
                                );
                            }
                        });

                    ui.separator();
                    ui.horizontal(|ui| {
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
                        if !self.state.chat.attachments.is_empty() {
                            ui.label(
                                RichText::new(format!(
                                    "{} lampiran",
                                    self.state.chat.attachments.len()
                                ))
                                .size(theme::fs(ui, 11.5))
                                .color(colors.muted),
                            );
                        }
                    });
                });

                if let Some(d) = nav {
                    self.state.chat.pick_dir = d;
                }
                if let Some(f) = attach {
                    self.state
                        .chat
                        .attachments
                        .push(screens::chat::attachment_from(&f));
                    close = true;
                }
            }
            Modal::ConfirmDelete(id) => {
                let judul = self
                    .state
                    .sessions
                    .iter()
                    .find(|s| s.id == id)
                    .map(|s| s.title.clone())
                    .unwrap_or_default();
                egui::Window::new(
                    self.state
                        .tr("Hapus sesi", "Delete session", "セッション削除"),
                )
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(RichText::new(&judul).size(theme::fs(ui, 14.0)).strong());
                    ui.add_space(theme::SP_XS);
                    ui.label(
                        RichText::new(self.state.tr(
                            "Sesi beserta seluruh pesannya akan dihapus.",
                            "The session and all its messages will be deleted.",
                            "セッションと全メッセージが削除されます。",
                        ))
                        .size(theme::fs(ui, 13.0))
                        .color(colors.muted),
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
                            self.state.delete_session(&id);
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
