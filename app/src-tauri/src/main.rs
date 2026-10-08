// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use deck_engine::config::{Action, Config, KeyFace, LabelStyle, Screen};
use deck_engine::rules::Simulation;
use deck_engine::runtime::{self, Command, Engine, Status};
use deck_engine::{context, icons};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_window_state::{StateFlags, WindowExt};

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
    deck_engine::actions::execute(&Action::Open { target, args: None, watch: false }).map_err(|e| format!("{e:#}"))
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

/// An icon on a background, as the icon picker shows it.
#[tauri::command]
async fn render_icon(icon: Option<String>, color: String, icon_color: Option<String>) -> Result<String, String> {
    let png = icons::render(icon.as_deref(), &color, icon_color.as_deref(), &config_dir()).map_err(|e| format!("{e:#}"))?;
    Ok(icons::data_url(&png))
}

/// A whole key as the device shows it: colors, frame and label.
#[tauri::command]
async fn render_key(key: KeyFace, label: LabelStyle) -> Result<String, String> {
    let default_text = format!("#{}", label.color);
    let look = icons::Look {
        icon: key.icon.as_deref(),
        background: &key.color,
        icon_color: key.icon_color.as_deref().unwrap_or(icons::ICON_COLOR),
        border: key.border.as_deref(),
        label: if label.show { &key.label } else { "" },
        text_color: key.text_color.as_deref().unwrap_or(&default_text),
        label_px: (u32::from(label.size) * 12 / 5).clamp(12, 48),
        // The editor shows keys as configured, not whether their app runs.
        dim: false,
    };
    let png = icons::render_key(&look, &config_dir()).map_err(|e| format!("{e:#}"))?;
    Ok(icons::data_url(&png))
}

/// The visor with example data; `None` when the device draws it itself.
#[tauri::command]
async fn render_screen(screen: Screen) -> Result<Option<String>, String> {
    let png = deck_engine::screen::preview(&screen, &config_dir()).map_err(|e| format!("{e:#}"))?;
    Ok(png.map(|p| icons::data_url(&p)))
}

/// The icon of an app (its exe), saved for a key. Returns the path to store.
#[tauri::command]
async fn app_icon(path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let exe = PathBuf::from(&path);
        let png = icons::app_icon(&exe).map_err(|e| format!("{e:#}"))?;
        let stem = exe.file_stem().unwrap_or_default().to_string_lossy().to_string();
        save_icon(&png, &format!("app-{stem}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Any file, for the "open" action: a program, a document, a shortcut.
#[tauri::command]
async fn pick_file(app: AppHandle) -> Result<Option<String>, String> {
    match app.dialog().file().blocking_pick_file() {
        None => Ok(None),
        Some(picked) => picked.into_path().map(|p| Some(p.display().to_string())).map_err(|e| e.to_string()),
    }
}

#[tauri::command]
async fn pick_app_icon(app: AppHandle) -> Result<Option<String>, String> {
    let Some(picked) = app.dialog().file().add_filter("Programa", &["exe"]).blocking_pick_file() else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    app_icon(path.display().to_string()).await.map(Some)
}

/// The icon Edge keeps for an open page, saved for a key.
#[tauri::command]
async fn site_icon(state: State<'_, DeckState>, url: String, host: String) -> Result<String, String> {
    let browser = state.engine.get().ok_or("o motor ainda não iniciou")?.browser();
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = browser.favicon(&url).map_err(|e| format!("{e:#}"))?;
        let png = icons::frame_bytes(&bytes).map_err(|e| format!("{e:#}"))?;
        save_icon(&png, &format!("site-{host}"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Writes a key image into the config folder; returns the path to store.
fn save_icon(png: &[u8], stem: &str) -> Result<String, String> {
    let folder = config_dir().join("icons");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let target = free_name(&folder, stem);
    std::fs::write(&target, png).map_err(|e| e.to_string())?;
    Ok(format!("icons/{}", target.file_name().unwrap_or_default().to_string_lossy()))
}

#[derive(Serialize)]
struct Glyph {
    id: &'static str,
    /// Portuguese words to search by.
    label: &'static str,
}

#[tauri::command]
fn glyphs() -> Vec<Glyph> {
    icons::GLYPHS.iter().map(|(id, label, _)| Glyph { id, label }).collect()
}

/// The extension ships with the app, so the guide can point to a real folder.
#[tauri::command]
fn extension_folder(app: AppHandle) -> Result<String, String> {
    let folder = app.path().resource_dir().map_err(|e| e.to_string())?.join("extension-edge");
    Ok(folder.display().to_string())
}

#[tauri::command]
async fn open_extension_folder(app: AppHandle) -> Result<(), String> {
    let target = extension_folder(app)?;
    deck_engine::actions::execute(&Action::Open { target, args: None, watch: false }).map_err(|e| format!("{e:#}"))
}

#[tauri::command]
async fn open_edge_extensions() -> Result<(), String> {
    let action = Action::Open { target: "msedge.exe".into(), args: Some("edge://extensions".into()), watch: false };
    deck_engine::actions::execute(&action).map_err(|e| format!("{e:#}"))
}

#[derive(Serialize)]
struct RunningApp {
    exe: String,
    title: String,
    path: String,
}

#[tauri::command]
async fn running_apps() -> Vec<RunningApp> {
    context::visible_apps()
        .into_iter()
        .map(|app| RunningApp { exe: app.exe, title: app.title, path: app.path })
        .collect()
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
    let stem = source.file_stem().unwrap_or_default().to_string_lossy().to_string();
    let target = free_name(&folder, &stem);
    std::fs::copy(&source, &target).map_err(|e| e.to_string())?;
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    Ok(Some(format!("icons/{name}")))
}

/// `folder/<stem>.png`, or `<stem>-2.png`, `<stem>-3.png`… when taken.
fn free_name(folder: &Path, stem: &str) -> PathBuf {
    let stem: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '-' }).collect();
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
        // The app draws its own title bar with the window buttons.
        .decorations(false)
        // Tauri's file-drop handling blocks HTML drag and drop on Windows.
        .disable_drag_drop_handler()
        // Shown after the saved position is applied, so it doesn't jump.
        .visible(false)
        .build();
    match built {
        Ok(window) => {
            let _ = window.restore_state(window_state_flags());
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(e) => log::warn!("a janela não abriu: {e}"),
    }
}

/// Position, size and maximized are remembered; visibility is ours to decide.
fn window_state_flags() -> StateFlags {
    StateFlags::all() - StateFlags::VISIBLE
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
        .plugin(tauri_plugin_window_state::Builder::default().with_state_flags(window_state_flags()).build())
        .manage(DeckState::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let activity_handle = app.handle().clone();
            let engine = runtime::spawn(
                Config::default_path(),
                move |status| {
                    *handle.state::<DeckState>().status.lock().unwrap() = status.clone();
                    let _ = handle.emit("status", status);
                    tray::update(&handle, status);
                },
                move |activity| {
                    let _ = activity_handle.emit("activity", activity);
                },
            );
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
            render_key,
            render_screen,
            glyphs,
            running_apps,
            pick_image,
            pick_file,
            app_icon,
            pick_app_icon,
            site_icon,
            extension_folder,
            open_extension_folder,
            open_edge_extensions
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
