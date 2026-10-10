//! What each key shows and does: the faces of two-state keys, states that
//! change on their own (apps running, Windows settings), folders, the images
//! sent to the device, and key presses.

use super::*;

/// Which two-state keys show their second face: (rule name, folder key,
/// key number), `None` being the default layout and no folder. By name so
/// reordering rules keeps it.
pub(super) type ToggleId = (Option<String>, Option<u8>, u8);

/// A folder showing on the device in place of the layout.
pub(super) struct OpenFolder {
    /// Where its key is, by rule name, so a config reload finds it again.
    pub(super) rule: Option<String>,
    pub(super) number: u8,
    /// The last press, to close a forgotten folder.
    pub(super) touched: Instant,
}

/// What keys show besides their config: states that change on their own.
#[derive(Default)]
pub(super) struct Live {
    /// Two-state keys showing their second face after a press.
    pub(super) toggled: HashSet<ToggleId>,
    /// Running apps that "open app" keys start (lowercase exe names).
    pub(super) apps_running: HashSet<String>,
    /// Windows settings keys show, as last read (`None`: not on this PC).
    pub(super) settings: HashMap<SettingId, Option<bool>>,
}

impl Live {
    /// `Some` for a key that shows a Windows setting: its state, if known.
    pub(super) fn setting(&self, key: &Key) -> Option<Option<bool>> {
        let id = key.action.as_ref()?.watched_setting()?;
        Some(self.settings.get(&id).copied().flatten())
    }

    /// A key showing a Windows setting has its second face while the setting
    /// is off, whoever turned it off; any other one after an odd number of presses.
    pub(super) fn toggled(&self, config: &Config, slot: &Slot, number: u8) -> bool {
        if slot.key.toggle.is_none() {
            return false;
        }
        match self.setting(&slot.key) {
            Some(state) => state == Some(false),
            None => self.toggled.contains(&toggle_id(config, slot, number)),
        }
    }

    /// Darkened: an "open app" key whose app is closed, a setting this PC
    /// lacks, or a setting that is off on a key with no second face for it.
    pub(super) fn dim(&self, key: &Key, face: &Face) -> bool {
        let closed = face.action.and_then(Action::watched_process).is_some_and(|process| !self.apps_running.contains(&process));
        let setting = match self.setting(key) {
            Some(None) => true,
            Some(Some(false)) => key.toggle.is_none(),
            _ => false,
        };
        closed || setting
    }
}

/// What a key looks like and does right now.
pub(super) struct Face<'a> {
    pub(super) appearance: Appearance<'a>,
    pub(super) action: Option<&'a Action>,
}

pub(super) fn face(key: &Key, toggled: bool) -> Face<'_> {
    match (&key.toggle, toggled) {
        (Some(second), true) => Face { appearance: second.appearance(), action: second.action.as_ref().or(key.action.as_ref()) },
        _ => Face { appearance: key.appearance(), action: key.action.as_ref() },
    }
}

pub(super) fn toggle_id(config: &Config, slot: &Slot, number: u8) -> ToggleId {
    (slot.rule.map(|i| config.rules[i].name.clone()), slot.folder, number)
}

impl State {
    /// Every key that may show soon: the layers' (beaten ones too), those in
    /// their folders and the open folder's, so opening a folder or switching
    /// rules never shows a key in a stale state.
    pub(super) fn keys_in_view(&self) -> impl Iterator<Item = &Key> {
        // Right after a reload `active` still points into the old rules: skip what is gone.
        let rules = self.active.iter().filter_map(|&i| self.config.rules.get(i)).flat_map(|rule| rule.keys.values());
        let layers = self.config.keys.values().chain(rules);
        let open = self.open_folder().map(|(folder, ..)| folder.keys.values());
        layers.flat_map(|key| std::iter::once(key).chain(key.folder.iter().flat_map(|f| f.keys.values()))).chain(open.into_iter().flatten())
    }

    /// What each key does now: the open folder's keys, or the rule layers'.
    pub(super) fn layout(&self) -> BTreeMap<u8, Slot> {
        match self.open_folder() {
            Some((folder, rule, number)) => resolve_folder(folder, rule, number),
            None => resolve(&self.config, &self.active),
        }
    }

    /// The open folder as the config has it now: the folder, its rule's index
    /// and its key's number. `None` once its key is gone from the config.
    pub(super) fn open_folder(&self) -> Option<(&Folder, Option<usize>, u8)> {
        let (key, rule) = self.open_folder_key()?;
        Some((key.folder.as_ref()?, rule, self.folder.as_ref()?.number))
    }

    pub(super) fn open_folder_key(&self) -> Option<(&Key, Option<usize>)> {
        let open = self.folder.as_ref()?;
        match &open.rule {
            None => Some((self.config.keys.get(&open.number)?, None)),
            Some(name) => {
                let i = self.config.rules.iter().position(|r| &r.name == name)?;
                Some((self.config.rules[i].keys.get(&open.number)?, Some(i)))
            }
        }
    }

    pub(super) fn sync_keys(&mut self, device: Option<&D200>) -> Result<()> {
        if self.folder.is_some() && self.open_folder().is_none() {
            info!("pasta fechada: ela saiu da config");
            self.folder = None;
        }
        self.resolved = self.layout();
        self.read_new_settings();
        // A rule with its own visor may have started or stopped.
        self.redraw_screen();
        self.status_dirty = true;
        let Some(device) = device else {
            // Nothing on the device to keep in sync: send everything on connect.
            self.sent.clear();
            return Ok(());
        };
        let base = self.config_base();
        let mut views = build_views(&self.resolved, &base, &mut self.icons, &self.config, &self.live);
        // The visor is not a key: it shows the app's drawing, or nothing when the device draws.
        views.insert(WINDOW_INDEX, KeyView { text: String::new(), png: self.screen_png.clone() });
        let changed = changed_keys(&self.sent, &views);
        if self.sent.is_empty() {
            let size = device.set_layout(&views, false)?;
            info!("layout enviado ({size} bytes)");
        } else if !changed.is_empty() {
            let size = device.set_layout(&changed, true)?;
            let numbers: Vec<String> = changed.keys().map(|i| (i + 1).to_string()).collect();
            let (noun, verb) = if numbers.len() == 1 { ("tecla", "atualizada") } else { ("teclas", "atualizadas") };
            info!("{noun} {} {verb} ({size} bytes)", numbers.join(", "));
        }
        self.sent = views;
        Ok(())
    }

    pub(super) fn press(&mut self, index: usize, device: &D200) -> Result<()> {
        let number = index as u8 + 1;
        let slot = self.resolved.get(&number).cloned();
        let rule = slot.as_ref().and_then(|s| s.rule).map(|i| &self.config.rules[i]);
        let toggled = slot.as_ref().is_some_and(|s| self.live.toggled(&self.config, s, number));
        let current = slot.as_ref().map(|s| face(&s.key, toggled));
        let in_folder = self.folder.is_some();
        let back = in_folder && number == BACK_KEY;
        let opens = slot.as_ref().is_some_and(|s| s.key.folder.is_some());
        let described = match () {
            _ if back => Some("voltar".to_string()),
            _ if opens => Some("abrir pasta".to_string()),
            _ => current.as_ref().and_then(|f| f.action).map(actions::describe),
        };
        let activity = Activity {
            number,
            label: current.as_ref().map(|f| f.appearance.label.to_string()).unwrap_or_default(),
            rule: rule.map(|r| r.name.clone()),
            action: described,
            error: None,
        };
        (self.on_activity)(&activity);
        if let Some(open) = &mut self.folder {
            open.touched = Instant::now();
        }
        let action = current.and_then(|f| f.action.cloned());
        let Some(slot) = slot else {
            info!("tecla {number}: vazia");
            return Ok(());
        };
        let origin = rule.map_or_else(|| "padrão".to_string(), |r| r.name.clone());
        if back {
            info!("pasta fechada");
            self.folder = None;
            return self.sync_keys(Some(device));
        }
        if opens {
            info!("tecla {number} ({origin}): pasta aberta");
            let rule = rule.map(|r| r.name.clone());
            self.folder = Some(OpenFolder { rule, number, touched: Instant::now() });
            return self.sync_keys(Some(device));
        }
        let ran = action.is_some();
        match action {
            None => info!("tecla {number}: sem ação"),
            Some(action) => {
                let front = match rule.filter(|_| slot.key.front) {
                    None => None,
                    Some(rule) => match self.front_for(rule) {
                        Some(front) => Some(front),
                        // No tab of the site to send it to: running anyway would land
                        // the action on whatever window is in front (text typed into a
                        // terminal), so nothing happens, toggles and folders included.
                        None => {
                            let error = "nenhuma aba aberta do site para receber a ação".to_string();
                            (self.on_activity)(&Activity { error: Some(error), ..activity });
                            return Ok(());
                        }
                    },
                };
                let place = match &front {
                    Some(Front::App(process)) => format!(" em {process}"),
                    Some(Front::Tab { tab, .. }) => format!(" na aba {tab} do Edge"),
                    None => String::new(),
                };
                info!("tecla {number} ({origin}): {}{place}", actions::describe(&action));
                if action.watched_setting().is_some() {
                    self.settings_fast_until = Some(Instant::now() + SETTING_FAST_FOR);
                }
                let sink = Arc::clone(&self.on_activity);
                actions::run(action, front, move |error| sink(&Activity { error: Some(error), ..activity }));
            }
        }
        let mut changed = false;
        // A key showing a Windows setting follows the setting, not the presses.
        if slot.key.toggle.is_some() && self.live.setting(&slot.key).is_none() {
            let id = toggle_id(&self.config, &slot, number);
            if !self.live.toggled.remove(&id) {
                self.live.toggled.insert(id);
            }
            changed = true;
        }
        // Picking something in a folder closes it, unless it stays open.
        if ran && in_folder && !self.open_folder().is_some_and(|(folder, ..)| folder.stay) {
            info!("pasta fechada");
            self.folder = None;
            changed = true;
        }
        if changed {
            self.sync_keys(Some(device))?;
        }
        Ok(())
    }

    fn front_for(&self, rule: &Rule) -> Option<Front> {
        if let Some(process) = rule.process() {
            return Some(Front::App(process));
        }
        let Some(tab) = matching_tab(rule, &self.context) else {
            warn!("{}: nenhuma aba aberta do site para receber a ação", rule.name);
            return None;
        };
        // Edge in front on another tab: select that tab again afterwards.
        let edge_in_front = self.context.focused.as_deref() == Some(BROWSER);
        let back_to = self.context.tabs.active.as_ref().filter(|t| edge_in_front && t.id != tab).map(|t| t.id);
        Some(Front::Tab { bridge: self.browser.clone(), tab, back_to })
    }
}

fn build_views(
    resolved: &BTreeMap<u8, Slot>,
    base: &Path,
    cache: &mut IconCache,
    config: &Config,
    live: &Live,
) -> BTreeMap<usize, KeyView> {
    let default_text = icons::default_text_color(&config.label);
    (0..KEY_COUNT)
        .map(|index| {
            let number = index + 1;
            let view = match resolved.get(&(number as u8)) {
                None => KeyView::default(),
                Some(slot) => {
                    let current = face(&slot.key, live.toggled(config, slot, number as u8));
                    // The label is drawn in the image, so each key can have its own colors.
                    let look = icons::Look::new(&current.appearance, &config.label, &default_text, live.dim(&slot.key, &current));
                    KeyView { text: String::new(), png: cached_icon(cache, &look, base, number) }
                }
            };
            (index, view)
        })
        .collect()
}

pub(super) fn cached_icon(cache: &mut IconCache, look: &icons::Look, base: &Path, number: usize) -> Option<Vec<u8>> {
    cache
        .entry(look.cache_key())
        .or_insert_with(|| icons::render_key(look, base).map_err(|e| warn!("tecla {number}: imagem ignorada: {e:#}")).ok())
        .clone()
}

fn changed_keys(old: &BTreeMap<usize, KeyView>, new: &BTreeMap<usize, KeyView>) -> BTreeMap<usize, KeyView> {
    new.iter().filter(|(i, view)| old.get(i) != Some(view)).map(|(i, view)| (*i, view.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_changed_keys_are_resent() {
        let view = |t: &str| KeyView { text: t.into(), png: None };
        let old = BTreeMap::from([(0, view("a")), (1, view("b")), (2, view("c"))]);
        let new = BTreeMap::from([(0, view("a")), (1, view("B")), (2, view("c"))]);
        assert_eq!(changed_keys(&old, &new), BTreeMap::from([(1, view("B"))]));
    }

    #[test]
    fn two_state_keys_show_their_second_face_when_toggled() {
        let mut config = Config::default();
        let mic = config.keys.get_mut(&1).unwrap();
        mic.toggle = Some(crate::config::KeyFace {
            label: "Mutado".into(),
            icon: Some("micOff".into()),
            color: "#3A1616".into(),
            ..Default::default()
        });
        let resolved = resolve(&config, &[]);
        let mut cache = IconCache::new();
        let mut live = Live::default();
        let off = build_views(&resolved, Path::new("."), &mut cache, &config, &live);
        live.toggled.insert((None, None, 1));
        let on = build_views(&resolved, Path::new("."), &mut cache, &config, &live);
        // Labels are drawn into the images, so the second face is a different image.
        assert_eq!(on[&0].text, "");
        assert_ne!(on[&0].png, off[&0].png);
        // The second face has no action of its own: it repeats the key's.
        assert_eq!(face(&resolved[&1].key, true).action, resolved[&1].key.action.as_ref());
    }

    #[test]
    fn default_config_renders_every_key() {
        let config = Config::default();
        let mut cache = IconCache::new();
        let views = build_views(&resolve(&config, &[]), Path::new("."), &mut cache, &config, &Live::default());
        assert_eq!(views.len(), KEY_COUNT);
        assert!(views[&0].png.is_some());
        assert_eq!(views[&12], KeyView::default());
        // Keys sharing an icon and color share one rendering.
        assert!(cache.len() <= config.keys.len());
    }

    fn bluetooth_key(second_face: bool) -> Key {
        let action = Action::System { setting: system::Setting::Bluetooth, set: system::Switch::Toggle, value: None };
        let toggle = second_face.then(|| crate::config::KeyFace { icon: Some("bluetoothOff".into()), ..Default::default() });
        Key { icon: Some("bluetooth".into()), action: Some(action), toggle, ..Key::default() }
    }

    #[test]
    fn setting_keys_show_the_real_state() {
        let config = Config::default();
        let id = (system::Setting::Bluetooth, None);
        let slot = Slot { key: bluetooth_key(true), rule: None, folder: None };
        let mut live = Live::default();
        // Presses don't count: only the setting does.
        live.toggled.insert((None, None, 1));
        live.settings.insert(id.clone(), Some(true));
        assert!(!live.toggled(&config, &slot, 1));
        live.settings.insert(id.clone(), Some(false));
        assert!(live.toggled(&config, &slot, 1), "off shows the second face");
        assert!(!live.dim(&slot.key, &face(&slot.key, true)));
        live.settings.insert(id.clone(), None);
        assert!(live.dim(&slot.key, &face(&slot.key, false)), "no Bluetooth on this PC: dimmed");
        // With no second face, off is shown by dimming.
        let plain = bluetooth_key(false);
        live.settings.insert(id, Some(false));
        assert!(live.dim(&plain, &face(&plain, false)));
    }

    #[test]
    fn toggles_in_a_folder_are_kept_apart_from_the_layout() {
        let mut config = Config::default();
        let mut mic = config.keys[&1].clone();
        mic.toggle = Some(crate::config::KeyFace::default());
        config.keys.insert(1, mic.clone());
        let layout = Slot { key: mic.clone(), rule: None, folder: None };
        let inside = Slot { key: mic, rule: None, folder: Some(5) };
        let mut live = Live::default();
        live.toggled.insert(toggle_id(&config, &layout, 1));
        assert!(live.toggled(&config, &layout, 1));
        assert!(!live.toggled(&config, &inside, 1));
    }
}
