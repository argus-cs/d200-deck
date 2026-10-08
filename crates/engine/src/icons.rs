//! Renders a key's 196×196 PNG: a built-in stroke glyph (the same set as the
//! interface prototype) or the user's PNG, on the key's background color.
//! The label is drawn by the device itself, below the image.

use std::io::Cursor;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use image::{imageops, DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::config::Key;
use d200::protocol::ICON_SIZE;

/// 24×24 stroke paths, one `d` attribute each.
pub const GLYPHS: &[(&str, &str)] = &[
    ("micOff", "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z M19 11a7 7 0 0 1-14 0 M12 18v3 M4 4l16 16"),
    ("mic", "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z M19 11a7 7 0 0 1-14 0 M12 18v3"),
    ("play", "M8 5l11 7-11 7z"),
    ("next", "M5 5l10 7-10 7z M19 5v14"),
    ("volume", "M11 5L6 9H3v6h3l5 4z M15.5 8.5a5 5 0 0 1 0 7 M18.5 5.5a9 9 0 0 1 0 13"),
    ("capture", "M4 8V5a1 1 0 0 1 1-1h3 M16 4h3a1 1 0 0 1 1 1v3 M20 16v3a1 1 0 0 1-1 1h-3 M8 20H5a1 1 0 0 1-1-1v-3 M9 12h6 M12 9v6"),
    ("terminal", "M4 17l6-5-6-5 M12 19h8"),
    ("globe", "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M3 12h18 M12 3c3 3.5 3 14.5 0 18 M12 3c-3 3.5-3 14.5 0 18"),
    ("folder", "M3 6h6l2 2h10v11H3z"),
    ("music", "M9 18V5l12-2v13 M9 18a3 3 0 1 1-6 0a3 3 0 1 1 6 0z M21 16a3 3 0 1 1-6 0a3 3 0 1 1 6 0z"),
    ("lock", "M6 11h12v9H6z M8 11V7a4 4 0 0 1 8 0v4"),
    ("record", "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z"),
    ("desktop", "M3 4h18v12H3z M8 20h8 M12 16v4"),
    ("headOff", "M4 15v-3a8 8 0 0 1 13.7-5.6 M20 12v3 M4 15h3v5H4z M17 15h3v5h-3z M3 3l18 18"),
    ("brush", "M18 3l3 3-9 9-3-3z M9 12c-2 0-4 1.5-4 4 0 2-2 3-2 3s4 1 6-1c1.5-1.5 1.5-3 1-4"),
    ("eraser", "M8 20h12 M5 16l9-9 5 5-8 8H8z M10 11l5 5"),
    ("zoom", "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M20 20l-4-4 M11 8v6 M8 11h6"),
    ("undo", "M9 14L4 9l5-5 M4 9h11a5 5 0 0 1 0 10h-3"),
    ("forward", "M20 12a8 8 0 1 1-2.3-5.7 M20 4v4h-4"),
    ("captions", "M3 6h18v12H3z M7 15h4 M13 15h4 M7 11h10"),
    ("fullscreen", "M4 9V4h5 M20 9V4h-5 M4 15v5h5 M20 15v5h-5"),
    ("camera", "M15 10l5-3v10l-5-3z M3 7h12v10H3z"),
];

pub fn is_glyph(name: &str) -> bool {
    glyph(name).is_some()
}

fn glyph(name: &str) -> Option<&'static str> {
    GLYPHS.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
}

/// `base_dir` resolves relative PNG paths (the config file's folder).
pub fn render(key: &Key, base_dir: &Path) -> Result<Vec<u8>> {
    let background = parse_color(&key.color)?;
    match key.icon.as_deref() {
        None => encode(RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, background)),
        Some(name) => match glyph(name) {
            Some(d) => render_glyph(d, &key.color),
            None => render_file(&base_dir.join(name), background),
        },
    }
}

fn render_glyph(d: &str, background: &str) -> Result<Vec<u8>> {
    // The glyph sits in the upper part of the key, leaving room for the label.
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><rect width="{s}" height="{s}" fill="{background}"/><g transform="translate(48 30) scale(4.1667)"><path d="{d}" fill="none" stroke="#ECEDEF" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></g></svg>"##,
        s = ICON_SIZE
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE).ok_or_else(|| anyhow!("pixmap"))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    Ok(pixmap.encode_png()?)
}

fn render_file(path: &Path, background: Rgba<u8>) -> Result<Vec<u8>> {
    let img = image::open(path).with_context(|| format!("não consegui abrir {}", path.display()))?;
    let fitted = img.resize(ICON_SIZE, ICON_SIZE, imageops::FilterType::Lanczos3).to_rgba8();
    let mut canvas = RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, background);
    let x = (ICON_SIZE - fitted.width()) / 2;
    let y = (ICON_SIZE - fitted.height()) / 2;
    imageops::overlay(&mut canvas, &fitted, x.into(), y.into());
    encode(canvas)
}

/// A PNG as a data URL, for showing it in the app.
pub fn data_url(png: &[u8]) -> String {
    use base64::Engine as _;
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

fn encode(img: RgbaImage) -> Result<Vec<u8>> {
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

fn parse_color(hex: &str) -> Result<Rgba<u8>> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("zz"), 16);
    match (byte(0), byte(2), byte(4)) {
        (Ok(r), Ok(g), Ok(b)) if h.len() == 6 => Ok(Rgba([r, g, b, 255])),
        _ => Err(anyhow!("cor inválida: {hex}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(icon: Option<&str>) -> Key {
        Key { label: String::new(), icon: icon.map(Into::into), color: "#24262B".into(), action: None, front: false }
    }

    #[test]
    fn every_glyph_renders_at_device_size() {
        for (name, _) in GLYPHS {
            let png = render(&key(Some(name)), Path::new(".")).unwrap();
            let img = image::load_from_memory(&png).unwrap();
            assert_eq!((img.width(), img.height()), (ICON_SIZE, ICON_SIZE), "{name}");
        }
    }

    #[test]
    fn user_png_is_fitted_on_the_background() {
        let dir = std::env::temp_dir().join("deck-engine-icon-test");
        std::fs::create_dir_all(&dir).unwrap();
        RgbaImage::from_pixel(400, 100, Rgba([255, 0, 0, 255])).save(dir.join("wide.png")).unwrap();
        let png = render(&key(Some("wide.png")), &dir).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (ICON_SIZE, ICON_SIZE));
        assert_eq!(img.get_pixel(98, 98), &Rgba([255, 0, 0, 255]));
        assert_eq!(img.get_pixel(98, 5), &Rgba([0x24, 0x26, 0x2B, 255]));
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(render(&key(Some("nao-existe.png")), Path::new(".")).is_err());
    }
}
