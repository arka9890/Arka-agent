//! Tema "claymorphism" — port 1:1 dari `android/.../ui/theme/Theme.kt`.
//!
//! Semua warna diambil apa adanya dari `DarkClay` / `LightClay` supaya versi
//! Windows tidak "mirip" tapi identik. Tidak ada warna yang dikarang di sini —
//! dan itu dijaga test `palet_sama_dengan_kotlin`, yang membaca berkas Kotlin
//! asli saat kompilasi. Kalau ada yang mengubah palet di satu sisi saja, test
//! merah.
//!
//! Ukuran teks: setelan "Ukuran teks" di Setelan harus benar-benar terlihat.
//! Karena itu **tidak ada** `.size(14.0)` mentah di layar mana pun; semuanya
//! lewat [`fs`], yang mengikuti skala aktif.

use eframe::egui::{
    self, Color32, Context, CornerRadius, FontFamily, FontId, Shadow, Stroke, TextStyle, Theme,
    ThemePreference, Ui, Visuals,
};
use std::sync::Arc;

/// Jarak standar, sama dengan `object Space` di Kotlin.
pub const SP_XS: f32 = 4.0;
pub const SP_SM: f32 = 8.0;
pub const SP_MD: f32 = 12.0;
pub const SP_LG: f32 = 16.0;
pub const SP_XL: f32 = 24.0;

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb(
        ((hex >> 16) & 0xFF) as u8,
        ((hex >> 8) & 0xFF) as u8,
        (hex & 0xFF) as u8,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClayColors {
    pub background: Color32,
    pub surface: Color32,
    pub surface_raised: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_surface: Color32,
    pub muted: Color32,
    pub shadow_dark: Color32,
    pub shadow_light: Color32,
    pub success: Color32,
    pub danger: Color32,
    pub user_bubble: Color32,
    pub ai_bubble: Color32,
}

pub const DARK: ClayColors = ClayColors {
    background: rgb(0x0E1015),
    surface: rgb(0x171A22),
    surface_raised: rgb(0x1E222C),
    accent: rgb(0x8B7CF6),
    accent_soft: rgb(0x2A2547),
    on_surface: rgb(0xEDEFF5),
    muted: rgb(0x8E94A6),
    shadow_dark: rgb(0x05060A),
    shadow_light: rgb(0x262B37),
    success: rgb(0x4ADE80),
    danger: rgb(0xF87171),
    user_bubble: rgb(0x8B7CF6),
    ai_bubble: rgb(0x1E222C),
};

pub const LIGHT: ClayColors = ClayColors {
    background: rgb(0xF2F3F8),
    surface: rgb(0xFBFBFE),
    surface_raised: rgb(0xFFFFFF),
    accent: rgb(0x6C5CE7),
    accent_soft: rgb(0xE4E0FF),
    on_surface: rgb(0x171A22),
    muted: rgb(0x6B7280),
    shadow_dark: rgb(0xC7CAD8),
    shadow_light: rgb(0xFFFFFF),
    success: rgb(0x16A34A),
    danger: rgb(0xDC2626),
    user_bubble: rgb(0x6C5CE7),
    ai_bubble: rgb(0xFFFFFF),
};

// ── Skala teks ──────────────────────────────────────────────────────────────

/// Skala teks disimpan di **data milik `Context`**, bukan di variabel global.
///
/// Percobaan pertama memakai `AtomicU32` global dan test-nya gagal di CI
/// Windows: test berjalan paralel dalam satu proses, jadi satu test bisa
/// mengubah skala yang sedang dibaca test lain. Data per-`Context` membuat tiap
/// jendela (dan tiap test) punya skalanya sendiri.
fn scale_id() -> egui::Id {
    egui::Id::new("arka.text_scale")
}

pub fn set_scale(ctx: &Context, scale: f32) {
    ctx.data_mut(|d| d.insert_temp(scale_id(), scale));
}

/// Skala aktif; 1.0 bila belum pernah dipasang (mis. saat test memanggil
/// widget tanpa `theme::apply`).
pub fn scale_of(ctx: &Context) -> f32 {
    ctx.data(|d| d.get_temp::<f32>(scale_id())).unwrap_or(1.0)
}

/// Ukuran teks (pt) yang mengikuti setelan "Ukuran teks".
pub fn fs_of(ctx: &Context, base: f32) -> f32 {
    base * scale_of(ctx)
}

/// Sama dengan [`fs_of`], untuk dipakai di dalam layar/widget.
pub fn fs(ui: &Ui, base: f32) -> f32 {
    fs_of(ui.ctx(), base)
}

/// Pilihan tema di Setelan. `System` = ikut tema Windows (padanan
/// `isSystemInDarkTheme()` di Android).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ThemeMode {
    #[default]
    System,
    Dark,
    Light,
}

impl ThemeMode {
    pub const ALL: [ThemeMode; 3] = [ThemeMode::System, ThemeMode::Dark, ThemeMode::Light];

    pub fn is_dark(self, system_dark: bool) -> bool {
        match self {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => system_dark,
        }
    }

    pub fn colors(self, system_dark: bool) -> &'static ClayColors {
        if self.is_dark(system_dark) {
            &DARK
        } else {
            &LIGHT
        }
    }

    pub fn preference(self) -> ThemePreference {
        match self {
            ThemeMode::System => ThemePreference::System,
            ThemeMode::Dark => ThemePreference::Dark,
            ThemeMode::Light => ThemePreference::Light,
        }
    }

    /// Label ditampilkan lewat i18n; ini hanya kunci stabil.
    #[allow(dead_code)] // dipakai saat setelan disimpan ke `arka_core` (M8).
    pub fn key(self) -> &'static str {
        match self {
            ThemeMode::System => "system",
            ThemeMode::Dark => "dark",
            ThemeMode::Light => "light",
        }
    }
}

/// Skala font dari Setelan. Di Android (`fontScaleFor`): 0.9/1.0/1.15.
/// Di desktop dipakai 0.85/1.0/1.15 sesuai todo M3 — di layar besar selisih
/// 0.9 terlalu tipis untuk terlihat, dan justru itu bug yang pernah terjadi di
/// versi Android ("memilih Kecil/Besar tidak mengubah apa pun").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FontScale {
    Small,
    #[default]
    Medium,
    Large,
}

impl FontScale {
    pub const ALL: [FontScale; 3] = [FontScale::Small, FontScale::Medium, FontScale::Large];

    pub fn factor(self) -> f32 {
        match self {
            FontScale::Small => 0.85,
            FontScale::Medium => 1.0,
            FontScale::Large => 1.15,
        }
    }

    #[allow(dead_code)] // dipakai saat setelan disimpan ke `arka_core` (M8).
    pub fn key(self) -> &'static str {
        match self {
            FontScale::Small => "small",
            FontScale::Medium => "medium",
            FontScale::Large => "large",
        }
    }
}

/// Pasang font yang dibundel. Inter (proporsional) + JetBrains Mono (kode).
///
/// Font bundel dipakai supaya tampilan sama di semua mesin Windows — tanpa
/// ini, `Segoe UI` di satu PC dan `Arial` di PC lain membuat lebar teks
/// berbeda dan layout bergeser.
pub fn install_fonts(ctx: &Context) {
    let mut fonts = egui::FontDefinitions::default();

    let inter = include_bytes!("../../assets/fonts/InterVariable.ttf");
    let mono = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");

    fonts.font_data.insert(
        "inter".to_owned(),
        Arc::new(egui::FontData::from_static(inter)),
    );
    fonts.font_data.insert(
        "jetbrains".to_owned(),
        Arc::new(egui::FontData::from_static(mono)),
    );

    // Teks Latin pakai Inter, sisanya (emoji/CJK) tetap dari font bawaan egui
    // sebagai fallback — jangan dihapus, kalau tidak emoji jadi kotak.
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "inter".to_owned());
    fonts
        .families
        .entry(FontFamily::Monospace)
        .or_default()
        .insert(0, "jetbrains".to_owned());

    ctx.set_fonts(fonts);
}

/// Palet → `Visuals` egui.
fn visuals(c: &ClayColors, dark: bool) -> Visuals {
    let mut v = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    v.panel_fill = c.background;
    v.window_fill = c.surface;
    v.extreme_bg_color = c.background;
    v.faint_bg_color = c.surface_raised;
    v.override_text_color = Some(c.on_surface);
    v.hyperlink_color = c.accent;
    v.warn_fg_color = c.danger;

    v.selection.bg_fill = c.accent_soft;
    v.selection.stroke = Stroke::new(1.0_f32, c.accent);

    v.window_corner_radius = CornerRadius::same(18);
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 28,
        spread: 0,
        color: Color32::from_black_alpha(if dark { 140 } else { 60 }),
    };
    v.popup_shadow = v.window_shadow;

    // "Timbul" ala clay: permukaan lebih terang dari latar + garis tepi tipis.
    v.widgets.noninteractive.bg_fill = c.surface;
    v.widgets.noninteractive.weak_bg_fill = c.surface;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, c.shadow_light.gamma_multiply(0.08));
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, c.on_surface);

    v.widgets.inactive.bg_fill = c.surface_raised;
    v.widgets.inactive.weak_bg_fill = c.surface_raised;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, c.on_surface);
    v.widgets.inactive.corner_radius = CornerRadius::same(14);

    v.widgets.hovered.bg_fill = c.accent_soft;
    v.widgets.hovered.weak_bg_fill = c.accent_soft;
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, c.accent);
    v.widgets.hovered.corner_radius = CornerRadius::same(14);

    v.widgets.active.bg_fill = c.accent;
    v.widgets.active.weak_bg_fill = c.accent;
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.widgets.active.corner_radius = CornerRadius::same(14);

    v.widgets.open = v.widgets.active;
    v
}

/// Terapkan palet, pilihan tema, dan skala teks.
///
/// Visuals dipasang untuk **kedua** tema sekaligus (`set_visuals_of`) dan
/// ukuran teks lewat `all_styles_mut`, bukan `style_mut`. Alasannya nyata:
/// `style_mut`/`set_visuals` hanya menyentuh tema yang sedang aktif, jadi saat
/// Windows beralih terang↔gelap — atau saat pengguna memilih "Gelap" pada
/// Windows bermode terang — sebagian widget (kotak teks, scrollbar, menu)
/// kembali ke visual bawaan egui.
pub fn apply(ctx: &Context, scale: f32, mode: ThemeMode) {
    set_scale(ctx, scale);
    ctx.set_theme(mode.preference());

    ctx.set_visuals_of(Theme::Dark, visuals(&DARK, true));
    ctx.set_visuals_of(Theme::Light, visuals(&LIGHT, false));

    let s = scale;
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(26.0 * s, FontFamily::Proportional),
            ),
            (
                TextStyle::Body,
                FontId::new(15.0 * s, FontFamily::Proportional),
            ),
            (
                TextStyle::Button,
                FontId::new(15.0 * s, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.0 * s, FontFamily::Proportional),
            ),
            (
                TextStyle::Monospace,
                FontId::new(14.0 * s, FontFamily::Monospace),
            ),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(SP_SM * s, SP_SM * s);
        style.spacing.button_padding = egui::vec2(14.0 * s, 8.0 * s);
        style.spacing.interact_size.y = 32.0 * s;
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Berkas Kotlin dibaca saat kompilasi: kalau palet di Android berubah dan
    /// di sini tidak (atau sebaliknya), test ini merah.
    const KOTLIN_THEME: &str =
        include_str!("../../../../android/app/src/main/java/com/arka/agent/ui/theme/Theme.kt");

    /// Ambil `0xFFRRGGBB` untuk field tertentu di dalam blok `DarkClay` /
    /// `LightClay`.
    fn kotlin_hex(block: &str, field: &str) -> u32 {
        let start = KOTLIN_THEME
            .find(block)
            .unwrap_or_else(|| panic!("blok {block} tidak ada"));
        let body = &KOTLIN_THEME[start..];
        // Blok ditutup oleh `)` di awal baris; `Color(0xFF...)` di dalamnya
        // juga memakai tanda kurung, jadi tidak boleh berhenti di `)` pertama.
        let body = &body[..body.find("\n)").expect("blok tidak ditutup")];
        let key = format!("{field} = Color(0xFF");
        let at = body
            .find(&key)
            .unwrap_or_else(|| panic!("field {field} tidak ada di {block}"));
        let hex = &body[at + key.len()..at + key.len() + 6];
        u32::from_str_radix(hex, 16).unwrap_or_else(|e| panic!("hex {hex} rusak: {e}"))
    }

    fn assert_palet(block: &str, c: &ClayColors) {
        let pairs: [(&str, Color32); 13] = [
            ("background", c.background),
            ("surface", c.surface),
            ("surfaceRaised", c.surface_raised),
            ("accent", c.accent),
            ("accentSoft", c.accent_soft),
            ("onSurface", c.on_surface),
            ("muted", c.muted),
            ("shadowDark", c.shadow_dark),
            ("shadowLight", c.shadow_light),
            ("success", c.success),
            ("danger", c.danger),
            ("userBubble", c.user_bubble),
            ("aiBubble", c.ai_bubble),
        ];
        for (field, actual) in pairs {
            let want = rgb(kotlin_hex(block, field));
            assert_eq!(
                actual, want,
                "{block}.{field}: Kotlin {want:?} != Rust {actual:?}"
            );
        }
    }

    #[test]
    fn palet_sama_dengan_kotlin() {
        assert_palet("DarkClay", &DARK);
        assert_palet("LightClay", &LIGHT);
    }

    #[test]
    fn skala_teks_mengubah_ukuran() {
        // Context sendiri: skala tidak lagi global, jadi test ini tidak bisa
        // "dicemari" test lain yang berjalan paralel.
        let ctx = Context::default();

        // Sebelum dipasang, widget tetap bisa dipakai: skala 1.0.
        assert!((scale_of(&ctx) - 1.0).abs() < 0.001);
        assert!((fs_of(&ctx, 14.0) - 14.0).abs() < 0.001);

        set_scale(&ctx, FontScale::Large.factor());
        assert!((fs_of(&ctx, 14.0) - 16.1).abs() < 0.001);
        assert!((scale_of(&ctx) - 1.15).abs() < 0.001);

        set_scale(&ctx, FontScale::Small.factor());
        assert!((fs_of(&ctx, 20.0) - 17.0).abs() < 0.001);

        // Context lain tidak terpengaruh.
        let other = Context::default();
        assert!((fs_of(&other, 20.0) - 20.0).abs() < 0.001);
    }

    #[test]
    fn mode_tema_menentukan_palet() {
        assert_eq!(ThemeMode::Dark.colors(false), &DARK);
        assert_eq!(ThemeMode::Light.colors(true), &LIGHT);
        assert_eq!(ThemeMode::System.colors(true), &DARK);
        assert_eq!(ThemeMode::System.colors(false), &LIGHT);
    }
}
