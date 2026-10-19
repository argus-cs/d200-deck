//! An addon's `addon.json`: who it is, how to start it and the actions it
//! offers, with the fields the editor draws for each. Read and checked
//! without running anything.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::icons;

/// The protocol version this app speaks; an addon made for a newer one is refused.
pub const API_VERSION: u32 = 1;
pub const MANIFEST: &str = "addon.json";
/// A manifest is a few KB: a bigger file is not one.
const MAX_MANIFEST: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Lowercase letters and digits, joined by dots and dashes ("exemplo.contador").
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    /// The protocol version it was made for.
    pub api: u32,
    /// The program and its arguments. A program with a path is relative to the
    /// addon's folder; a bare name ("node") is looked for there, then in PATH.
    pub run: Vec<String>,
    #[serde(default)]
    pub actions: Vec<ActionDef>,
}

/// An action an addon offers for keys.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActionDef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Suggested for a new key: a built-in glyph, or a PNG or SVG in the addon's folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Suggested text for a new key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The addon says whether the key is on (off shows its second face).
    #[serde(default)]
    pub state: bool,
    /// The addon may change the key's text while it shows.
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default)]
    pub fields: Vec<Field>,
}

/// A setting of an action, drawn by the editor and saved with the key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: FieldKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    /// The value a new key starts with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    /// A list's choices, when they are always the same.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Choice>,
    /// A list whose choices the addon gives while it runs (the games installed).
    #[serde(default)]
    pub dynamic: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Text,
    Number,
    Checkbox,
    Select,
}

/// One choice of a list. Picking it may suggest the key's text and image.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
    /// A shorter text for the key itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    /// An image for the key: a built-in glyph, or a PNG or SVG in the addon's
    /// folder or its data folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl Manifest {
    /// Reads and checks `addon.json` in an addon's folder.
    pub fn load(folder: &Path) -> Result<Self> {
        let path = folder.join(MANIFEST);
        let size = std::fs::metadata(&path).map_err(|_| anyhow!("falta o {MANIFEST} na pasta"))?.len();
        if size > MAX_MANIFEST {
            bail!("o {MANIFEST} tem {} KB: grande demais para um manifesto", size / 1024);
        }
        let text = std::fs::read_to_string(&path).with_context(|| format!("não consegui ler o {MANIFEST}"))?;
        // Notepad may save it with a BOM.
        let manifest: Manifest =
            serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("o {MANIFEST} não é válido"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        if !is_id(&self.id) {
            bail!("id {:?} inválido: use letras minúsculas e números, separados por pontos ou traços", self.id);
        }
        if self.name.trim().is_empty() {
            bail!("falta o nome (name)");
        }
        if self.api == 0 {
            bail!("api precisa ser a versão do protocolo, de 1 em diante");
        }
        if self.api > API_VERSION {
            bail!(
                "pede a versão {} do protocolo, e este D200 Deck entende até a {API_VERSION}: atualize o app ou peça ao autor uma versão compatível",
                self.api
            );
        }
        if self.run.first().is_none_or(|program| program.trim().is_empty()) {
            bail!("falta o programa a rodar (run)");
        }
        let mut actions = HashSet::new();
        for action in &self.actions {
            if !is_name(&action.id) {
                bail!("ação {:?}: o id usa só letras, números, _ e -", action.id);
            }
            if !actions.insert(action.id.as_str()) {
                bail!("duas ações têm o id {:?}", action.id);
            }
            action.validate().with_context(|| format!("ação {:?}", action.id))?;
        }
        Ok(())
    }

    /// The program to start and its arguments.
    pub fn command(&self, folder: &Path) -> Result<(PathBuf, &[String])> {
        let program = self.run[0].trim();
        let args = &self.run[1..];
        if program.contains(['\\', '/']) {
            // Relative to the folder, or an absolute path (join keeps it).
            let path = folder.join(program);
            if !path.is_file() {
                bail!("o programa {program:?} não existe");
            }
            return Ok((path, args));
        }
        // A bare name: the addon's folder first, then PATH, with PATHEXT like a terminal.
        let path = std::env::var_os("PATH").unwrap_or_default();
        let search: OsString = std::env::join_paths(std::iter::once(folder.to_path_buf()).chain(std::env::split_paths(&path)))?;
        let found = crate::actions::find_in_path(program, Some(&search), std::env::var_os("PATHEXT").as_deref())
            .ok_or_else(|| anyhow!("não achei {program:?} na pasta do addon nem no PATH"))?;
        Ok((found, args))
    }

    /// How it is started, as people read it ("node main.js").
    pub fn command_line(&self) -> String {
        self.run.iter().map(|part| if part.contains(' ') { format!("\"{part}\"") } else { part.clone() }).collect::<Vec<_>>().join(" ")
    }

    pub fn action(&self, id: &str) -> Option<&ActionDef> {
        self.actions.iter().find(|action| action.id == id)
    }
}

impl ActionDef {
    fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            bail!("falta o nome (name)");
        }
        if let Some(icon) = &self.icon {
            check_icon(icon)?;
        }
        let mut fields = HashSet::new();
        for field in &self.fields {
            if !is_name(&field.id) {
                bail!("campo {:?}: o id usa só letras, números, _ e -", field.id);
            }
            if !fields.insert(field.id.as_str()) {
                bail!("dois campos têm o id {:?}", field.id);
            }
            match field.kind {
                FieldKind::Select if field.options.is_empty() && !field.dynamic => {
                    bail!("o campo {:?} é uma lista sem opções (options) e sem dynamic", field.id)
                }
                FieldKind::Select => {}
                _ if !field.options.is_empty() || field.dynamic => bail!("o campo {:?}: só listas (select) têm options e dynamic", field.id),
                _ => {}
            }
        }
        Ok(())
    }
}

/// An addon id: lowercase letters and digits, joined by single dots or dashes.
pub fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id.split(['.', '-']).all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// An action's or field's id.
fn is_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// A built-in glyph, or a PNG or SVG inside the addon's folder.
fn check_icon(icon: &str) -> Result<()> {
    if icons::is_glyph(icon) {
        return Ok(());
    }
    let lower = icon.to_ascii_lowercase();
    if !lower.ends_with(".png") && !lower.ends_with(".svg") {
        bail!("ícone {icon:?} não é um ícone embutido nem um .png ou .svg");
    }
    if !inside(Path::new(icon)) {
        bail!("ícone {icon:?} precisa ficar dentro da pasta do addon");
    }
    Ok(())
}

/// A relative path that stays inside the folder it is relative to.
pub fn inside(path: &Path) -> bool {
    path.components().all(|part| matches!(part, Component::Normal(_)))
}

/// An addon folder as found on disk: its manifest, or why it didn't load.
#[derive(Clone, Debug)]
pub struct Found {
    pub folder: PathBuf,
    pub manifest: Result<Manifest, String>,
}

/// Every folder in `dir`, in name order. Two folders with one id: the
/// second is refused, since keys find their addon by id.
pub fn scan(dir: &Path) -> Vec<Found> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
    folders.sort();
    let mut taken: HashMap<String, String> = HashMap::new();
    folders
        .into_iter()
        .map(|folder| {
            let manifest = Manifest::load(&folder).map_err(|e| format!("{e:#}")).and_then(|manifest| {
                let name = folder_name(&folder);
                match taken.get(&manifest.id) {
                    Some(other) => Err(format!("a pasta {other} já tem um addon com o id {}", manifest.id)),
                    None => {
                        taken.insert(manifest.id.clone(), name);
                        Ok(manifest)
                    }
                }
            });
            Found { folder, manifest }
        })
        .collect()
}

pub fn folder_name(folder: &Path) -> String {
    folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(json: &str) -> Result<Manifest> {
        let manifest: Manifest = serde_json::from_str(json)?;
        manifest.validate()?;
        Ok(manifest)
    }

    const MINIMAL: &str = r#"{ "id": "exemplo.contador", "name": "Contador", "version": "1.0.0", "api": 1, "run": ["node", "main.js"] }"#;

    #[test]
    fn reads_a_manifest_with_actions_and_fields() {
        let manifest = manifest(
            r#"{ "id": "com.exemplo-x.steam2", "name": "Steam", "version": "1.0.0", "api": 1, "run": ["steam.exe"],
                 "actions": [ { "id": "launch", "name": "Abrir jogo", "icon": "gamepad", "state": true, "fields": [
                     { "id": "game", "type": "select", "label": "Jogo", "dynamic": true },
                     { "id": "when_open", "type": "select", "label": "Se já estiver aberto",
                       "options": [ { "value": "front", "label": "Trazer para a frente" } ] },
                     { "id": "args", "type": "text", "label": "Argumentos" },
                     { "id": "count", "type": "number", "label": "Vezes", "default": 1, "min": 1 },
                     { "id": "quiet", "type": "checkbox", "label": "Sem som" } ] } ] }"#,
        )
        .unwrap();
        let launch = manifest.action("launch").unwrap();
        assert!(launch.state && !launch.dynamic);
        assert_eq!(launch.fields[0].kind, FieldKind::Select);
        assert_eq!(launch.fields[3].default, Some(serde_json::json!(1)));
        assert_eq!(manifest.command_line(), "steam.exe");
        assert!(manifest.action("other").is_none());
    }

    #[test]
    fn refuses_bad_manifests() {
        let bad = [
            r#"{ "id": "Exemplo", "name": "x", "version": "1", "api": 1, "run": ["a"] }"#,
            r#"{ "id": "a..b", "name": "x", "version": "1", "api": 1, "run": ["a"] }"#,
            r#"{ "id": "a.b", "name": " ", "version": "1", "api": 1, "run": ["a"] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 0, "run": ["a"] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 1, "run": [] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 1, "run": ["a"], "actions": [ { "id": "go", "name": "Ir" }, { "id": "go", "name": "De novo" } ] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 1, "run": ["a"], "actions": [ { "id": "go", "name": "Ir", "icon": "../fora.png" } ] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 1, "run": ["a"], "actions": [ { "id": "go", "name": "Ir", "icon": "naoexiste" } ] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 1, "run": ["a"], "actions": [ { "id": "go", "name": "Ir", "fields": [ { "id": "l", "type": "select", "label": "L" } ] } ] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 1, "run": ["a"], "actions": [ { "id": "go", "name": "Ir", "fields": [ { "id": "t", "type": "text", "label": "T", "dynamic": true } ] } ] }"#,
            r#"{ "id": "a.b", "name": "x", "version": "1", "api": 1, "run": ["a"], "actions": [ { "id": "go", "name": "Ir", "fields": [ { "id": "t", "type": "text", "label": "T" }, { "id": "t", "type": "number", "label": "N" } ] } ] }"#,
        ];
        for json in bad {
            assert!(manifest(json).is_err(), "deveria recusar {json}");
        }
    }

    #[test]
    fn a_newer_protocol_says_to_update() {
        let newer = MINIMAL.replace(r#""api": 1"#, r#""api": 2"#);
        let error = manifest(&newer).unwrap_err().to_string();
        assert!(error.contains("versão 2") && error.contains("atualize"), "{error}");
    }

    #[test]
    fn icons_stay_inside_the_folder() {
        assert!(check_icon("play").is_ok());
        assert!(check_icon("icons/a.svg").is_ok());
        for outside in ["../a.png", r"C:\a.png", r"\\servidor\a.png", "/a.png", "a.jpg"] {
            assert!(check_icon(outside).is_err(), "{outside}");
        }
    }

    #[test]
    fn programs_are_found_in_the_folder_then_in_path() {
        let folder = std::env::temp_dir().join("deck-engine-addon-command-test");
        std::fs::create_dir_all(folder.join("bin")).unwrap();
        std::fs::write(folder.join("tool.exe"), b"").unwrap();
        std::fs::write(folder.join("bin").join("run.cmd"), b"").unwrap();
        let with = |run: &str| Manifest { run: vec![run.into(), "x".into()], ..manifest(MINIMAL).unwrap() };
        let (program, args) = with("tool").command(&folder).map(|(p, a)| (p, a.to_vec())).unwrap();
        assert_eq!(program, folder.join("tool.exe"));
        assert_eq!(args, ["x"]);
        assert_eq!(with("bin/run.cmd").command(&folder).unwrap().0, folder.join("bin/run.cmd"));
        assert!(with("bin/nada.cmd").command(&folder).is_err());
        assert!(with("programa-que-nao-existe-em-lugar-nenhum").command(&folder).is_err());
    }

    #[test]
    fn scanning_reports_every_folder() {
        let dir = std::env::temp_dir().join("deck-engine-addon-scan-test");
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, json) in [("a", Some(MINIMAL)), ("b", Some(MINIMAL)), ("c", None), ("d", Some("{ nada"))] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            if let Some(json) = json {
                std::fs::write(dir.join(folder).join(MANIFEST), format!("\u{feff}{json}")).unwrap();
            }
        }
        let found = scan(&dir);
        let names: Vec<String> = found.iter().map(|f| folder_name(&f.folder)).collect();
        assert_eq!(names, ["a", "b", "c", "d"]);
        assert_eq!(found[0].manifest.as_ref().unwrap().id, "exemplo.contador", "a BOM is fine");
        assert!(found[1].manifest.as_ref().unwrap_err().contains("a pasta a já tem"));
        assert!(found[2].manifest.as_ref().unwrap_err().contains("falta o addon.json"));
        assert!(found[3].manifest.as_ref().unwrap_err().contains("não é válido"));
        assert!(scan(&dir.join("nao-existe")).is_empty());
    }
}
