//! The device loop: connects to the D200, pushes the layout, keeps it awake,
//! runs actions on key presses, applies config edits live and reconnects
//! when the cable comes out.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use d200::device::D200;
use d200::layout::KeyView;
use d200::protocol::{Incoming, WindowMode, KEY_COUNT};
use hidapi::HidApi;
use log::{info, warn};
use serde_json::json;
use sysinfo::System;

use crate::actions;
use crate::config::{Config, Window};
use crate::icons;

/// Without traffic the device falls back to its screensaver; 1 s also keeps the clock exact.
const KEEPALIVE: Duration = Duration::from_secs(1);
const CONFIG_POLL: Duration = Duration::from_secs(1);
const RECONNECT: Duration = Duration::from_secs(2);
const WINDOW_INDEX: usize = KEY_COUNT - 1;

pub fn run(config_path: PathBuf) -> Result<()> {
    let config = Config::load_or_create(&config_path)?;
    info!("usando {}", config_path.display());
    let mut state = State { modified: modified(&config_path), config_path, config, sent: BTreeMap::new(), sys: System::new() };
    let mut api = HidApi::new()?;
    let mut waiting_logged = false;
    loop {
        match D200::open(&api) {
            Ok(device) => {
                info!("D200 conectado");
                waiting_logged = false;
                if let Err(e) = state.session(&device) {
                    warn!("conexão com o D200 perdida: {e:#}");
                }
            }
            Err(_) if !waiting_logged => {
                info!("aguardando o D200 (feche o Ulanzi Studio se ele estiver aberto)");
                waiting_logged = true;
            }
            Err(_) => {}
        }
        sleep(RECONNECT);
        api.refresh_devices()?;
    }
}

struct State {
    config_path: PathBuf,
    config: Config,
    modified: Option<SystemTime>,
    /// What the device is showing, to send only the keys that change.
    sent: BTreeMap<usize, KeyView>,
    sys: System,
}

impl State {
    fn session(&mut self, device: &D200) -> Result<()> {
        self.push_everything(device)?;
        let mut last_keepalive = Instant::now();
        let mut last_poll = Instant::now();
        loop {
            if last_keepalive.elapsed() >= KEEPALIVE {
                self.keepalive(device)?;
                last_keepalive = Instant::now();
            }
            if last_poll.elapsed() >= CONFIG_POLL {
                self.reload_if_changed(device)?;
                last_poll = Instant::now();
            }
            if let Some((Incoming::Button { index, pressed: true, .. }, _)) = device.read(100)? {
                self.press(index as usize);
                if index as usize == WINDOW_INDEX {
                    // A tap cycles the window mode on the device; put ours back right away.
                    self.keepalive(device)?;
                    last_keepalive = Instant::now();
                }
            }
        }
    }

    fn push_everything(&mut self, device: &D200) -> Result<()> {
        device.set_brightness(self.config.brightness)?;
        device.set_label_style(&label_style(&self.config))?;
        let views = build_views(&self.config, &self.config_path);
        let size = device.set_layout(&views, false)?;
        info!("layout enviado ({size} bytes)");
        self.sent = views;
        self.keepalive(device)
    }

    fn keepalive(&mut self, device: &D200) -> Result<()> {
        let (mode, cpu, mem) = match self.config.window {
            Window::Clock => (WindowMode::Clock, 0, 0),
            Window::Image => (WindowMode::Background, 0, 0),
            Window::Stats => {
                self.sys.refresh_cpu_usage();
                self.sys.refresh_memory();
                let mem = self.sys.used_memory() * 100 / self.sys.total_memory().max(1);
                (WindowMode::Stats, self.sys.global_cpu_usage().round() as u8, mem as u8)
            }
        };
        device.set_small_window(mode, cpu, mem, 0)
    }

    fn reload_if_changed(&mut self, device: &D200) -> Result<()> {
        let now = modified(&self.config_path);
        if now == self.modified {
            return Ok(());
        }
        self.modified = now;
        let config = match Config::load(&self.config_path) {
            Ok(config) => config,
            Err(e) => {
                warn!("config ignorada, mantendo a anterior: {e:#}");
                return Ok(());
            }
        };
        let old = std::mem::replace(&mut self.config, config);
        if old.brightness != self.config.brightness {
            device.set_brightness(self.config.brightness)?;
        }
        if old.label != self.config.label {
            // A new label style only shows on keys sent after it, so resend them all.
            device.set_label_style(&label_style(&self.config))?;
            self.sent.clear();
        }
        let views = build_views(&self.config, &self.config_path);
        let changed = changed_keys(&self.sent, &views);
        if !changed.is_empty() {
            let size = if self.sent.is_empty() { device.set_layout(&views, false)? } else { device.set_layout(&changed, true)? };
            let numbers: Vec<String> = changed.keys().map(|i| (i + 1).to_string()).collect();
            info!("config recarregada: teclas {} atualizadas ({size} bytes)", numbers.join(", "));
        } else {
            info!("config recarregada");
        }
        self.sent = views;
        if old.window != self.config.window {
            self.keepalive(device)?;
        }
        Ok(())
    }

    fn press(&self, index: usize) {
        let number = index + 1;
        match self.config.keys.get(&(number as u8)).and_then(|k| k.action.clone()) {
            Some(action) => {
                info!("tecla {number}: {}", actions::describe(&action));
                actions::run(action);
            }
            None => info!("tecla {number}: sem ação"),
        }
    }
}

fn build_views(config: &Config, config_path: &Path) -> BTreeMap<usize, KeyView> {
    let base = config_path.parent().unwrap_or(Path::new("."));
    (0..KEY_COUNT)
        .map(|index| {
            let view = match config.keys.get(&(index as u8 + 1)) {
                None => KeyView::default(),
                Some(key) => {
                    let png = icons::render(key, base)
                        .map_err(|e| warn!("tecla {}: ícone ignorado: {e:#}", index + 1))
                        .ok();
                    KeyView { text: key.label.clone(), png }
                }
            };
            (index, view)
        })
        .collect()
}

fn changed_keys(old: &BTreeMap<usize, KeyView>, new: &BTreeMap<usize, KeyView>) -> BTreeMap<usize, KeyView> {
    new.iter().filter(|(i, view)| old.get(i) != Some(view)).map(|(i, view)| (*i, view.clone())).collect()
}

fn label_style(config: &Config) -> serde_json::Value {
    let color = u32::from_str_radix(&config.label.color, 16).unwrap_or(0xFFFFFF);
    json!({
        "Align": config.label.align,
        "Color": color,
        "FontName": "Roboto",
        "ShowTitle": config.label.show,
        "Size": config.label.size,
        "Weight": 80
    })
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_keys_are_resent() {
        let view = |t: &str| KeyView { text: t.into(), png: None };
        let old = BTreeMap::from([(0, view("a")), (1, view("b")), (2, view("c"))]);
        let new = BTreeMap::from([(0, view("a")), (1, view("B")), (2, view("c"))]);
        assert_eq!(changed_keys(&old, &new), BTreeMap::from([(1, view("B"))]));
    }

    #[test]
    fn default_config_renders_every_key() {
        let views = build_views(&Config::default(), Path::new("config.json"));
        assert_eq!(views.len(), KEY_COUNT);
        assert!(views[&0].png.is_some());
        assert_eq!(views[&12], KeyView::default());
    }
}
