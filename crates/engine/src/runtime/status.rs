//! What the engine reports to the app: the `Status` it publishes when
//! something it shows changes, and the `Activity` of each key press.

use super::keys::{cached_icon, face};
use super::*;

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

impl State {
    pub(super) fn publish(&mut self) {
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
                let default_text = icons::default_text_color(&self.config.label);
                let image = slot
                    .zip(current.as_ref())
                    .and_then(|(s, f)| {
                        let look = icons::Look::new(&f.appearance, &self.config.label, &default_text, self.live.dim(&s.key, f));
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
                    label: current.as_ref().map(|f| f.appearance.label.to_string()).unwrap_or_default(),
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

pub(super) fn mode_name(mode: RuleMode) -> &'static str {
    match mode {
        RuleMode::Open => "aberto",
        RuleMode::Focus => "foco",
    }
}

