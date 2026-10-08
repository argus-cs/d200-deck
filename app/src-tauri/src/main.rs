// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::sync::{Mutex, OnceLock};

use deck_engine::config::{Action, Config};
use deck_engine::rules::Simulation;
use deck_engine::runtime::{self, Command, Engine, Status};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;

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
        .invoke_handler(tauri::generate_handler![get_status, set_paused, resend, simulate, open_config])
        .build(tauri::generate_context!())
        .expect("o D200 Deck não iniciou");

    app.run(|_, event| {
        // Closing the window keeps the engine running in the tray; only "Sair" quits.
        if let RunEvent::ExitRequested { code: None, api, .. } = event {
            api.prevent_exit();
        }
    });
}
