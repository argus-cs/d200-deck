//! The drawing steps keys, icons and the visor share: an SVG to pixels,
//! pixels to PNG, a picture centered in an area, and #RRGGBB colors.

use std::io::Cursor;
use std::sync::{Arc, OnceLock};

use anyhow::{anyhow, Result};
use image::{imageops, DynamicImage, ImageFormat, Rgba, RgbaImage};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{fontdb, Options, Tree};

/// How every SVG here is read. An `<image>` in it may carry its picture
/// (`data:`), but not point at a file or an address: that is read the moment
/// the SVG is drawn, and a network path makes Windows hand the user's
/// credentials to that server. Iconify's files and hand-picked ones alike.
pub(crate) fn svg_options(with_text: bool) -> Options<'static> {
    let mut options = Options::default();
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    if with_text {
        options.fontdb = fonts();
    }
    options
}

/// Segoe UI from Windows, loaded once. Elsewhere drawings have no text.
fn fonts() -> Arc<fontdb::Database> {
    static FONTS: OnceLock<Arc<fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = fontdb::Database::new();
            let dir = std::env::var_os("WINDIR").map(std::path::PathBuf::from).unwrap_or_else(|| "C:\\Windows".into());
            for file in ["segoeui.ttf", "seguisb.ttf", "segoeuib.ttf", "seguisym.ttf"] {
                if let Ok(data) = std::fs::read(dir.join("Fonts").join(file)) {
                    db.load_font_data(data);
                }
            }
            Arc::new(db)
        })
        .clone()
}

/// An SVG document, read with `svg_options`.
pub(crate) fn parse_svg(svg: &str, with_text: bool) -> Result<Tree> {
    Ok(Tree::from_str(svg, &svg_options(with_text))?)
}

/// `tree` drawn through `transform` on a transparent `width`×`height` canvas.
pub(crate) fn render(tree: &Tree, width: u32, height: u32, transform: Transform) -> Result<Pixmap> {
    let mut pixmap = Pixmap::new(width, height).ok_or_else(|| anyhow!("não deu para criar uma imagem de {width}×{height}"))?;
    resvg::render(tree, transform, &mut pixmap.as_mut());
    Ok(pixmap)
}

/// An SVG document drawn as it is, `width`×`height`, as a PNG.
pub(crate) fn svg_to_png(svg: &str, with_text: bool, width: u32, height: u32) -> Result<Vec<u8>> {
    let pixmap = render(&parse_svg(svg, with_text)?, width, height, Transform::identity())?;
    Ok(pixmap.encode_png()?)
}

/// An SVG document drawn as it is, `width`×`height`, as pixels.
pub(crate) fn svg_to_rgba(svg: &str, with_text: bool, width: u32, height: u32) -> Result<RgbaImage> {
    Ok(to_rgba(&render(&parse_svg(svg, with_text)?, width, height, Transform::identity())?))
}

/// The pixels as `image` keeps them: tiny-skia's are premultiplied.
pub(crate) fn to_rgba(pixmap: &Pixmap) -> RgbaImage {
    let mut img = RgbaImage::new(pixmap.width(), pixmap.height());
    for (dst, src) in img.pixels_mut().zip(pixmap.pixels()) {
        let c = src.demultiply();
        *dst = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    img
}

pub(crate) fn encode_png(img: RgbaImage) -> Result<Vec<u8>> {
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

/// Lays `img` in the middle of the area of `canvas` that starts at (`x`, `y`)
/// and measures `width`×`height`.
pub(crate) fn overlay_centered(canvas: &mut RgbaImage, img: &RgbaImage, (x, y, width, height): (u32, u32, u32, u32)) {
    let left = x + width.saturating_sub(img.width()) / 2;
    let top = y + height.saturating_sub(img.height()) / 2;
    imageops::overlay(canvas, img, left.into(), top.into());
}

/// "#RRGGBB" (the "#" may be left out) as an opaque color.
pub(crate) fn parse_color(hex: &str) -> Result<Rgba<u8>> {
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

    #[test]
    fn colors_are_six_hex_digits() {
        assert_eq!(parse_color("#24262B").unwrap(), Rgba([0x24, 0x26, 0x2B, 255]));
        assert_eq!(parse_color("f0a63a").unwrap(), Rgba([0xF0, 0xA6, 0x3A, 255]));
        for bad in ["", "#", "#FFF", "#GGGGGG", "#1234567", "red"] {
            assert!(parse_color(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn pictures_land_in_the_middle_of_their_area() {
        let mut canvas = RgbaImage::new(10, 10);
        let dot = RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255]));
        overlay_centered(&mut canvas, &dot, (4, 0, 6, 10));
        // Area x 4..10, y 0..10: a 2×2 picture starts at (6, 4).
        assert_eq!(canvas.get_pixel(6, 4).0, [255, 0, 0, 255]);
        assert_eq!(canvas.get_pixel(5, 4).0[3], 0);
        // Larger than the area: from its corner, never a panic.
        overlay_centered(&mut canvas, &RgbaImage::new(20, 20), (0, 0, 10, 10));
    }

    #[test]
    fn svgs_become_straight_rgba_and_png() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="#FF0000" fill-opacity="0.5"/></svg>"##;
        let img = svg_to_rgba(svg, false, 4, 4).unwrap();
        // Half-transparent red stays red, not darkened by premultiplying.
        let p = img.get_pixel(1, 1).0;
        assert_eq!((p[0], p[1], p[2]), (255, 0, 0));
        assert!((120..=135).contains(&p[3]), "{p:?}");
        let png = image::load_from_memory(&svg_to_png(svg, false, 4, 4).unwrap()).unwrap();
        assert_eq!((png.width(), png.height()), (4, 4));
    }
}
