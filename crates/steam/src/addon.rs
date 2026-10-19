//! The addon itself: answers D200 Deck over stdin and stdout, as any addon
//! does (docs/addons.md).

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::library;

/// How often the keys' games are looked at: running, still installed.
const POLL: Duration = Duration::from_secs(2);
/// How long the list of games may spend making covers before it answers
/// (the app waits 5 s); the rest come the next time it is asked.
const COVER_BUDGET: Duration = Duration::from_millis(2500);

/// Steam's pages the "open" action goes to.
const PAGES: &[(&str, &str)] = &[
    ("games", "steam://open/games"),
    ("friends", "steam://open/friends"),
    ("downloads", "steam://open/downloads"),
    ("store", "steam://store"),
    ("settings", "steam://open/settings"),
];

/// What the addon needs from Windows and Steam, set apart so tests can
/// stand in for them.
pub trait System {
    fn steam_dir(&self) -> Option<PathBuf>;
    fn running(&self, game: &str) -> bool;
    fn open(&self, url: &str) -> anyhow::Result<()>;
    /// Brings forward a window of a program in `folder`; false when none shows.
    fn bring_to_front(&self, folder: &Path) -> bool;
}

/// The app's messages this addon uses; the rest are let be.
#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Event {
    Hello {
        data_dir: PathBuf,
    },
    Watch {
        keys: Vec<Watched>,
    },
    Press {
        id: u64,
        action: String,
        #[serde(default)]
        settings: Map<String, Value>,
    },
    Options {
        id: u64,
        action: String,
        field: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Watched {
    context: String,
    action: String,
    #[serde(default)]
    settings: Map<String, Value>,
}

pub struct Addon<S, W> {
    system: S,
    out: W,
    /// Where the covers made for keys go (in the addon's data folder).
    covers: Option<PathBuf>,
    keys: Vec<Watched>,
    /// What each context was last told, to send only changes.
    sent: HashMap<String, Value>,
}

impl<S: System, W: Write> Addon<S, W> {
    pub fn new(system: S, out: W) -> Self {
        Self { system, out, covers: None, keys: Vec::new(), sent: HashMap::new() }
    }

    pub fn handle(&mut self, line: &str) {
        let event = match serde_json::from_str::<Event>(line) {
            Ok(event) => event,
            Err(e) => {
                eprintln!("mensagem ignorada ({e})");
                return;
            }
        };
        match event {
            Event::Hello { data_dir } => self.covers = Some(data_dir.join("covers")),
            Event::Watch { keys } => {
                self.keys = keys;
                self.sent.retain(|context, _| self.keys.iter().any(|key| &key.context == context));
                self.poll();
            }
            Event::Press { id, action, settings } => {
                let answer = match self.press(&action, &settings) {
                    Ok(()) => json!({ "type": "result", "id": id }),
                    Err(error) => json!({ "type": "result", "id": id, "error": error }),
                };
                self.send(&answer);
                self.poll();
            }
            Event::Options { id, action, field } => {
                let answer = match (action.as_str(), field.as_str()) {
                    ("launch", "game") => match self.system.steam_dir() {
                        Some(steam) => json!({ "type": "options", "id": id, "options": self.games(&steam) }),
                        None => json!({ "type": "options", "id": id, "error": "a Steam não está instalada neste PC" }),
                    },
                    _ => json!({ "type": "options", "id": id, "error": format!("o campo {field} não tem lista") }),
                };
                self.send(&answer);
            }
            Event::Other => {}
        }
    }

    /// Tells the app how each key looks now, when that changed.
    pub fn poll(&mut self) {
        let steam = self.system.steam_dir();
        let installed: Vec<String> = match (&steam, self.keys.iter().any(|key| key.action == "launch")) {
            (Some(steam), true) => library::games(steam).into_iter().map(|game| game.id).collect(),
            _ => Vec::new(),
        };
        let states: Vec<(String, Value)> = self
            .keys
            .iter()
            .map(|key| {
                let state = match key.action.as_str() {
                    "launch" => {
                        let game = key.settings.get("game").and_then(Value::as_str).filter(|game| installed.iter().any(|id| id == game));
                        // Lit while it runs; dimmed when it isn't installed (or none is chosen).
                        json!({ "available": game.is_some(), "on": game.is_some_and(|game| self.system.running(game)) })
                    }
                    _ => json!({ "available": steam.is_some() }),
                };
                (key.context.clone(), state)
            })
            .collect();
        for (context, state) in states {
            if self.sent.get(&context) != Some(&state) {
                let mut message = json!({ "type": "state", "context": context });
                message.as_object_mut().unwrap().extend(state.as_object().unwrap().clone());
                self.send(&message);
                self.sent.insert(context, state);
            }
        }
    }

    fn press(&self, action: &str, settings: &Map<String, Value>) -> Result<(), String> {
        let text = |field: &str| settings.get(field).and_then(Value::as_str);
        let steam = self.system.steam_dir().ok_or("a Steam não está instalada neste PC")?;
        match action {
            "launch" => {
                let game = text("game").ok_or("escolha o jogo no editor")?;
                if self.system.running(game) {
                    if text("when_running") == Some("nothing") {
                        return Ok(());
                    }
                    let folder = library::games(&steam).into_iter().find(|g| g.id == game).map(|g| g.folder);
                    if folder.is_some_and(|folder| self.system.bring_to_front(&folder)) {
                        return Ok(());
                    }
                    // No window of it shows (still loading, or a launcher): Steam brings it up.
                }
                self.system.open(&format!("steam://rungameid/{game}")).map_err(|e| format!("{e:#}"))
            }
            "open" => {
                let page = text("page").unwrap_or("games");
                let url = PAGES.iter().find(|(id, _)| *id == page).map(|(_, url)| *url).ok_or(format!("página desconhecida: {page}"))?;
                self.system.open(url).map_err(|e| format!("{e:#}"))
            }
            other => Err(format!("ação desconhecida: {other}")),
        }
    }

    /// The installed games for the editor's list, each suggesting its short
    /// name and its cover for the key.
    fn games(&self, steam: &Path) -> Vec<Value> {
        let started = Instant::now();
        library::games(steam)
            .into_iter()
            .map(|game| {
                let cover = match &self.covers {
                    Some(dir) if started.elapsed() < COVER_BUDGET => library::cover(steam, &game.id, dir).ok(),
                    _ => None,
                };
                let mut choice = json!({ "value": game.id, "label": game.name, "short": library::short_name(&game.name) });
                if let Some(cover) = cover {
                    choice["icon"] = json!(cover.display().to_string());
                }
                choice
            })
            .collect()
    }

    fn send(&mut self, message: &Value) {
        // A closed stdout means the app is gone; reading stdin ends the addon then.
        let _ = writeln!(self.out, "{message}").and_then(|()| self.out.flush());
    }
}

/// Windows and the real Steam.
pub struct Windows;

impl System for Windows {
    fn steam_dir(&self) -> Option<PathBuf> {
        library::steam_dir()
    }

    fn running(&self, game: &str) -> bool {
        library::running(game)
    }

    fn open(&self, url: &str) -> anyhow::Result<()> {
        deck_engine::actions::execute(&deck_engine::config::Action::Open { target: url.into(), args: None, watch: false })
    }

    #[cfg(windows)]
    fn bring_to_front(&self, folder: &Path) -> bool {
        let inside = format!("{}\\", folder.display().to_string().to_lowercase().trim_end_matches('\\'));
        let window = deck_engine::context::visible_apps().into_iter().find(|app| app.path.to_lowercase().starts_with(&inside));
        window.is_some_and(|app| deck_engine::context::bring_to_front(&app.exe.to_lowercase()).is_ok())
    }

    #[cfg(not(windows))]
    fn bring_to_front(&self, _folder: &Path) -> bool {
        false
    }
}

/// Runs the addon on this process's stdin and stdout until the app closes
/// stdin. Returns the exit code.
pub fn run() -> i32 {
    let (lines, incoming) = channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let mut addon = Addon::new(Windows, std::io::stdout());
    let mut last_poll = Instant::now();
    loop {
        match incoming.recv_timeout(POLL.saturating_sub(last_poll.elapsed())) {
            Ok(line) => addon.handle(&line),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return 0,
        }
        if last_poll.elapsed() >= POLL {
            addon.poll();
            last_poll = Instant::now();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashSet;

    use super::*;
    use crate::library::tests::fake_steam;

    #[derive(Default)]
    struct Fake {
        steam: Option<PathBuf>,
        running: RefCell<HashSet<String>>,
        opened: RefCell<Vec<String>>,
        /// Whether the game shows a window to bring forward.
        window: bool,
        brought: RefCell<Vec<PathBuf>>,
    }

    impl System for &Fake {
        fn steam_dir(&self) -> Option<PathBuf> {
            self.steam.clone()
        }
        fn running(&self, game: &str) -> bool {
            self.running.borrow().contains(game)
        }
        fn open(&self, url: &str) -> anyhow::Result<()> {
            self.opened.borrow_mut().push(url.into());
            Ok(())
        }
        fn bring_to_front(&self, folder: &Path) -> bool {
            self.brought.borrow_mut().push(folder.into());
            self.window
        }
    }

    /// What the addon wrote since the last call.
    fn said(addon: &mut Addon<&Fake, Vec<u8>>) -> Vec<Value> {
        let text = String::from_utf8(std::mem::take(&mut addon.out)).unwrap();
        text.lines().map(|line| serde_json::from_str(line).unwrap()).collect()
    }

    const KEYS: &str = r#"{ "event": "watch", "keys": [
        { "context": "pico", "action": "launch", "settings": { "game": "1509960" } },
        { "context": "gone", "action": "launch", "settings": { "game": "999" } },
        { "context": "none", "action": "launch", "settings": {} },
        { "context": "friends", "action": "open", "settings": { "page": "friends" } }
    ] }"#;

    fn hello(dir: &Path) -> String {
        json!({ "event": "hello", "api": 1, "app_version": "0.4.0", "addon_dir": "x", "data_dir": dir, "language": "pt-BR" }).to_string()
    }

    #[test]
    fn keys_show_installed_and_running_games() {
        let steam = fake_steam("addon-keys");
        let fake = Fake { steam: Some(steam.clone()), ..Fake::default() };
        let mut addon = Addon::new(&fake, Vec::new());
        addon.handle(&hello(&steam.join("dados")));
        addon.handle(KEYS);
        let states: HashMap<String, Value> = said(&mut addon).into_iter().map(|m| (m["context"].as_str().unwrap().to_string(), m)).collect();
        assert_eq!(states["pico"]["available"], true);
        assert_eq!(states["pico"]["on"], false);
        assert_eq!(states["gone"]["available"], false, "not installed: dimmed");
        assert_eq!(states["none"]["available"], false, "no game chosen");
        assert_eq!(states["friends"], json!({ "type": "state", "context": "friends", "available": true }));

        // The game starts: only its key changes.
        fake.running.borrow_mut().insert("1509960".into());
        addon.poll();
        let changed = said(&mut addon);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0]["context"], "pico");
        assert_eq!(changed[0]["on"], true);
        addon.poll();
        assert!(said(&mut addon).is_empty(), "nothing new, nothing sent");
    }

    #[test]
    fn presses_open_games_and_pages() {
        let steam = fake_steam("addon-press");
        let fake = Fake { steam: Some(steam.clone()), ..Fake::default() };
        let mut addon = Addon::new(&fake, Vec::new());
        let press = |id: u64, action: &str, settings: Value| json!({ "event": "press", "id": id, "context": "c", "action": action, "settings": settings }).to_string();

        addon.handle(&press(1, "launch", json!({ "game": "1509960" })));
        assert_eq!(said(&mut addon), [json!({ "type": "result", "id": 1 })]);
        assert_eq!(fake.opened.borrow().last().unwrap(), "steam://rungameid/1509960");

        // Running: its window comes forward instead, unless it has none to show.
        fake.running.borrow_mut().insert("1509960".into());
        addon.handle(&press(2, "launch", json!({ "game": "1509960" })));
        assert_eq!(fake.brought.borrow().last().unwrap(), &steam.join("steamapps").join("common").join("PICO PARK"));
        assert_eq!(fake.opened.borrow().len(), 2, "no window: Steam brings it up");
        addon.handle(&press(3, "launch", json!({ "game": "1509960", "when_running": "nothing" })));
        assert_eq!(fake.opened.borrow().len(), 2);

        addon.handle(&press(4, "open", json!({ "page": "friends" })));
        assert_eq!(fake.opened.borrow().last().unwrap(), "steam://open/friends");
        said(&mut addon);
        addon.handle(&press(5, "launch", json!({})));
        assert_eq!(said(&mut addon)[0]["error"], "escolha o jogo no editor");
        addon.handle(&press(6, "open", json!({ "page": "nada" })));
        assert_eq!(said(&mut addon)[0]["error"], "página desconhecida: nada");
    }

    #[test]
    fn a_running_game_with_a_window_comes_forward() {
        let steam = fake_steam("addon-front");
        let fake = Fake { steam: Some(steam), window: true, ..Fake::default() };
        fake.running.borrow_mut().insert("1509960".into());
        let mut addon = Addon::new(&fake, Vec::new());
        addon.handle(r#"{ "event": "press", "id": 1, "context": "c", "action": "launch", "settings": { "game": "1509960" } }"#);
        assert!(fake.opened.borrow().is_empty());
        assert_eq!(fake.brought.borrow().len(), 1);
    }

    #[test]
    fn the_list_of_games_suggests_names_and_covers() {
        let steam = fake_steam("addon-options");
        let art = steam.join("appcache").join("librarycache").join("1509960");
        std::fs::create_dir_all(&art).unwrap();
        image::RgbImage::from_pixel(300, 450, image::Rgb([10, 120, 200])).save(art.join("library_600x900.jpg")).unwrap();
        let fake = Fake { steam: Some(steam.clone()), ..Fake::default() };
        let mut addon = Addon::new(&fake, Vec::new());
        addon.handle(&hello(&steam.join("dados")));
        addon.handle(r#"{ "event": "options", "id": 9, "action": "launch", "field": "game", "settings": {} }"#);
        let answer = &said(&mut addon)[0];
        assert_eq!(answer["id"], 9);
        let options = answer["options"].as_array().unwrap();
        assert_eq!(options.len(), 2);
        assert_eq!(options[0]["short"], "Hades");
        assert!(options[0].get("icon").is_none(), "no art for it");
        assert_eq!(options[1]["value"], "1509960");
        let cover = PathBuf::from(options[1]["icon"].as_str().unwrap());
        assert!(cover.starts_with(steam.join("dados")) && cover.is_file());

        addon.handle(r#"{ "event": "options", "id": 10, "action": "open", "field": "page", "settings": {} }"#);
        assert!(said(&mut addon)[0]["error"].is_string());
    }

    #[test]
    fn without_steam_keys_dim_and_presses_say_why() {
        let fake = Fake::default();
        let mut addon = Addon::new(&fake, Vec::new());
        addon.handle(KEYS);
        assert!(said(&mut addon).iter().all(|state| state["available"] == false));
        addon.handle(r#"{ "event": "press", "id": 1, "context": "c", "action": "open", "settings": {} }"#);
        assert_eq!(said(&mut addon)[0]["error"], "a Steam não está instalada neste PC");
        addon.handle(r#"{ "event": "options", "id": 2, "action": "launch", "field": "game", "settings": {} }"#);
        assert!(said(&mut addon)[0]["error"].as_str().unwrap().contains("Steam"));
    }

    #[test]
    fn other_messages_are_let_be() {
        let fake = Fake::default();
        let mut addon = Addon::new(&fake, Vec::new());
        for line in ["", "nada", r#"{ "event": "release", "context": "c", "action": "launch", "settings": {} }"#, r#"{ "event": "futuro", "x": 1 }"#] {
            addon.handle(line);
        }
        assert!(said(&mut addon).is_empty());
    }
}
