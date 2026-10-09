//! Windows settings a key can switch or run: Bluetooth, Wi-Fi, mute, the
//! theme, keeping the PC awake, the audio output, projection, power mode,
//! sleep, the monitor and the recycle bin. `state` reads the real setting,
//! so a key shows what Windows has rather than what the last press did.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Setting {
    // On or off.
    Bluetooth,
    Wifi,
    /// On: not muted.
    Microphone,
    /// On: not muted.
    Sound,
    DarkTheme,
    KeepAwake,
    // One value out of several.
    AudioOutput,
    Projection,
    PowerMode,
    // Run once.
    Sleep,
    MonitorOff,
    EmptyRecycleBin,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Switch {
    #[default]
    Toggle,
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    OnOff,
    /// The key's value is the current one or not (this output, extend…).
    Choice,
    /// No state to show.
    Once,
}

/// Win+P's four choices, in its order.
const PROJECTIONS: &[&str] = &["internal", "clone", "extend", "external"];
/// Settings → System → Power's "Power mode", in its order.
const POWER_MODES: &[&str] = &["efficiency", "balanced", "performance"];

impl Setting {
    pub const ALL: [Setting; 12] = [
        Setting::Bluetooth,
        Setting::Wifi,
        Setting::Microphone,
        Setting::Sound,
        Setting::DarkTheme,
        Setting::KeepAwake,
        Setting::AudioOutput,
        Setting::Projection,
        Setting::PowerMode,
        Setting::Sleep,
        Setting::MonitorOff,
        Setting::EmptyRecycleBin,
    ];

    pub fn kind(self) -> Kind {
        match self {
            Setting::Bluetooth
            | Setting::Wifi
            | Setting::Microphone
            | Setting::Sound
            | Setting::DarkTheme
            | Setting::KeepAwake => Kind::OnOff,
            Setting::AudioOutput | Setting::Projection | Setting::PowerMode => Kind::Choice,
            Setting::Sleep | Setting::MonitorOff | Setting::EmptyRecycleBin => Kind::Once,
        }
    }

    /// Fixed values of a Choice setting; empty for AudioOutput (any device
    /// name) and for the others.
    pub fn choices(self) -> &'static [&'static str] {
        match self {
            Setting::Projection => PROJECTIONS,
            Setting::PowerMode => POWER_MODES,
            _ => &[],
        }
    }

    fn name(self) -> &'static str {
        match self {
            Setting::Bluetooth => "Bluetooth",
            Setting::Wifi => "Wi-Fi",
            Setting::Microphone => "microfone",
            Setting::Sound => "som",
            Setting::DarkTheme => "tema escuro",
            Setting::KeepAwake => "manter acordado",
            Setting::AudioOutput => "saída de áudio",
            Setting::Projection => "projeção",
            Setting::PowerMode => "modo de energia",
            Setting::Sleep => "suspender",
            Setting::MonitorOff => "desligar o monitor",
            Setting::EmptyRecycleBin => "esvaziar a lixeira",
        }
    }
}

impl Switch {
    pub fn is_toggle(&self) -> bool {
        *self == Switch::Toggle
    }
}

/// Before anything reaches config.json: Choice settings need a value (one of
/// `choices()`, or any device name for AudioOutput); the others take none.
pub fn validate(setting: Setting, value: Option<&str>) -> Result<()> {
    let name = setting.name();
    let value = value.map(str::trim).filter(|v| !v.is_empty());
    match (setting.kind(), value) {
        (Kind::Choice, None) => bail!("{name}: falta escolher o valor"),
        (Kind::Choice, Some(v)) if !setting.choices().is_empty() && !setting.choices().contains(&v) => {
            bail!("{name}: valor desconhecido {v:?}")
        }
        (Kind::Choice, Some(_)) | (_, None) => Ok(()),
        (_, Some(v)) => bail!("{name} não usa valor ({v:?})"),
    }
}

/// "Bluetooth: alternar", "projeção: estender", "suspender".
pub fn describe(setting: Setting, set: Switch, value: Option<&str>) -> String {
    let name = setting.name();
    match setting.kind() {
        Kind::OnOff => {
            let set = match set {
                Switch::Toggle => "alternar",
                Switch::On => "ligar",
                Switch::Off => "desligar",
            };
            format!("{name}: {set}")
        }
        Kind::Choice => format!("{name}: {}", value.map_or("?", |v| value_label(v.trim()))),
        Kind::Once => name.to_string(),
    }
}

fn value_label(value: &str) -> &str {
    match value {
        "internal" => "só a tela do PC",
        "clone" => "duplicar",
        "extend" => "estender",
        "external" => "só a segunda tela",
        "efficiency" => "melhor eficiência",
        "balanced" => "equilibrado",
        "performance" => "melhor desempenho",
        // A device name.
        other => other,
    }
}

/// What an on/off setting should become. Toggling an unknown state turns it on.
fn wanted(set: Switch, now: Option<bool>) -> bool {
    match set {
        Switch::Toggle => !now.unwrap_or(false),
        Switch::On => true,
        Switch::Off => false,
    }
}

/// Runs it, blocking. `set` only matters for on/off settings.
#[cfg(windows)]
pub fn apply(setting: Setting, set: Switch, value: Option<&str>) -> Result<()> {
    win::apply(setting, set, value.map(str::trim).unwrap_or(""))
}

#[cfg(not(windows))]
pub fn apply(_: Setting, _: Switch, _: Option<&str>) -> Result<()> {
    bail!("ações só funcionam no Windows")
}

/// Some(true): on, or `value` is the current choice. Some(false): off, or
/// another choice is current. None: unknown or missing on this PC, and
/// always for settings that run once. Quick enough to call every second.
#[cfg(windows)]
pub fn state(setting: Setting, value: Option<&str>) -> Option<bool> {
    win::state(setting, value.map(str::trim))
}

#[cfg(not(windows))]
pub fn state(_: Setting, _: Option<&str>) -> Option<bool> {
    None
}

/// Friendly names of the active audio outputs, for the editor's picker.
#[cfg(windows)]
pub fn audio_outputs() -> Result<Vec<String>> {
    win::com();
    Ok(win::outputs()?.into_iter().map(|(_, name)| name).collect())
}

#[cfg(not(windows))]
pub fn audio_outputs() -> Result<Vec<String>> {
    Ok(Vec::new())
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::sync::{Mutex, OnceLock, PoisonError};
    use std::time::{Duration, Instant};

    use anyhow::{anyhow, bail, Result};
    use windows::core::{s, w, GUID, PCWSTR, PWSTR};
    use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
    use windows::Win32::Devices::Display::{
        GetDisplayConfigBufferSizes, QueryDisplayConfig, SetDisplayConfig, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
        DISPLAYCONFIG_TOPOLOGY_CLONE, DISPLAYCONFIG_TOPOLOGY_EXTEND, DISPLAYCONFIG_TOPOLOGY_EXTERNAL,
        DISPLAYCONFIG_TOPOLOGY_ID, DISPLAYCONFIG_TOPOLOGY_INTERNAL, QDC_DATABASE_CURRENT, SDC_APPLY, SDC_TOPOLOGY_CLONE,
        SDC_TOPOLOGY_EXTEND, SDC_TOPOLOGY_EXTERNAL, SDC_TOPOLOGY_INTERNAL, SET_DISPLAY_CONFIG_FLAGS,
    };
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Foundation::{CloseHandle, E_UNEXPECTED, ERROR_FILE_NOT_FOUND, HANDLE, LPARAM, LUID, WPARAM};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eCapture, eCommunications, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator,
        MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
    };
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows::Win32::System::Power::{
        PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired, PowerSetRequest,
        SetSuspendState,
    };
    use windows::Win32::System::Registry::{RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD};
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcessToken, POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
    };
    use windows::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND, SHQUERYRBINFO,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        PostMessageW, SendMessageTimeoutW, HWND_BROADCAST, SC_MONITORPOWER, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
        WM_SYSCOMMAND,
    };

    use super::{wanted, Setting, Switch};

    /// Joins the multithreaded apartment once per thread. A thread that
    /// already has an apartment gets RPC_E_CHANGED_MODE, and COM works there too.
    pub fn com() {
        thread_local!(static COM: () = unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        });
        COM.with(|_| {});
    }

    pub fn apply(setting: Setting, set: Switch, value: &str) -> Result<()> {
        com();
        match setting {
            Setting::Bluetooth => set_radio(RadioKind::Bluetooth, set),
            Setting::Wifi => set_radio(RadioKind::WiFi, set),
            Setting::Microphone => set_unmuted(eCapture, set),
            Setting::Sound => set_unmuted(eRender, set),
            Setting::DarkTheme => set_dark_theme(set),
            Setting::KeepAwake => set_keep_awake(set),
            Setting::AudioOutput => set_audio_output(value),
            Setting::Projection => set_projection(value),
            Setting::PowerMode => set_power_mode(value),
            Setting::Sleep => sleep(),
            Setting::MonitorOff => monitor_off(),
            Setting::EmptyRecycleBin => empty_recycle_bin(),
        }
    }

    pub fn state(setting: Setting, value: Option<&str>) -> Option<bool> {
        com();
        match setting {
            Setting::Bluetooth => radio_on(RadioKind::Bluetooth),
            Setting::Wifi => radio_on(RadioKind::WiFi),
            Setting::Microphone => unmuted(eCapture),
            Setting::Sound => unmuted(eRender),
            Setting::DarkTheme => dark_theme(),
            Setting::KeepAwake => Some(keeping_awake()),
            Setting::AudioOutput => is_default_output(value?),
            Setting::Projection => Some(current_topology()? == topology(value?)?.1),
            Setting::PowerMode => Some(current_power_mode()? == power_mode(value?)?),
            Setting::Sleep | Setting::MonitorOff | Setting::EmptyRecycleBin => None,
        }
    }

    // Radios. Listing them takes a while, so the list is kept; a kind that
    // was missing is looked for again now and then (a USB dongle plugged in).

    static RADIOS: Mutex<Option<(Instant, Vec<Radio>)>> = Mutex::new(None);
    const RADIO_RETRY: Duration = Duration::from_secs(30);

    fn radios(kind: RadioKind) -> Vec<Radio> {
        let of_kind = |list: &[Radio]| -> Vec<Radio> { list.iter().filter(|r| r.Kind().ok() == Some(kind)).cloned().collect() };
        let mut cache = RADIOS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, list)) = cache.as_ref() {
            let found = of_kind(list);
            if !found.is_empty() || at.elapsed() < RADIO_RETRY {
                return found;
            }
        }
        let list: Vec<Radio> = Radio::GetRadiosAsync()
            .and_then(|op| op.get())
            .map(|radios| radios.into_iter().collect())
            .unwrap_or_default();
        let found = of_kind(&list);
        *cache = Some((Instant::now(), list));
        found
    }

    fn forget_radios() {
        *RADIOS.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// On when any radio of the kind is on. None when there is none, or all
    /// are disabled in Device Manager (they can't be switched on from here).
    fn radio_on(kind: RadioKind) -> Option<bool> {
        let mut usable = false;
        let mut on = false;
        for radio in radios(kind) {
            match radio.State() {
                Ok(RadioState::On) => (usable, on) = (true, true),
                Ok(RadioState::Off) => usable = true,
                Ok(_) => {}
                // The adapter went away.
                Err(_) => {
                    forget_radios();
                    return None;
                }
            }
        }
        usable.then_some(on)
    }

    fn set_radio(kind: RadioKind, set: Switch) -> Result<()> {
        let name = if kind == RadioKind::Bluetooth { "Bluetooth" } else { "Wi-Fi" };
        let radios = radios(kind);
        if radios.is_empty() {
            bail!("este PC não tem {name}");
        }
        let on = wanted(set, radio_on(kind));
        let access = Radio::RequestAccessAsync()?.get()?;
        if access != RadioAccessStatus::Allowed {
            bail!("o Windows não deixou mudar o {name}{}", denied(access));
        }
        let state = if on { RadioState::On } else { RadioState::Off };
        for radio in radios {
            let status = radio.SetStateAsync(state)?.get()?;
            if status != RadioAccessStatus::Allowed {
                bail!("o Windows não deixou mudar o {name}{}", denied(status));
            }
        }
        Ok(())
    }

    fn denied(status: RadioAccessStatus) -> &'static str {
        match status {
            RadioAccessStatus::DeniedByUser => " (bloqueado em Privacidade → Rádios)",
            RadioAccessStatus::DeniedBySystem => " (bloqueado pelo sistema, modo avião?)",
            _ => "",
        }
    }

    // Mute, on the default devices.

    fn enumerator() -> Result<IMMDeviceEnumerator> {
        Ok(unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? })
    }

    fn endpoint_volume(device: &IMMDevice) -> Result<IAudioEndpointVolume> {
        Ok(unsafe { device.Activate(CLSCTX_ALL, None)? })
    }

    fn device_id(device: &IMMDevice) -> Result<String> {
        unsafe {
            let id = device.GetId()?;
            let text = id.to_string();
            CoTaskMemFree(Some(id.0 as *const c_void));
            Ok(text?)
        }
    }

    fn unmuted(flow: EDataFlow) -> Option<bool> {
        let device = unsafe { enumerator().ok()?.GetDefaultAudioEndpoint(flow, eConsole).ok()? };
        let muted = unsafe { endpoint_volume(&device).ok()?.GetMute().ok()? };
        Some(!muted.as_bool())
    }

    /// The microphone is muted on the communications device too when that is
    /// a different one, so calls go quiet as well.
    fn set_unmuted(flow: EDataFlow, set: Switch) -> Result<()> {
        let enumerator = enumerator()?;
        let console = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) }.map_err(|_| {
            anyhow!(if flow == eCapture { "o Windows não tem microfone padrão" } else { "o Windows não tem saída de som padrão" })
        })?;
        let mute = !wanted(set, unmuted(flow));
        let mut devices = vec![console];
        if flow == eCapture {
            if let Ok(communications) = unsafe { enumerator.GetDefaultAudioEndpoint(flow, eCommunications) } {
                if device_id(&communications)? != device_id(&devices[0])? {
                    devices.push(communications);
                }
            }
        }
        for device in &devices {
            unsafe { endpoint_volume(device)?.SetMute(mute, std::ptr::null())? };
        }
        Ok(())
    }

    // The theme: what Settings → Personalization → Colors writes.

    const PERSONALIZE: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");

    fn dark_theme() -> Option<bool> {
        let mut light = 1u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let read = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PERSONALIZE,
                w!("AppsUseLightTheme"),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut light as *mut u32 as *mut c_void),
                Some(&mut size),
            )
        };
        // No value yet: Windows' default, the light theme.
        if read == ERROR_FILE_NOT_FOUND {
            return Some(false);
        }
        read.ok().ok()?;
        Some(light == 0)
    }

    fn set_dark_theme(set: Switch) -> Result<()> {
        let light = u32::from(!wanted(set, dark_theme()));
        for name in [w!("AppsUseLightTheme"), w!("SystemUsesLightTheme")] {
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    PERSONALIZE,
                    name,
                    REG_DWORD.0,
                    Some(&light as *const u32 as *const c_void),
                    std::mem::size_of::<u32>() as u32,
                )
            }
            .ok()?;
        }
        // Tells the taskbar and open apps to repaint; a hung window is skipped.
        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                WPARAM(0),
                LPARAM(w!("ImmersiveColorSet").as_ptr() as isize),
                SMTO_ABORTIFHUNG,
                200,
                None,
            );
        }
        Ok(())
    }

    // Keeping awake: a power request lives as long as its handle, so it
    // outlasts the short-lived action thread (SetThreadExecutionState wouldn't).

    /// The power request's handle while the PC is kept awake.
    static KEEP_AWAKE: Mutex<Option<usize>> = Mutex::new(None);

    fn keeping_awake() -> bool {
        KEEP_AWAKE.lock().unwrap_or_else(PoisonError::into_inner).is_some()
    }

    fn set_keep_awake(set: Switch) -> Result<()> {
        let mut request = KEEP_AWAKE.lock().unwrap_or_else(PoisonError::into_inner);
        let on = wanted(set, Some(request.is_some()));
        match (on, *request) {
            (true, None) => unsafe {
                let reason = REASON_CONTEXT {
                    Version: 0, // POWER_REQUEST_CONTEXT_VERSION
                    Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                    Reason: REASON_CONTEXT_0 { SimpleReasonString: PWSTR(w!("D200 Deck: manter o PC acordado").as_ptr() as *mut u16) },
                };
                let handle = PowerCreateRequest(&reason)?;
                let set = PowerSetRequest(handle, PowerRequestDisplayRequired)
                    .and_then(|()| PowerSetRequest(handle, PowerRequestSystemRequired));
                if let Err(e) = set {
                    let _ = CloseHandle(handle);
                    return Err(e.into());
                }
                *request = Some(handle.0 as usize);
            },
            (false, Some(raw)) => unsafe {
                let handle = HANDLE(raw as *mut c_void);
                let _ = PowerClearRequest(handle, PowerRequestDisplayRequired);
                let _ = PowerClearRequest(handle, PowerRequestSystemRequired);
                let _ = CloseHandle(handle);
                *request = None;
            },
            _ => {}
        }
        Ok(())
    }

    // The default audio output.

    /// Undocumented: the interface the Sound control panel uses to change the
    /// default device, which EarTrumpet and SoundSwitch rely on too. Only
    /// SetDefaultEndpoint is called; the methods before it keep its place.
    mod policy {
        #![allow(non_snake_case)]

        use windows::core::{GUID, HRESULT, HSTRING, PCWSTR};
        use windows::Win32::Media::Audio::{eCommunications, eConsole, eMultimedia, ERole};
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

        const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

        /// Makes the device the default for every role, like the Sound panel does.
        pub fn set_default(device_id: &str) -> windows::core::Result<()> {
            let id = HSTRING::from(device_id);
            unsafe {
                let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
                for role in [eConsole, eMultimedia, eCommunications] {
                    policy.SetDefaultEndpoint(PCWSTR(id.as_ptr()), role).ok()?;
                }
            }
            Ok(())
        }

        #[windows::core::interface("f8679f50-850a-41cf-9c72-430f290290c8")]
        unsafe trait IPolicyConfig: windows::core::IUnknown {
            fn GetMixFormat(&self) -> HRESULT;
            fn GetDeviceFormat(&self) -> HRESULT;
            fn ResetDeviceFormat(&self) -> HRESULT;
            fn SetDeviceFormat(&self) -> HRESULT;
            fn GetProcessingPeriod(&self) -> HRESULT;
            fn SetProcessingPeriod(&self) -> HRESULT;
            fn GetShareMode(&self) -> HRESULT;
            fn SetShareMode(&self) -> HRESULT;
            fn GetPropertyValue(&self) -> HRESULT;
            fn SetPropertyValue(&self) -> HRESULT;
            fn SetDefaultEndpoint(&self, device_id: PCWSTR, role: ERole) -> HRESULT;
            fn SetEndpointVisibility(&self) -> HRESULT;
        }
    }

    /// (id, friendly name) of each active output.
    pub fn outputs() -> Result<Vec<(String, String)>> {
        unsafe {
            let devices = enumerator()?.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)?;
            (0..devices.GetCount()?)
                .map(|i| {
                    let device = devices.Item(i)?;
                    Ok((device_id(&device)?, friendly_name(&device)?))
                })
                .collect()
        }
    }

    fn friendly_name(device: &IMMDevice) -> Result<String> {
        unsafe {
            let store = device.OpenPropertyStore(STGM_READ)?;
            let mut value = store.GetValue(&PKEY_Device_FriendlyName)?;
            let text = PropVariantToStringAlloc(&value);
            let _ = PropVariantClear(&mut value);
            let text = text?;
            let name = text.to_string();
            CoTaskMemFree(Some(text.0 as *const c_void));
            Ok(name?)
        }
    }

    fn output_id(name: &str) -> Result<Option<String>> {
        let name = name.to_lowercase();
        Ok(outputs()?.into_iter().find(|(_, n)| n.to_lowercase() == name).map(|(id, _)| id))
    }

    fn is_default_output(name: &str) -> Option<bool> {
        let id = output_id(name).ok()??;
        let default = unsafe { enumerator().ok()?.GetDefaultAudioEndpoint(eRender, eConsole).ok()? };
        Some(device_id(&default).ok()? == id)
    }

    fn set_audio_output(name: &str) -> Result<()> {
        let id = output_id(name)?.ok_or_else(|| anyhow!("nenhuma saída de áudio ativa se chama {name:?}"))?;
        Ok(policy::set_default(&id)?)
    }

    // Projection: what Win+P does.

    fn topology(value: &str) -> Option<(SET_DISPLAY_CONFIG_FLAGS, DISPLAYCONFIG_TOPOLOGY_ID)> {
        match value {
            "internal" => Some((SDC_TOPOLOGY_INTERNAL, DISPLAYCONFIG_TOPOLOGY_INTERNAL)),
            "clone" => Some((SDC_TOPOLOGY_CLONE, DISPLAYCONFIG_TOPOLOGY_CLONE)),
            "extend" => Some((SDC_TOPOLOGY_EXTEND, DISPLAYCONFIG_TOPOLOGY_EXTEND)),
            "external" => Some((SDC_TOPOLOGY_EXTERNAL, DISPLAYCONFIG_TOPOLOGY_EXTERNAL)),
            _ => None,
        }
    }

    fn current_topology() -> Option<DISPLAYCONFIG_TOPOLOGY_ID> {
        unsafe {
            let (mut paths, mut modes) = (0u32, 0u32);
            GetDisplayConfigBufferSizes(QDC_DATABASE_CURRENT, &mut paths, &mut modes).ok().ok()?;
            let mut path_info = vec![DISPLAYCONFIG_PATH_INFO::default(); paths as usize];
            let mut mode_info = vec![DISPLAYCONFIG_MODE_INFO::default(); modes as usize];
            let mut current = DISPLAYCONFIG_TOPOLOGY_ID::default();
            QueryDisplayConfig(
                QDC_DATABASE_CURRENT,
                &mut paths,
                path_info.as_mut_ptr(),
                &mut modes,
                mode_info.as_mut_ptr(),
                Some(&mut current),
            )
            .ok()
            .ok()?;
            Some(current)
        }
    }

    fn set_projection(value: &str) -> Result<()> {
        let Some((flag, _)) = topology(value) else {
            bail!("projeção desconhecida {value:?}");
        };
        let code = unsafe { SetDisplayConfig(None, None, SDC_APPLY | flag) };
        if code != 0 {
            bail!("o Windows não mudou a projeção (código {code}; há uma segunda tela ligada?)");
        }
        Ok(())
    }

    // Power mode. Settings' "Power mode" is an overlay on the Balanced plan,
    // switched by undocumented powrprof exports. They are looked up at run
    // time, so a Windows without them only loses this setting.

    type GetOverlay = unsafe extern "system" fn(*mut GUID) -> u32;
    type SetOverlay = unsafe extern "system" fn(GUID) -> u32;

    fn overlay() -> Option<(GetOverlay, SetOverlay)> {
        static FUNCTIONS: OnceLock<Option<(GetOverlay, SetOverlay)>> = OnceLock::new();
        *FUNCTIONS.get_or_init(|| unsafe {
            let module = LoadLibraryW(w!("powrprof.dll")).ok()?;
            let get = GetProcAddress(module, s!("PowerGetEffectiveOverlayScheme"))?;
            let set = GetProcAddress(module, s!("PowerSetActiveOverlayScheme"))?;
            Some((std::mem::transmute::<_, GetOverlay>(get), std::mem::transmute::<_, SetOverlay>(set)))
        })
    }

    fn power_mode(value: &str) -> Option<GUID> {
        match value {
            "efficiency" => Some(GUID::from_u128(0x961cc777_2547_4f9d_8174_7d86181b8a7a)),
            "balanced" => Some(GUID::zeroed()),
            "performance" => Some(GUID::from_u128(0xded574b5_45a0_4f42_8737_46345c09c238)),
            _ => None,
        }
    }

    fn current_power_mode() -> Option<GUID> {
        let (get, _) = overlay()?;
        let mut current = GUID::zeroed();
        (unsafe { get(&mut current) } == 0).then_some(current)
    }

    fn set_power_mode(value: &str) -> Result<()> {
        let mode = power_mode(value).ok_or_else(|| anyhow!("modo de energia desconhecido {value:?}"))?;
        let (_, set) = overlay().ok_or_else(|| anyhow!("este Windows não troca o modo de energia"))?;
        let code = unsafe { set(mode) };
        if code != 0 {
            bail!("o Windows não mudou o modo de energia (código {code})");
        }
        Ok(())
    }

    // Run once.

    fn sleep() -> Result<()> {
        enable_shutdown_privilege()?;
        if !unsafe { SetSuspendState(false, false, false) } {
            bail!("o Windows não suspendeu: {}", windows::core::Error::from_win32().message());
        }
        Ok(())
    }

    /// SetSuspendState needs it, and processes start with it disabled.
    fn enable_shutdown_privilege() -> Result<()> {
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token)?;
            let mut luid = LUID::default();
            let result = LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid).and_then(|()| {
                let privileges = TOKEN_PRIVILEGES {
                    PrivilegeCount: 1,
                    Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
                };
                AdjustTokenPrivileges(token, false, Some(&privileges), 0, None, None)
            });
            let _ = CloseHandle(token);
            Ok(result?)
        }
    }

    fn monitor_off() -> Result<()> {
        // Let the finger leave the key first, in case its release counts as
        // input that wakes the screen.
        std::thread::sleep(Duration::from_millis(500));
        // Posted, not sent: a hung window can't hold the action up.
        unsafe { PostMessageW(Some(HWND_BROADCAST), WM_SYSCOMMAND, WPARAM(SC_MONITORPOWER as usize), LPARAM(2))? };
        Ok(())
    }

    fn empty_recycle_bin() -> Result<()> {
        unsafe {
            let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
            if SHQueryRecycleBinW(PCWSTR::null(), &mut info).is_ok() && info.i64NumItems == 0 {
                return Ok(());
            }
            if let Err(e) = SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) {
                // What an already empty bin answers on some Windows versions.
                if e.code() != E_UNEXPECTED {
                    bail!("o Windows não esvaziou a lixeira: {}", e.message());
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_in_config_json() {
        let cases = [
            (Setting::Bluetooth, "bluetooth"),
            (Setting::Wifi, "wifi"),
            (Setting::DarkTheme, "dark_theme"),
            (Setting::KeepAwake, "keep_awake"),
            (Setting::AudioOutput, "audio_output"),
            (Setting::PowerMode, "power_mode"),
            (Setting::MonitorOff, "monitor_off"),
            (Setting::EmptyRecycleBin, "empty_recycle_bin"),
        ];
        for (setting, name) in cases {
            assert_eq!(serde_json::to_value(setting).unwrap(), name);
        }
        for setting in Setting::ALL {
            let json = serde_json::to_string(&setting).unwrap();
            assert_eq!(serde_json::from_str::<Setting>(&json).unwrap(), setting);
        }
        for (set, name) in [(Switch::Toggle, "toggle"), (Switch::On, "on"), (Switch::Off, "off")] {
            assert_eq!(serde_json::to_value(set).unwrap(), name);
            assert_eq!(serde_json::from_value::<Switch>(name.into()).unwrap(), set);
        }
        assert!(Switch::default().is_toggle());
    }

    #[test]
    fn only_choices_take_a_value() {
        for setting in Setting::ALL {
            let fixed = !setting.choices().is_empty();
            assert_eq!(fixed, matches!(setting, Setting::Projection | Setting::PowerMode), "{setting:?}");
            if fixed {
                assert_eq!(setting.kind(), Kind::Choice);
            }
        }
        assert_eq!(Setting::AudioOutput.kind(), Kind::Choice);
        assert_eq!(Setting::Microphone.kind(), Kind::OnOff);
        assert_eq!(Setting::Sleep.kind(), Kind::Once);
    }

    #[test]
    fn validates_values() {
        assert!(validate(Setting::Bluetooth, None).is_ok());
        assert!(validate(Setting::Sleep, Some("  ")).is_ok());
        assert!(validate(Setting::Bluetooth, Some("on")).is_err());
        assert!(validate(Setting::Projection, Some("extend")).is_ok());
        assert!(validate(Setting::Projection, Some("sideways")).is_err());
        assert!(validate(Setting::Projection, None).is_err());
        assert!(validate(Setting::PowerMode, Some("performance")).is_ok());
        assert!(validate(Setting::AudioOutput, Some("Fone (Jabra)")).is_ok());
        assert!(validate(Setting::AudioOutput, Some("")).is_err());
    }

    #[test]
    fn describes_for_the_activity_log() {
        assert_eq!(describe(Setting::Bluetooth, Switch::Toggle, None), "Bluetooth: alternar");
        assert_eq!(describe(Setting::Wifi, Switch::On, None), "Wi-Fi: ligar");
        assert_eq!(describe(Setting::AudioOutput, Switch::Toggle, Some("Fone")), "saída de áudio: Fone");
        assert_eq!(describe(Setting::Projection, Switch::Toggle, Some("extend")), "projeção: estender");
        assert_eq!(describe(Setting::Sleep, Switch::Toggle, None), "suspender");
    }

    #[test]
    fn toggling_flips_and_unknown_turns_on() {
        assert!(!wanted(Switch::Toggle, Some(true)));
        assert!(wanted(Switch::Toggle, Some(false)));
        assert!(wanted(Switch::Toggle, None));
        assert!(wanted(Switch::On, Some(true)));
        assert!(!wanted(Switch::Off, None));
    }
}
