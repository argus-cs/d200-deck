// New versions: looked for on GitHub at start and every few hours, installed
// only when the person asks (window or tray). On Windows the installer
// closes the app and opens it again with the same arguments.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// The first look waits for the app and the network to settle.
const FIRST_CHECK: Duration = Duration::from_secs(20);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The newer version found, kept until it is installed.
#[derive(Default)]
pub struct Updates {
    found: Mutex<Option<Update>>,
}

/// What the window shows about a newer version.
#[derive(Clone, Serialize)]
pub struct Available {
    version: String,
    notes: Option<String>,
}

impl Available {
    fn of(update: &Update) -> Self {
        Self { version: update.version.clone(), notes: update.body.clone() }
    }
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FIRST_CHECK);
        loop {
            if let Err(e) = tauri::async_runtime::block_on(check(&app)) {
                log::warn!("não deu para procurar atualização: {e}");
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

/// Asks GitHub for a newer version, then tells the window and the tray.
pub async fn check(app: &AppHandle) -> Result<Option<Available>, String> {
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    let available = update.as_ref().map(Available::of);
    match &available {
        Some(a) => log::info!("versão {} disponível", a.version),
        None => log::info!("nenhuma versão nova"),
    }
    *app.state::<Updates>().found.lock().unwrap() = update;
    crate::tray::show_update(app, available.as_ref().map(|a| a.version.as_str()));
    let _ = app.emit("update", &available);
    Ok(available)
}

/// Downloads the installer and runs it. On Windows it never returns: the
/// installer closes the app first.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let update = app.state::<Updates>().found.lock().unwrap().clone().ok_or("nenhuma versão nova encontrada")?;
    log::info!("baixando a versão {}", update.version);
    let window = app.clone();
    let (mut downloaded, mut shown) = (0u64, None);
    update
        .download_and_install(
            move |chunk, total| {
                downloaded += chunk as u64;
                // Whole percents only: a chunk is a few kilobytes.
                let percent = total.map(|t| (downloaded * 100 / t.max(1)) as u8);
                if percent != shown {
                    shown = percent;
                    let _ = window.emit("update-progress", percent);
                }
            },
            || log::info!("download terminado, instalando"),
        )
        .await
        .map_err(|e| e.to_string())?;
    app.restart()
}

#[tauri::command]
pub fn get_update(state: State<Updates>) -> Option<Available> {
    state.found.lock().unwrap().as_ref().map(Available::of)
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<Available>, String> {
    check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    install(&app).await
}
