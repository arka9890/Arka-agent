//! Test UI tanpa jendela dan tanpa GPU.
//!
//! `egui::Context` menghitung tata letak di CPU, jadi seluruh kerangka UI bisa
//! dijalankan di CI Linux — termasuk **klik sungguhan** pada koordinat hasil
//! tata letak. Ini bukan test "tidak panic": tiap test di bawah menuntut
//! perilaku yang terlihat (tab berpindah, warna latar berganti, tinggi teks
//! naik, snackbar hilang, isi folder terbaca).
//!
//! Yang tidak dibuktikan di sini: rupa akhir di layar (butuh GPU/jendela).
//! Itu diperiksa manual di Windows; lihat `windows/todo.md` M3.

use std::fs;

use eframe::egui::epaint::ClippedShape;
use eframe::egui::{
    self, Color32, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Shape, Vec2,
};

use arka_core::paths::AppPaths;

use crate::gui::engine::Msg;
use crate::gui::i18n::Lang;
use crate::gui::state::{
    AppState, ApprovalCard, Modal, MsgView, Persisted, ProvRow, ProviderForm, SessionRow, Tab,
};
use crate::gui::theme::{self, FontScale, ThemeMode};
use crate::gui::ArkaGui;

/// Path palsu. Test tidak memakai `paths::init` global, kecuali test tab File
/// yang menunjuk folder sementara sendiri.
fn paths() -> AppPaths {
    let base = std::env::temp_dir().join("arka-gui-test");
    AppPaths::windows(base.join("home"), base.join("data"))
}

fn tempdir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("arka-gui-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct Harness {
    ctx: egui::Context,
    gui: ArkaGui,
    time: f64,
    screen: Rect,
    /// Kirim `ViewportEvent::Close` pada frame berikutnya (tombol X).
    close_requested: bool,
    /// `CancelClose` dari frame terakhir (bukti permintaan tutup ditolak).
    last_cancel_close: bool,
}

impl Harness {
    fn new(state: AppState) -> Self {
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        Self {
            ctx,
            gui: ArkaGui::from_state(state),
            time: 0.0,
            screen: Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 880.0)),
            close_requested: false,
            last_cancel_close: false,
        }
    }

    fn with_default_state() -> Self {
        Self::new(AppState::new(paths(), Persisted::default()))
    }

    fn state(&self) -> &AppState {
        self.gui.state()
    }

    fn state_mut(&mut self) -> &mut AppState {
        self.gui.state_mut()
    }

    fn advance(&mut self, secs: f64) {
        self.time += secs;
    }

    fn frame_with(&mut self, events: Vec<Event>) -> Vec<Shape> {
        self.time += 1.0 / 60.0;
        // `RawInput::default()` sudah memuat ROOT; menimpa peta dengan peta
        // kosong membuat egui panik ("Failed to find current viewport").
        let mut viewports = egui::ViewportIdMap::default();
        viewports.insert(egui::ViewportId::ROOT, egui::ViewportInfo::default());
        if std::mem::take(&mut self.close_requested) {
            viewports.insert(
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    events: vec![egui::ViewportEvent::Close],
                    ..Default::default()
                },
            );
        }
        let input = RawInput {
            screen_rect: Some(self.screen),
            time: Some(self.time),
            events,
            viewports,
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let gui = &mut self.gui;
        let out = ctx.run(input, |c| gui.draw(c));
        self.last_cancel_close = out.viewport_output.values().any(|v| {
            v.commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
        });
        flatten(&out.shapes)
    }

    /// Frame dengan permintaan tutup jendela (tombol X).
    ///
    /// `ViewportEvent::Close` adalah satu-satunya cara egui menandainya, dan
    /// `handle_close` membacanya dari `input(|i| i.viewport().close_requested())`.
    /// Balasan `CancelClose` diperiksa dari output — itu bukti aplikasi benar-
    /// benar menolak ditutup, bukan hanya mengubah state sendiri.
    fn frame_closing(&mut self) -> (Vec<Shape>, bool) {
        self.frame();
        self.close_requested = true;
        self.time += 1.0 / 60.0;
        let input = RawInput {
            screen_rect: Some(self.screen),
            time: Some(self.time),
            viewports: std::iter::once((
                egui::ViewportId::ROOT,
                egui::ViewportInfo {
                    events: vec![egui::ViewportEvent::Close],
                    ..Default::default()
                },
            ))
            .collect(),
            ..Default::default()
        };
        let ctx = self.ctx.clone();
        let gui = &mut self.gui;
        let out = ctx.run(input, |c| gui.step(c));
        let canceled = out.viewport_output.values().any(|v| {
            v.commands
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::CancelClose))
        });
        (flatten(&out.shapes), canceled)
    }

    fn frame(&mut self) -> Vec<Shape> {
        self.frame_with(Vec::new())
    }

    /// Klik kiri pada satu titik: tekan di satu frame, lepas di frame berikut.
    fn click(&mut self, pos: Pos2) -> Vec<Shape> {
        let modifiers = Modifiers::default();
        self.frame_with(vec![
            Event::PointerMoved(pos),
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers,
            },
        ]);
        self.frame_with(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers,
        }])
    }
}

fn flatten(shapes: &[ClippedShape]) -> Vec<Shape> {
    fn walk(s: &Shape, out: &mut Vec<Shape>) {
        match s {
            Shape::Vec(v) => {
                for s in v {
                    walk(s, out);
                }
            }
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    for c in shapes {
        walk(&c.shape, &mut out);
    }
    out
}

/// Semua teks yang benar-benar tergambar, beserta kotaknya di layar.
fn texts(shapes: &[Shape]) -> Vec<(Rect, String)> {
    shapes
        .iter()
        .filter_map(|s| match s {
            Shape::Text(t) => Some((
                Rect::from_min_size(t.pos, t.galley.size()),
                t.galley.job.text.clone(),
            )),
            _ => None,
        })
        .collect()
}

/// Kotak teks yang isinya persis `needle`. Gagal bila tidak ada atau ganda —
/// kegandaan biasanya berarti tata letak berubah dan test perlu ditinjau.
fn find_text(shapes: &[Shape], needle: &str) -> Rect {
    let all = texts(shapes);
    let hits: Vec<Rect> = all
        .iter()
        .filter(|(_, t)| t == needle)
        .map(|(r, _)| *r)
        .collect();
    assert!(
        !hits.is_empty(),
        "teks {needle:?} tidak tergambar. Yang ada: {:?}",
        all.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(hits.len(), 1, "teks {needle:?} muncul {} kali", hits.len());
    hits[0]
}

/// Warna teks yang benar-benar dilukis untuk `needle`.
fn shape_color(shapes: &[Shape], needle: &str) -> Color32 {
    shapes
        .iter()
        .find_map(|s| match s {
            Shape::Text(t) if t.galley.job.text == needle => Some(t.fallback_color),
            _ => None,
        })
        .unwrap_or_else(|| panic!("teks {needle:?} tidak ditemukan"))
}

fn height_of(shapes: &[Shape], needle: &str) -> f32 {
    find_text(shapes, needle).height()
}

fn fill_count(shapes: &[Shape], color: Color32) -> usize {
    shapes
        .iter()
        .filter(|s| matches!(s, Shape::Rect(r) if r.fill == color))
        .count()
}

// ── Kriteria "selesai bila" M3 ──────────────────────────────────────────────

/// "5 tab bisa diklik" — dibuktikan dengan klik nyata pada koordinat label.
#[test]
fn lima_tab_bisa_diklik() {
    for target in Tab::ALL {
        let mut h = Harness::with_default_state();
        // Mulai dari tab lain supaya klik pada Chat pun benar-benar diuji.
        h.state_mut().tab = if target == Tab::Chat {
            Tab::Settings
        } else {
            Tab::Chat
        };
        let shapes = h.frame();

        let label = format!("{} {}", target.glyph(), target.label(Lang::Id));
        let rect = find_text(&shapes, &label);
        h.click(rect.center());
        h.frame();

        assert_eq!(h.state().tab, target, "klik {label:?} tidak berpindah tab");
    }
}

/// "Tema gelap/terang bisa diganti dan langsung terlihat" — diperiksa dari
/// warna yang benar-benar dilukis, bukan dari nilai setelan.
#[test]
fn tema_gelap_dan_terang_mengubah_warna_latar() {
    let mut h = Harness::with_default_state();

    h.state_mut().theme = ThemeMode::Light;
    let shapes = h.frame();
    assert!(
        fill_count(&shapes, theme::LIGHT.background) > 0,
        "latar terang tidak dilukis"
    );
    assert_eq!(fill_count(&shapes, theme::DARK.background), 0);

    h.state_mut().theme = ThemeMode::Dark;
    let shapes = h.frame();
    assert!(
        fill_count(&shapes, theme::DARK.background) > 0,
        "latar gelap tidak dilukis"
    );
    assert_eq!(fill_count(&shapes, theme::LIGHT.background), 0);
}

/// "Ukuran font bisa diganti dan langsung terlihat".
///
/// Ini menjaga bug yang pernah terjadi di versi Android: setelan ukuran teks
/// tersimpan, tetapi ukuran di layar tidak berubah karena semua `.size(...)`
/// ditulis mentah. Di sini teks harus benar-benar membesar.
#[test]
fn ukuran_teks_benar_benar_membesar() {
    let measure = |scale: FontScale| {
        let mut h = Harness::with_default_state();
        h.state_mut().font_scale = scale;
        height_of(&h.frame(), "Arka Agent")
    };

    let small = measure(FontScale::Small);
    let medium = measure(FontScale::Medium);
    let large = measure(FontScale::Large);

    assert!(
        small < medium,
        "Kecil ({small}) harus lebih kecil dari Sedang ({medium})"
    );
    assert!(
        medium < large,
        "Besar ({large}) harus lebih besar dari Sedang ({medium})"
    );

    let ratio = large / small;
    let want = FontScale::Large.factor() / FontScale::Small.factor();
    assert!(
        (ratio - want).abs() < 0.05,
        "rasio tinggi teks {ratio:.3}, seharusnya {want:.3}"
    );
}

// ── Interaksi lewat layar ───────────────────────────────────────────────────

#[test]
fn setelan_tema_bisa_diklik() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Settings;
    h.state_mut().theme = ThemeMode::Light;

    let shapes = h.frame();
    let rect = find_text(&shapes, "Gelap");
    h.click(rect.center());
    h.frame();

    assert_eq!(h.state().theme, ThemeMode::Dark);

    // Dan hasilnya terlihat: frame berikutnya berlatar gelap.
    let shapes = h.frame();
    assert!(fill_count(&shapes, theme::DARK.background) > 0);
}

#[test]
fn snackbar_hilang_sendiri() {
    let mut h = Harness::with_default_state();
    h.state_mut().notify("halo dunia");

    let shapes = h.frame();
    assert!(
        texts(&shapes).iter().any(|(_, t)| t == "halo dunia"),
        "snackbar tidak tampil"
    );

    h.advance(5.0); // NOTICE_SECS = 4
    let shapes = h.frame();
    assert!(
        !texts(&shapes).iter().any(|(_, t)| t == "halo dunia"),
        "snackbar masih tampil setelah 5 detik"
    );
    assert!(h.state().notice.is_none());
}

#[test]
fn dialog_tentang_bisa_ditutup() {
    let mut h = Harness::with_default_state();
    h.state_mut().modal = Modal::About;

    // egui butuh satu frame untuk mengukur jendela baru (ukurannya belum
    // diketahui pada frame pertama); di aplikasi nyata ini tidak terasa.
    h.frame();
    let shapes = h.frame();
    let rect = find_text(&shapes, "Tutup");
    h.click(rect.center());
    h.frame();

    assert_eq!(h.state().modal, Modal::None);
}

/// Tab File membaca disk sungguhan, dan **tidak** membacanya tiap frame.
#[test]
fn tab_file_membaca_folder_dan_memakai_cache() {
    let dir = tempdir("files");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::write(dir.join("catatan.txt"), b"halo").unwrap();

    let mut p = paths();
    p.workspace = dir.clone();
    // `AppState::workspace()` membaca workspace global (`paths`), bukan hanya
    // `AppPaths` milik state — arahkan keduanya ke folder sementara, lalu
    // kembalikan supaya test lain yang berjalan paralel tidak terpengaruh.
    arka_core::paths::override_workspace(dir.clone());
    let mut h = Harness::new(AppState::new(p, Persisted::default()));
    h.state_mut().tab = Tab::File;

    let shapes = h.frame();
    assert!(h.state().files.loaded);
    let names: Vec<String> = h
        .state()
        .files
        .entries
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["sub", "catatan.txt"], "folder dulu, lalu nama");
    assert!(
        texts(&shapes).iter().any(|(_, t)| t == "catatan.txt"),
        "nama berkas tidak tergambar"
    );

    // Berkas baru muncul di disk, tetapi frame berikutnya tidak membacanya
    // ulang: itulah gunanya cache.
    fs::write(dir.join("baru.txt"), b"x").unwrap();
    let shapes = h.frame();
    assert_eq!(
        h.state().files.entries.len(),
        2,
        "cache membaca ulang tiap frame"
    );
    assert!(!texts(&shapes).iter().any(|(_, t)| t == "baru.txt"));

    // Tombol "Muat ulang" memaksa baca.
    let rect = find_text(&shapes, "Muat ulang");
    h.click(rect.center());
    h.frame();
    assert_eq!(h.state().files.entries.len(), 3);
    assert!(h.state().files.entries.iter().any(|e| e.name == "baru.txt"));

    arka_core::paths::clear_workspace_override();
    let _ = fs::remove_dir_all(&dir);
}

// ── Ketahanan ───────────────────────────────────────────────────────────────

/// Setiap layar × setiap bahasa harus tergambar tanpa panic, dan menghasilkan
/// teks (bukan kartu kosong).
#[test]
fn semua_tab_semua_bahasa_tergambar() {
    for lang in Lang::ALL {
        for tab in Tab::ALL {
            let mut h = Harness::with_default_state();
            h.state_mut().lang = lang;
            h.state_mut().tab = tab;
            let shapes = h.frame();
            assert!(
                !texts(&shapes).is_empty(),
                "tab {tab:?} bahasa {lang:?} tidak menghasilkan teks"
            );
            assert!(
                shapes.iter().any(|s| matches!(s, Shape::Rect(_))),
                "tab {tab:?} bahasa {lang:?} tidak melukis apa pun"
            );
        }
    }
}

/// Setelan harus bertahan setelah aplikasi ditutup: serialisasi → deserialisasi.
#[test]
fn setelan_bertahan_setelah_dibuka_ulang() {
    let mut a = AppState::new(paths(), Persisted::default());
    a.theme = ThemeMode::Dark;
    a.font_scale = FontScale::Large;
    a.lang = Lang::Ja;
    a.tab = Tab::Terminal;
    a.sidebar_open = false;

    let json = serde_json::to_string(&a.persist()).unwrap();
    let loaded: Persisted = serde_json::from_str(&json).unwrap();
    let b = AppState::new(paths(), loaded);

    assert_eq!(b.theme, ThemeMode::Dark);
    assert_eq!(b.font_scale, FontScale::Large);
    assert_eq!(b.lang, Lang::Ja);
    assert_eq!(b.tab, Tab::Terminal);
    assert!(!b.sidebar_open);
}

// ── Kriteria "selesai bila" M4 ──────────────────────────────────────────────

fn pesan(id: &str, role: &str, content: &str) -> MsgView {
    MsgView {
        id: id.into(),
        role: role.into(),
        content: content.into(),
        created_at: 0,
        attachments: Vec::new(),
    }
}

fn baris(id: &str, title: &str) -> SessionRow {
    SessionRow {
        id: id.into(),
        title: title.into(),
        subtitle: "0 pesan".into(),
    }
}

/// "Bubble user/AI warna beda" — diperiksa dari warna yang benar-benar
/// dilukis, bukan dari nama variabel.
#[test]
fn chat_menampilkan_bubble_user_dan_ai_dengan_warna_beda() {
    let mut h = Harness::with_default_state();
    h.state_mut().theme = ThemeMode::Dark;
    h.state_mut().chat.messages = vec![
        pesan("1", "user", "Halo arka"),
        pesan("2", "assistant", "Hai juga"),
    ];

    let shapes = h.frame();
    find_text(&shapes, "Halo arka");
    find_text(&shapes, "Hai juga");

    assert!(
        fill_count(&shapes, theme::DARK.user_bubble) >= 1,
        "bubble user tidak memakai warna user_bubble"
    );
    assert!(
        fill_count(&shapes, theme::DARK.ai_bubble) >= 1,
        "bubble AI tidak memakai warna ai_bubble"
    );
}

/// Blok kode harus tampil terpisah dari teks, lengkap dengan label bahasanya.
#[test]
fn chat_merender_blok_kode_terpisah() {
    let mut h = Harness::with_default_state();
    h.state_mut().chat.messages = vec![pesan(
        "1",
        "assistant",
        "Contoh:\n```rust\nfn main() {}\n```",
    )];

    let shapes = h.frame();
    find_text(&shapes, "rust");
    find_text(&shapes, "fn main() {}");
}

/// Indikator "sedang berpikir" muncul sebelum token pertama tiba.
#[test]
fn indikator_mengetik_muncul_sebelum_token() {
    let mut h = Harness::with_default_state();
    h.state_mut().active_session = Some("s1".into());
    h.state().chat.engine.mark_running("s1");

    let shapes = h.frame();
    find_text(&shapes, "AI sedang berpikir");
}

/// Tombol kirim tanpa sesi memberi tahu, bukan diam saja.
#[test]
fn kirim_tanpa_sesi_memberi_notice() {
    let mut h = Harness::with_default_state();
    h.state_mut().draft = "halo".into();

    let shapes = h.frame();
    let rect = find_text(&shapes, "Kirim");
    h.click(rect.center());
    h.frame();

    let notice = h.state().notice.clone().unwrap_or_default();
    assert!(
        notice.contains("Buat sesi"),
        "notice tak sesuai: {notice:?}"
    );
    // Draf tidak boleh hilang saat pengiriman gagal.
    assert_eq!(h.state().draft, "halo");
}

/// Drawer sesi: klik baris membuka sesi itu.
#[test]
fn sidebar_membuka_sesi_yang_diklik() {
    let mut h = Harness::with_default_state();
    h.state_mut().sessions = vec![baris("a", "Sesi A"), baris("b", "Sesi B")];

    let shapes = h.frame();
    let rect = find_text(&shapes, "Sesi A");
    h.click(rect.center());
    h.frame();

    assert_eq!(h.state().active_session.as_deref(), Some("a"));
}

/// Kartu persetujuan menggantung di atas layar dan tombolnya menjawab.
#[test]
fn kartu_persetujuan_bisa_disetujui() {
    let mut h = Harness::with_default_state();
    h.state_mut().chat.approval = Some(ApprovalCard {
        id: "apv-uji".into(),
        action: "Jalankan perintah".into(),
        target: "ls -la".into(),
        detail: "menampilkan isi folder".into(),
    });

    // Satu frame untuk mengukur jendela (sama seperti dialog Tentang).
    h.frame();
    let shapes = h.frame();
    find_text(&shapes, "Jalankan perintah");
    let rect = find_text(&shapes, "Setujui");
    h.click(rect.center());
    h.frame();

    assert!(h.state().chat.approval.is_none(), "kartu tidak tertutup");
}

/// Kejadian dari thread agent benar-benar masuk ke state tampilan.
#[test]
fn kejadian_agent_mengalir_ke_state_chat() {
    let mut h = Harness::with_default_state();
    h.state_mut().active_session = Some("s1".into());
    let ctx = h.ctx.clone();

    h.gui.handle(
        Msg::Delta {
            session: "s1".into(),
            text: "ha".into(),
        },
        &ctx,
    );
    h.gui.handle(
        Msg::Delta {
            session: "s1".into(),
            text: "i".into(),
        },
        &ctx,
    );
    assert_eq!(h.state().chat.streaming, "hai");

    // Token dari sesi lain tidak boleh bocor ke sesi yang sedang dibuka.
    h.gui.handle(
        Msg::Delta {
            session: "s2".into(),
            text: "BOCO R".into(),
        },
        &ctx,
    );
    assert_eq!(h.state().chat.streaming, "hai");

    h.gui.handle(
        Msg::Tool {
            session: "s1".into(),
            label: "[Sesi A] PS>".into(),
            display: "Get-ChildItem".into(),
        },
        &ctx,
    );
    assert!(h.state().chat.terminal_log.contains("Get-ChildItem"));

    h.gui.handle(
        Msg::Approval {
            id: "apv-1".into(),
            action: "Jalankan".into(),
            target: "ls".into(),
            detail: String::new(),
        },
        &ctx,
    );
    assert!(h.state().chat.approval.is_some());

    h.gui.handle(
        Msg::Finished {
            session: "s1".into(),
        },
        &ctx,
    );
    assert!(h.state().chat.streaming.is_empty());
}

// ── Kriteria "selesai bila" M5 (tab Provider) ───────────────────────────────

fn baris_provider(id: &str, nama: &str, has_key: bool, unreadable: bool) -> ProvRow {
    // Statistik sengaja berbeda per baris: kalau sama, teksnya tergambar
    // berkali-kali dan test tidak bisa membuktikan baris mana yang benar.
    let (tokens, requests) = match id {
        "p1" => (12_345, 3),
        "p3" => (999, 1),
        _ => (0, 0),
    };
    ProvRow {
        id: id.into(),
        name: nama.into(),
        kind: "openai".into(),
        base_url: "https://api.contoh/v1".into(),
        model: "gpt-uji".into(),
        is_default: id == "p1",
        has_key,
        key_unreadable: unreadable,
        total_tokens: tokens,
        total_requests: requests,
    }
}

/// Daftar menampilkan yang penting tanpa membocorkan kunci: status "key terisi"
/// (bukan nilainya), jenis, model, dan statistik token yang sudah diringkas.
#[test]
fn provider_daftar_menampilkan_status_kunci_dan_statistik() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Provider;
    h.state_mut().provider.rows = vec![
        baris_provider("p1", "OpenAI", true, false),
        baris_provider("p2", "Kosong", false, false),
        baris_provider("p3", "Mesin lain", true, true),
    ];

    let shapes = h.frame();
    find_text(&shapes, "OpenAI");
    find_text(&shapes, "key terisi");
    find_text(&shapes, "key kosong");
    find_text(&shapes, "kunci tak terbaca");
    // Statistik: 12.345 token → "12.3K", bukan angka mentah.
    find_text(&shapes, "\u{1F4CA} 12.3K tokens \u{00B7} 3 req");
    // Di bawah seribu tetap apa adanya; nol tidak ditampilkan sama sekali.
    find_text(&shapes, "\u{1F4CA} 999 tokens \u{00B7} 1 req");
    assert!(
        texts(&shapes)
            .iter()
            .all(|(_, t)| !t.contains("tokens") || !t.starts_with("\u{1F4CA} 0")),
        "baris tanpa pemakaian tidak boleh menampilkan statistik nol"
    );
    assert!(
        texts(&shapes).iter().all(|(_, t)| !t.contains("sk-")),
        "nilai API key tidak boleh tergambar di daftar"
    );
}

/// "Tambah" membuka form; seluruh field yang wajib ada ikut tergambar.
#[test]
fn provider_tambah_membuka_form() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Provider;
    let shapes = h.frame();
    let rect = find_text(&shapes, "+ Tambah");
    h.click(rect.center());
    let shapes = h.frame();

    assert!(h.state().provider.form.is_some(), "form tidak terbuka");
    find_text(&shapes, "Tambah provider");
    find_text(&shapes, "Nama");
    find_text(&shapes, "Base URL");
    find_text(&shapes, "API key");
    find_text(&shapes, "Model utama");
}

/// Provider baru tanpa nama: tombol Simpan tidak boleh menyimpan apa pun
/// (dan form tetap terbuka supaya pengguna bisa memperbaiki).
#[test]
fn provider_simpan_tanpa_nama_tidak_menutup_form() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Provider;
    h.state_mut().open_provider_form(None);
    let shapes = h.frame();
    let rect = find_text(&shapes, "Simpan");
    h.click(rect.center());
    h.frame();

    assert!(
        h.state().provider.form.is_some(),
        "form tertutup tanpa nama"
    );
    assert!(
        h.state().provider.error.is_none(),
        "tombol mati seharusnya tidak menyentuh database: {:?}",
        h.state().provider.error
    );
}

/// Batal menutup form tanpa menyimpan.
#[test]
fn provider_batal_menutup_form() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Provider;
    h.state_mut().open_provider_form(None);
    let shapes = h.frame();
    let rect = find_text(&shapes, "Batal");
    h.click(rect.center());
    h.frame();

    assert!(h.state().provider.form.is_none(), "form tidak tertutup");
}

/// Hapus harus minta konfirmasi dulu, dan Batal membatalkannya.
#[test]
fn provider_hapus_meminta_konfirmasi() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Provider;
    h.state_mut().provider.rows = vec![baris_provider("p1", "OpenAI", true, false)];

    let shapes = h.frame();
    let rect = find_text(&shapes, "Hapus");
    h.click(rect.center());
    let shapes = h.frame();

    assert_eq!(
        h.state()
            .provider
            .confirm_delete
            .as_ref()
            .map(|(id, _)| id.clone()),
        Some("p1".to_string())
    );
    find_text(&shapes, "Hapus OpenAI?");

    let rect = find_text(&shapes, "Batal");
    h.click(rect.center());
    h.frame();
    assert!(
        h.state().provider.confirm_delete.is_none(),
        "konfirmasi tidak batal"
    );
}

/// Kunci yang tidak bisa dibuka DPAPI di mesin ini diberi peringatan eksplisit
/// di form — bukan ditampilkan sebagai kunci kosong biasa yang bisa terhapus
/// tanpa sadar.
#[test]
fn provider_form_memberi_peringatan_kunci_tak_terbaca() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Provider;
    h.state_mut().provider.form = Some(ProviderForm {
        id: "p1".into(),
        name: "Dari mesin lain".into(),
        key_unreadable: true,
        ..Default::default()
    });

    let shapes = h.frame();
    find_text(&shapes, "Ubah provider");
    assert!(
        texts(&shapes)
            .iter()
            .any(|(_, t)| t.contains("tidak bisa dibuka di mesin ini")),
        "peringatan DPAPI tidak tergambar. Yang ada: {:?}",
        texts(&shapes)
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
    );
}

/// Mengetik nama mengaktifkan Simpan; tanpa nama tombolnya mati.
#[test]
fn provider_tombol_simpan_mati_tanpa_nama() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Provider;
    h.state_mut().provider.form = Some(ProviderForm {
        id: String::new(),
        name: "Ada nama".into(),
        ..Default::default()
    });

    let shapes = h.frame();
    let rect = find_text(&shapes, "Simpan");
    // Dengan nama terisi, klik harus benar-benar mencoba menyimpan. Tanpa
    // database, kegagalannya muncul sebagai pesan di form — bukti tombol aktif.
    h.click(rect.center());
    let shapes = h.frame();
    assert!(
        h.state()
            .provider
            .form
            .as_ref()
            .and_then(|f| f.error.clone())
            .is_some(),
        "klik Simpan tidak melakukan apa pun. Tergambar: {:?}",
        texts(&shapes)
            .iter()
            .map(|(_, t)| t.as_str())
            .collect::<Vec<_>>()
    );
}

// ── M7: Terminal ────────────────────────────────────────────────────────────

fn key(k: egui::Key) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    }
}

/// Tab Terminal menampilkan kotak perintah sungguhan dan contoh perintah
/// **PowerShell** (bukan bash) yang bisa diklik.
#[test]
fn terminal_punya_kotak_perintah_dan_contoh_powershell() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    let shapes = h.frame();

    // Judul seksi dilukis huruf besar oleh `widgets::section`.
    find_text(&shapes, "KOTAK PERINTAH");
    find_text(&shapes, "CONTOH PERINTAH");
    find_text(&shapes, "Jalankan");
    // Hint dari daftar PowerShell; `ls -la` akan langsung gagal di Windows.
    find_text(&shapes, "git status");
    assert!(
        texts(&shapes)
            .iter()
            .any(|(_, t)| t.contains("Get-ChildItem")),
        "contoh perintah PowerShell tidak tergambar"
    );
}

/// ↑ mengisi kotak perintah dari riwayat, ↓ kembali. Ini jalur yang paling
/// sering dipakai saat mengulang perintah panjang.
#[test]
fn panah_atas_dan_bawah_menelusuri_riwayat() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.history.push("git status");
    h.state_mut().term.history.push("Get-ChildItem");
    h.state_mut().term.focus_input = true;
    h.frame(); // permintaan fokus diproses di frame ini
    h.frame(); // kotak perintah kini benar-benar punya fokus

    h.frame_with(vec![key(egui::Key::ArrowUp)]);
    assert_eq!(h.state().term.input, "Get-ChildItem", "↑ pertama = terbaru");

    h.frame_with(vec![key(egui::Key::ArrowUp)]);
    assert_eq!(h.state().term.input, "git status");

    h.frame_with(vec![key(egui::Key::ArrowDown)]);
    assert_eq!(h.state().term.input, "Get-ChildItem");

    h.frame_with(vec![key(egui::Key::ArrowDown)]);
    assert!(
        h.state().term.input.is_empty(),
        "melewati yang terbaru = kembali ke draft"
    );
    assert!(!h.state().term.history.is_navigating());
}

/// Saat perintah berjalan, yang tampil adalah Stop — bukan dua tombol yang
/// bisa ditekan bersamaan.
#[test]
fn tombol_stop_menggantikan_jalankan_saat_berjalan() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.job = Some(crate::gui::term_job::Job::running_stub(
        crate::gui::term_job::TaskKind::Run,
        "git clone https://contoh/repo.git",
    ));
    let shapes = h.frame();

    find_text(&shapes, "Stop");
    assert!(
        !texts(&shapes).iter().any(|(_, t)| t == "Jalankan"),
        "tombol Jalankan harus hilang selama perintah berjalan"
    );
    // Perintah yang sedang berjalan ditampilkan sebagai gema `PS>`.
    assert!(texts(&shapes).iter().any(|(_, t)| t.contains("git clone")));
    // Output yang sudah keluar tetap terlihat selama berjalan.
    assert!(texts(&shapes).iter().any(|(_, t)| t == "setengah jalan"));
}

/// Exit code ditampilkan; yang bukan nol berwarna bahaya, yang nol berwarna
/// sukses. Warna dibuktikan dari bentuk yang dilukis.
#[test]
fn exit_code_ditampilkan_dengan_warna_yang_benar() {
    use crate::gui::term_job::ExitLine;

    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.output = "fatal: not a git repository".into();
    // Tema dipatok eksplisit: `System` mengikuti tema OS, sehingga warna yang
    // dilukis bergantung pada mesin yang menjalankan test. Percobaan pertama
    // membandingkan dengan `DARK` tanpa mematok tema dan **gagal di Linux**
    // (di sana OS dianggap terang). Yang ingin dibuktikan bukan "warnanya
    // kebetulan sama", melainkan "warna keluar yang dilukis = warna bahaya /
    // sukses dari tema yang sedang aktif".
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        h.state_mut().theme = mode;
        let palette = mode.colors(false);

        h.state_mut().term.exit = Some(ExitLine::new(128, false, false));
        let shapes = h.frame();
        find_text(&shapes, "exit 128");
        assert_eq!(
            shape_color(&shapes, "exit 128"),
            palette.danger,
            "exit bukan nol harus berwarna bahaya pada tema {mode:?}"
        );

        h.state_mut().term.exit = Some(ExitLine::new(0, false, false));
        let shapes = h.frame();
        find_text(&shapes, "exit 0");
        assert_eq!(
            shape_color(&shapes, "exit 0"),
            palette.success,
            "exit nol harus berwarna sukses pada tema {mode:?}"
        );
    }
    h.state_mut().theme = ThemeMode::Dark;

    h.state_mut().term.exit = Some(ExitLine::new(-1, true, false));
    let shapes = h.frame();
    find_text(&shapes, "exit -1 · dihentikan pengguna");
}

/// "Bersihkan output" benar-benar mengosongkan panel — dan tidak menyentuh
/// riwayat perintah (itu tombol lain).
#[test]
fn bersihkan_output_tidak_menghapus_riwayat() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.output = "hasil lama".into();
    h.state_mut().term.history.push("git status");
    let shapes = h.frame();

    h.click(find_text(&shapes, "Bersihkan output").center());
    h.frame();

    assert!(h.state().term.output.is_empty());
    assert_eq!(
        h.state().term.history.len(),
        1,
        "riwayat tidak boleh ikut terhapus"
    );
}

/// Dropdown riwayat: perintah terbaru tampil paling atas dan bisa diklik
/// untuk dimasukkan ke kotak perintah.
#[test]
fn riwayat_bisa_dibuka_dan_dipilih() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.history.push("git status");
    h.state_mut().term.history.push("npm install");
    let shapes = h.frame();

    h.click(find_text(&shapes, "Riwayat (2)").center());
    let shapes = h.frame();
    let item = find_text(&shapes, "npm install");
    h.click(item.center());
    h.frame();

    assert_eq!(h.state().term.input, "npm install");
    assert!(
        !h.state().term.show_history,
        "dropdown menutup setelah dipilih"
    );
}

/// Statistik workspace tampil setelah dihitung (di thread latar).
#[test]
fn isi_workspace_ditampilkan_setelah_dihitung() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.ws = Some(crate::gui::term_job::WsStats {
        files: 3,
        bytes: 2048,
    });

    let shapes = h.frame();
    assert!(
        texts(&shapes)
            .iter()
            .any(|(_, t)| t.contains("berkas") && t.contains("2,0 KB")),
        "statistik workspace tidak tergambar: {:?}",
        texts(&shapes)
            .iter()
            .map(|(_, t)| t.as_str())
            .filter(|t| t.contains("KB") || t.contains("berkas"))
            .collect::<Vec<_>>()
    );
}

/// Kartu status menampilkan nilai PowerShell yang sudah diambil, bukan "—".
/// Datanya diisi langsung supaya test tidak menjalankan proses apa pun.
#[test]
fn kartu_status_menampilkan_versi_powershell() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.info = crate::gui::state::InfoState::Ready(vec![
        ("PSVersion".into(), "7.4.6".to_string()),
        ("PSEdition".into(), "Core".to_string()),
        ("Arch".into(), "X64".to_string()),
    ]);

    let shapes = h.frame();
    for nilai in ["7.4.6", "Core", "X64"] {
        find_text(&shapes, nilai);
    }
    // Tombol "Buka di Explorer" hanya ada di kartu ini.
    find_text(&shapes, "Buka di Explorer");
}

/// Tombol "Tes shell" menampilkan hasil diagnostik apa adanya — termasuk saat
/// shell belum terdeteksi, yang harus muncul sebagai pesan, bukan diam saja.
#[test]
fn tes_shell_menampilkan_hasil_diagnostik() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    // Shell dipatok ke keadaan "tidak ditemukan" supaya tata letak **tidak
    // bergeser** saat deteksi latar selesai. Percobaan pertama membiarkannya
    // `Unknown`: deteksi selesai di antara dua frame, kartu status tumbuh
    // (pesan galat panjang), dan klik pada koordinat lama jadi meleset.
    h.state_mut().shell =
        crate::gui::state::ShellProbe::Missing("powershell.exe tidak ada (uji)".to_string());

    let shapes = h.frame();
    h.click(find_text(&shapes, "Tes shell").center());
    let shapes = h.frame();

    // Judul bagian digambar oleh `widgets::section` dalam huruf besar.
    find_text(&shapes, "HASIL TES SHELL");
    let teks = texts(&shapes);
    // Di Linux `powershell.exe` tidak ada, jadi hasil tesnya pesan galat —
    // yang penting pesan itu **sampai ke layar** dengan penanda `!`, bukan
    // gagal diam-diam seperti percobaan pertama.
    assert!(
        teks.iter().any(|(_, t)| t.starts_with("! ")),
        "pesan galat diagnostik tidak tergambar: {:?}",
        teks.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>()
    );

    // Kartu bisa ditutup dan tidak muncul lagi.
    h.click(find_text(&shapes, "Tutup").center());
    let shapes = h.frame();
    assert!(
        !texts(&shapes).iter().any(|(_, t)| t == "HASIL TES SHELL"),
        "kartu diagnostik harus hilang setelah ditutup"
    );
}

/// Tombol "Ulangi" mengisi kotak dengan Perintah terakhir yang dijalankan.
/// Percobaan pertama memakai potongan *output* sebagai label, sementara
/// klik-nya mengulang perintah dari riwayat — label dan perbuatan berbeda.
#[test]
fn tombol_ulangi_mengisi_perintah_terakhir() {
    let mut h = Harness::with_default_state();
    h.state_mut().tab = Tab::Terminal;
    h.screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 1800.0));
    h.state_mut().term.history.push("git status");
    h.state_mut().term.output = "On branch main".into();

    let shapes = h.frame();
    let tombol = texts(&shapes)
        .into_iter()
        .find(|(_, t)| t.starts_with("Ulangi:"))
        .unwrap_or_else(|| panic!("tombol Ulangi tidak ada: {:?}", texts(&shapes)));
    assert!(
        tombol.1.contains("git status"),
        "label harus perintah terakhir, bukan output: {}",
        tombol.1
    );

    h.click(tombol.0.center());
    assert_eq!(h.state().term.input, "git status");
}

/// Tombol X → tray hanya berlaku kalau setelan "jalan di latar belakang" aktif.
#[test]
fn jendela_disembunyikan_ke_tray_hanya_bila_diminta() {
    let mut h = Harness::with_default_state();

    h.state_mut().prefs.background_run = false;
    let (_, canceled) = h.frame_closing();
    assert!(
        !canceled && !h.gui.hidden_for_test(),
        "tanpa setelan background, X harus menutup aplikasi"
    );

    h.state_mut().prefs.background_run = true;
    let (_, canceled) = h.frame_closing();
    assert!(canceled, "X harus dibatalkan saat jalan di latar belakang");
    assert!(
        h.gui.hidden_for_test(),
        "jendela harus ditandai tersembunyi"
    );

    let ctx = h.ctx.clone();
    h.gui.show_window_for_test(&ctx);
    assert!(
        !h.gui.hidden_for_test(),
        "tray Show harus memunculkan jendela"
    );
}
