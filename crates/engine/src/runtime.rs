//! The device loop: connects to the D200, keeps it awake, follows the app in
//! front, the running apps and Edge's tabs to switch rule layers, runs
//! actions on key presses, applies config edits live and reconnects when the
//! cable comes out. An app drives it with `Command`s and gets a `Status`
//! whenever something it shows changes.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use base64::Engine as _;
use d200::device::D200;
use d200::layout::KeyView;
use d200::protocol::{Incoming, WindowMode, KEY_COUNT};
use hidapi::HidApi;
use log::{info, warn};
use serde::Serialize;
use serde_json::json;
use sysinfo::System;

use crate::actions::{self, Front};
use crate::browser::{self, Bridge};
use crate::config::{Config, Key, Rule, RuleMode, When, Window};
use crate::context::{watch_foreground, ProcessList};
use crate::icons;
use crate::rules::{
    active_rules, host_of, matching_tab, resolve, simulated_context, site_matches, Context, Simulation, Slot, Tabs,
    BROWSER,
};

/// Without traffic the device falls back to its screensaver; 1 s also keeps the clock exact.
const KEEPALIVE: Duration = Duration::from_secs(1);
const CONFIG_POLL: Duration = Duration::from_secs(1);
const PROCESS_POLL: Duration = Duration::from_secs(1);
const RECONNECT: Duration = Duration::from_secs(2);
const IDLE_TICK: Duration = Duration::from_millis(100);
/// The active rules must stay the same this long before the keys change,
/// so Alt+Tab through several windows doesn't make them flicker.
const SETTLE: Duration = Duration::from_millis(200);
const WINDOW_INDEX: usize = KEY_COUNT - 1;

type IconCache = HashMap<(Option<String>, String), Option<Vec<u8>>>;

pub enum Command {
    /// Paused, the device shows the default layout whatever is open.
    Pause(bool),
    /// Sends brightness, label style and every key again.
    Resend,
    /// Replaces the real context until `Simulate(None)`.
    Simulate(Option<Simulation>),
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub config_path: String,
    /// Why the config file was not applied, when it has an error.
    pub config_error: Option<String>,
    pub device: bool,
    pub extension: bool,
    pub paused: bool,
    pub simulation: Option<Simulation>,
    pub window: Window,
    /// Exe name of the app in front (real or simulated).
    pub focused: Option<String>,
    /// Domain of Edge's selected tab when Edge is in front.
    pub focused_site: Option<String>,
    /// Watched apps that are running and sites that have a tab open.
    pub open: Vec<String>,
    pub rules: Vec<RuleStatus>,
    /// Keys 1 to 14, 14 being the status window.
    pub keys: Vec<KeyStatus>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RuleStatus {
    pub name: String,
    /// "app" or "site".
    pub kind: &'static str,
    pub target: String,
    pub mode: RuleMode,
    pub enabled: bool,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KeyStatus {
    pub number: u8,
    pub label: String,
    /// The PNG the device shows, as a data URL.
    pub image: Option<String>,
    pub action: Option<String>,
    /// The rule the key comes from; `None` is the default layout.
    pub rule: Option<String>,
    pub mode: Option<RuleMode>,
    /// Other active rules that also replace this key and lost.
    pub beaten: Vec<String>,
}

pub struct Engine {
    commands: Sender<Command>,
}

impl Engine {
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

/// Starts the engine on its own thread. `on_status` runs on that thread.
pub fn spawn(config_path: PathBuf, on_status: impl Fn(&Status) + Send + 'static) -> Engine {
    let (tx, rx) = channel();
    std::thread::spawn(move || State::new(config_path, rx, Box::new(on_status)).run());
    Engine { commands: tx }
}

/// Runs the engine on this thread, without an app to report to (deckd).
pub fn run(config_path: PathBuf) -> Result<()> {
    let (_commands, rx) = channel();
    State::new(config_path, rx, Box::new(|_| {})).run();
    Ok(())
}

struct State {
    config_path: PathBuf,
    config: Config,
    config_error: Option<String>,
    modified: Option<SystemTime>,
    commands: Receiver<Command>,
    on_status: Box<dyn Fn(&Status) + Send>,
    foreground: Receiver<Option<String>>,
    browser: Bridge,
    tabs: Receiver<Tabs>,
    processes: ProcessList,
    context: Context,
    paused: bool,
    simulation: Option<Simulation>,
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
    device: bool,
    extension: bool,
    last_config: Instant,
    last_processes: Instant,
    status_dirty: bool,
    last_status: Status,
}

impl State {
    fn new(config_path: PathBuf, commands: Receiver<Command>, on_status: Box<dyn Fn(&Status) + Send>) -> Self {
        // A broken config file must not keep the app from starting: run on
        // the defaults and report the error until the file is fixed.
        let (config, config_error) = match Config::load_or_create(&config_path) {
            Ok(config) => (config, None),
            Err(e) => {
                warn!("config com erro, usando o layout padrão: {e:#}");
                (Config::default(), Some(format!("{e:#}")))
            }
        };
        info!("usando {}", config_path.display());
        let (browser, tabs) = browser::start();
        let mut state = Self {
            modified: modified(&config_path),
            config_path,
            config,
            config_error,
            commands,
            on_status,
            foreground: watch_foreground(),
            browser,
            tabs,
            processes: ProcessList::default(),
            context: Context::default(),
            paused: false,
            simulation: None,
            active: Vec::new(),
            pending: None,
            resolved: BTreeMap::new(),
            sent: BTreeMap::new(),
            icons: HashMap::new(),
            sys: System::new(),
            device: false,
            extension: false,
            last_config: Instant::now(),
            last_processes: Instant::now(),
            status_dirty: true,
            last_status: Status::default(),
        };
        state.drain_events();
        state.poll_processes();
        state.active = state.compute_active();
        state.resolved = resolve(&state.config, &state.active);
        state
    }

    fn run(mut self) {
        let mut api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                warn!("o acesso USB não abriu: {e}");
                return;
            }
        };
        let mut waiting_logged = false;
        loop {
            match D200::open(&api) {
                Ok(device) => {
                    info!("D200 conectado");
                    waiting_logged = false;
                    self.device = true;
                    if let Err(e) = self.session(&device) {
                        warn!("conexão com o D200 perdida: {e:#}");
                    }
                    self.device = false;
                    self.status_dirty = true;
                }
                Err(_) if !waiting_logged => {
                    info!("aguardando o D200 (feche o Ulanzi Studio se ele estiver aberto)");
                    waiting_logged = true;
                }
                Err(_) => {}
            }
            // Keep following apps and commands while the device is away.
            let until = Instant::now() + RECONNECT;
            while Instant::now() < until {
                if let Err(e) = self.tick(None) {
                    warn!("{e:#}");
                }
                sleep(IDLE_TICK);
            }
            let _ = api.refresh_devices();
        }
    }

    fn session(&mut self, device: &D200) -> Result<()> {
        self.connect(device)?;
        let mut last_keepalive = Instant::now();
        loop {
            if last_keepalive.elapsed() >= KEEPALIVE {
                self.keepalive(device)?;
                last_keepalive = Instant::now();
            }
            self.tick(Some(device))?;
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

    /// Everything except reading keys and the keep-alive, with or without a device.
    fn tick(&mut self, device: Option<&D200>) -> Result<()> {
        self.handle_commands(device)?;
        if self.last_config.elapsed() >= CONFIG_POLL {
            self.reload_if_changed(device)?;
            self.last_config = Instant::now();
        }
        if self.last_processes.elapsed() >= PROCESS_POLL {
            self.poll_processes();
            self.last_processes = Instant::now();
        }
        self.drain_events();
        let extension = self.browser.is_connected();
        if extension != self.extension {
            self.extension = extension;
            self.status_dirty = true;
        }
        self.settle_rules(device)?;
        self.publish();
        Ok(())
    }

    fn connect(&mut self, device: &D200) -> Result<()> {
        device.set_brightness(self.config.brightness)?;
        device.set_label_style(&label_style(&self.config))?;
        self.drain_events();
        self.poll_processes();
        self.active = self.compute_active();
        self.pending = None;
        // The device may have lost power since the last session: send every key.
        self.sent.clear();
        self.sync_keys(Some(device))?;
        self.log_active();
        self.publish();
        self.keepalive(device)
    }

    fn handle_commands(&mut self, device: Option<&D200>) -> Result<()> {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Pause(paused) => {
                    self.paused = paused;
                    info!("{}", if paused { "regras pausadas" } else { "regras retomadas" });
                    self.apply_rules(device)?;
                }
                Command::Resend => {
                    info!("reenviando tudo para o D200");
                    if let Some(device) = device {
                        device.set_brightness(self.config.brightness)?;
                        device.set_label_style(&label_style(&self.config))?;
                    }
                    self.icons.clear();
                    self.sent.clear();
                    self.sync_keys(device)?;
                }
                Command::Simulate(simulation) => {
                    match &simulation {
                        Some(_) => info!("simulação de contexto ligada"),
                        None => info!("simulação desligada: de volta ao contexto real"),
                    }
                    self.simulation = simulation;
                    self.apply_rules(device)?;
                }
            }
            self.status_dirty = true;
        }
        Ok(())
    }

    /// Latest app in front and latest Edge tabs.
    fn drain_events(&mut self) {
        while let Ok(focused) = self.foreground.try_recv() {
            if focused != self.context.focused {
                self.context.focused = focused;
                self.status_dirty = true;
            }
        }
        while let Ok(tabs) = self.tabs.try_recv() {
            if tabs != self.context.tabs {
                self.context.tabs = tabs;
                self.status_dirty = true;
            }
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
            .filter_map(Rule::process)
            .collect();
        let running: HashSet<String> = if watched.is_empty() {
            HashSet::new()
        } else {
            self.processes.running().into_iter().filter(|p| watched.contains(p)).collect()
        };
        if running != self.context.running {
            self.context.running = running;
            self.status_dirty = true;
        }
    }

    fn effective_context(&self) -> Cow<'_, Context> {
        match &self.simulation {
            Some(simulation) => Cow::Owned(simulated_context(&self.config, simulation)),
            None => Cow::Borrowed(&self.context),
        }
    }

    fn compute_active(&self) -> Vec<usize> {
        if self.paused {
            return Vec::new();
        }
        active_rules(&self.config, &self.effective_context())
    }

    /// Applies the current rules right away, without waiting for them to settle.
    fn apply_rules(&mut self, device: Option<&D200>) -> Result<()> {
        self.active = self.compute_active();
        self.pending = None;
        self.sync_keys(device)?;
        self.log_active();
        Ok(())
    }

    fn settle_rules(&mut self, device: Option<&D200>) -> Result<()> {
        let active = self.compute_active();
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

    fn sync_keys(&mut self, device: Option<&D200>) -> Result<()> {
        self.resolved = resolve(&self.config, &self.active);
        self.status_dirty = true;
        let Some(device) = device else {
            // Nothing on the device to keep in sync: send everything on connect.
            self.sent.clear();
            return Ok(());
        };
        let base = self.config_base();
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
                format!("{} ({})", rule.name, mode_name(rule.mode))
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

    fn reload_if_changed(&mut self, device: Option<&D200>) -> Result<()> {
        let now = modified(&self.config_path);
        if now == self.modified {
            return Ok(());
        }
        self.modified = now;
        let config = match Config::load(&self.config_path) {
            Ok(config) => config,
            Err(e) => {
                warn!("config ignorada, mantendo a anterior: {e:#}");
                self.config_error = Some(format!("{e:#}"));
                self.status_dirty = true;
                return Ok(());
            }
        };
        self.config_error = None;
        let old = std::mem::replace(&mut self.config, config);
        info!("config recarregada");
        // A PNG may have changed on disk under the same name.
        self.icons.clear();
        // Rule indices may point elsewhere now.
        if let Some(simulation) = &mut self.simulation {
            let count = self.config.rules.len();
            simulation.focus = simulation.focus.filter(|&i| i < count);
            simulation.open.retain(|&i| i < count);
        }
        if let Some(device) = device {
            if old.brightness != self.config.brightness {
                device.set_brightness(self.config.brightness)?;
            }
            if old.label != self.config.label {
                // A new label style only shows on keys sent after it, so resend them all.
                device.set_label_style(&label_style(&self.config))?;
                self.sent.clear();
            }
        }
        self.poll_processes();
        self.apply_rules(device)?;
        if old.window != self.config.window {
            if let Some(device) = device {
                self.keepalive(device)?;
            }
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
        let front = rule.filter(|_| slot.key.front).and_then(|r| self.front_for(r));
        let place = match &front {
            Some(Front::App(process)) => format!(" em {process}"),
            Some(Front::Tab { tab, .. }) => format!(" na aba {tab} do Edge"),
            None => String::new(),
        };
        info!("tecla {number} ({origin}): {}{place}", actions::describe(&action));
        actions::run(action, front);
    }

    fn front_for(&self, rule: &Rule) -> Option<Front> {
        if let Some(process) = rule.process() {
            return Some(Front::App(process));
        }
        let Some(tab) = matching_tab(rule, &self.context) else {
            warn!("{}: nenhuma aba aberta do site para receber a ação", rule.name);
            return None;
        };
        // Edge in front on another tab: select that tab again afterwards.
        let edge_in_front = self.context.focused.as_deref() == Some(BROWSER);
        let back_to = self.context.tabs.active.as_ref().filter(|t| edge_in_front && t.id != tab).map(|t| t.id);
        Some(Front::Tab { bridge: self.browser.clone(), tab, back_to })
    }

    fn config_base(&self) -> PathBuf {
        self.config_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }

    fn publish(&mut self) {
        if !self.status_dirty {
            return;
        }
        self.status_dirty = false;
        let status = self.build_status();
        if status != self.last_status {
            (self.on_status)(&status);
            self.last_status = status;
        }
    }

    fn build_status(&mut self) -> Status {
        let ctx = self.effective_context().into_owned();
        let focused_site = ctx
            .tabs
            .active
            .as_ref()
            .filter(|_| ctx.focused.as_deref() == Some(BROWSER))
            .and_then(|t| host_of(&t.url));
        let mut open: Vec<String> = ctx.running.iter().cloned().collect();
        open.sort();
        for site in self.config.rules.iter().filter_map(Rule::site) {
            if !open.contains(&site) && ctx.tabs.open.iter().any(|t| site_matches(&site, &t.url)) {
                open.push(site);
            }
        }
        let rules = self
            .config
            .rules
            .iter()
            .enumerate()
            .map(|(i, rule)| {
                let (kind, target) = match &rule.when {
                    When::Process(process) => ("app", process.clone()),
                    When::Site(_) => ("site", rule.site().unwrap_or_default()),
                };
                RuleStatus {
                    name: rule.name.clone(),
                    kind,
                    target,
                    mode: rule.mode,
                    enabled: rule.enabled,
                    active: self.active.contains(&i),
                }
            })
            .collect();
        let base = self.config_base();
        let keys = (1..=KEY_COUNT as u8)
            .map(|number| {
                let slot = self.resolved.get(&number);
                let contenders: Vec<usize> =
                    self.active.iter().copied().filter(|&i| self.config.rules[i].keys.contains_key(&number)).collect();
                let beaten = contenders
                    .split_last()
                    .map(|(_, losers)| losers.iter().map(|&i| self.config.rules[i].name.clone()).collect())
                    .unwrap_or_default();
                let image = slot
                    .and_then(|s| cached_icon(&mut self.icons, &s.key, &base, number as usize))
                    .map(|png| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png)));
                let rule = slot.and_then(|s| s.rule).map(|i| &self.config.rules[i]);
                KeyStatus {
                    number,
                    label: slot.map(|s| s.key.label.clone()).unwrap_or_default(),
                    image,
                    action: slot.and_then(|s| s.key.action.as_ref()).map(actions::describe),
                    rule: rule.map(|r| r.name.clone()),
                    mode: rule.map(|r| r.mode),
                    beaten,
                }
            })
            .collect();
        Status {
            config_path: self.config_path.display().to_string(),
            config_error: self.config_error.clone(),
            device: self.device,
            extension: self.extension,
            paused: self.paused,
            simulation: self.simulation.clone(),
            window: self.config.window,
            focused: ctx.focused.clone(),
            focused_site,
            open,
            rules,
            keys,
        }
    }
}

fn mode_name(mode: RuleMode) -> &'static str {
    match mode {
        RuleMode::Open => "aberto",
        RuleMode::Focus => "foco",
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
