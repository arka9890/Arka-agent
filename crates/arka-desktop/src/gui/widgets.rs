//! Widget "clay" — port `android/.../ui/components/Clay.kt`.
//!
//! Prinsipnya sama: satu tempat untuk "rasa" visual (radius besar, bayangan
//! dua arah, garis tepi tipis). Kalau mau mengubah tampilan seluruh aplikasi,
//! cukup ubah berkas ini.
//!
//! Semua ukuran teks lewat `theme::fs(...)` supaya setelan ukuran teks
//! berpengaruh di sini juga, bukan cuma di `ui.label` biasa.

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontFamily, FontId, Layout, Margin, Response, RichText,
    Shadow, Stroke, Ui, Vec2,
};

use super::theme::{fs, scale, ClayColors, SP_SM, SP_XS};

/// Bayangan clay: gelap di bawah (jatuh) + terang di atas (pantulan).
fn clay_shadow(c: &ClayColors, elevation: u8) -> Shadow {
    Shadow {
        offset: [0, (elevation / 2).max(1) as i8],
        blur: elevation,
        spread: 0,
        color: c.shadow_dark.gamma_multiply(0.55),
    }
}

fn hairline(c: &ClayColors) -> Stroke {
    Stroke::new(1.0_f32, c.shadow_light.gamma_multiply(0.08))
}

/// `ClayCard` — panel utama bergaya clay. `ui` yang diteruskan ke `add_contents`
/// sudah berada di dalam padding kartu.
pub fn clay_card<R>(
    ui: &mut Ui,
    colors: &ClayColors,
    fill: Option<Color32>,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let frame = egui::Frame::new()
        .fill(fill.unwrap_or(colors.surface))
        .corner_radius(CornerRadius::same(22))
        .stroke(hairline(colors))
        .inner_margin(Margin::same((16.0 * scale()) as i8))
        .shadow(clay_shadow(colors, 10));

    frame.show(ui, add_contents).inner
}

/// Varian tanpa padding dalam (dipakai bubble chat & panel terminal).
pub fn clay_panel<R>(
    ui: &mut Ui,
    colors: &ClayColors,
    fill: Color32,
    radius: u8,
    margin: i8,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> R {
    let frame = egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(radius))
        .stroke(hairline(colors))
        .inner_margin(Margin::same(margin))
        .shadow(clay_shadow(colors, 6));

    frame.show(ui, add_contents).inner
}

/// Galley satu baris untuk widget buatan sendiri (pill & tombol).
fn galley(ui: &Ui, text: &str, size: f32, color: Color32) -> std::sync::Arc<egui::Galley> {
    ui.painter()
        .layout_no_wrap(text.to_owned(), FontId::new(fs(size), FontFamily::Proportional), color)
}

/// `ClayPill` — label/status kecil berwarna lembut. Tidak bisa diklik.
pub fn clay_pill(ui: &mut Ui, _colors: &ClayColors, text: &str, color: Color32) -> Response {
    let g = galley(ui, text, 12.0, color);
    let size = g.size() + Vec2::new(20.0 * scale(), 8.0 * scale());

    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(50), color.gamma_multiply(0.16));
        p.rect_stroke(
            rect,
            CornerRadius::same(50),
            Stroke::new(1.0_f32, color.gamma_multiply(0.35)),
            egui::StrokeKind::Inside,
        );
        p.galley(rect.center() - g.size() / 2.0, g, color);
    }
    response
}

/// `ClayPill` yang bisa diklik (mis. "Sesi baru", "Lepas lampiran").
pub fn clay_pill_button(ui: &mut Ui, _colors: &ClayColors, text: &str, color: Color32) -> Response {
    let g = galley(ui, text, 12.0, color);
    let size = g.size() + Vec2::new(22.0 * scale(), 9.0 * scale());
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());

    if ui.is_rect_visible(rect) {
        let bg = if response.hovered() {
            color.gamma_multiply(0.28)
        } else {
            color.gamma_multiply(0.16)
        };
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(50), bg);
        p.rect_stroke(
            rect,
            CornerRadius::same(50),
            Stroke::new(1.0_f32, color.gamma_multiply(0.35)),
            egui::StrokeKind::Inside,
        );
        p.galley(rect.center() - g.size() / 2.0, g, color);
    }

    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Tombol utama (aksen penuh). Saat `enabled == false` tombol diredupkan dan
/// **tidak** bereaksi, sama seperti `enabled = false` di Compose.
pub fn clay_button(ui: &mut Ui, colors: &ClayColors, text: &str, enabled: bool) -> Response {
    let g = galley(ui, text, 15.0, Color32::WHITE);
    let size = Vec2::new(g.size().x + 32.0 * scale(), 36.0 * scale());
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);

    if ui.is_rect_visible(rect) {
        let base = if enabled { colors.accent } else { colors.muted };
        let fill = if !enabled {
            base.gamma_multiply(0.45)
        } else if response.hovered() {
            base.gamma_multiply(1.12)
        } else {
            base
        };
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(14), fill);
        p.rect_stroke(
            rect,
            CornerRadius::same(14),
            hairline(colors),
            egui::StrokeKind::Inside,
        );
        p.galley(rect.center() - g.size() / 2.0, g, Color32::WHITE);
    }

    if enabled && response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    response
}

/// Judul bagian kecil di atas kartu (huruf besar, warna muted).
pub fn section(ui: &mut Ui, colors: &ClayColors, title: &str) {
    ui.add_space(SP_SM * scale());
    ui.label(
        RichText::new(title.to_uppercase())
            .size(fs(11.0))
            .color(colors.muted)
            .strong(),
    );
    ui.add_space(SP_XS);
}

/// Baris "label : nilai" rata kanan-kiri, dipakai kartu status.
pub fn kv(ui: &mut Ui, colors: &ClayColors, key: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(key).size(fs(14.0)).color(colors.muted));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).size(fs(14.0)).color(colors.on_surface));
        });
    });
}

/// Latar halaman: warna dasar + gradasi tipis di sudut atas supaya tidak rata
/// (Kotlin: `clayBackground()`).
pub fn paint_background(ui: &Ui, colors: &ClayColors) {
    let rect = ui.max_rect();
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::ZERO, colors.background);

    let tint = colors.accent.gamma_multiply(0.045);
    let top = egui::Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.height() * 0.45));
    p.rect_filled(top, CornerRadius::ZERO, tint);
}
