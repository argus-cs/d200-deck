//! The device loop: connects to the D200, keeps it awake, follows the app in
//! front and the running apps to switch rule layers, runs actions on key
//! presses, applies config edits live and reconnects when the cable comes out.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
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
use crate::config::{Config, Key, RuleMode, Window};
use crate::context::{watch_foreground, ProcessList};
use crate::icons;
use crate::rules::{active_rules, resolve, Context, Slot};

/// Without traffic the device falls back to its screensaver; 1 s also keeps the clock exact.
const KEEPALIVE: Duration = Duration::from_secs(1);
const CONFIG_POLL: Duration = Duration::from_secs(1);
const PROCESS_POLL: Duration = Duration::from_secs(1);
const RECONNECT: Duration = Duration::from_secs(2);
/// The active rules must stay the same this long before the keys change,
/// so Alt+Tab through several windows doesn't make them flicker.
const SETTLE: Duration = Duration::from_millis(200);
const WINDOW_INDEX: usize = KEY_COUNT - 1;

type IconCache = HashMap<(Option<String>, String), Option<Vec<u8>>>;

pub fn run(config_path: PathBuf) -> Result<()> {
    let config = Config::load_or_create(&config_path)?;
    info!("usando {}", config_path.display());
    let mut state = State {
        modified: modified(&config_path),
        config_path,
        config,
        foreground: watch_foreground(),
        processes: ProcessList::default(),
        context: Context::default(),
        active: Vec::new(),
        pending: None,
        resolved: BTreeMap::new(),
        sent: BTreeMap::new(),
        icons: HashMap::new(),
        sys: System::new(),
    };
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
    foreground: Receiver<Option<String>>,
    processes: ProcessList,
    context: Context,
    /// The rules on the device now, in layering order.
    active: Vec<usize>,
    /// A different set of active rules waiting to settle.
    pending: Option<(Vec<usize>, Instant)>,
    /// What each key does now.
    resolved: BTreeMap<u8, Slot>,
    /// What the device is showing, to send only the keys that change.
    sent: BTreeMap<usize, KeyView>,
    icons: IconCache,
    sys: System,
}

impl State {
    fn session(&mut self, device: &D200) -> Result<()> {
        self.connect(device)?;
        let mut last_keepalive = Instant::now();
        let mut last_config = Instant::now();
        let mut last_processes = Instant::now();
        loop {
            if last_keepalive.elapsed() >= KEEPALIVE {
                self.keepalive(device)?;
                last_keepalive = Instant::now();
            }
            if last_config.elapsed() >= CONFIG_POLL {
                self.reload_if_changed(device)?;
                last_config = Instant::now();
            }
            if last_processes.elapsed() >= PROCESS_POLL {
                self.poll_processes();
                last_processes = Instant::now();
            }
            self.drain_foreground();
            self.settle_rules(device)?;
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

    fn connect(&mut self, device: &D200) -> Result<()> {
        device.set_brightness(self.config.brightness)?;
        device.set_label_style(&label_style(&self.config))?;
        self.drain_foreground();
        self.poll_processes();
        self.active = active_rules(&self.config, &self.context);
        self.pending = None;
        // The device may have lost power since the last session: send every key.
        self.sent.clear();
        self.sync_keys(device)?;
        self.log_active();
        self.keepalive(device)
    }

    fn drain_foreground(&mut self) {
        while let Ok(focused) = self.foreground.try_recv() {
            self.context.focused = focused;
        }
    }

    /// Only the processes that "open" rules watch, so other apps starting and
    /// stopping never touch the keys.
    fn poll_processes(&mut self) {
        let watched: HashSet<String> = self
            .config
            .rules
            .iter()
            .filter(|r| r.enabled && r.mode == RuleMode::Open)
            .map(|r| r.process())
            .collect();
        self.context.running = if watched.is_empty() {
            HashSet::new()
        } else {
            self.processes.running().into_iter().filter(|p| watched.contains(p)).collect()
        };
    }

    fn settle_rules(&mut self, device: &D200) -> Result<()> {
        let active = active_rules(&self.config, &self.context);
        if active == self.active {
            self.pending = None;
            return Ok(());
        }
        let waiting = matches!(&self.pending, Some((p, _)) if *p == active);
        let settled = matches!(&self.pending, Some((p, since)) if *p == active && since.elapsed() >= SETTLE);
        if settled {
            self.pending = None;
            self.active = active;
            self.sync_keys(device)?;
            self.log_active();
        } else if !waiting {
            self.pending = Some((active, Instant::now()));
        }
        Ok(())
    }

    fn sync_keys(&mut self, device: &D200) -> Result<()> {
        self.resolved = resolve(&self.config, &self.active);
        let base = self.config_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let views = build_views(&self.resolved, &base, &mut self.icons);
        let changed = changed_keys(&self.sent, &views);
        if self.sent.is_empty() {
            let size = device.set_layout(&views, false)?;
            info!("layout enviado ({size} bytes)");
        } else if !changed.is_empty() {
            let size = device.set_layout(&changed, true)?;
            let numbers: Vec<String> = changed.keys().map(|i| (i + 1).to_string()).collect();
            let (noun, verb) = if numbers.len() == 1 { ("tecla", "atualizada") } else { ("teclas", "atualizadas") };
            info!("{noun} {} {verb} ({size} bytes)", numbers.join(", "));
        }
        self.sent = views;
        Ok(())
    }

    fn log_active(&self) {
        if self.active.is_empty() {
            info!("nenhuma regra valendo: layout padrão");
            return;
        }
        let names: Vec<String> = self
            .active
            .iter()
            .map(|&i| {
                let rule = &self.config.rules[i];
                let mode = if rule.mode == RuleMode::Open { "aberto" } else { "foco" };
                format!("{} ({mode})", rule.name)
            })
            .collect();
        info!("regras valendo: {}", names.join(", "));
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
        info!("config recarregada");
        // A PNG may have changed on disk under the same name.
        self.icons.clear();
        if old.brightness != self.config.brightness {
            device.set_brightness(self.config.brightness)?;
        }
        if old.label != self.config.label {
            // A new label style only shows on keys sent after it, so resend them all.
            device.set_label_style(&label_style(&self.config))?;
            self.sent.clear();
        }
        self.poll_processes();
        self.active = active_rules(&self.config, &self.context);
        self.pending = None;
        self.sync_keys(device)?;
        self.log_active();
        if old.window != self.config.window {
            self.keepalive(device)?;
        }
        Ok(())
    }

    fn press(&self, index: usize) {
        let number = index as u8 + 1;
        let Some(slot) = self.resolved.get(&number) else {
            info!("tecla {number}: vazia");
            return;
        };
        let Some(action) = slot.key.action.clone() else {
            info!("tecla {number}: sem ação");
            return;
        };
        let rule = slot.rule.map(|i| &self.config.rules[i]);
        let origin = rule.map_or_else(|| "padrão".to_string(), |r| r.name.clone());
        let front = rule.filter(|_| slot.key.front).map(|r| r.process());
        let to_front = front.as_ref().map(|p| format!(" em {p}")).unwrap_or_default();
        info!("tecla {number} ({origin}): {}{to_front}", actions::describe(&action));
        actions::run(action, front);
    }
}

fn build_views(resolved: &BTreeMap<u8, Slot>, base: &Path, cache: &mut IconCache) -> BTreeMap<usize, KeyView> {
    (0..KEY_COUNT)
        .map(|index| {
            let number = index + 1;
            let view = match resolved.get(&(number as u8)) {
                None => KeyView::default(),
                Some(slot) => KeyView { text: slot.key.label.clone(), png: cached_icon(cache, &slot.key, base, number) },
            };
            (index, view)
        })
        .collect()
}

fn cached_icon(cache: &mut IconCache, key: &Key, base: &Path, number: usize) -> Option<Vec<u8>> {
    cache
        .entry((key.icon.clone(), key.color.clone()))
        .or_insert_with(|| icons::render(key, base).map_err(|e| warn!("tecla {number}: ícone ignorado: {e:#}")).ok())
        .clone()
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
        let config = Config::default();
        let mut cache = IconCache::new();
        let views = build_views(&resolve(&config, &[]), Path::new("."), &mut cache);
        assert_eq!(views.len(), KEY_COUNT);
        assert!(views[&0].png.is_some());
        assert_eq!(views[&12], KeyView::default());
        // Keys sharing an icon and color share one rendering.
        assert!(cache.len() <= config.keys.len());
    }
}
