//! What the rules need from Windows: which app is in front (an event, no
//! polling), which apps are running, and bringing an app to the front.

use std::collections::HashSet;
use std::sync::mpsc::Receiver;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

pub struct ProcessList {
    sys: System,
}

impl Default for ProcessList {
    fn default() -> Self {
        Self { sys: System::new() }
    }
}

impl ProcessList {
    /// Lowercase exe names of every running process.
    pub fn running(&mut self) -> HashSet<String> {
        self.sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        self.sys.processes().values().map(|p| p.name().to_string_lossy().to_ascii_lowercase()).collect()
    }
}

/// Sends the lowercase exe name of the foreground app now and on every change.
#[cfg(windows)]
pub fn watch_foreground() -> Receiver<Option<String>> {
    win::watch_foreground()
}

#[cfg(not(windows))]
pub fn watch_foreground() -> Receiver<Option<String>> {
    std::sync::mpsc::channel().1
}

#[cfg(windows)]
pub use win::{bring_to_front, give_back, visible_apps};

#[cfg(not(windows))]
pub fn visible_apps() -> Vec<(String, String)> {
    Vec::new()
}

#[cfg(windows)]
mod win {
    use std::sync::mpsc::{channel, Receiver, Sender};
    use std::sync::OnceLock;

    use anyhow::{bail, Result};
    use windows::core::{BOOL, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        AttachThreadInput, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MENU,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, DispatchMessageW, EnumWindows, GetForegroundWindow, GetMessageW, GetWindow,
        GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
        SetForegroundWindow, ShowWindow, EVENT_SYSTEM_FOREGROUND, GW_OWNER, MSG, SW_RESTORE, WINEVENT_OUTOFCONTEXT,
    };

    static FOREGROUND: OnceLock<Sender<Option<String>>> = OnceLock::new();

    pub fn watch_foreground() -> Receiver<Option<String>> {
        let (tx, rx) = channel();
        let _ = tx.send(process_of(unsafe { GetForegroundWindow() }));
        if FOREGROUND.set(tx).is_err() {
            log::warn!("watch_foreground chamado duas vezes; só o primeiro recebe eventos");
            return rx;
        }
        std::thread::spawn(|| unsafe {
            // The hook calls back on this thread, so it needs a message loop.
            let hook = SetWinEventHook(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND, None, Some(on_foreground), 0, 0, WINEVENT_OUTOFCONTEXT);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                DispatchMessageW(&msg);
            }
            let _ = UnhookWinEvent(hook);
        });
        rx
    }

    unsafe extern "system" fn on_foreground(_: HWINEVENTHOOK, _: u32, hwnd: HWND, _: i32, _: i32, _: u32, _: u32) {
        if let Some(tx) = FOREGROUND.get() {
            let _ = tx.send(process_of(hwnd));
        }
    }

    fn process_of(hwnd: HWND) -> Option<String> {
        exe_name(hwnd).map(|name| name.to_ascii_lowercase())
    }

    /// The exe file name of the window's process, as spelled on disk.
    fn exe_name(hwnd: HWND) -> Option<String> {
        if hwnd.is_invalid() {
            return None;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == 0 {
            return None;
        }
        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let result = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
            let _ = CloseHandle(handle);
            result.ok()?;
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            path.rsplit('\\').next().map(str::to_string)
        }
    }

    /// A top-level window a person would call "the app": visible, titled, not a dialog of another window.
    fn is_app_window(hwnd: HWND) -> bool {
        unsafe {
            let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|owner| !owner.is_invalid());
            IsWindowVisible(hwnd).as_bool() && !owned && GetWindowTextLengthW(hwnd) > 0
        }
    }

    unsafe extern "system" fn collect_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let apps = &mut *(lparam.0 as *mut Vec<(String, String)>);
        if is_app_window(hwnd) {
            if let Some(exe) = exe_name(hwnd) {
                let mut buf = [0u16; 512];
                let len = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
                let title = String::from_utf16_lossy(&buf[..len]);
                if !apps.iter().any(|(known, _)| known.eq_ignore_ascii_case(&exe)) {
                    apps.push((exe, title));
                }
            }
        }
        BOOL(1)
    }

    /// Apps with a window open now, front to back: (exe name, window title).
    pub fn visible_apps() -> Vec<(String, String)> {
        let mut apps: Vec<(String, String)> = Vec::new();
        unsafe {
            let _ = EnumWindows(Some(collect_window), LPARAM(&mut apps as *mut _ as isize));
        }
        apps
    }

    struct Search<'a> {
        process: &'a str,
        found: Option<HWND>,
    }

    unsafe extern "system" fn match_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam.0 as *mut Search);
        if is_app_window(hwnd) && process_of(hwnd).as_deref() == Some(search.process) {
            search.found = Some(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }

    fn find_window(process: &str) -> Option<HWND> {
        let mut search = Search { process, found: None };
        unsafe {
            let _ = EnumWindows(Some(match_window), LPARAM(&mut search as *mut Search as isize));
        }
        search.found
    }

    fn tap_alt() {
        let key = |flags| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_MENU, wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
        };
        unsafe {
            SendInput(&[key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)], std::mem::size_of::<INPUT>() as i32);
        }
    }

    fn set_foreground(hwnd: HWND) -> bool {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            // Windows lets only the app the user is typing in change the foreground.
            // Borrowing its input queue for a moment is the quiet way in...
            let current = GetForegroundWindow();
            let theirs = GetWindowThreadProcessId(current, None);
            let ours = GetCurrentThreadId();
            let attached = theirs != 0 && theirs != ours && AttachThreadInput(ours, theirs, true).as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            if attached {
                let _ = AttachThreadInput(ours, theirs, false);
            }
            if GetForegroundWindow() == hwnd {
                return true;
            }
            // ...and an Alt tap, which counts as our input, is the fallback.
            tap_alt();
            let _ = SetForegroundWindow(hwnd);
            GetForegroundWindow() == hwnd
        }
    }

    /// Brings `process` (lowercase exe name) to the front. Returns the window
    /// that was in front, or `None` when the app already was.
    pub fn bring_to_front(process: &str) -> Result<Option<HWND>> {
        let Some(target) = find_window(process) else {
            bail!("nenhuma janela visível de {process}");
        };
        let previous = unsafe { GetForegroundWindow() };
        if previous == target {
            return Ok(None);
        }
        if !set_foreground(target) {
            bail!("o Windows não deixou trazer {process} para a frente");
        }
        Ok(Some(previous))
    }

    pub fn give_back(previous: HWND) {
        if !previous.is_invalid() {
            set_foreground(previous);
        }
    }
}
