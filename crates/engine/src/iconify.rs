//! More icons from Iconify, the open library icones.js.org browses (some 240
//! sets). Nothing ships with the app: a set is downloaded once, when opened,
//! and kept on disk; the editor asks for one page of it at a time, filtered
//! here, so even emoji sets of tens of megabytes never reach the window
//! whole. A chosen icon is saved as an SVG beside the other key images, so
//! the device never needs the network.
//!
//! Search results and the sets' samples come through here too, as SVG text,
//! one request per set: Iconify's server turns away an address that asks for
//! many single icons (429), as a window of `<img>` straight from it did.

use std::collections::HashMap;
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
/// Sets asked for at the same time by `lookup`: a search's 120 results come
/// from some 40 sets, each request taking up to a second.
const FETCH_THREADS: usize = 6;
/// Aliases of aliases, as far as they are followed.
const MAX_ALIAS_DEPTH: usize = 8;

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
    /// Its name in the set ("bluetooth"), or its id ("mdi:bluetooth") when
    /// it comes from several sets.
    pub name: String,
    /// A whole SVG document; one-color icons paint with `currentColor`.
    pub svg: String,
}

/// A set's icons as SVG files.
struct Set {
    /// The ones to browse.
    icons: Vec<Icon>,
    /// Aliases (another name for an icon, maybe turned or flipped) and hidden
    /// icons: searches and older names find them, browsing doesn't show them.
    extra: Vec<Icon>,
}

impl Set {
    fn get(&self, name: &str) -> Option<&Icon> {
        self.icons.iter().chain(&self.extra).find(|icon| icon.name == name)
    }
}

#[derive(Debug, Serialize)]
pub struct Page {
    /// Icons matching the filter, of which `icons` is one page.
    pub total: usize,
    pub icons: Vec<Icon>,
}

type Loaded = Mutex<Vec<(String, Arc<Set>)>>;

fn loaded() -> &'static Loaded {
    static LOADED: OnceLock<Loaded> = OnceLock::new();
    LOADED.get_or_init(|| Mutex::new(Vec::new()))
}

/// Icons `lookup` asked for while the app runs, by id; None when the set has
/// no such icon. Each one is downloaded once.
type Fetched = Mutex<HashMap<String, Option<String>>>;

fn fetched() -> &'static Fetched {
    static FETCHED: OnceLock<Fetched> = OnceLock::new();
    FETCHED.get_or_init(|| Mutex::new(HashMap::new()))
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            // A connection kept for each of `lookup`'s threads.
            .max_idle_connections_per_host(FETCH_THREADS)
            .build()
            .into()
    })
}

fn download(url: &str) -> Result<String> {
    let mut response = match agent().get(url).header("User-Agent", "D200Deck").call() {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(429)) => bail!("o Iconify recusou por excesso de pedidos; tente de novo em alguns minutos"),
        Err(e) => return Err(e).with_context(|| format!("não consegui baixar {url}")),
    };
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
    let matching: Vec<&Icon> = set.icons.iter().filter(|icon| words.iter().all(|w| icon.name.contains(w.as_str()))).collect();
    let icons = matching.iter().skip(offset).take(limit).map(|icon| (*icon).clone()).collect();
    Ok(Page { total: matching.len(), icons })
}

fn open_set(base: &Path, prefix: &str) -> Result<Arc<Set>> {
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

fn remembered(prefix: &str) -> Option<Arc<Set>> {
    let mut loaded = loaded().lock().unwrap();
    let at = loaded.iter().position(|(p, _)| p == prefix)?;
    let entry = loaded.remove(at);
    let set = Arc::clone(&entry.1);
    loaded.insert(0, entry);
    Some(set)
}

/// The icons of an IconifyJSON document as SVG files. Sizes default to the
/// set's (16 when absent).
fn parse_set(text: &str) -> Result<Set> {
    let json: Value = serde_json::from_str(text).context("coleção inválida")?;
    let number = |key: &str, default: f64| json.get(key).and_then(Value::as_f64).unwrap_or(default);
    let defaults = Shape {
        body: String::new(),
        left: number("left", 0.0),
        top: number("top", 0.0),
        width: number("width", 16.0),
        height: number("height", 16.0),
        rotate: 0,
        h_flip: false,
        v_flip: false,
    };
    let icons = json.get("icons").and_then(Value::as_object).ok_or_else(|| anyhow!("coleção sem ícones"))?;
    let no_aliases = serde_json::Map::new();
    let aliases = json.get("aliases").and_then(Value::as_object).unwrap_or(&no_aliases);
    let mut set = Set { icons: Vec::new(), extra: Vec::new() };
    for (name, props) in icons {
        if props.get("body").and_then(Value::as_str).is_none() {
            continue;
        }
        let icon = Icon { name: name.clone(), svg: defaults.with(props).svg() };
        if props.get("hidden").and_then(Value::as_bool).unwrap_or(false) {
            set.extra.push(icon);
        } else {
            set.icons.push(icon);
        }
    }
    for name in aliases.keys() {
        if let Some(shape) = alias_shape(name, icons, aliases, &defaults, 0) {
            set.extra.push(Icon { name: name.clone(), svg: shape.svg() });
        }
    }
    Ok(set)
}

/// An alias's parent (itself maybe an alias) with the alias's properties
/// over it. None when the chain is broken or too long.
fn alias_shape(name: &str, icons: &serde_json::Map<String, Value>, aliases: &serde_json::Map<String, Value>, defaults: &Shape, depth: usize) -> Option<Shape> {
    let props = aliases.get(name)?;
    let parent = props.get("parent")?.as_str()?;
    let shape = match icons.get(parent) {
        Some(icon) if icon.get("body").and_then(Value::as_str).is_some() => defaults.with(icon),
        Some(_) => return None,
        None if depth < MAX_ALIAS_DEPTH => alias_shape(parent, icons, aliases, defaults, depth + 1)?,
        None => return None,
    };
    Some(shape.with(props))
}

/// An icon as IconifyJSON describes it, with the set's sizes filled in.
#[derive(Clone)]
struct Shape {
    body: String,
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    /// Quarter turns, clockwise.
    rotate: i64,
    h_flip: bool,
    v_flip: bool,
}

impl Shape {
    /// An icon's or alias's own properties over these, merged as Iconify
    /// does: sizes replace, turns add up, a flip undoes another.
    fn with(&self, props: &Value) -> Shape {
        let number = |key: &str, default: f64| props.get(key).and_then(Value::as_f64).unwrap_or(default);
        let flag = |key: &str| props.get(key).and_then(Value::as_bool).unwrap_or(false);
        Shape {
            body: props.get("body").and_then(Value::as_str).map_or_else(|| self.body.clone(), str::to_string),
            left: number("left", self.left),
            top: number("top", self.top),
            width: number("width", self.width),
            height: number("height", self.height),
            rotate: self.rotate + props.get("rotate").and_then(Value::as_i64).unwrap_or(0),
            h_flip: self.h_flip != flag("hFlip"),
            v_flip: self.v_flip != flag("vFlip"),
        }
    }

    /// The SVG document, turned and flipped the way Iconify's renderer
    /// (`iconToSVG`) does it.
    fn svg(&self) -> String {
        let (mut left, mut top, mut width, mut height) = (self.left, self.top, self.width, self.height);
        let mut turns = self.rotate;
        let mut transforms = Vec::new();
        if self.h_flip && self.v_flip {
            turns += 2;
        } else if self.h_flip {
            transforms.push(format!("translate({} {}) scale(-1 1)", width + left, 0.0 - top));
            (left, top) = (0.0, 0.0);
        } else if self.v_flip {
            transforms.push(format!("translate({} {}) scale(1 -1)", 0.0 - left, height + top));
            (left, top) = (0.0, 0.0);
        }
        match turns.rem_euclid(4) {
            1 => {
                let center = height / 2.0 + top;
                transforms.insert(0, format!("rotate(90 {center} {center})"));
            }
            2 => transforms.insert(0, format!("rotate(180 {} {})", width / 2.0 + left, height / 2.0 + top)),
            3 => {
                let center = width / 2.0 + left;
                transforms.insert(0, format!("rotate(-90 {center} {center})"));
            }
            _ => {}
        }
        if turns.rem_euclid(2) == 1 {
            (left, top) = (top, left);
            (width, height) = (height, width);
        }
        let body = if transforms.is_empty() { self.body.clone() } else { format!(r#"<g transform="{}">{}</g>"#, transforms.join(" "), self.body) };
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{left} {top} {width} {height}">{body}</svg>"#)
    }
}

/// Icons from any sets by id ("mdi:bluetooth"), named by it, in the order
/// asked; ids that don't exist are left out. One request per set, a few at a
/// time, and an icon only once while the app runs. Fails only when nothing
/// could be found and a request failed.
pub fn lookup(ids: &[String]) -> Result<Vec<Icon>> {
    let mut wanted: Vec<(&str, Vec<&str>)> = Vec::new();
    {
        let known = fetched().lock().unwrap();
        for id in ids.iter().filter(|id| !known.contains_key(id.as_str())) {
            let Ok((prefix, name)) = parse_id(id) else { continue };
            match wanted.iter_mut().find(|(p, _)| *p == prefix) {
                Some((_, names)) if names.contains(&name) => {}
                Some((_, names)) => names.push(name),
                None => wanted.push((prefix, vec![name])),
            }
        }
    }
    let threads = FETCH_THREADS.min(wanted.len());
    let queue = Mutex::new(wanted.into_iter());
    let failure = Mutex::new(None);
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                // The queue is unlocked before downloading.
                let next = queue.lock().unwrap().next();
                let Some((prefix, names)) = next else { break };
                match download(&format!("{API}/{prefix}.json?icons={}", names.join(","))).and_then(|text| parse_set(&text)) {
                    Ok(set) => {
                        let mut known = fetched().lock().unwrap();
                        for name in names {
                            known.insert(format!("{prefix}:{name}"), set.get(name).map(|icon| icon.svg.clone()));
                        }
                    }
                    Err(e) => {
                        failure.lock().unwrap().get_or_insert(e);
                    }
                }
            });
        }
    });
    let known = fetched().lock().unwrap();
    let found: Vec<Icon> = ids.iter().filter_map(|id| Some(Icon { name: id.clone(), svg: known.get(id.as_str())?.clone()? })).collect();
    match failure.into_inner().unwrap() {
        Some(e) if found.is_empty() => Err(e),
        _ => Ok(found),
    }
}

/// Icons across every set, as Iconify's search finds them, named by id.
pub fn search(query: &str) -> Result<Vec<Icon>> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let text = download(&format!("{API}/search?query={}&limit={SEARCH_LIMIT}", encode(query)))?;
    let json: Value = serde_json::from_str(&text).context("busca inválida")?;
    let ids: Vec<String> = json.get("icons").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    lookup(&ids)
}

/// Like `lookup`, but also kept on disk: the samples the list of sets shows
/// every time the window opens. A failed download only leaves samples out.
pub fn samples(base: &Path, ids: &[String]) -> Result<Vec<Icon>> {
    static WRITING: Mutex<()> = Mutex::new(());
    let path = cache(base).join("samples.json");
    let read = || -> HashMap<String, String> { std::fs::read_to_string(&path).ok().and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default() };
    let mut kept = read();
    let missing: Vec<String> = ids.iter().filter(|id| !kept.contains_key(id.as_str())).cloned().collect();
    if !missing.is_empty() {
        match lookup(&missing) {
            Ok(found) if !found.is_empty() => {
                let _writing = WRITING.lock().unwrap();
                kept = read();
                kept.extend(found.into_iter().map(|icon| (icon.name, icon.svg)));
                write_atomic(&path, &serde_json::to_string(&kept)?)?;
            }
            Ok(_) => {}
            Err(e) => log::warn!("amostras do Iconify: {e:#}"),
        }
    }
    Ok(ids.iter().filter_map(|id| Some(Icon { name: id.clone(), svg: kept.get(id.as_str())?.clone() })).collect())
}

/// Saves an icon ("mdi:bluetooth", as icones.js.org copies it) into the icons
/// folder and returns the path a key stores ("icons/mdi-bluetooth.svg").
pub fn save(base: &Path, id: &str) -> Result<String> {
    let (prefix, name) = parse_id(id)?;
    let svg = find(base, prefix, name)?;
    let file = format!("{prefix}-{name}.svg");
    std::fs::create_dir_all(base.join("icons"))?;
    std::fs::write(base.join("icons").join(&file), svg)?;
    Ok(format!("icons/{file}"))
}

/// An icon's SVG: from its set when that is on disk, else as `lookup` finds it.
fn find(base: &Path, prefix: &str, name: &str) -> Result<String> {
    let on_disk = remembered(prefix).is_some() || cache(base).join("sets").join(format!("{prefix}.json")).exists();
    let svg = if on_disk {
        open_set(base, prefix)?.get(name).map(|icon| icon.svg.clone())
    } else {
        lookup(&[format!("{prefix}:{name}")])?.pop().map(|icon| icon.svg)
    };
    svg.ok_or_else(|| anyhow!("o ícone {prefix}:{name} não existe"))
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
        let icons = parse_set(SET).unwrap().icons;
        assert_eq!(icons.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["home", "home-outline"]);
        assert_eq!(icons[0].svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M0 0h24v24H0z"/></svg>"#);
        assert!(icons[1].svg.contains(r#"viewBox="-4 0 32 24""#));
        // A set without sizes uses Iconify's 16.
        let small = parse_set(r#"{ "icons": { "a": { "body": "<g/>" } } }"#).unwrap();
        assert!(small.icons[0].svg.contains(r#"viewBox="0 0 16 16""#));
    }

    #[test]
    fn aliases_and_hidden_icons_are_found_but_not_browsed() {
        let set = parse_set(
            r#"{
            "width": 24, "height": 24,
            "icons": { "home": { "body": "<path/>" }, "old": { "body": "<g/>", "hidden": true } },
            "aliases": {
                "house": { "parent": "home" },
                "building": { "parent": "house", "width": 32 },
                "broken": { "parent": "nothing" },
                "a": { "parent": "b" }, "b": { "parent": "a" }
            }
        }"#,
        )
        .unwrap();
        assert_eq!(set.icons.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["home"]);
        assert_eq!(set.get("house").unwrap().svg, set.get("home").unwrap().svg);
        assert!(set.get("building").unwrap().svg.contains(r#"viewBox="0 0 32 24""#));
        assert!(set.get("old").is_some());
        for missing in ["broken", "a", "b", "nope"] {
            assert!(set.get(missing).is_none(), "{missing}");
        }
    }

    #[test]
    fn turns_and_flips_follow_iconify() {
        let set = parse_set(
            r#"{
            "width": 32, "height": 24,
            "icons": { "arrow": { "body": "<path/>" }, "up": { "body": "<path/>", "rotate": 3 } },
            "aliases": {
                "mirror": { "parent": "arrow", "hFlip": true },
                "upside": { "parent": "arrow", "vFlip": true },
                "both": { "parent": "arrow", "hFlip": true, "vFlip": true },
                "right": { "parent": "arrow", "rotate": 1 },
                "unmirror": { "parent": "mirror", "hFlip": true },
                "back": { "parent": "up", "rotate": 1 }
            }
        }"#,
        )
        .unwrap();
        let svg = |name: &str| set.get(name).unwrap().svg.clone();
        assert!(svg("mirror").contains(r#"viewBox="0 0 32 24"><g transform="translate(32 0) scale(-1 1)"><path/></g>"#));
        assert!(svg("upside").contains(r#"<g transform="translate(0 24) scale(1 -1)">"#));
        assert!(svg("both").contains(r#"viewBox="0 0 32 24"><g transform="rotate(180 16 12)">"#));
        // A quarter turn swaps the sides.
        assert!(svg("right").contains(r#"viewBox="0 0 24 32"><g transform="rotate(90 12 12)">"#));
        assert!(svg("up").contains(r#"viewBox="0 0 24 32"><g transform="rotate(-90 16 16)">"#));
        // Flipping twice, or turning all the way, leaves the icon as it was.
        assert_eq!(svg("unmirror"), svg("arrow"));
        assert_eq!(svg("back"), svg("arrow"));
    }

    #[test]
    fn lookups_keep_the_order_and_skip_what_does_not_exist() {
        {
            let mut known = fetched().lock().unwrap();
            known.insert("lookup-test:b".into(), Some("<svg>b</svg>".into()));
            known.insert("lookup-test:a".into(), Some("<svg>a</svg>".into()));
            known.insert("lookup-test:none".into(), None);
        }
        let ids = ["lookup-test:b", "lookup-test:none", "not an id", "lookup-test:a"].map(String::from);
        let found = lookup(&ids).unwrap();
        assert_eq!(found.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), ["lookup-test:b", "lookup-test:a"]);
        assert_eq!(found[1].svg, "<svg>a</svg>");
    }

    #[test]
    fn samples_stay_on_disk() {
        let dir = std::env::temp_dir().join("deck-engine-iconify-samples-test");
        let _ = std::fs::remove_dir_all(&dir);
        fetched().lock().unwrap().insert("samples-test:home".into(), Some("<svg>home</svg>".into()));
        fetched().lock().unwrap().insert("samples-test:gone".into(), None);
        let ids = ["samples-test:home", "samples-test:gone"].map(String::from);
        assert_eq!(samples(&dir, &ids).unwrap(), [Icon { name: ids[0].clone(), svg: "<svg>home</svg>".into() }]);
        // Read back from the file once the app forgets it.
        fetched().lock().unwrap().remove("samples-test:home");
        assert_eq!(samples(&dir, &ids).unwrap()[0].svg, "<svg>home</svg>");
        assert!(cache(&dir).join("samples.json").exists());
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
        // An alias is saved too, as its icon.
        assert_eq!(save(&dir, "demo:house").unwrap(), "icons/demo-house.svg");
        assert!(std::fs::read_to_string(dir.join("icons/demo-house.svg")).unwrap().contains("currentColor"));
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
