//! The visor (the wide screen): what it shows, drawing it when that
//! changes, and its tap and hold.

use super::*;

/// The visor's touch while it is down.
#[derive(Clone, Copy)]
pub(super) enum VisorPress {
    Up,
    /// `holds`: holding this visor does something, so a long press is not a tap.
    Down { since: Instant, holds: bool },
    /// The hold already ran; the release does nothing.
    Held,
}

impl State {
    /// The visor showing now, and the rule it comes from (the last active
    /// rule with a visor of its own wins, like keys). Its `content` is what
    /// the hold cycle reached.
    pub(super) fn current_screen(&self) -> (Screen, Option<usize>) {
        let (mut screen, rule) = self
            .active
            .iter()
            .rev()
            .find_map(|&i| self.config.rules[i].screen.clone().map(|s| (s, Some(i))))
            .unwrap_or_else(|| (self.config.default_screen(), None));
        let step = self.screen_steps.get(&self.screen_origin(rule)).copied().unwrap_or(0);
        screen.content = screen.showing(step);
        (screen, rule)
    }

    fn screen_origin(&self, rule: Option<usize>) -> Option<String> {
        rule.map(|i| self.config.rules[i].name.clone())
    }

    /// Draws the visor again if what it shows changed; true when it did.
    pub(super) fn redraw_screen(&mut self) -> bool {
        let (screen, _) = self.current_screen();
        if screen::uses_now_playing(&screen) && self.playing_rx.is_none() {
            self.playing_rx = Some(screen::watch_now_playing());
        }
        let data = screen::Data { now: chrono::Local::now(), usage: &self.usage, playing: self.playing.as_ref(), timer: &self.timer };
        let signature = screen::signature(&screen, &data);
        if signature == self.screen_signature {
            return false;
        }
        self.screen_png = screen::render(&screen, &data, &self.config_base())
            .map_err(|e| warn!("visor não desenhado: {e:#}"))
            .unwrap_or(None);
        self.screen_signature = signature;
        self.status_dirty = true;
        true
    }

    /// Fresh numbers and songs for the visor, then draws it if they show.
    pub(super) fn refresh_screen(&mut self, device: Option<&D200>) -> Result<()> {
        let (screen, _) = self.current_screen();
        let device_stats = screen.content == ScreenContent::DeviceStats;
        if (screen::uses_usage(&screen) || device_stats) && self.last_usage.elapsed() >= USAGE_POLL {
            self.usage = self.sampler.sample();
            self.last_usage = Instant::now();
        }
        if let Some(rx) = &self.playing_rx {
            while let Ok(playing) = rx.try_recv() {
                self.playing = playing;
            }
        }
        if self.redraw_screen() {
            self.sync_keys(device)?;
            if let Some(device) = device {
                // The visor may have switched between what the device and the app draw.
                self.keepalive(device)?;
            }
        }
        Ok(())
    }

    /// A tap on the visor: the timer's controls (two quick taps reset it), or the visor's action.
    pub(super) fn tap_screen(&mut self, device: &D200) -> Result<()> {
        let (screen, rule) = self.current_screen();
        if let ScreenContent::Timer { .. } = screen.content {
            let label = match self.last_timer_tap.take() {
                Some(at) if at.elapsed() <= DOUBLE_TAP => {
                    self.timer.reset();
                    "Timer zerado"
                }
                _ => {
                    self.timer.toggle();
                    self.last_timer_tap = Some(Instant::now());
                    if self.timer.running() { "Timer rodando" } else { "Timer pausado" }
                }
            };
            info!("visor: {}", label.to_lowercase());
            self.refresh_screen(Some(device))?;
            self.report_screen(label.to_string(), rule, None);
            return Ok(());
        }
        // Older configs put the visor's action on key 14.
        let fallback = || self.resolved.get(&(WINDOW_INDEX as u8 + 1)).and_then(|s| s.key.action.clone());
        let action = screen.action.clone().or_else(fallback);
        self.report_screen("Visor".to_string(), rule, action);
        Ok(())
    }

    /// Holding the visor: its own action, or the next content of its cycle.
    pub(super) fn hold_screen(&mut self, device: &D200) -> Result<()> {
        let (screen, rule) = self.current_screen();
        match screen.hold {
            Some(ScreenHold::Cycle { contents }) if !contents.is_empty() => {
                let origin = self.screen_origin(rule);
                let step = self.screen_steps.entry(origin).or_insert(0);
                *step = (*step + 1) % (contents.len() + 1);
                let name = self.current_screen().0.content.name();
                info!("visor: {name}");
                self.refresh_screen(Some(device))?;
                self.report_screen(format!("Visor: {name}"), rule, None);
            }
            Some(ScreenHold::Action { action: Some(action) }) => {
                self.report_screen("Visor (segurar)".to_string(), rule, Some(action));
            }
            _ => {}
        }
        Ok(())
    }

    /// Tells the app what the visor did, then runs the action, if any.
    fn report_screen(&self, label: String, rule: Option<usize>, action: Option<Action>) {
        let activity = Activity {
            number: WINDOW_INDEX as u8 + 1,
            label,
            rule: self.screen_origin(rule),
            action: action.as_ref().map(actions::describe),
            error: None,
        };
        (self.on_activity)(&activity);
        if let Some(action) = action {
            info!("visor: {}", actions::describe(&action));
            let sink = Arc::clone(&self.on_activity);
            actions::run(action, None, move |error| sink(&Activity { error: Some(error), ..activity }));
        }
    }
}

