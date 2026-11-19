//! Project folders: `od.toml`, `pages/<id>.json`, `.od/history/{log.jsonl,checkpoints.json}`.
//! Serialization is deterministic so a saved project is byte-identical when nothing changed.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::annotations::Annotations;
use crate::error::ProjectError;
use crate::history::{Checkpoint, Session, Transaction};
use crate::limits::MAX_FILE_BYTES;
use crate::model::{Document, FORMAT_VERSION, NodeId, Page};

const MANIFEST: &str = "od.toml";
const HISTORY_DIR: &str = ".od/history";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    format: u32,
    name: String,
    next_id: u64,
    pages: Vec<String>,
    // Tables must come last in TOML.
    #[serde(default)]
    tokens: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    locales: BTreeMap<String, BTreeMap<String, String>>,
}

fn io_err(path: &Path, source: std::io::Error) -> ProjectError {
    ProjectError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn parse_err(path: &Path, message: impl ToString) -> ProjectError {
    ProjectError::Parse {
        path: path.display().to_string(),
        message: message.to_string(),
    }
}

fn read_limited(path: &Path) -> Result<String, ProjectError> {
    let size = fs::metadata(path).map_err(|e| io_err(path, e))?.len();
    if size > MAX_FILE_BYTES {
        return Err(ProjectError::TooLarge {
            path: path.display().to_string(),
            size,
            limit: MAX_FILE_BYTES,
        });
    }
    fs::read_to_string(path).map_err(|e| io_err(path, e))
}

/// Writes through a temp file in the same directory and renames, so a crash never leaves a
/// half-written project file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let dir = path
        .parent()
        .ok_or_else(|| parse_err(path, "path has no parent directory"))?;
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let tmp = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp).map_err(|e| io_err(&tmp, e))?;
    file.write_all(bytes).map_err(|e| io_err(&tmp, e))?;
    file.sync_all().map_err(|e| io_err(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| io_err(path, e))
}

fn json_line<T: Serialize>(value: &T, path: &Path) -> Result<String, ProjectError> {
    serde_json::to_string(value).map_err(|e| parse_err(path, e))
}

const ANNOTATIONS: &str = "annotations.json";

pub fn save_annotations(dir: &Path, annotations: &Annotations) -> Result<(), ProjectError> {
    let path = dir.join(ANNOTATIONS);
    let mut json = serde_json::to_string_pretty(annotations).map_err(|e| parse_err(&path, e))?;
    json.push('\n');
    write_atomic(&path, json.as_bytes())
}

/// Annotations are optional: a project without the file has none.
pub fn load_annotations(dir: &Path) -> Result<Annotations, ProjectError> {
    let path = dir.join(ANNOTATIONS);
    if !path.exists() {
        return Ok(Annotations::default());
    }
    serde_json::from_str(&read_limited(&path)?).map_err(|e| parse_err(&path, e))
}

/// Creates a new project folder with one empty page and returns its session.
pub fn create(dir: &Path, name: &str) -> Result<Session, ProjectError> {
    if dir.join(MANIFEST).exists() {
        return Err(ProjectError::Invalid(format!(
            "{} already contains a project",
            dir.display()
        )));
    }
    let mut doc = Document::new(name);
    doc.add_page("Page 1")
        .map_err(|e| ProjectError::Invalid(e.to_string()))?;
    let session = Session::new(doc);
    save(dir, &session)?;
    Ok(session)
}

pub fn save(dir: &Path, session: &Session) -> Result<(), ProjectError> {
    let doc = session.document();
    let pages_dir = dir.join("pages");
    for page in &doc.pages {
        let path = pages_dir.join(format!("{}.json", page.id));
        let mut json = serde_json::to_string_pretty(page).map_err(|e| parse_err(&path, e))?;
        json.push('\n');
        write_atomic(&path, json.as_bytes())?;
    }
    remove_stale_pages(&pages_dir, doc)?;

    let manifest = Manifest {
        format: doc.format,
        name: doc.name.clone(),
        next_id: doc.next_id,
        pages: doc.pages.iter().map(|p| p.id.to_string()).collect(),
        tokens: doc.tokens.clone(),
        locales: doc
            .locales
            .iter()
            .map(|(l, texts)| {
                let texts = texts.iter().map(|(id, t)| (id.to_string(), t.clone()));
                (l.clone(), texts.collect())
            })
            .collect(),
    };
    let toml = toml::to_string(&manifest).map_err(|e| parse_err(&dir.join(MANIFEST), e))?;
    write_atomic(&dir.join(MANIFEST), toml.as_bytes())?;

    let history = dir.join(HISTORY_DIR);
    let log_path = history.join("log.jsonl");
    let mut log = String::new();
    for tx in session.log() {
        log.push_str(&json_line(tx, &log_path)?);
        log.push('\n');
    }
    write_atomic(&log_path, log.as_bytes())?;
    let cp_path = history.join("checkpoints.json");
    let mut cps =
        serde_json::to_string_pretty(session.checkpoints()).map_err(|e| parse_err(&cp_path, e))?;
    cps.push('\n');
    write_atomic(&cp_path, cps.as_bytes())
}

fn remove_stale_pages(pages_dir: &Path, doc: &Document) -> Result<(), ProjectError> {
    let keep: HashSet<String> = doc.pages.iter().map(|p| format!("{}.json", p.id)).collect();
    let Ok(entries) = fs::read_dir(pages_dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(".json") && !keep.contains(&name) {
            fs::remove_file(entry.path()).map_err(|e| io_err(&entry.path(), e))?;
        }
    }
    Ok(())
}

pub fn load(dir: &Path) -> Result<Session, ProjectError> {
    let manifest_path = dir.join(MANIFEST);
    let manifest: Manifest =
        toml::from_str(&read_limited(&manifest_path)?).map_err(|e| parse_err(&manifest_path, e))?;
    if manifest.format > FORMAT_VERSION {
        return Err(ProjectError::NewerFormat {
            found: manifest.format,
            supported: FORMAT_VERSION,
        });
    }
    let mut pages = Vec::with_capacity(manifest.pages.len());
    for page_id in &manifest.pages {
        let id = NodeId::parse(page_id).map_err(|e| ProjectError::Invalid(e.to_string()))?;
        let path = dir.join("pages").join(format!("{id}.json"));
        let page: Page =
            serde_json::from_str(&read_limited(&path)?).map_err(|e| parse_err(&path, e))?;
        if page.id != id {
            return Err(ProjectError::Invalid(format!(
                "{} holds page {}, expected {id}",
                path.display(),
                page.id
            )));
        }
        pages.push(page);
    }
    let mut locales = BTreeMap::new();
    for (locale, texts) in manifest.locales {
        let mut parsed = BTreeMap::new();
        for (id, text) in texts {
            let id = NodeId::parse(&id).map_err(|e| ProjectError::Invalid(e.to_string()))?;
            parsed.insert(id, text);
        }
        locales.insert(locale, parsed);
    }
    let doc = Document {
        format: manifest.format,
        name: manifest.name,
        next_id: manifest.next_id,
        tokens: manifest.tokens,
        locales,
        pages,
    };

    let history = dir.join(HISTORY_DIR);
    let log = read_log(&history.join("log.jsonl"))?;
    let cp_path = history.join("checkpoints.json");
    let checkpoints: Vec<Checkpoint> = if cp_path.exists() {
        serde_json::from_str(&read_limited(&cp_path)?).map_err(|e| parse_err(&cp_path, e))?
    } else {
        Vec::new()
    };
    Session::from_parts(doc, log, checkpoints).map_err(|e| ProjectError::Invalid(e.to_string()))
}

fn read_log(path: &Path) -> Result<Vec<Transaction>, ProjectError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut log = Vec::new();
    for (i, line) in read_limited(path)?.lines().enumerate() {
        let tx: Transaction = serde_json::from_str(line)
            .map_err(|e| parse_err(path, format!("line {}: {e}", i + 1)))?;
        log.push(tx);
    }
    Ok(log)
}
