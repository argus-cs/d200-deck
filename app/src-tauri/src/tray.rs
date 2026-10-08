// Notification-area icon: status, open, pause rules, resend, start with Windows, quit.

use deck_engine::runtime::{Command, Status};
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::{show_window, DeckState};

const TRAY_ID: &str = "main";

/// Menu items whose text or check mark follows the engine.
struct TrayItems {
    device: MenuItem<Wry>,
    rules: MenuItem<Wry>,
    pause: CheckMenuItem<Wry>,
    autostart: CheckMenuItem<Wry>,
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let device = MenuItem::with_id(app, "device", "D200 desconectado", false, None::<&str>)?;
    let rules = MenuItem::with_id(app, "rules", "Nenhuma regra valendo", false, None::<&str>)?;
    let open = MenuItem::with_id(app, "open", "Abrir D200 Deck", true, None::<&str>)?;
    let pause = CheckMenuItem::with_id(app, "pause", "Pausar regras", true, false, None::<&str>)?;
    let resend = MenuItem::with_id(app, "resend", "Reenviar layout para o D200", true, None::<&str>)?;
    let starts_with_windows = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Iniciar com o Windows", true, starts_with_windows, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&device, &rules, &sep1, &open, &pause, &resend, &autostart, &sep2, &quit])?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("D200 Deck")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app: &AppHandle, event| match event.id.as_ref() {
            "open" => show_window(app),
            "pause" => {
                // The check mark has already flipped when the event arrives.
                if let Some(items) = app.try_state::<TrayItems>() {
                    let paused = items.pause.is_checked().unwrap_or(false);
                    app.state::<DeckState>().send(Command::Pause(paused));
                }
            }
            "resend" => app.state::<DeckState>().send(Command::Resend),
            "autostart" => toggle_autostart(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    app.manage(TrayItems { device, rules, pause, autostart });
    Ok(())
}

fn toggle_autostart(app: &AppHandle) {
    let launcher = app.autolaunch();
    let enabled = launcher.is_enabled().unwrap_or(false);
    let result = if enabled { launcher.disable() } else { launcher.enable() };
    if let Err(e) = result {
        log::warn!("início com o Windows não mudou: {e}");
    }
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.autostart.set_checked(launcher.is_enabled().unwrap_or(false));
    }
}

pub fn update(app: &AppHandle, status: &Status) {
    let Some(items) = app.try_state::<TrayItems>() else {
        return;
    };
    let device = if status.device { "D200 conectado" } else { "D200 desconectado" };
    let active: Vec<&str> = status.rules.iter().filter(|r| r.active).map(|r| r.name.as_str()).collect();
    let rules = if status.paused {
        "Regras pausadas".to_string()
    } else if active.is_empty() {
        "Nenhuma regra valendo".to_string()
    } else {
        format!("Valendo: {}", active.join(", "))
    };
    let _ = items.device.set_text(device);
    let _ = items.rules.set_text(&rules);
    let _ = items.pause.set_checked(status.paused);
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(format!("D200 Deck · {device} · {rules}")));
    }
}
