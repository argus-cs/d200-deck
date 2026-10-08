//! Runs a key's action on Windows. Synthetic input (SendInput) does not
//! reach windows of apps running as administrator.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::browser::Bridge;
use crate::config::{Action, MediaKey};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyStroke {
    pub vk: u16,
    pub extended: bool,
}

/// Where an action must happen when its key has `"front": true`.
pub enum Front {
    /// A lowercase exe name.
    App(String),
    /// An Edge tab, and the tab to select again afterwards when Edge was
    /// already in front on another one.
    Tab { bridge: Bridge, tab: i64, back_to: Option<i64> },
}

/// Runs the action on its own thread so a slow app launch never delays the
/// device loop. With `front`, the app or tab is brought forward for the
/// action and the focus goes back afterwards.
pub fn run(action: Action, front: Option<Front>) {
    std::thread::spawn(move || {
        if let Err(e) = run_now(&action, front.as_ref()) {
            log::warn!("ação falhou: {e:#}");
        }
    });
}

#[cfg(windows)]
fn run_now(action: &Action, front: Option<&Front>) -> Result<()> {
    use std::time::Duration;

    use crate::context::{bring_to_front, give_back};

    let previous = match front {
        None => None,
        Some(Front::App(process)) => bring_to_front(process)?,
        Some(Front::Tab { bridge, tab, .. }) => {
            bridge.activate(*tab)?;
            bring_to_front(crate::rules::BROWSER)?
        }
    };
    if front.is_some() {
        // Let the app or page take the keyboard focus before the shortcut arrives.
        std::thread::sleep(Duration::from_millis(120));
    }
    let result = execute(action);
    if front.is_some() {
        std::thread::sleep(Duration::from_millis(120));
    }
    if let Some(Front::Tab { bridge, back_to: Some(tab), .. }) = front {
        let _ = bridge.activate(*tab);
    }
    if let Some(window) = previous {
        give_back(window);
    }
    result
}

#[cfg(not(windows))]
fn run_now(action: &Action, _front: Option<&Front>) -> Result<()> {
    execute(action)
}

pub fn describe(action: &Action) -> String {
    match action {
        Action::Hotkey { keys } => format!("atalho {keys}"),
        Action::Open { target, .. } => format!("abrir {target}"),
        Action::Command { command } => format!("comando {command}"),
        Action::Text { text } => format!("digitar {} caracteres", text.chars().count()),
        Action::Media { key } => format!("mídia {key:?}"),
    }
}

/// "Ctrl+Shift+M" → the virtual keys to press, modifiers first.
pub fn parse_hotkey(spec: &str) -> Result<Vec<KeyStroke>> {
    let parts: Vec<&str> = spec.split('+').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        bail!("atalho inválido: {spec:?}");
    }
    parts.iter().map(|p| key_from_name(p).ok_or_else(|| anyhow::anyhow!("tecla desconhecida {p:?} em {spec:?}"))).collect()
}

fn key_from_name(name: &str) -> Option<KeyStroke> {
    let plain = |vk| Some(KeyStroke { vk, extended: false });
    let ext = |vk| Some(KeyStroke { vk, extended: true });
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "ctrl" | "control" => plain(0x11),
        "shift" => plain(0x10),
        "alt" => plain(0x12),
        "win" | "windows" | "super" | "meta" => ext(0x5B),
        "enter" | "return" => plain(0x0D),
        "esc" | "escape" => plain(0x1B),
        "tab" => plain(0x09),
        "space" => plain(0x20),
        "backspace" => plain(0x08),
        "delete" | "del" => ext(0x2E),
        "insert" | "ins" => ext(0x2D),
        "home" => ext(0x24),
        "end" => ext(0x23),
        "pageup" | "pgup" => ext(0x21),
        "pagedown" | "pgdn" => ext(0x22),
        "left" => ext(0x25),
        "up" => ext(0x26),
        "right" => ext(0x27),
        "down" => ext(0x28),
        "printscreen" | "prtsc" => ext(0x2C),
        _ => {
            if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
                return if (1..=24).contains(&n) { plain(0x70 + n - 1) } else { None };
            }
            let mut chars = name.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => plain(c.to_ascii_uppercase() as u16),
                (Some(c), None) => layout_key(c),
                _ => None,
            }
        }
    }
}

/// Punctuation depends on the keyboard layout, so ask Windows.
#[cfg(windows)]
fn layout_key(c: char) -> Option<KeyStroke> {
    let mut buf = [0u16; 2];
    let unit = *c.encode_utf16(&mut buf).first()?;
    let scan = unsafe { windows::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW(unit) };
    // Low byte is the key; -1 means the layout has no such key. Shift state is left to the user.
    if scan == -1 { None } else { Some(KeyStroke { vk: (scan as u16) & 0xFF, extended: false }) }
}

#[cfg(not(windows))]
fn layout_key(_: char) -> Option<KeyStroke> {
    None
}

/// Finds a bare program name ("notepad.exe", "wt") in PATH, like a terminal does.
///
/// ShellExecute checks the App Paths registry before PATH, and the Store
/// registers its apps there with the real exe inside WindowsApps. Started
/// from that path a Store app runs outside its package: wt.exe opens
/// nothing and notepad.exe fails with a missing DLL. PATH has the right
/// entry points (System32's notepad.exe redirector, the WindowsApps aliases).
pub fn find_in_path(target: &str, path: Option<&OsStr>, pathext: Option<&OsStr>) -> Option<PathBuf> {
    if target.is_empty() || target.contains(['\\', '/', ':']) {
        return None;
    }
    let names: Vec<String> = if Path::new(target).extension().is_some() {
        vec![target.to_string()]
    } else {
        let exts = pathext.and_then(OsStr::to_str).unwrap_or(".COM;.EXE;.BAT;.CMD");
        exts.split(';').filter(|e| !e.is_empty()).map(|e| format!("{target}{}", e.to_ascii_lowercase())).collect()
    };
    std::env::split_paths(path?).find_map(|dir| {
        // symlink_metadata: the Store aliases are reparse points that can't be followed.
        names.iter().map(|n| dir.join(n)).find(|p| std::fs::symlink_metadata(p).is_ok())
    })
}

fn media_vk(key: MediaKey) -> u16 {
    match key {
        MediaKey::Mute => 0xAD,
        MediaKey::VolumeDown => 0xAE,
        MediaKey::VolumeUp => 0xAF,
        MediaKey::Next => 0xB0,
        MediaKey::Previous => 0xB1,
        MediaKey::Stop => 0xB2,
        MediaKey::PlayPause => 0xB3,
    }
}

#[cfg(windows)]
pub fn execute(action: &Action) -> Result<()> {
    match action {
        Action::Hotkey { keys } => win::press(&parse_hotkey(keys)?),
        Action::Media { key } => win::press(&[KeyStroke { vk: media_vk(*key), extended: false }]),
        Action::Text { text } => win::type_text(text),
        Action::Open { target, args } => win::open(target, args.as_deref()),
        Action::Command { command } => win::command(command),
    }
}

#[cfg(not(windows))]
pub fn execute(_: &Action) -> Result<()> {
    bail!("ações só funcionam no Windows")
}

#[cfg(windows)]
mod win {
    use std::os::windows::process::CommandExt;

    use anyhow::{bail, Result};
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::System::Com::{CoInitializeEx, COINIT, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_EXTENDEDKEY,
        KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, VIRTUAL_KEY,
    };
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    use super::KeyStroke;

    const VK_RETURN: u16 = 0x0D;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn input(vk: u16, scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT { wVk: VIRTUAL_KEY(vk), wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 },
            },
        }
    }

    fn stroke(s: &KeyStroke, up: bool) -> INPUT {
        let mut flags = KEYBD_EVENT_FLAGS(0);
        if s.extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if up {
            flags |= KEYEVENTF_KEYUP;
        }
        input(s.vk, 0, flags)
    }

    fn send(inputs: &[INPUT]) -> Result<()> {
        let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize != inputs.len() {
            bail!("o Windows aceitou {sent} de {} eventos de teclado (janela de administrador na frente?)", inputs.len());
        }
        Ok(())
    }

    /// Presses every key in order, then releases them in reverse order.
    pub fn press(keys: &[KeyStroke]) -> Result<()> {
        let inputs: Vec<INPUT> = keys.iter().map(|k| stroke(k, false)).chain(keys.iter().rev().map(|k| stroke(k, true))).collect();
        send(&inputs)
    }

    pub fn type_text(text: &str) -> Result<()> {
        let mut inputs = Vec::new();
        for unit in text.encode_utf16() {
            match unit {
                0x0D => {}
                0x0A => {
                    inputs.push(input(VK_RETURN, 0, KEYBD_EVENT_FLAGS(0)));
                    inputs.push(input(VK_RETURN, 0, KEYEVENTF_KEYUP));
                }
                _ => {
                    inputs.push(input(0, unit, KEYEVENTF_UNICODE));
                    inputs.push(input(0, unit, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP));
                }
            }
        }
        send(&inputs)
    }

    /// Opens an app, file, folder, URL or URI scheme the way Explorer would.
    /// Store apps (like wt.exe) are activated asynchronously: NOASYNC keeps
    /// this short-lived thread alive until the launch has really happened.
    pub fn open(target: &str, args: Option<&str>) -> Result<()> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT(COINIT_APARTMENTTHREADED.0 | COINIT_DISABLE_OLE1DDE.0));
        }
        let path = std::env::var_os("PATH");
        let pathext = std::env::var_os("PATHEXT");
        let file = match super::find_in_path(target, path.as_deref(), pathext.as_deref()) {
            Some(found) => HSTRING::from(found.as_os_str()),
            None => HSTRING::from(target),
        };
        let params = args.map(HSTRING::from);
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC,
            lpVerb: w!("open"),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: params.as_ref().map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
            bail!("o Windows não conseguiu abrir {target:?}: {}", e.message());
        }
        Ok(())
    }

    pub fn command(command: &str) -> Result<()> {
        std::process::Command::new("cmd.exe")
            .raw_arg(format!("/C {command}"))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vks(spec: &str) -> Vec<u16> {
        parse_hotkey(spec).unwrap().iter().map(|k| k.vk).collect()
    }

    #[test]
    fn parses_common_hotkeys() {
        assert_eq!(vks("Ctrl+Shift+M"), [0x11, 0x10, 0x4D]);
        assert_eq!(vks("Win + Alt + K"), [0x5B, 0x12, 0x4B]);
        assert_eq!(vks("F5"), [0x74]);
        assert_eq!(vks("ctrl+1"), [0x11, 0x31]);
        assert_eq!(vks("F24"), [0x87]);
    }

    #[test]
    fn navigation_keys_are_extended() {
        let keys = parse_hotkey("Ctrl+Left").unwrap();
        assert!(!keys[0].extended);
        assert!(keys[1].extended);
    }

    #[test]
    fn bare_names_are_found_in_path_order() {
        let root = std::env::temp_dir().join("deck-engine-path-test");
        let (first, second) = (root.join("first"), root.join("second"));
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        std::fs::write(first.join("tool.exe"), b"").unwrap();
        std::fs::write(second.join("tool.exe"), b"").unwrap();
        std::fs::write(second.join("script.cmd"), b"").unwrap();
        let path = std::env::join_paths([&first, &second]).unwrap();
        let ext = OsStr::new(".EXE;.CMD");

        assert_eq!(find_in_path("tool.exe", Some(&path), Some(ext)), Some(first.join("tool.exe")));
        assert_eq!(find_in_path("tool", Some(&path), Some(ext)), Some(first.join("tool.exe")));
        assert_eq!(find_in_path("script", Some(&path), Some(ext)), Some(second.join("script.cmd")));
        assert_eq!(find_in_path("missing.exe", Some(&path), Some(ext)), None);
    }

    #[test]
    fn paths_and_uris_are_left_alone() {
        let path = std::env::var_os("PATH");
        for target in ["spotify:", "https://example.com", r"C:\Windows\notepad.exe", "dir/app.exe", ""] {
            assert_eq!(find_in_path(target, path.as_deref(), None), None, "{target}");
        }
    }

    #[test]
    fn rejects_unknown_keys() {
        for spec in ["Ctrl+Nada", "", "Ctrl+", "F25", "F0"] {
            assert!(parse_hotkey(spec).is_err(), "{spec:?}");
        }
    }
}
