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
use log::{info, warn};
use serde::Serialize;
use serde_json::json;

use crate::actions::{self, Front};
use crate::browser::{self, Bridge};
use crate::config::{Action, Config, Key, LabelStyle, Rule, RuleMode, When, Window};
use crate::context::{watch_foreground, ProcessList};
use crate::icons;
use crate::rules::{
    active_rules, host_of, matching_tab, resolve, simulated_context, site_matches, Context, Simulation, Slot, Tabs,
    BROWSER,
};
use crate::config::{Screen, ScreenContent};
use crate::screen::{self as visor, NowPlaying, Timer, Usage, UsageSampler};

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
/// Holding the visor this long resets the timer instead of pausing it.
const HOLD: Duration = Duration::from_millis(700);

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

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Status {
    pub config_path: String,
    /// Why the config file was not applied, when it has an error.
    pub config_error: Option<String>,
    /// Goes up each time config.json is applied, so editors know to reload it.
    pub config_revision: u64,
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
    /// Domains of every tab open in Edge (real, never simulated).
    pub edge_tabs: Vec<String>,
    /// One open page per domain, to pick a site icon from.
    pub edge_pages: Vec<EdgePage>,
    pub rules: Vec<RuleStatus>,
    /// Keys 1 to 14, 14 being the status window.
    pub keys: Vec<KeyStatus>,
    pub screen: ScreenStatus,
}

/// The visor as it is now.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ScreenStatus {
    /// What the app drew, as a data URL; `None` when the device draws itself.
    pub image: Option<String>,
    /// The content type, as in config.json ("clock", "now_playing"…).
    pub content: String,
    /// The rule whose visor shows; `None` is the default.
    pub rule: Option<String>,
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
pub struct EdgePage {
    pub host: String,
    pub url: String,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct KeyStatus {
    pub number: u8,
    /// Two-state keys: true while the second face shows.
    pub toggled: bool,
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

/// A key press on the device, and later its failure if the action fails.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Activity {
    pub number: u8,
    pub label: String,
    /// The rule the key came from; `None` is the default layout.
    pub rule: Option<String>,
    pub action: Option<String>,
    pub error: Option<String>,
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

/// Starts the engine on its own thread. `on_status` runs on that thread;
/// `on_activity` also runs on action threads, to report failures.
pub fn spawn(
    config_path: PathBuf,
    on_status: impl Fn(&Status) + Send + 'static,
    on_activity: impl Fn(&Activity) + Send + Sync + 'static,
) -> Engine {
    let (tx, rx) = channel();
    let (browser, tabs) = browser::start();
    let engine = Engine { commands: tx, browser: browser.clone() };
    std::thread::spawn(move || State::new(config_path, rx, Box::new(on_status), Arc::new(on_activity), browser, tabs).run());
    engine
}

/// Runs the engine on this thread, without an app to report to (deckd).
pub fn run(config_path: PathBuf) -> Result<()> {
    let (_commands, rx) = channel();
    let (browser, tabs) = browser::start();
    State::new(config_path, rx, Box::new(|_| {}), Arc::new(|_| {}), browser, tabs).run();
    Ok(())
}

/// Which two-state keys show their second face: (rule name, key number),
/// `None` being the default layout. By name so reordering rules keeps it.
type ToggleId = (Option<String>, u8);

/// What a key looks like and does right now.
struct Face<'a> {
    label: &'a str,
    icon: Option<&'a str>,
    color: &'a str,
    icon_color: Option<&'a str>,
    text_color: Option<&'a str>,
    border: Option<&'a str>,
    action: Option<&'a Action>,
}

fn face(key: &Key, toggled: bool) -> Face<'_> {
    match (&key.toggle, toggled) {
        (Some(second), true) => Face {
            label: &second.label,
            icon: second.icon.as_deref(),
            color: &second.color,
            icon_color: second.icon_color.as_deref(),
            text_color: second.text_color.as_deref(),
            border: second.border.as_deref(),
            action: second.action.as_ref().or(key.action.as_ref()),
        },
        _ => Face {
            label: &key.label,
            icon: key.icon.as_deref(),
            color: &key.color,
            icon_color: key.icon_color.as_deref(),
            text_color: key.text_color.as_deref(),
            border: key.border.as_deref(),
            action: key.action.as_ref(),
        },
    }
}

/// The image a face makes, with the label style's defaults filled in. An
/// "open app" key dims while its app is not among `running`.
fn look<'a>(face: &Face<'a>, style: &'a LabelStyle, default_text: &'a str, running: &HashSet<String>) -> icons::Look<'a> {
    let dim = face.action.and_then(Action::watched_process).is_some_and(|process| !running.contains(&process));
    icons::Look {
        dim,
        icon: face.icon,
        background: face.color,
        icon_color: face.icon_color.unwrap_or(icons::ICON_COLOR),
        border: face.border,
        label: if style.show { face.label } else { "" },
        text_color: face.text_color.unwrap_or(default_text),
        // The device's label sizes (about 6 to 24) read best at 2.4 px each.
        label_px: (u32::from(style.size) * 12 / 5).clamp(12, 48),
    }
}

fn default_text_color(style: &LabelStyle) -> String {
    format!("#{}", style.color)
}

fn toggle_id(config: &Config, slot: &Slot, number: u8) -> ToggleId {
    (slot.rule.map(|i| config.rules[i].name.clone()), number)
}

fn is_toggled(config: &Config, toggled: &HashSet<ToggleId>, slot: &Slot, number: u8) -> bool {
    slot.key.toggle.is_some() && toggled.contains(&toggle_id(config, slot, number))
}

struct State {
    config_path: PathBuf,
    config: Config,
    config_error: Option<String>,
    config_revision: u64,
    modified: Option<SystemTime>,
    commands: Receiver<Command>,
    on_status: Box<dyn Fn(&Status) + Send>,
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
    /// Two-state keys showing their second face.
    toggled: HashSet<ToggleId>,
    /// Running apps that "open app" keys start (lowercase exe names).
    apps_running: HashSet<String>,
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
    screen_pressed_at: Option<Instant>,
    device: bool,
    extension: bool,
    last_config: Instant,
    last_processes: Instant,
    status_dirty: bool,
    last_status: Status,
}

impl State {
    fn new(
        config_path: PathBuf,
        commands: Receiver<Command>,
        on_status: Box<dyn Fn(&Status) + Send>,
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
            toggled: HashSet::new(),
            apps_running: HashSet::new(),
            icons: HashMap::new(),
            sampler: UsageSampler::default(),
            usage: Usage::default(),
            last_usage: Instant::now(),
            playing_rx: None,
            playing: None,
            timer: Timer::default(),
            screen_signature: String::new(),
            screen_png: None,
            screen_pressed_at: None,
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
        // Now that the keys are known, see which of their apps run.
        state.poll_processes();
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
                        self.screen_pressed_at = Some(Instant::now());
                    } else if let Some(at) = self.screen_pressed_at.take() {
                        self.tap_screen(at.elapsed(), device)?;
                    }
                }
                Some((Incoming::Button { index, pressed: true, .. }, _)) => self.press(index as usize, device)?,
                _ => {}
            }
        }
    }

    /// The visor showing now, and the rule it comes from (the last active
    /// rule with a visor of its own wins, like keys).
    fn current_screen(&self) -> (Screen, Option<usize>) {
        self.active
            .iter()
            .rev()
            .find_map(|&i| self.config.rules[i].screen.clone().map(|s| (s, Some(i))))
            .unwrap_or_else(|| (self.config.default_screen(), None))
    }

    /// Draws the visor again if what it shows changed; true when it did.
    fn redraw_screen(&mut self) -> bool {
        let (screen, _) = self.current_screen();
        if visor::uses_now_playing(&screen) && self.playing_rx.is_none() {
            self.playing_rx = Some(visor::watch_now_playing());
        }
        let data = visor::Data { now: chrono::Local::now(), usage: &self.usage, playing: self.playing.as_ref(), timer: &self.timer };
        let signature = visor::signature(&screen, &data);
        if signature == self.screen_signature {
            return false;
        }
        self.screen_png = visor::render(&screen, &data, &self.config_base())
            .map_err(|e| warn!("visor não desenhado: {e:#}"))
            .unwrap_or(None);
        self.screen_signature = signature;
        self.status_dirty = true;
        true
    }

    /// Fresh numbers and songs for the visor, then draws it if they show.
    fn refresh_screen(&mut self, device: Option<&D200>) -> Result<()> {
        let (screen, _) = self.current_screen();
        let device_stats = screen.content == ScreenContent::DeviceStats;
        if (visor::uses_usage(&screen) || device_stats) && self.last_usage.elapsed() >= USAGE_POLL {
            self.usage = self.sampler.sample();
            self.last_usage = Instant::now();
        }
        if let Some(rx) = &self.playing_rx {
            while let Ok(playing) = rx.try_recv() {
                self.playing = playing;
            }
        }
        if self.redraw_screen() {
            self.sync_keys(device)?;
            if let Some(device) = device {
                // The visor may have switched between what the device and the app draw.
                self.keepalive(device)?;
            }
        }
        Ok(())
    }

    /// A short tap or a hold on the visor: the timer's controls, or the visor's action.
    fn tap_screen(&mut self, held: Duration, device: &D200) -> Result<()> {
        let (screen, rule) = self.current_screen();
        let origin = rule.map(|i| self.config.rules[i].name.clone());
        let (label, action) = match screen.content {
            ScreenContent::Timer { .. } => {
                let label = if held >= HOLD {
                    self.timer.reset();
                    "Timer zerado"
                } else {
                    self.timer.toggle();
                    if self.timer.running() { "Timer rodando" } else { "Timer pausado" }
                };
                info!("visor: {}", label.to_lowercase());
                self.refresh_screen(Some(device))?;
                (label.to_string(), None)
            }
            _ => {
                // Older configs put the visor's action on key 14.
                let fallback = self.resolved.get(&(WINDOW_INDEX as u8 + 1)).and_then(|s| s.key.action.clone());
                ("Visor".to_string(), screen.action.clone().or(fallback))
            }
        };
        let activity = Activity {
            number: WINDOW_INDEX as u8 + 1,
            label,
            rule: origin,
            action: action.as_ref().map(actions::describe),
            error: None,
        };
        (self.on_activity)(&activity);
        if let Some(action) = action {
            info!("visor: {}", actions::describe(&action));
            let sink = Arc::clone(&self.on_activity);
            actions::run(action, None, move |error| sink(&Activity { error: Some(error), ..activity }));
        }
        Ok(())
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
        self.resolved = resolve(&self.config, &self.active);
        self.poll_processes();
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
            .resolved
            .values()
            .flat_map(|slot| [slot.key.action.as_ref(), slot.key.toggle.as_ref().and_then(|t| t.action.as_ref())])
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
        if apps_running == self.apps_running {
            return false;
        }
        self.apps_running = apps_running;
        true
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
        // A rule with its own visor may have started or stopped.
        self.redraw_screen();
        self.status_dirty = true;
        let Some(device) = device else {
            // Nothing on the device to keep in sync: send everything on connect.
            self.sent.clear();
            return Ok(());
        };
        let base = self.config_base();
        let mut views = build_views(&self.resolved, &base, &mut self.icons, &self.config, &self.toggled, &self.apps_running);
        // The visor is not a key: it shows the app's drawing, or nothing when the device draws.
        views.insert(WINDOW_INDEX, KeyView { text: String::new(), png: self.screen_png.clone() });
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
        let (screen, _) = self.current_screen();
        let mode = visor::window_mode(&screen);
        let (cpu, mem) = if mode == WindowMode::Stats {
            (self.usage.cpu.round() as u8, self.usage.memory.round() as u8)
        } else {
            (0, 0)
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
        if let Some(device) = device {
            // The visor may have changed between what the device and the app draw.
            self.keepalive(device)?;
        }
        Ok(())
    }

    fn press(&mut self, index: usize, device: &D200) -> Result<()> {
        let number = index as u8 + 1;
        let slot = self.resolved.get(&number).cloned();
        let rule = slot.as_ref().and_then(|s| s.rule).map(|i| &self.config.rules[i]);
        let toggled = slot.as_ref().is_some_and(|s| is_toggled(&self.config, &self.toggled, s, number));
        let current = slot.as_ref().map(|s| face(&s.key, toggled));
        let activity = Activity {
            number,
            label: current.as_ref().map(|f| f.label.to_string()).unwrap_or_default(),
            rule: rule.map(|r| r.name.clone()),
            action: current.as_ref().and_then(|f| f.action).map(actions::describe),
            error: None,
        };
        (self.on_activity)(&activity);
        let action = current.and_then(|f| f.action.cloned());
        let Some(slot) = slot else {
            info!("tecla {number}: vazia");
            return Ok(());
        };
        let origin = rule.map_or_else(|| "padrão".to_string(), |r| r.name.clone());
        match action {
            None => info!("tecla {number}: sem ação"),
            Some(action) => {
                let front = rule.filter(|_| slot.key.front).and_then(|r| self.front_for(r));
                let place = match &front {
                    Some(Front::App(process)) => format!(" em {process}"),
                    Some(Front::Tab { tab, .. }) => format!(" na aba {tab} do Edge"),
                    None => String::new(),
                };
                info!("tecla {number} ({origin}): {}{place}", actions::describe(&action));
                let sink = Arc::clone(&self.on_activity);
                actions::run(action, front, move |error| sink(&Activity { error: Some(error), ..activity }));
            }
        }
        if slot.key.toggle.is_some() {
            let id = toggle_id(&self.config, &slot, number);
            if !self.toggled.remove(&id) {
                self.toggled.insert(id);
            }
            self.sync_keys(Some(device))?;
        }
        Ok(())
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
        Config::dir_of(&self.config_path)
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
                let toggled = slot.is_some_and(|s| is_toggled(&self.config, &self.toggled, s, number));
                let current = slot.map(|s| face(&s.key, toggled));
                let default_text = default_text_color(&self.config.label);
                let image = current
                    .as_ref()
                    .and_then(|f| {
                        let look = look(f, &self.config.label, &default_text, &self.apps_running);
                        cached_icon(&mut self.icons, &look, &base, number as usize)
                    })
                    .map(|png| icons::data_url(&png));
                let rule = slot.and_then(|s| s.rule).map(|i| &self.config.rules[i]);
                KeyStatus {
                    number,
                    toggled,
                    label: current.as_ref().map(|f| f.label.to_string()).unwrap_or_default(),
                    image,
                    action: current.as_ref().and_then(|f| f.action).map(actions::describe),
                    rule: rule.map(|r| r.name.clone()),
                    mode: rule.map(|r| r.mode),
                    beaten,
                }
            })
            .collect();
        let (screen, screen_rule) = self.current_screen();
        let screen = ScreenStatus {
            image: self.screen_png.as_deref().map(icons::data_url),
            content: serde_json::to_value(&screen.content)
                .ok()
                .and_then(|v| v.get("type").and_then(|t| t.as_str()).map(str::to_string))
                .unwrap_or_default(),
            rule: screen_rule.map(|i| self.config.rules[i].name.clone()),
        };
        let mut edge_tabs: Vec<String> = self.context.tabs.open.iter().filter_map(|t| host_of(&t.url)).collect();
        edge_tabs.sort();
        edge_tabs.dedup();
        let mut edge_pages: Vec<EdgePage> = Vec::new();
        for tab in &self.context.tabs.open {
            if let Some(host) = host_of(&tab.url).filter(|h| !h.is_empty()) {
                if !edge_pages.iter().any(|p| p.host == host) {
                    edge_pages.push(EdgePage { host, url: tab.url.clone(), title: tab.title.clone() });
                }
            }
        }
        Status {
            config_path: self.config_path.display().to_string(),
            config_error: self.config_error.clone(),
            config_revision: self.config_revision,
            edge_tabs,
            edge_pages,
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
            screen,
        }
    }
}

fn mode_name(mode: RuleMode) -> &'static str {
    match mode {
        RuleMode::Open => "aberto",
        RuleMode::Focus => "foco",
    }
}

fn build_views(
    resolved: &BTreeMap<u8, Slot>,
    base: &Path,
    cache: &mut IconCache,
    config: &Config,
    toggled: &HashSet<ToggleId>,
    apps_running: &HashSet<String>,
) -> BTreeMap<usize, KeyView> {
    let default_text = default_text_color(&config.label);
    (0..KEY_COUNT)
        .map(|index| {
            let number = index + 1;
            let view = match resolved.get(&(number as u8)) {
                None => KeyView::default(),
                Some(slot) => {
                    let current = face(&slot.key, is_toggled(config, toggled, slot, number as u8));
                    // The label is drawn in the image, so each key can have its own colors.
                    let look = look(&current, &config.label, &default_text, apps_running);
                    KeyView { text: String::new(), png: cached_icon(cache, &look, base, number) }
                }
            };
            (index, view)
        })
        .collect()
}

fn cached_icon(cache: &mut IconCache, look: &icons::Look, base: &Path, number: usize) -> Option<Vec<u8>> {
    cache
        .entry(look.cache_key())
        .or_insert_with(|| icons::render_key(look, base).map_err(|e| warn!("tecla {number}: imagem ignorada: {e:#}")).ok())
        .clone()
}

fn changed_keys(old: &BTreeMap<usize, KeyView>, new: &BTreeMap<usize, KeyView>) -> BTreeMap<usize, KeyView> {
    new.iter().filter(|(i, view)| old.get(i) != Some(view)).map(|(i, view)| (*i, view.clone())).collect()
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
    fn two_state_keys_show_their_second_face_when_toggled() {
        let mut config = Config::default();
        let mic = config.keys.get_mut(&1).unwrap();
        mic.toggle = Some(crate::config::KeyFace {
            label: "Mutado".into(),
            icon: Some("micOff".into()),
            color: "#3A1616".into(),
            ..Default::default()
        });
        let resolved = resolve(&config, &[]);
        let mut cache = IconCache::new();
        let mut toggled = HashSet::new();
        let none = HashSet::new();
        let off = build_views(&resolved, Path::new("."), &mut cache, &config, &toggled, &none);
        toggled.insert((None, 1));
        let on = build_views(&resolved, Path::new("."), &mut cache, &config, &toggled, &none);
        // Labels are drawn into the images, so the second face is a different image.
        assert_eq!(on[&0].text, "");
        assert_ne!(on[&0].png, off[&0].png);
        // The second face has no action of its own: it repeats the key's.
        assert_eq!(face(&resolved[&1].key, true).action, resolved[&1].key.action.as_ref());
    }

    #[test]
    fn default_config_renders_every_key() {
        let config = Config::default();
        let mut cache = IconCache::new();
        let views = build_views(&resolve(&config, &[]), Path::new("."), &mut cache, &config, &HashSet::new(), &HashSet::new());
        assert_eq!(views.len(), KEY_COUNT);
        assert!(views[&0].png.is_some());
        assert_eq!(views[&12], KeyView::default());
        // Keys sharing an icon and color share one rendering.
        assert!(cache.len() <= config.keys.len());
    }
}
