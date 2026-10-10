//! The device loop: connects to the D200, keeps it awake, follows the app in
//! front, the running apps and Edge's tabs to switch rule layers, runs
//! actions on key presses, applies config edits live and reconnects when the
//! cable comes out. An app drives it with `Command`s and gets a `Status`
//! whenever something it shows changes.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime};

use anyhow::Result;
use d200::device::D200;
use d200::layout::KeyView;
use d200::protocol::{Incoming, WindowMode, KEY_COUNT};
use hidapi::HidApi;
use log::{debug, info, warn};
use serde::Serialize;
use serde_json::json;

use crate::actions::{self, Front};
use crate::browser::{self, Bridge};
use crate::config::{Action, Appearance, Config, Folder, Key, Rule, RuleMode, SettingId, When, Window, BACK_KEY};
use crate::context::{watch_foreground, ProcessList};
use crate::icons;
use crate::rules::{
    active_rules, host_of, matching_tab, resolve, resolve_folder, simulated_context, site_matches, Context, Simulation,
    Slot, Tabs, BROWSER,
};
use crate::config::{Screen, ScreenContent, ScreenHold};
use crate::screen::{self, NowPlaying, Timer, Usage, UsageSampler};
use crate::system;

mod keys;
mod status;
mod visor;

use keys::{Live, OpenFolder};
pub use status::{Activity, EdgePage, KeyStatus, RuleStatus, ScreenStatus, Status};
use status::mode_name;
use visor::VisorPress;

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
const USAGE_POLL: Duration = Duration::from_secs(2);
/// Holding the visor this long runs its hold instead of a tap.
const HOLD: Duration = Duration::from_millis(700);
/// A second tap on the timer this soon after the first resets it.
const DOUBLE_TAP: Duration = Duration::from_millis(450);
const SETTING_POLL: Duration = Duration::from_secs(1);
/// After a key changes a Windows setting, read it this often for a while,
/// so the key follows quickly (a radio takes about a second to switch).
const SETTING_FAST_POLL: Duration = Duration::from_millis(200);
const SETTING_FAST_FOR: Duration = Duration::from_secs(3);
/// A folder nobody touches goes back to the layout after this long.
const FOLDER_TIMEOUT: Duration = Duration::from_secs(30);

/// Rendered key images by `Look::cache_key`.
type IconCache = HashMap<String, Option<Vec<u8>>>;

pub enum Command {
    /// Paused, the device shows the default layout whatever is open.
    Pause(bool),
    /// Sends brightness, label style and every key again.
    Resend,
    /// Replaces the real context until `Simulate(None)`.
    Simulate(Option<Simulation>),
    /// Reads config.json now instead of at the next poll.
    Reload,
}

type ActivitySink = Arc<dyn Fn(&Activity) + Send + Sync>;

pub struct Engine {
    commands: Sender<Command>,
    browser: Bridge,
}

impl Engine {
    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    /// The link to the Edge extension, for asking it things (site icons).
    pub fn browser(&self) -> Bridge {
        self.browser.clone()
    }
}

/// Starts the engine on its own thread. `on_status` and `on_screen` run on
/// that thread: the whole status when anything in it changes, or only the
/// visor when nothing else did (every second or two, with a clock or the PC's
/// usage). `on_activity` also runs on action threads, to report failures.
pub fn spawn(
    config_path: PathBuf,
    on_status: impl Fn(&Status) + Send + 'static,
    on_screen: impl Fn(&ScreenStatus) + Send + 'static,
    on_activity: impl Fn(&Activity) + Send + Sync + 'static,
) -> Engine {
    let (tx, rx) = channel();
    let (browser, tabs) = browser::start();
    let engine = Engine { commands: tx, browser: browser.clone() };
    std::thread::spawn(move || State::new(config_path, rx, Box::new(on_status), Box::new(on_screen), Arc::new(on_activity), browser, tabs).run());
    engine
}

/// Runs the engine on this thread, without an app to report to (deckd).
pub fn run(config_path: PathBuf) -> Result<()> {
    let (_commands, rx) = channel();
    let (browser, tabs) = browser::start();
    State::new(config_path, rx, Box::new(|_| {}), Box::new(|_| {}), Arc::new(|_| {}), browser, tabs).run();
    Ok(())
}

struct State {
    config_path: PathBuf,
    config: Config,
    config_error: Option<String>,
    config_revision: u64,
    modified: Option<SystemTime>,
    commands: Receiver<Command>,
    on_status: Box<dyn Fn(&Status) + Send>,
    on_screen: Box<dyn Fn(&ScreenStatus) + Send>,
    on_activity: ActivitySink,
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
    live: Live,
    folder: Option<OpenFolder>,
    icons: IconCache,
    sampler: UsageSampler,
    usage: Usage,
    last_usage: Instant,
    /// Started the first time a visor shows what is playing.
    playing_rx: Option<Receiver<Option<NowPlaying>>>,
    playing: Option<NowPlaying>,
    timer: Timer,
    /// What the visor shows, to draw it again only when it changes.
    screen_signature: String,
    screen_png: Option<Vec<u8>>,
    visor_press: VisorPress,
    /// How far each visor's hold cycle went, by rule name (None: the default visor).
    screen_steps: HashMap<Option<String>, usize>,
    last_timer_tap: Option<Instant>,
    device: bool,
    extension: bool,
    last_config: Instant,
    last_processes: Instant,
    last_settings: Instant,
    /// Read settings often until then: a key just changed one.
    settings_fast_until: Option<Instant>,
    status_dirty: bool,
    /// Only the visor changed since the last status.
    screen_dirty: bool,
    last_status: Status,
}

impl State {
    fn new(
        config_path: PathBuf,
        commands: Receiver<Command>,
        on_status: Box<dyn Fn(&Status) + Send>,
        on_screen: Box<dyn Fn(&ScreenStatus) + Send>,
        on_activity: ActivitySink,
        browser: Bridge,
        tabs: Receiver<Tabs>,
    ) -> Self {
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
        let mut state = Self {
            modified: modified(&config_path),
            config_path,
            config,
            config_error,
            config_revision: 0,
            commands,
            on_status,
            on_screen,
            on_activity,
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
            live: Live::default(),
            folder: None,
            icons: HashMap::new(),
            sampler: UsageSampler::default(),
            usage: Usage::default(),
            last_usage: Instant::now(),
            playing_rx: None,
            playing: None,
            timer: Timer::default(),
            screen_signature: String::new(),
            screen_png: None,
            visor_press: VisorPress::Up,
            screen_steps: HashMap::new(),
            last_timer_tap: None,
            device: false,
            extension: false,
            last_config: Instant::now(),
            last_processes: Instant::now(),
            last_settings: Instant::now(),
            settings_fast_until: None,
            status_dirty: true,
            screen_dirty: false,
            last_status: Status::default(),
        };
        state.drain_events();
        state.poll_processes();
        state.active = state.compute_active();
        state.resolved = state.layout();
        // Now that the keys are known, see which of their apps run and what their settings are.
        state.poll_processes();
        state.poll_settings();
        state.redraw_screen();
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
            match device.read(100)? {
                Some((Incoming::Button { index, pressed, .. }, _)) if index as usize == WINDOW_INDEX => {
                    if pressed {
                        // A tap cycles the window mode on the device; put ours back right away.
                        self.keepalive(device)?;
                        last_keepalive = Instant::now();
                        let holds = self.current_screen().0.holds();
                        self.visor_press = VisorPress::Down { since: Instant::now(), holds };
                    } else if let VisorPress::Down { .. } = std::mem::replace(&mut self.visor_press, VisorPress::Up) {
                        self.tap_screen(device)?;
                    }
                }
                Some((Incoming::Button { index, pressed: true, .. }, _)) => self.press(index as usize, device)?,
                _ => {}
            }
            // The hold runs while the finger is still down, so it is felt right away.
            if let VisorPress::Down { since, holds: true } = self.visor_press {
                if since.elapsed() >= HOLD {
                    self.visor_press = VisorPress::Held;
                    self.hold_screen(device)?;
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
            if self.poll_processes() {
                // An "open app" key's app started or quit: brighten or dim it.
                self.sync_keys(device)?;
            }
            self.last_processes = Instant::now();
        }
        let fast = self.settings_fast_until.is_some_and(|until| Instant::now() < until);
        if self.last_settings.elapsed() >= if fast { SETTING_FAST_POLL } else { SETTING_POLL } {
            if self.poll_settings() {
                // Bluetooth went off, another audio output was chosen…: show it.
                self.sync_keys(device)?;
            }
            self.last_settings = Instant::now();
        }
        if self.folder.as_ref().is_some_and(|f| f.touched.elapsed() >= FOLDER_TIMEOUT) {
            info!("pasta fechada: ninguém tocou nela");
            self.folder = None;
            self.sync_keys(device)?;
        }
        self.drain_events();
        let extension = self.browser.is_connected();
        if extension != self.extension {
            self.extension = extension;
            self.status_dirty = true;
        }
        self.settle_rules(device)?;
        self.refresh_screen(device)?;
        self.publish();
        Ok(())
    }

    fn connect(&mut self, device: &D200) -> Result<()> {
        device.set_brightness(self.config.brightness)?;
        device.set_label_style(&label_style(&self.config))?;
        self.drain_events();
        self.poll_processes();
        self.active = self.compute_active();
        // Plugged in again: back to the layout, not to a folder from before.
        self.folder = None;
        // Nor to a finger on the visor when the cable went: its hold would run
        // at once, with nobody touching it.
        self.visor_press = VisorPress::Up;
        self.resolved = self.layout();
        self.poll_processes();
        self.poll_settings();
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
                Command::Reload => {
                    self.modified = None;
                    self.reload_if_changed(device)?;
                    self.last_config = Instant::now();
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

    /// Only the processes that "open" rules and "open app" keys watch, so
    /// other apps starting and stopping never touch the keys. True when an
    /// app key's app started or quit.
    fn poll_processes(&mut self) -> bool {
        let rule_apps: HashSet<String> = self
            .config
            .rules
            .iter()
            .filter(|r| r.enabled && r.mode == RuleMode::Open)
            .filter_map(Rule::process)
            .collect();
        // Both faces of a two-state key, so a press never shows a stale state.
        let key_apps: HashSet<String> = self
            .keys_in_view()
            .flat_map(|key| [key.action.as_ref(), key.toggle.as_ref().and_then(|t| t.action.as_ref())])
            .flatten()
            .filter_map(Action::watched_process)
            .collect();
        let running = if rule_apps.is_empty() && key_apps.is_empty() { HashSet::new() } else { self.processes.running() };
        let rules_running: HashSet<String> = running.iter().filter(|p| rule_apps.contains(*p)).cloned().collect();
        let apps_running: HashSet<String> = running.into_iter().filter(|p| key_apps.contains(p)).collect();
        if rules_running != self.context.running {
            self.context.running = rules_running;
            self.status_dirty = true;
        }
        if apps_running == self.live.apps_running {
            return false;
        }
        self.live.apps_running = apps_running;
        true
    }

    /// Reads the Windows settings that keys show. True when one changed.
    fn poll_settings(&mut self) -> bool {
        let watched: HashSet<SettingId> = self.keys_in_view().filter_map(|key| key.action.as_ref()?.watched_setting()).collect();
        let settings: HashMap<SettingId, Option<bool>> = watched
            .into_iter()
            .map(|id| {
                let state = system::state(id.0, id.1.as_deref());
                (id, state)
            })
            .collect();
        if settings == self.live.settings {
            return false;
        }
        self.live.settings = settings;
        true
    }

    /// Reads the settings of keys that just came up (a rule that started
    /// applying), so they don't show as missing until the next poll.
    fn read_new_settings(&mut self) {
        let new: Vec<SettingId> = self
            .resolved
            .values()
            .filter_map(|slot| slot.key.action.as_ref()?.watched_setting())
            .filter(|id| !self.live.settings.contains_key(id))
            .collect();
        for id in new {
            let state = system::state(id.0, id.1.as_deref());
            self.live.settings.insert(id, state);
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
        let (screen, _) = self.current_screen();
        let mode = screen::window_mode(&screen);
        let (cpu, mem, gpu) = if mode == WindowMode::Stats {
            let gpu = self.usage.gpu.map_or(0, |gpu| gpu.round() as u8);
            (self.usage.cpu.round() as u8, self.usage.memory.round() as u8, gpu)
        } else {
            (0, 0, 0)
        };
        device.set_small_window(mode, cpu, mem, gpu)
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
        self.config_revision += 1;
        let old = std::mem::replace(&mut self.config, config);
        info!("config recarregada");
        // A PNG may have changed on disk under the same name.
        self.icons.clear();
        self.screen_signature.clear();
        // Rule indices may point elsewhere now.
        if let Some(simulation) = &mut self.simulation {
            let count = self.config.rules.len();
            simulation.focus = simulation.focus.filter(|&i| i < count);
            simulation.open.retain(|&i| i < count);
        }
        self.poll_processes();
        self.poll_settings();
        // Before writing to the device: a write that fails would leave the old
        // config's rule indices behind, and the next tick would index past the end.
        self.active = self.compute_active();
        self.pending = None;
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
        self.sync_keys(device)?;
        self.log_active();
        if let Some(device) = device {
            // The visor may have changed between what the device and the app draw.
            self.keepalive(device)?;
        }
        Ok(())
    }

    fn config_base(&self) -> PathBuf {
        Config::dir_of(&self.config_path)
    }
}

/// The device's own labels stay off: the app draws them into the images.
fn label_style(config: &Config) -> serde_json::Value {
    let color = u32::from_str_radix(&config.label.color, 16).unwrap_or(0xFFFFFF);
    json!({
        "Align": config.label.align,
        "Color": color,
        "FontName": "Roboto",
        "ShowTitle": false,
        "Size": config.label.size,
        "Weight": 80
    })
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

