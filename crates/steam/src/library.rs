//! What Steam has on this PC: where it is, the games in each of its library
//! folders, whether one is running, and their cover art for a key.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use image::imageops::FilterType;
use image::ImageFormat;

use crate::vdf;

/// A key's image, in pixels (the device's).
pub const KEY_SIZE: u32 = 196;

/// Apps that install with games but aren't one.
const NOT_GAMES: &[&str] = &[
    // Steamworks Common Redistributables.
    "228980",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub id: String,
    pub name: String,
    /// Its install folder (`steamapps\common\…`), to find its window.
    pub folder: PathBuf,
}

/// The installed games of every library folder, by name.
pub fn games(steam: &Path) -> Vec<Game> {
    let mut libraries = vec![steam.to_path_buf()];
    if let Ok(text) = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")) {
        match vdf::parse(&text) {
            Ok(file) => {
                let folders = file.get("libraryfolders").map(vdf::Vdf::entries).unwrap_or_default();
                libraries.extend(folders.iter().filter_map(|(_, folder)| folder.text("path")).map(PathBuf::from));
            }
            Err(e) => eprintln!("libraryfolders.vdf ignorado: {e:#}"),
        }
    }
    let mut seen = Vec::new();
    libraries.retain(|library| {
        let key = library.to_string_lossy().to_lowercase().replace('/', "\\");
        let new = !seen.contains(&key);
        seen.push(key);
        new
    });
    let mut games: Vec<Game> = libraries.iter().flat_map(|library| library_games(library)).collect();
    games.sort_by_key(|game| game.name.to_lowercase());
    games.dedup_by(|a, b| a.id == b.id);
    games
}

fn library_games(library: &Path) -> Vec<Game> {
    let apps = library.join("steamapps");
    let Ok(entries) = std::fs::read_dir(&apps) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            name.starts_with("appmanifest_") && name.ends_with(".acf")
        })
        .filter_map(|path| {
            let state = vdf::parse(&std::fs::read_to_string(&path).ok()?).ok()?.get("AppState")?.clone();
            let id = state.text("appid")?.to_string();
            let name = state.text("name")?.trim().to_string();
            // Bit 4: fully installed (not only being downloaded).
            let installed = state.text("StateFlags").and_then(|flags| flags.parse::<u32>().ok()).is_some_and(|flags| flags & 4 != 0);
            if !installed || name.is_empty() || NOT_GAMES.contains(&id.as_str()) {
                return None;
            }
            let folder = apps.join("common").join(state.text("installdir").unwrap_or_default());
            Some(Game { id, name, folder })
        })
        .collect()
}

/// A shorter name for the key: "Hades II: Edição X" → "Hades II".
pub fn short_name(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !matches!(c, '™' | '®' | '©')).collect();
    let cut = [":", " - ", " – "].iter().filter_map(|sep| clean.find(sep)).min().map_or(clean.as_str(), |at| &clean[..at]);
    let cut = cut.trim();
    if cut.chars().count() >= 3 { cut.to_string() } else { clean.trim().to_string() }
}

/// Where Steam keeps a piece of a game's art: a folder per game in newer
/// versions, flat names (`<id>_header.jpg`) in older ones.
fn art(steam: &Path, id: &str, name: &str) -> Option<PathBuf> {
    let cache = steam.join("appcache").join("librarycache");
    [cache.join(id).join(name), cache.join(format!("{id}_{name}"))].into_iter().find(|path| path.is_file())
}

/// The game's cover cut to a square (its middle), as a key-sized PNG in
/// `dir`. Made again only when Steam has newer art.
pub fn cover(steam: &Path, id: &str, dir: &Path) -> Result<PathBuf> {
    let source = ["library_600x900.jpg", "library_600x900_2x.jpg", "header.jpg"]
        .iter()
        .find_map(|name| art(steam, id, name))
        .ok_or_else(|| anyhow!("a Steam não tem a capa do jogo {id}"))?;
    let target = dir.join(format!("{id}.png"));
    let modified = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    if modified(&target).is_some_and(|made| modified(&source).is_some_and(|art| made >= art)) {
        return Ok(target);
    }
    let image = image::open(&source).with_context(|| format!("não consegui abrir {}", source.display()))?;
    let side = image.width().min(image.height());
    let square = image.crop_imm((image.width() - side) / 2, (image.height() - side) / 2, side, side);
    let key = square.resize_exact(KEY_SIZE, KEY_SIZE, FilterType::Lanczos3);
    std::fs::create_dir_all(dir)?;
    // Written beside and moved, so the app never copies half a file.
    let partial = dir.join(format!("{id}.png.part"));
    key.save_with_format(&partial, ImageFormat::Png)?;
    std::fs::rename(&partial, &target)?;
    Ok(target)
}

/// Steam's folder, from the registry, if Steam is installed.
#[cfg(windows)]
pub fn steam_dir() -> Option<PathBuf> {
    use registry::{string, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let path = string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
        .or_else(|| string(HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"))?;
    let dir = PathBuf::from(path.replace('/', "\\"));
    dir.join("steam.exe").is_file().then_some(dir)
}

#[cfg(not(windows))]
pub fn steam_dir() -> Option<PathBuf> {
    None
}

/// Whether Steam says the game runs now.
#[cfg(windows)]
pub fn running(id: &str) -> bool {
    registry::dword(registry::HKEY_CURRENT_USER, &format!(r"Software\Valve\Steam\Apps\{id}"), "Running") == Some(1)
}

#[cfg(not(windows))]
pub fn running(_id: &str) -> bool {
    false
}

#[cfg(windows)]
mod registry {
    use std::ffi::c_void;

    use windows::core::HSTRING;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};
    pub use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    pub fn dword(root: HKEY, key: &str, value: &str) -> Option<u32> {
        let mut data = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        let read = unsafe {
            RegGetValueW(
                root,
                &HSTRING::from(key),
                &HSTRING::from(value),
                RRF_RT_REG_DWORD,
                None,
                Some(&mut data as *mut u32 as *mut c_void),
                Some(&mut size),
            )
        };
        read.ok().ok().map(|()| data)
    }

    pub fn string(root: HKEY, key: &str, value: &str) -> Option<String> {
        let (key, value) = (HSTRING::from(key), HSTRING::from(value));
        let mut size = 0u32;
        unsafe { RegGetValueW(root, &key, &value, RRF_RT_REG_SZ, None, None, Some(&mut size)) }.ok().ok()?;
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        unsafe { RegGetValueW(root, &key, &value, RRF_RT_REG_SZ, None, Some(buffer.as_mut_ptr().cast()), Some(&mut size)) }
            .ok()
            .ok()?;
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A Steam folder with two libraries: two games, a download in progress,
    /// the redistributables and a manifest that doesn't parse.
    pub(crate) fn fake_steam(name: &str) -> PathBuf {
        let steam = std::env::temp_dir().join(format!("steam-addon-{name}"));
        let _ = std::fs::remove_dir_all(&steam);
        let other = steam.join("outra-biblioteca");
        let manifest = |library: &Path, id: &str, name: &str, flags: u32| {
            let apps = library.join("steamapps");
            std::fs::create_dir_all(&apps).unwrap();
            let text = format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{id}\"\n\t\"name\"\t\t\"{name}\"\n\t\"StateFlags\"\t\t\"{flags}\"\n\t\"installdir\"\t\t\"{name}\"\n}}\n");
            std::fs::write(apps.join(format!("appmanifest_{id}.acf")), text).unwrap();
        };
        manifest(&steam, "1509960", "PICO PARK", 4);
        manifest(&steam, "228980", "Steamworks Common Redistributables", 4);
        manifest(&other, "1145360", "Hades™: Edição Especial", 1030);
        manifest(&other, "999", "Baixando", 1026);
        std::fs::write(steam.join("steamapps").join("appmanifest_5.acf"), "\"AppState\" {").unwrap();
        let libraries = format!(
            "\"libraryfolders\"\n{{\n\t\"0\" {{ \"path\" \"{}\" }}\n\t\"1\" {{ \"path\" \"{}\" }}\n}}\n",
            steam.display().to_string().replace('\\', "\\\\"),
            other.display().to_string().replace('\\', "\\\\"),
        );
        std::fs::write(steam.join("steamapps").join("libraryfolders.vdf"), libraries).unwrap();
        steam
    }

    #[test]
    fn lists_the_installed_games_of_every_library() {
        let steam = fake_steam("games");
        let games = games(&steam);
        let names: Vec<&str> = games.iter().map(|game| game.name.as_str()).collect();
        assert_eq!(names, ["Hades™: Edição Especial", "PICO PARK"]);
        assert_eq!(games[1].id, "1509960");
        assert_eq!(games[1].folder, steam.join("steamapps").join("common").join("PICO PARK"));
        assert!(super::games(&steam.join("nao-existe")).is_empty());
    }

    #[test]
    fn short_names_fit_a_key() {
        assert_eq!(short_name("Hades™: Edição Especial"), "Hades");
        assert_eq!(short_name("PICO PARK"), "PICO PARK");
        assert_eq!(short_name("Half-Life 2 - Episode One"), "Half-Life 2");
        assert_eq!(short_name("X: Rebirth"), "X: Rebirth", "too short to cut");
    }

    #[test]
    fn covers_become_square_key_images() {
        let steam = fake_steam("covers");
        let art = steam.join("appcache").join("librarycache");
        std::fs::create_dir_all(art.join("1509960")).unwrap();
        // A tall cover in the newer layout, a wide header in the older one.
        image::RgbImage::from_pixel(300, 450, image::Rgb([200, 30, 30])).save(art.join("1509960").join("library_600x900.jpg")).unwrap();
        image::RgbImage::from_pixel(460, 215, image::Rgb([30, 30, 200])).save(art.join("1145360_header.jpg")).unwrap();
        let out = steam.join("capas");
        for id in ["1509960", "1145360"] {
            let png = cover(&steam, id, &out).unwrap();
            let image = image::open(&png).unwrap();
            assert_eq!((image.width(), image.height()), (KEY_SIZE, KEY_SIZE), "{id}");
        }
        // Made once: the second time it is the same file.
        let first = std::fs::metadata(out.join("1509960.png")).unwrap().modified().unwrap();
        cover(&steam, "1509960", &out).unwrap();
        assert_eq!(std::fs::metadata(out.join("1509960.png")).unwrap().modified().unwrap(), first);
        assert!(cover(&steam, "999", &out).is_err(), "no art");
    }
}
