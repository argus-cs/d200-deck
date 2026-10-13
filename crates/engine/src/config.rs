use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::actions::parse_hotkey;
use crate::icons;
use crate::system::{self, Kind, Setting, Switch};

/// Keys are numbered like on the device: 1 to 13, and 14 for the status window.
pub const KEY_NUMBERS: std::ops::RangeInclusive<u8> = 1..=14;
/// In an open folder this key goes back to the layout, so a folder holds keys 2 to 13.
pub const BACK_KEY: u8 = 1;
const VISOR_KEY: u8 = 14;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub brightness: u8,
    pub window: Window,
    pub label: LabelStyle,
    pub keys: BTreeMap<u8, Key>,
    /// Layers over `keys`; see `rules::active_rules` for the order.
    pub rules: Vec<Rule>,
    /// The visor. Configs from before it existed only have `window`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screen: Option<Screen>,
}

/// What the visor (the wide screen, key 14) shows.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Screen {
    pub content: ScreenContent,
    #[serde(default = "default_screen_background")]
    pub background: String,
    /// Text color.
    #[serde(default = "default_screen_color")]
    pub color: String,
    /// Bars and highlights.
    #[serde(default = "default_screen_accent")]
    pub accent: String,
    /// Runs when the visor is tapped (the timer uses taps itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
    /// What holding the visor does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<ScreenHold>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScreenHold {
    Action {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        action: Option<Action>,
    },
    /// Each hold shows the next of these, then back to the screen's own content.
    Cycle {
        #[serde(default)]
        contents: Vec<ScreenContent>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ScreenContent {
    /// The clock the device draws itself.
    DeviceClock,
    /// The CPU and memory view the device draws itself.
    DeviceStats,
    Clock {
        #[serde(default = "enabled_by_default")]
        hour24: bool,
        #[serde(default)]
        seconds: bool,
        #[serde(default = "enabled_by_default")]
        date: bool,
    },
    Stats {
        #[serde(default = "enabled_by_default")]
        cpu: bool,
        #[serde(default = "enabled_by_default")]
        memory: bool,
        #[serde(default)]
        gpu: bool,
        #[serde(default)]
        network: bool,
    },
    ClockStats {
        #[serde(default = "enabled_by_default")]
        hour24: bool,
        #[serde(default)]
        seconds: bool,
    },
    NowPlaying,
    /// 0 minutes counts up (stopwatch); more counts down (25 = Pomodoro).
    Timer {
        #[serde(default)]
        minutes: u32,
    },
    Image {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
    },
    Text {
        #[serde(default)]
        text: String,
    },
}

fn default_screen_background() -> String {
    "#16171A".into()
}

fn default_screen_color() -> String {
    "#ECEDEF".into()
}

fn default_screen_accent() -> String {
    "#F0A63A".into()
}

impl Screen {
    pub fn new(content: ScreenContent) -> Self {
        Self {
            content,
            background: default_screen_background(),
            color: default_screen_color(),
            accent: default_screen_accent(),
            action: None,
            hold: None,
        }
    }

    /// What it shows now: its own content, or one of the hold cycle's.
    pub fn showing(&self, step: usize) -> ScreenContent {
        match &self.hold {
            Some(ScreenHold::Cycle { contents }) if !contents.is_empty() => match step % (contents.len() + 1) {
                0 => self.content.clone(),
                i => contents[i - 1].clone(),
            },
            _ => self.content.clone(),
        }
    }

    /// Whether holding it does anything; when not, a long press is a tap.
    pub fn holds(&self) -> bool {
        match &self.hold {
            Some(ScreenHold::Action { action }) => action.is_some(),
            Some(ScreenHold::Cycle { contents }) => !contents.is_empty(),
            None => false,
        }
    }
}

impl ScreenContent {
    /// A name for people, like the editor shows.
    pub fn name(&self) -> &'static str {
        match self {
            ScreenContent::DeviceClock => "Relógio do aparelho",
            ScreenContent::DeviceStats => "Uso do PC do aparelho",
            ScreenContent::Clock { .. } => "Relógio",
            ScreenContent::Stats { .. } => "Uso do PC",
            ScreenContent::ClockStats { .. } => "Relógio + uso do PC",
            ScreenContent::NowPlaying => "Agora tocando",
            ScreenContent::Timer { .. } => "Cronômetro",
            ScreenContent::Image { .. } => "Imagem",
            ScreenContent::Text { .. } => "Texto",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub when: When,
    pub mode: RuleMode,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    /// Only the keys this rule replaces; the others keep the default layout.
    #[serde(default)]
    pub keys: BTreeMap<u8, Key>,
    /// Replaces the visor while the rule applies.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screen: Option<Screen>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum When {
    Process(String),
    /// A site in Edge: a domain ("youtube.com", subdomains included) or an
    /// address with `*` wildcards ("github.com/*/pulls").
    Site(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleMode {
    /// While the app is running (even minimized) or the site has a tab open.
    Open,
    /// Only while the app's window, or the site's tab in Edge, is in front.
    Focus,
}

impl Rule {
    /// The process to match: lowercase, with ".exe" added when missing.
    pub fn process(&self) -> Option<String> {
        match &self.when {
            When::Process(name) => Some(normalize_process(name)),
            When::Site(_) => None,
        }
    }

    pub fn site(&self) -> Option<String> {
        match &self.when {
            When::Site(site) => Some(normalize_site(site)),
            When::Process(_) => None,
        }
    }
}

pub fn normalize_process(name: &str) -> String {
    let lower = name.trim().to_ascii_lowercase();
    if lower.ends_with(".exe") { lower } else { format!("{lower}.exe") }
}

/// "https://www.YouTube.com/" → "youtube.com".
pub fn normalize_site(site: &str) -> String {
    let lower = site.trim().to_ascii_lowercase();
    let bare = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")).unwrap_or(&lower);
    bare.strip_prefix("www.").unwrap_or(bare).trim_end_matches('/').to_string()
}

fn enabled_by_default() -> bool {
    true
}

fn is_true(value: &bool) -> bool {
    *value
}

impl Action {
    /// The process an "open" action starts, when it starts an app (lowercase
    /// exe name), so its key can show whether the app is running.
    pub fn watched_process(&self) -> Option<String> {
        let Action::Open { target, watch: true, .. } = self else {
            return None;
        };
        let target = target.trim().trim_matches('"');
        // URLs and URI schemes ("https://…", "spotify:") are not processes;
        // a drive letter ("C:\…") is.
        let url = target.contains("://");
        let scheme = target.contains(':') && !target.contains(":\\") && !target.contains(":/");
        if url || scheme {
            return None;
        }
        let name = target.rsplit(['\\', '/']).next()?.to_ascii_lowercase();
        let exe = match name.rsplit_once('.') {
            Some((_, "exe")) => name,
            None if !name.is_empty() => format!("{name}.exe"),
            _ => return None,
        };
        match exe.as_str() {
            // The shell itself: always running, so it would never dim.
            "explorer.exe" => None,
            // Store aliases that start a process under another name.
            "wt.exe" => Some("windowsterminal.exe".into()),
            _ => Some(exe),
        }
    }

    /// The Windows setting a "system" action changes, when it has a state
    /// to show on the key (Bluetooth on, this audio output chosen…).
    pub fn watched_setting(&self) -> Option<SettingId> {
        match self {
            Action::System { setting, value, .. } if setting.kind() != Kind::Once => Some((*setting, value.clone())),
            _ => None,
        }
    }
}

/// A setting and, for those that pick one, the choice a key stands for.
pub type SettingId = (Setting, Option<String>);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Window {
    #[default]
    Clock,
    Stats,
    Image,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LabelStyle {
    pub show: bool,
    pub size: u8,
    pub color: String,
    pub align: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    #[serde(default)]
    pub label: String,
    /// A built-in glyph name (see `icons::GLYPHS`) or a path to a PNG file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Background color.
    #[serde(default = "default_key_color")]
    pub color: String,
    /// Built-in icon color; `None` is the usual light gray.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    /// Label color; `None` uses the label style's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    /// A frame around the key; `None` is no frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
    /// On a rule's key: bring the rule's app to the front for the action,
    /// then give the focus back. Shortcuts only reach the window in front.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub front: bool,
    /// A second look: each press runs the current action and switches
    /// between this face and the key's own (microphone on / muted). With
    /// a Windows setting, it shows while the setting is off instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toggle: Option<KeyFace>,
    /// Pressing it shows these keys in place of the layout, instead of an action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<Folder>,
}

/// Keys that replace the whole layout while open. Key 1 goes back; using
/// any other key with an action goes back too, unless the folder `stay`s.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Folder {
    /// Numbered like the layout's, 2 to 13 (see `BACK_KEY`).
    #[serde(default)]
    pub keys: BTreeMap<u8, Key>,
    /// Stays open after a key runs, for keys used in a row (volume up and down).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stay: bool,
}

/// The second state of a two-state key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KeyFace {
    #[serde(default)]
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default = "default_key_color")]
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    /// `None` runs the key's own action in this state too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
}

impl Default for Key {
    fn default() -> Self {
        Self {
            label: String::new(),
            icon: None,
            color: default_key_color(),
            icon_color: None,
            text_color: None,
            border: None,
            action: None,
            front: false,
            toggle: None,
            folder: None,
        }
    }
}

impl Key {
    /// Key 1 of an open folder. The editor draws the same (types.ts `BACK_KEY`).
    pub fn back() -> Self {
        Self { label: "Voltar".into(), icon: Some("back".into()), icon_color: Some("#F0A63A".into()), ..Self::default() }
    }

    /// How its first state looks.
    pub fn appearance(&self) -> Appearance<'_> {
        Appearance {
            label: &self.label,
            icon: self.icon.as_deref(),
            color: &self.color,
            icon_color: self.icon_color.as_deref(),
            text_color: self.text_color.as_deref(),
            border: self.border.as_deref(),
        }
    }
}

impl KeyFace {
    pub fn appearance(&self) -> Appearance<'_> {
        Appearance {
            label: &self.label,
            icon: self.icon.as_deref(),
            color: &self.color,
            icon_color: self.icon_color.as_deref(),
            text_color: self.text_color.as_deref(),
            border: self.border.as_deref(),
        }
    }
}

/// What a key or its second state shows: the fields `Key` and `KeyFace`
/// share. `icons::Look::new` turns it into what gets drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Appearance<'a> {
    pub label: &'a str,
    pub icon: Option<&'a str>,
    pub color: &'a str,
    pub icon_color: Option<&'a str>,
    pub text_color: Option<&'a str>,
    pub border: Option<&'a str>,
}

impl Default for KeyFace {
    fn default() -> Self {
        Self { label: String::new(), icon: None, color: default_key_color(), icon_color: None, text_color: None, border: None, action: None }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Action {
    Hotkey { keys: String },
    Open {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<String>,
        /// For apps: the key dims while the app is not running.
        #[serde(default = "enabled_by_default", skip_serializing_if = "is_true")]
        watch: bool,
    },
    Command { command: String },
    Text { text: String },
    Media { key: MediaKey },
    /// A Windows setting: Bluetooth, sound, theme, the audio output…
    System {
        setting: Setting,
        /// For on/off settings: switch, or force one state.
        #[serde(default, skip_serializing_if = "Switch::is_toggle")]
        set: Switch,
        /// For settings that pick one: the audio output's name, "extend"…
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKey {
    PlayPause,
    Next,
    Previous,
    Stop,
    VolumeUp,
    VolumeDown,
    Mute,
}

fn default_key_color() -> String {
    "#24262B".into()
}

impl Default for LabelStyle {
    fn default() -> Self {
        Self { show: true, size: 10, color: "FFFFFF".into(), align: "bottom".into() }
    }
}

impl Default for Config {
    /// The default layout from the interface prototype.
    fn default() -> Self {
        let hotkey = |keys: &str| Action::Hotkey { keys: keys.into() };
        let open = |target: &str| Action::Open { target: target.into(), args: None, watch: true };
        let media = |key| Action::Media { key };
        let entries = [
            (1, "micOff", "Mutar mic", hotkey("Win+Alt+K")),
            (2, "play", "Play/Pause", media(MediaKey::PlayPause)),
            (3, "next", "Próxima", media(MediaKey::Next)),
            (4, "volume", "Mudo", media(MediaKey::Mute)),
            (5, "capture", "Captura", hotkey("Win+Shift+S")),
            (6, "terminal", "Terminal", open("wt.exe")),
            (7, "globe", "Edge", open("msedge.exe")),
            (8, "folder", "Arquivos", open("explorer.exe")),
            (9, "music", "Spotify", open("spotify:")),
            (10, "lock", "Bloquear", Action::Command { command: "rundll32.exe user32.dll,LockWorkStation".into() }),
            (11, "record", "Gravar tela", hotkey("Win+Alt+R")),
            (12, "desktop", "Desktop", hotkey("Win+D")),
        ];
        let keys = entries
            .into_iter()
            .map(|(n, icon, label, action)| {
                let key = Key { label: label.into(), icon: Some(icon.into()), action: Some(action), ..Key::default() };
                (n, key)
            })
            .collect();
        Self { brightness: 80, window: Window::Clock, label: LabelStyle::default(), keys, rules: Vec::new(), screen: None }
    }
}

impl Config {
    /// The default visor: `screen`, or what the older `window` field meant.
    pub fn default_screen(&self) -> Screen {
        self.screen.clone().unwrap_or_else(|| {
            Screen::new(match self.window {
                Window::Clock => ScreenContent::DeviceClock,
                Window::Stats => ScreenContent::DeviceStats,
                Window::Image => ScreenContent::Image { icon: self.keys.get(&14).and_then(|k| k.icon.clone()) },
            })
        })
    }

    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("D200Deck").join("config.json")
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            return Self::load(path);
        }
        let mut config = Self::default();
        config.fill_screen();
        config.save(path)?;
        log::info!("config criada com o layout padrão em {}", path.display());
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("não consegui ler {}", path.display()))?;
        let mut config: Self = serde_json::from_str(&text).with_context(|| format!("JSON inválido em {}", path.display()))?;
        config.rename_repeated_rules();
        config.fill_screen();
        config.validate()?;
        Ok(config)
    }

    /// Configs from before the visor only have `window`, and `Default` leaves
    /// `screen` empty (missing fields come from it, so a filled one would
    /// replace what `window` means). The window edits `screen`, and filling it
    /// in there counted as an edit nobody made.
    fn fill_screen(&mut self) {
        if self.screen.is_none() {
            self.screen = Some(self.default_screen());
        }
    }

    /// Rules are told apart by name (an open folder, a two-state key), so two
    /// with one name would share those. The editor no longer makes them, but an
    /// older config may hold them: later ones get a free name ("X" → "X (2)").
    fn rename_repeated_rules(&mut self) {
        let mut taken: HashSet<String> = self.rules.iter().map(|rule| rule.name.clone()).collect();
        let mut seen = HashSet::new();
        for (i, rule) in self.rules.iter_mut().enumerate() {
            if seen.insert(rule.name.clone()) {
                continue;
            }
            let base = if rule.name.trim().is_empty() { format!("regra {}", i + 1) } else { rule.name.clone() };
            let free = std::iter::once(base.clone())
                .chain((2..).map(|n| format!("{base} ({n})")))
                .find(|name| !taken.contains(name))
                .unwrap_or_default();
            log::info!("regra {} renomeada para {free:?}: outra regra já tinha o nome {:?}", i + 1, rule.name);
            taken.insert(free.clone());
            seen.insert(free.clone());
            rule.name = free;
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        // The engine watching the file never reads half of it.
        crate::files::write_atomic(path, serde_json::to_string_pretty(self)? + "\n")
    }

    /// The folder relative icon paths start from.
    pub fn dir_of(path: &Path) -> PathBuf {
        path.parent().unwrap_or(Path::new(".")).to_path_buf()
    }

    pub fn validate(&self) -> Result<()> {
        if self.brightness > 100 {
            bail!("brightness vai de 0 a 100 (está {})", self.brightness);
        }
        if !is_hex_color(&self.label.color, false) {
            bail!("label.color precisa ser hex sem # (ex.: FFFFFF)");
        }
        for (n, key) in &self.keys {
            validate_key(*n, key)?;
        }
        if let Some(screen) = &self.screen {
            validate_screen(screen).context("visor")?;
        }
        let mut names = HashSet::new();
        for rule in &self.rules {
            if !names.insert(rule.name.as_str()) {
                if rule.name.trim().is_empty() {
                    bail!("duas regras estão sem nome: dê um nome a cada uma");
                }
                bail!("duas regras se chamam {:?}: cada regra precisa de um nome só dela", rule.name);
            }
        }
        for (i, rule) in self.rules.iter().enumerate() {
            let name = if rule.name.trim().is_empty() { format!("regra {}", i + 1) } else { rule.name.clone() };
            match &rule.when {
                When::Process(process) if process.trim().is_empty() => bail!("{name}: when.process está vazio"),
                When::Site(site) if normalize_site(site).is_empty() => bail!("{name}: when.site está vazio"),
                _ => {}
            }
            for (n, key) in &rule.keys {
                validate_key(*n, key).with_context(|| name.clone())?;
            }
            if let Some(screen) = &rule.screen {
                validate_screen(screen).with_context(|| format!("{name}: visor"))?;
            }
        }
        Ok(())
    }
}

fn validate_key(n: u8, key: &Key) -> Result<()> {
    if !KEY_NUMBERS.contains(&n) {
        bail!("tecla {n} não existe (use 1 a 14)");
    }
    validate_face(key.icon.as_deref(), &key.color, key.action.as_ref()).with_context(|| format!("tecla {n}"))?;
    validate_colors([&key.icon_color, &key.text_color, &key.border]).with_context(|| format!("tecla {n}"))?;
    if let Some(face) = &key.toggle {
        validate_face(face.icon.as_deref(), &face.color, face.action.as_ref())
            .and_then(|_| validate_colors([&face.icon_color, &face.text_color, &face.border]))
            .with_context(|| format!("tecla {n}, segundo estado"))?;
    }
    if let Some(folder) = &key.folder {
        if n == VISOR_KEY {
            bail!("o visor não pode ser uma pasta");
        }
        if key.action.is_some() || key.toggle.is_some() {
            bail!("tecla {n}: uma pasta não tem ação nem segundo estado");
        }
        for (i, inner) in &folder.keys {
            if *i == BACK_KEY || *i >= VISOR_KEY {
                bail!("tecla {n}: a pasta usa as teclas 2 a 13 (a 1 é \"Voltar\"), não a {i}");
            }
            if inner.folder.is_some() {
                bail!("tecla {n}: uma pasta não pode ter outra pasta dentro");
            }
            validate_key(*i, inner).with_context(|| format!("tecla {n}, pasta"))?;
        }
    }
    Ok(())
}

fn validate_colors(colors: [&Option<String>; 3]) -> Result<()> {
    for color in colors.into_iter().flatten() {
        if !is_hex_color(color, true) {
            bail!("cor precisa ser #RRGGBB (está {color:?})");
        }
    }
    Ok(())
}

fn validate_screen(screen: &Screen) -> Result<()> {
    for color in [&screen.background, &screen.color, &screen.accent] {
        if !is_hex_color(color, true) {
            bail!("cor precisa ser #RRGGBB (está {color:?})");
        }
    }
    let cycle: &[ScreenContent] = match &screen.hold {
        Some(ScreenHold::Cycle { contents }) => contents,
        _ => &[],
    };
    for content in std::iter::once(&screen.content).chain(cycle) {
        let icon = match content {
            ScreenContent::Image { icon } => icon.as_deref(),
            ScreenContent::Timer { minutes } if *minutes > 600 => bail!("o timer vai até 600 minutos"),
            _ => None,
        };
        validate_face(icon, &screen.background, None)?;
    }
    validate_face(None, &screen.background, screen.action.as_ref())?;
    if let Some(ScreenHold::Action { action }) = &screen.hold {
        validate_face(None, &screen.background, action.as_ref()).context("ao segurar")?;
    }
    Ok(())
}

fn validate_face(icon: Option<&str>, color: &str, action: Option<&Action>) -> Result<()> {
    if !is_hex_color(color, true) {
        bail!("color precisa ser #RRGGBB (está {color:?})");
    }
    if let Some(icon) = icon {
        let file = icon.to_ascii_lowercase().ends_with(".png") || icons::is_svg(icon);
        if icon.is_empty() || (!icons::is_glyph(icon) && !file) {
            bail!("ícone {icon:?} não é um ícone embutido nem um arquivo .png ou .svg");
        }
    }
    match action {
        Some(Action::Hotkey { keys }) => {
            parse_hotkey(keys)?;
        }
        Some(Action::System { setting, value, .. }) => system::validate(*setting, value.as_deref())?,
        _ => {}
    }
    Ok(())
}

fn is_hex_color(s: &str, with_hash: bool) -> bool {
    let hex = if with_hash { s.strip_prefix('#') } else { Some(s) };
    matches!(hex, Some(h) if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_rules(names: &[&str]) -> Config {
        let rules: Vec<String> = names.iter().map(|name| format!(r#"{{"name":{name:?},"when":{{"process":"a.exe"}},"mode":"open"}}"#)).collect();
        Config { rules: serde_json::from_str(&format!("[{}]", rules.join(","))).unwrap(), ..Config::default() }
    }

    #[test]
    fn repeated_rule_names_are_refused_when_saving() {
        assert!(with_rules(&["Discord", "YouTube"]).validate().is_ok());
        let err = with_rules(&["Discord", "Discord"]).validate().unwrap_err().to_string();
        assert!(err.contains("duas regras se chamam \"Discord\""), "{err}");
        assert!(with_rules(&["", ""]).validate().unwrap_err().to_string().contains("sem nome"));
    }

    #[test]
    fn older_configs_with_repeated_names_still_load_renamed() {
        let path = std::env::temp_dir().join("deck-engine-rule-names-test").join("config.json");
        with_rules(&["A", "A", "A (2)", "", "", "A"]).save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        let names: Vec<&str> = loaded.rules.iter().map(|rule| rule.name.as_str()).collect();
        // "A (2)" was already some rule's name, so the second "A" skips it.
        assert_eq!(names, ["A", "A (3)", "A (2)", "", "regra 5", "A (4)"]);
    }

    #[test]
    fn configs_load_with_their_visor() {
        let path = std::env::temp_dir().join("deck-engine-visor-test").join("config.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{ "window": "stats" }"#).unwrap();
        assert_eq!(Config::load(&path).unwrap().screen, Some(Screen::new(ScreenContent::DeviceStats)));
        // A new config, as on the first run.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(Config::load_or_create(&path).unwrap().screen, Some(Screen::new(ScreenContent::DeviceClock)));
        assert_eq!(Config::load(&path).unwrap().screen, Some(Screen::new(ScreenContent::DeviceClock)));
    }

    #[test]
    fn default_config_is_valid_and_round_trips() {
        let config = Config::default();
        config.validate().unwrap();
        let json = serde_json::to_string_pretty(&config).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
    }

    #[test]
    fn demo_config_is_valid() {
        // The README screenshots are taken with it (docs/screenshots/capture.mjs).
        let config: Config = serde_json::from_str(include_str!("../../../docs/screenshots/demo-config.json")).unwrap();
        config.validate().unwrap();
        assert_eq!(config.rules.len(), 4);
    }

    #[test]
    fn parses_every_action_type() {
        let json = r##"{
            "keys": {
                "1": { "label": "A", "action": { "type": "hotkey", "keys": "Ctrl+C" } },
                "2": { "action": { "type": "open", "target": "wt.exe", "args": "-p PowerShell" } },
                "3": { "action": { "type": "command", "command": "echo oi" } },
                "4": { "action": { "type": "text", "text": "Já volto!" } },
                "5": { "icon": "play", "color": "#112233", "action": { "type": "media", "key": "play_pause" } }
            }
        }"##;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        assert_eq!(config.brightness, 80);
        assert_eq!(config.keys[&5].action, Some(Action::Media { key: MediaKey::PlayPause }));
        assert_eq!(config.keys[&2].color, "#24262B");
    }

    #[test]
    fn parses_rules() {
        let json = r#"{
            "rules": [
                { "name": "Discord", "when": { "process": "Discord" }, "mode": "open",
                  "keys": { "1": { "label": "Mute", "front": true, "action": { "type": "hotkey", "keys": "Ctrl+Shift+M" } } } },
                { "name": "Photoshop", "when": { "process": "Photoshop.exe" }, "mode": "focus", "enabled": false }
            ]
        }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        assert_eq!(config.rules[0].process().as_deref(), Some("discord.exe"));
        assert_eq!(config.rules[0].mode, RuleMode::Open);
        assert!(config.rules[0].enabled && config.rules[0].keys[&1].front);
        assert!(!config.rules[1].enabled);
        assert!(config.rules[1].keys.is_empty());
    }

    #[test]
    fn parses_two_state_keys() {
        let json = r##"{ "keys": { "1": {
            "label": "Mic", "icon": "mic", "action": { "type": "hotkey", "keys": "Ctrl+Shift+M" },
            "toggle": { "label": "Mutado", "icon": "micOff", "color": "#3A1616" }
        } } }"##;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        let face = config.keys[&1].toggle.as_ref().unwrap();
        assert_eq!(face.label, "Mutado");
        assert_eq!(face.action, None);
        let bad = r#"{ "keys": { "1": { "toggle": { "color": "red" } } } }"#;
        assert!(serde_json::from_str::<Config>(bad).unwrap().validate().is_err());
    }

    #[test]
    fn holding_the_visor_cycles_through_contents() {
        let json = r#"{ "screen": {
            "content": { "type": "clock" },
            "hold": { "type": "cycle", "contents": [ { "type": "stats" }, { "type": "now_playing" } ] }
        } }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        let screen = config.default_screen();
        let names: Vec<&str> = (0..4).map(|step| screen.showing(step).name()).collect();
        assert_eq!(names, ["Relógio", "Uso do PC", "Agora tocando", "Relógio"]);
        let action = r#"{ "screen": { "content": { "type": "clock" },
            "hold": { "type": "action", "action": { "type": "hotkey", "keys": "Ctrl+Nada" } } } }"#;
        assert!(serde_json::from_str::<Config>(action).unwrap().validate().is_err(), "a hold action is validated too");
    }

    #[test]
    fn open_actions_know_which_process_to_watch() {
        let open = |target: &str| Action::Open { target: target.into(), args: None, watch: true };
        assert_eq!(open("spotify.exe").watched_process().as_deref(), Some("spotify.exe"));
        assert_eq!(open(r#""C:\Program Files\Discord\Discord.exe""#).watched_process().as_deref(), Some("discord.exe"));
        assert_eq!(open("notepad").watched_process().as_deref(), Some("notepad.exe"));
        assert_eq!(open("wt.exe").watched_process().as_deref(), Some("windowsterminal.exe"));
        for not_an_app in ["https://youtube.com", "spotify:", "explorer.exe", r"C:\Users\eu\relatorio.pdf", "C:/pasta/doc.txt"] {
            assert_eq!(open(not_an_app).watched_process(), None, "{not_an_app}");
        }
        let off = Action::Open { target: "spotify.exe".into(), args: None, watch: false };
        assert_eq!(off.watched_process(), None);
        let json: Action = serde_json::from_str(r#"{ "type": "open", "target": "a.exe" }"#).unwrap();
        assert_eq!(json, open("a.exe"), "watching is on unless turned off");
    }

    #[test]
    fn parses_system_actions() {
        let json = r#"{ "keys": {
            "1": { "action": { "type": "system", "setting": "bluetooth" } },
            "2": { "action": { "type": "system", "setting": "microphone", "set": "off" } },
            "3": { "action": { "type": "system", "setting": "projection", "value": "extend" } },
            "4": { "action": { "type": "system", "setting": "sleep" } }
        } }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        let bluetooth = Action::System { setting: Setting::Bluetooth, set: Switch::Toggle, value: None };
        assert_eq!(config.keys[&1].action, Some(bluetooth.clone()));
        assert_eq!(serde_json::to_string(&bluetooth).unwrap(), r#"{"type":"system","setting":"bluetooth"}"#, "toggle is left out");
        assert_eq!(bluetooth.watched_setting(), Some((Setting::Bluetooth, None)));
        let extend = config.keys[&3].action.as_ref().unwrap().watched_setting();
        assert_eq!(extend, Some((Setting::Projection, Some("extend".into()))));
        assert_eq!(config.keys[&4].action.as_ref().unwrap().watched_setting(), None, "running once has no state");
        for bad in [
            r#"{ "keys": { "1": { "action": { "type": "system", "setting": "projection" } } } }"#,
            r#"{ "keys": { "1": { "action": { "type": "system", "setting": "power_mode", "value": "turbo" } } } }"#,
            r#"{ "keys": { "1": { "action": { "type": "system", "setting": "wifi", "value": "x" } } } }"#,
        ] {
            assert!(serde_json::from_str::<Config>(bad).unwrap().validate().is_err(), "deveria rejeitar {bad}");
        }
    }

    #[test]
    fn parses_folders() {
        let json = r#"{ "keys": { "5": { "label": "Sistema", "icon": "folder", "folder": { "keys": {
            "2": { "label": "BT", "action": { "type": "system", "setting": "bluetooth" } },
            "13": { "label": "Wi-Fi", "action": { "type": "system", "setting": "wifi" } }
        } } } } }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        let folder = config.keys[&5].folder.as_ref().unwrap();
        assert_eq!(folder.keys.keys().copied().collect::<Vec<_>>(), [2, 13]);
        assert!(!folder.stay);
        let json = serde_json::to_string(&config).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
        let bad = [
            // Key 1 is "back".
            r#"{ "keys": { "5": { "folder": { "keys": { "1": {} } } } } }"#,
            r#"{ "keys": { "5": { "folder": { "keys": { "14": {} } } } } }"#,
            r#"{ "keys": { "5": { "folder": { "keys": { "2": { "folder": {} } } } } } }"#,
            r#"{ "keys": { "5": { "folder": {}, "action": { "type": "text", "text": "x" } } } }"#,
            r#"{ "keys": { "14": { "folder": {} } } }"#,
            r#"{ "keys": { "5": { "folder": { "keys": { "2": { "icon": "naoexiste" } } } } } }"#,
        ];
        for json in bad {
            assert!(serde_json::from_str::<Config>(json).unwrap().validate().is_err(), "deveria rejeitar {json}");
        }
    }

    #[test]
    fn parses_key_colors() {
        let json = r##"{ "keys": { "1": { "icon_color": "#F0A63A", "text_color": "#FFFFFF", "border": "#E5533D" } } }"##;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        assert_eq!(config.keys[&1].border.as_deref(), Some("#E5533D"));
        let bad = r#"{ "keys": { "1": { "border": "vermelho" } } }"#;
        assert!(serde_json::from_str::<Config>(bad).unwrap().validate().is_err());
    }

    #[test]
    fn parses_screens_and_falls_back_to_window() {
        let json = r##"{
            "screen": { "content": { "type": "clock", "seconds": true } },
            "rules": [ { "name": "YT", "when": { "site": "youtube.com" }, "mode": "focus",
                         "screen": { "content": { "type": "timer", "minutes": 25 }, "background": "#000000" } } ]
        }"##;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        assert_eq!(config.default_screen().content, ScreenContent::Clock { hour24: true, seconds: true, date: true });
        assert_eq!(config.rules[0].screen.as_ref().unwrap().content, ScreenContent::Timer { minutes: 25 });
        let old: Config = serde_json::from_str(r#"{ "window": "stats" }"#).unwrap();
        assert_eq!(old.default_screen().content, ScreenContent::DeviceStats);
        let bad = r#"{ "screen": { "content": { "type": "text", "text": "oi" }, "color": "branco" } }"#;
        assert!(serde_json::from_str::<Config>(bad).unwrap().validate().is_err());
    }

    #[test]
    fn parses_site_rules() {
        let json = r#"{ "rules": [ { "name": "YT", "when": { "site": "https://www.YouTube.com/" }, "mode": "focus" } ] }"#;
        let config: Config = serde_json::from_str(json).unwrap();
        config.validate().unwrap();
        assert_eq!(config.rules[0].site().as_deref(), Some("youtube.com"));
        assert_eq!(config.rules[0].process(), None);
    }

    #[test]
    fn rejects_bad_values() {
        let bad = [
            r#"{ "keys": { "15": { "label": "x" } } }"#,
            r#"{ "brightness": 120 }"#,
            r#"{ "keys": { "1": { "color": "red" } } }"#,
            r#"{ "keys": { "1": { "icon": "naoexiste" } } }"#,
            r#"{ "keys": { "1": { "action": { "type": "hotkey", "keys": "Ctrl+Nada" } } } }"#,
            r#"{ "rules": [ { "name": "x", "when": { "process": " " }, "mode": "open" } ] }"#,
            r#"{ "rules": [ { "name": "x", "when": { "site": "https://" }, "mode": "open" } ] }"#,
            r#"{ "rules": [ { "name": "x", "when": { "process": "a.exe" }, "mode": "open", "keys": { "0": {} } } ] }"#,
        ];
        for json in bad {
            let config: Config = serde_json::from_str(json).unwrap();
            assert!(config.validate().is_err(), "deveria rejeitar {json}");
        }
    }
}
