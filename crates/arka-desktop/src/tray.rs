//! Ikon tray Windows + menu Buka / Jalan di latar belakang / Keluar.
//!
//! Padanan `AgentForegroundService` di Android, tapi jauh lebih kecil: Windows
//! tidak butuh service, cukup sebuah ikon di notification area dan jendela
//! yang disembunyikan (`ViewportCommand::Visible(false)`) saat tombol X
//! ditekan bila setelan `background_run` aktif.
//!
//! Bagian yang bisa diuji tanpa OS sengaja dipisah: [`Action`] dan
//! [`action_of`] hanyalah pencocokan id menu, jadi CI Linux tetap bisa
//! membuktikan bahwa klik "Keluar" benar-benar berarti keluar.

// Beberapa item di bawah hanya dipakai lewat FFI/UI Windows; pada build
// Linux (CI) jalur itu tidak dikompilasi. Pembatasan ini sengaja
// HANYA untuk non-Windows: kalau ada kode mati saat build Windows,
// warningnya tetap muncul di sana.
#![cfg_attr(not(windows), allow(dead_code))]

/// Perintah yang dikirim menu tray ke UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Tampilkan & fokuskan jendela.
    Show,
    /// Ubah setelan "jalan di latar belakang" (centang/lepas centang).
    ToggleBackground(bool),
    /// Keluar sungguhan, apa pun setelan background.
    Quit,
}

pub const ID_SHOW: &str = "arka.tray.show";
pub const ID_BACKGROUND: &str = "arka.tray.background";
pub const ID_QUIT: &str = "arka.tray.quit";

/// Terjemahkan id menu menjadi aksi. `checked` = keadaan centang item
/// "Jalan di latar belakang" **setelah** klik (muda sudah membaliknya).
pub fn action_of(id: &str, checked: bool) -> Option<Action> {
    match id {
        ID_SHOW => Some(Action::Show),
        ID_BACKGROUND => Some(Action::ToggleBackground(checked)),
        ID_QUIT => Some(Action::Quit),
        _ => None,
    }
}

/// Ikon 16×16 sederhana: huruf "A" di atas latar bulat. Dibuat dari piksel
/// (bukan berkas .ico) supaya rilis tidak perlu membundel aset tambahan.
pub fn icon_rgba() -> Vec<u8> {
    const N: u32 = 32;
    let mut px = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let i = ((y * N + x) * 4) as usize;
            let fx = x as f32 - (N as f32 - 1.0) / 2.0;
            let fy = y as f32 - (N as f32 - 1.0) / 2.0;
            let r = (fx * fx + fy * fy).sqrt();
            // Latar biru-ungu, pinggiran dilunakkan dengan alpha.
            let inside = (16.0 - r).clamp(0.0, 1.0);
            if inside <= 0.0 {
                continue;
            }
            // Garis "A": dua kaki miring + palang.
            let leg = (fx.abs() - (fy + 8.0) * 0.45).abs() < 1.6 && (-8.0..=8.0).contains(&fy);
            let bar = (fy + 1.0).abs() < 1.4 && fx.abs() < 5.0;
            let (cr, cg, cb) = if leg || bar {
                (255, 255, 255)
            } else {
                (91, 76, 224)
            };
            px[i] = cr;
            px[i + 1] = cg;
            px[i + 2] = cb;
            px[i + 3] = (255.0 * inside) as u8;
        }
    }
    px
}

/// Pemegang ikon & menu yang nyata (hanya Windows).
#[cfg(windows)]
pub struct Tray {
    // Harus tetap hidup: kalau di-drop, Windows menghapus ikon dari tray.
    _icon: tray_icon::TrayIcon,
    pub background: tray_icon::menu::CheckMenuItem,
}

#[cfg(windows)]
impl Tray {
    pub fn new(background_run: bool) -> Result<Self, String> {
        use tray_icon::menu::{CheckMenuItem, Menu, MenuItem};
        use tray_icon::{Icon, TrayIconBuilder};

        let menu = Menu::new();
        let show = MenuItem::with_id(ID_SHOW, "Buka Arka Agent", true, None);
        let background = CheckMenuItem::with_id(
            ID_BACKGROUND,
            "Jalan di latar belakang",
            true,
            background_run,
            None,
        );
        let quit = MenuItem::with_id(ID_QUIT, "Keluar", true, None);
        menu.append_items(&[&show, &background, &quit])
            .map_err(|e| e.to_string())?;

        let icon = Icon::from_rgba(icon_rgba(), 32, 32).map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_icon(icon)
            .with_tooltip("Arka Agent")
            .with_menu_on_left_click(true)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| e.to_string())?;
        // Item harus disimpan: id-nya dipakai untuk mencocokkan klik, dan
        // `is_checked()` dibaca saat centang berubah.
        Ok(Self {
            _icon: tray,
            background,
        })
    }

    /// Ambil semua aksi yang menunggu. Tidak memblok frame UI.
    pub fn poll(&self) -> Vec<Action> {
        use tray_icon::{menu::MenuEvent, MouseButton, MouseButtonState, TrayIconEvent};
        let mut out = Vec::new();
        while let Ok(ev) = MenuEvent::receiver().try_recv() {
            let id = ev.id.0.as_str();
            let checked = self.background.is_checked();
            if let Some(a) = action_of(id, checked) {
                out.push(a);
            }
        }
        // Klik kiri pada ikon = buka jendela (kebiasaan aplikasi Windows).
        while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = ev
            {
                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    out.push(Action::Show);
                }
            }
        }
        out
    }

    /// Sinkronkan centang kalau setelan diubah dari tab Setelan.
    pub fn set_background(&self, on: bool) {
        self.background.set_checked(on);
    }
}

/// Tidak ada tray di Linux/CI; UI memakai ini supaya kodenya seragam.
#[cfg(not(windows))]
pub struct Tray;

#[cfg(not(windows))]
impl Tray {
    pub fn new(_background_run: bool) -> Result<Self, String> {
        Ok(Self)
    }
    pub fn poll(&self) -> Vec<Action> {
        Vec::new()
    }
    pub fn set_background(&self, _on: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_menu_diterjemahkan_dengan_benar() {
        assert_eq!(action_of(ID_SHOW, false), Some(Action::Show));
        assert_eq!(action_of(ID_QUIT, false), Some(Action::Quit));
        assert_eq!(
            action_of(ID_BACKGROUND, true),
            Some(Action::ToggleBackground(true))
        );
        assert_eq!(
            action_of(ID_BACKGROUND, false),
            Some(Action::ToggleBackground(false))
        );
        assert_eq!(action_of("id.asing", false), None);
    }

    #[test]
    fn ikon_berupa_rgba_32x32() {
        let px = icon_rgba();
        assert_eq!(px.len(), 32 * 32 * 4);
        assert!(
            px.iter().any(|b| *b != 0),
            "ikon tidak boleh seluruhnya transparan"
        );
    }
}
