//! Decides which rules apply and what each key shows. Pure logic: the
//! Windows side and the Edge extension only fill in a `Context`.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::config::{Config, Key, Rule, RuleMode};

/// The browser whose tabs the extension reports.
pub const BROWSER: &str = "msedge.exe";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// Lowercase exe name of the process that owns the window in front.
    pub focused: Option<String>,
    /// Lowercase exe names of running processes that "open" rules watch.
    pub running: HashSet<String>,
    /// Edge's tabs, as the extension last reported them.
    pub tabs: Tabs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Tabs {
    /// The selected tab of the Edge window used last.
    pub active: Option<Tab>,
    pub open: Vec<Tab>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Tab {
    pub id: i64,
    pub url: String,
}

/// A pretend context to try rules without opening the apps: one rule's app
/// or site in front, some others open. Indices into `Config::rules`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Simulation {
    pub focus: Option<usize>,
    pub open: Vec<usize>,
}

pub fn simulated_context(config: &Config, simulation: &Simulation) -> Context {
    let mut ctx = Context::default();
    let mut next_id = -1;
    let mut fake_tab = |site: &str| {
        // Wildcards become a plain segment so the pattern still matches.
        let tab = Tab { id: next_id, url: format!("https://{}/", site.replace('*', "x")) };
        next_id -= 1;
        tab
    };
    for rule in simulation.open.iter().filter_map(|&i| config.rules.get(i)) {
        if let Some(process) = rule.process() {
            ctx.running.insert(process);
        } else if let Some(site) = rule.site() {
            ctx.tabs.open.push(fake_tab(&site));
        }
    }
    if let Some(rule) = simulation.focus.and_then(|i| config.rules.get(i)) {
        if let Some(process) = rule.process() {
            ctx.focused = Some(process);
        } else if let Some(site) = rule.site() {
            let tab = fake_tab(&site);
            ctx.focused = Some(BROWSER.into());
            ctx.tabs.active = Some(tab.clone());
            ctx.tabs.open.push(tab);
        }
    }
    ctx
}

/// "https://www.youtube.com/watch?v=1" → "youtube.com".
pub fn host_of(url: &str) -> Option<String> {
    host_and_path(url).map(|t| t.split('/').next().unwrap_or_default().to_string())
}

/// A key as the device should show it, and the rule it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Slot {
    pub key: Key,
    pub rule: Option<usize>,
}

pub fn is_active(rule: &Rule, ctx: &Context) -> bool {
    if !rule.enabled {
        return false;
    }
    let (in_front, open) = if let Some(process) = rule.process() {
        let in_front = ctx.focused.as_deref() == Some(process.as_str());
        (in_front, ctx.running.contains(&process))
    } else if let Some(site) = rule.site() {
        let edge_in_front = ctx.focused.as_deref() == Some(BROWSER);
        let selected = ctx.tabs.active.as_ref().is_some_and(|t| site_matches(&site, &t.url));
        (edge_in_front && selected, ctx.tabs.open.iter().any(|t| site_matches(&site, &t.url)))
    } else {
        (false, false)
    };
    match rule.mode {
        RuleMode::Focus => in_front,
        RuleMode::Open => in_front || open,
    }
}

/// Active rule indices in layering order: "open" rules, then "focus" rules,
/// each in list order. Later layers win, so focus beats open and, within a
/// mode, the rule further down the list wins.
pub fn active_rules(config: &Config, ctx: &Context) -> Vec<usize> {
    let in_mode = |mode: RuleMode| {
        config.rules.iter().enumerate().filter(move |(_, r)| r.mode == mode && is_active(r, ctx)).map(|(i, _)| i)
    };
    in_mode(RuleMode::Open).chain(in_mode(RuleMode::Focus)).collect()
}

pub fn resolve(config: &Config, active: &[usize]) -> BTreeMap<u8, Slot> {
    let mut slots: BTreeMap<u8, Slot> =
        config.keys.iter().map(|(n, key)| (*n, Slot { key: key.clone(), rule: None })).collect();
    for &i in active {
        for (n, key) in &config.rules[i].keys {
            slots.insert(*n, Slot { key: key.clone(), rule: Some(i) });
        }
    }
    slots
}

/// The tab a site rule's key should act on: the selected one if it
/// matches, else the first open match.
pub fn matching_tab(rule: &Rule, ctx: &Context) -> Option<i64> {
    let site = rule.site()?;
    let active = ctx.tabs.active.as_ref().filter(|t| site_matches(&site, &t.url));
    active.or_else(|| ctx.tabs.open.iter().find(|t| site_matches(&site, &t.url))).map(|t| t.id)
}

/// `site` is normalized (see `config::normalize_site`). A plain domain
/// matches itself and its subdomains; with `/` or `*` it is a wildcard
/// pattern over "domain/path", also matching anything below it.
pub fn site_matches(site: &str, url: &str) -> bool {
    let Some(target) = host_and_path(url) else {
        return false;
    };
    if site.contains(['/', '*']) {
        return wildcard(site, &target) || wildcard(&format!("{site}/*"), &target);
    }
    let host = target.split('/').next().unwrap_or_default();
    host == site || host.strip_suffix(site).is_some_and(|rest| rest.ends_with('.'))
}

/// "https://user@www.YouTube.com:443/Watch?v=1#t" → "youtube.com/watch".
fn host_and_path(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let rest = &rest[..rest.find(['?', '#']).unwrap_or(rest.len())];
    let (authority, path) = rest.split_at(rest.find('/').unwrap_or(rest.len()));
    let host = authority.rsplit('@').next()?.split(':').next()?.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    Some(format!("{host}{}", path.to_ascii_lowercase()))
}

/// `*` matches any run of characters, including `/`.
fn wildcard(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            backtrack = Some((pi, ti));
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some((star, matched)) = backtrack {
            pi = star + 1;
            ti = matched + 1;
            backtrack = Some((star, matched + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::When;

    fn key(label: &str) -> Key {
        Key { label: label.into(), icon: None, color: "#24262B".into(), action: None, front: false }
    }

    fn rule(name: &str, when: When, mode: RuleMode, keys: &[(u8, &str)]) -> Rule {
        Rule { name: name.into(), when, mode, enabled: true, keys: keys.iter().map(|(n, l)| (*n, key(l))).collect() }
    }

    fn process(name: &str) -> When {
        When::Process(name.into())
    }

    fn site(name: &str) -> When {
        When::Site(name.into())
    }

    fn config() -> Config {
        Config {
            keys: [(1, key("mic")), (2, key("play")), (3, key("next"))].into_iter().collect(),
            rules: vec![
                rule("Discord", process("Discord.exe"), RuleMode::Open, &[(1, "discord mic")]),
                rule("Photoshop", process("Photoshop"), RuleMode::Focus, &[(2, "pincel")]),
                rule("Meet", site("meet.google.com"), RuleMode::Open, &[(1, "meet mic"), (3, "câmera")]),
                rule("YouTube", site("youtube.com"), RuleMode::Focus, &[(3, "legendas")]),
            ],
            ..Config::default()
        }
    }

    fn tab(id: i64, url: &str) -> Tab {
        Tab { id, url: url.into() }
    }

    fn ctx(focused: Option<&str>, running: &[&str]) -> Context {
        Context {
            focused: focused.map(Into::into),
            running: running.iter().map(|s| s.to_string()).collect(),
            tabs: Tabs::default(),
        }
    }

    fn with_tabs(mut ctx: Context, active: Option<Tab>, open: &[Tab]) -> Context {
        ctx.tabs = Tabs { active, open: open.to_vec() };
        ctx
    }

    fn labels(config: &Config, ctx: &Context) -> Vec<String> {
        let slots = resolve(config, &active_rules(config, ctx));
        [1, 2, 3].iter().map(|n| slots[n].key.label.clone()).collect()
    }

    #[test]
    fn no_context_means_default_layout() {
        assert!(active_rules(&config(), &Context::default()).is_empty());
        assert_eq!(labels(&config(), &Context::default()), ["mic", "play", "next"]);
    }

    #[test]
    fn open_rule_applies_while_running_even_in_background() {
        let c = ctx(Some("explorer.exe"), &["discord.exe"]);
        assert_eq!(labels(&config(), &c), ["discord mic", "play", "next"]);
    }

    #[test]
    fn focus_rule_needs_the_window_in_front() {
        assert_eq!(labels(&config(), &ctx(None, &["photoshop.exe"])), ["mic", "play", "next"]);
        assert_eq!(labels(&config(), &ctx(Some("photoshop.exe"), &[])), ["mic", "pincel", "next"]);
    }

    #[test]
    fn focused_app_counts_as_open() {
        assert_eq!(labels(&config(), &ctx(Some("discord.exe"), &[])), ["discord mic", "play", "next"]);
    }

    #[test]
    fn later_open_rule_wins_and_focus_beats_open() {
        let yt = tab(1, "https://www.youtube.com/watch?v=x");
        let meet = tab(2, "https://meet.google.com/abc");
        let c = with_tabs(ctx(Some("msedge.exe"), &["discord.exe"]), Some(yt.clone()), &[yt, meet]);
        assert_eq!(active_rules(&config(), &c), [0, 2, 3]);
        let slots = resolve(&config(), &active_rules(&config(), &c));
        assert_eq!(slots[&1].key.label, "meet mic");
        assert_eq!(slots[&1].rule, Some(2));
        assert_eq!(slots[&3].key.label, "legendas");
        assert_eq!(slots[&2].rule, None);
    }

    #[test]
    fn disabled_rules_are_ignored() {
        let mut config = config();
        config.rules[0].enabled = false;
        assert_eq!(labels(&config, &ctx(None, &["discord.exe"])), ["mic", "play", "next"]);
    }

    #[test]
    fn site_focus_rule_needs_edge_in_front_and_the_tab_selected() {
        let yt = tab(1, "https://youtube.com/");
        let other = tab(2, "https://example.com/");
        let selected = with_tabs(ctx(Some("msedge.exe"), &[]), Some(yt.clone()), &[yt.clone(), other.clone()]);
        assert_eq!(labels(&config(), &selected)[2], "legendas");
        let edge_behind = with_tabs(ctx(Some("explorer.exe"), &[]), Some(yt.clone()), &[yt.clone()]);
        assert_eq!(labels(&config(), &edge_behind)[2], "next");
        let other_tab = with_tabs(ctx(Some("msedge.exe"), &[]), Some(other.clone()), &[yt, other]);
        assert_eq!(labels(&config(), &other_tab)[2], "next");
    }

    #[test]
    fn site_open_rule_follows_any_open_tab() {
        let meet = tab(7, "https://meet.google.com/xyz");
        let c = with_tabs(ctx(Some("code.exe"), &[]), None, &[meet]);
        assert_eq!(labels(&config(), &c), ["meet mic", "play", "câmera"]);
        assert_eq!(matching_tab(&config().rules[2], &c), Some(7));
    }

    #[test]
    fn matching_tab_prefers_the_selected_one() {
        let (a, b) = (tab(1, "https://meet.google.com/a"), tab(2, "https://meet.google.com/b"));
        let c = with_tabs(ctx(None, &[]), Some(b.clone()), &[a, b]);
        assert_eq!(matching_tab(&config().rules[2], &c), Some(2));
    }

    #[test]
    fn simulation_activates_the_chosen_rules() {
        let config = config();
        let sim = Simulation { focus: Some(3), open: vec![0, 2] };
        let ctx = simulated_context(&config, &sim);
        assert_eq!(active_rules(&config, &ctx), [0, 2, 3]);
        let photoshop = simulated_context(&config, &Simulation { focus: Some(1), open: vec![] });
        assert_eq!(active_rules(&config, &photoshop), [1]);
        assert!(active_rules(&config, &simulated_context(&config, &Simulation::default())).is_empty());
        let wildcard = Config {
            rules: vec![rule("PRs", site("github.com/*/pulls"), RuleMode::Focus, &[])],
            ..Config::default()
        };
        let ctx = simulated_context(&wildcard, &Simulation { focus: Some(0), open: vec![] });
        assert_eq!(active_rules(&wildcard, &ctx), [0]);
    }

    #[test]
    fn host_of_strips_www_and_path() {
        assert_eq!(host_of("https://www.youtube.com/watch?v=1").as_deref(), Some("youtube.com"));
        assert_eq!(host_of("nothing"), None);
    }

    #[test]
    fn domains_match_subdomains_only() {
        assert!(site_matches("youtube.com", "https://www.youtube.com/watch?v=1"));
        assert!(site_matches("youtube.com", "https://m.youtube.com"));
        assert!(site_matches("youtube.com", "http://user@YouTube.com:8080/x"));
        assert!(!site_matches("youtube.com", "https://notyoutube.com/"));
        assert!(!site_matches("youtube.com", "https://youtube.com.evil.io/"));
        assert!(!site_matches("youtube.com", "edge://settings"));
        assert!(!site_matches("youtube.com", "not a url"));
    }

    #[test]
    fn wildcards_match_domain_and_path() {
        assert!(site_matches("github.com/*/pulls", "https://github.com/rust-lang/rust/pulls"));
        assert!(site_matches("github.com/*/pulls", "https://github.com/rust-lang/rust/pulls/42?x=1"));
        assert!(!site_matches("github.com/*/pulls", "https://github.com/rust-lang/rust/issues"));
        assert!(site_matches("docs.google.com/spreadsheets", "https://docs.google.com/spreadsheets/d/1/edit"));
        assert!(!site_matches("docs.google.com/spreadsheets", "https://docs.google.com/document/d/1"));
    }
}
