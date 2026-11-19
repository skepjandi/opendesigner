//! Downloadable packs: more icon sets and the Kenney CC0 packs. They live in the per-user config
//! folder (`<config>/asset-packs/<id>/`), never in a project, each as one compressed container
//! plus a small `meta.json`, and are deleted on request.
//!
//! A download needs the user's yes: an agent's `install_asset_pack` only records a request that
//! the Connect tab shows; the user's click approves it and starts the download. The approval is
//! used up by the download, so installing again after an unload asks again.
//!
//! `OPENDESIGNER_ASSETS_URL` points every pack at `<base>/<pack id>.zip|.tgz` instead of its real
//! address; the tests use it (through the `base` argument) against a local server.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::archive::{self, Limits};
use super::catalog::{self, Download, Format, PackSpec};
use super::container::{self, Entry};
use super::{Error, Result, io, msg};
use crate::http;

/// The environment variable that points the packs at another server (for tests).
pub const URL_ENV: &str = "OPENDESIGNER_ASSETS_URL";
/// The most all installed packs may take on disk.
pub const TOTAL_CAP_BYTES: u64 = 256 * 1024 * 1024;
const MIN_DOWNLOAD_CAP: u64 = 8 * 1024 * 1024;
const MAX_DOWNLOAD_CAP: u64 = 64 * 1024 * 1024;

/// The server the packs come from instead of their own, when the environment names one.
pub fn base_override() -> Option<String> {
    std::env::var(URL_ENV).ok().filter(|b| !b.trim().is_empty())
}

/// Where packs live.
pub fn root() -> Result<PathBuf> {
    crate::token::config_dir()
        .map(|d| d.join("asset-packs"))
        .map_err(|e| Error::Msg(e.to_string()))
}

/// What is written next to an installed pack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Meta {
    pub id: String,
    pub url: String,
    pub sha256: String,
    pub archive_bytes: u64,
    pub stored_bytes: u64,
    pub files: usize,
    pub installed_unix: u64,
}

/// A pack as the agent and the window see it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PackState {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub about: String,
    pub licence: String,
    pub page: String,
    /// What the download takes, in bytes (as read from the source).
    pub download_bytes: u64,
    /// available | requested | downloading | failed | installed
    pub state: String,
    pub error: Option<String>,
    /// On disk once installed.
    pub stored_bytes: u64,
    pub files: usize,
    pub sha256: Option<String>,
    pub installed_unix: Option<u64>,
}

fn dir(root: &Path, id: &str) -> PathBuf {
    root.join(id)
}

fn meta_of(root: &Path, id: &str) -> Option<Meta> {
    serde_json::from_str(&std::fs::read_to_string(dir(root, id).join("meta.json")).ok()?).ok()
}

/// The ids of the installed packs.
pub fn installed_ids(root: &Path) -> Vec<String> {
    catalog::PACKS
        .iter()
        .filter(|p| dir(root, p.id).join("pack.oda").is_file())
        .map(|p| p.id.to_string())
        .collect()
}

/// The entries of an installed pack.
pub fn load(root: &Path, id: &str) -> Result<Vec<Entry>> {
    let path = dir(root, id).join("pack.oda");
    container::decode(&std::fs::read(&path).map_err(|e| io(&path, e))?)
}

/// What every installed pack takes on disk.
pub fn total_bytes(root: &Path) -> u64 {
    installed_ids(root)
        .iter()
        .filter_map(|id| meta_of(root, id))
        .map(|m| m.stored_bytes)
        .sum()
}

/// Packs a download is under way for (`None`) or that failed (`Some(reason)`).
fn busy() -> &'static Mutex<HashMap<String, Option<String>>> {
    static BUSY: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    BUSY.get_or_init(Default::default)
}

/// Every pack with its state.
pub fn states(root: &Path) -> Vec<PackState> {
    let asked = read_list(root, "requests.json");
    let running = busy().lock().map(|b| b.clone()).unwrap_or_default();
    catalog::PACKS
        .iter()
        .map(|spec| {
            let meta =
                meta_of(root, spec.id).filter(|_| dir(root, spec.id).join("pack.oda").is_file());
            let (state, error) = match (&meta, running.get(spec.id)) {
                (_, Some(None)) => ("downloading", None),
                (_, Some(Some(why))) => ("failed", Some(why.clone())),
                (Some(_), None) => ("installed", None),
                (None, None) if asked.iter().any(|a| a == spec.id) => ("requested", None),
                (None, None) => ("available", None),
            };
            PackState {
                id: spec.id.into(),
                name: spec.name.into(),
                kind: spec.kind.into(),
                about: spec.about.into(),
                licence: spec.licence.into(),
                page: spec.page.into(),
                download_bytes: spec.download_bytes,
                state: state.into(),
                error,
                stored_bytes: meta.as_ref().map_or(0, |m| m.stored_bytes),
                files: meta.as_ref().map_or(0, |m| m.files),
                sha256: meta.as_ref().map(|m| m.sha256.clone()),
                installed_unix: meta.as_ref().map(|m| m.installed_unix),
            }
        })
        .collect()
}

fn read_list(root: &Path, file: &str) -> Vec<String> {
    std::fs::read_to_string(root.join(file))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn write_list(root: &Path, file: &str, list: &[String]) -> Result<()> {
    std::fs::create_dir_all(root).map_err(|e| io(root, e))?;
    let path = root.join(file);
    let text = serde_json::to_string(list).map_err(|e| Error::Msg(e.to_string()))?;
    std::fs::write(&path, text).map_err(|e| io(&path, e))
}

fn known(id: &str) -> Result<&'static PackSpec> {
    catalog::pack(id).ok_or_else(|| {
        Error::Msg(format!(
            "there is no pack {id:?}; list_asset_packs names them"
        ))
    })
}

/// Records that an agent wants a pack, for the user to see.
pub fn request(root: &Path, id: &str) -> Result<()> {
    known(id)?;
    let mut list = read_list(root, "requests.json");
    if !list.iter().any(|a| a == id) {
        list.push(id.into());
        write_list(root, "requests.json", &list)?;
    }
    Ok(())
}

/// Packs agents asked for and the user has not answered.
pub fn requests(root: &Path) -> Vec<String> {
    read_list(root, "requests.json")
}

/// The user's yes for one download.
pub fn approve(root: &Path, id: &str) -> Result<()> {
    known(id)?;
    let mut list = read_list(root, "approved.json");
    if !list.iter().any(|a| a == id) {
        list.push(id.into());
        write_list(root, "approved.json", &list)?;
    }
    Ok(())
}

pub fn is_approved(root: &Path, id: &str) -> bool {
    read_list(root, "approved.json").iter().any(|a| a == id)
}

/// The user's no (or a used-up yes).
fn settle(root: &Path, id: &str) {
    for file in ["approved.json", "requests.json"] {
        let mut list = read_list(root, file);
        let before = list.len();
        list.retain(|a| a != id);
        if list.len() != before {
            let _ = write_list(root, file, &list);
        }
    }
}

/// The user says no to a request.
pub fn dismiss(root: &Path, id: &str) {
    settle(root, id);
}

/// The address of a pack's archive.
fn url_of(spec: &PackSpec, base: Option<&str>) -> Result<(String, Option<String>)> {
    if let Some(base) = base {
        let ext = if spec.format == Format::Zip {
            "zip"
        } else {
            "tgz"
        };
        return Ok((
            format!("{}/{}.{ext}", base.trim_end_matches('/'), spec.id),
            None,
        ));
    }
    match spec.download {
        Download::Url(u) => Ok((u.to_string(), None)),
        Download::Kenney(slug) => {
            let hex: String = format!("assets/{slug}")
                .bytes()
                .map(|b| format!("{b:02x}"))
                .collect();
            let page = format!("https://kenney.nl/assets/{slug}");
            let answer = http::get_with_referer(
                &format!("https://kenney.nl/download-link/{hex}"),
                64 * 1024,
                Some(&page),
            )?;
            let url = (answer.status == 200)
                .then(|| serde_json::from_slice::<serde_json::Value>(&answer.body).ok())
                .flatten()
                .and_then(|v| v["url"].as_str().map(str::to_string));
            let Some(url) = url else {
                return msg(format!(
                    "kenney.nl did not give a download link for {slug} (it answered {}); download the zip from {page} yourself",
                    answer.status
                ));
            };
            let (a, b) = (http::parse_url(&url)?, http::parse_url(&page)?);
            if !a.same_site(&b) {
                return msg("kenney.nl pointed the download at another site; not following it");
            }
            Ok((url, Some(page)))
        }
    }
}

fn keep(spec: &PackSpec, path: &str) -> bool {
    let ext = path
        .rsplit_once('.')
        .map_or("", |(_, e)| e)
        .to_ascii_lowercase();
    (spec.prefixes.is_empty() || spec.prefixes.iter().any(|p| path.starts_with(p)))
        && spec.exts.iter().any(|e| *e == ext)
}

fn is_licence_file(path: &str) -> bool {
    !path.contains('/') && path.to_ascii_lowercase().starts_with("licen")
}

/// Downloads a pack and stores what it keeps. The caller has the user's approval; nothing here
/// asks. `base` replaces the pack's address (tests).
pub fn install(root: &Path, id: &str, base: Option<&str>) -> Result<Meta> {
    let spec = known(id)?;
    if dir(root, id).join("pack.oda").is_file() {
        return meta_of(root, id).ok_or_else(|| {
            Error::Msg(format!(
                "{id} is installed but its meta.json is unreadable; unload it and install again"
            ))
        });
    }
    let (url, referer) = url_of(spec, base)?;
    let cap = (spec.download_bytes.saturating_mul(3)).clamp(MIN_DOWNLOAD_CAP, MAX_DOWNLOAD_CAP);
    let answer = http::get_with_referer(&url, cap, referer.as_deref())?;
    if answer.status != 200 {
        return msg(format!(
            "the server answered {} for {}",
            answer.status, spec.name
        ));
    }
    let sha256: String = Sha256::digest(&answer.body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let wanted = |p: &str| keep(spec, p) || is_licence_file(p);
    let members = match spec.format {
        Format::Zip => archive::read_zip(&answer.body, &wanted, Limits::default())?,
        Format::Tgz => archive::read_tgz(&answer.body, spec.strip_top, &wanted, Limits::default())?,
    };
    let mut entries: Vec<Entry> = members
        .into_iter()
        .map(|m| Entry {
            name: if is_licence_file(&m.path) {
                "LICENSE".into()
            } else {
                m.path
            },
            tags: String::new(),
            body: m.data,
        })
        .collect();
    let files = entries.iter().filter(|e| e.name != "LICENSE").count();
    if files == 0 {
        return msg(format!(
            "the archive for {} held none of the files the pack keeps",
            spec.name
        ));
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let packed = container::encode(&entries)?;
    let used = total_bytes(root);
    if used + packed.len() as u64 > TOTAL_CAP_BYTES {
        return msg(format!(
            "installed packs would take {} MB, over the {} MB cap; unload one first (unload_asset_pack)",
            (used + packed.len() as u64) / 1_048_576,
            TOTAL_CAP_BYTES / 1_048_576
        ));
    }
    let folder = dir(root, id);
    std::fs::create_dir_all(&folder).map_err(|e| io(&folder, e))?;
    let tmp = folder.join("pack.oda.tmp");
    std::fs::write(&tmp, &packed).map_err(|e| io(&tmp, e))?;
    let meta = Meta {
        id: id.into(),
        url: url.split('?').next().unwrap_or(&url).to_string(),
        sha256,
        archive_bytes: answer.body.len() as u64,
        stored_bytes: packed.len() as u64,
        files,
        installed_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
    };
    let text = serde_json::to_string_pretty(&meta).map_err(|e| Error::Msg(e.to_string()))?;
    let meta_path = folder.join("meta.json");
    std::fs::write(&meta_path, text).map_err(|e| io(&meta_path, e))?;
    let target = folder.join("pack.oda");
    std::fs::rename(&tmp, &target).map_err(|e| io(&target, e))?;
    settle(root, id);
    Ok(meta)
}

/// Deletes a pack; returns the bytes freed.
pub fn unload(root: &Path, id: &str) -> Result<u64> {
    known(id)?;
    let freed = meta_of(root, id).map_or(0, |m| m.stored_bytes);
    let folder = dir(root, id);
    match std::fs::remove_dir_all(&folder) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(&folder, e)),
    }
    if let Ok(mut b) = busy().lock() {
        b.remove(id);
    }
    Ok(freed)
}

/// The user's click: approve, then download on a thread so the window stays alive. `states`
/// shows `downloading` and then `installed` or `failed`.
pub fn approve_and_install_in_background(root: PathBuf, id: String) -> Result<()> {
    approve(&root, &id)?;
    {
        let mut b = busy().lock().map_err(|_| Error::Msg("busy".into()))?;
        if matches!(b.get(&id), Some(None)) {
            return Ok(());
        }
        b.insert(id.clone(), None);
    }
    std::thread::spawn(move || {
        let result = install(&root, &id, base_override().as_deref());
        if let Ok(mut b) = busy().lock() {
            match result {
                Ok(_) => {
                    b.remove(&id);
                }
                Err(e) => {
                    b.insert(id.clone(), Some(e.to_string()));
                }
            }
        }
    });
    Ok(())
}
