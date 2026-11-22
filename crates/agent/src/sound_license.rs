//! What every sound in a project owes to its maker: the licence it came with, written next to the
//! file as `media/<name>.license.json`, the `commercial` setting that decides which licences a
//! project may take, and the credits a finished video must show (`docs/wiki/security.md#network`,
//! `docs/wiki/video-editing.md#finding-sounds`).
//!
//! The sidecar is the one record of a sound's licence, whatever its source: a Freesound or
//! Openverse download, a track from Incompetech or ccMixter, or a sound the app synthesized.
//! Nothing here is guessed: a licence this module cannot name is stored as unknown, and an
//! unknown licence is never cleared for a commercial project.

use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use od_core::SoundTrack;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::http;

/// The sidecar's file name ending: `media/<name>.license.json`.
pub const SIDECAR_SUFFIX: &str = ".license.json";
/// The sidecar older versions wrote for a Freesound sound; still read, never written.
const LEGACY_FREESOUND_SUFFIX: &str = ".freesound.json";
/// The marker of a synthesized sound, which has no licence to honour.
const SYNTH_SUFFIX: &str = ".synth.json";
/// The project's sound settings.
pub const SETTINGS_FILE: &str = "sound-settings.json";
/// The most a downloaded sound may take.
pub const MAX_AUDIO_BYTES: u64 = 32 * 1024 * 1024;
/// The sidecar format this version writes.
pub const SCHEMA: u32 = 1;

const CC_BASE: &str = "https://creativecommons.org";

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Http(#[from] http::HttpError),
    #[error("{provider} answered {status}: {detail}")]
    Api {
        provider: &'static str,
        status: u16,
        detail: String,
    },
    #[error("{provider} answered something this client cannot read: {detail}")]
    Malformed {
        provider: &'static str,
        detail: String,
    },
    #[error("{provider} has no sound with the id {id}")]
    NotFound { provider: &'static str, id: String },
    #[error("the file of {id} is on {host}, which is not a download site of {provider}")]
    ForeignHost {
        provider: &'static str,
        id: String,
        host: String,
    },
    #[error("the file name {0:?} is not a plain name")]
    BadName(String),
    #[error("{0} has no file to download")]
    NoDownload(String),
    #[error("{0}")]
    NotAllowed(String),
    #[error("the download is not a sound this app can use (.mp3 .wav .m4a .aac .ogg)")]
    NotSound,
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

fn io_error(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Where sounds can be searched for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Freesound,
    Openverse,
    Incompetech,
    Ccmixter,
}

impl Provider {
    pub const NAMES: [&'static str; 4] = ["freesound", "openverse", "incompetech", "ccmixter"];

    pub fn parse(name: &str) -> Option<Provider> {
        Some(match name {
            "freesound" => Provider::Freesound,
            "openverse" => Provider::Openverse,
            "incompetech" => Provider::Incompetech,
            "ccmixter" => Provider::Ccmixter,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Provider::Freesound => "freesound",
            Provider::Openverse => "openverse",
            Provider::Incompetech => "incompetech",
            Provider::Ccmixter => "ccmixter",
        }
    }
}

/// The licence of one sound, in the terms that decide whether a project may use it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct License {
    /// An SPDX-like identifier (`CC-BY-4.0`, `CC0-1.0`); empty when the licence is unknown.
    pub spdx: String,
    pub url: String,
    pub attribution_required: bool,
    pub commercial_ok: bool,
    pub derivatives_ok: bool,
    pub share_alike: bool,
}

impl License {
    /// A licence nobody could name: not cleared for anything.
    pub fn unknown(url: &str) -> License {
        License {
            spdx: String::new(),
            url: url.to_string(),
            attribution_required: true,
            commercial_ok: false,
            derivatives_ok: false,
            share_alike: false,
        }
    }

    /// A Creative Commons licence by the short name Openverse and creativecommons.org use:
    /// `cc0`, `pdm`, `by`, `by-sa`, `by-nd`, `by-nc`, `by-nc-sa`, `by-nc-nd`, and its version.
    pub fn creative_commons(kind: &str, version: &str) -> License {
        let kind = kind.trim().to_ascii_lowercase();
        match kind.as_str() {
            "cc0" => return public_domain("CC0-1.0", &format!("{CC_BASE}/publicdomain/zero/1.0/")),
            "pdm" => return public_domain("PDM-1.0", &format!("{CC_BASE}/publicdomain/mark/1.0/")),
            _ => {}
        }
        let version = version.trim();
        let version_ok = !version.is_empty()
            && version
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
        let parts: Vec<&str> = kind.split('-').collect();
        let known = matches!(
            parts.as_slice(),
            ["by"]
                | ["by", "sa"]
                | ["by", "nd"]
                | ["by", "nc"]
                | ["by", "nc", "sa"]
                | ["by", "nc", "nd"]
        );
        if !known || !version_ok {
            return License::unknown("");
        }
        License {
            spdx: format!("CC-{}-{version}", kind.to_ascii_uppercase()),
            url: format!("{CC_BASE}/licenses/{kind}/{version}/"),
            attribution_required: true,
            commercial_ok: !parts.contains(&"nc"),
            derivatives_ok: !parts.contains(&"nd"),
            share_alike: parts.contains(&"sa"),
        }
    }

    /// A licence from the address of its deed, `creativecommons.org/licenses/by/4.0/` and the
    /// like, over http or https; anything else is unknown.
    pub fn from_url(url: &str) -> License {
        let trimmed = url.trim();
        let rest = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .map(|r| r.strip_prefix("www.").unwrap_or(r));
        let Some(path) = rest.and_then(|r| r.strip_prefix("creativecommons.org/")) else {
            return License::unknown(trimmed);
        };
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        match segments.as_slice() {
            ["publicdomain", "zero", v] if *v == "1.0" => License::creative_commons("cc0", v),
            ["publicdomain", "mark", v] if *v == "1.0" => License::creative_commons("pdm", v),
            ["licenses", kind, version] => {
                let found = License::creative_commons(kind, version);
                if found.spdx.is_empty() {
                    License::unknown(trimmed)
                } else {
                    found
                }
            }
            _ => License::unknown(trimmed),
        }
    }

    /// A licence by the name a site gives it (Freesound's `Creative Commons 0`, `Attribution`,
    /// `Attribution NonCommercial`, `Sampling+`), or by its address when there is one.
    pub fn from_name(name: &str, url: &str) -> License {
        let by_url = License::from_url(url);
        if !by_url.spdx.is_empty() {
            return by_url;
        }
        let by_name = License::from_url(name);
        if !by_name.spdx.is_empty() {
            return by_name;
        }
        match name.trim().to_ascii_lowercase().as_str() {
            "creative commons 0" | "cc0" | "cc0 1.0" => License::creative_commons("cc0", "1.0"),
            "attribution" | "cc by 4.0" => License::creative_commons("by", "4.0"),
            "attribution noncommercial" | "attribution non-commercial" => {
                License::creative_commons("by-nc", "4.0")
            }
            "sampling+" | "sampling plus" => License {
                spdx: "CC-Sampling+-1.0".to_string(),
                url: format!("{CC_BASE}/licenses/sampling+/1.0/"),
                attribution_required: true,
                commercial_ok: false,
                derivatives_ok: true,
                share_alike: false,
            },
            _ => License::unknown(url),
        }
    }

    /// The licence as a person reads it: `CC BY 4.0`, `CC0 1.0`.
    pub fn label(&self) -> String {
        let parts: Vec<&str> = self.spdx.split('-').collect();
        match parts.as_slice() {
            ["CC0", v] => format!("CC0 {v}"),
            ["CC", middle @ .., version] if !middle.is_empty() => {
                format!("CC {} {version}", middle.join("-"))
            }
            _ if self.spdx.is_empty() => "unknown licence".to_string(),
            _ => self.spdx.clone(),
        }
    }

    /// Whether a project may use the sound. A project that is not commercial takes any licence;
    /// a commercial one takes only what allows commercial use and changes (a sound is trimmed,
    /// mixed and ducked). The error says what is wrong.
    pub fn allowed(&self, commercial: bool) -> Result<(), String> {
        if !commercial {
            return Ok(());
        }
        if self.spdx.is_empty() {
            return Err("its licence is unknown".to_string());
        }
        if !self.commercial_ok {
            return Err(format!("{} forbids commercial use", self.label()));
        }
        if !self.derivatives_ok {
            return Err(format!(
                "{} forbids changes, and a video trims and mixes its sounds",
                self.label()
            ));
        }
        Ok(())
    }
}

fn public_domain(spdx: &str, url: &str) -> License {
    License {
        spdx: spdx.to_string(),
        url: url.to_string(),
        attribution_required: false,
        commercial_ok: true,
        derivatives_ok: true,
        share_alike: false,
    }
}

/// The line a person must show for a sound that has no better one of its own:
/// `"Title" by Creator (page), licensed under CC BY 4.0 (licence url)`.
pub fn credit_line(title: &str, creator: &str, landing_url: &str, license: &License) -> String {
    let mut line = format!("\"{title}\"");
    if !creator.is_empty() {
        line.push_str(&format!(" by {creator}"));
    }
    if !landing_url.is_empty() {
        line.push_str(&format!(" ({landing_url})"));
    }
    line.push_str(&format!(", licensed under {}", license.label()));
    if !license.url.is_empty() {
        line.push_str(&format!(" ({})", license.url));
    }
    line
}

/// Where a sound came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    pub provider: String,
    pub item_id: String,
    pub landing_url: String,
    pub download_url: String,
    pub retrieved_at: String,
    /// `preview` for a lossy preview, `original` for the file as the author uploaded it, and
    /// `synthesized` for a sound made here.
    pub quality: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Creator {
    pub name: String,
    pub url: String,
}

/// How an AI model made the sound, when one did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ai {
    pub model: String,
    pub prompt: String,
}

/// What is written next to a sound: `media/<name>.license.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub schema: u32,
    pub source: Source,
    pub title: String,
    pub creator: Creator,
    pub license: License,
    /// What must be shown wherever the sound is used; empty when nothing is required. May
    /// hold several lines.
    pub credit_line: String,
    /// What was done to the file after it came in.
    #[serde(default)]
    pub modifications: Vec<String>,
    pub sha256: String,
    #[serde(default)]
    pub ai: Option<Ai>,
}

impl Sidecar {
    /// The sidecar of a sound made by the synthesizer: a licence of nobody's, nothing to show.
    pub fn synthesized(kind: &str, wav: &[u8]) -> Sidecar {
        Sidecar {
            schema: SCHEMA,
            source: Source {
                provider: "synth".to_string(),
                item_id: kind.to_string(),
                landing_url: String::new(),
                download_url: String::new(),
                retrieved_at: now_iso(),
                quality: "synthesized".to_string(),
            },
            title: kind.to_string(),
            creator: Creator {
                name: "OpenDesigner".to_string(),
                url: String::new(),
            },
            license: License::creative_commons("cc0", "1.0"),
            credit_line: String::new(),
            modifications: Vec::new(),
            sha256: hex_sha256(wav),
            ai: None,
        }
    }
}

/// The record older versions wrote for a Freesound sound.
#[derive(Debug, Deserialize)]
struct LegacyFreesound {
    id: u64,
    title: String,
    author: String,
    license: String,
    #[serde(default)]
    license_url: String,
    #[serde(default)]
    source_url: String,
}

impl LegacyFreesound {
    fn into_sidecar(self) -> Sidecar {
        let credit_line = format!(
            "{} by {}, {}, {}",
            self.title, self.author, self.license, self.source_url
        );
        Sidecar {
            schema: SCHEMA,
            source: Source {
                provider: "freesound".to_string(),
                item_id: self.id.to_string(),
                landing_url: self.source_url,
                download_url: String::new(),
                retrieved_at: String::new(),
                quality: "preview".to_string(),
            },
            title: self.title,
            creator: Creator {
                name: self.author,
                url: String::new(),
            },
            license: License::from_name(&self.license, &self.license_url),
            credit_line,
            modifications: Vec::new(),
            sha256: String::new(),
            ai: None,
        }
    }
}

/// The licence record of `media/<stem>.*`: the sidecar of this version, or the one an older
/// version wrote for a Freesound sound. A file that cannot be read is no record, never an error:
/// a project with old or damaged sidecars must still open and list.
pub fn read_sidecar(media: &Path, stem: &str) -> Option<Sidecar> {
    let current = std::fs::read_to_string(media.join(format!("{stem}{SIDECAR_SUFFIX}")));
    if let Some(side) = current
        .ok()
        .and_then(|t| serde_json::from_str::<Sidecar>(&t).ok())
    {
        return Some(side);
    }
    std::fs::read_to_string(media.join(format!("{stem}{LEGACY_FREESOUND_SUFFIX}")))
        .ok()
        .and_then(|t| serde_json::from_str::<LegacyFreesound>(&t).ok())
        .map(LegacyFreesound::into_sidecar)
}

/// One sound in a project's `media/` folder and what is known about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    /// `media/<name>.<ext>`
    pub file: String,
    /// The licence sidecar, when there is one.
    pub sidecar: Option<Sidecar>,
    /// The name of the sidecar file, when there is one.
    pub sidecar_file: Option<String>,
    /// Whether the sound was made here (its own `.synth.json`).
    pub synthesized: bool,
}

/// Every sound in a project's `media/` folder with what is known about it. A file with no
/// licence record is listed with none: nothing is guessed.
pub fn list_records(project: &Path) -> Vec<Record> {
    let media = project.join("media");
    let Ok(entries) = std::fs::read_dir(&media) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((stem, ext)) = name.rsplit_once('.') else {
            continue;
        };
        if !od_core::media::AUDIO_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
            continue;
        }
        let side = read_sidecar(&media, stem);
        let sidecar_file = side.as_ref().map(|_| {
            if media.join(format!("{stem}{SIDECAR_SUFFIX}")).is_file() {
                format!("media/{stem}{SIDECAR_SUFFIX}")
            } else {
                format!("media/{stem}{LEGACY_FREESOUND_SUFFIX}")
            }
        });
        out.push(Record {
            file: format!("media/{name}"),
            sidecar: side,
            sidecar_file,
            synthesized: media.join(format!("{stem}{SYNTH_SUFFIX}")).is_file(),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    out
}

/// Adds what a video did to a sound to its sidecar, so the record of the licence says what was
/// changed (Creative Commons asks for that). A sound with no sidecar of this version has nothing
/// to record it in and is left alone.
pub fn record_modifications(media: &Path, stem: &str, notes: &[String]) -> Result<(), Error> {
    if notes.is_empty() {
        return Ok(());
    }
    let path = media.join(format!("{stem}{SIDECAR_SUFFIX}"));
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let Ok(mut sidecar) = serde_json::from_str::<Sidecar>(&text) else {
        return Ok(());
    };
    for note in notes {
        if !sidecar.modifications.contains(note) {
            sidecar.modifications.push(note.clone());
        }
    }
    let out = serde_json::to_string_pretty(&sidecar).map_err(|e| Error::Malformed {
        provider: "sidecar",
        detail: e.to_string(),
    })?;
    std::fs::write(&path, out).map_err(|e| io_error(&path, e))
}

/// Writes the sidecar of a sound; it must not exist yet.
pub fn write_sidecar(media: &Path, stem: &str, sidecar: &Sidecar) -> Result<(), Error> {
    let text = serde_json::to_string_pretty(sidecar).map_err(|e| Error::Malformed {
        provider: "sidecar",
        detail: e.to_string(),
    })?;
    write_new(
        &media.join(format!("{stem}{SIDECAR_SUFFIX}")),
        text.as_bytes(),
    )
}

/// The stem of a `media/<name>.<ext>` reference.
fn stem_of(file: &str) -> &str {
    let name = file.rsplit('/').next().unwrap_or(file);
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

/// A project's sound settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Whether the videos of this project may earn money or promote a business. While true,
    /// sounds with a non-commercial, no-derivatives or unknown licence are hidden and refused.
    #[serde(default = "yes")]
    pub commercial: bool,
    /// Why the setting was last changed, in the user's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_at: Option<String>,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            commercial: true,
            reason: None,
            changed_at: None,
        }
    }
}

/// The settings of a project folder; missing or unreadable ones are the safe default.
pub fn load_settings(dir: &Path) -> Settings {
    std::fs::read_to_string(dir.join(SETTINGS_FILE))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_settings(dir: &Path, settings: &Settings) -> Result<(), Error> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| Error::Malformed {
        provider: "settings",
        detail: e.to_string(),
    })?;
    let path = dir.join(SETTINGS_FILE);
    std::fs::write(&path, text).map_err(|e| io_error(&path, e))
}

/// Records a change of the `commercial` setting with the reason for it.
pub fn set_commercial(dir: &Path, commercial: bool, reason: &str) -> Result<Settings, Error> {
    let settings = Settings {
        commercial,
        reason: Some(reason.trim().to_string()).filter(|r| !r.is_empty()),
        changed_at: Some(now_iso()),
    };
    save_settings(dir, &settings)?;
    Ok(settings)
}

/// One sound as a search found it, whatever site it came from.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub provider: &'static str,
    pub id: String,
    pub title: String,
    pub creator: String,
    pub creator_url: String,
    pub duration_ms: u64,
    pub license: License,
    /// What must be shown wherever the sound is used.
    pub attribution: String,
    /// The file to download: a preview, or the track itself.
    pub download_url: String,
    pub landing_url: String,
    /// Where it is hosted (`freesound`, `jamendo`, `wikimedia_audio`, ...).
    pub origin: String,
    /// `preview` or `original`.
    pub quality: &'static str,
}

impl Candidate {
    /// The sidecar for the bytes that were downloaded.
    pub fn sidecar(&self, bytes: &[u8]) -> Sidecar {
        Sidecar {
            schema: SCHEMA,
            source: Source {
                provider: self.provider.to_string(),
                item_id: self.id.clone(),
                landing_url: self.landing_url.clone(),
                download_url: self.download_url.clone(),
                retrieved_at: now_iso(),
                quality: self.quality.to_string(),
            },
            title: self.title.clone(),
            creator: Creator {
                name: self.creator.clone(),
                url: self.creator_url.clone(),
            },
            license: self.license.clone(),
            credit_line: if self.license.attribution_required {
                self.attribution.clone()
            } else {
                String::new()
            },
            modifications: Vec::new(),
            sha256: hex_sha256(bytes),
            ai: None,
        }
    }
}

/// The sites a provider may download from. A host must be one of the suffixes over https; the
/// one host a test server runs on is the only exception.
#[derive(Debug, Clone)]
pub struct Allowlist {
    suffixes: Vec<String>,
    test_host: Option<String>,
}

impl Allowlist {
    /// `override_url` is the address a test pointed the provider at, when it did.
    pub fn new(suffixes: &[&str], override_url: Option<&str>) -> Allowlist {
        Allowlist {
            suffixes: suffixes.iter().map(|s| (*s).to_string()).collect(),
            test_host: override_url
                .and_then(|u| http::parse_url(u).ok())
                .map(|u| u.host),
        }
    }

    pub fn check(&self, provider: Provider, id: &str, url: &str) -> Result<(), Error> {
        let parsed = http::parse_url(url)?;
        if self.test_host.as_deref() == Some(parsed.host.as_str()) {
            return Ok(());
        }
        let on_list = self
            .suffixes
            .iter()
            .any(|s| parsed.host == *s || parsed.host.ends_with(&format!(".{s}")));
        if on_list && parsed.secure && parsed.port == 443 {
            return Ok(());
        }
        Err(Error::ForeignHost {
            provider: provider.name(),
            id: id.to_string(),
            host: parsed.host,
        })
    }
}

/// What an import brought in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Imported {
    /// `media/<name>.mp3`
    pub file: String,
    pub sidecar: String,
    /// What must be shown wherever the sound is used; empty for a licence that asks for nothing.
    pub attribution: String,
    pub license: String,
    pub attribution_required: bool,
}

/// Refuses a sound the project's `commercial` setting does not clear, with how to switch.
pub fn check_license(candidate: &Candidate, commercial: bool) -> Result<(), Error> {
    candidate.license.allowed(commercial).map_err(|why| {
        Error::NotAllowed(format!(
            "\"{}\" cannot be used in this project: {why}. This project is set to commercial use. \
             Pick a sound with a licence that allows it (the search hides the others), or, if \
             the video is not commercial, ask the user and then call sound_settings with \
             commercial false and their words as the reason",
            candidate.title
        ))
    })
}

/// Downloads a candidate into the project's `media/` folder with its licence sidecar. The host
/// must be on the allowlist, the licence must be cleared for the project, and the file must be a
/// sound by its first bytes.
pub fn store(
    project: &Path,
    provider: Provider,
    candidate: &Candidate,
    allow: &Allowlist,
    name: Option<&str>,
    commercial: bool,
) -> Result<Imported, Error> {
    check_license(candidate, commercial)?;
    if candidate.download_url.is_empty() {
        return Err(Error::NoDownload(candidate.title.clone()));
    }
    allow.check(provider, &candidate.id, &candidate.download_url)?;
    let answer = http::get(&candidate.download_url, MAX_AUDIO_BYTES)?;
    if answer.status != 200 {
        return Err(Error::Api {
            provider: provider.name(),
            status: answer.status,
            detail: "the file could not be downloaded".to_string(),
        });
    }
    store_bytes(project, candidate, &answer.body, name, commercial)
}

/// Asks a provider's API and hands back the answer's body. A refusal carries the reason the
/// API gave, when it gave one.
pub fn get_json(provider: Provider, url: &str, max_bytes: u64) -> Result<String, Error> {
    let answer = http::get(url, max_bytes)?;
    if answer.status != 200 {
        let detail = serde_json::from_slice::<serde_json::Value>(&answer.body)
            .ok()
            .and_then(|v| {
                ["detail", "error", "message"]
                    .iter()
                    .find_map(|k| v.get(k).and_then(|d| d.as_str()).map(str::to_string))
            })
            .unwrap_or_else(|| "no reason given".to_string());
        return Err(Error::Api {
            provider: provider.name(),
            status: answer.status,
            detail,
        });
    }
    String::from_utf8(answer.body).map_err(|e| Error::Malformed {
        provider: provider.name(),
        detail: e.to_string(),
    })
}

/// Writes downloaded bytes and their sidecar. Nothing is overwritten: a free name is found.
pub fn store_bytes(
    project: &Path,
    candidate: &Candidate,
    bytes: &[u8],
    name: Option<&str>,
    commercial: bool,
) -> Result<Imported, Error> {
    check_license(candidate, commercial)?;
    if bytes.is_empty() {
        return Err(Error::NotSound);
    }
    let head = &bytes[..bytes.len().min(12)];
    let ext = crate::device::sniff_audio(head).ok_or(Error::NotSound)?;
    let media = project.join("media");
    std::fs::create_dir_all(&media).map_err(|e| io_error(&media, e))?;
    let stem = match name {
        Some(n) => clean_name(n)?,
        None => {
            clean_name(&candidate.title).unwrap_or_else(|_| format!("sound-{}", candidate.provider))
        }
    };
    let stem = free_stem(&media, &stem)?;
    let audio = media.join(format!("{stem}.{ext}"));
    write_new(&audio, bytes)?;
    let sidecar = candidate.sidecar(bytes);
    if let Err(e) = write_sidecar(&media, &stem, &sidecar) {
        let _ = std::fs::remove_file(&audio);
        return Err(e);
    }
    Ok(Imported {
        file: format!("media/{stem}.{ext}"),
        sidecar: format!("media/{stem}{SIDECAR_SUFFIX}"),
        attribution: sidecar.credit_line,
        license: sidecar.license.spdx,
        attribution_required: sidecar.license.attribution_required,
    })
}

/// A plain file name: letters, digits, space, `.`, `_` and `-`.
pub fn clean_name(name: &str) -> Result<String, Error> {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(['.', '-', ' ']).to_string();
    if trimmed.is_empty() {
        return Err(Error::BadName(name.to_string()));
    }
    Ok(trimmed.chars().take(60).collect())
}

/// A file name in `media/` that is taken by no sound and no record of one.
fn free_stem(media: &Path, stem: &str) -> Result<String, Error> {
    for n in 1..10_000 {
        let name = if n == 1 {
            stem.to_string()
        } else {
            format!("{stem}-{n}")
        };
        let taken = od_core::media::AUDIO_EXTENSIONS
            .iter()
            .any(|ext| media.join(format!("{name}.{ext}")).exists())
            || media.join(format!("{name}{SIDECAR_SUFFIX}")).exists()
            || media
                .join(format!("{name}{LEGACY_FREESOUND_SUFFIX}"))
                .exists();
        if !taken {
            return Ok(name);
        }
    }
    Err(Error::BadName(stem.to_string()))
}

/// Writes a file that must not exist yet.
pub fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| io_error(path, e))?;
    file.write_all(bytes).map_err(|e| {
        let _ = std::fs::remove_file(path);
        io_error(path, e)
    })
}

pub fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The time as `2026-11-22T09:30:00Z`.
pub fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let (y, m, d) = civil((secs / 86_400) as i64);
    let rest = secs % 86_400;
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

/// Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A sound file a video uses, and what the video does to it.
#[derive(Debug, Clone, PartialEq)]
pub struct UsedSound {
    /// `media/<name>.<ext>`
    pub file: String,
    pub modifications: Vec<String>,
}

/// What a track does to its file, in the words credits use: Creative Commons asks that a
/// changed work says so.
pub fn modifications_of(track: &SoundTrack) -> Vec<String> {
    let mut notes = Vec::new();
    if track.in_ms > 0 || track.out_ms.is_some() {
        notes.push("trimmed".to_string());
    }
    if (track.gain - 1.0).abs() > 1e-3 {
        notes.push(format!("volume changed (x{:.2})", track.gain));
    }
    match (track.fade_in_ms > 0, track.fade_out_ms > 0) {
        (true, true) => notes.push("faded in and out".to_string()),
        (true, false) => notes.push("faded in".to_string()),
        (false, true) => notes.push("faded out".to_string()),
        (false, false) => {}
    }
    if track.pan.abs() > 1e-3 {
        notes.push("panned".to_string());
    }
    if track.duck.is_some() {
        notes.push("lowered under the voice (ducking)".to_string());
    }
    notes
}

/// The credits of a set of sounds, and what could not be settled.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Credits {
    /// The text to show; empty when no sound asks for a credit.
    pub text: String,
    /// Sounds with no licence record (the user's own files, or older imports): their licence is
    /// not known here, so nothing was written for them.
    pub unlicensed: Vec<String>,
    /// Sounds under a ShareAlike licence: what is made with them may have to carry the same one.
    pub share_alike: Vec<String>,
    /// How many credit entries the text holds.
    pub credited: usize,
}

/// The credits text for the sounds a video uses: one entry for each sound whose licence asks
/// for attribution, in order of first use, with a note when it was changed.
pub fn credits_for(project_dir: &Path, used_files: &[UsedSound]) -> String {
    credits_report(project_dir, used_files).text
}

pub fn credits_report(project_dir: &Path, used: &[UsedSound]) -> Credits {
    let media = project_dir.join("media");
    let mut files: Vec<(&str, Vec<&str>)> = Vec::new();
    for sound in used {
        let notes = sound.modifications.iter().map(String::as_str);
        match files.iter_mut().find(|(f, _)| *f == sound.file) {
            Some((_, merged)) => {
                for note in notes {
                    if !merged.contains(&note) {
                        merged.push(note);
                    }
                }
            }
            None => files.push((sound.file.as_str(), notes.collect())),
        }
    }
    let mut credits = Credits::default();
    let mut entries: Vec<(String, Vec<&str>)> = Vec::new();
    for (file, notes) in files {
        let stem = stem_of(file);
        let Some(side) = read_sidecar(&media, stem) else {
            if !media.join(format!("{stem}{SYNTH_SUFFIX}")).is_file() {
                credits.unlicensed.push(file.to_string());
            }
            continue;
        };
        if side.license.share_alike {
            credits.share_alike.push(file.to_string());
        }
        let line = side.credit_line.trim();
        if !side.license.attribution_required || line.is_empty() {
            continue;
        }
        match entries.iter_mut().find(|(l, _)| l == line) {
            Some((_, merged)) => {
                for note in notes {
                    if !merged.contains(&note) {
                        merged.push(note);
                    }
                }
            }
            None => entries.push((line.to_string(), notes)),
        }
    }
    if !entries.is_empty() {
        credits.credited = entries.len();
        let blocks: Vec<String> = entries
            .into_iter()
            .map(|(line, notes)| {
                if notes.is_empty() {
                    line
                } else {
                    format!("{line}\nModified: {}.", notes.join(", "))
                }
            })
            .collect();
        credits.text = format!("Sound credits\n\n{}\n", blocks.join("\n\n"));
    }
    credits
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn incompetech_line(title: &str) -> String {
        format!(
            "\"{title}\" Kevin MacLeod (incompetech.com)\nLicensed under Creative Commons: By Attribution 4.0\nhttps://creativecommons.org/licenses/by/4.0/"
        )
    }

    fn candidate(license: License, line: &str) -> Candidate {
        Candidate {
            provider: "openverse",
            id: "abc".into(),
            title: "Bell".into(),
            creator: "Someone".into(),
            creator_url: String::new(),
            duration_ms: 1000,
            license,
            attribution: line.into(),
            download_url: "https://cdn.freesound.org/x.mp3".into(),
            landing_url: "https://freesound.org/s/1/".into(),
            origin: "freesound".into(),
            quality: "preview",
        }
    }

    fn mp3() -> Vec<u8> {
        let mut b = b"ID3\x03\0\0\0\0\0\0".to_vec();
        b.extend_from_slice(&[0u8; 32]);
        b
    }

    #[test]
    fn creative_commons_licences_are_normalised() {
        let by = License::creative_commons("by", "4.0");
        assert_eq!(by.spdx, "CC-BY-4.0");
        assert_eq!(by.url, "https://creativecommons.org/licenses/by/4.0/");
        assert!(by.attribution_required && by.commercial_ok && by.derivatives_ok);
        let nc_sa = License::creative_commons("by-nc-sa", "3.0");
        assert_eq!(nc_sa.spdx, "CC-BY-NC-SA-3.0");
        assert!(!nc_sa.commercial_ok && nc_sa.derivatives_ok && nc_sa.share_alike);
        let nd = License::creative_commons("by-nd", "2.0");
        assert!(nd.commercial_ok && !nd.derivatives_ok);
        let zero = License::creative_commons("cc0", "1.0");
        assert_eq!(zero.spdx, "CC0-1.0");
        assert!(!zero.attribution_required && zero.commercial_ok);
        assert!(License::creative_commons("by", "x").spdx.is_empty());
        assert!(License::creative_commons("gpl", "3.0").spdx.is_empty());
        assert_eq!(by.label(), "CC BY 4.0");
        assert_eq!(nc_sa.label(), "CC BY-NC-SA 3.0");
        assert_eq!(zero.label(), "CC0 1.0");
    }

    #[test]
    fn licences_are_read_from_deed_addresses_and_site_names() {
        let a = License::from_url("http://creativecommons.org/licenses/by/2.5/");
        assert_eq!(a.spdx, "CC-BY-2.5");
        assert_eq!(a.url, "https://creativecommons.org/licenses/by/2.5/");
        assert_eq!(
            License::from_url("https://creativecommons.org/publicdomain/zero/1.0/").spdx,
            "CC0-1.0"
        );
        assert!(
            License::from_url("https://example.com/licenses/by/4.0/")
                .spdx
                .is_empty()
        );
        assert_eq!(License::from_name("Creative Commons 0", "").spdx, "CC0-1.0");
        assert_eq!(License::from_name("Attribution", "").spdx, "CC-BY-4.0");
        assert_eq!(
            License::from_name("Attribution NonCommercial", "").spdx,
            "CC-BY-NC-4.0"
        );
        let sampling = License::from_name("Sampling+", "");
        assert_eq!(sampling.spdx, "CC-Sampling+-1.0");
        assert!(!sampling.commercial_ok);
        assert_eq!(
            License::from_name("Attribution", "http://creativecommons.org/licenses/by/3.0/").spdx,
            "CC-BY-3.0"
        );
        assert!(License::from_name("Something else", "").spdx.is_empty());
    }

    #[test]
    fn a_commercial_project_clears_only_commercial_and_changeable_licences() {
        let ok = |l: &License| l.allowed(true).is_ok();
        assert!(ok(&License::creative_commons("cc0", "1.0")));
        assert!(ok(&License::creative_commons("by", "4.0")));
        assert!(ok(&License::creative_commons("by-sa", "4.0")));
        assert!(!ok(&License::creative_commons("by-nc", "4.0")));
        assert!(!ok(&License::creative_commons("by-nd", "4.0")));
        assert!(!ok(&License::creative_commons("by-nc-nd", "4.0")));
        assert!(!ok(&License::unknown("")));
        let nc = License::creative_commons("by-nc", "4.0");
        assert!(nc.allowed(true).unwrap_err().contains("commercial"));
        assert!(nc.allowed(false).is_ok());
        assert!(License::unknown("").allowed(false).is_ok());
    }

    #[test]
    fn a_refused_import_says_how_to_switch_and_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let nc = candidate(License::creative_commons("by-nc", "4.0"), "x");
        let err = store_bytes(d.path(), &nc, &mp3(), None, true)
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("sound_settings") && err.contains("commercial"),
            "{err}"
        );
        assert!(!d.path().join("media").exists());
        assert!(store_bytes(d.path(), &nc, &mp3(), None, false).is_ok());
    }

    #[test]
    fn a_sidecar_is_written_read_back_and_never_overwritten() {
        let d = tempfile::tempdir().unwrap();
        let c = candidate(
            License::creative_commons("by", "4.0"),
            "\"Bell\" by Someone",
        );
        let first = store_bytes(d.path(), &c, &mp3(), Some("bell"), true).unwrap();
        assert_eq!(first.file, "media/bell.mp3");
        assert_eq!(first.sidecar, "media/bell.license.json");
        assert_eq!(first.attribution, "\"Bell\" by Someone");
        assert!(first.attribution_required);
        let media = d.path().join("media");
        let side = read_sidecar(&media, "bell").unwrap();
        assert_eq!(side.schema, 1);
        assert_eq!(side.source.provider, "openverse");
        assert_eq!(side.source.item_id, "abc");
        assert_eq!(side.sha256, hex_sha256(&mp3()));
        assert_eq!(side.license.spdx, "CC-BY-4.0");
        assert!(side.ai.is_none() && side.modifications.is_empty());
        assert_eq!(side.creator.name, "Someone");
        let again = store_bytes(d.path(), &c, &mp3(), Some("bell"), true).unwrap();
        assert_eq!(again.file, "media/bell-2.mp3");
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(media.join("bell.license.json")).unwrap())
                .unwrap();
        for key in [
            "schema",
            "source",
            "title",
            "creator",
            "license",
            "credit_line",
            "modifications",
            "sha256",
            "ai",
        ] {
            assert!(value.get(key).is_some(), "{key}");
        }
        assert!(value["ai"].is_null());
        assert!(
            store_bytes(d.path(), &c, b"not a sound at all", None, true)
                .unwrap_err()
                .to_string()
                .contains("not a sound")
        );
    }

    #[test]
    fn an_old_freesound_sidecar_is_still_read_and_a_broken_one_is_ignored() {
        let d = tempfile::tempdir().unwrap();
        let media = d.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        std::fs::write(
            media.join("drum.freesound.json"),
            r#"{"id":8,"title":"Drum loop","author":"other","license":"Attribution","license_url":"https://creativecommons.org/licenses/by/4.0/","source_url":"https://freesound.org/s/8/"}"#,
        )
        .unwrap();
        let side = read_sidecar(&media, "drum").unwrap();
        assert_eq!(side.source.provider, "freesound");
        assert_eq!(side.source.item_id, "8");
        assert_eq!(side.license.spdx, "CC-BY-4.0");
        assert_eq!(
            side.credit_line,
            "Drum loop by other, Attribution, https://freesound.org/s/8/"
        );
        std::fs::write(media.join("odd.freesound.json"), "{not json").unwrap();
        assert!(read_sidecar(&media, "odd").is_none());
        std::fs::write(media.join("odd2.license.json"), r#"{"schema": 9}"#).unwrap();
        assert!(read_sidecar(&media, "odd2").is_none());
        assert!(read_sidecar(&media, "missing").is_none());
        let c = candidate(License::creative_commons("by", "4.0"), "x");
        let next = store_bytes(d.path(), &c, &mp3(), Some("drum"), true).unwrap();
        assert_eq!(next.file, "media/drum-2.mp3");
    }

    #[test]
    fn the_allowlist_refuses_other_hosts_plain_http_and_odd_ports() {
        let allow = Allowlist::new(&["freesound.org", "jamendo.com"], None);
        let check = |u: &str| allow.check(Provider::Openverse, "1", u);
        assert!(check("https://cdn.freesound.org/previews/1.mp3").is_ok());
        assert!(check("https://prod-1.storage.jamendo.com/?trackid=1").is_ok());
        assert!(check("https://freesound.org/x.mp3").is_ok());
        assert!(matches!(
            check("https://evil.example/x.mp3"),
            Err(Error::ForeignHost { .. })
        ));
        assert!(check("https://notfreesound.org/x.mp3").is_err());
        assert!(check("https://freesound.org.evil.example/x.mp3").is_err());
        assert!(check("http://cdn.freesound.org/x.mp3").is_err());
        assert!(check("https://cdn.freesound.org:8443/x.mp3").is_err());
        assert!(check("file:///etc/passwd").is_err());
        let test = Allowlist::new(&["freesound.org"], Some("http://127.0.0.1:4000/apiv2"));
        assert!(
            test.check(Provider::Freesound, "1", "http://127.0.0.1:4000/p.mp3")
                .is_ok()
        );
        assert!(
            test.check(Provider::Freesound, "1", "http://127.0.0.2:4000/p.mp3")
                .is_err()
        );
    }

    #[test]
    fn a_mixed_project_credits_only_what_asks_for_it_and_says_what_was_changed() {
        let d = tempfile::tempdir().unwrap();
        let media = d.path().join("media");
        std::fs::create_dir_all(&media).unwrap();
        let put = |stem: &str, side: Sidecar| {
            std::fs::write(media.join(format!("{stem}.mp3")), mp3()).unwrap();
            write_sidecar(&media, stem, &side).unwrap();
        };
        let mut zero =
            candidate(License::creative_commons("cc0", "1.0"), "ignored").sidecar(&mp3());
        zero.title = "Pop".into();
        put("pop", zero);
        let by = candidate(
            License::creative_commons("by", "4.0"),
            "\"Piano Melody\" by benpm is licensed under CC BY 4.0. To view a copy of this license, visit https://creativecommons.org/licenses/by/4.0/.",
        );
        put("piano", by.sidecar(&mp3()));
        let mut inc = candidate(
            License::creative_commons("by", "4.0"),
            &incompetech_line("The Britons"),
        );
        inc.provider = "incompetech";
        put("britons", inc.sidecar(&mp3()));
        std::fs::write(media.join("mine.wav"), b"RIFF").unwrap();
        std::fs::write(media.join("boom.wav"), b"RIFF").unwrap();
        std::fs::write(media.join("boom.synth.json"), "{}").unwrap();
        let sa = candidate(License::creative_commons("by-sa", "4.0"), "\"Pad\" by Z");
        put("pad", sa.sidecar(&mp3()));

        let used = vec![
            UsedSound {
                file: "media/britons.mp3".into(),
                modifications: vec!["trimmed".into(), "volume changed (x0.30)".into()],
            },
            UsedSound {
                file: "media/pop.mp3".into(),
                modifications: vec![],
            },
            UsedSound {
                file: "media/piano.mp3".into(),
                modifications: vec![],
            },
            UsedSound {
                file: "media/britons.mp3".into(),
                modifications: vec!["trimmed".into(), "faded out".into()],
            },
            UsedSound {
                file: "media/mine.wav".into(),
                modifications: vec![],
            },
            UsedSound {
                file: "media/boom.wav".into(),
                modifications: vec![],
            },
            UsedSound {
                file: "media/pad.mp3".into(),
                modifications: vec![],
            },
        ];
        let report = credits_report(d.path(), &used);
        let expected = format!(
            "Sound credits\n\n{}\nModified: trimmed, volume changed (x0.30), faded out.\n\n{}\n\n{}\n",
            incompetech_line("The Britons"),
            "\"Piano Melody\" by benpm is licensed under CC BY 4.0. To view a copy of this license, visit https://creativecommons.org/licenses/by/4.0/.",
            "\"Pad\" by Z"
        );
        assert_eq!(report.text, expected);
        assert_eq!(report.unlicensed, vec!["media/mine.wav"]);
        assert_eq!(report.share_alike, vec!["media/pad.mp3"]);
        assert_eq!(credits_for(d.path(), &used), expected);
        let block: Vec<&str> = report.text.lines().skip(2).take(3).collect();
        assert_eq!(block.len(), 3);
        assert_eq!(block[0], "\"The Britons\" Kevin MacLeod (incompetech.com)");
        assert_eq!(
            block[1],
            "Licensed under Creative Commons: By Attribution 4.0"
        );
        assert_eq!(block[2], "https://creativecommons.org/licenses/by/4.0/");
        let only_cc0 = credits_for(d.path(), &used[1..2]);
        assert_eq!(only_cc0, "");
    }

    #[test]
    fn a_track_says_how_it_changes_its_file() {
        let track: SoundTrack = serde_json::from_value(serde_json::json!({
            "id": "t1", "file": "media/a.mp3", "in_ms": 500, "gain": 0.3,
            "fade_in_ms": 200, "fade_out_ms": 400, "duck": {}
        }))
        .unwrap();
        assert_eq!(
            modifications_of(&track),
            vec![
                "trimmed",
                "volume changed (x0.30)",
                "faded in and out",
                "lowered under the voice (ducking)"
            ]
        );
        let plain: SoundTrack = serde_json::from_value(serde_json::json!({
            "id": "t2", "file": "media/a.mp3"
        }))
        .unwrap();
        assert!(modifications_of(&plain).is_empty());
    }

    #[test]
    fn settings_default_to_commercial_and_survive_a_round_trip() {
        let d = tempfile::tempdir().unwrap();
        assert!(load_settings(d.path()).commercial);
        let s = set_commercial(d.path(), false, "a school project").unwrap();
        assert!(!s.commercial);
        let back = load_settings(d.path());
        assert!(!back.commercial);
        assert_eq!(back.reason.as_deref(), Some("a school project"));
        std::fs::write(d.path().join(SETTINGS_FILE), "{broken").unwrap();
        assert!(load_settings(d.path()).commercial);
    }

    #[test]
    fn the_time_is_iso_8601() {
        let t = now_iso();
        assert_eq!(t.len(), 20);
        assert!(t.ends_with('Z') && t.as_bytes()[10] == b'T');
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_453), (2025, 12, 31));
    }
}
