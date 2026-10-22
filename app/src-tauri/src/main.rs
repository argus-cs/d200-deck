// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod tray;
mod update;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use deck_engine::addons::{self, catalog, Choice, Settings};
use deck_engine::config::{Action, Config, KeyFace, LabelStyle, Screen};
use deck_engine::rules::Simulation;
use deck_engine::runtime::{self, Command, Engine, Status};
use deck_engine::system::Setting;
use deck_engine::{context, iconify, icons};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_window_state::{StateFlags, WindowExt};

/// Passed by the "start with Windows" entry: start in the tray, no window.
const HIDDEN_FLAG: &str = "--hidden";
/// Passed by the engine to run an addon that lives in this executable
/// (`$app` in its addon.json): `--addon steam`.
const ADDON_FLAG: &str = "--addon";
const MAIN_WINDOW: &str = "main";

#[derive(Default)]
pub struct DeckState {
    engine: OnceLock<Engine>,
    status: Mutex<Status>,
    /// The addons being installed or removed, by id or folder.
    addon_jobs: Mutex<HashSet<String>>,
}

impl DeckState {
    pub fn send(&self, command: Command) {
        if let Some(engine) = self.engine.get() {
            engine.send(command);
        }
    }
}

/// An addon being installed or removed, until this goes.
struct AddonJob<'a> {
    jobs: &'a Mutex<HashSet<String>>,
    name: String,
}

impl<'a> AddonJob<'a> {
    /// `None` while another job has this addon.
    fn start(state: &'a DeckState, name: &str) -> Option<Self> {
        let jobs = &state.addon_jobs;
        jobs.lock().unwrap().insert(name.to_string()).then(|| Self { jobs, name: name.to_string() })
    }
}

impl Drop for AddonJob<'_> {
    fn drop(&mut self) {
        self.jobs.lock().unwrap().remove(&self.name);
    }
}

/// What a command failed with, for the window: the whole chain of causes
/// (`{e:#}`), as the error text it shows.
pub struct CmdError(String);

type CmdResult<T> = Result<T, CmdError>;

impl<E: Into<anyhow::Error>> From<E> for CmdError {
    fn from(e: E) -> Self {
        CmdError(format!("{:#}", e.into()))
    }
}

impl Serialize for CmdError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Drawing, downloads and disk work on a worker thread, not the async ones.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> CmdResult<T> {
    Ok(tauri::async_runtime::spawn_blocking(work).await??)
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
async fn open_config() -> CmdResult<()> {
    open(Config::default_path().display().to_string(), None)
}

// Loading and saving stay on the main thread, which runs them one at a time
// in the order the window calls them: on worker threads, an older save could
// finish after a newer one and write over it. The file is small.
#[tauri::command]
fn get_config() -> CmdResult<Config> {
    Ok(Config::load_or_create(&Config::default_path())?)
}

/// Validates, writes config.json and has the engine apply it right away.
/// Nothing reaches the file unless it is valid. Returns it as written, so the
/// window can tell its own save from a hand edit when the engine reloads.
#[tauri::command]
fn save_config(state: State<DeckState>, config: Config) -> CmdResult<Config> {
    config.validate()?;
    config.save(&Config::default_path())?;
    state.send(Command::Reload);
    Ok(config)
}

/// An icon on a background, as the icon picker shows it.
#[tauri::command]
async fn render_icon(icon: Option<String>, color: String, icon_color: Option<String>) -> CmdResult<String> {
    blocking(move || Ok(icons::data_url(&icons::render(icon.as_deref(), &color, icon_color.as_deref(), &config_dir())?))).await
}

/// A whole key as the device shows it: colors, frame and label.
#[tauri::command]
async fn render_key(key: KeyFace, label: LabelStyle) -> CmdResult<String> {
    blocking(move || {
        let default_text = icons::default_text_color(&label);
        // Drawn the way the engine draws it for the device; the editor shows
        // keys as configured, not whether their app runs.
        let look = icons::Look::new(&key.appearance(), &label, &default_text, false);
        Ok(icons::data_url(&icons::render_key(&look, &config_dir())?))
    })
    .await
}

/// The visor with example data; `None` when the device draws it itself.
#[tauri::command]
async fn render_screen(screen: Screen) -> CmdResult<Option<String>> {
    blocking(move || Ok(deck_engine::screen::preview(&screen, &config_dir())?.map(|png| icons::data_url(&png)))).await
}

/// The icon of an app (its exe), saved for a key. Returns the path to store.
#[tauri::command]
async fn app_icon(path: String) -> CmdResult<String> {
    blocking(move || {
        let exe = PathBuf::from(&path);
        let png = icons::app_icon(&exe)?;
        let stem = exe.file_stem().unwrap_or_default().to_string_lossy().to_string();
        store_icon(&format!("app-{stem}"), "png", |target| std::fs::write(target, &png))
    })
    .await
}

/// Any file, for the "open" action: a program, a document, a shortcut.
#[tauri::command]
async fn pick_file(window: WebviewWindow) -> CmdResult<Option<String>> {
    Ok(pick(&window, None)?.map(|path| path.display().to_string()))
}

#[tauri::command]
async fn pick_app_icon(window: WebviewWindow) -> CmdResult<Option<String>> {
    match pick(&window, Some(("Programa", &["exe"])))? {
        None => Ok(None),
        Some(exe) => app_icon(exe.display().to_string()).await.map(Some),
    }
}

/// Lets the person pick a PNG and copies it into the config folder, so the
/// key keeps working if the original moves. Returns the path to store.
#[tauri::command]
async fn pick_image(window: WebviewWindow) -> CmdResult<Option<String>> {
    let Some(source) = pick(&window, Some(("Imagem PNG", &["png"])))? else {
        return Ok(None);
    };
    let stem = source.file_stem().unwrap_or_default().to_string_lossy().to_string();
    Ok(Some(store_icon(&stem, "png", |target| std::fs::copy(&source, target).map(|_| ()))?))
}

/// The icon Edge keeps for an open page, saved for a key.
#[tauri::command]
async fn site_icon(state: State<'_, DeckState>, url: String, host: String) -> CmdResult<String> {
    let browser = engine(&state)?.browser();
    blocking(move || {
        let png = icons::frame_bytes(&browser.favicon(&url)?)?;
        store_icon(&format!("site-{host}"), "png", |target| std::fs::write(target, &png))
    })
    .await
}

/// The file the person picks in Windows' dialog, or `None` if they cancel.
/// The dialog belongs to the window that asked: it stays in front of it, and
/// the window waits until it closes (without an owner, the dialog could hide
/// behind it while the editors stayed usable).
fn pick(window: &WebviewWindow, filter: Option<(&str, &[&str])>) -> anyhow::Result<Option<PathBuf>> {
    let mut dialog = window.dialog().file().set_parent(window);
    if let Some((name, extensions)) = filter {
        dialog = dialog.add_filter(name, extensions);
    }
    dialog.blocking_pick_file().map(|picked| picked.into_path()).transpose().map_err(Into::into)
}

/// Writes a key image into the config folder, under a free name made from
/// `stem`; returns the path to store ("icons/app-Discord.png").
fn store_icon(stem: &str, extension: &str, write: impl FnOnce(&Path) -> std::io::Result<()>) -> anyhow::Result<String> {
    let folder = config_dir().join("icons");
    std::fs::create_dir_all(&folder)?;
    let target = free_name(&folder, stem, extension);
    write(&target)?;
    Ok(format!("icons/{}", target.file_name().unwrap_or_default().to_string_lossy()))
}

/// Opens a file, folder or program the way the "open" action does.
fn open(target: String, args: Option<String>) -> CmdResult<()> {
    Ok(deck_engine::actions::execute(&Action::Open { target, args, watch: false })?)
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
fn extension_folder(app: AppHandle) -> CmdResult<String> {
    Ok(app.path().resource_dir()?.join("extension-edge").display().to_string())
}

#[tauri::command]
async fn open_extension_folder(app: AppHandle) -> CmdResult<()> {
    open(extension_folder(app)?, None)
}

#[tauri::command]
async fn open_edge_extensions() -> CmdResult<()> {
    open("msedge.exe".into(), Some("edge://extensions".into()))
}

/// Every Iconify set, for the "more icons" window.
#[tauri::command]
async fn iconify_sets() -> CmdResult<Vec<iconify::IconSet>> {
    blocking(|| iconify::sets(&config_dir())).await
}

/// One page of a set (downloaded the first time it is opened).
#[tauri::command]
async fn iconify_icons(prefix: String, filter: String, offset: usize, limit: usize) -> CmdResult<iconify::Page> {
    blocking(move || iconify::icons(&config_dir(), &prefix, &filter, offset, limit)).await
}

/// Icons across every set, named by id ("mdi:bluetooth"), with their SVG.
#[tauri::command]
async fn iconify_search(query: String) -> CmdResult<Vec<iconify::Icon>> {
    blocking(move || iconify::search(&query)).await
}

/// The sets' samples by id, kept on disk after the first time.
#[tauri::command]
async fn iconify_samples(ids: Vec<String>) -> CmdResult<Vec<iconify::Icon>> {
    blocking(move || iconify::samples(&config_dir(), &ids)).await
}

/// Saves "set:icon" as an SVG for a key; returns the path to store.
#[tauri::command]
async fn iconify_save(id: String) -> CmdResult<String> {
    blocking(move || iconify::save(&config_dir(), &id)).await
}

fn engine(state: &DeckState) -> anyhow::Result<&Engine> {
    state.engine.get().ok_or_else(|| anyhow::anyhow!("o motor ainda não iniciou"))
}

/// The choices of a list an addon fills while it runs (the games installed).
#[tauri::command]
async fn addon_options(state: State<'_, DeckState>, addon: String, action: String, field: String, settings: Settings) -> CmdResult<Vec<Choice>> {
    let addons = engine(&state)?.addons();
    blocking(move || addons.options(&addon, &action, &field, &settings)).await
}

/// An image an addon suggests for a key (its action's icon, a list choice's):
/// a built-in glyph stays as it is, a file is copied beside the config so the
/// key keeps it. Returns the icon to store.
#[tauri::command]
async fn addon_icon(state: State<'_, DeckState>, addon: String, icon: String) -> CmdResult<String> {
    if icons::is_glyph(&icon) {
        return Ok(icon);
    }
    let addons = engine(&state)?.addons();
    blocking(move || {
        let source = addons.image(&addon, &icon)?;
        let extension = if icons::is_svg(&icon) { "svg" } else { "png" };
        let stem = source.file_stem().unwrap_or_default().to_string_lossy().to_string();
        store_icon(&format!("addon-{stem}"), extension, |target| std::fs::copy(&source, target).map(|_| ()))
    })
    .await
}

/// Reads the addons folder again (the Addons page asks when it opens).
#[tauri::command]
fn rescan_addons(state: State<DeckState>) {
    state.send(Command::RescanAddons);
}

/// Starts an addon that gave up again.
#[tauri::command]
fn retry_addon(state: State<DeckState>, id: String) {
    state.send(Command::RetryAddon(id));
}

/// The addon catalog, downloaded at most once a day, or now with `refresh`.
#[tauri::command]
async fn addon_catalog(refresh: bool) -> CmdResult<catalog::Listing> {
    blocking(move || catalog::list(&config_dir(), refresh)).await
}

/// Downloads an addon of the catalog and puts it in the addons folder, in
/// place of the installed one for an update. Only the id comes from the
/// window: the address and the hash come from the catalog on disk. A second
/// call while it installs does nothing (Tauri's IPC may send a command again).
#[tauri::command]
async fn install_addon(state: State<'_, DeckState>, id: String) -> CmdResult<()> {
    let Some(_job) = AddonJob::start(&state, &id) else {
        return Ok(());
    };
    let engine = engine(&state)?;
    let staged = blocking(move || catalog::fetch(&config_dir(), &addons::staging(&config_dir()), &id)).await?;
    let (reply, answer) = channel();
    engine.send(Command::InstallAddon(staged, reply));
    blocking(move || engine_answer(answer)).await
}

/// Deletes an addon the person installed, by its folder's name.
#[tauri::command]
async fn remove_addon(state: State<'_, DeckState>, folder: String) -> CmdResult<()> {
    let Some(_job) = AddonJob::start(&state, &folder) else {
        return Ok(());
    };
    let (reply, answer) = channel();
    engine(&state)?.send(Command::RemoveAddon(folder, reply));
    blocking(move || engine_answer(answer)).await
}

/// What the engine says about a command it answers. It takes commands every
/// tenth of a second, with or without the device.
fn engine_answer(answer: Receiver<anyhow::Result<()>>) -> anyhow::Result<()> {
    answer.recv_timeout(Duration::from_secs(60)).map_err(|_| anyhow::anyhow!("o motor não respondeu"))?
}

/// An addon's page, in the browser: only the https address its entry in
/// the catalog gives, so the window can't open others through it.
#[tauri::command]
async fn open_addon_page(id: String) -> CmdResult<()> {
    let entry = blocking(move || catalog::find(&config_dir(), &id)).await?;
    match entry.homepage {
        Some(page) if page.starts_with("https://") => open(page, None),
        _ => Err(anyhow::anyhow!("o {} não tem uma página", entry.name).into()),
    }
}

#[tauri::command]
async fn open_addons_folder() -> CmdResult<()> {
    let folder = addons::folder(&config_dir());
    std::fs::create_dir_all(&folder)?;
    open(folder.display().to_string(), None)
}

/// The guide for addon authors, in the browser. A fixed address: the window
/// can't open others through it.
#[tauri::command]
async fn open_addons_guide() -> CmdResult<()> {
    open("https://github.com/argus-cs/d200-deck/blob/main/docs/addons.md".into(), None)
}

/// The log file, where addons' messages go too.
#[tauri::command]
async fn open_log() -> CmdResult<()> {
    open(deck_engine::logging::file(&config_dir()).display().to_string(), None)
}

/// The names of the active audio outputs or inputs, for the setting that picks one.
#[tauri::command]
async fn audio_devices(setting: Setting) -> CmdResult<Vec<String>> {
    blocking(move || deck_engine::system::audio_devices(setting)).await
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

/// `folder/<stem>.png`, or `<stem>-2.png`, `<stem>-3.png`… when taken.
fn free_name(folder: &Path, stem: &str, extension: &str) -> PathBuf {
    let stem: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '.' { c } else { '-' }).collect();
    let mut candidate = folder.join(format!("{stem}.{extension}"));
    let mut n = 2;
    while candidate.exists() {
        candidate = folder.join(format!("{stem}-{n}.{extension}"));
        n += 1;
    }
    candidate
}

/// `\\?\C:\…` (how Tauri hands the install folder) as `C:\…`, which reads
/// better where the window shows it. A network path keeps its prefix.
fn plain_path(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|text| text.strip_prefix(r"\\?\")) {
        Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
        _ => path,
    }
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

/// Runs the addon named after `--addon`, if asked to, and returns its exit
/// code. No window, tray or single-instance check: the engine that started
/// it talks to it over stdin and stdout, and its stderr goes to the log.
fn run_addon() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let at = args.iter().position(|arg| arg == ADDON_FLAG)?;
    Some(match args.get(at + 1).map(String::as_str) {
        Some("steam") => steam_addon::run(),
        other => {
            eprintln!("addon desconhecido: {other:?}");
            2
        }
    })
}

/// The Steam addon comes with the app and starts enabled where Steam is
/// installed, the first time the app meets it; after that the person decides.
/// Its data folder marks that the app already did (it exists once it ran).
fn enable_steam_addon() {
    let marker = addons::data_folder(&config_dir(), steam_addon::ID);
    if marker.exists() || !steam_addon::installed() {
        return;
    }
    let enabled = (|| -> anyhow::Result<()> {
        let path = Config::default_path();
        let mut config = Config::load_or_create(&path)?;
        if !config.addons.iter().any(|id| id == steam_addon::ID) {
            config.addons.push(steam_addon::ID.to_string());
            config.save(&path)?;
        }
        std::fs::create_dir_all(&marker)?;
        Ok(())
    })();
    match enabled {
        Ok(()) => log::info!("addon da Steam ligado: a Steam está instalada"),
        // A config with an error: the next start tries again.
        Err(e) => log::warn!("o addon da Steam não foi ligado: {e:#}"),
    }
}

fn main() {
    if let Some(code) = run_addon() {
        std::process::exit(code);
    }
    // Also to a file beside config.json: the installed app has no console.
    deck_engine::logging::init(&config_dir());
    enable_steam_addon();
    let hidden = std::env::args().any(|arg| arg == HIDDEN_FLAG);

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![HIDDEN_FLAG])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().with_state_flags(window_state_flags()).build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(DeckState::default())
        .manage(update::Updates::default())
        .setup(move |app| {
            let handle = app.handle().clone();
            let screen_handle = app.handle().clone();
            let activity_handle = app.handle().clone();
            // The addons that come with the app, in its install folder.
            let bundled = app.path().resource_dir().ok().map(|dir| plain_path(dir).join("addons"));
            let engine = runtime::spawn(
                Config::default_path(),
                bundled,
                move |status| {
                    *handle.state::<DeckState>().status.lock().unwrap() = status.clone();
                    let _ = handle.emit("status", status);
                    // Each change to the menu waits for the main thread: do it
                    // there, not here, where the device's keys wait in turn.
                    let (tray_handle, status) = (handle.clone(), status.clone());
                    let _ = handle.run_on_main_thread(move || tray::update(&tray_handle, &status));
                },
                // Only the visor changed: a few KB instead of every key's image.
                move |screen| {
                    screen_handle.state::<DeckState>().status.lock().unwrap().screen = screen.clone();
                    let _ = screen_handle.emit("screen", screen);
                },
                move |activity| {
                    let _ = activity_handle.emit("activity", activity);
                },
            );
            let _ = app.state::<DeckState>().engine.set(engine);
            tray::build(app.handle())?;
            update::start(app.handle());
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
            audio_devices,
            iconify_sets,
            iconify_icons,
            iconify_search,
            iconify_samples,
            iconify_save,
            pick_image,
            pick_file,
            app_icon,
            pick_app_icon,
            site_icon,
            extension_folder,
            open_extension_folder,
            open_edge_extensions,
            addon_options,
            addon_icon,
            rescan_addons,
            retry_addon,
            addon_catalog,
            install_addon,
            remove_addon,
            open_addon_page,
            open_addons_folder,
            open_addons_guide,
            open_log,
            update::get_update,
            update::check_update,
            update::install_update
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
