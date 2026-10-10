//! The visor (key 14): the data it can show and how the app draws it.
//! The device stretches whatever it gets to its 458×196 panel, so images are
//! drawn at that size (measured on the device).

use std::fmt::Write as _;
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use chrono::{Datelike, Local, Timelike};
use d200::protocol::WindowMode;
use image::{imageops, DynamicImage, ImageFormat, Rgba, RgbaImage};

use crate::config::{Screen, ScreenContent};
use crate::icons::{self, escape, fit, fonts};

pub const WIDTH: u32 = 458;
pub const HEIGHT: u32 = 196;

/// A sample of how busy the PC is. Rates are bytes per second.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub cpu: f32,
    pub memory: f32,
    pub gpu: Option<f32>,
    pub down: f64,
    pub up: f64,
}

#[derive(Clone, Debug)]
pub struct NowPlaying {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// The cover or video thumbnail the player publishes (JPEG or PNG).
    pub art: Option<Arc<Vec<u8>>>,
    /// Tells covers apart without comparing their bytes.
    pub art_id: u64,
}

impl NowPlaying {
    pub fn new(title: &str, artist: &str, playing: bool) -> Self {
        Self { title: title.into(), artist: artist.into(), playing, art: None, art_id: 0 }
    }

    pub fn with_art(mut self, bytes: Vec<u8>) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        bytes.hash(&mut hasher);
        self.art_id = hasher.finish();
        self.art = Some(Arc::new(bytes));
        self
    }
}

impl PartialEq for NowPlaying {
    fn eq(&self, other: &Self) -> bool {
        (&self.title, &self.artist, self.playing, self.art_id) == (&other.title, &other.artist, other.playing, other.art_id)
    }
}

impl Eq for NowPlaying {}

/// Stopwatch or countdown, depending on the screen's minutes.
#[derive(Clone, Debug, Default)]
pub struct Timer {
    running_since: Option<Instant>,
    banked: Duration,
}

impl Timer {
    pub fn toggle(&mut self) {
        match self.running_since.take() {
            Some(since) => self.banked += since.elapsed(),
            None => self.running_since = Some(Instant::now()),
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn running(&self) -> bool {
        self.running_since.is_some()
    }

    pub fn elapsed(&self) -> Duration {
        self.banked + self.running_since.map(|s| s.elapsed()).unwrap_or_default()
    }
}

/// Everything a screen may show right now.
pub struct Data<'a> {
    pub now: chrono::DateTime<Local>,
    pub usage: &'a Usage,
    pub playing: Option<&'a NowPlaying>,
    pub timer: &'a Timer,
}

/// The mode the device's own window must be in for this screen.
pub fn window_mode(screen: &Screen) -> WindowMode {
    match screen.content {
        ScreenContent::DeviceClock => WindowMode::Clock,
        ScreenContent::DeviceStats => WindowMode::Stats,
        _ => WindowMode::Background,
    }
}

pub fn uses_usage(screen: &Screen) -> bool {
    matches!(screen.content, ScreenContent::Stats { .. } | ScreenContent::ClockStats { .. })
}

pub fn uses_now_playing(screen: &Screen) -> bool {
    matches!(screen.content, ScreenContent::NowPlaying)
}

/// The drawing for the device, or `None` when the device draws itself.
/// `base` resolves relative image paths.
pub fn render(screen: &Screen, data: &Data, base: &Path) -> Result<Option<Vec<u8>>> {
    let body = match &screen.content {
        ScreenContent::DeviceClock | ScreenContent::DeviceStats => return Ok(None),
        ScreenContent::Image { icon: Some(icon) } if !icons::is_glyph(icon) => {
            return image_screen(screen, &base.join(icon)).map(Some);
        }
        content => body(screen, content, data),
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{HEIGHT}" viewBox="0 0 {WIDTH} {HEIGHT}"><rect width="{WIDTH}" height="{HEIGHT}" fill="{}"/>{body}</svg>"#,
        screen_background(screen, data)
    );
    let mut options = resvg::usvg::Options::default();
    options.fontdb = fonts();
    let tree = resvg::usvg::Tree::from_str(&svg, &options)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(WIDTH, HEIGHT).ok_or_else(|| anyhow!("pixmap"))?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
    let png = pixmap.encode_png()?;
    // The cover is a photo: laid over the drawing rather than drawn in it.
    match (&screen.content, data.playing.and_then(|s| s.art.as_deref())) {
        (ScreenContent::NowPlaying, Some(art)) => Ok(Some(with_cover(&png, art).unwrap_or(png))),
        _ => Ok(Some(png)),
    }
}

/// Where the cover goes on a "now playing" visor.
const COVER: (u32, u32, u32, u32) = (24, 38, 120, 18);

/// Puts the cover, cropped square with round corners, over the drawn visor.
fn with_cover(png: &[u8], art: &[u8]) -> Result<Vec<u8>> {
    let (x, y, size, radius) = COVER;
    let mut canvas = image::load_from_memory(png)?.to_rgba8();
    let cover = image::load_from_memory(art)?;
    let side = cover.width().min(cover.height());
    let square = cover.crop_imm((cover.width() - side) / 2, (cover.height() - side) / 2, side, side);
    let fitted = square.resize_exact(size, size, imageops::FilterType::Lanczos3).to_rgba8();
    for (px, py, pixel) in fitted.enumerate_pixels() {
        if inside_rounded(px, py, size, radius) {
            canvas.put_pixel(x + px, y + py, Rgba([pixel[0], pixel[1], pixel[2], 255]));
        }
    }
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(canvas).write_to(&mut Cursor::new(&mut out), ImageFormat::Png)?;
    Ok(out)
}

fn inside_rounded(x: u32, y: u32, size: u32, radius: u32) -> bool {
    let near = |v: u32| if v < radius { radius - v } else if v >= size - radius { v + radius + 1 - size } else { 0 };
    let (dx, dy) = (near(x), near(y));
    dx == 0 || dy == 0 || dx * dx + dy * dy <= radius * radius
}

/// The visor with example data, for the editor's preview.
pub fn preview(screen: &Screen, base: &Path) -> Result<Option<Vec<u8>>> {
    let usage = Usage { cpu: 32.0, memory: 58.0, gpu: Some(18.0), down: 1_200_000.0, up: 150_000.0 };
    let song = NowPlaying::new("Nome da música", "Artista", true);
    let timer = Timer::default();
    render(screen, &Data { now: Local::now(), usage: &usage, playing: Some(&song), timer: &timer }, base)
}

/// What would change on the visor: draw again only when this changes.
pub fn signature(screen: &Screen, data: &Data) -> String {
    let content = match &screen.content {
        ScreenContent::DeviceClock | ScreenContent::DeviceStats | ScreenContent::Image { .. } | ScreenContent::Text { .. } => {
            String::new()
        }
        ScreenContent::Clock { hour24, seconds, date } => {
            format!("{}|{}", clock_text(&data.now, *hour24, *seconds), if *date { date_text(&data.now) } else { String::new() })
        }
        ScreenContent::ClockStats { hour24, seconds } => format!(
            "{}|{}|{:.0}|{:.0}|{:?}",
            clock_text(&data.now, *hour24, *seconds),
            date_text(&data.now),
            data.usage.cpu,
            data.usage.memory,
            data.usage.gpu.map(f32::round)
        ),
        ScreenContent::Stats { .. } => format!(
            "{:.0}|{:.0}|{:?}|{}|{}",
            data.usage.cpu,
            data.usage.memory,
            data.usage.gpu.map(f32::round),
            rate(data.usage.down),
            rate(data.usage.up)
        ),
        ScreenContent::NowPlaying => match data.playing {
            Some(song) => format!("{}|{}|{}|{}", song.title, song.artist, song.playing, song.art_id),
            None => "nada".into(),
        },
        ScreenContent::Timer { minutes } => {
            let (text, _, done) = timer_text(*minutes, data.timer);
            format!("{text}|{}|{done}", data.timer.running())
        }
    };
    format!("{:?}|{}|{}|{}|{content}", screen.content, screen.background, screen.color, screen.accent)
}

fn screen_background(screen: &Screen, data: &Data) -> String {
    match screen.content {
        // A finished countdown fills the visor with the accent color.
        ScreenContent::Timer { minutes } if timer_text(minutes, data.timer).2 => screen.accent.clone(),
        _ => screen.background.clone(),
    }
}

fn body(screen: &Screen, content: &ScreenContent, data: &Data) -> String {
    let (fg, accent) = (&screen.color, &screen.accent);
    let mut out = String::new();
    match content {
        ScreenContent::Clock { hour24, seconds, date } => {
            let size = if *seconds { 78 } else { 96 };
            let y = if *date { 112 } else { 128 };
            out += &clock_svg(&data.now, *hour24, *seconds, WIDTH / 2, y, size, fg);
            if *date {
                out += &text(WIDTH / 2, 160, 26, 400, fg, 0.7, "middle", &date_text(&data.now));
            }
        }
        ScreenContent::ClockStats { hour24, seconds } => {
            out += &clock_svg(&data.now, *hour24, *seconds, 122, 104, if *seconds { 50 } else { 64 }, fg);
            out += &text(122, 144, 20, 400, fg, 0.7, "middle", &date_text(&data.now));
            out += &format!(r#"<rect x="246" y="30" width="2" height="136" fill="{fg}" fill-opacity="0.15"/>"#);
            let mut rows = vec![("CPU", Some(data.usage.cpu)), ("RAM", Some(data.usage.memory))];
            if let Some(gpu) = data.usage.gpu {
                rows.push(("GPU", Some(gpu)));
            }
            let step = 136 / rows.len() as u32;
            for (i, (label, value)) in rows.iter().enumerate() {
                let y = 30 + step * i as u32 + step / 2;
                out += &bar_row(264, 440, y, 18, 72, label, *value, fg, accent);
            }
        }
        ScreenContent::Stats { cpu, memory, gpu, network } => {
            let mut rows: Vec<(&str, Option<f32>)> = Vec::new();
            if *cpu {
                rows.push(("CPU", Some(data.usage.cpu)));
            }
            if *memory {
                rows.push(("Memória", Some(data.usage.memory)));
            }
            if *gpu {
                rows.push(("GPU", data.usage.gpu));
            }
            let total = rows.len() + usize::from(*network);
            if total == 0 {
                out += &text(WIDTH / 2, 108, 26, 400, fg, 0.7, "middle", "Escolha o que mostrar");
            } else {
                let step = (HEIGHT - 24) / total as u32;
                let size = if total > 3 { 20 } else { 24 };
                for (i, (label, value)) in rows.iter().enumerate() {
                    let y = 12 + step * i as u32 + step / 2;
                    // Room for "Memória", the longest label.
                    out += &bar_row(22, 436, y, size, size * 5, label, *value, fg, accent);
                }
                if *network {
                    let y = 12 + step * rows.len() as u32 + step / 2;
                    let size = if total > 3 { 20 } else { 24 };
                    out += &text(22, y + size / 3, size, 400, fg, 0.7, "start", "Rede");
                    let speeds = format!("↓ {}   ↑ {}", rate(data.usage.down), rate(data.usage.up));
                    out += &text(436, y + size / 3, size, 600, fg, 1.0, "end", &speeds);
                }
            }
        }
        ScreenContent::NowPlaying => match data.playing {
            None => {
                out += &glyph_svg("music", 154, 46, 56, fg, 0.5);
                out += &text(250, 112, 30, 600, fg, 0.6, "start", "Nada tocando");
            }
            Some(song) => {
                // With a cover, `render` lays it in this spot afterwards.
                if song.art.is_none() {
                    out += &format!(r#"<rect x="24" y="38" width="120" height="120" rx="18" fill="{accent}"/>"#);
                    out += &glyph_svg(if song.playing { "music" } else { "pause" }, 44, 58, 80, &screen.background, 1.0);
                }
                out += &text(164, 88, 32, 700, fg, 1.0, "start", &fit(&song.title, 16));
                out += &text(164, 128, 24, 400, fg, 0.7, "start", &fit(&song.artist, 22));
                let state = if song.playing { "Tocando" } else { "Pausado" };
                out += &text(164, 164, 18, 600, accent, 1.0, "start", state);
            }
        },
        ScreenContent::Timer { minutes } => {
            let (shown, progress, done) = timer_text(*minutes, data.timer);
            let ink = if done { screen.background.as_str() } else { fg.as_str() };
            out += &text(WIDTH / 2, 112, 92, 600, ink, 1.0, "middle", &shown);
            let label = match (*minutes, done, data.timer.running(), data.timer.elapsed().is_zero()) {
                (_, true, _, _) => "Tempo esgotado · toque duplo zera".to_string(),
                (0, _, true, _) => "Cronômetro · toque para pausar".to_string(),
                (_, _, true, _) => format!("{minutes} min · toque para pausar"),
                (_, _, false, true) => "Toque para iniciar".to_string(),
                _ => "Pausado · toque duplo zera".to_string(),
            };
            out += &text(WIDTH / 2, 156, 22, 400, ink, 0.75, "middle", &label);
            if let Some(progress) = progress {
                let width = ((WIDTH - 48) as f32 * progress) as u32;
                out += &format!(r#"<rect x="24" y="176" width="{}" height="6" rx="3" fill="{fg}" fill-opacity="0.15"/>"#, WIDTH - 48);
                // usvg rejects a 0 width with a warning; there is nothing to draw anyway.
                if width > 0 {
                    out += &format!(r#"<rect x="24" y="176" width="{width}" height="6" rx="3" fill="{accent}"/>"#);
                }
            }
        }
        ScreenContent::Image { icon } => match icon.as_deref() {
            Some(glyph) => out += &glyph_svg(glyph, (WIDTH - 140) / 2, 28, 140, fg, 1.0),
            None => out += &text(WIDTH / 2, 108, 26, 400, fg, 0.6, "middle", "Escolha uma imagem"),
        },
        ScreenContent::Text { text: words } => out += &text_block(words, fg),
        ScreenContent::DeviceClock | ScreenContent::DeviceStats => {}
    }
    out
}

/// A PNG of the person's, fitted on the background color, or an SVG icon
/// at the size and in the color of a built-in glyph.
fn image_screen(screen: &Screen, path: &Path) -> Result<Vec<u8>> {
    let background = parse_rgb(&screen.background);
    let mut canvas = RgbaImage::from_pixel(WIDTH, HEIGHT, background);
    let fitted = if icons::is_svg(&path.to_string_lossy()) {
        icons::svg_icon(path, &screen.color, 140)?
    } else {
        let img = image::open(path).map_err(|e| anyhow!("não consegui abrir {}: {e}", path.display()))?;
        img.resize(WIDTH - 24, HEIGHT - 24, imageops::FilterType::Lanczos3).to_rgba8()
    };
    let x = (WIDTH - fitted.width()) / 2;
    let y = (HEIGHT - fitted.height()) / 2;
    imageops::overlay(&mut canvas, &fitted, x.into(), y.into());
    let mut png = Vec::new();
    DynamicImage::ImageRgba8(canvas).write_to(&mut Cursor::new(&mut png), ImageFormat::Png)?;
    Ok(png)
}

fn parse_rgb(hex: &str) -> Rgba<u8> {
    let h = hex.trim_start_matches('#');
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok()).unwrap_or(0);
    Rgba([byte(0), byte(2), byte(4), 255])
}

const WEEKDAYS: [&str; 7] = ["domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado"];
const MONTHS: [&str; 12] = [
    "janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro",
];

fn clock_text(now: &chrono::DateTime<Local>, hour24: bool, seconds: bool) -> String {
    let hour = if hour24 { now.hour() } else { (now.hour() + 11) % 12 + 1 };
    let mut out = if hour24 { format!("{hour:02}:{:02}", now.minute()) } else { format!("{hour}:{:02}", now.minute()) };
    if seconds {
        let _ = write!(out, ":{:02}", now.second());
    }
    out
}

/// "quarta, 8 de outubro".
fn date_text(now: &chrono::DateTime<Local>) -> String {
    let weekday = WEEKDAYS[now.weekday().num_days_from_sunday() as usize];
    format!("{weekday}, {} de {}", now.day(), MONTHS[now.month0() as usize])
}

fn clock_svg(now: &chrono::DateTime<Local>, hour24: bool, seconds: bool, x: u32, y: u32, size: u32, fg: &str) -> String {
    let main = escape(&clock_text(now, hour24, seconds));
    let suffix = if hour24 {
        String::new()
    } else {
        format!(r#"<tspan font-size="{}" dx="8">{}</tspan>"#, size * 2 / 5, if now.hour() < 12 { "AM" } else { "PM" })
    };
    format!(
        r#"<text x="{x}" y="{y}" font-family="Segoe UI" font-size="{size}" font-weight="600" fill="{fg}" text-anchor="middle">{main}{suffix}</text>"#
    )
}

/// The shown time, how far a countdown is (0 to 1), and whether it ran out.
fn timer_text(minutes: u32, timer: &Timer) -> (String, Option<f32>, bool) {
    let elapsed = timer.elapsed().as_secs();
    if minutes == 0 {
        return (duration_text(elapsed), None, false);
    }
    let total = u64::from(minutes) * 60;
    let left = total.saturating_sub(elapsed);
    (duration_text(left), Some((elapsed.min(total) as f32) / total as f32), left == 0)
}

fn duration_text(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m:02}:{s:02}") }
}

/// "1,2 MB/s".
fn rate(bytes_per_second: f64) -> String {
    let (value, unit) = if bytes_per_second >= 1_000_000.0 {
        (bytes_per_second / 1_000_000.0, "MB/s")
    } else {
        (bytes_per_second / 1_000.0, "KB/s")
    };
    let digits = if value < 10.0 { 1 } else { 0 };
    format!("{value:.digits$} {unit}").replace('.', ",")
}

#[allow(clippy::too_many_arguments)]
fn bar_row(x0: u32, x1: u32, y: u32, size: u32, label_width: u32, label: &str, value: Option<f32>, fg: &str, accent: &str) -> String {
    let value_width = size * 3;
    let bar_x = x0 + label_width;
    let bar_width = x1.saturating_sub(bar_x + value_width);
    let height = (size / 2).max(8);
    let mut out = text(x0, y + size / 3, size, 400, fg, 0.7, "start", label);
    out += &format!(
        r#"<rect x="{bar_x}" y="{}" width="{bar_width}" height="{height}" rx="{}" fill="{fg}" fill-opacity="0.15"/>"#,
        y - height / 2,
        height / 2
    );
    match value {
        Some(v) => {
            let filled = (bar_width as f32 * (v.clamp(0.0, 100.0) / 100.0)) as u32;
            // At 0% (an idle GPU) there is no bar, and usvg warns about a 0 width.
            if filled > 0 {
                out += &format!(
                    r#"<rect x="{bar_x}" y="{}" width="{filled}" height="{height}" rx="{}" fill="{accent}"/>"#,
                    y - height / 2,
                    height / 2
                );
            }
            out += &text(x1, y + size / 3, size, 700, fg, 1.0, "end", &format!("{:.0}%", v));
        }
        None => out += &text(x1, y + size / 3, size, 700, fg, 0.5, "end", "—"),
    }
    out
}

/// Free text, centered, as large as it fits (up to 3 lines).
fn text_block(words: &str, fg: &str) -> String {
    let lines: Vec<&str> = words.lines().map(str::trim).filter(|l| !l.is_empty()).take(3).collect();
    if lines.is_empty() {
        return text(WIDTH / 2, 108, 26, 400, fg, 0.6, "middle", "Escreva um texto");
    }
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f32;
    let by_width = (WIDTH as f32 - 40.0) / (longest * 0.56);
    let by_height = (HEIGHT as f32 - 30.0) / (lines.len() as f32 * 1.15);
    let size = by_width.min(by_height).clamp(18.0, 110.0) as u32;
    let line_height = size as f32 * 1.15;
    let first = HEIGHT as f32 / 2.0 - line_height * (lines.len() as f32 - 1.0) / 2.0 + size as f32 * 0.35;
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| text(WIDTH / 2, (first + line_height * i as f32) as u32, size, 600, fg, 1.0, "middle", line))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn text(x: u32, y: u32, size: u32, weight: u32, fill: &str, opacity: f32, anchor: &str, content: &str) -> String {
    format!(
        r#"<text x="{x}" y="{y}" font-family="Segoe UI" font-size="{size}" font-weight="{weight}" fill="{fill}" fill-opacity="{opacity}" text-anchor="{anchor}">{}</text>"#,
        escape(content)
    )
}

/// A built-in stroke glyph at (x, y), `size` pixels wide.
fn glyph_svg(id: &str, x: u32, y: u32, size: u32, stroke: &str, opacity: f32) -> String {
    let Some(d) = icons::glyph_path(id) else {
        return String::new();
    };
    let scale = size as f32 / 24.0;
    format!(
        r#"<g transform="translate({x} {y}) scale({scale})" opacity="{opacity}"><path d="{d}" fill="none" stroke="{stroke}" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></g>"#
    )
}

/// Samples CPU, memory, network and (on Windows) GPU use.
pub struct UsageSampler {
    sys: sysinfo::System,
    networks: sysinfo::Networks,
    last: Instant,
    #[cfg(windows)]
    gpu: Option<win::GpuCounter>,
}

impl Default for UsageSampler {
    fn default() -> Self {
        Self {
            sys: sysinfo::System::new(),
            networks: sysinfo::Networks::new_with_refreshed_list(),
            last: Instant::now(),
            #[cfg(windows)]
            gpu: win::GpuCounter::new(),
        }
    }
}

impl UsageSampler {
    pub fn sample(&mut self) -> Usage {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.networks.refresh(true);
        let seconds = self.last.elapsed().as_secs_f64().max(0.001);
        self.last = Instant::now();
        let (down, up) = self
            .networks
            .values()
            .fold((0u64, 0u64), |(d, u), n| (d + n.received(), u + n.transmitted()));
        Usage {
            cpu: self.sys.global_cpu_usage(),
            memory: (self.sys.used_memory() as f64 * 100.0 / self.sys.total_memory().max(1) as f64) as f32,
            #[cfg(windows)]
            gpu: self.gpu.as_ref().and_then(win::GpuCounter::sample),
            #[cfg(not(windows))]
            gpu: None,
            down: down as f64 / seconds,
            up: up as f64 / seconds,
        }
    }
}

/// Sends what Windows says is playing (any player that shows in the media
/// flyout), checked every 2 s.
#[cfg(windows)]
pub fn watch_now_playing() -> std::sync::mpsc::Receiver<Option<NowPlaying>> {
    win::watch_now_playing()
}

#[cfg(not(windows))]
pub fn watch_now_playing() -> std::sync::mpsc::Receiver<Option<NowPlaying>> {
    std::sync::mpsc::channel().1
}

#[cfg(windows)]
mod win {
    use std::sync::mpsc::{channel, Receiver};
    use std::time::Duration;

    use windows::core::w;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager, GlobalSystemMediaTransportControlsSessionMediaProperties,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus,
    };
    use windows::Storage::Streams::DataReader;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W,
        PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
    };

    use super::NowPlaying;

    /// GPU use from Windows' performance counters (what Task Manager shows for 3D).
    pub struct GpuCounter {
        query: PDH_HQUERY,
        counter: PDH_HCOUNTER,
    }

    impl GpuCounter {
        pub fn new() -> Option<Self> {
            unsafe {
                let mut query = PDH_HQUERY::default();
                if PdhOpenQueryW(None, 0, &mut query) != 0 {
                    return None;
                }
                let mut counter = PDH_HCOUNTER::default();
                if PdhAddEnglishCounterW(query, w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"), 0, &mut counter) != 0 {
                    return None;
                }
                // Rates need two samples; this is the first.
                let _ = PdhCollectQueryData(query);
                Some(Self { query, counter })
            }
        }

        pub fn sample(&self) -> Option<f32> {
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return None;
                }
                let (mut size, mut count) = (0u32, 0u32);
                let first = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
                if first != PDH_MORE_DATA || size == 0 {
                    return None;
                }
                let mut buffer = vec![0u8; size as usize];
                let items = buffer.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                    return None;
                }
                let values = std::slice::from_raw_parts(items, count as usize);
                let total: f64 = values.iter().map(|v| v.FmtValue.Anonymous.doubleValue).sum();
                Some(total.clamp(0.0, 100.0) as f32)
            }
        }
    }

    pub fn watch_now_playing() -> Receiver<Option<NowPlaying>> {
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            let manager = match GlobalSystemMediaTransportControlsSessionManager::RequestAsync().and_then(|op| op.get()) {
                Ok(manager) => manager,
                Err(e) => {
                    log::warn!("o Windows não informa o que está tocando: {e}");
                    return;
                }
            };
            let mut last: Option<Option<NowPlaying>> = None;
            loop {
                let previous = last.as_ref().and_then(|l| l.as_ref());
                let now = current(&manager, previous);
                if last.as_ref() != Some(&now) {
                    if tx.send(now.clone()).is_err() {
                        return;
                    }
                    last = Some(now);
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });
        rx
    }

    /// `previous` lends its cover when the song is the same, so the
    /// thumbnail is read once per song rather than every 2 s.
    fn current(manager: &GlobalSystemMediaTransportControlsSessionManager, previous: Option<&NowPlaying>) -> Option<NowPlaying> {
        let session = manager.GetCurrentSession().ok()?;
        let properties = session.TryGetMediaPropertiesAsync().ok()?.get().ok()?;
        let title = properties.Title().ok()?.to_string();
        if title.is_empty() {
            return None;
        }
        let artist = properties.Artist().map(|a| a.to_string()).unwrap_or_default();
        let playing = session
            .GetPlaybackInfo()
            .and_then(|info| info.PlaybackStatus())
            .is_ok_and(|status| status == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing);
        let mut song = NowPlaying::new(&title, &artist, playing);
        match previous.filter(|p| p.title == title && p.artist == artist && p.art.is_some()) {
            Some(same) => {
                song.art = same.art.clone();
                song.art_id = same.art_id;
            }
            None => {
                if let Some(bytes) = thumbnail(&properties) {
                    song = song.with_art(bytes);
                }
            }
        }
        Some(song)
    }

    fn thumbnail(properties: &GlobalSystemMediaTransportControlsSessionMediaProperties) -> Option<Vec<u8>> {
        let stream = properties.Thumbnail().ok()?.OpenReadAsync().ok()?.get().ok()?;
        let size = u32::try_from(stream.Size().ok()?).ok().filter(|&s| s > 0)?;
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0).ok()?).ok()?;
        reader.LoadAsync(size).ok()?.get().ok()?;
        let mut bytes = vec![0u8; size as usize];
        reader.ReadBytes(&mut bytes).ok()?;
        Some(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn data<'a>(usage: &'a Usage, timer: &'a Timer, playing: Option<&'a NowPlaying>) -> Data<'a> {
        Data { now: Local.with_ymd_and_hms(2026, 10, 8, 16, 5, 9).unwrap(), usage, playing, timer }
    }

    fn screens() -> Vec<ScreenContent> {
        vec![
            ScreenContent::Clock { hour24: true, seconds: false, date: true },
            ScreenContent::Clock { hour24: false, seconds: true, date: false },
            ScreenContent::Stats { cpu: true, memory: true, gpu: true, network: true },
            ScreenContent::ClockStats { hour24: true, seconds: false },
            ScreenContent::NowPlaying,
            ScreenContent::Timer { minutes: 25 },
            ScreenContent::Timer { minutes: 0 },
            ScreenContent::Image { icon: Some("mic".into()) },
            ScreenContent::Text { text: "Em reunião\nnão perturbe".into() },
        ]
    }

    #[test]
    fn every_content_draws_a_visor_image() {
        let usage = Usage { cpu: 23.0, memory: 58.0, gpu: Some(12.0), down: 1_200_000.0, up: 140_000.0 };
        let timer = Timer::default();
        let song = NowPlaying::new("Uma música com um nome bem comprido", "Artista", true);
        for content in screens() {
            let screen = Screen::new(content.clone());
            let png = render(&screen, &data(&usage, &timer, Some(&song)), Path::new(".")).unwrap().unwrap();
            let img = image::load_from_memory(&png).unwrap().to_rgba8();
            assert_eq!(img.dimensions(), (WIDTH, HEIGHT), "{content:?}");
            let background = img.get_pixel(1, 1).0;
            assert!(img.pixels().any(|p| p.0 != background), "{content:?} drew nothing");
        }
    }

    #[test]
    fn empty_bars_are_left_out() {
        // Idle: nothing measured yet, a countdown not started.
        let usage = Usage { gpu: Some(0.0), ..Usage::default() };
        let timer = Timer::default();
        for content in screens() {
            let screen = Screen::new(content.clone());
            let svg = body(&screen, &content, &data(&usage, &timer, None));
            assert!(!svg.contains(r#"width="0""#), "{content:?} has a 0-wide shape");
        }
    }

    #[test]
    fn the_cover_replaces_the_note_with_round_corners() {
        let mut cover = Vec::new();
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(300, 200, Rgba([200, 30, 30, 255])))
            .write_to(&mut Cursor::new(&mut cover), ImageFormat::Png)
            .unwrap();
        let song = NowPlaying::new("Música", "Artista", true).with_art(cover);
        let (usage, timer) = (Usage::default(), Timer::default());
        let screen = Screen::new(ScreenContent::NowPlaying);
        let png = render(&screen, &data(&usage, &timer, Some(&song)), Path::new(".")).unwrap().unwrap();
        let img = image::load_from_memory(&png).unwrap().to_rgba8();
        let (x, y, size, _) = COVER;
        assert_eq!(img.get_pixel(x + size / 2, y + size / 2).0, [200, 30, 30, 255], "cover in the middle");
        assert_ne!(img.get_pixel(x, y).0, [200, 30, 30, 255], "round corner left out");
        let plain = NowPlaying::new("Música", "Artista", true);
        assert_ne!(signature(&screen, &data(&usage, &timer, Some(&song))), signature(&screen, &data(&usage, &timer, Some(&plain))));
    }

    #[test]
    fn device_drawn_screens_send_no_image() {
        let (usage, timer) = (Usage::default(), Timer::default());
        let clock = Screen::new(ScreenContent::DeviceClock);
        assert!(render(&clock, &data(&usage, &timer, None), Path::new(".")).unwrap().is_none());
        assert_eq!(window_mode(&clock), WindowMode::Clock);
        assert_eq!(window_mode(&Screen::new(ScreenContent::NowPlaying)), WindowMode::Background);
    }

    #[test]
    fn texts_are_portuguese_and_tidy() {
        let now = Local.with_ymd_and_hms(2026, 10, 8, 16, 5, 9).unwrap();
        assert_eq!(date_text(&now), "quinta, 8 de outubro");
        assert_eq!(clock_text(&now, true, false), "16:05");
        assert_eq!(clock_text(&now, false, true), "4:05:09");
        assert_eq!(rate(1_234_567.0), "1,2 MB/s");
        assert_eq!(rate(140_000.0), "140 KB/s");
        assert_eq!(fit("abcdefghij", 5), "abcd…");
        assert_eq!(duration_text(3725), "1:02:05");
    }

    #[test]
    fn countdown_runs_out() {
        let mut timer = Timer::default();
        assert_eq!(timer_text(25, &timer).0, "25:00");
        timer.banked = Duration::from_secs(25 * 60 + 3);
        let (shown, progress, done) = timer_text(25, &timer);
        assert_eq!((shown.as_str(), progress, done), ("00:00", Some(1.0), true));
        timer.reset();
        assert!(!timer.running() && timer.elapsed().is_zero());
    }

    #[test]
    fn signature_changes_only_with_what_is_shown() {
        let (usage, timer) = (Usage::default(), Timer::default());
        let clock = Screen::new(ScreenContent::Clock { hour24: true, seconds: false, date: true });
        let a = signature(&clock, &data(&usage, &timer, None));
        let busier = Usage { cpu: 90.0, ..Usage::default() };
        assert_eq!(a, signature(&clock, &data(&busier, &timer, None)), "a clock ignores CPU");
        let stats = Screen::new(ScreenContent::Stats { cpu: true, memory: false, gpu: false, network: false });
        assert_ne!(signature(&stats, &data(&usage, &timer, None)), signature(&stats, &data(&busier, &timer, None)));
    }
}
