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
use crate::config::{Action, Config, Folder, Key, LabelStyle, Rule, RuleMode, SettingId, When, Window, BACK_KEY};
use crate::context::{watch_foreground, ProcessList};
use crate::icons;
use crate::rules::{
    active_rules, host_of, matching_tab, resolve, resolve_folder, simulated_context, site_matches, Context, Simulation,
    Slot, Tabs, BROWSER,
};
use crate::config::{Screen, ScreenContent, ScreenHold};
use crate::screen::{self as visor, NowPlaying, Timer, Usage, UsageSampler};
use crate::system;

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

/// The visor's touch while it is down.
#[derive(Clone, Copy)]
enum VisorPress {
    Up,
    /// `holds`: holding this visor does something, so a long press is not a tap.
    Down { since: Instant, holds: bool },
    /// The hold already ran; the release does nothing.
    Held,
}

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
    /// The label of the folder showing in place of the layout.
    pub folder: Option<String>,
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

/// Which two-state keys show their second face: (rule name, folder key,
/// key number), `None` being the default layout and no folder. By name so
/// reordering rules keeps it.
type ToggleId = (Option<String>, Option<u8>, u8);

/// A folder showing on the device in place of the layout.
struct OpenFolder {
    /// Where its key is, by rule name, so a config reload finds it again.
    rule: Option<String>,
    number: u8,
    /// The last press, to close a forgotten folder.
    touched: Instant,
}

/// What keys show besides their config: states that change on their own.
#[derive(Default)]
struct Live {
    /// Two-state keys showing their second face after a press.
    toggled: HashSet<ToggleId>,
    /// Running apps that "open app" keys start (lowercase exe names).
    apps_running: HashSet<String>,
    /// Windows settings keys show, as last read (`None`: not on this PC).
    settings: HashMap<SettingId, Option<bool>>,
}

impl Live {
    /// `Some` for a key that shows a Windows setting: its state, if known.
    fn setting(&self, key: &Key) -> Option<Option<bool>> {
        let id = key.action.as_ref()?.watched_setting()?;
        Some(self.settings.get(&id).copied().flatten())
    }

    /// A key showing a Windows setting has its second face while the setting
    /// is off, whoever turned it off; any other one after an odd number of presses.
    fn toggled(&self, config: &Config, slot: &Slot, number: u8) -> bool {
        if slot.key.toggle.is_none() {
            return false;
        }
        match self.setting(&slot.key) {
            Some(state) => state == Some(false),
            None => self.toggled.contains(&toggle_id(config, slot, number)),
        }
    }

    /// Darkened: an "open app" key whose app is closed, a setting this PC
    /// lacks, or a setting that is off on a key with no second face for it.
    fn dim(&self, key: &Key, face: &Face) -> bool {
        let closed = face.action.and_then(Action::watched_process).is_some_and(|process| !self.apps_running.contains(&process));
        let setting = match self.setting(key) {
            Some(None) => true,
            Some(Some(false)) => key.toggle.is_none(),
            _ => false,
        };
        closed || setting
    }
}

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

/// The image a face makes, with the label style's defaults filled in.
fn look<'a>(face: &Face<'a>, style: &'a LabelStyle, default_text: &'a str, dim: bool) -> icons::Look<'a> {
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
    (slot.rule.map(|i| config.rules[i].name.clone()), slot.folder, number)
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

    /// The visor showing now, and the rule it comes from (the last active
    /// rule with a visor of its own wins, like keys). Its `content` is what
    /// the hold cycle reached.
    fn current_screen(&self) -> (Screen, Option<usize>) {
        let (mut screen, rule) = self
            .active
            .iter()
            .rev()
            .find_map(|&i| self.config.rules[i].screen.clone().map(|s| (s, Some(i))))
            .unwrap_or_else(|| (self.config.default_screen(), None));
        let step = self.screen_steps.get(&self.screen_origin(rule)).copied().unwrap_or(0);
        screen.content = screen.showing(step);
        (screen, rule)
    }

    fn screen_origin(&self, rule: Option<usize>) -> Option<String> {
        rule.map(|i| self.config.rules[i].name.clone())
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

    /// A tap on the visor: the timer's controls (two quick taps reset it), or the visor's action.
    fn tap_screen(&mut self, device: &D200) -> Result<()> {
        let (screen, rule) = self.current_screen();
        if let ScreenContent::Timer { .. } = screen.content {
            let label = match self.last_timer_tap.take() {
                Some(at) if at.elapsed() <= DOUBLE_TAP => {
                    self.timer.reset();
                    "Timer zerado"
                }
                _ => {
                    self.timer.toggle();
                    self.last_timer_tap = Some(Instant::now());
                    if self.timer.running() { "Timer rodando" } else { "Timer pausado" }
                }
            };
            info!("visor: {}", label.to_lowercase());
            self.refresh_screen(Some(device))?;
            self.report_screen(label.to_string(), rule, None);
            return Ok(());
        }
        // Older configs put the visor's action on key 14.
        let fallback = || self.resolved.get(&(WINDOW_INDEX as u8 + 1)).and_then(|s| s.key.action.clone());
        let action = screen.action.clone().or_else(fallback);
        self.report_screen("Visor".to_string(), rule, action);
        Ok(())
    }

    /// Holding the visor: its own action, or the next content of its cycle.
    fn hold_screen(&mut self, device: &D200) -> Result<()> {
        let (screen, rule) = self.current_screen();
        match screen.hold {
            Some(ScreenHold::Cycle { contents }) if !contents.is_empty() => {
                let origin = self.screen_origin(rule);
                let step = self.screen_steps.entry(origin).or_insert(0);
                *step = (*step + 1) % (contents.len() + 1);
                let name = self.current_screen().0.content.name();
                info!("visor: {name}");
                self.refresh_screen(Some(device))?;
                self.report_screen(format!("Visor: {name}"), rule, None);
            }
            Some(ScreenHold::Action { action: Some(action) }) => {
                self.report_screen("Visor (segurar)".to_string(), rule, Some(action));
            }
            _ => {}
        }
        Ok(())
    }

    /// Tells the app what the visor did, then runs the action, if any.
    fn report_screen(&self, label: String, rule: Option<usize>, action: Option<Action>) {
        let activity = Activity {
            number: WINDOW_INDEX as u8 + 1,
            label,
            rule: self.screen_origin(rule),
            action: action.as_ref().map(actions::describe),
            error: None,
        };
        (self.on_activity)(&activity);
        if let Some(action) = action {
            info!("visor: {}", actions::describe(&action));
            let sink = Arc::clone(&self.on_activity);
            actions::run(action, None, move |error| sink(&Activity { error: Some(error), ..activity }));
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

    /// Every key that may show soon: the layers' (beaten ones too), those in
    /// their folders and the open folder's, so opening a folder or switching
    /// rules never shows a key in a stale state.
    fn keys_in_view(&self) -> impl Iterator<Item = &Key> {
        // Right after a reload `active` still points into the old rules: skip what is gone.
        let rules = self.active.iter().filter_map(|&i| self.config.rules.get(i)).flat_map(|rule| rule.keys.values());
        let layers = self.config.keys.values().chain(rules);
        let open = self.open_folder().map(|(folder, ..)| folder.keys.values());
        layers.flat_map(|key| std::iter::once(key).chain(key.folder.iter().flat_map(|f| f.keys.values()))).chain(open.into_iter().flatten())
    }

    /// What each key does now: the open folder's keys, or the rule layers'.
    fn layout(&self) -> BTreeMap<u8, Slot> {
        match self.open_folder() {
            Some((folder, rule, number)) => resolve_folder(folder, rule, number),
            None => resolve(&self.config, &self.active),
        }
    }

    /// The open folder as the config has it now: the folder, its rule's index
    /// and its key's number. `None` once its key is gone from the config.
    fn open_folder(&self) -> Option<(&Folder, Option<usize>, u8)> {
        let (key, rule) = self.open_folder_key()?;
        Some((key.folder.as_ref()?, rule, self.folder.as_ref()?.number))
    }

    fn open_folder_key(&self) -> Option<(&Key, Option<usize>)> {
        let open = self.folder.as_ref()?;
        match &open.rule {
            None => Some((self.config.keys.get(&open.number)?, None)),
            Some(name) => {
                let i = self.config.rules.iter().position(|r| &r.name == name)?;
                Some((self.config.rules[i].keys.get(&open.number)?, Some(i)))
            }
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
        if self.folder.is_some() && self.open_folder().is_none() {
            info!("pasta fechada: ela saiu da config");
            self.folder = None;
        }
        self.resolved = self.layout();
        self.read_new_settings();
        // A rule with its own visor may have started or stopped.
        self.redraw_screen();
        self.status_dirty = true;
        let Some(device) = device else {
            // Nothing on the device to keep in sync: send everything on connect.
            self.sent.clear();
            return Ok(());
        };
        let base = self.config_base();
        let mut views = build_views(&self.resolved, &base, &mut self.icons, &self.config, &self.live);
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
        self.poll_settings();
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
        let toggled = slot.as_ref().is_some_and(|s| self.live.toggled(&self.config, s, number));
        let current = slot.as_ref().map(|s| face(&s.key, toggled));
        let in_folder = self.folder.is_some();
        let back = in_folder && number == BACK_KEY;
        let opens = slot.as_ref().is_some_and(|s| s.key.folder.is_some());
        let described = match () {
            _ if back => Some("voltar".to_string()),
            _ if opens => Some("abrir pasta".to_string()),
            _ => current.as_ref().and_then(|f| f.action).map(actions::describe),
        };
        let activity = Activity {
            number,
            label: current.as_ref().map(|f| f.label.to_string()).unwrap_or_default(),
            rule: rule.map(|r| r.name.clone()),
            action: described,
            error: None,
        };
        (self.on_activity)(&activity);
        if let Some(open) = &mut self.folder {
            open.touched = Instant::now();
        }
        let action = current.and_then(|f| f.action.cloned());
        let Some(slot) = slot else {
            info!("tecla {number}: vazia");
            return Ok(());
        };
        let origin = rule.map_or_else(|| "padrão".to_string(), |r| r.name.clone());
        if back {
            info!("pasta fechada");
            self.folder = None;
            return self.sync_keys(Some(device));
        }
        if opens {
            info!("tecla {number} ({origin}): pasta aberta");
            let rule = rule.map(|r| r.name.clone());
            self.folder = Some(OpenFolder { rule, number, touched: Instant::now() });
            return self.sync_keys(Some(device));
        }
        let ran = action.is_some();
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
                if action.watched_setting().is_some() {
                    self.settings_fast_until = Some(Instant::now() + SETTING_FAST_FOR);
                }
                let sink = Arc::clone(&self.on_activity);
                actions::run(action, front, move |error| sink(&Activity { error: Some(error), ..activity }));
            }
        }
        let mut changed = false;
        // A key showing a Windows setting follows the setting, not the presses.
        if slot.key.toggle.is_some() && self.live.setting(&slot.key).is_none() {
            let id = toggle_id(&self.config, &slot, number);
            if !self.live.toggled.remove(&id) {
                self.live.toggled.insert(id);
            }
            changed = true;
        }
        // Picking something in a folder closes it, unless it stays open.
        if ran && in_folder && !self.open_folder().is_some_and(|(folder, ..)| folder.stay) {
            info!("pasta fechada");
            self.folder = None;
            changed = true;
        }
        if changed {
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
        let in_folder = self.folder.is_some();
        let keys = (1..=KEY_COUNT as u8)
            .map(|number| {
                let slot = self.resolved.get(&number);
                // An open folder is no layer: no rule fights over its keys.
                let contenders: Vec<usize> = if in_folder {
                    Vec::new()
                } else {
                    self.active.iter().copied().filter(|&i| self.config.rules[i].keys.contains_key(&number)).collect()
                };
                let beaten = contenders
                    .split_last()
                    .map(|(_, losers)| losers.iter().map(|&i| self.config.rules[i].name.clone()).collect())
                    .unwrap_or_default();
                let toggled = slot.is_some_and(|s| self.live.toggled(&self.config, s, number));
                let current = slot.map(|s| face(&s.key, toggled));
                let default_text = default_text_color(&self.config.label);
                let image = slot
                    .zip(current.as_ref())
                    .and_then(|(s, f)| {
                        let look = look(f, &self.config.label, &default_text, self.live.dim(&s.key, f));
                        cached_icon(&mut self.icons, &look, &base, number as usize)
                    })
                    .map(|png| icons::data_url(&png));
                let rule = slot.and_then(|s| s.rule).map(|i| &self.config.rules[i]);
                let action = match () {
                    _ if in_folder && number == BACK_KEY => Some("voltar".to_string()),
                    _ if slot.is_some_and(|s| s.key.folder.is_some()) => Some("abrir pasta".to_string()),
                    _ => current.as_ref().and_then(|f| f.action).map(actions::describe),
                };
                KeyStatus {
                    number,
                    toggled,
                    label: current.as_ref().map(|f| f.label.to_string()).unwrap_or_default(),
                    image,
                    action,
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
            folder: self.open_folder_key().map(|(key, _)| if key.label.is_empty() { "Pasta".to_string() } else { key.label.clone() }),
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
    live: &Live,
) -> BTreeMap<usize, KeyView> {
    let default_text = default_text_color(&config.label);
    (0..KEY_COUNT)
        .map(|index| {
            let number = index + 1;
            let view = match resolved.get(&(number as u8)) {
                None => KeyView::default(),
                Some(slot) => {
                    let current = face(&slot.key, live.toggled(config, slot, number as u8));
                    // The label is drawn in the image, so each key can have its own colors.
                    let look = look(&current, &config.label, &default_text, live.dim(&slot.key, &current));
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
        let mut live = Live::default();
        let off = build_views(&resolved, Path::new("."), &mut cache, &config, &live);
        live.toggled.insert((None, None, 1));
        let on = build_views(&resolved, Path::new("."), &mut cache, &config, &live);
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
        let views = build_views(&resolve(&config, &[]), Path::new("."), &mut cache, &config, &Live::default());
        assert_eq!(views.len(), KEY_COUNT);
        assert!(views[&0].png.is_some());
        assert_eq!(views[&12], KeyView::default());
        // Keys sharing an icon and color share one rendering.
        assert!(cache.len() <= config.keys.len());
    }

    fn bluetooth_key(second_face: bool) -> Key {
        let action = Action::System { setting: system::Setting::Bluetooth, set: system::Switch::Toggle, value: None };
        let toggle = second_face.then(|| crate::config::KeyFace { icon: Some("bluetoothOff".into()), ..Default::default() });
        Key { icon: Some("bluetooth".into()), action: Some(action), toggle, ..Key::default() }
    }

    #[test]
    fn setting_keys_show_the_real_state() {
        let config = Config::default();
        let id = (system::Setting::Bluetooth, None);
        let slot = Slot { key: bluetooth_key(true), rule: None, folder: None };
        let mut live = Live::default();
        // Presses don't count: only the setting does.
        live.toggled.insert((None, None, 1));
        live.settings.insert(id.clone(), Some(true));
        assert!(!live.toggled(&config, &slot, 1));
        live.settings.insert(id.clone(), Some(false));
        assert!(live.toggled(&config, &slot, 1), "off shows the second face");
        assert!(!live.dim(&slot.key, &face(&slot.key, true)));
        live.settings.insert(id.clone(), None);
        assert!(live.dim(&slot.key, &face(&slot.key, false)), "no Bluetooth on this PC: dimmed");
        // With no second face, off is shown by dimming.
        let plain = bluetooth_key(false);
        live.settings.insert(id, Some(false));
        assert!(live.dim(&plain, &face(&plain, false)));
    }

    #[test]
    fn toggles_in_a_folder_are_kept_apart_from_the_layout() {
        let mut config = Config::default();
        let mut mic = config.keys[&1].clone();
        mic.toggle = Some(crate::config::KeyFace::default());
        config.keys.insert(1, mic.clone());
        let layout = Slot { key: mic.clone(), rule: None, folder: None };
        let inside = Slot { key: mic, rule: None, folder: Some(5) };
        let mut live = Live::default();
        live.toggled.insert(toggle_id(&config, &layout, 1));
        assert!(live.toggled(&config, &layout, 1));
        assert!(!live.toggled(&config, &inside, 1));
    }
}
