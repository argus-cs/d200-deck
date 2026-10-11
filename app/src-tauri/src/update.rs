// New versions: looked for on GitHub at start and every few hours, installed
// only when the person asks (window or tray). On Windows the installer
// closes the app and opens it again with the same arguments.

use std::sync::atomic::{AtomicBool, Ordering};
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
    /// Set while its installer downloads. A second click (window and tray, or
    /// the tray twice) would download it again, and two installers could run
    /// over the same files.
    installing: AtomicBool,
}

impl Updates {
    fn available(&self) -> Option<Available> {
        let installing = self.installing.load(Ordering::SeqCst);
        self.found.lock().unwrap().as_ref().map(|update| Available {
            version: update.version.clone(),
            notes: update.body.clone(),
            installing,
        })
    }
}

/// What the window and the tray show about a newer version.
#[derive(Clone, Serialize)]
pub struct Available {
    pub version: String,
    notes: Option<String>,
    /// Its installer is downloading.
    pub installing: bool,
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

/// Tells the window and the tray what there is now.
fn announce(app: &AppHandle) -> Option<Available> {
    let available = app.state::<Updates>().available();
    crate::tray::show_update(app, available.as_ref());
    let _ = app.emit("update", &available);
    available
}

/// Asks GitHub for a newer version, then tells the window and the tray.
pub async fn check(app: &AppHandle) -> Result<Option<Available>, String> {
    // The version downloading stays: an answer now would only bring the
    // "Update" button back in the middle of the download.
    if app.state::<Updates>().installing.load(Ordering::SeqCst) {
        return Ok(app.state::<Updates>().available());
    }
    let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?;
    match &update {
        Some(u) => log::info!("versão {} disponível", u.version),
        None => log::info!("nenhuma versão nova"),
    }
    *app.state::<Updates>().found.lock().unwrap() = update;
    Ok(announce(app))
}

/// Downloads the installer and runs it. On Windows it never returns: the
/// installer closes the app first.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let updates = app.state::<Updates>();
    let update = updates.found.lock().unwrap().clone().ok_or("nenhuma versão nova encontrada")?;
    // Already downloading, asked from the window and from the tray: that
    // download goes on and does what was asked.
    if updates.installing.swap(true, Ordering::SeqCst) {
        log::info!("a versão {} já está sendo baixada", update.version);
        return Ok(());
    }
    announce(app);
    log::info!("baixando a versão {}", update.version);
    let window = app.clone();
    let (mut downloaded, mut shown) = (0u64, None);
    let result = update
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
        .await;
    if let Err(e) = result {
        // The window and the tray offer it again.
        updates.installing.store(false, Ordering::SeqCst);
        announce(app);
        return Err(e.to_string());
    }
    app.restart()
}

#[tauri::command]
pub fn get_update(state: State<Updates>) -> Option<Available> {
    state.available()
}

#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<Available>, String> {
    check(&app).await
}

#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    install(&app).await
}
