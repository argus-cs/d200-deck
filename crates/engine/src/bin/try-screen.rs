//! Draws every kind of visor content into PNG files, to look at them.
//! Usage: cargo run -p deck-engine --bin try-screen -- <folder>

use std::path::PathBuf;
use std::time::Duration;

use deck_engine::config::{Screen, ScreenContent};
use deck_engine::icons::{render_key, Look, ICON_COLOR};
use deck_engine::screen::{render, watch_now_playing, Data, NowPlaying, Timer, Usage, UsageSampler};

fn main() -> anyhow::Result<()> {
    let folder = PathBuf::from(std::env::args().nth(1).expect("faltou a pasta de saída"));
    std::fs::create_dir_all(&folder)?;
    // Two samples a moment apart, so CPU, GPU and network have rates.
    let mut sampler = UsageSampler::default();
    sampler.sample();
    std::thread::sleep(Duration::from_millis(500));
    let usage: Usage = sampler.sample();
    let song = NowPlaying::new("Bohemian Rhapsody", "Queen", true);
    let mut timer = Timer::default();
    timer.toggle();
    let screens = [
        ("clock", ScreenContent::Clock { hour24: true, seconds: false, date: true }),
        ("clock-12h-seconds", ScreenContent::Clock { hour24: false, seconds: true, date: true }),
        ("stats", ScreenContent::Stats { cpu: true, memory: true, gpu: true, network: true }),
        ("clock-stats", ScreenContent::ClockStats { hour24: true, seconds: false }),
        ("now-playing", ScreenContent::NowPlaying),
        ("pomodoro", ScreenContent::Timer { minutes: 25 }),
        ("image", ScreenContent::Image { icon: Some("headset".into()) }),
        ("text", ScreenContent::Text { text: "Em reunião\nnão perturbe".into() }),
    ];
    for (name, content) in screens {
        let data = Data { now: chrono::Local::now(), usage: &usage, playing: Some(&song), timer: &timer };
        if let Some(png) = render(&Screen::new(content), &data, &folder)? {
            std::fs::write(folder.join(format!("{name}.png")), png)?;
        }
    }
    println!("CPU {:.0}% · RAM {:.0}% · GPU {:?} · ↓ {:.0} B/s", usage.cpu, usage.memory, usage.gpu, usage.down);

    // A few keys with colors, frame and labels drawn by the app.
    let keys = [
        ("key-plain", Look { icon: Some("mic"), background: "#24262B", icon_color: ICON_COLOR, border: None, label: "Mutar mic", text_color: "#FFFFFF", label_px: 24, dim: false }),
        ("key-colors", Look { icon: Some("micOff"), background: "#3A1616", icon_color: "#FFC2B8", border: Some("#E5533D"), label: "Mutado", text_color: "#FFC2B8", label_px: 24, dim: false }),
        ("key-long", Look { icon: Some("terminal"), background: "#12303A", icon_color: "#BFF0D4", border: None, label: "Bloco de Notas do Windows", text_color: "#FFFFFF", label_px: 24, dim: false }),
        ("key-nolabel", Look { icon: Some("play"), background: "#1F2A44", icon_color: "#F0A63A", border: Some("#F0A63A"), label: "", text_color: "#FFFFFF", label_px: 24, dim: false }),
        ("key-dimmed", Look { icon: Some("music"), background: "#1F3A2A", icon_color: ICON_COLOR, border: None, label: "Spotify", text_color: "#FFFFFF", label_px: 24, dim: true }),
    ];
    for (name, look) in keys {
        std::fs::write(folder.join(format!("{name}.png")), render_key(&look, &folder)?)?;
    }

    // What Windows says is playing right now, cover included.
    match watch_now_playing().recv_timeout(Duration::from_secs(5)) {
        Ok(Some(real)) => {
            let data = Data { now: chrono::Local::now(), usage: &usage, playing: Some(&real), timer: &timer };
            if let Some(png) = render(&Screen::new(ScreenContent::NowPlaying), &data, &folder)? {
                std::fs::write(folder.join("now-playing-real.png"), png)?;
            }
            let cover = real.art.as_ref().map_or(0, |a| a.len());
            println!("tocando agora: {} — {} (capa: {cover} bytes)", real.title, real.artist);
        }
        _ => println!("nada tocando agora"),
    }
    Ok(())
}
