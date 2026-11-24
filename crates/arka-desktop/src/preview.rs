//! Pratinjau berkas workspace (M6): memuat isi, bukan menggambar.
//!
//! Bagian yang bisa salah — memilih cara pratinjau, membaca teks, mendekode
//! gambar, menghitung halaman PDF — dipisah dari egui di sini, sehingga bisa
//! diuji di CI tanpa GPU. Layarnya (`gui::screens::files`) hanya menampilkan
//! [`PreviewState`] yang sudah jadi.
//!
//! Keputusan yang tidak bisa diambil "nanti":
//!
//! * **Gambar** didekode di sini (bukan di `draw`): mendekode PNG 4000×3000 di
//!   dalam frame membuat jendela membeku sesaat di setiap repaint.
//! * **Ukuran dibatasi**: berkas gambar > [`MAX_IMAGE_BYTES`] dan gambar > sisi
//!   [`MAX_EDGE`] diperkecil, supaya memori tidak meledak. Android memakai
//!   `inSampleSize` untuk hal yang sama.
//! * **Video/audio/PDF tidak dirender.** egui tidak punya dekoder media, dan
//!   merender PDF butuh pdfium/mupdf berukuran puluhan MB per arsitektur.
//!   Yang ditampilkan: metadata + tombol "Buka di aplikasi lain".

use std::fs;
use std::path::Path;

use arka_core::file::{self, FileInfo};

/// Batas berkas gambar yang mau didekode (32 MB).
pub const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

/// Sisi terpanjang maksimum setelah diperkecil (px).
pub const MAX_EDGE: u32 = 2048;

/// Isi pratinjau yang sudah siap tampil.
pub enum Content {
    Text(String),
    Image {
        texture: eframe::egui::TextureHandle,
        /// Ukuran asli berkas (sebelum diperkecil).
        width: u32,
        height: u32,
        downscaled: bool,
    },
    Pdf {
        pages: usize,
    },
    /// Tidak bisa ditampilkan di dalam aplikasi; alasannya untuk pengguna.
    External(String),
    Error(String),
}

/// Satu berkas yang sedang dipratinjau.
pub struct PreviewState {
    pub rel: String,
    pub name: String,
    pub kind: String,
    pub size: i64,
    pub content: Content,
}

impl PreviewState {
    /// Muat pratinjau untuk [`FileInfo`]. Tidak pernah gagal: kegagalan jadi
    /// [`Content::Error`] supaya dialog tetap bisa ditutup.
    pub fn load(root: &Path, info: &FileInfo, ctx: &eframe::egui::Context) -> Self {
        let content = match info.kind.as_str() {
            "text" => match file::read_text_at(root, &info.path) {
                Ok(t) => Content::Text(t),
                Err(e) => Content::Error(e),
            },
            "image" => match decode_image(root, &info.path, ctx) {
                Ok((texture, width, height, downscaled)) => Content::Image {
                    texture,
                    width,
                    height,
                    downscaled,
                },
                Err(e) => Content::Error(e),
            },
            "pdf" => match pdf_pages(root, &info.path) {
                Ok(pages) => Content::Pdf { pages },
                Err(e) => Content::Error(e),
            },
            "video" => Content::External(
                "Video dan audio tidak diputar di dalam aplikasi ini.".to_string(),
            ),
            _ => Content::External("Berkas biner tidak bisa dipratinjau.".to_string()),
        };

        Self {
            rel: info.path.clone(),
            name: info.name.clone(),
            kind: info.kind.clone(),
            size: info.size,
            content,
        }
    }
}

fn decode_image(
    root: &Path,
    rel: &str,
    ctx: &eframe::egui::Context,
) -> Result<(eframe::egui::TextureHandle, u32, u32, bool), String> {
    let path = file::resolve_at(root, rel)?;
    let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "Gambar terlalu besar ({} MB, maksimum {} MB).",
            meta.len() / 1024 / 1024,
            MAX_IMAGE_BYTES / 1024 / 1024
        ));
    }
    let reader = image::ImageReader::open(&path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let img = reader.decode().map_err(|e| format!("Gambar tidak bisa dibaca: {e}"))?;
    let (width, height) = (img.width(), img.height());
    let downscaled = width > MAX_EDGE || height > MAX_EDGE;
    let img = if downscaled {
        img.thumbnail(MAX_EDGE, MAX_EDGE)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = eframe::egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    let texture = ctx.load_texture(
        format!("preview:{rel}"),
        color,
        eframe::egui::TextureOptions::LINEAR,
    );
    Ok((texture, width, height, downscaled))
}

/// Jumlah halaman PDF tanpa merender.
///
/// Hanya membaca struktur (xref + objek `/Type /Page`); tidak ada rasterisasi,
/// jadi tidak ada DLL tambahan. PDF terenkripsi ditolak dengan pesan yang jelas.
pub fn pdf_pages(root: &Path, rel: &str) -> Result<usize, String> {
    let path = file::resolve_at(root, rel)?;
    let doc = lopdf::Document::load(&path).map_err(|e| format!("PDF tidak bisa dibaca: {e}"))?;
    let pages = doc.get_pages().len();
    if pages == 0 {
        return Err("PDF tidak berisi halaman".into());
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "arka-preview-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn info(rel: &str, kind: &str) -> FileInfo {
        FileInfo {
            path: rel.to_string(),
            name: rel.rsplit('/').next().unwrap_or(rel).to_string(),
            is_dir: false,
            size: 0,
            modified: 0,
            kind: kind.to_string(),
        }
    }

    #[test]
    fn teks_dibaca_apa_adanya() {
        let root = tmp("teks");
        fs::write(root.join("catatan.md"), "# Halo\nisi berkas").unwrap();
        let ctx = eframe::egui::Context::default();
        let p = PreviewState::load(&root, &info("catatan.md", "text"), &ctx);
        match p.content {
            Content::Text(t) => assert_eq!(t, "# Halo\nisi berkas"),
            _ => panic!("bukan teks"),
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn berkas_hilang_jadi_error_bukan_panic() {
        let root = tmp("hilang");
        let ctx = eframe::egui::Context::default();
        let p = PreviewState::load(&root, &info("tidak-ada.txt", "text"), &ctx);
        assert!(matches!(p.content, Content::Error(_)));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gambar_didekode_dan_diperkecil() {
        let root = tmp("gambar");
        // Lebar 3000 px → harus diperkecil ke 2048, tapi ukuran asli dilaporkan.
        let img = image::RgbaImage::from_pixel(3000, 12, image::Rgba([10, 20, 30, 255]));
        img.save(root.join("lebar.png")).unwrap();

        let ctx = eframe::egui::Context::default();
        let p = PreviewState::load(&root, &info("lebar.png", "image"), &ctx);
        match p.content {
            Content::Image {
                ref texture,
                width,
                height,
                downscaled,
            } => {
                assert_eq!((width, height), (3000, 12));
                assert!(downscaled, "3000 px harus diperkecil");
                assert_eq!(texture.size(), [2048, 8], "rasio 3000:12 → 2048:8");
            }
            Content::Error(e) => panic!("gagal: {e}"),
            _ => panic!("bukan gambar"),
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn gambar_kecil_tidak_diperkecil() {
        let root = tmp("gambar-kecil");
        image::RgbaImage::from_pixel(64, 32, image::Rgba([1, 2, 3, 255]))
            .save(root.join("kecil.png"))
            .unwrap();
        let ctx = eframe::egui::Context::default();
        let p = PreviewState::load(&root, &info("kecil.png", "image"), &ctx);
        match p.content {
            Content::Image {
                ref texture,
                downscaled,
                ..
            } => {
                assert!(!downscaled);
                assert_eq!(texture.size(), [64, 32]);
            }
            _ => panic!("bukan gambar"),
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn berkas_bukan_gambar_jadi_error() {
        let root = tmp("bukan-gambar");
        fs::write(root.join("rusak.png"), b"ini bukan png").unwrap();
        let ctx = eframe::egui::Context::default();
        let p = PreviewState::load(&root, &info("rusak.png", "image"), &ctx);
        assert!(matches!(p.content, Content::Error(_)), "harus error");
        let _ = fs::remove_dir_all(&root);
    }

    /// PDF dua halaman hasil tulisan tangan (bukan buatan lopdf), supaya
    /// pembacaan diuji terhadap berkas yang tidak dibuat kode yang sama.
    const DUA_HALAMAN: &[u8] = include_bytes!("../assets/test/dua-halaman.pdf");

    #[test]
    fn jumlah_halaman_pdf() {
        let root = tmp("pdf");
        fs::write(root.join("dua.pdf"), DUA_HALAMAN).unwrap();
        assert_eq!(pdf_pages(&root, "dua.pdf").unwrap(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn pdf_rusak_jadi_error() {
        let root = tmp("pdf-rusak");
        fs::write(root.join("rusak.pdf"), b"%PDF-1.4\nbukan pdf").unwrap();
        assert!(pdf_pages(&root, "rusak.pdf").is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn video_dan_biner_minta_buka_di_luar() {
        let root = tmp("luar");
        let ctx = eframe::egui::Context::default();
        let p = PreviewState::load(&root, &info("film.mp4", "video"), &ctx);
        assert!(matches!(p.content, Content::External(_)));
        let p = PreviewState::load(&root, &info("data.bin", "binary"), &ctx);
        assert!(matches!(p.content, Content::External(_)));
        let _ = fs::remove_dir_all(&root);
    }
}
