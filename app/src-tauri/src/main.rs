// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use deck_engine::config::{Action, Config, Key};
use deck_engine::rules::Simulation;
use deck_engine::runtime::{self, Command, Engine, Status};
use deck_engine::{context, icons};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_dialog::DialogExt;

/// Passed by the "start with Windows" entry: start in the tray, no window.
const HIDDEN_FLAG: &str = "--hidden";
const MAIN_WINDOW: &str = "main";

#[derive(Default)]
pub struct DeckState {
    engine: OnceLock<Engine>,
    status: Mutex<Status>,
}

impl DeckState {
    pub fn send(&self, command: Command) {
        if let Some(engine) = self.engine.get() {
            engine.send(command);
        }
    }
}

#[tauri::command]
fn get_status(state: State<DeckState>) -> Status {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
fn set_paused(state: State<DeckState>, paused: bool) {
    state.send(Command::Pause(paused));
}

#[tauri::command]
fn resend(state: State<DeckState>) {
    state.send(Command::Resend);
}

#[tauri::command]
fn simulate(state: State<DeckState>, simulation: Option<Simulation>) {
    state.send(Command::Simulate(simulation));
}

/// Opens config.json in the app Windows uses for .json files.
#[tauri::command]
async fn open_config() -> Result<(), String> {
    let target = Config::default_path().display().to_string();
    deck_engine::actions::execute(&Action::Open { target, args: None }).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn get_config() -> Result<Config, String> {
    Config::load_or_create(&Config::default_path()).map_err(|e| format!("{e:#}"))
}

/// Validates, writes config.json and has the engine apply it right away.
/// Nothing reaches the file unless it is valid.
#[tauri::command]
fn save_config(state: State<DeckState>, config: Config) -> Result<(), String> {
    config.validate().map_err(|e| format!("{e:#}"))?;
    config.save(&Config::default_path()).map_err(|e| format!("{e:#}"))?;
    state.send(Command::Reload);
    Ok(())
}

/// The image a key with this icon and color shows on the device.
#[tauri::command]
async fn render_icon(icon: Option<String>, color: String) -> Result<String, String> {
    let key = Key { label: String::new(), icon, color, action: None, front: false };
    let png = icons::render(&key, &config_dir()).map_err(|e| format!("{e:#}"))?;
    Ok(icons::data_url(&png))
}

#[tauri::command]
fn glyphs() -> Vec<&'static str> {
    icons::GLYPHS.iter().map(|(name, _)| *name).collect()
}

#[derive(Serialize)]
struct RunningApp {
    exe: String,
    title: String,
}

#[tauri::command]
async fn running_apps() -> Vec<RunningApp> {
    context::visible_apps().into_iter().map(|(exe, title)| RunningApp { exe, title }).collect()
}

/// Lets the person pick a PNG and copies it into the config folder, so the
/// key keeps working if the original moves. Returns the path to store.
#[tauri::command]
async fn pick_image(app: AppHandle) -> Result<Option<String>, String> {
    let Some(picked) = app.dialog().file().add_filter("Imagem PNG", &["png"]).blocking_pick_file() else {
        return Ok(None);
    };
    let source = picked.into_path().map_err(|e| e.to_string())?;
    let folder = config_dir().join("icons");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let target = free_name(&folder, &source);
    std::fs::copy(&source, &target).map_err(|e| e.to_string())?;
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    Ok(Some(format!("icons/{name}")))
}

/// `folder/<source name>`, or `name-2.png`, `name-3.png`… when taken.
fn free_name(folder: &Path, source: &Path) -> PathBuf {
    let stem = source.file_stem().unwrap_or_default().to_string_lossy();
    let mut candidate = folder.join(format!("{stem}.png"));
    let mut n = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{stem}-{n}.png"));
        n += 1;
    }
    candidate
}

fn config_dir() -> PathBuf {
    Config::dir_of(&Config::default_path())
}

/// Shows the window, creating it again if it was closed: a closed window
/// frees its WebView while the engine keeps running in the tray.
pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::default())
        .title("D200 Deck")
        .inner_size(1280.0, 860.0)
        .min_inner_size(960.0, 640.0)
        .center()
        .build();
    if let Err(e) = built {
        log::warn!("a janela não abriu: {e}");
    }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .format_timestamp_secs()
        .init();
    let hidden = std::env::args().any(|arg| arg == HIDDEN_FLAG);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![HIDDEN_FLAG])))
        .plugin(tauri_plugin_dialog::init())
        .manage(DeckState::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let engine = runtime::spawn(Config::default_path(), move |status| {
                *handle.state::<DeckState>().status.lock().unwrap() = status.clone();
                let _ = handle.emit("status", status);
                tray::update(&handle, status);
            });
            let _ = app.state::<DeckState>().engine.set(engine);
            tray::build(app.handle())?;
            if !hidden {
                show_window(app.handle());
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            set_paused,
            resend,
            simulate,
            open_config,
            get_config,
            save_config,
            render_icon,
            glyphs,
            running_apps,
            pick_image
        ])
        .build(tauri::generate_context!())
        .expect("o D200 Deck não iniciou");

    app.run(|_, event| {
        // Closing the window keeps the engine running in the tray; only "Sair" quits.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
