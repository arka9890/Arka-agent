//! Tab Provider — port `ProvidersScreen.kt`.
//!
//! Dua tampilan: **daftar** (nama, jenis, model, statistik token, status kunci)
//! dan **form** tambah/ubah. API key tidak pernah ditampilkan di daftar — layar
//! ini sering terlihat orang lain; di form pun awalnya tersembunyi.
//!
//! Pengamanan yang khusus Windows:
//! * kunci disimpan lewat DPAPI (`crate::dpapi`), bukan teks biasa;
//! * kalau kunci tersimpan tidak bisa dibuka (database dibawa dari mesin lain),
//!   form menandainya dan **tidak** menghapus kunci saat disimpan — lihat
//!   `provider::save` di core.

use eframe::egui::{self, RichText, Ui};

use crate::gui::jobs::{JobKind, NetJob};
use crate::gui::state::{AppState, ProviderForm};
use crate::gui::theme::{self, fs, ClayColors};
use crate::gui::widgets;

/// Aksi yang dijalankan setelah menggambar selesai (menghindari pinjam ganda).
enum Action {
    None,
    New,
    Edit(String),
    SetDefault(String),
    AskDelete(String, String),
    Delete(String),
    CancelDelete,
    Save,
    Cancel,
    Test,
    FetchModels,
    AddManualModel,
}

/// Ringkas angka besar: 1234567 → "1.2M". Sama seperti versi Android.
pub fn format_tokens(n: i64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1_000_000.0),
        n if n >= 1_000 => format!("{:.1}K", n as f64 / 1_000.0),
        n => n.to_string(),
    }
}

fn job_running(job: &Option<NetJob>) -> bool {
    job.as_ref().map(|j| j.running()).unwrap_or(false)
}

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    // Hasil tes koneksi / daftar model dari thread latar masuk ke form.
    state.poll_provider_job();

    let mut action = Action::None;
    let mut form = state.provider.form.clone();

    header(ui, state, colors, &mut action, form.is_some());
    ui.add_space(theme::SP_MD);

    if let Some(f) = form.as_mut() {
        form_ui(ui, state, colors, f, &mut action);
    } else {
        list_ui(ui, state, colors, &mut action);
    }

    // Tulis balik form (isi field yang baru diketik).
    state.provider.form = form;

    match action {
        Action::None => {}
        Action::New => state.open_provider_form(None),
        Action::Edit(id) => state.open_provider_form(Some(&id)),
        Action::SetDefault(id) => state.set_default_provider(&id),
        Action::AskDelete(id, name) => state.provider.confirm_delete = Some((id, name)),
        Action::CancelDelete => state.provider.confirm_delete = None,
        Action::Delete(id) => state.delete_provider(&id),
        Action::Save => {
            state.save_provider_form();
        }
        Action::Cancel => {
            state.provider.form = None;
            state.provider.job = None;
        }
        Action::Test => state.start_provider_job(JobKind::Test, ui.ctx()),
        Action::FetchModels => state.start_provider_job(JobKind::Models, ui.ctx()),
        Action::AddManualModel => {
            if let Some(f) = state.provider.form.as_mut() {
                let m = f.manual_model.trim().to_string();
                if !m.is_empty() {
                    if !f.models.iter().any(|x| x == &m) {
                        f.models.push(m.clone());
                    }
                    if f.model.trim().is_empty() {
                        f.model = m;
                    }
                    f.manual_model.clear();
                }
            }
        }
    }
}

// ── Kepala layar ───────────────────────────────────────────────────────────

fn header(
    ui: &mut Ui,
    state: &mut AppState,
    colors: &ClayColors,
    action: &mut Action,
    in_form: bool,
) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(
                RichText::new(state.tr("Provider AI", "AI Providers", "AIプロバイダー"))
                    .size(fs(ui, 22.0))
                    .strong()
                    .color(colors.on_surface),
            );
            ui.label(
                RichText::new(state.tr(
                    "API key disimpan lokal, terenkripsi Windows (DPAPI)",
                    "API keys stored locally, encrypted by Windows (DPAPI)",
                    "APIキーはローカルに保存され、Windows (DPAPI) で暗号化されます",
                ))
                .size(fs(ui, 12.0))
                .color(colors.muted),
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !in_form
                && widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("+ Tambah", "+ Add", "+ 追加"),
                    colors.accent,
                )
                .clicked()
            {
                *action = Action::New;
            }
            if let Some(job) = state.provider.job.as_ref() {
                if job.running() {
                    ui.label(
                        RichText::new(job.kind.label(state.lang.short()))
                            .size(fs(ui, 12.0))
                            .color(colors.muted),
                    );
                }
            }
        });
    });

    if let Some(err) = state.provider.error.clone() {
        ui.add_space(theme::SP_XS);
        ui.label(RichText::new(err).size(fs(ui, 12.0)).color(colors.danger));
    }
}

// ── Daftar provider ────────────────────────────────────────────────────────

fn list_ui(ui: &mut Ui, state: &AppState, colors: &ClayColors, action: &mut Action) {
    if let Some((id, name)) = state.provider.confirm_delete.clone() {
        widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
            ui.label(
                RichText::new(state.tr(
                    &format!("Hapus {name}?"),
                    &format!("Delete {name}?"),
                    &format!("{name} を削除しますか？"),
                ))
                .size(fs(ui, 15.0))
                .strong()
                .color(colors.on_surface),
            );
            ui.label(
                RichText::new(state.tr(
                    "API key provider ini ikut terhapus.",
                    "This provider's API key will also be deleted.",
                    "このプロバイダーのAPIキーも削除されます。",
                ))
                .size(fs(ui, 12.0))
                .color(colors.muted),
            );
            ui.add_space(theme::SP_XS);
            ui.horizontal(|ui| {
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Hapus", "Delete", "削除"),
                    colors.danger,
                )
                .clicked()
                {
                    *action = Action::Delete(id);
                }
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Batal", "Cancel", "キャンセル"),
                    colors.muted,
                )
                .clicked()
                {
                    *action = Action::CancelDelete;
                }
            });
        });
        ui.add_space(theme::SP_SM);
    }

    if state.provider.rows.is_empty() {
        widgets::clay_card(ui, colors, None, |ui| {
            ui.label(
                RichText::new(state.tr("Belum ada provider", "No providers yet", "プロバイダーがありません"))
                    .size(fs(ui, 16.0))
                    .strong()
                    .color(colors.on_surface),
            );
            ui.add_space(theme::SP_XS);
            ui.label(
                RichText::new(state.tr(
                    "Tekan Tambah untuk memakai OpenAI, Anthropic, atau endpoint yang kompatibel. Kunci API disimpan terenkripsi oleh Windows (DPAPI), bukan teks biasa.",
                    "Press Add to use OpenAI, Anthropic, or a compatible endpoint. API keys are encrypted by Windows (DPAPI), never plaintext.",
                    "「追加」を押して OpenAI、Anthropic、または互換エンドポイントを設定してください。APIキーは Windows (DPAPI) で暗号化されます。",
                ))
                .size(fs(ui, 13.0))
                .color(colors.muted),
            );
        });
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in &state.provider.rows {
                widgets::clay_card(ui, colors, None, |ui| {
                    let mut clicked_edit = false;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                if widgets::clay_pill_button(
                                    ui,
                                    colors,
                                    &row.name,
                                    if row.is_default { colors.success } else { colors.accent },
                                )
                                .clicked()
                                {
                                    clicked_edit = true;
                                }
                                if row.is_default {
                                    widgets::clay_pill(
                                        ui,
                                        colors,
                                        &state.tr("default", "default", "デフォルト"),
                                        colors.success,
                                    );
                                }
                            });
                            let model = if row.model.is_empty() {
                                state.tr("model belum dipilih", "no model selected", "モデル未選択")
                            } else {
                                row.model.clone()
                            };
                            ui.label(
                                RichText::new(format!("{} \u{00B7} {model}", row.kind))
                                    .size(fs(ui, 12.0))
                                    .color(colors.muted),
                            );
                            ui.label(
                                RichText::new(row.base_url.clone())
                                    .size(fs(ui, 12.0))
                                    .color(colors.muted),
                            );
                            if row.total_tokens > 0 || row.total_requests > 0 {
                                ui.label(
                                    RichText::new(format!(
                                        "\u{1F4CA} {} tokens \u{00B7} {} req",
                                        format_tokens(row.total_tokens),
                                        row.total_requests
                                    ))
                                    .size(fs(ui, 12.0))
                                    .color(colors.muted),
                                );
                            }
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if widgets::clay_pill_button(
                                ui,
                                colors,
                                &state.tr("Hapus", "Delete", "削除"),
                                colors.danger,
                            )
                            .clicked()
                            {
                                *action = Action::AskDelete(row.id.clone(), row.name.clone());
                            }
                            if widgets::clay_pill_button(
                                ui,
                                colors,
                                &state.tr("Ubah", "Edit", "編集"),
                                colors.muted,
                            )
                            .clicked()
                            {
                                clicked_edit = true;
                            }
                            if !row.is_default
                                && widgets::clay_pill_button(
                                    ui,
                                    colors,
                                    &state.tr("Jadikan default", "Set default", "デフォルトにする"),
                                    colors.muted,
                                )
                                .clicked()
                            {
                                *action = Action::SetDefault(row.id.clone());
                            }
                            if row.key_unreadable {
                                widgets::clay_pill(
                                    ui,
                                    colors,
                                    &state.tr("kunci tak terbaca", "key unreadable", "キー読み取り不可"),
                                    colors.danger,
                                );
                            } else if row.has_key {
                                widgets::clay_pill(
                                    ui,
                                    colors,
                                    &state.tr("key terisi", "key set", "キー設定済み"),
                                    colors.success,
                                );
                            } else {
                                widgets::clay_pill(
                                    ui,
                                    colors,
                                    &state.tr("key kosong", "no key", "キーなし"),
                                    colors.danger,
                                );
                            }
                        });
                    });
                    if clicked_edit {
                        *action = Action::Edit(row.id.clone());
                    }
                });
                ui.add_space(theme::SP_SM);
            }
        });
}

// ── Form tambah/ubah ───────────────────────────────────────────────────────

fn form_ui(
    ui: &mut Ui,
    state: &AppState,
    colors: &ClayColors,
    form: &mut ProviderForm,
    action: &mut Action,
) {
    widgets::clay_card(ui, colors, None, |ui| {
        ui.label(
            RichText::new(if form.id.is_empty() {
                state.tr("Tambah provider", "Add provider", "プロバイダーを追加")
            } else {
                state.tr("Ubah provider", "Edit provider", "プロバイダーを編集")
            })
            .size(fs(ui, 18.0))
            .strong()
            .color(colors.on_surface),
        );
        ui.add_space(theme::SP_SM);

        field(ui, colors, state.tr("Nama", "Name", "名前").as_str(), &mut form.name, false, None);
        ui.add_space(theme::SP_XS);

        ui.horizontal(|ui| {
            for (id, label) in [("openai", "OpenAI-compatible"), ("anthropic", "Anthropic")] {
                let on = form.kind == id;
                if widgets::clay_pill_button(
                    ui,
                    colors,
                    label,
                    if on { colors.accent } else { colors.muted },
                )
                .clicked()
                {
                    form.kind = id.to_string();
                }
            }
        });
        ui.add_space(theme::SP_XS);

        let hint_url = form.resolved_url();
        field(
            ui,
            colors,
            state.tr("Base URL", "Base URL", "ベースURL").as_str(),
            &mut form.base_url,
            false,
            Some(hint_url),
        );
        ui.add_space(theme::SP_XS);

        // API key: tersembunyi sampai tombol mata ditekan.
        ui.horizontal(|ui| {
            let teks = if form.reveal_key {
                state.tr("Sembunyikan", "Hide", "隠す")
            } else {
                state.tr("Tampilkan", "Show", "表示")
            };
            if widgets::clay_pill_button(ui, colors, &teks, colors.muted).clicked() {
                form.reveal_key = !form.reveal_key;
            }
            field(
                ui,
                colors,
                state.tr("API key", "API key", "APIキー").as_str(),
                &mut form.api_key,
                !form.reveal_key,
                None,
            );
        });

        if form.key_unreadable {
            ui.label(
                RichText::new(state.tr(
                    "Kunci tersimpan tidak bisa dibuka di mesin ini (DPAPI milik akun lain). Biarkan kosong: kunci lama dipertahankan saat disimpan.",
                    "The stored key cannot be opened on this machine (another account's DPAPI). Leave it empty: the old key is kept when saving.",
                    "保存されたキーはこのマシンでは開けません（別アカウントの DPAPI）。空欄のままにすると保存時に古いキーが保持されます。",
                ))
                .size(fs(ui, 12.0))
                .color(colors.danger),
            );
        }
        ui.add_space(theme::SP_XS);

        field(ui, colors, state.tr("Model utama", "Main model", "メインモデル").as_str(), &mut form.model, false, None);
        ui.add_space(theme::SP_XS);

        ui.horizontal(|ui| {
            let on = form.is_default;
            let teks = if on {
                state.tr("default \u{2713}", "default \u{2713}", "デフォルト \u{2713}")
            } else {
                state.tr("jadikan default", "set as default", "デフォルトにする")
            };
            if widgets::clay_pill_button(
                ui,
                colors,
                &teks,
                if on { colors.success } else { colors.muted },
            )
            .clicked()
            {
                form.is_default = !form.is_default;
            }
            let running = job_running(&state.provider.job);
            if !running
                && widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Tes koneksi", "Test connection", "接続テスト"),
                    colors.accent,
                )
                .clicked()
            {
                *action = Action::Test;
            }
            if !running
                && widgets::clay_pill_button(
                    ui,
                    colors,
                    &state.tr("Cek model dari server", "Refresh model list", "モデル一覧を再取得"),
                    colors.muted,
                )
                .clicked()
            {
                *action = Action::FetchModels;
            }
        });

        if let Some(r) = form.test_result.clone() {
            ui.label(RichText::new(r).size(fs(ui, 12.0)).color(colors.muted));
        }

        widgets::section(
            ui,
            colors,
            &state.tr("Model tersedia (boleh pilih lebih dari satu)", "Available models (multiple choice)", "利用可能なモデル（複数選択可）"),
        );

        let models = form.all_models();
        egui::ScrollArea::vertical()
            .id_salt("model-chips")
            .max_height(160.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for m in &models {
                        let on = form.models.iter().any(|x| x == m);
                        if widgets::clay_pill_button(
                            ui,
                            colors,
                            m,
                            if on { colors.accent } else { colors.muted },
                        )
                        .clicked()
                        {
                            if on {
                                form.models.retain(|x| x != m);
                            } else {
                                form.models.push(m.clone());
                            }
                            if form.model.trim().is_empty() {
                                form.model = m.clone();
                            }
                        }
                    }
                    if models.is_empty() {
                        ui.label(
                            RichText::new(state.tr(
                                "Belum ada model. Tekan \u{201C}Cek model dari server\u{201D} atau tambah manual.",
                                "No models yet. Press \u{201C}Refresh model list\u{201D} or add one manually.",
                                "モデルがありません。「モデル一覧を再取得」を押すか手動で追加してください。",
                            ))
                            .size(fs(ui, 12.0))
                            .color(colors.muted),
                        );
                    }
                });
            });

        ui.add_space(theme::SP_XS);
        ui.horizontal(|ui| {
            field(
                ui,
                colors,
                state.tr("Tambah model manual", "Add a model manually", "モデルを手動で追加").as_str(),
                &mut form.manual_model,
                false,
                None,
            );
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Tambah", "Add", "追加"),
                colors.accent,
            )
            .clicked()
            {
                *action = Action::AddManualModel;
            }
        });

        if let Some(err) = form.error.clone() {
            ui.add_space(theme::SP_XS);
            ui.label(RichText::new(err).size(fs(ui, 12.0)).color(colors.danger));
        }

        ui.add_space(theme::SP_SM);
        ui.horizontal(|ui| {
            if widgets::clay_button(
                ui,
                colors,
                &state.tr("Simpan", "Save", "保存"),
                !form.name.trim().is_empty(),
            )
            .clicked()
            {
                *action = Action::Save;
            }
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Batal", "Cancel", "キャンセル"),
                colors.muted,
            )
            .clicked()
            {
                *action = Action::Cancel;
            }
        });
    });
}

/// Satu baris input: label kecil di atas, kotak isian selebar kartu.
fn field(
    ui: &mut Ui,
    colors: &ClayColors,
    label: &str,
    value: &mut String,
    password: bool,
    hint: Option<String>,
) {
    ui.vertical(|ui| {
        ui.label(RichText::new(label).size(fs(ui, 11.0)).color(colors.muted));
        let mut edit = egui::TextEdit::singleline(value)
            .desired_width(f32::INFINITY)
            .password(password);
        if let Some(h) = hint {
            edit = edit.hint_text(h);
        }
        ui.add(edit);
    });
}

// ── Test ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_tokens_ringkas_angka_besar() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(1_234), "1.2K");
        assert_eq!(format_tokens(999_999), "1000.0K");
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(12_345_678), "12.3M");
    }
}
