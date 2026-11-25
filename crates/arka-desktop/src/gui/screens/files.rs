//! Tab File (M6): penjelajah workspace + pratinjau + impor/ekspor/hapus.
//!
//! Pembacaan disk ada di `arka_core::file` (akar eksplisit, guard traversal),
//! pemuatan pratinjau di [`crate::preview`], dan cache di
//! [`crate::gui::fs_model`]. Berkas ini hanya menggambar dan meneruskan aksi —
//! pola yang sama dipakai tab lain.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Ui};

use arka_core::file::{self, FileInfo};

use crate::gui::fs_model::{breadcrumb, human_size, icon};
use crate::gui::state::AppState;
use crate::gui::theme::{self, fs, scale_of, ClayColors};
use crate::gui::widgets;
use crate::platform;
use crate::preview::Content;

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    let root = state.workspace();
    let dir = state.files_ui.dir.clone();

    // Baca hanya bila folder berubah atau belum pernah dibaca.
    state.files.ensure(&root, &dir);

    header(ui, state, colors, &root, &dir);

    ui.add_space(theme::SP_SM);
    crumbs(ui, state, colors, &dir);
    ui.add_space(theme::SP_MD);

    if let Some(e) = state.files.error.clone() {
        widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
            ui.label(RichText::new(e).size(fs(ui, 13.0)).color(colors.danger));
        });
    } else if state.files.entries.is_empty() {
        widgets::clay_card(ui, colors, None, |ui| {
            ui.label(
                RichText::new(state.tr(
                    "Folder masih kosong. Berkas yang dibuat dari Terminal atau \
                     dijatuhkan ke jendela ini akan muncul di sini.",
                    "This folder is empty. Files created in Terminal or dropped \
                     onto this window will show up here.",
                    "このフォルダーは空です。ターミナルで作成したファイルや、このウィンドウに\
                     ドロップしたファイルが表示されます。",
                ))
                .size(fs(ui, 13.0))
                .color(colors.muted),
            );
        });
    } else {
        list(ui, state, colors, &root);
    }

    preview_window(ui.ctx(), state, colors);
    trash_dialog(ui.ctx(), state, colors, &root);
    new_folder_dialog(ui.ctx(), state, colors, &root);
    import_picker(ui.ctx(), state, colors);
}

// ── Kepala & breadcrumb ─────────────────────────────────────────────────────

fn header(
    ui: &mut Ui,
    state: &mut AppState,
    colors: &ClayColors,
    root: &std::path::Path,
    dir: &str,
) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(state.tr("Workspace", "Workspace", "ワークスペース"))
                .size(fs(ui, 22.0))
                .strong(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // "Muat ulang": nama ini dipakai test UI, jangan diganti sembarangan.
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Muat ulang", "Refresh", "更新"),
                colors.accent,
            )
            .clicked()
            {
                state.files.refresh(root, dir);
            }

            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Impor", "Import", "インポート"),
                colors.accent,
            )
            .clicked()
            {
                begin_import(state);
            }

            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Folder baru", "New folder", "新しいフォルダー"),
                colors.accent,
            )
            .clicked()
            {
                state.files_ui.new_folder = Some(String::new());
            }

            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Explorer", "Explorer", "エクスプローラー"),
                colors.muted,
            )
            .clicked()
            {
                if let Err(e) = platform::open_path(root) {
                    state.notify(e);
                }
            }
        });
    });

    ui.add_space(theme::SP_XS);
    ui.label(
        RichText::new(root.display().to_string())
            .size(fs(ui, 12.0))
            .color(colors.muted)
            .monospace(),
    );
}

fn crumbs(ui: &mut Ui, state: &mut AppState, colors: &ClayColors, dir: &str) {
    let mut go: Option<String> = None;
    ui.horizontal_wrapped(|ui| {
        for (i, c) in breadcrumb(dir).iter().enumerate() {
            if i > 0 {
                ui.label(RichText::new("/").size(fs(ui, 12.0)).color(colors.muted));
            }
            let label = if i == 0 {
                state.tr("akar", "root", "ルート").to_string()
            } else {
                c.label.clone()
            };
            let last = i + 1 == breadcrumb(dir).len();
            let text = RichText::new(label).size(fs(ui, 13.0)).color(if last {
                colors.on_surface
            } else {
                colors.accent
            });
            // Segmen terakhir tidak perlu diklik (sudah di sana).
            if last {
                ui.label(text.strong());
            } else if ui
                .add(egui::Label::new(text).sense(egui::Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                go = Some(c.rel.clone());
            }
        }
    });
    if let Some(g) = go {
        state.files_ui.dir = g;
        state.files_ui.rename = None;
    }
}

// ── Daftar berkas ───────────────────────────────────────────────────────────

fn list(ui: &mut Ui, state: &mut AppState, colors: &ClayColors, root: &std::path::Path) {
    let dir = state.files_ui.dir.clone();
    let entries = state.files.entries.clone();

    // Aksi dikumpulkan dulu: `state` tidak boleh dipinjam dua kali di closure.
    let mut enter: Option<String> = None;
    let mut open_preview: Option<FileInfo> = None;
    let mut ask_trash: Option<FileInfo> = None;
    let mut start_rename: Option<(String, String)> = None;
    let mut commit_rename: Option<(String, String)> = None;
    let mut cancel_rename = false;

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if !dir.is_empty() {
                let margin = (10.0 * scale_of(ui.ctx())) as i8;
                widgets::clay_panel(ui, colors, colors.surface, 16, margin, |ui| {
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Label::new(
                                    RichText::new(format!(
                                        "\u{2B06}  {}",
                                        state.tr("Folder induk", "Parent folder", "親フォルダー")
                                    ))
                                    .size(fs(ui, 13.0))
                                    .color(colors.accent),
                                )
                                .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            enter = Some(crate::gui::fs_model::parent(&dir));
                        }
                    });
                });
                ui.add_space(theme::SP_XS);
            }

            for e in &entries {
                let rel = e.path.clone();
                let is_dir = e.is_dir;

                // Mode ubah nama: baris ini menjadi kotak teks.
                if let Some((target, draft)) = state.files_ui.rename.clone() {
                    if target == rel {
                        let margin = (8.0 * scale_of(ui.ctx())) as i8;
                        widgets::clay_panel(ui, colors, colors.surface_raised, 14, margin, |ui| {
                            ui.horizontal(|ui| {
                                let mut text = draft.clone();
                                let resp = ui.add(
                                    egui::TextEdit::singleline(&mut text)
                                        .desired_width(260.0 * scale_of(ui.ctx()))
                                        .hint_text(state.tr("Nama baru", "New name", "新しい名前")),
                                );
                                if resp.changed() {
                                    start_rename = Some((target.clone(), text.clone()));
                                }
                                let enter_key = ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if widgets::clay_pill_button(
                                    ui,
                                    colors,
                                    &state.tr("Simpan", "Save", "保存"),
                                    colors.accent,
                                )
                                .clicked()
                                    || enter_key
                                {
                                    commit_rename = Some((target.clone(), text));
                                }
                                if widgets::clay_pill_button(
                                    ui,
                                    colors,
                                    &state.tr("Batal", "Cancel", "キャンセル"),
                                    colors.muted,
                                )
                                .clicked()
                                {
                                    cancel_rename = true;
                                }
                            });
                        });
                        ui.add_space(theme::SP_XS);
                        continue;
                    }
                }

                let margin = (10.0 * scale_of(ui.ctx())) as i8;
                widgets::clay_panel(ui, colors, colors.surface, 16, margin, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(icon(&e.kind, is_dir)).size(fs(ui, 15.0)));
                        let name = RichText::new(&e.name)
                            .size(fs(ui, 14.0))
                            .color(colors.on_surface);
                        if ui
                            .add(
                                egui::Label::new(name)
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            if is_dir {
                                enter = Some(rel.clone());
                            } else {
                                open_preview = Some(e.clone());
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("\u{1F5D1}").size(fs(ui, 13.0)),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text(state.tr("Hapus", "Delete", "削除"))
                                .clicked()
                            {
                                ask_trash = Some(e.clone());
                            }
                            if ui
                                .add(
                                    egui::Button::new(RichText::new("\u{270F}").size(fs(ui, 13.0)))
                                        .frame(false),
                                )
                                .on_hover_text(state.tr("Ubah nama", "Rename", "名前変更"))
                                .clicked()
                            {
                                start_rename = Some((rel.clone(), e.name.clone()));
                            }
                            if !is_dir {
                                ui.label(
                                    RichText::new(human_size(e.size as u64))
                                        .size(fs(ui, 12.0))
                                        .color(colors.muted)
                                        .monospace(),
                                );
                            } else {
                                ui.label(
                                    RichText::new(state.tr("folder", "folder", "フォルダー"))
                                        .size(fs(ui, 11.5))
                                        .color(colors.muted),
                                );
                            }
                        });
                    });
                });
                ui.add_space(theme::SP_XS);
            }
        });

    // ── Terapkan aksi ───────────────────────────────────────────────────────
    if let Some(d) = enter {
        state.files_ui.dir = d;
        state.files_ui.rename = None;
    }
    if let Some((target, text)) = start_rename {
        state.files_ui.rename = Some((target, text));
    }
    if cancel_rename {
        state.files_ui.rename = None;
    }
    if let Some((from, to)) = commit_rename {
        commit_rename_now(state, root, &from, &to);
    }
    if let Some(f) = ask_trash {
        state.files_ui.confirm_trash = Some(f);
    }
    if let Some(f) = open_preview {
        state.files_ui.preview = Some(crate::preview::PreviewState::load(root, &f, ui.ctx()));
    }
}

/// Ubah nama berkas di disk. Nama kosong / sama = batal tanpa pesan.
fn commit_rename_now(state: &mut AppState, root: &std::path::Path, from: &str, to: &str) {
    let to = to.trim();
    let dir = crate::gui::fs_model::parent(from);
    if to.is_empty() || to == from.rsplit('/').next().unwrap_or("") {
        state.files_ui.rename = None;
        return;
    }
    // Nama dengan garis miring akan memindahkan berkas ke folder lain tanpa
    // disadari pengguna; ubah nama saja, bukan path.
    if to.contains('/') || to.contains('\\') {
        state.notify(state.tr(
            "Nama tidak boleh berisi / atau \\",
            "Name cannot contain / or \\",
            "名前に / や \\ は使えません",
        ));
        return;
    }
    let dst = file::join_rel(&dir, to);
    match file::rename_at(root, from, &dst) {
        Ok(()) => {
            state.files_ui.rename = None;
            state.files.refresh(root, &state.files_ui.dir.clone());
        }
        Err(e) => state.notify(e),
    }
}

// ── Impor ───────────────────────────────────────────────────────────────────

/// Tombol "Impor": dialog OS bila ada, kalau tidak pemilih di dalam aplikasi.
fn begin_import(state: &mut AppState) {
    if platform::HAS_OS_DIALOG {
        // Pengguna membatalkan dialog → tidak membuka pemilih kedua.
        if let Some(paths) = platform::pick_files() {
            if !paths.is_empty() {
                import_now(state, &paths);
            }
        }
        return;
    }
    // Linux/pengembangan: tak ada dialog OS, buka pemilih bawaan.
    let start = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| state.workspace());
    state.files_ui.picker_dir = Some(start);
}

/// Salin berkas/folder luar ke folder yang sedang dibuka. Mengembalikan jumlah
/// yang berhasil.
pub fn import_now(state: &mut AppState, paths: &[PathBuf]) -> usize {
    let root = state.workspace();
    let dir = state.files_ui.dir.clone();
    let mut ok = 0usize;
    let mut err: Option<String> = None;
    for p in paths {
        match file::copy_into_at(&root, &dir, p) {
            Ok(_) => ok += 1,
            Err(e) => err = Some(format!("{}: {e}", p.display())),
        }
    }
    state.files.invalidate();
    let dir_now = state.files_ui.dir.clone();
    state.files.refresh(&root, &dir_now);
    match (ok, err) {
        (n, None) => state.notify(format!(
            "{} {} {}",
            n,
            state.tr(
                "berkas diimpor ke",
                "file(s) imported to",
                "件をインポート:"
            ),
            if dir.is_empty() {
                state.tr("akar workspace", "workspace root", "ワークスペースのルート")
            } else {
                dir
            }
        )),
        (_, Some(e)) => state.notify(e),
    }
    ok
}

fn import_picker(ctx: &egui::Context, state: &mut AppState, colors: &ClayColors) {
    let Some(dir) = state.files_ui.picker_dir.clone() else {
        return;
    };
    let mut open = true;
    let mut nav: Option<PathBuf> = None;
    let mut chosen: Option<PathBuf> = None;

    egui::Window::new(state.tr("Impor berkas", "Import file", "ファイルをインポート"))
        .collapsible(false)
        .resizable(true)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut open)
        .show(ctx, |ui| {
            ui.set_max_width(560.0 * scale_of(ctx));
            ui.horizontal(|ui| {
                if ui.small_button("\u{2B06}").clicked() {
                    if let Some(p) = dir.parent() {
                        nav = Some(p.to_path_buf());
                    }
                }
                ui.label(
                    RichText::new(dir.display().to_string())
                        .size(fs(ui, 11.5))
                        .monospace()
                        .color(colors.muted),
                );
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(340.0 * scale_of(ctx))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut entries: Vec<(String, PathBuf, bool)> = std::fs::read_dir(&dir)
                        .map(|rd| {
                            rd.filter_map(|e| e.ok())
                                .map(|e| {
                                    (
                                        e.file_name().to_string_lossy().to_string(),
                                        e.path(),
                                        e.file_type().map(|t| t.is_dir()).unwrap_or(false),
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    entries.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
                    for (name, path, is_dir) in entries {
                        let label = format!(
                            "{} {name}",
                            icon(if is_dir { "dir" } else { "text" }, is_dir)
                        );
                        if ui
                            .add(
                                egui::Label::new(RichText::new(label).size(fs(ui, 13.0)))
                                    .sense(egui::Sense::click()),
                            )
                            .clicked()
                        {
                            if is_dir {
                                nav = Some(path);
                            } else {
                                chosen = Some(path);
                            }
                        }
                    }
                });
        });

    if let Some(d) = nav {
        state.files_ui.picker_dir = Some(d);
    }
    if let Some(p) = chosen {
        state.files_ui.picker_dir = None;
        import_now(state, &[p]);
    }
    if !open {
        state.files_ui.picker_dir = None;
    }
}

// ── Dialog: sampah, folder baru, pratinjau ─────────────────────────────────

fn trash_dialog(
    ctx: &egui::Context,
    state: &mut AppState,
    colors: &ClayColors,
    root: &std::path::Path,
) {
    let Some(info) = state.files_ui.confirm_trash.clone() else {
        return;
    };
    // `win_open` milik judul jendela (tombol X), `open` milik tombol di dalam.
    let mut win_open = true;
    let mut open = true;
    let mut confirmed = false;
    egui::Window::new(state.tr("Pindahkan ke sampah?", "Move to trash?", "ゴミ箱へ移動しますか?"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut win_open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(&info.name)
                    .size(fs(ui, 14.0))
                    .strong()
                    .color(colors.on_surface),
            );
            ui.add_space(theme::SP_XS);
            ui.label(
                RichText::new(if info.is_dir {
                    state.tr(
                        "Folder dan seluruh isinya dipindahkan ke sampah workspace \
                         (bisa dipulihkan selama belum dikosongkan).",
                        "The folder and everything inside it goes to the workspace \
                         trash (recoverable until emptied).",
                        "フォルダーと中身はワークスペースのゴミ箱へ移動します（空にするまで復元可能）。",
                    )
                } else {
                    state.tr(
                        "Berkas dipindahkan ke sampah workspace (bisa dipulihkan \
                         selama belum dikosongkan).",
                        "The file goes to the workspace trash (recoverable until \
                         emptied).",
                        "ファイルはワークスペースのゴミ箱へ移動します（空にするまで復元可能）。",
                    )
                })
                .size(fs(ui, 12.5))
                .color(colors.muted),
            );
            ui.add_space(theme::SP_SM);
            ui.horizontal(|ui| {
                if widgets::clay_button(ui, colors, &state.tr("Pindahkan", "Move", "移動"), true)
                    .clicked()
                {
                    confirmed = true;
                }
                if widgets::clay_button(ui, colors, &state.tr("Batal", "Cancel", "キャンセル"), true)
                    .clicked()
                {
                    confirmed = false;
                    open = false;
                }
            });
        });

    if confirmed {
        match file::trash_at(root, &info.path) {
            Ok(_) => {
                state.notify(state.tr(
                    "Dipindahkan ke sampah workspace.",
                    "Moved to the workspace trash.",
                    "ワークスペースのゴミ箱へ移動しました。",
                ));
                state.files.refresh(root, &state.files_ui.dir.clone());
            }
            Err(e) => state.notify(e),
        }
        state.files_ui.confirm_trash = None;
    } else if !open || !win_open {
        state.files_ui.confirm_trash = None;
    }
}

fn new_folder_dialog(
    ctx: &egui::Context,
    state: &mut AppState,
    colors: &ClayColors,
    root: &std::path::Path,
) {
    let Some(mut draft) = state.files_ui.new_folder.clone() else {
        return;
    };
    let mut win_open = true;
    let mut open = true;
    let mut create = false;

    egui::Window::new(state.tr("Folder baru", "New folder", "新しいフォルダー"))
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut win_open)
        .show(ctx, |ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut draft)
                    .desired_width(260.0 * scale_of(ctx))
                    .hint_text(state.tr("Nama folder", "Folder name", "フォルダー名")),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                create = true;
            }
            ui.add_space(theme::SP_SM);
            ui.horizontal(|ui| {
                let valid = !draft.trim().is_empty();
                if widgets::clay_button(ui, colors, &state.tr("Buat", "Create", "作成"), valid)
                    .clicked()
                    && valid
                {
                    create = true;
                }
                if widgets::clay_button(
                    ui,
                    colors,
                    &state.tr("Batal", "Cancel", "キャンセル"),
                    true,
                )
                .clicked()
                {
                    open = false;
                }
            });
        });

    if create {
        let name = draft.trim();
        if name.contains('/') || name.contains('\\') {
            state.notify(state.tr(
                "Nama folder tidak boleh berisi / atau \\",
                "Folder name cannot contain / or \\",
                "フォルダー名に / や \\ は使えません",
            ));
        } else {
            let rel = file::join_rel(&state.files_ui.dir.clone(), name);
            match file::mkdir_at(root, &rel) {
                Ok(()) => {
                    state.notify(format!(
                        "{} {}",
                        state.tr("Folder dibuat:", "Folder created:", "フォルダーを作成:"),
                        rel
                    ));
                    state.files.refresh(root, &state.files_ui.dir.clone());
                }
                Err(e) => state.notify(e),
            }
        }
        state.files_ui.new_folder = None;
    } else if !open || !win_open {
        state.files_ui.new_folder = None;
    } else {
        state.files_ui.new_folder = Some(draft);
    }
}

fn preview_window(ctx: &egui::Context, state: &mut AppState, colors: &ClayColors) {
    let Some(p) = state.files_ui.preview.as_ref() else {
        return;
    };
    let name = p.name.clone();
    let kind = p.kind.clone();
    let size = p.size;
    let rel = p.rel.clone();

    let mut win_open = true;
    let mut open = true;
    let mut do_open: Option<String> = None;
    let mut do_export = false;
    let mut do_trash = false;

    egui::Window::new(name)
        .collapsible(false)
        .resizable(true)
        .default_width(720.0 * scale_of(ctx))
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .open(&mut win_open)
        .show(ctx, |ui| {
            ui.label(
                RichText::new(format!(
                    "{} \u{00B7} {}",
                    state.tr(&kind_label_id(&kind), &kind_label_en(&kind), &kind_label_ja(&kind)),
                    human_size(size.max(0) as u64)
                ))
                .size(fs(ui, 11.5))
                .color(colors.muted),
            );
            ui.add_space(theme::SP_XS);
            ui.separator();

            let p = state.files_ui.preview.as_ref().unwrap();
            match &p.content {
                Content::Text(t) => {
                    egui::ScrollArea::both()
                        .max_height(460.0 * scale_of(ctx))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(t).size(fs(ui, 12.0)).monospace(),
                                )
                                .wrap_mode(egui::TextWrapMode::Extend),
                            );
                        });
                }
                Content::Image {
                    texture,
                    width,
                    height,
                    downscaled,
                } => {
                    let max_w = 680.0 * scale_of(ctx);
                    let max_h = 460.0 * scale_of(ctx);
                    let ratio = (*width as f32 / *height as f32).max(0.01);
                    let (mut w, mut h) = (max_w, max_w / ratio);
                    if h > max_h {
                        h = max_h;
                        w = max_h * ratio;
                    }
                    ui.add(egui::Image::new(texture).fit_to_exact_size(egui::vec2(w, h)));
                    if *downscaled {
                        ui.label(
                            RichText::new(format!(
                                "{} {}×{} px",
                                state.tr(
                                    "Diperkecil dari",
                                    "Downscaled from",
                                    "縮小表示（元サイズ）"
                                ),
                                width,
                                height
                            ))
                            .size(fs(ui, 11.0))
                            .color(colors.muted),
                        );
                    }
                }
                Content::Pdf { pages } => {
                    ui.add_space(theme::SP_SM);
                    ui.label(
                        RichText::new(format!(
                            "PDF \u{00B7} {} {pages} {}",
                            state.tr("berisi", "with", "全"),
                            state.tr("halaman", "pages", "ページ")
                        ))
                        .size(fs(ui, 13.0))
                        .color(colors.on_surface),
                    );
                    ui.add_space(theme::SP_XS);
                    ui.label(
                        RichText::new(state.tr(
                            "Pratinjau halaman tidak dirender di aplikasi ini. \
                             Buka dengan aplikasi PDF di komputer.",
                            "Page rendering is not done in this app. Open it with a \
                             PDF app on your computer.",
                            "このアプリではページを描画しません。PC の PDF アプリで開いてください。",
                        ))
                        .size(fs(ui, 12.0))
                        .color(colors.muted),
                    );
                }
                Content::External(why) => {
                    ui.add_space(theme::SP_SM);
                    ui.label(
                        RichText::new(why)
                            .size(fs(ui, 12.5))
                            .color(colors.muted),
                    );
                }
                Content::Error(e) => {
                    ui.add_space(theme::SP_SM);
                    ui.label(RichText::new(e).size(fs(ui, 12.5)).color(colors.danger));
                }
            }

            ui.add_space(theme::SP_SM);
            ui.separator();
            ui.horizontal(|ui| {
                // "Buka" selalu ada: untuk gambar/PDF pengguna tetap ingin
                // melihatnya di aplikasi bawaannya, bukan di jendela ini.
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Buka", "Open", "開く"),
                    colors.accent,
                )
                .clicked()
                {
                    do_open = Some(rel.clone());
                }
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Ekspor", "Export", "エクスポート"),
                    colors.accent,
                )
                .clicked()
                {
                    do_export = true;
                }
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Hapus", "Delete", "削除"),
                    colors.danger,
                )
                .clicked()
                {
                    do_trash = true;
                }
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Tutup", "Close", "閉じる"),
                    colors.muted,
                )
                .clicked()
                {
                    open = false;
                }
            });
        });

    if let Some(rel) = do_open {
        let root = state.workspace();
        match file::resolve_at(&root, &rel) {
            Ok(p) => {
                if let Err(e) = platform::open_path(&p) {
                    state.notify(e);
                }
            }
            Err(e) => state.notify(e),
        }
    }
    if do_export {
        export_now(state, &rel);
    }
    if do_trash {
        state.files_ui.preview = None;
        if let Ok(info) = file::stat_at(&state.workspace().clone(), &rel) {
            state.files_ui.confirm_trash = Some(info);
        }
        return;
    }
    if !open || !win_open {
        state.files_ui.preview = None;
    }
}

/// Ekspor: dialog "Simpan sebagai" bila OS punya, kalau tidak ke
/// `<data>/exports` (dipakai di Linux/pengembangan, dan tetap memberi hasil).
fn export_now(state: &mut AppState, rel: &str) {
    let root = state.workspace();
    let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
    if let Some(dst) = platform::pick_save(&name) {
        let src = match file::resolve_at(&root, rel) {
            Ok(p) => p,
            Err(e) => return state.notify(e),
        };
        match std::fs::copy(&src, &dst) {
            Ok(_) => state.notify(format!(
                "{} {}",
                state.tr("Disimpan ke", "Saved to", "保存先:"),
                dst.display()
            )),
            Err(e) => state.notify(e.to_string()),
        }
        return;
    }
    let dir = state.paths.data_dir.join("exports");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return state.notify(e.to_string());
    }
    match crate::gui::fs_model::export_to(&root, rel, &dir) {
        Ok(p) => state.notify(format!(
            "{} {}",
            state.tr("Disimpan ke", "Saved to", "保存先:"),
            p.display()
        )),
        Err(e) => state.notify(e),
    }
}

fn kind_label_id(kind: &str) -> String {
    label(kind, "Folder", "Teks", "Gambar", "Video", "PDF", "Biner")
}
fn kind_label_en(kind: &str) -> String {
    label(kind, "Folder", "Text", "Image", "Video", "PDF", "Binary")
}
fn kind_label_ja(kind: &str) -> String {
    label(
        kind,
        "フォルダー",
        "テキスト",
        "画像",
        "動画",
        "PDF",
        "バイナリ",
    )
}

fn label(
    kind: &str,
    dir: &str,
    text: &str,
    image: &str,
    video: &str,
    pdf: &str,
    binary: &str,
) -> String {
    match kind {
        "dir" => dir,
        "text" => text,
        "image" => image,
        "video" => video,
        "pdf" => pdf,
        _ => binary,
    }
    .to_string()
}
