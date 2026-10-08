use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::actions::parse_hotkey;
use crate::icons;

/// Keys are numbered like on the device: 1 to 13, and 14 for the status window.
pub const KEY_NUMBERS: std::ops::RangeInclusive<u8> = 1..=14;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub brightness: u8,
    pub window: Window,
    pub label: LabelStyle,
    pub keys: BTreeMap<u8, Key>,
    /// Layers over `keys`; see `rules::active_rules` for the order.
    pub rules: Vec<Rule>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Window {
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
    #[serde(default = "default_key_color")]
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<Action>,
    /// On a rule's key: bring the rule's app to the front for the action,
    /// then give the focus back. Shortcuts only reach the window in front.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub front: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Action {
    Hotkey { keys: String },
    Open {
        target: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        args: Option<String>,
    },
    Command { command: String },
    Text { text: String },
    Media { key: MediaKey },
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
        let open = |target: &str| Action::Open { target: target.into(), args: None };
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
                let key = Key { label: label.into(), icon: Some(icon.into()), color: default_key_color(), action: Some(action), front: false };
                (n, key)
            })
            .collect();
        Self { brightness: 80, window: Window::Clock, label: LabelStyle::default(), keys, rules: Vec::new() }
    }
}

impl Config {
    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("D200Deck").join("config.json")
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            return Self::load(path);
        }
        let config = Self::default();
        config.save(path)?;
        log::info!("config criada com o layout padrão em {}", path.display());
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("não consegui ler {}", path.display()))?;
        let config: Self = serde_json::from_str(&text).with_context(|| format!("JSON inválido em {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
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
        }
        Ok(())
    }
}

fn validate_key(n: u8, key: &Key) -> Result<()> {
    if !KEY_NUMBERS.contains(&n) {
        bail!("tecla {n} não existe (use 1 a 14)");
    }
    if !is_hex_color(&key.color, true) {
        bail!("tecla {n}: color precisa ser #RRGGBB (está {:?})", key.color);
    }
    if let Some(icon) = &key.icon {
        if icon.is_empty() || (!icons::is_glyph(icon) && !icon.to_ascii_lowercase().ends_with(".png")) {
            bail!("tecla {n}: ícone {icon:?} não é um ícone embutido nem um arquivo .png");
        }
    }
    if let Some(Action::Hotkey { keys }) = &key.action {
        parse_hotkey(keys).with_context(|| format!("tecla {n}"))?;
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

    #[test]
    fn default_config_is_valid_and_round_trips() {
        let config = Config::default();
        config.validate().unwrap();
        let json = serde_json::to_string_pretty(&config).unwrap();
        assert_eq!(serde_json::from_str::<Config>(&json).unwrap(), config);
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
