//! Addons: programs that bring new actions to the keys. Each one is a folder
//! in %APPDATA%\D200Deck\addons with an addon.json (`manifest`), started as
//! a process of its own while enabled and talked to over its stdin and
//! stdout (`protocol`). One that quits is started again, a few times. The
//! engine owns the `Host`; `Addons` reaches the running ones from any thread.
//! The `catalog` lists addons to install.

pub mod catalog;
pub mod manifest;
pub mod protocol;

use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use log::{debug, error, info, warn};
use serde::Serialize;

pub use manifest::{is_id, ActionDef, Choice, Field, FieldKind, Manifest};
pub use protocol::{context_of, KeyState, Settings};

use crate::config::Action;
use crate::icons::fit;
use catalog::{Staged, Temp};
use manifest::Found;
use protocol::{FromAddon, Level, Line, ToAddon, Watched};

/// A message longer than this is skipped.
const MAX_LINE: usize = 1 << 20;
/// Messages waiting for an addon to read them; past this it is stuck.
const QUEUE: usize = 256;
/// How long a key press or the editor waits for an addon's answer.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);
/// The pause before starting a quit addon again, doubling each time.
const RESTART_DELAYS: [u64; 4] = [1, 2, 4, 8];
/// Quick exits in a row before giving up on an addon.
const MAX_QUICK_EXITS: u32 = 5;
/// After running this long, an exit no longer counts as part of a crash loop.
const STABLE: Duration = Duration::from_secs(30);

/// Where the addons are: one folder each, beside config.json.
pub fn folder(config_dir: &Path) -> PathBuf {
    config_dir.join("addons")
}

/// Where an addon keeps its own files (its `data_dir`).
pub fn data_folder(config_dir: &Path, id: &str) -> PathBuf {
    config_dir.join("addon-data").join(id)
}

/// Where addons are unpacked before they go in place, and where replaced or
/// removed ones go before they are deleted: beside the addons folder (a
/// rename can't cross disks) but out of it, so a scan never lists them.
pub fn staging(config_dir: &Path) -> PathBuf {
    config_dir.join("addon-staging")
}

/// Talks to the running addons from any thread: key presses from action
/// threads, the editor's questions from the app.
#[derive(Clone, Default)]
pub struct Addons {
    inner: Arc<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Each running addon's stdin queue, with its process's generation.
    running: Mutex<HashMap<String, (u64, SyncSender<String>)>>,
    results: Mutex<HashMap<u64, Sender<Option<String>>>>,
    options: Mutex<HashMap<u64, Sender<Result<Vec<Choice>, String>>>>,
    /// Every addon whose manifest loaded, by id.
    installed: Mutex<HashMap<String, Installed>>,
    next_id: AtomicU64,
}

#[derive(Clone)]
struct Installed {
    name: String,
    folder: PathBuf,
    data: PathBuf,
}

impl Addons {
    /// Runs a key's action: tells the addon and waits for its answer.
    pub fn press(&self, addon: &str, action: &str, settings: &Settings) -> Result<()> {
        let context = context_of(action, settings);
        let answer = self.ask(addon, &self.inner.results, |id| ToAddon::Press { id, context: &context, action, settings })?;
        match answer {
            None => bail!("o addon {} não respondeu em {} s", self.name(addon), ANSWER_TIMEOUT.as_secs()),
            Some(None) => Ok(()),
            Some(Some(error)) => bail!("{}: {error}", self.name(addon)),
        }
    }

    /// The key that ran `press` came up. Nothing waits for an answer.
    pub fn release(&self, addon: &str, action: &str, settings: &Settings) {
        let context = context_of(action, settings);
        if let Err(e) = self.send(addon, &ToAddon::Release { context: &context, action, settings }) {
            debug!("{e:#}");
        }
    }

    /// The choices of a list the addon fills (`dynamic`), for the editor.
    pub fn options(&self, addon: &str, action: &str, field: &str, settings: &Settings) -> Result<Vec<Choice>> {
        match self.ask(addon, &self.inner.options, |id| ToAddon::Options { id, action, field, settings })? {
            None => bail!("o addon {} não respondeu em {} s", self.name(addon), ANSWER_TIMEOUT.as_secs()),
            Some(Ok(options)) => Ok(options),
            Some(Err(error)) => bail!("{}: {error}", self.name(addon)),
        }
    }

    /// An image an addon offers for a key: a file in its folder (relative
    /// paths start there) or in its data folder, and nowhere else.
    pub fn image(&self, addon: &str, name: &str) -> Result<PathBuf> {
        let installed = self.inner.installed.lock().unwrap().get(addon).cloned();
        let installed = installed.ok_or_else(|| anyhow!("o addon {addon} não está instalado"))?;
        let lower = name.to_ascii_lowercase();
        if !lower.ends_with(".png") && !lower.ends_with(".svg") {
            bail!("{name:?} não é uma imagem .png ou .svg");
        }
        let path = Path::new(name);
        let candidate = match path.is_absolute() {
            true => path.to_path_buf(),
            false if manifest::inside(path) => installed.folder.join(path),
            false => bail!("a imagem {name:?} fica fora da pasta do addon"),
        };
        // Through links too: the real file must be in one of its folders.
        let real = candidate.canonicalize().with_context(|| format!("a imagem {name:?} não existe"))?;
        let allowed = [&installed.folder, &installed.data].iter().filter_map(|dir| dir.canonicalize().ok()).any(|dir| real.starts_with(dir));
        if !allowed {
            bail!("a imagem {name:?} fica fora das pastas do addon");
        }
        Ok(real)
    }

    /// Sends a request and waits for its answer: `None` when none came in time.
    fn ask<'m, T>(&self, addon: &str, waiting: &Mutex<HashMap<u64, Sender<T>>>, message: impl FnOnce(u64) -> ToAddon<'m>) -> Result<Option<T>> {
        let id = self.inner.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        waiting.lock().unwrap().insert(id, tx);
        let answer = self.send(addon, &message(id)).map(|()| rx.recv_timeout(ANSWER_TIMEOUT).ok());
        waiting.lock().unwrap().remove(&id);
        answer
    }

    fn send(&self, addon: &str, message: &ToAddon) -> Result<()> {
        let line = serde_json::to_string(message)?;
        let sent = match self.inner.running.lock().unwrap().get(addon) {
            Some((_, queue)) => queue.try_send(line),
            None => Err(TrySendError::Disconnected(line)),
        };
        match sent {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => bail!("o addon {} parou de ler as mensagens", self.name(addon)),
            Err(TrySendError::Disconnected(_)) => bail!("o addon {} não está rodando", self.name(addon)),
        }
    }

    /// Its name for people, or its id when it isn't installed.
    fn name(&self, addon: &str) -> String {
        self.inner.installed.lock().unwrap().get(addon).map_or_else(|| addon.to_string(), |installed| installed.name.clone())
    }

    /// A line from an addon's stdout: answers go to whoever waits for them,
    /// key states to the engine.
    fn receive(&self, addon: &str, generation: u64, text: &str, events: &Sender<Event>) {
        match serde_json::from_str::<FromAddon>(text) {
            Ok(FromAddon::State { context, state }) => {
                let _ = events.send(Event::State { addon: addon.to_string(), generation, context, state });
            }
            Ok(FromAddon::Result { id, error }) => {
                if let Some(waiter) = self.inner.results.lock().unwrap().remove(&id) {
                    let _ = waiter.send(error);
                }
            }
            Ok(FromAddon::Options { id, options, error }) => {
                if let Some(waiter) = self.inner.options.lock().unwrap().remove(&id) {
                    let _ = waiter.send(error.map_or(Ok(options), Err));
                }
            }
            Ok(FromAddon::Log { level, message }) => {
                let message = fit(&message, 2000);
                match level {
                    Level::Debug => debug!("addon {addon}: {message}"),
                    Level::Info => info!("addon {addon}: {message}"),
                    Level::Warn => warn!("addon {addon}: {message}"),
                    Level::Error => error!("addon {addon}: {message}"),
                }
            }
            Err(e) => warn!("addon {addon}: mensagem ignorada ({e}): {}", fit(text, 200)),
        }
    }
}

/// What the reading threads tell the engine.
enum Event {
    State { addon: String, generation: u64, context: String, state: KeyState },
    /// Its stdout closed: it quit.
    Exited { addon: String, generation: u64 },
}

/// An enabled addon's process, running or waiting to run again.
struct Proc {
    /// Tells this process apart from the ones before it, whose last messages may still arrive.
    generation: u64,
    /// As it was started: a new one on disk starts it again.
    manifest: Manifest,
    child: Option<Child>,
    started: Instant,
    quick_exits: u32,
    /// Starts again then.
    restart_at: Option<Instant>,
    /// Why it isn't running, while it isn't.
    error: Option<String>,
    /// The last line it wrote to stderr: often why it quit.
    last_words: Arc<Mutex<Option<String>>>,
    /// Signals that stderr was read to its end.
    stderr_done: Option<Receiver<()>>,
}

/// An addon folder as the window lists it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AddonStatus {
    /// The manifest's id; empty when it didn't load.
    pub id: String,
    /// It came with the app.
    pub bundled: bool,
    /// The folder's name, which also tells apart addons whose manifest didn't load.
    pub folder: String,
    pub path: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub enabled: bool,
    pub state: AddonState,
    pub error: Option<String>,
    /// How it is started ("node main.js").
    pub run: String,
    pub actions: Vec<ActionDef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AddonState {
    /// Not enabled.
    Off,
    Running,
    /// Quit, and starts again shortly.
    Restarting,
    /// Didn't start, or quit too often: waits for "try again".
    Stopped,
    /// Its addon.json didn't load.
    Invalid,
}

/// Starts and stops the enabled addons, reads what they say and keeps it.
/// Lives on the engine's thread.
pub struct Host {
    dir: PathBuf,
    /// The addons that came with the app, if it has any.
    bundled: Option<PathBuf>,
    config_dir: PathBuf,
    found: Vec<Found>,
    enabled: Vec<String>,
    procs: HashMap<String, Proc>,
    /// The keys each addon was last told about.
    watching: HashMap<String, Vec<Watched>>,
    /// What addons said about their keys, by addon id and context.
    states: HashMap<(String, String), KeyState>,
    events_tx: Sender<Event>,
    events: Receiver<Event>,
    handle: Addons,
    generation: u64,
    /// An addon started, stopped or changed state since the last poll.
    dirty: bool,
}

impl Host {
    /// Finds the addons beside config.json and those that came with the app
    /// (`bundled`, a folder of addon folders); none runs until `apply` enables it.
    pub fn new(config_dir: &Path, bundled: Option<PathBuf>) -> Self {
        // What a closed app left there: an unpacked addon, or one it couldn't delete.
        let _ = std::fs::remove_dir_all(staging(config_dir));
        let (events_tx, events) = channel();
        let mut host = Self {
            dir: folder(config_dir),
            bundled,
            config_dir: config_dir.to_path_buf(),
            found: Vec::new(),
            enabled: Vec::new(),
            procs: HashMap::new(),
            watching: HashMap::new(),
            states: HashMap::new(),
            events_tx,
            events,
            handle: Addons::default(),
            generation: 0,
            dirty: true,
        };
        host.rescan();
        host
    }

    pub fn handle(&self) -> Addons {
        self.handle.clone()
    }

    /// Reads the addons folder again: new addons show, removed ones stop and
    /// changed ones start again with their new manifest.
    pub fn rescan(&mut self) {
        self.found = manifest::scan(&self.dir, self.bundled.as_deref());
        let installed = self
            .found
            .iter()
            .filter_map(|found| {
                let manifest = found.manifest.as_ref().ok()?;
                let data = data_folder(&self.config_dir, &manifest.id);
                Some((manifest.id.clone(), Installed { name: manifest.name.clone(), folder: found.folder.clone(), data }))
            })
            .collect();
        *self.handle.inner.installed.lock().unwrap() = installed;
        let changed: Vec<String> =
            self.procs.iter().filter(|(id, proc)| self.manifest(id) != Some(&proc.manifest)).map(|(id, _)| id.clone()).collect();
        for id in changed {
            self.stop(&id);
        }
        self.sync();
        self.dirty = true;
    }

    /// The addons config.json enables: the others stop, these start.
    pub fn apply(&mut self, enabled: &[String]) {
        if self.enabled != enabled {
            self.enabled = enabled.to_vec();
            self.dirty = true;
        }
        self.sync();
    }

    /// Starts an addon that gave up again, with a clean slate.
    pub fn retry(&mut self, id: &str) {
        self.stop(id);
        self.sync();
    }

    /// Puts an addon from the catalog in place of the person's addon with its
    /// id (an update), or in a new folder named after it. It stops first:
    /// Windows keeps the folder of a running program. Reading the folder
    /// again starts it if it is enabled.
    pub fn install(&mut self, staged: Staged) -> Result<()> {
        let id = staged.manifest.id.clone();
        let current = self.found.iter().find(|found| !found.bundled && found.manifest.as_ref().is_ok_and(|m| m.id == id));
        let current = current.map(|found| found.folder.clone());
        let target = current.clone().unwrap_or_else(|| free_folder(&self.dir, &id));
        self.stop(&id);
        let installed = (|| {
            let old = current.as_deref().map(|folder| self.set_aside(folder)).transpose()?;
            std::fs::create_dir_all(&self.dir)?;
            if let Err(e) = move_folder(&staged.folder, &target) {
                // The old one goes back, to keep working.
                if let Some(old) = &old {
                    let _ = move_folder(old.path(), &target);
                }
                return Err(e);
            }
            info!("addon {id} {} instalado em {}", staged.manifest.version, target.display());
            Ok(old)
        })();
        self.rescan();
        // The old folder is deleted now, with nothing running from it.
        installed.map(drop)
    }

    /// Deletes an addon the person put in the addons folder, by the folder's
    /// name, stopping it first. Its data folder stays, for when it comes back.
    pub fn remove(&mut self, folder: &str) -> Result<()> {
        // Not one that came with the app: those live in its install folder.
        let found = self.found.iter().find(|found| !found.bundled && manifest::folder_name(&found.folder) == folder);
        let (path, id) = found
            .map(|found| (found.folder.clone(), found.manifest.as_ref().ok().map(|m| m.id.clone())))
            .ok_or_else(|| anyhow!("a pasta {folder} não está entre os addons que você instalou"))?;
        if let Some(id) = id {
            self.stop(&id);
        }
        let removed = self.set_aside(&path);
        self.rescan();
        removed.map(|old| {
            info!("addon da pasta {folder} removido");
            drop(old);
        })
    }

    /// Moves an addon's folder out of the addons folder, to be deleted when
    /// what this returns is dropped.
    fn set_aside(&self, folder: &Path) -> Result<Temp> {
        let aside = Temp::new(&staging(&self.config_dir), &format!("{}-old", manifest::folder_name(folder)))?;
        move_folder(folder, aside.path()).context("feche os programas abertos de dentro da pasta do addon e tente de novo")?;
        Ok(aside)
    }

    fn sync(&mut self) {
        let wanted: HashSet<String> = self.enabled.iter().filter(|id| self.manifest(id).is_some()).cloned().collect();
        let unwanted: Vec<String> = self.procs.keys().filter(|id| !wanted.contains(*id)).cloned().collect();
        for id in unwanted {
            self.stop(&id);
        }
        let mut new: Vec<&String> = wanted.iter().filter(|id| !self.procs.contains_key(*id)).collect();
        new.sort();
        for id in new.into_iter().cloned().collect::<Vec<_>>() {
            self.start(&id, 0);
        }
    }

    fn manifest(&self, id: &str) -> Option<&Manifest> {
        self.found.iter().filter_map(|found| found.manifest.as_ref().ok()).find(|manifest| manifest.id == id)
    }

    fn start(&mut self, id: &str, quick_exits: u32) {
        let Some(found) = self.found.iter().find(|found| found.manifest.as_ref().is_ok_and(|m| m.id == id)) else {
            return;
        };
        let (Ok(manifest), folder, bundled) = (found.manifest.clone(), found.folder.clone(), found.bundled) else {
            return;
        };
        self.generation += 1;
        let data = data_folder(&self.config_dir, id);
        let mut proc = Proc {
            generation: self.generation,
            manifest: manifest.clone(),
            child: None,
            started: Instant::now(),
            quick_exits,
            restart_at: None,
            error: None,
            last_words: Arc::default(),
            stderr_done: None,
        };
        let spawned = spawn(&manifest, (&folder, bundled), &data, proc.generation, &self.handle, &self.events_tx, &proc.last_words);
        match spawned {
            Ok((child, queue, stderr_done)) => {
                info!("addon {id} rodando ({})", manifest.command_line());
                let hello = ToAddon::Hello {
                    api: manifest::API_VERSION,
                    app_version: env!("CARGO_PKG_VERSION"),
                    addon_dir: folder.display().to_string(),
                    data_dir: data.display().to_string(),
                    language: "pt-BR",
                };
                let keys = self.watching.get(id).cloned().unwrap_or_default();
                for message in [hello, ToAddon::Watch { keys: &keys }] {
                    let _ = queue.try_send(serde_json::to_string(&message).unwrap_or_default());
                }
                self.handle.inner.running.lock().unwrap().insert(id.to_string(), (proc.generation, queue));
                proc.child = Some(child);
                proc.stderr_done = Some(stderr_done);
            }
            Err(e) => {
                warn!("addon {id} não abriu: {e:#}");
                proc.error = Some(format!("Não abriu: {e:#}"));
            }
        }
        self.procs.insert(id.to_string(), proc);
        self.dirty = true;
    }

    fn stop(&mut self, id: &str) {
        let Some(mut proc) = self.procs.remove(id) else {
            return;
        };
        self.unregister(id, proc.generation);
        self.states.retain(|(addon, _), _| addon != id);
        if let Some(mut child) = proc.child.take() {
            let _ = child.kill();
            let _ = child.wait();
            info!("addon {id} parado");
        }
        self.dirty = true;
    }

    /// Its stdin queue goes: the writing thread ends and closes the pipe.
    fn unregister(&self, id: &str, generation: u64) {
        let mut running = self.handle.inner.running.lock().unwrap();
        if running.get(id).is_some_and(|(current, _)| *current == generation) {
            running.remove(id);
        }
    }

    /// Reads what the addons said, notices the ones that quit and starts
    /// again those due. True when a key or the addons' list may look different.
    pub fn poll(&mut self) -> bool {
        let mut changed = std::mem::take(&mut self.dirty);
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::State { addon, generation, context, state } => {
                    let current = self.procs.get(&addon).is_some_and(|proc| proc.generation == generation && proc.child.is_some());
                    let key = (addon, context);
                    if current && self.states.get(&key) != Some(&state) {
                        self.states.insert(key, state);
                        changed = true;
                    }
                }
                Event::Exited { addon, generation } => changed |= self.exited(&addon, generation),
            }
        }
        // A process can end with its stdout still open in a program it started.
        let ended: Vec<(String, u64)> = self
            .procs
            .iter_mut()
            .filter_map(|(id, proc)| proc.child.as_mut()?.try_wait().ok().flatten().map(|_| (id.clone(), proc.generation)))
            .collect();
        for (id, generation) in ended {
            changed |= self.exited(&id, generation);
        }
        let now = Instant::now();
        let due: Vec<(String, u32)> = self
            .procs
            .iter()
            .filter(|(_, proc)| proc.restart_at.is_some_and(|at| at <= now))
            .map(|(id, proc)| (id.clone(), proc.quick_exits))
            .collect();
        for (id, quick_exits) in due {
            self.procs.remove(&id);
            self.start(&id, quick_exits);
            changed = true;
        }
        changed
    }

    fn exited(&mut self, addon: &str, generation: u64) -> bool {
        let Some(proc) = self.procs.get_mut(addon).filter(|proc| proc.generation == generation) else {
            return false;
        };
        let Some(mut child) = proc.child.take() else {
            return false;
        };
        let code = exit_code(&mut child);
        // Its last words may still be on their way.
        if let Some(done) = proc.stderr_done.take() {
            let _ = done.recv_timeout(Duration::from_millis(300));
        }
        if proc.started.elapsed() >= STABLE {
            proc.quick_exits = 0;
        }
        proc.quick_exits += 1;
        let words = proc.last_words.lock().unwrap().clone();
        let why = match (words, code) {
            (Some(words), _) => words,
            (None, Some(code)) => format!("saiu com o código {code}"),
            (None, None) => "fechou".to_string(),
        };
        if proc.quick_exits >= MAX_QUICK_EXITS {
            warn!("addon {addon} fechou {} vezes seguidas e não vai ser aberto de novo: {why}", proc.quick_exits);
            proc.error = Some(format!(
                "O addon fechou {} vezes seguidas e não vai ser aberto de novo sozinho. Última mensagem: {why}",
                proc.quick_exits
            ));
        } else {
            let delay = RESTART_DELAYS[(proc.quick_exits as usize - 1).min(RESTART_DELAYS.len() - 1)];
            warn!("addon {addon} fechou ({why}); abrindo de novo em {delay} s");
            proc.restart_at = Some(Instant::now() + Duration::from_secs(delay));
            proc.error = Some(format!("Fechou: {why}. Abrindo de novo em {delay} s."));
        }
        self.unregister(addon, generation);
        self.states.retain(|(id, _), _| id != addon);
        true
    }

    /// Tells each addon which of its keys may show now (the layers', the
    /// folders', both faces), when that changed. An addon that starts hears it too.
    pub fn watch<'a>(&mut self, actions: impl IntoIterator<Item = &'a Action>) {
        let mut lists: HashMap<String, Vec<Watched>> = HashMap::new();
        for action in actions {
            let Some((addon, id, settings)) = action.addon() else {
                continue;
            };
            let context = context_of(id, settings);
            let list = lists.entry(addon.to_string()).or_default();
            if !list.iter().any(|watched| watched.context == context) {
                list.push(Watched { context, action: id.to_string(), settings: settings.clone() });
            }
        }
        let addons: HashSet<String> = lists.keys().chain(self.watching.keys()).cloned().collect();
        for addon in addons {
            let mut list = lists.remove(&addon).unwrap_or_default();
            list.sort_by(|a, b| a.context.cmp(&b.context));
            if self.watching.get(&addon).map_or(list.is_empty(), |old| *old == list) {
                continue;
            }
            if self.procs.get(&addon).is_some_and(|proc| proc.child.is_some()) {
                if let Err(e) = self.handle.send(&addon, &ToAddon::Watch { keys: &list }) {
                    warn!("{e:#}");
                }
            }
            if list.is_empty() {
                self.watching.remove(&addon);
            } else {
                self.watching.insert(addon, list);
            }
        }
    }

    /// The ids of the addons running now.
    pub fn running(&self) -> HashSet<String> {
        self.procs.iter().filter(|(_, proc)| proc.child.is_some()).map(|(id, _)| id.clone()).collect()
    }

    /// What the running addons said about their keys, by addon id and context.
    pub fn states(&self) -> &HashMap<(String, String), KeyState> {
        &self.states
    }

    /// The actions whose keys follow what their addon says (on or off), by addon and action id.
    pub fn stateful(&self) -> HashSet<(String, String)> {
        self.found
            .iter()
            .filter_map(|found| found.manifest.as_ref().ok())
            .flat_map(|manifest| manifest.actions.iter().filter(|a| a.state).map(|a| (manifest.id.clone(), a.id.clone())))
            .collect()
    }

    /// "Contador: Contar", for the log and the window.
    pub fn describe(&self, addon: &str, action: &str) -> String {
        match self.manifest(addon) {
            Some(manifest) => format!("{}: {}", manifest.name, manifest.action(action).map_or(action, |a| a.name.as_str())),
            None => format!("addon {addon}: {action}"),
        }
    }

    pub fn statuses(&self) -> Vec<AddonStatus> {
        self.found
            .iter()
            .map(|found| {
                let folder = manifest::folder_name(&found.folder);
                let path = found.folder.display().to_string();
                match &found.manifest {
                    Err(error) => AddonStatus {
                        id: String::new(),
                        bundled: found.bundled,
                        name: folder.clone(),
                        folder,
                        path,
                        version: String::new(),
                        author: String::new(),
                        description: String::new(),
                        enabled: false,
                        state: AddonState::Invalid,
                        error: Some(error.clone()),
                        run: String::new(),
                        actions: Vec::new(),
                    },
                    Ok(manifest) => {
                        let enabled = self.enabled.contains(&manifest.id);
                        let proc = self.procs.get(&manifest.id);
                        let state = match proc {
                            _ if !enabled => AddonState::Off,
                            Some(proc) if proc.child.is_some() => AddonState::Running,
                            Some(proc) if proc.restart_at.is_some() => AddonState::Restarting,
                            _ => AddonState::Stopped,
                        };
                        AddonStatus {
                            id: manifest.id.clone(),
                            bundled: found.bundled,
                            folder,
                            path,
                            name: manifest.name.clone(),
                            version: manifest.version.clone(),
                            author: manifest.author.clone(),
                            description: manifest.description.clone(),
                            enabled,
                            state,
                            error: proc.filter(|_| enabled).and_then(|proc| proc.error.clone()),
                            run: manifest.command_line(),
                            actions: manifest.actions.clone(),
                        }
                    }
                }
            })
            .collect()
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let ids: Vec<String> = self.procs.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
        }
    }
}

/// `dir/<id>`, or `<id>-2`, `<id>-3`… when a folder has that name.
fn free_folder(dir: &Path, id: &str) -> PathBuf {
    let mut candidate = dir.join(id);
    let mut n = 2;
    while candidate.exists() {
        candidate = dir.join(format!("{id}-{n}"));
        n += 1;
    }
    candidate
}

/// Renames a folder, trying a few times: right after a process ends, or
/// while an antivirus reads the files, Windows may hold it for a moment.
fn move_folder(from: &Path, to: &Path) -> Result<()> {
    let mut tries = 0;
    loop {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(_) if tries < 5 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e).with_context(|| format!("não consegui mover {} para {}", from.display(), to.display())),
        }
    }
}

/// How the process ended. Its stdout closes as it quits, slightly before
/// Windows reports it ended; one that keeps running without it is ended here.
fn exit_code(child: &mut Child) -> Option<i32> {
    for _ in 0..10 {
        if let Ok(Some(status)) = child.try_wait() {
            return status.code();
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    child.wait().ok().and_then(|status| status.code())
}

type Spawned = (Child, SyncSender<String>, Receiver<()>);

/// Starts the addon's program in its folder, with a thread for each pipe.
fn spawn(
    manifest: &Manifest,
    (folder, bundled): (&Path, bool),
    data: &Path,
    generation: u64,
    handle: &Addons,
    events: &Sender<Event>,
    last_words: &Arc<Mutex<Option<String>>>,
) -> Result<Spawned> {
    let (program, args) = manifest.command(folder, bundled)?;
    std::fs::create_dir_all(data).with_context(|| format!("a pasta de dados {} não foi criada", data.display()))?;
    let mut command = Command::new(&program);
    command.args(args).current_dir(folder).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().with_context(|| format!("{} não abriu", program.display()))?;
    #[cfg(windows)]
    job::assign(&child);
    let pipes = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let threads = match pipes {
        (Some(stdin), Some(stdout), Some(stderr)) => {
            start_threads(&manifest.id, generation, (stdin, stdout, stderr), handle, events, last_words)
        }
        _ => Err(anyhow!("os canais do processo não abriram")),
    };
    match threads {
        Ok((queue, stderr_done)) => Ok((child, queue, stderr_done)),
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(e)
        }
    }
}

fn start_threads(
    id: &str,
    generation: u64,
    (stdin, stdout, stderr): (ChildStdin, ChildStdout, ChildStderr),
    handle: &Addons,
    events: &Sender<Event>,
    last_words: &Arc<Mutex<Option<String>>>,
) -> Result<(SyncSender<String>, Receiver<()>)> {
    let (queue, outgoing) = sync_channel::<String>(QUEUE);
    std::thread::Builder::new().name(format!("addon {id} stdin")).spawn(move || write_lines(stdin, outgoing))?;
    let (addon, handle, events) = (id.to_string(), handle.clone(), events.clone());
    std::thread::Builder::new().name(format!("addon {id} stdout")).spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match protocol::read_line(&mut reader, MAX_LINE) {
                Ok(Some(Line::Text(text))) if !text.trim().is_empty() => handle.receive(&addon, generation, &text, &events),
                Ok(Some(Line::Text(_))) => {}
                Ok(Some(Line::TooLong)) => warn!("addon {addon}: mensagem de mais de {} MiB ignorada", MAX_LINE >> 20),
                Ok(None) | Err(_) => break,
            }
        }
        let _ = events.send(Event::Exited { addon, generation });
    })?;
    let (addon, last_words) = (id.to_string(), Arc::clone(last_words));
    let (done_tx, done) = channel();
    std::thread::Builder::new().name(format!("addon {id} stderr")).spawn(move || {
        let mut reader = BufReader::new(stderr);
        while let Ok(Some(line)) = protocol::read_line(&mut reader, MAX_LINE) {
            let Line::Text(text) = line else { continue };
            let text = text.trim();
            if !text.is_empty() {
                info!("addon {addon}: {}", fit(text, 2000));
                *last_words.lock().unwrap() = Some(fit(text, 300));
            }
        }
        let _ = done_tx.send(());
    })?;
    Ok((queue, done))
}

/// Writes each queued message as a line; when the queue goes, the pipe closes
/// and the addon reads the end of its input.
fn write_lines(mut stdin: ChildStdin, lines: Receiver<String>) {
    for line in lines {
        let written = stdin.write_all(line.as_bytes()).and_then(|()| stdin.write_all(b"\n")).and_then(|()| stdin.flush());
        if written.is_err() {
            break;
        }
    }
}

/// Every addon runs in one job: when this app's process ends, however it
/// ends, Windows ends them too. Programs an addon opens stay out of it
/// (silent breakaway), so they stay open.
#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;
    use std::sync::OnceLock;

    use log::warn;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK,
    };

    struct Job(HANDLE);

    // A job handle can be used from any thread; it is never closed.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    fn job() -> Option<&'static Job> {
        static JOB: OnceLock<Option<Job>> = OnceLock::new();
        JOB.get_or_init(|| create().map_err(|e| warn!("os addons não vão fechar junto se o app cair: {e}")).ok()).as_ref()
    }

    fn create() -> windows::core::Result<Job> {
        unsafe {
            let job = CreateJobObjectW(None, PCWSTR::null())?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )?;
            Ok(Job(job))
        }
    }

    pub fn assign(child: &Child) {
        let Some(job) = job() else { return };
        if let Err(e) = unsafe { AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())) } {
            warn!("o processo {} não entrou no grupo dos addons: {e}", child.id());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Files<'a> = &'a [(&'a str, &'a str)];

    /// A config folder of its own, with these addons in it.
    fn config_dir(name: &str, addons: &[(&str, &str, Files)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deck-engine-addons-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, manifest, files) in addons {
            let folder = super::folder(&dir).join(folder);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(manifest::MANIFEST), manifest).unwrap();
            for (name, text) in *files {
                std::fs::write(folder.join(name), text).unwrap();
            }
        }
        dir
    }

    fn wait_for(host: &mut Host, what: &str, mut done: impl FnMut(&Host) -> bool) {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            host.poll();
            if done(host) {
                return;
            }
            assert!(Instant::now() < until, "esperando {what}: {:?}", host.statuses());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn settings(json: &str) -> Settings {
        serde_json::from_str(json).unwrap()
    }

    fn addon_action(action: &str, json: &str) -> Action {
        Action::Addon { addon: "exemplo.contador".into(), action: action.into(), settings: settings(json) }
    }

    fn state<'a>(host: &'a Host, action: &str, json: &str) -> Option<&'a KeyState> {
        host.states().get(&("exemplo.contador".to_string(), context_of(action, &settings(json))))
    }

    /// The example in docs/addons, run with node (CI installs it for the frontend).
    #[test]
    fn the_example_addon_runs_and_answers() {
        let example: Files = &[("main.js", include_str!("../../../../docs/addons/exemplo/main.js"))];
        let dir = config_dir("example", &[("exemplo", include_str!("../../../../docs/addons/exemplo/addon.json"), example)]);
        let mut host = Host::new(&dir, None);
        let addons = host.handle();
        assert_eq!(host.statuses()[0].state, AddonState::Off, "found, but off until enabled");
        assert!(addons.press("exemplo.contador", "count", &Settings::new()).unwrap_err().to_string().contains("não está rodando"));

        host.apply(&["exemplo.contador".to_string()]);
        let keys = [
            addon_action("count", r#"{ "step": 2 }"#),
            addon_action("switch", r#"{ "item": "luz" }"#),
            addon_action("switch", "{}"),
            // The same key twice is watched once.
            addon_action("count", r#"{ "step": 2 }"#),
        ];
        host.watch(&keys);
        assert_eq!(host.watching["exemplo.contador"].len(), 3);
        wait_for(&mut host, "o contador na tecla", |host| {
            state(host, "count", r#"{ "step": 2 }"#).is_some_and(|s| s.label.as_deref() == Some("0 toques"))
        });
        assert_eq!(host.statuses()[0].state, AddonState::Running);
        assert!(host.running().contains("exemplo.contador"));
        assert!(host.stateful().contains(&("exemplo.contador".to_string(), "switch".to_string())));
        assert_eq!(host.describe("exemplo.contador", "count"), "Contador: Contar");

        addons.press("exemplo.contador", "count", &settings(r#"{ "step": 2 }"#)).unwrap();
        wait_for(&mut host, "a contagem nova", |host| {
            state(host, "count", r#"{ "step": 2 }"#).is_some_and(|s| s.label.as_deref() == Some("2 toques"))
        });
        assert_eq!(state(&host, "switch", r#"{ "item": "luz" }"#).unwrap().on, Some(false));
        addons.press("exemplo.contador", "switch", &settings(r#"{ "item": "luz" }"#)).unwrap();
        wait_for(&mut host, "o interruptor ligado", |host| state(host, "switch", r#"{ "item": "luz" }"#).unwrap().on == Some(true));
        assert!(!state(&host, "switch", "{}").unwrap().available, "no item chosen: dimmed");
        let error = addons.press("exemplo.contador", "switch", &Settings::new()).unwrap_err().to_string();
        assert_eq!(error, "Contador: escolha no editor o que ligar e desligar");
        addons.release("exemplo.contador", "switch", &settings(r#"{ "item": "luz" }"#));

        let options = addons.options("exemplo.contador", "switch", "item", &Settings::new()).unwrap();
        let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
        assert_eq!(values, ["luz", "som", "ventilador"]);
        assert_eq!(options[0].short.as_deref(), Some("Luz"));
        assert!(addons.options("exemplo.contador", "count", "step", &Settings::new()).is_err());

        // Keys that go away: the addon hears an empty list.
        host.watch(&[]);
        assert!(host.watching.is_empty());

        // Turned off: it stops, and its keys say nothing any more.
        host.apply(&[]);
        host.poll();
        assert!(host.running().is_empty() && host.states().is_empty());
        assert_eq!(host.statuses()[0].state, AddonState::Off);
        assert!(addons.press("exemplo.contador", "count", &Settings::new()).is_err());
        // Its count was kept in its data folder.
        let kept = std::fs::read_to_string(data_folder(&dir, "exemplo.contador").join("contagens.json")).unwrap();
        assert!(kept.contains(":2"), "{kept}");
    }

    #[test]
    fn an_addon_that_quits_starts_again_and_one_that_cannot_start_waits() {
        let quits = r#"{ "id": "sai.logo", "name": "Sai logo", "version": "1", "api": 1,
                         "run": ["node", "-e", "console.error('sem conexão'); process.exit(3)"] }"#;
        let missing = r#"{ "id": "nao.abre", "name": "Não abre", "version": "1", "api": 1, "run": ["programa-que-nao-existe-em-lugar-nenhum"] }"#;
        let dir = config_dir("restart", &[("a", quits, &[]), ("b", missing, &[]), ("c", "{", &[])]);
        let mut host = Host::new(&dir, None);
        host.apply(&["sai.logo".to_string(), "nao.abre".to_string(), "nao.instalado".to_string()]);
        wait_for(&mut host, "o addon fechar", |host| host.statuses()[0].state == AddonState::Restarting);
        let statuses = host.statuses();
        let error = statuses[0].error.clone().unwrap();
        assert!(error.contains("sem conexão") && error.contains("1 s"), "{error}");
        assert_eq!(statuses[1].state, AddonState::Stopped);
        assert!(statuses[1].error.as_deref().unwrap().contains("Não abriu"), "{:?}", statuses[1].error);
        assert_eq!(statuses[2].state, AddonState::Invalid);
        assert_eq!(statuses[2].folder, "c");
        // It starts again by itself after the pause, still counting the exit.
        wait_for(&mut host, "o addon abrir de novo", |host| {
            host.procs["sai.logo"].quick_exits == 1 && host.procs["sai.logo"].restart_at.is_none()
        });
        // Trying again starts afresh (and fails again here).
        host.retry("nao.abre");
        assert_eq!(host.statuses()[1].state, AddonState::Stopped);
    }

    /// An update from the catalog takes the place of the running addon, which
    /// starts again in its new version; removing one stops it and keeps its data.
    #[test]
    fn installs_updates_and_removes_addons() {
        let main = include_str!("../../../../docs/addons/exemplo/main.js");
        let example = include_str!("../../../../docs/addons/exemplo/addon.json");
        let dir = config_dir("install", &[("exemplo", example, &[("main.js", main)])]);
        let mut host = Host::new(&dir, None);
        host.apply(&["exemplo.contador".to_string()]);
        wait_for(&mut host, "o addon rodando", |host| host.running().contains("exemplo.contador"));
        let unpack = |files: &[(&str, &str)], manifest: &str| {
            let zip = catalog::zip_of(files);
            catalog::unpack(&zip, &catalog::entry_of(&zip, manifest), &staging(&dir)).unwrap()
        };
        let folders = |host: &Host| host.statuses().iter().map(|status| status.folder.clone()).collect::<Vec<_>>();

        let newer = example.replace(r#""version": "0.1.0""#, r#""version": "0.2.0""#);
        host.install(unpack(&[("contador/addon.json", &newer), ("contador/main.js", main)], &newer)).unwrap();
        assert_eq!(folders(&host), ["exemplo"], "in the same folder");
        assert_eq!(host.statuses()[0].version, "0.2.0");
        wait_for(&mut host, "a versão nova rodando", |host| host.running().contains("exemplo.contador"));

        // A new one gets a folder named after it, with a number when the name is taken.
        std::fs::create_dir_all(folder(&dir).join("outro.addon")).unwrap();
        let other = r#"{ "id": "outro.addon", "name": "Outro", "version": "1.0.0", "api": 1, "run": ["node"] }"#;
        host.install(unpack(&[("addon.json", other)], other)).unwrap();
        assert_eq!(folders(&host), ["exemplo", "outro.addon", "outro.addon-2"]);

        host.remove("exemplo").unwrap();
        assert!(host.running().is_empty());
        assert!(!folder(&dir).join("exemplo").exists());
        assert!(data_folder(&dir, "exemplo.contador").exists(), "its data stays");
        // A folder whose addon.json didn't load goes too.
        host.remove("outro.addon").unwrap();
        assert_eq!(folders(&host), ["outro.addon-2"]);
        assert!(host.remove("nao-existe").is_err());
        assert_eq!(std::fs::read_dir(staging(&dir)).unwrap().count(), 0, "nothing left aside");
    }

    #[test]
    fn images_must_be_in_the_addon_folders() {
        let manifest = r#"{ "id": "com.imagens", "name": "Imagens", "version": "1", "api": 1, "run": ["x"] }"#;
        let dir = config_dir("images", &[("imagens", manifest, &[("a.png", ""), ("b.txt", "")])]);
        let data = data_folder(&dir, "com.imagens");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("capa.svg"), "").unwrap();
        std::fs::write(dir.join("fora.png"), "").unwrap();
        let host = Host::new(&dir, None);
        let addons = host.handle();
        assert!(addons.image("com.imagens", "a.png").unwrap().ends_with("a.png"));
        assert!(addons.image("com.imagens", &data.join("capa.svg").display().to_string()).is_ok());
        let outside = dir.join("fora.png").display().to_string();
        for refused in ["b.txt", "../../fora.png", "nao-existe.png", outside.as_str()] {
            assert!(addons.image("com.imagens", refused).is_err(), "{refused}");
        }
        assert!(addons.image("nao.instalado", "a.png").is_err());
    }
}
