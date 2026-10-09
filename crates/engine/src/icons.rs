//! Renders a key's 196×196 PNG: a built-in stroke glyph (the same set as the
//! interface prototype) or the user's PNG, on the key's background color.
//! The label is drawn by the device itself, below the image.

use std::io::Cursor;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use image::{imageops, DynamicImage, ImageFormat, Rgba, RgbaImage};

use d200::protocol::ICON_SIZE;

/// Built-in icons: (id kept in config.json, Portuguese words for search,
/// 24×24 stroke path). Never rename an id: keys point to it.
pub const GLYPHS: &[(&str, &str, &str)] = &[
    ("micOff", "microfone mudo mutar", "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z M19 11a7 7 0 0 1-14 0 M12 18v3 M4 4l16 16"),
    ("mic", "microfone", "M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z M19 11a7 7 0 0 1-14 0 M12 18v3"),
    ("play", "play tocar", "M8 5l11 7-11 7z"),
    ("pause", "pausa pausar", "M7 5h3v14H7z M14 5h3v14h-3z"),
    ("stop", "parar stop", "M6 6h12v12H6z"),
    ("next", "próxima faixa avançar", "M5 5l10 7-10 7z M19 5v14"),
    ("previous", "faixa anterior voltar", "M19 5L9 12l10 7z M5 5v14"),
    ("shuffle", "aleatório embaralhar", "M3 7h4l10 10h4 M3 17h4l3-3 M14 10l3-3h4 M18 4l3 3-3 3 M18 14l3 3-3 3"),
    ("repeat", "repetir", "M17 2l4 4-4 4 M3 12V9a3 3 0 0 1 3-3h15 M7 22l-4-4 4-4 M21 12v3a3 3 0 0 1-3 3H3"),
    ("volume", "volume som alto", "M11 5L6 9H3v6h3l5 4z M15.5 8.5a5 5 0 0 1 0 7 M18.5 5.5a9 9 0 0 1 0 13"),
    ("volumeDown", "volume baixo diminuir", "M11 5L6 9H3v6h3l5 4z M15.5 8.5a5 5 0 0 1 0 7"),
    ("volumeOff", "mudo sem som silenciar", "M11 5L6 9H3v6h3l5 4z M16 9l6 6 M22 9l-6 6"),
    ("headphones", "fone de ouvido áudio", "M4 15v-3a8 8 0 0 1 16 0v3 M4 15h3v5H4z M17 15h3v5h-3z"),
    ("headOff", "fone mudo ensurdecer", "M4 15v-3a8 8 0 0 1 13.7-5.6 M20 12v3 M4 15h3v5H4z M17 15h3v5h-3z M3 3l18 18"),
    ("music", "música nota", "M9 18V5l12-2v13 M9 18a3 3 0 1 1-6 0a3 3 0 1 1 6 0z M21 16a3 3 0 1 1-6 0a3 3 0 1 1 6 0z"),
    ("record", "gravar gravação", "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z"),
    ("live", "ao vivo transmissão live", "M12 10a2 2 0 1 0 0 4a2 2 0 1 0 0-4z M8.5 8.5a5 5 0 0 0 0 7 M15.5 8.5a5 5 0 0 1 0 7 M5.6 5.6a9 9 0 0 0 0 12.8 M18.4 5.6a9 9 0 0 1 0 12.8"),
    ("camera", "câmera vídeo webcam", "M15 10l5-3v10l-5-3z M3 7h12v10H3z"),
    ("cameraOff", "câmera desligada sem vídeo", "M15 10l5-3v10l-5-3z M3 7h12v10H3z M2 3l20 18"),
    ("hangUp", "desligar chamada encerrar ligação", "M3 14.5c5-4.7 13-4.7 18 0l-2.2 2.6-3.8-1.2v-3a10 10 0 0 0-6 0v3l-3.8 1.2z"),
    ("screenShare", "compartilhar tela apresentar", "M3 5h18v12H3z M8 21h8 M12 17v4 M12 14V8 M9 11l3-3 3 3"),
    ("hand", "levantar a mão mão pedir a vez", "M8 13V5.5a1.5 1.5 0 0 1 3 0V11 M11 11V4a1.5 1.5 0 0 1 3 0v7 M14 11V5.5a1.5 1.5 0 0 1 3 0V14a6 6 0 0 1-6 6h-1a6 6 0 0 1-5-2.7l-2.4-3.9a1.5 1.5 0 0 1 2.5-1.6L8 13"),
    ("users", "participantes pessoas grupo", "M9 4a3.5 3.5 0 1 0 0 7a3.5 3.5 0 1 0 0-7z M2 20a7 7 0 0 1 14 0 M16 4.5a3.5 3.5 0 0 1 0 6.5 M18 13.5a7 7 0 0 1 4 6.5"),
    ("headset", "headset fone com microfone chamada", "M4 14v-2a8 8 0 0 1 16 0v2 M4 14h3v5H4z M17 14h3v5h-3z M20 19a3 3 0 0 1-3 3h-4"),
    ("capture", "captura de tela print recorte", "M4 8V5a1 1 0 0 1 1-1h3 M16 4h3a1 1 0 0 1 1 1v3 M20 16v3a1 1 0 0 1-1 1h-3 M8 20H5a1 1 0 0 1-1-1v-3 M9 12h6 M12 9v6"),
    ("image", "imagem foto", "M3 5h18v14H3z M3 16l5-5 4 4 3-3 6 6 M15 9h.01"),
    ("fullscreen", "tela cheia expandir", "M4 9V4h5 M20 9V4h-5 M4 15v5h5 M20 15v5h-5"),
    ("captions", "legendas texto", "M3 6h18v12H3z M7 15h4 M13 15h4 M7 11h10"),
    ("forward", "avançar girar atualizar", "M20 12a8 8 0 1 1-2.3-5.7 M20 4v4h-4"),
    ("desktop", "tela monitor desktop cena", "M3 4h18v12H3z M8 20h8 M12 16v4"),
    ("window", "janela app aplicativo", "M3 5h18v14H3z M3 9h18"),
    ("grid", "grade apps blocos", "M4 4h6v6H4z M14 4h6v6h-6z M4 14h6v6H4z M14 14h6v6h-6z"),
    ("layers", "camadas", "M12 3l9 5-9 5-9-5z M3 13l9 5 9-5"),
    ("terminal", "terminal console prompt", "M4 17l6-5-6-5 M12 19h8"),
    ("code", "código programação", "M8 8l-4 4 4 4 M16 8l4 4-4 4 M14 5l-4 14"),
    ("branch", "git branch ramo", "M6 3a2 2 0 1 0 0 4a2 2 0 1 0 0-4z M6 17a2 2 0 1 0 0 4a2 2 0 1 0 0-4z M18 7a2 2 0 1 0 0 4a2 2 0 1 0 0-4z M6 7v10 M18 11c0 4-6 4-11.5 6.5"),
    ("cpu", "processador cpu hardware", "M7 7h10v10H7z M10 10h4v4h-4z M9 3v4 M15 3v4 M9 17v4 M15 17v4 M3 9h4 M3 15h4 M17 9h4 M17 15h4"),
    ("keyboard", "teclado", "M3 6h18v12H3z M7 10h.01 M11 10h.01 M15 10h.01 M7 14h10"),
    ("mouse", "mouse", "M12 3a6 6 0 0 0-6 6v6a6 6 0 0 0 12 0V9a6 6 0 0 0-6-6z M12 7v4"),
    ("gamepad", "controle jogo game", "M6 8h12a4 4 0 0 1 4 4v1a3 3 0 0 1-5.2 2L15 13H9l-1.8 2A3 3 0 0 1 2 13v-1a4 4 0 0 1 4-4z M7 10v4 M5 12h4 M16 11h.01 M18 13h.01"),
    ("globe", "navegador internet site web", "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M3 12h18 M12 3c3 3.5 3 14.5 0 18 M12 3c-3 3.5-3 14.5 0 18"),
    ("link", "link endereço corrente", "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1 M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1"),
    ("mail", "email correio mensagem", "M3 6h18v12H3z M3 7l9 6 9-6"),
    ("chat", "chat conversa mensagem", "M4 5h16v11H9l-5 4z"),
    ("phone", "telefone ligação chamada", "M6 3h4l2 5-3 2a11 11 0 0 0 5 5l2-3 5 2v4a2 2 0 0 1-2 2A17 17 0 0 1 4 5a2 2 0 0 1 2-2z"),
    ("user", "pessoa usuário perfil", "M12 4a4 4 0 1 0 0 8a4 4 0 1 0 0-8z M4 21a8 8 0 0 1 16 0"),
    ("bell", "sino notificação aviso", "M6 16v-5a6 6 0 0 1 12 0v5l2 2H4z M10 21h4"),
    ("bellOff", "sino desligado não perturbe", "M6 16v-5a6 6 0 0 1 12 0v5l2 2H4z M10 21h4 M3 3l18 18"),
    ("calendar", "calendário agenda data", "M4 6h16v14H4z M4 10h16 M8 3v4 M16 3v4"),
    ("clock", "relógio hora", "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18z M12 7v5l3 2"),
    ("timer", "cronômetro timer temporizador", "M12 6a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M12 10v3 M10 3h4"),
    ("back", "voltar retornar sair da pasta", "M9 14L4 9l5-5 M4 9h11a5 5 0 0 1 5 5v6"),
    ("folder", "pasta arquivos explorer", "M3 6h6l2 2h10v11H3z"),
    ("save", "salvar disquete", "M5 3h11l3 3v15H5z M8 3v5h7V3 M8 21v-7h8v7"),
    ("copy", "copiar duplicar", "M8 8h12v12H8z M4 16V4h12"),
    ("paste", "colar área de transferência", "M8 4h8v3H8z M6 5H5v16h14V5h-1"),
    ("cut", "recortar tesoura", "M6 4a3 3 0 1 0 0 6a3 3 0 1 0 0-6z M6 14a3 3 0 1 0 0 6a3 3 0 1 0 0-6z M8.5 8.5L20 20 M8.5 15.5L20 4"),
    ("undo", "desfazer voltar", "M9 14L4 9l5-5 M4 9h11a5 5 0 0 1 0 10h-3"),
    ("redo", "refazer", "M15 14l5-5-5-5 M20 9H9a5 5 0 0 0 0 10h3"),
    ("trash", "lixeira apagar excluir", "M4 7h16 M9 7V4h6v3 M6 7l1 13h10l1-13"),
    ("download", "baixar download", "M12 4v12 M7 11l5 5 5-5 M4 20h16"),
    ("upload", "enviar upload subir", "M12 16V4 M7 9l5-5 5 5 M4 20h16"),
    ("search", "buscar pesquisar procurar", "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M20 20l-4-4"),
    ("zoom", "zoom lupa ampliar", "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M20 20l-4-4 M11 8v6 M8 11h6"),
    ("brush", "pincel pintar desenho", "M18 3l3 3-9 9-3-3z M9 12c-2 0-4 1.5-4 4 0 2-2 3-2 3s4 1 6-1c1.5-1.5 1.5-3 1-4"),
    ("pen", "caneta lápis editar", "M4 20l4-1 11-11-3-3L5 16z M14 6l3 3"),
    ("eraser", "borracha apagar", "M8 20h12 M5 16l9-9 5 5-8 8H8z M10 11l5 5"),
    ("text", "texto letra fonte", "M5 6V4h14v2 M12 4v16 M9 20h6"),
    ("list", "lista itens", "M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01"),
    ("check", "confirmar ok certo", "M5 12l5 5 9-10"),
    ("close", "fechar x cancelar", "M6 6l12 12 M18 6L6 18"),
    ("plus", "mais adicionar novo", "M12 5v14 M5 12h14"),
    ("minus", "menos remover", "M5 12h14"),
    ("arrowUp", "seta cima subir", "M12 19V5 M6 11l6-6 6 6"),
    ("arrowDown", "seta baixo descer", "M12 5v14 M6 13l6 6 6-6"),
    ("arrowLeft", "seta esquerda voltar", "M19 12H5 M11 6l-6 6 6 6"),
    ("arrowRight", "seta direita avançar", "M5 12h14 M13 6l6 6-6 6"),
    ("lock", "cadeado bloquear trancar", "M6 11h12v9H6z M8 11V7a4 4 0 0 1 8 0v4"),
    ("power", "desligar energia ligar", "M12 3v9 M6.6 6.6a8 8 0 1 0 10.8 0"),
    ("sun", "sol brilho claro dia", "M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z M12 2v2 M12 20v2 M4.9 4.9l1.4 1.4 M17.7 17.7l1.4 1.4 M2 12h2 M20 12h2 M4.9 19.1l1.4-1.4 M17.7 6.3l1.4-1.4"),
    ("moon", "lua noite escuro", "M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"),
    ("wifi", "wifi rede internet", "M2 9a15 15 0 0 1 20 0 M5 12.5a10 10 0 0 1 14 0 M8.5 16a5 5 0 0 1 7 0 M12 19.5h.01"),
    ("wifiOff", "wifi desligado sem rede", "M2 9a15 15 0 0 1 20 0 M5 12.5a10 10 0 0 1 14 0 M8.5 16a5 5 0 0 1 7 0 M12 19.5h.01 M3 3l18 18"),
    ("bluetooth", "bluetooth", "M7 7l10 10-5 5V2l5 5L7 17"),
    ("bluetoothOff", "bluetooth desligado", "M7 7l10 10-5 5V2l5 5L7 17 M3 3l18 18"),
    ("battery", "bateria", "M3 8h16v8H3z M21 11v2"),
    ("zap", "raio energia rápido atalho", "M13 2L4 14h7l-1 8 9-12h-7z"),
    ("star", "estrela favorito", "M12 3l2.8 5.8 6.2.9-4.5 4.4 1 6.2-5.5-2.9-5.5 2.9 1-6.2L3 9.7l6.2-.9z"),
    ("heart", "coração curtir", "M12 20s-8-4.5-8-10a4.5 4.5 0 0 1 8-2.8A4.5 4.5 0 0 1 20 10c0 5.5-8 10-8 10z"),
    ("flag", "bandeira marcar", "M5 21V4 M5 4h12l-2 4 2 4H5"),
    ("bookmark", "marcador favorito salvar", "M6 3h12v18l-6-4-6 4z"),
    ("pin", "local mapa marcador", "M12 21s7-6.5 7-12a7 7 0 0 0-14 0c0 5.5 7 12 7 12z M12 7a2 2 0 1 0 0 4a2 2 0 1 0 0-4z"),
    ("eye", "olho ver mostrar", "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z"),
    ("eyeOff", "olho esconder ocultar", "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z M12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z M3 3l18 18"),
    ("coffee", "café pausa intervalo", "M4 9h12v6a5 5 0 0 1-5 5H9a5 5 0 0 1-5-5z M16 11h2a2 2 0 0 1 0 4h-2 M8 3v3 M12 3v3"),
];

pub fn is_glyph(name: &str) -> bool {
    glyph(name).is_some()
}

fn glyph(name: &str) -> Option<&'static str> {
    GLYPHS.iter().find(|(id, _, _)| *id == name).map(|(_, _, d)| *d)
}

/// A built-in icon's 24×24 path, for drawing it elsewhere (the visor).
pub fn glyph_path(id: &str) -> Option<&'static str> {
    glyph(id)
}

/// The usual color of built-in icons.
pub const ICON_COLOR: &str = "#ECEDEF";

/// Everything that shapes a key's image.
#[derive(Clone, Debug, PartialEq)]
pub struct Look<'a> {
    /// A built-in glyph id or a PNG path.
    pub icon: Option<&'a str>,
    pub background: &'a str,
    pub icon_color: &'a str,
    /// A frame around the key.
    pub border: Option<&'a str>,
    /// Drawn at the bottom; empty for none.
    pub label: &'a str,
    pub text_color: &'a str,
    /// Label size in pixels; long labels shrink to fit.
    pub label_px: u32,
    /// Darkened, for an app key whose app is not running.
    pub dim: bool,
}

/// How bright a dimmed key stays.
const DIMMED: f32 = 0.4;

impl Look<'_> {
    /// Same string, same image.
    pub fn cache_key(&self) -> String {
        format!("{self:?}")
    }
}

/// A key's image: the icon on its background, then the frame and the label
/// over it. `base_dir` resolves relative PNG paths (the config folder).
pub fn render_key(look: &Look, base_dir: &Path) -> Result<Vec<u8>> {
    let labeled = !look.label.trim().is_empty();
    let icon = render_icon(look.icon, look.background, look.icon_color, labeled, base_dir)?;
    if look.border.is_none() && !labeled && !look.dim {
        return Ok(icon);
    }
    let mut canvas = image::load_from_memory(&icon)?.to_rgba8();
    if look.border.is_some() || labeled {
        imageops::overlay(&mut canvas, &decorations(look, labeled)?, 0, 0);
    }
    if look.dim {
        for pixel in canvas.pixels_mut() {
            for channel in &mut pixel.0[..3] {
                *channel = (*channel as f32 * DIMMED) as u8;
            }
        }
    }
    encode(canvas)
}

/// The frame and the label, on a transparent layer.
fn decorations(look: &Look, labeled: bool) -> Result<RgbaImage> {
    let mut over = String::new();
    if let Some(border) = look.border {
        over += &format!(r#"<rect x="5" y="5" width="186" height="186" rx="24" fill="none" stroke="{border}" stroke-width="10"/>"#);
    }
    if labeled {
        over += &label_svg(look.label, look.text_color, look.label_px);
    }
    rasterize(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}">{over}</svg>"#,
        s = ICON_SIZE
    ))
}

/// Only the icon on its background, as the icon picker shows it.
pub fn render(icon: Option<&str>, color: &str, icon_color: Option<&str>, base_dir: &Path) -> Result<Vec<u8>> {
    render_icon(icon, color, icon_color.unwrap_or(ICON_COLOR), false, base_dir)
}

fn render_icon(icon: Option<&str>, color: &str, icon_color: &str, labeled: bool, base_dir: &Path) -> Result<Vec<u8>> {
    let background = parse_color(color)?;
    match icon {
        None => encode(RgbaImage::from_pixel(ICON_SIZE, ICON_SIZE, background)),
        Some(name) => match glyph(name) {
            Some(d) => render_glyph(d, color, icon_color, labeled),
            None => render_file(&base_dir.join(name), background),
        },
    }
}

/// The label, one line, shrunk to fit (down to 16 px) and cut with "…" beyond that.
fn label_svg(label: &str, color: &str, px: u32) -> String {
    const WIDTH: f32 = (ICON_SIZE - 20) as f32;
    const CHAR: f32 = 0.56;
    let chars = label.chars().count().max(1) as f32;
    let size = (WIDTH / (chars * CHAR)).min(px as f32).max(16.0);
    let shown = if chars * size * CHAR > WIDTH { fit(label, (WIDTH / (size * CHAR)) as usize) } else { label.to_string() };
    // A thin dark outline keeps the label readable over light images.
    format!(
        r##"<text x="98" y="179" font-family="Segoe UI" font-size="{size:.0}" font-weight="600" fill="{color}" text-anchor="middle" stroke="#000000" stroke-opacity="0.45" stroke-width="3" stroke-linejoin="round" paint-order="stroke">{}</text>"##,
        escape(&shown)
    )
}

fn rasterize(svg: &str) -> Result<RgbaImage> {
    let mut options = resvg::usvg::Options::default();
    options.fontdb = fonts();
    let tree = resvg::usvg::Tree::from_str(svg, &options)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(ICON_SIZE, ICON_SIZE).ok_or_else(|| anyhow!("pixmap"))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    let mut img = RgbaImage::new(ICON_SIZE, ICON_SIZE);
    for (dst, src) in img.pixels_mut().zip(pixmap.pixels()) {
        let c = src.demultiply();
        *dst = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(img)
}

/// Segoe UI from Windows, loaded once. Elsewhere drawings have no text.
pub(crate) fn fonts() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    use std::sync::{Arc, OnceLock};
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
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

/// Cuts text to `max` characters, ending in "…".
pub(crate) fn fit(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Places an icon taken from an app or a site like the built-in glyphs:
/// centered in the upper part of a transparent square, leaving room for the
/// label. The result is saved as the key's PNG.
pub fn frame(icon: RgbaImage) -> Result<Vec<u8>> {
    const CANVAS: u32 = 256;
    const SIZE: u32 = 140;
    const TOP: u32 = 36;
    let fitted = DynamicImage::ImageRgba8(icon).resize(SIZE, SIZE, imageops::FilterType::Lanczos3).to_rgba8();
    let mut canvas = RgbaImage::new(CANVAS, CANVAS);
    let x = (CANVAS - fitted.width()) / 2;
    let y = TOP + (SIZE - fitted.height()) / 2;
    imageops::overlay(&mut canvas, &fitted, x.into(), y.into());
    encode(canvas)
}

/// `frame` for an encoded image (a site's PNG favicon).
pub fn frame_bytes(bytes: &[u8]) -> Result<Vec<u8>> {
    frame(image::load_from_memory(bytes).context("o ícone do site não é uma imagem PNG")?.to_rgba8())
}

/// The icon Windows shows for an exe, framed for a key.
#[cfg(windows)]
pub fn app_icon(exe: &Path) -> Result<Vec<u8>> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
        BI_RGB, DIB_RGB_COLORS, HGDIOBJ,
    };
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(&HSTRING::from(exe.as_os_str()), None)
            .with_context(|| format!("o Windows não achou {}", exe.display()))?;
        let bitmap = factory.GetImage(SIZE { cx: 256, cy: 256 }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)?;
        let object = HGDIOBJ(bitmap.0);
        let mut info = BITMAP::default();
        GetObjectW(object, std::mem::size_of::<BITMAP>() as i32, Some(&mut info as *mut BITMAP as *mut _));
        let (width, height) = (info.bmWidth.max(0) as u32, info.bmHeight.unsigned_abs());
        let mut header = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                // Negative: rows top to bottom.
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        let dc = CreateCompatibleDC(None);
        let rows = GetDIBits(dc, bitmap, 0, height, Some(pixels.as_mut_ptr() as *mut _), &mut header, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(object);
        if rows == 0 || width == 0 {
            anyhow::bail!("o ícone de {} não pôde ser lido", exe.display());
        }
        let opaque = pixels.chunks_exact(4).all(|p| p[3] == 0);
        for p in pixels.chunks_exact_mut(4) {
            // Windows hands back BGRA with the color already multiplied by alpha.
            p.swap(0, 2);
            if opaque {
                p[3] = 255;
            } else if p[3] > 0 && p[3] < 255 {
                let alpha = p[3] as u32;
                for c in &mut p[..3] {
                    *c = ((*c as u32 * 255) / alpha).min(255) as u8;
                }
            }
        }
        let image = RgbaImage::from_raw(width, height, pixels).ok_or_else(|| anyhow!("ícone com tamanho inválido"))?;
        frame(image)
    }
}

#[cfg(not(windows))]
pub fn app_icon(_exe: &Path) -> Result<Vec<u8>> {
    anyhow::bail!("ícones de app só no Windows")
}

fn render_glyph(d: &str, background: &str, stroke: &str, labeled: bool) -> Result<Vec<u8>> {
    // With a label the glyph moves up to leave it room; alone it is centered.
    let top = if labeled { 30 } else { 48 };
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{s}" height="{s}" viewBox="0 0 {s} {s}"><rect width="{s}" height="{s}" fill="{background}"/><g transform="translate(48 {top}) scale(4.1667)"><path d="{d}" fill="none" stroke="{stroke}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></g></svg>"##,
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

    const BG: &str = "#24262B";

    fn look<'a>(label: &'a str, border: Option<&'a str>) -> Look<'a> {
        Look {
            icon: Some("mic"),
            background: BG,
            icon_color: "#F0A63A",
            border,
            label,
            text_color: "#FFFFFF",
            label_px: 24,
            dim: false,
        }
    }

    #[test]
    fn dimmed_keys_are_darker() {
        let bright = image::load_from_memory(&render_key(&look("Spotify", None), Path::new(".")).unwrap()).unwrap().to_rgba8();
        let dark = image::load_from_memory(&render_key(&Look { dim: true, ..look("Spotify", None) }, Path::new(".")).unwrap())
            .unwrap()
            .to_rgba8();
        let sum = |img: &RgbaImage| img.pixels().map(|p| p.0[..3].iter().map(|&c| c as u64).sum::<u64>()).sum::<u64>();
        assert!(sum(&dark) * 2 < sum(&bright), "dimmed is much darker");
    }

    #[test]
    fn keys_get_icon_color_frame_and_label() {
        let plain = image::load_from_memory(&render_key(&look("", None), Path::new(".")).unwrap()).unwrap().to_rgba8();
        assert!(plain.pixels().any(|p| p.0[..3] == [0xF0, 0xA6, 0x3A]), "icon in its color");
        let framed = image::load_from_memory(&render_key(&look("", Some("#E5533D")), Path::new(".")).unwrap()).unwrap().to_rgba8();
        assert_eq!(framed.get_pixel(98, 8).0, [0xE5, 0x53, 0x3D, 255], "frame along the edge");
        let labeled = image::load_from_memory(&render_key(&look("Mutar", None), Path::new(".")).unwrap()).unwrap().to_rgba8();
        let white_low = (150..190).any(|y| (40..156).any(|x| labeled.get_pixel(x, y).0[..3] == [255, 255, 255]));
        assert!(white_low, "label drawn at the bottom");
    }

    #[test]
    fn long_labels_are_cut() {
        assert_eq!(fit("Uma tecla com texto enorme", 10), "Uma tecla…");
        assert!(label_svg(&"x".repeat(80), "#FFFFFF", 24).contains('…'));
    }

    #[test]
    fn every_glyph_renders_at_device_size() {
        for (id, _, _) in GLYPHS {
            let png = render(Some(id), BG, None, Path::new(".")).unwrap();
            let img = image::load_from_memory(&png).unwrap();
            assert_eq!((img.width(), img.height()), (ICON_SIZE, ICON_SIZE), "{id}");
            // Something was drawn over the background.
            let rgba = img.to_rgba8();
            assert!(rgba.pixels().any(|p| p.0[0] > 0xC0), "{id} rendered empty");
        }
    }

    #[test]
    fn glyph_ids_are_unique() {
        let mut ids: Vec<&str> = GLYPHS.iter().map(|(id, _, _)| *id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    #[test]
    fn user_png_is_fitted_on_the_background() {
        let dir = std::env::temp_dir().join("deck-engine-icon-test");
        std::fs::create_dir_all(&dir).unwrap();
        RgbaImage::from_pixel(400, 100, Rgba([255, 0, 0, 255])).save(dir.join("wide.png")).unwrap();
        let png = render(Some("wide.png"), BG, None, &dir).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (ICON_SIZE, ICON_SIZE));
        assert_eq!(img.get_pixel(98, 98), &Rgba([255, 0, 0, 255]));
        assert_eq!(img.get_pixel(98, 5), &Rgba([0x24, 0x26, 0x2B, 255]));
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(render(Some("nao-existe.png"), BG, None, Path::new(".")).is_err());
    }

    #[test]
    fn framed_icons_sit_in_the_upper_middle() {
        let png = frame(RgbaImage::from_pixel(32, 32, Rgba([0, 200, 0, 255]))).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(img.dimensions(), (256, 256));
        assert_eq!(img.get_pixel(128, 100).0[1], 200);
        assert_eq!(img.get_pixel(128, 240).0[3], 0, "room for the label stays transparent");
    }

    #[cfg(windows)]
    #[test]
    fn extracts_an_exe_icon() {
        let png = app_icon(Path::new(r"C:\Windows\explorer.exe")).unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        assert!(img.pixels().any(|p| p.0[3] > 0), "the icon has visible pixels");
    }
}
