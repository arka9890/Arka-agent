//! Tab Setelan — M8.
//!
//! Tiga kelompok setelan sengaja dipisah sumbernya:
//!   * **Tampilan** (tema/font/bahasa) → `Persisted` (eframe Storage), karena
//!     dipakai sebelum database sempat dibuka;
//!   * **Agent & pemberitahuan** → [`Prefs`] di tabel `settings` (`arka_core`),
//!     supaya angka yang dipakai agent loop dan yang terlihat di UI sama;
//!   * **Lingkungan** → `AppPaths` + [`crate::sysinfo`] (dibaca di thread latar).
//!
//! Semua helper menerima `Lang` (Copy), bukan `&mut AppState`: kalau helper
//! memegang seluruh state sementara pemanggil juga meminjam `&mut state.prefs`,
//! Rust menolaknya. Memisahkan borrow per field juga membuat jelas setelan
//! mana yang sedang diubah.
//!
//! Semua teks wajib tiga bahasa lewat `tr`, termasuk pesan validasi.

use eframe::egui::{self, RichText, Ui};

use crate::gui::i18n::{tr, Lang};
use crate::gui::prefs::{NumError, NumSpec, Prefs};
use crate::gui::state::{AppState, Modal};
use crate::gui::theme::{self, fs, ClayColors, FontScale, ThemeMode};
use crate::gui::widgets;
use crate::sysinfo::{self, SysInfo};

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    let lang = state.lang;
    ui.label(
        RichText::new(tr(lang, "Setelan", "Settings", "設定"))
            .size(fs(ui, 22.0))
            .strong(),
    );
    ui.add_space(theme::SP_XS);
    ui.label(
        RichText::new(tr(
            lang,
            "Perubahan tersimpan saat Anda menekan Simpan.",
            "Changes are saved when you press Save.",
            "「保存」を押すと設定が保存されます。",
        ))
        .size(fs(ui, 12.0))
        .color(colors.muted),
    );
    ui.add_space(theme::SP_MD);

    state.ensure_sysinfo(ui.ctx());
    tampilan(ui, state, lang, colors);
    pemberitahuan(ui, state, lang, colors);
    agent(ui, state, lang, colors);
    workspace(ui, state, lang, colors);
    lingkungan(ui, state, lang, colors);
    tentang(ui, state, lang, colors);

    ui.add_space(theme::SP_XL);
}

// ── Tampilan ──────────────────────────────────────────────────────────────

fn tampilan(ui: &mut Ui, state: &mut AppState, lang: Lang, colors: &ClayColors) {
    widgets::section(ui, colors, &tr(lang, "Tampilan", "Appearance", "外観"));
    widgets::clay_card(ui, colors, None, |ui| {
        label(ui, lang, "Tema", "Theme", "テーマ");
        choice_row(
            ui,
            lang,
            colors,
            ThemeMode::ALL,
            |m, l| match m {
                ThemeMode::System => tr(l, "Ikuti sistem", "Follow system", "システムに従う"),
                ThemeMode::Dark => tr(l, "Gelap", "Dark", "ダーク"),
                ThemeMode::Light => tr(l, "Terang", "Light", "ライト"),
            },
            &mut state.theme,
            |_, _| None,
            // Tema ikut disimpan ke database (kunci `theme`) supaya versi
            // Android membaca pilihan yang sama; `apply_prefs` menuliskannya.
            |m, _| state.prefs.theme = theme_key(m).to_string(),
        );

        ui.add_space(theme::SP_MD);
        label(ui, lang, "Ukuran teks", "Text size", "文字サイズ");
        choice_row(
            ui,
            lang,
            colors,
            FontScale::ALL,
            |m, l| match m {
                FontScale::Small => tr(l, "Kecil", "Small", "小"),
                FontScale::Medium => tr(l, "Sedang", "Medium", "中"),
                FontScale::Large => tr(l, "Besar", "Large", "大"),
            },
            &mut state.font_scale,
            |_, _| None,
            |m, _| state.prefs.font_size = font_key(m).to_string(),
        );
        ui.label(
            RichText::new(format!("{}x", state.font_scale.factor()))
                .size(fs(ui, 12.0))
                .color(colors.muted),
        );

        ui.add_space(theme::SP_MD);
        label(ui, lang, "Bahasa", "Language", "言語");
        choice_row(
            ui,
            lang,
            colors,
            Lang::ALL,
            |l, _| l.native().to_string(),
            &mut state.lang,
            |_, _| None,
            |l, _| state.prefs.language = l.key().to_string(),
        );
    });
}

/// Kunci database untuk satu tema (dipakai bersama `apply_prefs`).
pub fn theme_key(m: ThemeMode) -> &'static str {
    match m {
        ThemeMode::System => "system",
        ThemeMode::Dark => "dark",
        ThemeMode::Light => "light",
    }
}

pub fn font_key(f: FontScale) -> &'static str {
    match f {
        FontScale::Small => "small",
        FontScale::Medium => "medium",
        FontScale::Large => "large",
    }
}

fn label(ui: &mut Ui, lang: Lang, id: &str, en: &str, ja: &str) {
    ui.label(
        RichText::new(tr(lang, id, en, ja))
            .size(fs(ui, 14.0))
            .strong(),
    );
    ui.add_space(theme::SP_XS);
}

/// Satu baris tombol pilihan (eksklusif) + pesan validasi di bawahnya.
///
/// `valid` menerima nilai **calon** pilihan, bukan nilai yang sedang aktif:
/// tombol yang diklik harus langsung memberi tahu kalau pilihannya tidak
/// bisa dipakai (mis. folder yang tidak bisa dibuat), tanpa menunggu Simpan.
#[allow(clippy::too_many_arguments)] // 6 parameter teks/ Borrow dipisah; lihat toggle.
fn choice_row<T: Copy + PartialEq>(
    ui: &mut Ui,
    lang: Lang,
    colors: &ClayColors,
    all: impl IntoIterator<Item = T>,
    text: impl Fn(T, Lang) -> String,
    current: &mut T,
    valid: impl Fn(T, Lang) -> Option<String>,
    mut on_pick: impl FnMut(T, Lang),
) {
    let mut error: Option<String> = None;
    ui.horizontal(|ui| {
        for item in all {
            let active = item == *current;
            let label = text(item, lang);
            let btn = egui::Button::new(RichText::new(label).size(fs(ui, 13.0)).color(if active {
                colors.accent
            } else {
                colors.on_surface
            }))
            .fill(if active {
                colors.accent_soft
            } else {
                colors.surface_raised
            });
            if ui.add(btn).clicked() {
                if let Some(e) = valid(item, lang) {
                    error = Some(e);
                } else {
                    *current = item;
                    on_pick(item, lang);
                }
            }
        }
    });
    if let Some(e) = error {
        ui.add_space(theme::SP_XS);
        ui.label(RichText::new(e).size(fs(ui, 11.5)).color(colors.danger));
    }
}

// ── Pemberitahuan & latar belakang ─────────────────────────────────────────

fn pemberitahuan(ui: &mut Ui, state: &mut AppState, lang: Lang, colors: &ClayColors) {
    ui.add_space(theme::SP_MD);
    widgets::section(
        ui,
        colors,
        &tr(lang, "Pemberitahuan", "Notifications", "通知"),
    );
    widgets::clay_card(ui, colors, None, |ui| {
        toggle(
            ui,
            lang,
            colors,
            &mut state.prefs.notifications,
            "Notifikasi saat balasan selesai",
            "Notify when a reply finishes",
            "応答完了時に通知",
            "Muncul hanya bila jendela tidak sedang fokus.",
            "Only shown when the window is not focused.",
            "ウィンドウにフォーカスがない時のみ表示します。",
        );
        toggle(
            ui,
            lang,
            colors,
            &mut state.prefs.background_run,
            "Jalan di latar belakang",
            "Run in background",
            "バックグラウンドで実行",
            "Tombol X menyembunyikan jendela ke tray, aplikasi tetap hidup.",
            "The X button hides the window to the tray; the app keeps running.",
            "× ボタンでトレイに隠し、アプリは実行を続けます。",
        );
        toggle(
            ui,
            lang,
            colors,
            &mut state.chat.auto_scroll,
            "Gulir otomatis ke pesan terbaru",
            "Auto-scroll to the latest message",
            "最新メッセージへ自動スクロール",
            "",
            "",
            "",
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn toggle(
    ui: &mut Ui,
    lang: Lang,
    colors: &ClayColors,
    value: &mut bool,
    id: &str,
    en: &str,
    ja: &str,
    hint_id: &str,
    hint_en: &str,
    hint_ja: &str,
) {
    let label = tr(lang, id, en, ja);
    ui.checkbox(
        value,
        RichText::new(label)
            .size(fs(ui, 14.0))
            .color(colors.on_surface),
    );
    if !hint_id.is_empty() {
        ui.add_space(theme::SP_XS);
        ui.label(
            RichText::new(tr(lang, hint_id, hint_en, hint_ja))
                .size(fs(ui, 11.5))
                .color(colors.muted),
        );
    }
    ui.add_space(theme::SP_SM);
}

// ── Angka agent & system prompt ────────────────────────────────────────────

fn agent(ui: &mut Ui, state: &mut AppState, lang: Lang, colors: &ClayColors) {
    ui.add_space(theme::SP_MD);
    widgets::section(ui, colors, &tr(lang, "Agent", "Agent", "エージェント"));
    // Snapshot murah (clone string), bukan `Prefs::load()` yang akan melakukan
    // belasan query database **setiap frame** hanya untuk label "belum simpan".
    let saved: Prefs = state.prefs.clone();
    let dirty = saved != state.prefs;

    widgets::clay_card(ui, colors, None, |ui| {
        num(
            ui,
            lang,
            colors,
            "Batas putaran tool",
            "Tool round limit",
            "ツール実行回数の上限",
            &mut state.prefs.max_tool_rounds,
            NumSpec::ROUNDS,
        );
        num(
            ui,
            lang,
            colors,
            "Batas konteks (token)",
            "Context budget (tokens)",
            "コンテキスト上限 (トークン)",
            &mut state.prefs.context_budget,
            NumSpec::CONTEXT,
        );
        num(
            ui,
            lang,
            colors,
            "Batas output tool (karakter)",
            "Tool output limit (characters)",
            "ツール出力上限 (文字)",
            &mut state.prefs.tool_output_chars,
            NumSpec::OUTPUT,
        );
        num(
            ui,
            lang,
            colors,
            "Batas waktu balasan (detik)",
            "Reply timeout (seconds)",
            "応答タイムアウト (秒)",
            &mut state.prefs.chat_timeout_secs,
            NumSpec::TIMEOUT,
        );
        num(
            ui,
            lang,
            colors,
            "Batas waktu perintah (detik)",
            "Command timeout (seconds)",
            "コマンドのタイムアウト (秒)",
            &mut state.prefs.tool_timeout_secs,
            NumSpec::TOOL_TIMEOUT,
        );

        ui.add_space(theme::SP_MD);
        toggle(
            ui,
            lang,
            colors,
            &mut state.prefs.auto_summary,
            "Ringkas riwayat otomatis",
            "Auto-summarize history",
            "履歴を自動要約",
            "",
            "",
            "",
        );

        ui.add_space(theme::SP_SM);
        label(
            ui,
            lang,
            "System prompt",
            "System prompt",
            "システムプロンプト",
        );
        ui.label(
            RichText::new(tr(
                lang,
                "Kosong = pakai prompt bawaan platform.",
                "Leave empty to use the platform default prompt.",
                "空欄の場合は既定のプロンプトを使用します。",
            ))
            .size(fs(ui, 11.5))
            .color(colors.muted),
        );
        ui.add_space(theme::SP_XS);
        let hint = tr(
            lang,
            " Instruksi khusus untuk agent…",
            " Custom agent instructions…",
            " エージェントへの指示…",
        );
        ui.add_sized(
            [ui.available_width(), 110.0 * theme::scale_of(ui.ctx())],
            egui::TextEdit::multiline(&mut state.prefs.system_prompt)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace),
        );

        ui.add_space(theme::SP_MD);
        ui.horizontal(|ui| {
            if widgets::clay_button(ui, colors, &tr(lang, "Simpan", "Save", "保存"), true).clicked()
            {
                simpan_sekarang(state);
            }
            if widgets::clay_button(
                ui,
                colors,
                &tr(
                    lang,
                    "Kembalikan ke bawaan",
                    "Reset to defaults",
                    "既定に戻す",
                ),
                true,
            )
            .clicked()
            {
                state.prefs.reset();
            }
            if dirty {
                ui.label(
                    RichText::new(tr(
                        lang,
                        "Ada perubahan belum disimpan",
                        "Unsaved changes",
                        "未保存の変更があります",
                    ))
                    .size(fs(ui, 11.5))
                    .color(colors.danger),
                );
            }
        });
    });
}

#[allow(clippy::too_many_arguments)] // id/en/ja dipisah supaya wajib tiga bahasa.
fn num(
    ui: &mut Ui,
    lang: Lang,
    colors: &ClayColors,
    id: &str,
    en: &str,
    ja: &str,
    raw: &mut String,
    spec: NumSpec,
) {
    let parsed = spec.parse(raw);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr(lang, id, en, ja))
                .size(fs(ui, 13.5))
                .color(colors.on_surface),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let w = 120.0 * theme::scale_of(ui.ctx());
            ui.add_sized(
                [w, 28.0 * theme::scale_of(ui.ctx())],
                egui::TextEdit::singleline(raw),
            );
        });
    });
    let text = match parsed {
        Ok(n) => tr(
            lang,
            &format!("Rentang {}–{}. Dipakai: {n}.", spec.min, spec.max),
            &format!("Range {}–{}. Effective: {n}.", spec.min, spec.max),
            &format!("範囲 {}～{}。有効値: {n}。", spec.min, spec.max),
        ),
        Err(e) => num_error(lang, spec, e),
    };
    ui.label(
        RichText::new(text)
            .size(fs(ui, 11.0))
            .color(if parsed.is_err() {
                colors.danger
            } else {
                colors.muted
            }),
    );
    ui.add_space(theme::SP_XS);
}

fn num_error(lang: Lang, spec: NumSpec, e: NumError) -> String {
    let (id, en, ja) = match e {
        NumError::Empty => (
            format!("Kosong — dipakai bawaan {}.", spec.default),
            format!("Empty — using default {}.", spec.default),
            format!("空欄 — 既定値 {} を使用。", spec.default),
        ),
        NumError::NotNumber => (
            format!("Harus angka {}–{}.", spec.min, spec.max),
            format!("Must be a number {}–{}.", spec.min, spec.max),
            format!("{}～{} の数値を入力してください。", spec.min, spec.max),
        ),
        NumError::TooSmall => (
            format!("Terlalu kecil (min {}).", spec.min),
            format!("Too small (min {}).", spec.min),
            format!("小さすぎます (最小 {})。", spec.min),
        ),
        NumError::TooLarge => (
            format!("Terlalu besar (maks {}).", spec.max),
            format!("Too large (max {}).", spec.max),
            format!("大きすぎます (最大 {})。", spec.max),
        ),
    };
    tr(lang, &id, &en, &ja)
}

/// Tulis `Prefs` ke DB, lalu terapkan efek yang tidak boleh menunggu restart:
/// folder kerja, auto-scroll, dan centang menu tray (lewat `apply_prefs`).
pub fn simpan_sekarang(state: &mut AppState) {
    if let Err(e) = state.prefs.save() {
        state.notice = Some(e);
        return;
    }
    // Auto-scroll tidak ada di `Prefs` (sumbernya `Persisted`), jadi tulis
    // sendiri supaya start berikutnya membaca nilai yang sama.
    arka_core::settings::set(
        "auto_scroll",
        if state.chat.auto_scroll {
            "true"
        } else {
            "false"
        },
    )
    .ok();
    match state.apply_workspace() {
        Ok(dir) => {
            state.files.invalidate();
            state.chat.pick_dir = dir.clone();
            state.notice = Some(state.tr(
                &format!("Setelan tersimpan. Folder kerja: {}", dir.display()),
                &format!("Settings saved. Workspace: {}", dir.display()),
                &format!("設定を保存しました。ワークスペース: {}", dir.display()),
            ));
        }
        Err(e) => state.notice = Some(e),
    }
    super::super::apply_prefs(state);
}

// ── Workspace ──────────────────────────────────────────────────────────────

fn workspace(ui: &mut Ui, state: &mut AppState, lang: Lang, colors: &ClayColors) {
    ui.add_space(theme::SP_MD);
    widgets::section(
        ui,
        colors,
        &tr(lang, "Folder kerja", "Workspace", "ワークスペース"),
    );
    let hint = state.workspace().display().to_string();
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        ui.label(
            RichText::new(tr(
                lang,
                "Path Windows. Kosong = folder bawaan.",
                "A Windows path. Leave empty for the default folder.",
                "Windows のパス。空欄なら既定フォルダー。",
            ))
            .size(fs(ui, 11.5))
            .color(colors.muted),
        );
        ui.add_space(theme::SP_XS);
        ui.add_sized(
            [ui.available_width(), 30.0 * theme::scale_of(ui.ctx())],
            egui::TextEdit::singleline(&mut state.prefs.workspace_path).hint_text(hint),
        );
        ui.add_space(theme::SP_SM);
        ui.horizontal(|ui| {
            if widgets::clay_button(
                ui,
                colors,
                &tr(
                    lang,
                    "Buka di Explorer",
                    "Open in Explorer",
                    "エクスプローラーで開く",
                ),
                true,
            )
            .clicked()
            {
                let p = state.workspace();
                if let Err(e) = crate::platform::open_path(&p) {
                    state.notify(e);
                }
            }
            if widgets::clay_button(
                ui,
                colors,
                &tr(lang, "Simpan folder", "Save folder", "フォルダーを保存"),
                true,
            )
            .clicked()
            {
                simpan_sekarang(state);
            }
        });
        widgets::kv(
            ui,
            colors,
            &tr(lang, "Sedang dipakai", "Currently active", "現在使用中"),
            &state.workspace().display().to_string(),
        );
    });
}

// ── Lingkungan / statistik sistem ──────────────────────────────────────────

fn lingkungan(ui: &mut Ui, state: &AppState, lang: Lang, colors: &ClayColors) {
    ui.add_space(theme::SP_MD);
    widgets::section(ui, colors, &tr(lang, "Lingkungan", "Environment", "環境"));
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        widgets::kv(
            ui,
            colors,
            &tr(lang, "Folder skill", "Skills folder", "スキルフォルダー"),
            &state.paths.skills_dir.display().to_string(),
        );
        widgets::kv(
            ui,
            colors,
            &tr(lang, "Platform", "Platform", "プラットフォーム"),
            &format!("{:?}", state.paths.platform),
        );
        ui.separator();
        let i = state.sysinfo.clone();
        widgets::kv(
            ui,
            colors,
            &tr(lang, "Arsitektur", "Architecture", "アーキテクチャ"),
            &dash(lang, &i.arch),
        );
        widgets::kv(
            ui,
            colors,
            &tr(lang, "Sistem", "System", "システム"),
            &dash(lang, &i.os),
        );
        widgets::kv(
            ui,
            colors,
            &tr(lang, "RAM", "RAM", "メモリ"),
            &ram_text(lang, &i),
        );
        widgets::kv(
            ui,
            colors,
            &tr(lang, "Disk bebas", "Free disk", "空きディスク"),
            &disk_text(lang, &i),
        );
    });
}

fn dash(lang: Lang, value: &str) -> String {
    if value.is_empty() {
        tr(lang, "—", "—", "—")
    } else {
        value.to_string()
    }
}

fn ram_text(lang: Lang, i: &SysInfo) -> String {
    match (i.ram_used_mb, i.ram_total_mb) {
        (Some(u), Some(t)) => {
            let pct = sysinfo::ram_percent(i.ram_used_mb, i.ram_total_mb)
                .map(|p| format!(" ({p}%)"))
                .unwrap_or_default();
            format!("{u} / {t} MB{pct}")
        }
        _ => tr(lang, "—", "—", "—"),
    }
}

fn disk_text(lang: Lang, i: &SysInfo) -> String {
    match (i.disk_free_gb, i.disk_total_gb) {
        (Some(f), Some(t)) => format!("{f} / {t} GB"),
        _ => tr(lang, "—", "—", "—"),
    }
}

// ── Tentang ────────────────────────────────────────────────────────────────

fn tentang(ui: &mut Ui, state: &mut AppState, lang: Lang, colors: &ClayColors) {
    ui.add_space(theme::SP_MD);
    widgets::section(ui, colors, &tr(lang, "Tentang", "About", "情報"));
    widgets::clay_card(ui, colors, None, |ui| {
        let resp = ui.add_sized(
            [220.0 * theme::scale_of(ui.ctx()), 34.0],
            egui::Button::new(
                RichText::new(tr(
                    lang,
                    "Tentang Arka Agent",
                    "About Arka Agent",
                    "Arka Agent について",
                ))
                .size(fs(ui, 14.0)),
            )
            .fill(colors.surface_raised),
        );
        if resp.clicked() {
            state.modal = Modal::About;
        }
        ui.add_space(theme::SP_SM);
        ui.label(
            RichText::new(format!(
                "v{} · 100% Rust · egui · MIT",
                env!("CARGO_PKG_VERSION")
            ))
            .size(fs(ui, 12.0))
            .color(colors.muted),
        );
        ui.label(
            RichText::new(format!("commit {}", crate::build_commit()))
                .size(fs(ui, 11.0))
                .color(colors.muted),
        );
    });
}
