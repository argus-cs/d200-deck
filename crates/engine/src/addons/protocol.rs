//! What the app and an addon say to each other: one JSON object per line,
//! in UTF-8, on the addon's stdin (from the app) and stdout (from the addon).
//! docs/addons.md describes it for addon authors.

use std::io::{self, BufRead, Read};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::manifest::Choice;

/// Settings of an addon key, as saved in config.json.
pub type Settings = Map<String, Value>;

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ToAddon<'a> {
    /// The first message, right after the addon starts.
    Hello { api: u32, app_version: &'a str, addon_dir: String, data_dir: String, language: &'a str },
    /// Every key of this addon that may show now (all of them, each time the list changes).
    Watch { keys: &'a [Watched] },
    /// A key went down; the addon answers with a `result` of the same id.
    Press { id: u64, context: &'a str, action: &'a str, settings: &'a Settings },
    /// The key that went down came up.
    Release { context: &'a str, action: &'a str, settings: &'a Settings },
    /// The choices of a `dynamic` list, for the editor; answered with `options`.
    Options { id: u64, action: &'a str, field: &'a str, settings: &'a Settings },
}

/// A key showing one of the addon's actions. Keys with the same action and
/// settings share a context, and so what the addon shows on them.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Watched {
    pub context: String,
    pub action: String,
    pub settings: Settings,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromAddon {
    /// How the keys of a context look now; each one replaces the last.
    State {
        context: String,
        #[serde(flatten)]
        state: KeyState,
    },
    Result {
        id: u64,
        #[serde(default)]
        error: Option<String>,
    },
    Options {
        id: u64,
        #[serde(default)]
        options: Vec<Choice>,
        #[serde(default)]
        error: Option<String>,
    },
    Log {
        #[serde(default)]
        level: Level,
        message: String,
    },
}

/// What an addon says about a key.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct KeyState {
    /// False dims the key (the game isn't installed, the service is offline).
    #[serde(default = "available")]
    pub available: bool,
    /// For actions with `state`: off shows the key's second face.
    #[serde(default)]
    pub on: Option<bool>,
    /// For `dynamic` actions: the text in place of the key's own.
    #[serde(default)]
    pub label: Option<String>,
}

fn available() -> bool {
    true
}

impl Default for KeyState {
    fn default() -> Self {
        Self { available: true, on: None, label: None }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    #[default]
    Info,
    Warn,
    Error,
}

/// The context of keys with this action and these settings: the same text
/// every time, whatever order the settings are in.
pub fn context_of(action: &str, settings: &Settings) -> String {
    let mut text = String::new();
    canonical_map(settings, &mut text);
    format!("{action}-{:016x}", fnv1a(action.as_bytes(), fnv1a(text.as_bytes(), FNV_OFFSET)))
}

/// JSON with every object's keys sorted.
fn canonical_map(map: &Settings, out: &mut String) {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    out.push('{');
    for key in keys {
        out.push_str(&Value::String(key.clone()).to_string());
        out.push(':');
        canonical(&map[key], out);
        out.push(',');
    }
    out.push('}');
}

fn canonical(value: &Value, out: &mut String) {
    match value {
        Value::Object(map) => canonical_map(map, out),
        Value::Array(items) => {
            out.push('[');
            for item in items {
                canonical(item, out);
                out.push(',');
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

/// FNV-1a: a hash that stays the same across Rust versions, unlike std's.
fn fnv1a(bytes: &[u8], start: u64) -> u64 {
    bytes.iter().fold(start, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3))
}

pub enum Line {
    Text(String),
    /// Longer than allowed: skipped up to its end.
    TooLong,
}

/// One line of at most `max` bytes, without its line break; `None` at the end.
pub fn read_line(reader: &mut impl BufRead, max: usize) -> io::Result<Option<Line>> {
    let mut buffer = Vec::new();
    if reader.by_ref().take(max as u64 + 1).read_until(b'\n', &mut buffer)? == 0 {
        return Ok(None);
    }
    if buffer.last() != Some(&b'\n') && buffer.len() > max {
        let mut rest = Vec::new();
        loop {
            rest.clear();
            let read = reader.by_ref().take(64 * 1024).read_until(b'\n', &mut rest)?;
            if read == 0 || rest.last() == Some(&b'\n') {
                return Ok(Some(Line::TooLong));
            }
        }
    }
    while matches!(buffer.last(), Some(b'\n' | b'\r')) {
        buffer.pop();
    }
    Ok(Some(Line::Text(String::from_utf8_lossy(&buffer).into_owned())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(json: &str) -> Settings {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn contexts_ignore_the_order_of_settings() {
        let a = context_of("launch", &settings(r#"{ "game": "730", "opts": { "x": 1, "y": [1, 2] } }"#));
        let b = context_of("launch", &settings(r#"{ "opts": { "y": [1, 2], "x": 1 }, "game": "730" }"#));
        assert_eq!(a, b);
        assert!(a.starts_with("launch-"));
        assert_ne!(a, context_of("launch", &settings(r#"{ "game": "570" }"#)));
        assert_ne!(a, context_of("other", &settings(r#"{ "game": "730", "opts": { "x": 1, "y": [1, 2] } }"#)));
        // Fixed across versions: addons may keep data by context.
        assert_eq!(context_of("count", &Settings::new()), context_of("count", &Settings::new()));
        assert_eq!(context_of("count", &Settings::new()).len(), "count-".len() + 16);
    }

    #[test]
    fn messages_to_addons_are_tagged_by_event() {
        let empty = Settings::new();
        let press = ToAddon::Press { id: 7, context: "c", action: "count", settings: &empty };
        assert_eq!(serde_json::to_string(&press).unwrap(), r#"{"event":"press","id":7,"context":"c","action":"count","settings":{}}"#);
        let watch = ToAddon::Watch { keys: &[Watched { context: "c".into(), action: "count".into(), settings: empty.clone() }] };
        assert_eq!(serde_json::to_string(&watch).unwrap(), r#"{"event":"watch","keys":[{"context":"c","action":"count","settings":{}}]}"#);
    }

    #[test]
    fn messages_from_addons() {
        let parse = |json: &str| serde_json::from_str::<FromAddon>(json);
        match parse(r#"{ "type": "state", "context": "c", "label": "3 toques" }"#).unwrap() {
            FromAddon::State { context, state } => {
                assert_eq!(context, "c");
                assert_eq!(state, KeyState { available: true, on: None, label: Some("3 toques".into()) });
            }
            other => panic!("{other:?}"),
        }
        match parse(r#"{ "type": "state", "context": "c", "available": false, "on": false }"#).unwrap() {
            FromAddon::State { state, .. } => assert_eq!(state, KeyState { available: false, on: Some(false), label: None }),
            other => panic!("{other:?}"),
        }
        assert!(matches!(parse(r#"{ "type": "result", "id": 3 }"#).unwrap(), FromAddon::Result { id: 3, error: None }));
        assert!(matches!(parse(r#"{ "type": "log", "message": "oi" }"#).unwrap(), FromAddon::Log { level: Level::Info, .. }));
        for bad in ["", "{}", r#"{ "type": "nada" }"#, r#"{ "type": "state" }"#, "[1]"] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn long_lines_are_skipped_whole() {
        let text = format!("curta\r\n{}\nde novo\nsem fim", "x".repeat(100));
        let mut reader = io::Cursor::new(text.into_bytes());
        let mut lines = Vec::new();
        while let Some(line) = read_line(&mut reader, 20).unwrap() {
            lines.push(match line {
                Line::Text(text) => text,
                Line::TooLong => "(longa)".into(),
            });
        }
        assert_eq!(lines, ["curta", "(longa)", "de novo", "sem fim"]);
    }
}
