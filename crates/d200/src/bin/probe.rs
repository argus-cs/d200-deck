//! Phase 0 probe: checks that Windows can talk to the D200.
//! Close Ulanzi Studio first, otherwise both programs fight over the device.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use d200::device::D200;
use d200::layout::KeyView;
use d200::protocol::{Incoming, WindowMode, ICON_SIZE, KEY_COUNT, VENDOR_ID};
use hidapi::HidApi;
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, DynamicImage, Frame, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};
use serde_json::json;

const USAGE: &str = "uso: probe <comando> [--keepalive <segundos>] [--window <clock|stats|image>] [--no-report-id]

comandos:
  list                 lista as interfaces HID do D200
  listen               mostra as teclas apertadas
  test                 manda 14 teclas coloridas numeradas e fica ouvindo
  partial <1-14>       troca só uma tecla (testa a atualização parcial 0x000D) e sai
  screen-test <w> <h>  manda uma imagem de teste w×h para o visor e fica ouvindo (use --window image)
  gif [arquivo.gif]    manda um GIF animado para uma tecla e outro para o visor e fica ouvindo por
                       --seconds (padrão 60); sem arquivo, a tecla recebe um GIF de teste
                       opções: --key <1-13> (padrão 7), --frames <n> (padrão 24, a 20 por segundo),
                       --noise (fundo de ruído, que quase não comprime: GIFs grandes para achar o limite),
                       --save <arquivo> (só grava o GIF da tecla e sai, sem falar com o aparelho),
                       --once (o GIF de teste toca uma vez só), --no-screen (não mexe no visor)
  brightness <0-100>   muda o brilho
  window <clock|stats|image>  muda o visor

--window escolhe o modo do visor que o keep-alive reenvia (padrão: clock)";

/// The visor's measured proportion (`screen::WIDTH/HEIGHT` in the engine).
const SCREEN_SIZE: (u32, u32) = (458, 196);

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let keepalive = match flag_value(&args, "--keepalive") {
        Some(v) => v.parse::<u64>().context("--keepalive precisa ser um número de segundos")?,
        None => 5,
    };
    let mode = window_mode(flag_value(&args, "--window").unwrap_or("clock"))?;
    let report_id = !args.iter().any(|a| a == "--no-report-id");
    let api = HidApi::new()?;

    match args.first().map(String::as_str) {
        Some("list") => list(&api),
        Some("listen") => listen(&open(&api, report_id)?, keepalive, mode, None),
        Some("test") => {
            let d = open(&api, report_id)?;
            send_test_layout(&d, mode)?;
            listen(&d, keepalive, mode, None)
        }
        Some("partial") => {
            let key: usize = arg(&args, 1)?.parse().context("tecla de 1 a 14")?;
            if !(1..=KEY_COUNT).contains(&key) {
                bail!("tecla de 1 a {KEY_COUNT}");
            }
            let d = open(&api, report_id)?;
            let keys = BTreeMap::from([(key - 1, KeyView { text: "PARCIAL".into(), image: Some(icon([240, 240, 240])?) })]);
            let size = d.set_layout(&keys, true)?;
            println!("atualização parcial enviada para a tecla {key} ({size} bytes)");
            println!("confira: só a tecla {key} deve ter mudado.");
            Ok(())
        }
        Some("screen-test") => {
            let width: u32 = arg(&args, 1)?.parse().context("largura em pixels")?;
            let height: u32 = arg(&args, 2)?.parse().context("altura em pixels")?;
            let d = open(&api, report_id)?;
            let keys = BTreeMap::from([(KEY_COUNT - 1, KeyView { text: String::new(), image: Some(screen_pattern(width, height)?) })]);
            let size = d.set_layout(&keys, true)?;
            d.set_small_window(mode, 0, 0, 0)?;
            println!("imagem de teste {width}x{height} enviada para o visor ({size} bytes)");
            println!("esperado: metade esquerda azul, direita verde, borda vermelha, círculo branco no meio, quadradinhos brancos nos 4 cantos");
            listen(&d, keepalive, mode, None)
        }
        Some("gif") => {
            let key: usize = number_flag(&args, "--key", 7)?;
            if !(1..KEY_COUNT).contains(&key) {
                bail!("--key precisa ser uma tecla de 1 a {}", KEY_COUNT - 1);
            }
            let frames: u32 = number_flag(&args, "--frames", 24)?;
            if frames == 0 {
                bail!("--frames precisa ser pelo menos 1");
            }
            let seconds: u64 = number_flag(&args, "--seconds", 60)?;
            let noise = args.iter().any(|a| a == "--noise");
            let once = args.iter().any(|a| a == "--once");
            let screen = !args.iter().any(|a| a == "--no-screen");
            let key_gif = match args.get(1).filter(|a| !a.starts_with("--")) {
                Some(path) => {
                    let bytes = std::fs::read(path).with_context(|| format!("não abriu {path}"))?;
                    if !bytes.starts_with(b"GIF8") {
                        bail!("{path} não é um GIF");
                    }
                    bytes
                }
                None => test_gif(ICON_SIZE, ICON_SIZE, frames, noise, once)?,
            };
            if let Some(path) = flag_value(&args, "--save") {
                std::fs::write(path, &key_gif)?;
                println!("GIF da tecla gravado em {path} ({} bytes); o aparelho não foi tocado", key_gif.len());
                return Ok(());
            }
            let d = open(&api, report_id)?;
            // Separate updates: if the device rejects one, the other still shows.
            let one = |index: usize, gif: &[u8]| {
                d.set_layout(&BTreeMap::from([(index, KeyView { text: String::new(), image: Some(gif.to_vec()) })]), true)
            };
            let sent = one(key - 1, &key_gif)?;
            println!("GIF da tecla {key}: {} bytes (zip de {sent} bytes)", key_gif.len());
            if screen {
                let screen_gif = test_gif(SCREEN_SIZE.0, SCREEN_SIZE.1, frames, noise, once)?;
                let sent = one(KEY_COUNT - 1, &screen_gif)?;
                println!("GIF do visor: {} bytes (zip de {sent} bytes)", screen_gif.len());
                d.set_small_window(WindowMode::Background, 0, 0, 0)?;
            }
            println!(
                "esperado (GIF de teste, {frames} quadros, {:.1} s por volta): um ponto branco girando e uma barra colorida enchendo embaixo, na tecla {key} e no visor",
                frames as f32 * 0.05
            );
            println!("imagem parada = o aparelho mostra só o primeiro quadro; tecla vazia = não aceitou o GIF");
            listen(&d, keepalive, WindowMode::Background, Some(Duration::from_secs(seconds)))
        }
        Some("brightness") => {
            let percent: u8 = arg(&args, 1)?.parse().context("brilho de 0 a 100")?;
            open(&api, report_id)?.set_brightness(percent)?;
            println!("brilho enviado: {percent}%");
            Ok(())
        }
        Some("window") => {
            let mode = window_mode(arg(&args, 1)?)?;
            open(&api, report_id)?.set_small_window(mode, 9, 64, 12)?;
            println!("visor atualizado");
            Ok(())
        }
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    }
}

fn open(api: &HidApi, report_id: bool) -> Result<D200> {
    let mut d = D200::open(api)?;
    d.set_report_id_prefix(report_id);
    println!("D200 aberto (prefixo de report ID: {})", if report_id { "sim" } else { "não" });
    Ok(d)
}

fn list(api: &HidApi) -> Result<()> {
    let mut found = false;
    for info in api.device_list().filter(|d| d.vendor_id() == VENDOR_ID) {
        found = true;
        println!(
            "interface {:>2}  usage page 0x{:04X}  usage 0x{:04X}  {:?}  serial {:?}",
            info.interface_number(),
            info.usage_page(),
            info.usage(),
            info.product_string().unwrap_or("?"),
            info.serial_number().unwrap_or("?"),
        );
        println!("  path: {}", info.path().to_string_lossy());
        match info.open_device(api) {
            Ok(dev) => {
                let mut desc = [0u8; 4096];
                match dev.get_report_descriptor(&mut desc) {
                    Ok(n) => println!("  report descriptor ({n} bytes): {}", hex(&desc[..n])),
                    Err(e) => println!("  report descriptor indisponível: {e}"),
                }
            }
            Err(e) => println!("  não abriu: {e}"),
        }
    }
    if !found {
        println!("nenhum dispositivo com VID 2207 encontrado");
    }
    Ok(())
}

fn window_mode(name: &str) -> Result<WindowMode> {
    Ok(match name {
        "clock" => WindowMode::Clock,
        "stats" => WindowMode::Stats,
        "image" => WindowMode::Background,
        other => bail!("modo de visor desconhecido: {other} (use clock, stats ou image)"),
    })
}

fn send_test_layout(d: &D200, mode: WindowMode) -> Result<()> {
    d.set_brightness(80)?;
    d.set_label_style(&json!({
        "Align": "bottom", "Color": 0xFFFFFF, "FontName": "Roboto",
        "ShowTitle": true, "Size": 10, "Weight": 80
    }))?;
    let mut keys = BTreeMap::new();
    for i in 0..KEY_COUNT {
        let hue = i as f32 * 360.0 / KEY_COUNT as f32;
        keys.insert(i, KeyView { text: format!("{}", i + 1), image: Some(icon(hsv(hue, 0.65, 0.85))?) });
    }
    let size = d.set_layout(&keys, false)?;
    d.set_small_window(mode, 9, 64, 12)?;
    println!("layout de teste enviado ({size} bytes): as teclas devem aparecer coloridas e numeradas de 1 a 14");
    Ok(())
}

/// `until` ends it after that long; `None` listens until Ctrl+C.
fn listen(d: &D200, keepalive_secs: u64, mode: WindowMode, until: Option<Duration>) -> Result<()> {
    let how_long = match until {
        Some(t) => format!("por {} s, Ctrl+C para sair antes", t.as_secs()),
        None => "Ctrl+C para sair".into(),
    };
    println!("ouvindo ({how_long}; keep-alive a cada {keepalive_secs} s, 0 = desligado; visor: {mode:?})");
    let start = Instant::now();
    let mut last = Instant::now();
    loop {
        if until.is_some_and(|t| start.elapsed() >= t) {
            return Ok(());
        }
        if keepalive_secs > 0 && last.elapsed() >= Duration::from_secs(keepalive_secs) {
            d.set_small_window(mode, 9, 64, 12)?;
            last = Instant::now();
        }
        match d.read(200)? {
            Some((Incoming::Button { index, pressed, state }, _)) => println!(
                "tecla {:>2} {} (state {state})",
                index as usize + 1,
                if pressed { "apertada" } else { "solta" }
            ),
            Some((Incoming::DeviceInfo(info), _)) => println!("info do aparelho: {info}"),
            Some((Incoming::Unknown { command, length }, raw)) => println!(
                "pacote desconhecido 0x{command:04X} (len {length}): {}",
                hex(&raw[..raw.len().min(32)])
            ),
            None => {}
        }
    }
}

fn icon(rgb: [u8; 3]) -> Result<Vec<u8>> {
    let border = 8;
    let img = RgbImage::from_fn(ICON_SIZE, ICON_SIZE, |x, y| {
        let edge = x < border || y < border || x >= ICON_SIZE - border || y >= ICON_SIZE - border;
        if edge { Rgb([255, 255, 255]) } else { Rgb(rgb) }
    });
    let mut png = Vec::new();
    DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

/// Shows how the visor fits an image: halves in two colors (cropping), a
/// circle (stretching), a border and corner squares (what reaches the edges).
fn screen_pattern(width: u32, height: u32) -> Result<Vec<u8>> {
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let radius = height as f32 * 0.3;
    let corner = (height / 8).max(6);
    let img = RgbImage::from_fn(width, height, |x, y| {
        let border = x < 6 || y < 6 || x >= width - 6 || y >= height - 6;
        let in_corner = |ax: u32, ay: u32| x >= ax && x < ax + corner && y >= ay && y < ay + corner;
        let corners = in_corner(8, 8) || in_corner(width - 8 - corner, 8) || in_corner(8, height - 8 - corner)
            || in_corner(width - 8 - corner, height - 8 - corner);
        let (dx, dy) = (x as f32 - cx, y as f32 - cy);
        let ring = ((dx * dx + dy * dy).sqrt() - radius).abs() < 4.0;
        if border {
            Rgb([220, 40, 40])
        } else if corners || ring {
            Rgb([255, 255, 255])
        } else if x < width / 2 {
            Rgb([40, 90, 220])
        } else {
            Rgb([40, 170, 80])
        }
    });
    let mut png = Vec::new();
    DynamicImage::ImageRgb8(img).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

/// A looping animation at 20 frames per second: a white dot circling the
/// middle and a bar filling along the bottom, changing color as it goes.
/// `noise` fills the background with random pixels, which barely compress;
/// `once` leaves out the loop extension, so the GIF plays a single time.
fn test_gif(width: u32, height: u32, frames: u32, noise: bool, once: bool) -> Result<Vec<u8>> {
    let mut gif = Vec::new();
    {
        let mut encoder = GifEncoder::new_with_speed(&mut gif, 30);
        if !once {
            encoder.set_repeat(Repeat::Infinite)?;
        }
        let mut seed = 0x2545_F491_4F6C_DD1D_u64;
        let (w, h) = (width as f32, height as f32);
        for i in 0..frames {
            let t = i as f32 / frames as f32;
            let angle = t * std::f32::consts::TAU;
            let (dot_x, dot_y) = (w / 2.0 + h * 0.28 * angle.cos(), h * 0.45 + h * 0.28 * angle.sin());
            let bar_end = ((i + 1) as f32 / frames as f32 * w) as u32;
            let [r, g, b] = hsv(t * 360.0, 0.7, 0.9);
            let img = RgbaImage::from_fn(width, height, |x, y| {
                let (dx, dy) = (x as f32 - dot_x, y as f32 - dot_y);
                if (dx * dx + dy * dy).sqrt() < h * 0.1 {
                    Rgba([255, 255, 255, 255])
                } else if y >= height - height / 10 && x < bar_end {
                    Rgba([r, g, b, 255])
                } else if noise {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    Rgba([seed as u8, (seed >> 8) as u8, (seed >> 16) as u8, 255])
                } else {
                    Rgba([24, 24, 32, 255])
                }
            });
            encoder.encode_frame(Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(50, 1)))?;
        }
    }
    Ok(gif)
}

fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0..=59 => (c, x, 0.0),
        60..=119 => (x, c, 0.0),
        120..=179 => (0.0, c, x),
        180..=239 => (0.0, x, c),
        240..=299 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8]
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn number_flag<T: std::str::FromStr>(args: &[String], flag: &str, default: T) -> Result<T> {
    match flag_value(args, flag) {
        Some(v) => v.parse().map_err(|_| anyhow::anyhow!("{flag} precisa ser um número, veio {v:?}")),
        None => Ok(default),
    }
}

fn arg<'a>(args: &'a [String], i: usize) -> Result<&'a str> {
    args.get(i).map(String::as_str).with_context(|| format!("faltou argumento\n\n{USAGE}"))
}
