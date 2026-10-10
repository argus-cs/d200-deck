//! More icons from Iconify, the open library icones.js.org browses (some 240
//! sets). Nothing ships with the app: a set is downloaded once, when opened,
//! and kept on disk; the editor asks for one page of it at a time, filtered
//! here, so even emoji sets of tens of megabytes never reach the window
//! whole. A chosen icon is saved as an SVG beside the other key images, so
//! the device never needs the network.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::Value;

const API: &str = "https://api.iconify.design";
/// Each set is also an npm package, which this CDN serves compressed.
const CDN: &str = "https://cdn.jsdelivr.net/npm/@iconify-json";
/// The list of sets changes rarely.
const LIST_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Emoji sets reach tens of megabytes.
const MAX_DOWNLOAD: u64 = 128 * 1024 * 1024;
/// Sets kept parsed in memory, the last opened first.
const KEEP_SETS: usize = 3;
const SEARCH_LIMIT: u32 = 120;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct IconSet {
    pub prefix: String,
    pub name: String,
    pub total: u64,
    pub category: String,
    pub author: String,
    /// The license's name ("MIT", "CC BY 4.0").
    pub license: String,
    /// The license asks for credit to the author (CC BY).
    pub attribution: bool,
    /// The icons have their own colors: a key's icon color doesn't apply.
    pub palette: bool,
    /// A few icon names that show what the set looks like.
    pub samples: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Icon {
    pub name: String,
    /// A whole SVG document; one-color icons paint with `currentColor`.
    pub svg: String,
}

#[derive(Debug, Serialize)]
pub struct Page {
    /// Icons matching the filter, of which `icons` is one page.
    pub total: usize,
    pub icons: Vec<Icon>,
}

type Loaded = Mutex<Vec<(String, Arc<Vec<Icon>>)>>;

fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| Mutex::new(Vec::new()))
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(30))).build().into())
}

fn download(url: &str) -> Result<String> {
    let mut response = agent().get(url).header("User-Agent", "D200Deck").call().with_context(|| format!("não consegui baixar {url}"))?;
    response.body_mut().with_config().limit(MAX_DOWNLOAD).read_to_string().with_context(|| format!("resposta inválida de {url}"))
}

/// Where downloads are kept: beside config.json.
fn cache(base: &Path) -> PathBuf {
    base.join("iconify")
}

/// Written beside and renamed over, so a half-written file is never read.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Every set, by Iconify's categories. Downloaded at most once a week; an
/// older copy serves while offline.
pub fn sets(base: &Path) -> Result<Vec<IconSet>> {
    let path = cache(base).join("collections.json");
    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    let text = match age {
        Some(age) if age < LIST_MAX_AGE => std::fs::read_to_string(&path)?,
        _ => match download(&format!("{API}/collections")) {
            Ok(text) => {
                write_atomic(&path, &text)?;
                text
            }
            Err(e) => std::fs::read_to_string(&path).map_err(|_| e)?,
        },
    };
    parse_sets(&text)
}

fn parse_sets(text: &str) -> Result<Vec<IconSet>> {
    let list: serde_json::Map<String, Value> = serde_json::from_str(text).context("lista de coleções inválida")?;
    let str_at = |v: &Value, path: &[&str]| path.iter().try_fold(v, |v, k| v.get(*k)).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut sets: Vec<IconSet> = list
        .into_iter()
        .filter(|(_, info)| !info.get("hidden").and_then(Value::as_bool).unwrap_or(false))
        .map(|(prefix, info)| IconSet {
            name: str_at(&info, &["name"]),
            total: info.get("total").and_then(Value::as_u64).unwrap_or(0),
            category: str_at(&info, &["category"]),
            author: str_at(&info, &["author", "name"]),
            license: str_at(&info, &["license", "title"]),
            attribution: str_at(&info, &["license", "spdx"]).starts_with("CC-BY"),
            palette: info.get("palette").and_then(Value::as_bool).unwrap_or(false),
            samples: info
                .get("samples")
                .and_then(Value::as_array)
                .map(|s| s.iter().filter_map(Value::as_str).take(3).map(str::to_string).collect())
                .unwrap_or_default(),
            prefix,
        })
        .collect();
    sets.sort_by(|a, b| (category_rank(&a.category), &a.category, &a.name).cmp(&(category_rank(&b.category), &b.category, &b.name)));
    Ok(sets)
}

/// Iconify's own order (its list comes in it, but JSON objects lose order
/// here); categories it adds later go before the unmaintained sets.
const CATEGORIES: &[&str] = &[
    "Material",
    "UI 24px",
    "UI 16px / 32px",
    "UI Other / Mixed Grid",
    "UI Multicolor",
    "Programming",
    "Logos",
    "Emoji",
    "Flags / Maps",
    "Thematic",
];

fn category_rank(category: &str) -> usize {
    match CATEGORIES.iter().position(|c| *c == category) {
        Some(rank) => rank,
        None if category.starts_with("Archive") => CATEGORIES.len() + 1,
        None => CATEGORIES.len(),
    }
}

/// One page of a set's icons whose names have every word of `filter`.
/// The first call for a set downloads it (or reads it from disk).
pub fn icons(base: &Path, prefix: &str, filter: &str, offset: usize, limit: usize) -> Result<Page> {
    let set = open_set(base, prefix)?;
    let words: Vec<String> = filter.split_whitespace().map(str::to_lowercase).collect();
    let matching: Vec<&Icon> = set.iter().filter(|icon| words.iter().all(|w| icon.name.contains(w.as_str()))).collect();
    let icons = matching.iter().skip(offset).take(limit).map(|icon| (*icon).clone()).collect();
    Ok(Page { total: matching.len(), icons })
}

fn open_set(base: &Path, prefix: &str) -> Result<Arc<Vec<Icon>>> {
    check_part(prefix)?;
    if let Some(set) = remembered(prefix) {
        return Ok(set);
    }
    let path = cache(base).join("sets").join(format!("{prefix}.json"));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => {
            let text = download(&format!("{CDN}/{prefix}/icons.json"))?;
            write_atomic(&path, &text)?;
            text
        }
    };
    let set = Arc::new(parse_set(&text)?);
    let mut loaded = loaded().lock().unwrap();
    loaded.insert(0, (prefix.to_string(), Arc::clone(&set)));
    loaded.truncate(KEEP_SETS);
    Ok(set)
}

fn remembered(prefix: &str) -> Option<Arc<Vec<Icon>>> {
    let mut loaded = loaded().lock().unwrap();
    let at = loaded.iter().position(|(p, _)| p == prefix)?;
    let entry = loaded.remove(at);
    let set = Arc::clone(&entry.1);
    loaded.insert(0, entry);
    Some(set)
}

/// The icons of an IconifyJSON document as SVG files. Sizes default to the
/// set's (16 when absent). Aliases and hidden icons are left out.
fn parse_set(text: &str) -> Result<Vec<Icon>> {
    let json: Value = serde_json::from_str(text).context("coleção inválida")?;
    let number = |v: &Value, key: &str, default: f64| v.get(key).and_then(Value::as_f64).unwrap_or(default);
    let (width, height) = (number(&json, "width", 16.0), number(&json, "height", 16.0));
    let (left, top) = (number(&json, "left", 0.0), number(&json, "top", 0.0));
    let icons = json.get("icons").and_then(Value::as_object).ok_or_else(|| anyhow!("coleção sem ícones"))?;
    Ok(icons
        .iter()
        .filter(|(_, icon)| !icon.get("hidden").and_then(Value::as_bool).unwrap_or(false))
        .filter_map(|(name, icon)| {
            let body = icon.get("body")?.as_str()?;
            let view_box = format!(
                "{} {} {} {}",
                number(icon, "left", left),
                number(icon, "top", top),
                number(icon, "width", width),
                number(icon, "height", height)
            );
            let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}">{body}</svg>"#);
            Some(Icon { name: name.clone(), svg })
        })
        .collect())
}

/// Icon ids ("mdi:bluetooth") across every set, as Iconify's search finds them.
pub fn search(query: &str) -> Result<Vec<String>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let text = download(&format!("{API}/search?query={}&limit={SEARCH_LIMIT}", encode(query)))?;
    let json: Value = serde_json::from_str(&text).context("busca inválida")?;
    Ok(json.get("icons").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default())
}

/// Saves an icon ("mdi:bluetooth", as icones.js.org copies it) into the icons
/// folder and returns the path a key stores ("icons/mdi-bluetooth.svg").
pub fn save(base: &Path, id: &str) -> Result<String> {
    let (prefix, name) = parse_id(id)?;
    let icon = find(base, prefix, name)?;
    let file = format!("{prefix}-{name}.svg");
    std::fs::create_dir_all(base.join("icons"))?;
    std::fs::write(base.join("icons").join(&file), &icon.svg)?;
    Ok(format!("icons/{file}"))
}

/// From a set already on disk when there is one, else just this icon from the API.
fn find(base: &Path, prefix: &str, name: &str) -> Result<Icon> {
    let on_disk = remembered(prefix).is_some() || cache(base).join("sets").join(format!("{prefix}.json")).exists();
    let set = if on_disk {
        open_set(base, prefix)?
    } else {
        Arc::new(parse_set(&download(&format!("{API}/{prefix}.json?icons={name}"))?)?)
    };
    set.iter().find(|icon| icon.name == name).cloned().ok_or_else(|| anyhow!("o ícone {prefix}:{name} não existe"))
}

/// "mdi:bluetooth" → ("mdi", "bluetooth").
pub fn parse_id(id: &str) -> Result<(&str, &str)> {
    let Some((prefix, name)) = id.trim().split_once(':') else {
        bail!("use o nome como o Icônes copia, por exemplo mdi:bluetooth");
    };
    check_part(prefix)?;
    check_part(name)?;
    Ok((prefix, name))
}

/// Iconify names are lowercase words joined by "-", which also keeps them
/// safe in file names and addresses.
fn check_part(part: &str) -> Result<()> {
    let ok = !part.is_empty()
        && part.split('-').all(|word| !word.is_empty() && word.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    if !ok {
        bail!("nome de ícone inválido: {part:?}");
    }
    Ok(())
}

/// Percent-encodes a search query for a URL.
fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SET: &str = r#"{
        "prefix": "demo", "width": 24, "height": 24,
        "icons": {
            "home": { "body": "<path fill=\"currentColor\" d=\"M0 0h24v24H0z\"/>" },
            "home-outline": { "body": "<path d=\"M1 1\"/>", "width": 32, "left": -4 },
            "old": { "body": "<path/>", "hidden": true }
        },
        "aliases": { "house": { "parent": "home" } }
    }"#;

    #[test]
    fn sets_become_svg_files_with_their_sizes() {
        let icons = parse_set(SET).unwrap();
        assert_eq!(icons.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["home", "home-outline"]);
        assert_eq!(icons[0].svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z"/></svg>"#);
        assert!(icons[1].svg.contains(r#"viewBox="-4 0 32 24""#));
        // A set without sizes uses Iconify's 16.
        let small = parse_set(r#"{ "icons": { "a": { "body": "<g/>" } } }"#).unwrap();
        assert!(small[0].svg.contains(r#"viewBox="0 0 16 16""#));
    }

    #[test]
    fn pages_filter_by_every_word() {
        let dir = std::env::temp_dir().join("deck-engine-iconify-test");
        let path = cache(&dir).join("sets").join("demo.json");
        write_atomic(&path, SET).unwrap();
        loaded().lock().unwrap().retain(|(p, _)| p != "demo");
        let page = icons(&dir, "demo", "", 0, 1).unwrap();
        assert_eq!((page.total, page.icons.len()), (2, 1));
        assert_eq!(icons(&dir, "demo", "outline HOME", 0, 10).unwrap().icons[0].name, "home-outline");
        assert_eq!(icons(&dir, "demo", "car", 0, 10).unwrap().total, 0);
        assert_eq!(save(&dir, "demo:home").unwrap(), "icons/demo-home.svg");
        assert!(std::fs::read_to_string(dir.join("icons/demo-home.svg")).unwrap().contains("currentColor"));
    }

    #[test]
    fn ids_are_checked_before_any_path_or_address() {
        assert_eq!(parse_id(" mdi:bluetooth-off ").unwrap(), ("mdi", "bluetooth-off"));
        assert_eq!(parse_id("fa6-solid:0").unwrap(), ("fa6-solid", "0"));
        for bad in ["bluetooth", "mdi:", ":home", "mdi:../x", "MDI:home", "mdi:home?x=1", "mdi:a--b", "mdi:a/b"] {
            assert!(parse_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_list_keeps_iconify_category_order_and_hides_hidden_ones() {
        let list = r#"{
            "zz": { "name": "Arquivo", "total": 5, "category": "Archive / Unmaintained" },
            "mdi": { "name": "Material Design Icons", "total": 7447, "category": "Material",
                     "author": { "name": "Pictogrammers" }, "license": { "title": "Apache 2.0", "spdx": "Apache-2.0" },
                     "samples": ["account", "home", "bell", "x"] },
            "emo": { "name": "Emoji", "category": "Emoji", "palette": true, "license": { "spdx": "CC-BY-4.0" } },
            "gone": { "name": "Oculta", "hidden": true }
        }"#;
        let sets = parse_sets(list).unwrap();
        assert_eq!(sets.iter().map(|s| s.prefix.as_str()).collect::<Vec<_>>(), ["mdi", "emo", "zz"]);
        assert_eq!(sets[0].samples, ["account", "home", "bell"]);
        assert!(sets[1].palette && sets[1].attribution && !sets[0].attribution);
    }

    #[test]
    fn queries_are_encoded() {
        assert_eq!(encode("seta à direita"), "seta%20%C3%A0%20direita");
    }
}
