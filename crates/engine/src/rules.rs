//! Decides which rules apply and what each key shows. Pure logic: the
//! Windows side only fills in a `Context`.

use std::collections::{BTreeMap, HashSet};

use crate::config::{Config, Key, Rule, RuleMode};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Context {
    /// Lowercase exe name of the process that owns the window in front.
    pub focused: Option<String>,
    /// Lowercase exe names of running processes that "open" rules watch.
    pub running: HashSet<String>,
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
    let process = rule.process();
    let focused = ctx.focused.as_deref() == Some(process.as_str());
    match rule.mode {
        RuleMode::Focus => focused,
        RuleMode::Open => focused || ctx.running.contains(&process),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::When;

    fn key(label: &str) -> Key {
        Key { label: label.into(), icon: None, color: "#24262B".into(), action: None, front: false }
    }

    fn rule(name: &str, process: &str, mode: RuleMode, keys: &[(u8, &str)]) -> Rule {
        Rule {
            name: name.into(),
            when: When::Process(process.into()),
            mode,
            enabled: true,
            keys: keys.iter().map(|(n, l)| (*n, key(l))).collect(),
        }
    }

    fn config() -> Config {
        Config {
            keys: [(1, key("mic")), (2, key("play")), (3, key("next"))].into_iter().collect(),
            rules: vec![
                rule("Discord", "Discord.exe", RuleMode::Open, &[(1, "discord mic")]),
                rule("Photoshop", "Photoshop", RuleMode::Focus, &[(2, "pincel")]),
                rule("Meet", "chrome.exe", RuleMode::Open, &[(1, "meet mic"), (3, "câmera")]),
                rule("Zoom", "zoom.exe", RuleMode::Focus, &[(3, "zoom")]),
            ],
            ..Config::default()
        }
    }

    fn ctx(focused: Option<&str>, running: &[&str]) -> Context {
        Context { focused: focused.map(Into::into), running: running.iter().map(|s| s.to_string()).collect() }
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
        let c = ctx(Some("zoom.exe"), &["discord.exe", "chrome.exe"]);
        assert_eq!(active_rules(&config(), &c), [0, 2, 3]);
        let slots = resolve(&config(), &active_rules(&config(), &c));
        assert_eq!(slots[&1].key.label, "meet mic");
        assert_eq!(slots[&1].rule, Some(2));
        assert_eq!(slots[&3].key.label, "zoom");
        assert_eq!(slots[&2].rule, None);
    }

    #[test]
    fn disabled_rules_are_ignored() {
        let mut config = config();
        config.rules[0].enabled = false;
        assert_eq!(labels(&config, &ctx(None, &["discord.exe"])), ["mic", "play", "next"]);
    }
}
