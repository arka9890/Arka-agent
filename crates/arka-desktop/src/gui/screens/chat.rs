//! Tab Chat: riwayat sungguhan dari SQLite, streaming, dan kotak pesan.
//!
//! Alur satu pesan (padanan `MainViewModel.send`):
//! 1. pesan pengguna **disimpan dulu** ke database, baru dikirim ke model;
//! 2. riwayat dibaca ulang dari database — bukan dari state UI, supaya sesi
//!    yang baru dipindah tidak mengirim percakapan yang salah;
//! 3. `Engine::start` menjalankan agent di thread latar, UI tetap 60 fps.

use eframe::egui::{self, RichText, Ui};

use arka_core::agent::ChatRequest;

use crate::gui::bubble;
use crate::gui::state::{AppState, Attachment, MsgView};
use crate::gui::theme::{self, fs, ClayColors};
use crate::gui::widgets;

/// Aksi yang dikumpulkan selama menggambar, lalu dijalankan setelah closure
/// selesai — supaya tidak meminjam `state` dua kali.
enum Action {
    None,
    Send,
    Stop,
    NewSession,
    Attach,
    SetModel(String),
    ToggleAutoScroll,
    RemoveAttachment(usize),
}

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    let mut action = Action::None;

    header(ui, state, colors, &mut action);
    ui.add_space(theme::SP_MD);

    messages(ui, state, colors);

    ui.add_space(theme::SP_SM);
    input_bar(ui, state, colors, &mut action);

    match action {
        Action::None => {}
        Action::Send => send(state, ui.ctx()),
        Action::Stop => {
            if let Some(sid) = state.active_session.clone() {
                state.chat.engine.cancel(&sid);
                state.notify(state.tr("Dihentikan.", "Stopped.", "停止しました。"));
            }
        }
        Action::NewSession => state.create_session(),
        Action::Attach => {
            state.chat.pick_dir = state.workspace();
            state.modal = crate::gui::state::Modal::PickFile;
        }
        Action::SetModel(m) => {
            state.chat.model = m.clone();
            if let Some(sid) = state.active_session.clone() {
                let pid = state.chat.provider_id.clone();
                let _ = arka_core::session::set_model(&sid, &pid, &m);
            }
        }
        Action::ToggleAutoScroll => state.chat.auto_scroll = !state.chat.auto_scroll,
        Action::RemoveAttachment(i) => {
            if i < state.chat.attachments.len() {
                state.chat.attachments.remove(i);
            }
        }
    }
}

fn running(state: &AppState) -> bool {
    state
        .active_session
        .as_deref()
        .map(|sid| state.chat.engine.is_running(sid))
        .unwrap_or(false)
}

fn header(ui: &mut Ui, state: &mut AppState, colors: &ClayColors, action: &mut Action) {
    ui.horizontal(|ui| {
        let title = state
            .active_title()
            .unwrap_or_else(|| state.tr("Chat", "Chat", "チャット"));
        ui.label(RichText::new(title).size(fs(ui, 22.0)).strong());

        if running(state) {
            ui.add_space(theme::SP_SM);
            widgets::clay_pill(
                ui,
                colors,
                &state.tr("menjawab…", "answering…", "応答中…"),
                colors.accent,
            );
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("+ Sesi baru", "+ New session", "+ 新しいセッション"),
                colors.accent,
            )
            .clicked()
            {
                *action = Action::NewSession;
            }
            model_picker(ui, state, colors, action);
        });
    });
}

/// Dropdown model: cari, pilih dari daftar provider, atau ketik manual.
fn model_picker(ui: &mut Ui, state: &mut AppState, colors: &ClayColors, action: &mut Action) {
    let current = if state.chat.model.is_empty() {
        state.tr("model", "model", "モデル")
    } else {
        state.chat.model.clone()
    };

    let mut models: Vec<String> = state
        .chat
        .providers
        .iter()
        .filter(|p| p.id == state.chat.provider_id)
        .flat_map(|p| {
            let mut v = p.models.clone();
            if !p.model.is_empty() && !v.contains(&p.model) {
                v.push(p.model.clone());
            }
            v
        })
        .collect();
    models.dedup();

    egui::ComboBox::from_id_salt("model-picker")
        .width(190.0 * theme::scale_of(ui.ctx()))
        .selected_text(RichText::new(current).size(fs(ui, 13.0)))
        .show_ui(ui, |ui| {
            let cari = state.tr("Cari model…", "Search model…", "モデルを検索…");
            ui.add(
                egui::TextEdit::singleline(&mut state.chat.model_query)
                    .hint_text(cari)
                    .desired_width(f32::INFINITY),
            );
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    let q = state.chat.model_query.to_lowercase();
                    for m in &models {
                        if !q.is_empty() && !m.to_lowercase().contains(&q) {
                            continue;
                        }
                        if ui
                            .selectable_label(state.chat.model == *m, RichText::new(m).size(fs(ui, 13.0)))
                            .clicked()
                        {
                            *action = Action::SetModel(m.clone());
                        }
                    }
                    if models.is_empty() {
                        ui.label(
                            RichText::new(state.tr(
                                "Provider ini belum punya daftar model.",
                                "This provider has no models yet.",
                                "このプロバイダーにはモデルがありません。",
                            ))
                            .size(fs(ui, 12.0))
                            .color(colors.muted),
                        );
                    }
                });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new(state.tr("Manual", "Manual", "手動")).size(fs(ui, 12.0)));
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut state.chat.model)
                            .hint_text("model-id")
                            .desired_width(140.0),
                    )
                    .lost_focus()
                {
                    // Nilai sudah tertulis langsung ke state; sesi disimpan
                    // saat pengguna menutup dropdown.
                }
            });
        });
}

fn messages(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    let footer_h = 120.0 * theme::scale_of(ui.ctx());
    let avail = (ui.available_height() - footer_h).max(140.0);
    let stick = state.chat.auto_scroll;

    egui::ScrollArea::vertical()
        .max_height(avail)
        .auto_shrink([false, false])
        .stick_to_bottom(stick)
        .show(ui, |ui| {
            if state.chat.messages.is_empty() && !running(state) {
                empty_state(ui, state, colors);
                return;
            }

            // Salinan ringan supaya `state` tidak dipinjam dua kali di closure.
            let msgs = state.chat.messages.clone();
            for m in &msgs {
                bubble::show(ui, colors, m);
            }

            if running(state) {
                let text = state.chat.streaming.clone();
                bubble::streaming(ui, colors, &text);
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
            }
        });

    if let Some(e) = &state.db_error {
        ui.label(RichText::new(e).size(fs(ui, 11.0)).color(colors.danger));
    }
}

fn input_bar(ui: &mut Ui, state: &mut AppState, colors: &ClayColors, action: &mut Action) {
    // Lampiran yang menempel.
    if !state.chat.attachments.is_empty() {
        let mut remove: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, a) in state.chat.attachments.iter().enumerate() {
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    &format!("\u{1F4C4} {} \u{00D7}", a.name),
                    colors.accent_soft,
                )
                .clicked()
                {
                    remove = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            *action = Action::RemoveAttachment(i);
        }
        ui.add_space(theme::SP_XS);
    }

    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        ui.horizontal(|ui| {
            let busy = running(state);
            let button_w = 96.0 * theme::scale_of(ui.ctx());
            let attach_w = 40.0 * theme::scale_of(ui.ctx());
            let w = (ui.available_width() - button_w - attach_w - theme::SP_MD * 2.0).max(120.0);

            if ui
                .add(
                    egui::Button::new(RichText::new("\u{1F4CE}").size(fs(ui, 15.0)))
                        .fill(colors.surface)
                        .min_size(egui::Vec2::new(attach_w, 36.0 * theme::scale_of(ui.ctx()))),
                )
                .on_hover_text(state.tr("Lampirkan berkas", "Attach a file", "ファイルを添付"))
                .clicked()
            {
                *action = Action::Attach;
            }

            let hint = state.tr(
                "Tulis pesan… (Enter kirim, Shift+Enter baris baru)",
                "Type a message… (Enter sends, Shift+Enter adds a line)",
                "メッセージを入力… (Enter で送信、Shift+Enter で改行)",
            );
            let resp = ui.add_sized(
                [w, 40.0 * theme::scale_of(ui.ctx())],
                egui::TextEdit::multiline(&mut state.draft)
                    .hint_text(hint)
                    .desired_rows(1)
                    .desired_width(w),
            );

            if state.chat.focus_input {
                resp.request_focus();
                state.chat.focus_input = false;
            }

            // Enter mengirim, Shift+Enter menambah baris. `consume_key` dipakai
            // supaya newline-nya tidak ikut masuk ke teks.
            let enter = ui.input(|i| {
                i.key_pressed(egui::Key::Enter) && !i.modifiers.shift && !i.modifiers.command
            });
            if resp.has_focus() && enter && !busy {
                ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                });
                *action = Action::Send;
            }

            let label = if busy {
                state.tr("Stop", "Stop", "停止")
            } else {
                state.tr("Kirim", "Send", "送信")
            };
            if widgets::clay_button(ui, colors, &label, true).clicked() {
                *action = if busy { Action::Stop } else { Action::Send };
            }
        });

        ui.add_space(theme::SP_XS);
        ui.horizontal(|ui| {
            let mut auto = state.chat.auto_scroll;
            if ui
                .checkbox(&mut auto, RichText::new(state.tr("Ikuti bawah", "Auto-scroll", "自動スクロール")).size(fs(ui, 11.5)))
                .changed()
            {
                *action = Action::ToggleAutoScroll;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if !state.chat.provider_id.is_empty() {
                    let name = state
                        .chat
                        .providers
                        .iter()
                        .find(|p| p.id == state.chat.provider_id)
                        .map(|p| p.name.clone())
                        .unwrap_or_default();
                    ui.label(RichText::new(name).size(fs(ui, 11.0)).color(colors.muted));
                }
            });
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

        let samples = [
            state.tr("Cek versi Python", "Check Python version", "Python のバージョンを確認"),
            state.tr("Daftar isi folder kerja", "List workspace files", "ワークスペースの一覧"),
            state.tr("Lihat penggunaan disk", "Show disk usage", "ディスク使用量を表示"),
        ];
        ui.horizontal_wrapped(|ui| {
            for s in samples {
                if widgets::clay_pill_button(ui, colors, &s, colors.accent).clicked() {
                    state.draft = s;
                    state.chat.focus_input = true;
                }
            }
        });
    });
}

// ── Pengiriman ──────────────────────────────────────────────────────────────

/// Kirim isi kotak pesan. Menyimpan pesan pengguna dulu, baru memanggil model.
pub fn send(state: &mut AppState, ctx: &egui::Context) {
    let Some(sid) = state.active_session.clone() else {
        state.notify(state.tr(
            "Buat sesi baru dulu.",
            "Create a session first.",
            "先にセッションを作成してください。",
        ));
        return;
    };
    if state.chat.engine.is_running(&sid) {
        return;
    }

    let body = state.draft.trim().to_string();
    let attachments = std::mem::take(&mut state.chat.attachments);
    if body.is_empty() && attachments.is_empty() {
        return;
    }

    let att_json =
        serde_json::Value::Array(attachments.iter().map(Attachment::to_json).collect()).to_string();

    let user_msg = match arka_core::session::add_message(&sid, "user", &body, &att_json) {
        Ok(m) => m,
        Err(e) => {
            state.chat.attachments = attachments;
            state.notify(e);
            return;
        }
    };
    if state.active_session.as_deref() == Some(sid.as_str()) {
        state.chat.messages.push(MsgView::from_core(user_msg));
    }
    state.draft.clear();
    let _ = arka_core::session::autotitle(&sid, &body);
    state.refresh_sessions();

    // Riwayat dari database, bukan dari `state.chat.messages`: state UI bisa
    // masih berisi pesan sesi lain kalau pengguna baru pindah lalu langsung
    // mengirim.
    let mut history = arka_core::session::messages(&sid).unwrap_or_default();

    // Konteks berkas teks: model "melihat" isinya tanpa tool tambahan.
    let mut file_ctx = String::new();
    for a in &attachments {
        if a.kind != "text" {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&a.path) {
            let cuplikan: String = content.chars().take(20_000).collect();
            file_ctx.push_str(&format!("\n\nIsi file `{}`:\n```\n{}\n```", a.name, cuplikan));
        }
    }
    if !file_ctx.is_empty() {
        if let Some(last) = history.last_mut() {
            if last.role == "user" {
                last.content.push_str(&file_ctx);
            }
        }
    }

    let messages_json = serde_json::to_string(&history).unwrap_or_else(|_| "[]".to_string());

    // Model yang dipakai dicatat ke sesi supaya riwayat menampilkan model yang
    // benar saat dibuka lagi.
    let provider_id = state.chat.provider_id.clone();
    let model = state.chat.model.clone();
    let _ = arka_core::session::set_model(&sid, &provider_id, &model);

    state.chat.streaming.clear();
    state.chat.unread.remove(&sid);

    let req = ChatRequest {
        session_id: sid,
        provider_id,
        model,
        messages_json,
    };
    state.chat.engine.start(req, ctx.clone());
}

// ── Pemilih berkas ──────────────────────────────────────────────────────────

/// Tebak jenis berkas dari ekstensi (sama seperti `ui::file_preview` di core).
fn kind_of(path: &std::path::Path) -> &'static str {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp" | "ico" => "image",
        "mp4" | "webm" | "mkv" | "3gp" | "avi" | "mov" => "video",
        "mp3" | "wav" | "ogg" | "m4a" | "aac" | "flac" => "audio",
        "txt" | "md" | "rs" | "py" | "js" | "ts" | "json" | "toml" | "yaml" | "yml" | "kt"
        | "java" | "c" | "h" | "cpp" | "cs" | "go" | "sh" | "ps1" | "css" | "html" => "text",
        _ => "file",
    }
}

/// Bentuk [`Attachment`] dari path di disk — dipakai pemilih berkas.
pub fn attachment_from(path: &std::path::Path) -> Attachment {
    Attachment {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string()),
        kind: kind_of(path).to_string(),
        path: path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jenis_berkas_ditebak_dari_ekstensi() {
        assert_eq!(kind_of(std::path::Path::new("a.PNG")), "image");
        assert_eq!(kind_of(std::path::Path::new("a.mp4")), "video");
        assert_eq!(kind_of(std::path::Path::new("a.rs")), "text");
        assert_eq!(kind_of(std::path::Path::new("a.bin")), "file");
        assert_eq!(kind_of(std::path::Path::new("tanpa-ekstensi")), "file");
    }

    #[test]
    fn lampiran_json_sama_bentuknya_dengan_android() {
        let a = Attachment {
            path: "C:\\x\\a.txt".into(),
            name: "a.txt".into(),
            kind: "text".into(),
        };
        let v = a.to_json();
        assert_eq!(v["path"], "C:\\x\\a.txt");
        assert_eq!(v["kind"], "text");
        assert_eq!(v["name"], "a.txt");
    }
}
