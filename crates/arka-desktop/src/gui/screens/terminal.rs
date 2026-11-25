//! Tab Terminal: kotak perintah interaktif + status mesin.
//!
//! Mesinnya (`PwshShell`, `-EncodedCommand`, Job Object) selesai di M2.
//! Yang ada di sini adalah **pengantarnya**: input, riwayat, tombol
//! Jalankan/Stop, output real-time, dan kartu status shell.
//!
//! Aturan yang dipegang: **tidak ada proses yang dijalankan dari `draw`**.
//! Deteksi shell, status mesin, ukuran workspace, dan setiap perintah berjalan
//! di thread latar lalu hasilnya diambil [`AppState::poll_terminal`]. Kalau
//! salah satu dipanggil langsung di sini, jendela membeku sampai prosesnya
//! selesai — termasuk tombol Stop.

use eframe::egui::{self, RichText, Ui};

use crate::gui::state::{AppState, InfoState, ShellProbe};
use crate::gui::theme::{self, fs, ClayColors};
use crate::gui::widgets;

/// Contoh perintah PowerShell. **Bukan** bash: `ls -la`, `cat`, `apt-get`
/// tidak ada di Windows, jadi tidak boleh ditampilkan sebagai saran.
pub const HINTS: [&str; 6] = [
    "Get-ChildItem",
    "git status",
    "python --version",
    "node --version",
    "Get-Process | Sort-Object CPU -Descending | Select-Object -First 5",
    "Get-ChildItem -Recurse -Filter *.log | Measure-Object -Property Length -Sum",
];

/// Ukuran byte → teks pendek (`1,4 MB`). Pemisah ribuan memakai titik desimal
/// gaya Indonesia supaya konsisten dengan sisa UI.
pub fn human_bytes(n: i64) -> String {
    if n < 0 {
        return "?".into();
    }
    let f = n as f64;
    const KB: f64 = 1024.0;
    if f < KB {
        return format!("{n} B");
    }
    let (v, unit) = if f < KB * KB {
        (f / KB, "KB")
    } else if f < KB * KB * KB {
        (f / (KB * KB), "MB")
    } else {
        (f / (KB * KB * KB), "GB")
    };
    format!("{v:.1} {unit}").replace('.', ",")
}

pub fn show(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    // Deteksi shell sekali (mahal: menjalankan proses), lalu ambil status,
    // ukuran workspace, dan hasil pekerjaan yang sudah selesai.
    state.ensure_shell_probe(ui.ctx());
    state.ensure_term_info(ui.ctx());
    state.ensure_workspace_stats(ui.ctx());
    state.poll_terminal();

    // Seluruh isi tab dibungkus gulir. Tanpa ini, pada jendela 1366×768
    // (banyak laptop) bagian output dan log agent berada di luar layar dan
    // **tidak bisa dicapai sama sekali** — bukan sekadar tersembunyi.
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            header(ui, state, colors);
            ui.add_space(theme::SP_MD);
            status_card(ui, state, colors);
            diagnostic_card(ui, state, colors);
            ui.add_space(theme::SP_MD);
            hints(ui, state, colors);
            ui.add_space(theme::SP_MD);
            command_box(ui, state, colors);
            ui.add_space(theme::SP_MD);
            output_panel(ui, state, colors);
            ui.add_space(theme::SP_MD);
            agent_log(ui, state, colors);
        });
}

fn header(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(state.tr("Terminal", "Terminal", "ターミナル"))
                .size(fs(ui, 22.0))
                .strong(),
        );
        let (label, color) = match &state.shell {
            ShellProbe::Ready(_) => (state.tr("Siap", "Ready", "準備完了"), colors.success),
            ShellProbe::Missing(_) => (
                state.tr("Tidak ditemukan", "Not found", "見つかりません"),
                colors.danger,
            ),
            ShellProbe::Unknown | ShellProbe::Probing => {
                (state.tr("Memeriksa…", "Checking…", "確認中…"), colors.muted)
            }
        };
        widgets::clay_pill(ui, colors, &label, color);

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Periksa ulang", "Re-check", "再確認"),
                colors.accent,
            )
            .clicked()
            {
                state.refresh_shell_probe(ui.ctx());
                // Status mesin ikut diambil ulang: pengguna bisa saja baru
                // memasang PowerShell 7 tanpa menutup aplikasi.
                state.term.info_attempted = false;
                state.term.info = InfoState::Unknown;
                if let ShellProbe::Missing(e) = &state.shell {
                    state.notify(e.clone());
                }
            }
        });
    });
}

fn status_card(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    widgets::section(ui, colors, &state.tr("Shell", "Shell", "シェル"));
    let info = state.term.info.clone();
    let ws = state.workspace();
    let stats = state.term.ws;
    let dash = "—".to_string();

    let mut open_ws = false;
    widgets::clay_card(ui, colors, None, |ui| {
        match &state.shell {
            ShellProbe::Ready(d) => {
                ui.horizontal(|ui| {
                    widgets::clay_pill(
                        ui,
                        colors,
                        &state.tr("Aktif", "Active", "有効"),
                        colors.success,
                    );
                    ui.label(RichText::new(d.describe()).size(fs(ui, 15.0)).strong());
                });
            }
            ShellProbe::Missing(e) => {
                ui.label(RichText::new(e).size(fs(ui, 12.5)).color(colors.danger));
                ui.add_space(theme::SP_XS);
                ui.label(
                    RichText::new(state.tr(
                        "Pasang PowerShell 7: winget install Microsoft.PowerShell",
                        "Install PowerShell 7: winget install Microsoft.PowerShell",
                        "PowerShell 7 を導入: winget install Microsoft.PowerShell",
                    ))
                    .size(fs(ui, 12.0))
                    .monospace()
                    .color(colors.muted),
                );
            }
            ShellProbe::Unknown | ShellProbe::Probing => {
                ui.label(
                    RichText::new(state.tr("Memeriksa…", "Checking…", "確認中…"))
                        .size(fs(ui, 13.0))
                        .color(colors.muted),
                );
            }
        }

        ui.add_space(theme::SP_SM);
        let get = |k: &str| info.get(k).unwrap_or("—").to_string();
        widgets::kv(
            ui,
            colors,
            &state.tr("Versi", "Version", "バージョン"),
            &get("PSVersion"),
        );
        widgets::kv(
            ui,
            colors,
            &state.tr("Edisi", "Edition", "エディション"),
            &get("PSEdition"),
        );
        widgets::kv(ui, colors, &state.tr("Sistem", "OS", "OS"), &get("OS"));
        widgets::kv(
            ui,
            colors,
            &state.tr("Arsitektur", "Architecture", "アーキテクチャ"),
            &get("Arch"),
        );
        widgets::kv(
            ui,
            colors,
            &state.tr("ExecutionPolicy", "ExecutionPolicy", "ExecutionPolicy"),
            &get("Policy"),
        );
        widgets::kv(
            ui,
            colors,
            &state.tr("Encoding", "Encoding", "エンコーディング"),
            "UTF-8 · -EncodedCommand",
        );

        ui.add_space(theme::SP_SM);
        let size = match stats {
            Some(s) => format!(
                "{} {} · {}",
                s.files,
                state.tr("berkas", "files", "ファイル"),
                human_bytes(s.bytes)
            ),
            None => dash.clone(),
        };
        widgets::kv(
            ui,
            colors,
            &state.tr("Folder kerja", "Workspace", "ワークスペース"),
            &ws.display().to_string(),
        );
        widgets::kv(ui, colors, &state.tr("Isi", "Contents", "内容"), &size);

        ui.add_space(theme::SP_SM);
        ui.horizontal(|ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr(
                    "Buka di Explorer",
                    "Open in Explorer",
                    "エクスプローラーで開く",
                ),
                colors.accent,
            )
            .clicked()
            {
                open_ws = true;
            }
            let busy = state.term.diag_job.is_some();
            let label = if busy {
                state.tr("Menguji…", "Testing…", "テスト中…")
            } else {
                state.tr("Tes shell", "Test shell", "シェルをテスト")
            };
            if widgets::clay_pill_button(ui, colors, &label, colors.surface_raised).clicked()
                && !busy
            {
                let ctx = ui.ctx().clone();
                state.run_terminal_diagnostic(&ctx);
            }
        });
    });

    if open_ws {
        if let Err(e) = crate::platform::open_path(&ws) {
            state.notify(e);
        }
    }
}

fn diagnostic_card(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    let Some(text) = state.term.diagnostic.clone() else {
        return;
    };
    ui.add_space(theme::SP_MD);
    ui.horizontal(|ui| {
        widgets::section(
            ui,
            colors,
            &state.tr("Hasil tes shell", "Shell test result", "シェルテスト結果"),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Tutup", "Close", "閉じる"),
                colors.accent,
            )
            .clicked()
            {
                state.term.diagnostic = None;
            }
        });
    });
    widgets::clay_card(ui, colors, Some(colors.background), |ui| {
        egui::ScrollArea::vertical()
            .max_height(180.0 * theme::scale_of(ui.ctx()))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(&text)
                        .size(fs(ui, 12.0))
                        .monospace()
                        .color(colors.on_surface),
                );
            });
    });
}

fn hints(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    widgets::section(
        ui,
        colors,
        &state.tr("Contoh perintah", "Command examples", "コマンド例"),
    );
    let mut pick: Option<&str> = None;
    widgets::clay_card(ui, colors, Some(colors.surface_raised), |ui| {
        for h in HINTS {
            if widgets::clay_pill_button(ui, colors, h, colors.accent).clicked() {
                pick = Some(h);
            }
            ui.add_space(theme::SP_XS);
        }
    });
    if let Some(h) = pick {
        state.term.input = h.to_string();
        state.term.focus_input = true;
        state.term.history.reset();
    }
}

fn command_box(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    widgets::section(
        ui,
        colors,
        &state.tr("Kotak perintah", "Command box", "コマンド入力"),
    );

    // Riwayat: dropdown 15 terbaru. ↑/↓ tetap jalan walau dropdown tertutup.
    ui.horizontal(|ui| {
        let label = format!(
            "{} ({})",
            state.tr("Riwayat", "History", "履歴"),
            state.term.history.len()
        );
        if widgets::clay_pill_button(ui, colors, &label, colors.surface_raised).clicked() {
            state.term.show_history = !state.term.show_history;
        }
        if state.term.history.is_empty() {
            ui.label(
                RichText::new(state.tr("belum ada", "empty", "空"))
                    .size(fs(ui, 11.5))
                    .color(colors.muted),
            );
        } else if widgets::clay_pill_button(
            ui,
            colors,
            &state.tr("Hapus riwayat", "Clear history", "履歴を消去"),
            colors.muted,
        )
        .clicked()
        {
            state.term.history.clear();
            state.term.show_history = false;
        }
        if state.term.history.is_navigating() {
            ui.label(
                RichText::new(state.tr("menelusuri ↑/↓", "browsing ↑/↓", "↑/↓ で参照中"))
                    .size(fs(ui, 11.0))
                    .color(colors.accent),
            );
        }
    });

    if state.term.show_history {
        let items: Vec<String> = state
            .term
            .history
            .items_newest_first()
            .into_iter()
            .take(15)
            .collect();
        let mut pick: Option<String> = None;
        widgets::clay_card(ui, colors, Some(colors.background), |ui| {
            for it in &items {
                if ui
                    .add(
                        egui::Label::new(
                            RichText::new(it)
                                .size(fs(ui, 12.0))
                                .monospace()
                                .color(colors.on_surface),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .clicked()
                {
                    pick = Some(it.clone());
                }
            }
        });
        if let Some(p) = pick {
            state.term.input = p;
            state.term.show_history = false;
            state.term.focus_input = true;
            state.term.history.reset();
        }
    }

    ui.add_space(theme::SP_SM);
    let busy = state.term.running();
    let mut run = false;
    let mut stop = false;
    ui.horizontal(|ui| {
        // Monospace: perintah PowerShell sering berisi path & kurung kurawal.
        let hint = state.tr(
            "Ketik perintah PowerShell…",
            "Type a PowerShell command…",
            "PowerShell コマンドを入力…",
        );
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.term.input)
                .hint_text(hint)
                .font(egui::TextStyle::Monospace)
                .desired_width(f32::INFINITY)
                .interactive(true),
        );

        if state.term.focus_input {
            resp.request_focus();
            state.term.focus_input = false;
        }

        if resp.has_focus() {
            // ↑/↓ menelusuri riwayat. `consume_key` mencegah kursor teks ikut
            // melompat ke awal/akhir baris.
            let up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp));
            let down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown));
            if up {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp));
                if let Some(cmd) = state.term.history.prev() {
                    state.term.input = cmd;
                }
            } else if down {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown));
                if let Some(cmd) = state.term.history.next() {
                    state.term.input = cmd;
                } else {
                    state.term.input.clear();
                }
            }

            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
            if enter && !busy {
                ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
                run = true;
            }
        }
    });

    ui.add_space(theme::SP_SM);
    ui.horizontal(|ui| {
        if busy {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Stop", "Stop", "停止"),
                colors.danger,
            )
            .clicked()
            {
                stop = true;
            }
            ui.label(
                RichText::new(state.tr("Berjalan…", "Running…", "実行中…"))
                    .size(fs(ui, 12.0))
                    .color(colors.muted),
            );
        } else if widgets::clay_pill_button(
            ui,
            colors,
            &state.tr("Jalankan", "Run", "実行"),
            colors.accent,
        )
        .clicked()
        {
            run = true;
        }

        // Labelnya **perintah** terakhir, bukan potongan output: percobaan
        // pertama memakai `tail(output)` sebagai label sementara klik-nya
        // mengulang perintah dari riwayat — tombol yang janjinya berbeda
        // dengan perbuatannya.
        if !busy {
            if let Some(last) = state.term.history.items().last().cloned() {
                let label = format!(
                    "{} {}",
                    state.tr("Ulangi:", "Repeat:", "再実行:"),
                    tail(&last, 40)
                );
                if widgets::clay_pill_button(ui, colors, &label, colors.surface_raised).clicked() {
                    state.term.input = last;
                    state.term.focus_input = true;
                    state.term.history.reset();
                }
            }
        }
    });

    if stop {
        state.stop_terminal();
    }
    if run {
        let ctx = ui.ctx().clone();
        state.run_terminal(&ctx);
    }
}

/// Potong teks jadi satu baris pendek untuk label tombol.
fn tail(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.chars().count() <= max {
        return line.to_string();
    }
    line.chars().take(max).collect::<String>() + "…"
}

fn output_panel(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    ui.horizontal(|ui| {
        widgets::section(ui, colors, &state.tr("Output", "Output", "出力"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Label sengaja berbeda dari tombol di log agent: dua tombol
            // "Bersihkan" di satu layar membingungkan pengguna (dan test).
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Bersihkan output", "Clear output", "出力をクリア"),
                colors.accent,
            )
            .clicked()
            {
                state.clear_terminal();
            }
        });
    });

    // Saat berjalan, sumbernya `Job` (terus bertambah). Setelah selesai,
    // hasilnya sudah dipindah ke `TermState::output` oleh `poll_terminal`.
    let text: String = match &state.term.job {
        Some(job) => job.output.clone(),
        None => state.term.output.clone(),
    };
    let exit = state.term.exit;
    let busy = state.term.running();
    let scale = theme::scale_of(ui.ctx());

    let echo = state.term.job.as_ref().and_then(|j| {
        (j.kind == crate::gui::term_job::TaskKind::Run && !j.cmd.is_empty()).then(|| j.cmd.clone())
    });

    widgets::clay_card(ui, colors, Some(colors.background), |ui| {
        if let Some(cmd) = &echo {
            ui.label(
                RichText::new(format!("PS> {cmd}"))
                    .size(fs(ui, 12.0))
                    .monospace()
                    .color(colors.accent),
            );
            ui.add_space(theme::SP_XS);
        }
        if text.trim().is_empty() && !busy {
            ui.label(
                RichText::new(state.tr(
                    "Jalankan perintah untuk melihat output-nya di sini.",
                    "Run a command to see its output here.",
                    "コマンドを実行するとここに出力が表示されます。",
                ))
                .size(fs(ui, 12.5))
                .color(colors.muted),
            );
            return;
        }
        egui::ScrollArea::vertical()
            .max_height(300.0 * scale)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    RichText::new(&text)
                        .size(fs(ui, 12.0))
                        .monospace()
                        .color(colors.on_surface),
                );
                if busy {
                    ui.label(
                        RichText::new("▌")
                            .size(fs(ui, 12.0))
                            .monospace()
                            .color(colors.accent),
                    );
                }
            });

        if let Some(line) = exit {
            ui.add_space(theme::SP_XS);
            ui.horizontal(|ui| {
                let color = if line.ok() {
                    colors.success
                } else {
                    colors.danger
                };
                widgets::clay_pill(ui, colors, &line.text(state.lang.short()), color);
            });
        }
    });
}

fn agent_log(ui: &mut Ui, state: &mut AppState, colors: &ClayColors) {
    ui.horizontal(|ui| {
        widgets::section(
            ui,
            colors,
            &state.tr(
                "Log perintah agent",
                "Agent command log",
                "エージェントのコマンドログ",
            ),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::clay_pill_button(
                ui,
                colors,
                &state.tr("Bersihkan log", "Clear log", "ログをクリア"),
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ukuran_manusiawi_memakai_titik_desimal_indonesia() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(1023), "1023 B");
        assert_eq!(human_bytes(1024), "1,0 KB");
        assert_eq!(human_bytes(1536), "1,5 KB");
        assert_eq!(human_bytes(1024 * 1024), "1,0 MB");
        assert_eq!(human_bytes(3 * 1024 * 1024 * 1024), "3,0 GB");
        assert_eq!(human_bytes(-1), "?");
    }

    #[test]
    fn contoh_perintah_adalah_powershell_bukan_bash() {
        // Sama seperti penjaga prompt di M1: saran bash di aplikasi Windows
        // langsung gagal saat diklik, dan membuat pengguna bingung.
        let bash_only = [
            "ls -la", "cat ", "apt-get", "sudo ", "chmod ", "grep ", "which ",
        ];
        for h in HINTS {
            for b in bash_only {
                assert!(!h.contains(b), "hint `{h}` memakai perintah bash `{b}`");
            }
        }
        assert!(HINTS.iter().any(|h| h.starts_with("Get-")));
    }

    #[test]
    fn tail_mengambil_baris_terakhir_yang_berisi() {
        assert_eq!(tail("a\nb\n\n", 10), "b");
        assert_eq!(tail("", 10), "");
        assert_eq!(tail("abcdefghij", 4), "abcd…");
        // Karakter multibyte tidak boleh terpotong di tengah.
        assert_eq!(tail("日本語テスト", 3), "日本語…");
    }
}
