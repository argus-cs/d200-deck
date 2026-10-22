//! The addon catalog: addons people find and install from the Addons page,
//! listed in a repository of their own (argus-cs/d200-deck-addons) and
//! reviewed there. Each entry pins its zip by size and SHA-256, so what gets
//! installed is the file that was reviewed. The window names an addon only by
//! its id: addresses and hashes come from the copy of the list kept here.
//!
//! Installing takes two steps: `fetch` downloads, checks and unpacks on a
//! worker thread (`Staged`), then the engine's thread puts the folder in
//! place (`Host::install`), since a running addon must stop first.

use std::collections::HashSet;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Context, Result};
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::manifest::{Manifest, API_VERSION, APP, MANIFEST};
use crate::files::write_atomic;

pub const URL: &str = "https://raw.githubusercontent.com/argus-cs/d200-deck-addons/main/catalog.json";
/// The catalog's format this app reads. Fields are added without changing it;
/// a newer one asks for a newer app.
const FORMAT: u32 = 1;
/// Kept on disk beside config.json.
const CACHE: &str = "addon-catalog.json";
/// The list changes a few times a month at most.
const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const MAX_LIST: u64 = 4 * 1024 * 1024;
/// An addon with its node_modules or a runtime of its own.
const MAX_ZIP: u64 = 100 * 1024 * 1024;
/// The sizes a zip states may lie: this counts what is really written.
const MAX_UNPACKED: u64 = 300 * 1024 * 1024;
const MAX_FILES: usize = 20_000;

/// An addon in the catalog, with where to get it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    /// The version the zip has: the one in its addon.json.
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    pub api: u32,
    /// Its actions' names, to search by.
    #[serde(default)]
    pub actions: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Programs it needs in PATH ("node").
    #[serde(default)]
    pub requires: Vec<String>,
    pub download: Download,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Download {
    /// An https address; it may redirect (GitHub's release files do).
    pub url: String,
    pub size: u64,
    /// In hex.
    pub sha256: String,
}

/// An entry as the Addons page shows it.
#[derive(Clone, Debug, Serialize)]
pub struct Listed {
    #[serde(flatten)]
    pub entry: Entry,
    /// This app speaks the protocol version it was made for.
    pub supported: bool,
    /// The programs it needs that aren't in PATH.
    pub missing: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Listing {
    pub addons: Vec<Listed>,
    /// Why the list may be old: it couldn't be downloaded, and the copy on disk serves.
    pub offline: Option<String>,
}

#[derive(Deserialize)]
struct Document {
    format: u32,
    /// Read one by one: an entry this app doesn't understand doesn't hide the others.
    #[serde(default)]
    addons: Vec<Value>,
}

impl Entry {
    fn validate(&self) -> Result<()> {
        if !super::is_id(&self.id) {
            bail!("id {:?} inválido", self.id);
        }
        if self.name.trim().is_empty() {
            bail!("{}: falta o nome", self.id);
        }
        if !self.download.url.starts_with("https://") {
            bail!("{}: o endereço do zip precisa ser https", self.id);
        }
        if self.download.size == 0 || self.download.size > MAX_ZIP {
            bail!("{}: o zip precisa ter de 1 byte a {} MB", self.id, MAX_ZIP >> 20);
        }
        if self.download.sha256.len() != 64 || !self.download.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            bail!("{}: o sha256 precisa ter 64 dígitos hexadecimais", self.id);
        }
        if self.homepage.as_ref().is_some_and(|page| !page.starts_with("https://")) {
            bail!("{}: a página do addon precisa ser https", self.id);
        }
        if self.requires.iter().any(|program| program.is_empty() || program.contains(['\\', '/', ':'])) {
            bail!("{}: requires lista nomes de programas, sem pasta", self.id);
        }
        Ok(())
    }
}

/// The catalog, downloaded at most once a day, or now with `refresh`. The
/// copy on disk serves while offline.
pub fn list(config_dir: &Path, refresh: bool) -> Result<Listing> {
    let path = config_dir.join(CACHE);
    let age = std::fs::metadata(&path).and_then(|m| m.modified()).ok().and_then(|t| SystemTime::now().duration_since(t).ok());
    let (entries, offline) = match age {
        Some(age) if !refresh && age < MAX_AGE => (parse(&std::fs::read_to_string(&path)?)?, None),
        _ => match download(URL, MAX_LIST).and_then(|bytes| String::from_utf8(bytes).context("o catálogo não é texto")) {
            Ok(text) => {
                // A list this app can't read doesn't take the place of one it can.
                let entries = parse(&text)?;
                write_atomic(&path, &text)?;
                (entries, None)
            }
            Err(e) => match std::fs::read_to_string(&path) {
                Ok(text) => (parse(&text)?, Some(format!("{e:#}"))),
                Err(_) => return Err(e),
            },
        },
    };
    Ok(Listing { addons: entries.into_iter().map(listed).collect(), offline })
}

/// The entry for `id` in the copy on disk, without the network: the one the
/// window listed.
pub fn find(config_dir: &Path, id: &str) -> Result<Entry> {
    let text = std::fs::read_to_string(config_dir.join(CACHE)).map_err(|_| anyhow!("o catálogo ainda não foi baixado"))?;
    parse(&text)?.into_iter().find(|entry| entry.id == id).ok_or_else(|| anyhow!("o addon {id} não está no catálogo"))
}

/// The entries this app can use; the others are skipped.
fn parse(text: &str) -> Result<Vec<Entry>> {
    let entries = read(text)?.into_iter().filter_map(|entry| entry.map_err(|e| warn!("catálogo: entrada ignorada: {e}")).ok());
    Ok(entries.collect())
}

/// Every entry of a catalog, or why this app can't use it (the catalog's
/// check shows them all).
pub fn read(text: &str) -> Result<Vec<Result<Entry, String>>> {
    let document: Document = serde_json::from_str(text.trim_start_matches('\u{feff}')).context("o catálogo não é válido")?;
    if document.format > FORMAT {
        bail!("o catálogo está num formato mais novo: atualize o D200 Deck para ver os addons");
    }
    let mut ids = HashSet::new();
    let entries = document.addons.into_iter().enumerate().map(|(n, value)| {
        let id = value.get("id").and_then(Value::as_str).map_or_else(|| format!("entrada {}", n + 1), str::to_string);
        let entry = serde_json::from_value::<Entry>(value).with_context(|| format!("{id}: entrada inválida"));
        match entry.and_then(|entry| entry.validate().map(|()| entry)) {
            Ok(entry) if ids.insert(entry.id.clone()) => Ok(entry),
            Ok(entry) => Err(format!("{}: o id aparece duas vezes; vale o primeiro", entry.id)),
            Err(e) => Err(format!("{e:#}")),
        }
    });
    Ok(entries.collect())
}

/// Where an entry and the addon.json in its zip disagree, beyond the id and
/// the version (which `unpack` refuses): the catalog's check asks that the
/// list tell what the zip holds.
pub fn differences(entry: &Entry, manifest: &Manifest) -> Vec<String> {
    let actions: Vec<&str> = manifest.actions.iter().map(|action| action.name.as_str()).collect();
    let fields = [
        ("name", entry.name.clone(), manifest.name.clone()),
        ("author", entry.author.clone(), manifest.author.clone()),
        ("description", entry.description.clone(), manifest.description.clone()),
        ("api", entry.api.to_string(), manifest.api.to_string()),
        ("actions", format!("{:?}", entry.actions), format!("{actions:?}")),
    ];
    fields
        .into_iter()
        .filter(|(_, listed, zipped)| listed != zipped)
        .map(|(field, listed, zipped)| format!("{field}: o catálogo diz {listed:?}, e o zip, {zipped:?}"))
        .collect()
}

fn listed(entry: Entry) -> Listed {
    let path = std::env::var_os("PATH");
    let pathext = std::env::var_os("PATHEXT");
    let missing = entry
        .requires
        .iter()
        .filter(|program| crate::actions::find_in_path(program, path.as_deref(), pathext.as_deref()).is_none())
        .cloned()
        .collect();
    Listed { supported: entry.api <= API_VERSION, missing, entry }
}

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            // A redirect to plain http would skip the certificate.
            .https_only(true)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_global(Some(Duration::from_secs(10 * 60)))
            .build()
            .into()
    })
}

/// The file at `url`, refused past `max` bytes.
fn download(url: &str, max: u64) -> Result<Vec<u8>> {
    let mut response = agent().get(url).header("User-Agent", "D200Deck").call().with_context(|| format!("não consegui baixar {url}"))?;
    // ureq refuses a body of exactly its limit (it reads once more to find
    // the end), hence one byte of room: a file of `max` bytes passes.
    match response.body_mut().with_config().limit(max + 1).read_to_vec() {
        Ok(bytes) => Ok(bytes),
        Err(ureq::Error::BodyExceedsLimit(_)) => bail!("o arquivo em {url} passa de {max} bytes"),
        Err(e) => Err(e).with_context(|| format!("resposta inválida de {url}")),
    }
}

/// A folder that goes away with this, with whatever is left in it.
pub(super) struct Temp(PathBuf);

impl Temp {
    /// A new name in `dir` (created if needed), for `label`.
    pub(super) fn new(dir: &Path, label: &str) -> Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::fs::create_dir_all(dir).with_context(|| format!("a pasta {} não foi criada", dir.display()))?;
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Ok(Self(dir.join(format!("{label}-{}-{n}", std::process::id()))))
    }

    pub(super) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        if self.0.exists() {
            if let Err(e) = std::fs::remove_dir_all(&self.0) {
                warn!("não consegui apagar {}: {e} (sai na próxima vez que o app abrir)", self.0.display());
            }
        }
    }
}

/// An addon downloaded and unpacked beside the addons folder, waiting for
/// `Host::install` to put it in place.
pub struct Staged {
    /// Deletes what is left of the download when this goes.
    _temp: Temp,
    /// Its folder: the temporary one, or the zip's one folder inside it.
    pub(super) folder: PathBuf,
    pub manifest: Manifest,
}

/// Downloads the addon `id` names in the catalog on disk, checks it and
/// unpacks it in `staging`.
pub fn fetch(config_dir: &Path, staging: &Path, id: &str) -> Result<Staged> {
    fetch_entry(&find(config_dir, id)?, staging)
}

/// Downloads an entry's zip, checks it and unpacks it in `staging`.
pub fn fetch_entry(entry: &Entry, staging: &Path) -> Result<Staged> {
    if entry.api > API_VERSION {
        bail!("o {} pede a versão {} do protocolo, e este D200 Deck entende até a {API_VERSION}: atualize o app", entry.name, entry.api);
    }
    // A bigger file stops here; a smaller one, in `verify`.
    let zip = download(&entry.download.url, entry.download.size).context("o arquivo baixado não é o que o catálogo descreve")?;
    unpack(&zip, entry, staging)
}

/// Checks a downloaded zip against its entry and unpacks it in a new folder in `staging`.
pub fn unpack(zip: &[u8], entry: &Entry, staging: &Path) -> Result<Staged> {
    verify(zip, &entry.download)?;
    let temp = Temp::new(staging, &format!("{}-new", entry.id))?;
    extract(zip, temp.path())?;
    let folder = root(temp.path())?;
    let manifest = Manifest::load(&folder).context("o addon do zip não carregou")?;
    if manifest.id != entry.id {
        bail!("o zip traz o addon {}, e o catálogo diz {}", manifest.id, entry.id);
    }
    if manifest.version != entry.version {
        bail!("o zip traz a versão {}, e o catálogo diz {}", manifest.version, entry.version);
    }
    if manifest.run.first().is_some_and(|program| program.trim() == APP) {
        bail!("{APP} só vale para os addons que vêm com o D200 Deck");
    }
    Ok(Staged { _temp: temp, folder, manifest })
}

fn verify(zip: &[u8], download: &Download) -> Result<()> {
    if zip.len() as u64 != download.size {
        bail!("o arquivo baixado tem {} bytes, e o catálogo diz {}", zip.len(), download.size);
    }
    let hash: String = Sha256::digest(zip).iter().map(|byte| format!("{byte:02x}")).collect();
    if !hash.eq_ignore_ascii_case(&download.sha256) {
        bail!("o arquivo baixado não é o que o catálogo descreve (o SHA-256 não confere)");
    }
    Ok(())
}

/// Unpacks every file, refusing links and paths that would land outside `into`.
fn extract(zip: &[u8], into: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip)).context("o arquivo baixado não é um zip")?;
    if archive.len() > MAX_FILES {
        bail!("o zip tem {} arquivos, e o limite é {MAX_FILES}", archive.len());
    }
    std::fs::create_dir_all(into)?;
    let mut total = 0;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        let name = file.name().to_string();
        if file.is_symlink() {
            bail!("o zip tem um link ({name}), e links não são aceitos");
        }
        let path = file.enclosed_name().filter(|path| plain(path)).ok_or_else(|| anyhow!("o zip tem um caminho que não é aceito: {name:?}"))?;
        let target = into.join(path);
        if file.is_dir() {
            std::fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut out = std::fs::File::create_new(&target).with_context(|| format!("não consegui criar {name}"))?;
        // Reads to the end, which checks the CRC.
        total += std::io::copy(&mut (&mut file).take(MAX_UNPACKED + 1 - total), &mut out).with_context(|| format!("{name} não descompactou"))?;
        if total > MAX_UNPACKED {
            bail!("o zip descompactado passa de {} MB", MAX_UNPACKED >> 20);
        }
    }
    Ok(())
}

/// A relative path made only of ordinary Windows names: no drive, `..`,
/// stream (`a:b`) or device (`CON`, `NUL.txt`).
fn plain(path: &Path) -> bool {
    let mut names = 0;
    let ordinary = path.components().all(|part| match part {
        Component::CurDir => true,
        Component::Normal(name) => {
            names += 1;
            name.to_str().is_some_and(|name| !name.contains(':') && !name.ends_with(['.', ' ']) && !is_device(name))
        }
        _ => false,
    });
    ordinary && names > 0
}

fn is_device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or_default().trim_end().to_ascii_uppercase();
    match stem.as_str() {
        "CON" | "PRN" | "AUX" | "NUL" => true,
        _ => stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT")) && matches!(stem.as_bytes()[3], b'1'..=b'9'),
    }
}

/// Where the addon.json is: at the zip's root, or in its one folder (as in
/// the zips GitHub makes of a repository).
fn root(dir: &Path) -> Result<PathBuf> {
    if dir.join(MANIFEST).is_file() {
        return Ok(dir.to_path_buf());
    }
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)?.flatten().map(|entry| entry.path()).collect();
    match entries.as_slice() {
        [only] if only.join(MANIFEST).is_file() => Ok(only.clone()),
        _ => bail!("o zip não tem um {MANIFEST} na raiz nem numa pasta única"),
    }
}

/// A zip of these files, for tests here and in the host's.
#[cfg(test)]
pub(super) fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in files {
        if let Some(dir) = name.strip_suffix('/') {
            zip.add_directory(dir, zip::write::SimpleFileOptions::default()).unwrap();
        } else {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
    }
    zip.finish().unwrap().into_inner()
}

/// The entry that describes this zip of this manifest.
#[cfg(test)]
pub(super) fn entry_of(zip: &[u8], manifest: &str) -> Entry {
    let manifest: Manifest = serde_json::from_str(manifest).unwrap();
    Entry {
        id: manifest.id,
        name: manifest.name,
        version: manifest.version,
        author: manifest.author,
        description: manifest.description,
        api: manifest.api,
        actions: manifest.actions.into_iter().map(|action| action.name).collect(),
        tags: Vec::new(),
        homepage: None,
        requires: Vec::new(),
        download: Download {
            url: "https://example.com/addon.zip".into(),
            size: zip.len() as u64,
            sha256: Sha256::digest(zip).iter().map(|byte| format!("{byte:02x}")).collect(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST_JSON: &str = r#"{ "id": "exemplo.contador", "name": "Contador", "version": "0.2.0", "api": 1, "run": ["node", "main.js"] }"#;

    fn staging(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("deck-engine-catalog-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn left_in(dir: &Path) -> usize {
        std::fs::read_dir(dir).map_or(0, |entries| entries.count())
    }

    #[test]
    fn reads_the_catalog_and_skips_entries_it_cannot_use() {
        let sha = "ab".repeat(32);
        let entry = |id: &str, extra: &str| {
            format!(
                r#"{{ "id": "{id}", "name": "N", "version": "1.0.0", "api": 1, "novo": true,
                     "download": {{ "url": "https://example.com/a.zip", "size": 10, "sha256": "{sha}" }} {extra} }}"#
            )
        };
        let addons = [
            entry("bom.addon", r#", "tags": ["jogos"], "requires": ["node"]"#),
            entry("Ruim", ""),
            entry("http.addon", "").replace("https://example.com", "http://example.com"),
            entry("hash.addon", "").replace(&sha, "xyz"),
            entry("pagina.addon", r#", "homepage": "http://example.com""#),
            entry("pasta.addon", r#", "requires": ["C:\\node.exe"]"#),
            entry("bom.addon", ""),
            r#"{ "id": "sem.download", "name": "N", "version": "1", "api": 1 }"#.to_string(),
        ];
        let text = format!(r#"{{ "format": 1, "addons": [{}] }}"#, addons.join(","));
        let entries = parse(&text).unwrap();
        assert_eq!(entries.len(), 1, "{entries:?}");
        assert_eq!(entries[0].tags, ["jogos"]);
        assert_eq!(entries[0].requires, ["node"]);
        // The catalog's check sees why each one was skipped.
        let errors: Vec<String> = read(&text).unwrap().into_iter().filter_map(Result::err).collect();
        assert_eq!(errors.len(), 7, "{errors:?}");
        assert!(errors.iter().any(|e| e.starts_with("sem.download: entrada inválida")), "{errors:?}");
        assert!(errors.iter().any(|e| e.starts_with("bom.addon: o id aparece duas vezes")), "{errors:?}");
        assert!(errors.iter().any(|e| e.starts_with("http.addon:")), "{errors:?}");

        let newer = parse(r#"{ "format": 2, "addons": [] }"#).unwrap_err().to_string();
        assert!(newer.contains("atualize"), "{newer}");
        assert!(parse("{").is_err());
    }

    #[test]
    fn differences_tell_where_the_list_and_the_zip_disagree() {
        let zip = zip_of(&[("addon.json", MANIFEST_JSON)]);
        let mut entry = entry_of(&zip, MANIFEST_JSON);
        let manifest: Manifest = serde_json::from_str(MANIFEST_JSON).unwrap();
        assert!(differences(&entry, &manifest).is_empty());
        entry.name = "Outro".into();
        entry.actions = vec!["Contar".into()];
        let found = differences(&entry, &manifest);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].starts_with("name:") && found[1].starts_with("actions:"), "{found:?}");
    }

    #[test]
    fn listing_says_what_this_pc_lacks() {
        let zip = zip_of(&[("addon.json", MANIFEST_JSON)]);
        let mut entry = entry_of(&zip, MANIFEST_JSON);
        entry.requires = vec!["cmd".into(), "programa-que-nao-existe-em-lugar-nenhum".into()];
        let shown = listed(entry.clone());
        assert!(shown.supported);
        assert_eq!(shown.missing, ["programa-que-nao-existe-em-lugar-nenhum"]);
        entry.api = API_VERSION + 1;
        assert!(!listed(entry).supported);
    }

    #[test]
    fn unpacks_at_the_root_or_in_one_folder() {
        let dir = staging("unpack");
        for prefix in ["", "contador-main/"] {
            let files = [(format!("{prefix}addon.json"), MANIFEST_JSON), (format!("{prefix}lib/main.js"), "// oi")];
            let files: Vec<(&str, &str)> = files.iter().map(|(name, text)| (name.as_str(), *text)).collect();
            let zip = zip_of(&files);
            let staged = unpack(&zip, &entry_of(&zip, MANIFEST_JSON), &dir).unwrap();
            assert_eq!(staged.manifest.version, "0.2.0");
            assert_eq!(std::fs::read_to_string(staged.folder.join("lib").join("main.js")).unwrap(), "// oi");
            assert!(staged.folder.ends_with(prefix.trim_end_matches('/')));
            drop(staged);
            assert_eq!(left_in(&dir), 0, "dropped, it goes");
        }
    }

    #[test]
    fn refuses_zips_that_differ_from_the_catalog_or_escape_their_folder() {
        let dir = staging("refuse");
        let good = zip_of(&[("addon.json", MANIFEST_JSON)]);
        let entry = entry_of(&good, MANIFEST_JSON);

        let mut other = entry.clone();
        other.download.sha256 = "00".repeat(32);
        assert!(unpack(&good, &other, &dir).err().unwrap().to_string().contains("SHA-256"));
        other = entry.clone();
        other.download.size += 1;
        assert!(unpack(&good, &other, &dir).err().unwrap().to_string().contains("bytes"));

        // The catalog always describes MANIFEST_JSON; the zips differ.
        let refused = |files: &[(&str, &str)], why: &str| {
            let zip = zip_of(files);
            let error = format!("{:#}", unpack(&zip, &entry_of(&zip, MANIFEST_JSON), &dir).err().unwrap());
            assert!(error.contains(why), "{files:?}: {error}");
        };
        refused(&[("addon.json", MANIFEST_JSON), ("../fora.txt", "x")], "caminho");
        refused(&[("addon.json", MANIFEST_JSON), ("a/../../fora.txt", "x")], "caminho");
        refused(&[("addon.json", MANIFEST_JSON), ("/fora.txt", "x")], "caminho");
        refused(&[("addon.json", MANIFEST_JSON), ("C:/fora.txt", "x")], "caminho");
        refused(&[("addon.json", MANIFEST_JSON), ("main.js:oculto", "x")], "caminho");
        refused(&[("addon.json", MANIFEST_JSON), ("CON.txt", "x")], "caminho");
        refused(&[("addon.json", MANIFEST_JSON), ("lib/nul", "x")], "caminho");
        refused(&[("a/addon.json", MANIFEST_JSON), ("b/x.txt", "x")], "pasta única");
        refused(&[("leia.txt", "x")], "pasta única");
        refused(&[("addon.json", &MANIFEST_JSON.replace("0.2.0", "0.3.0"))], "versão 0.3.0");
        refused(&[("addon.json", &MANIFEST_JSON.replace("exemplo.contador", "outro.addon"))], "outro.addon");
        refused(&[("addon.json", &MANIFEST_JSON.replace(r#"["node", "main.js"]"#, r#"["$app"]"#))], "$app");
        refused(&[("addon.json", &MANIFEST_JSON.replace(r#""api": 1"#, r#""api": 9"#))], "atualize");

        let mut link = zip::ZipWriter::new(Cursor::new(Vec::new()));
        link.add_symlink("atalho", "C:/Windows", zip::write::SimpleFileOptions::default()).unwrap();
        let link = link.finish().unwrap().into_inner();
        assert!(unpack(&link, &entry_of(&link, MANIFEST_JSON), &dir).err().unwrap().to_string().contains("link"));

        assert!(!dir.parent().unwrap().join("fora.txt").exists());
        assert_eq!(left_in(&dir), 0, "nothing stays after a refusal");
        assert!(!plain(Path::new("")) && plain(Path::new("./a/b.js")) && !is_device("COM0") && is_device("lpt9.log"));
    }
}
