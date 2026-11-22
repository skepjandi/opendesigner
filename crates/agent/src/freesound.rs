//! Freesound: finding music and sound effects, and bringing one into the project
//! (`docs/wiki/security.md#network`).
//!
//! The user adds a free API token in the Connect tab; it is stored in the config folder and
//! sent only to freesound.org, in the query string where the API documents it. Without a token
//! the tools refuse and say what to do. Nothing here is made up: a sound is only imported with
//! the metadata Freesound returned, and the attribution the user must show is built from it.
//!
//! The API is at `https://freesound.org/apiv2/` (the search resource is `/search/text/`, one
//! sound is `/sounds/<id>/`). `OPENDESIGNER_FREESOUND_URL` points the client at another server;
//! it exists for the tests, which run against a local one and never touch the network.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::http;
use crate::sound_license::{self, Candidate, Imported, License};

/// Where the API lives.
pub const API_URL: &str = "https://freesound.org/apiv2";
/// The environment variable that points the client at another server (for tests).
pub const URL_ENV: &str = "OPENDESIGNER_FREESOUND_URL";
/// Where a user asks for a token.
pub const APPLY_URL: &str = "https://freesound.org/apiv2/apply/";
/// The file in the config folder that holds the token.
pub const TOKEN_FILE: &str = "freesound/token";
/// The most a search answer may take.
const MAX_SEARCH_BYTES: u64 = 2 * 1024 * 1024;
/// How many sounds a search returns.
const PAGE_SIZE: u32 = 15;
/// The hosts a preview may come from: Freesound's own.
const HOSTS: &[&str] = &["freesound.org"];

#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "no Freesound token: ask the user to add one in the Connect tab; they can get one at {APPLY_URL}"
    )]
    NoToken,
    #[error("the Freesound token in {path} cannot be read: {source}")]
    Token {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Http(#[from] http::HttpError),
    #[error("Freesound answered {status}: {detail}")]
    Api { status: u16, detail: String },
    #[error("Freesound answered something this client cannot read: {0}")]
    Malformed(String),
    #[error("there is no sound with the id {0} on Freesound")]
    NoSuchSound(u64),
    #[error("the sound {0} has no preview to download")]
    NoPreview(u64),
    #[error("{0}")]
    Sound(#[from] sound_license::Error),
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// One sound as Freesound describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sound {
    pub id: u64,
    pub name: String,
    /// How long it lasts, in milliseconds.
    pub duration_ms: u64,
    pub license: License,
    /// The uploader's name.
    pub author: String,
    /// The preview to download (an mp3, not the original recording).
    pub preview_url: String,
    /// The page a person can listen to it on.
    pub source_url: String,
}

impl Sound {
    /// The line the user must show wherever the sound is used.
    pub fn attribution(&self) -> String {
        sound_license::credit_line(&self.name, &self.author, &self.source_url, &self.license)
    }

    /// The sound as the tools list and import it.
    pub fn candidate(&self) -> Candidate {
        Candidate {
            provider: "freesound",
            id: self.id.to_string(),
            title: self.name.clone(),
            creator: self.author.clone(),
            creator_url: format!("https://freesound.org/people/{}/", self.author),
            duration_ms: self.duration_ms,
            license: self.license.clone(),
            attribution: self.attribution(),
            download_url: self.preview_url.clone(),
            landing_url: self.source_url.clone(),
            origin: "freesound".to_string(),
            quality: "preview",
        }
    }
}

/// The API address, which the tests may point elsewhere.
fn api_url() -> String {
    std::env::var(URL_ENV).unwrap_or_else(|_| API_URL.to_string())
}

/// The file that holds the user's token.
pub fn token_path() -> Result<PathBuf, Error> {
    let dir = crate::token::config_dir().map_err(|e| Error::Token {
        path: "the config folder".into(),
        source: std::io::Error::other(e.to_string()),
    })?;
    Ok(dir.join(TOKEN_FILE))
}

/// The user's token, or the error that says what to do about it.
pub fn token() -> Result<String, Error> {
    token_in(&token_path()?)
}

/// The token in a file, or the error that says what to do about it.
pub fn token_in(path: &Path) -> Result<String, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::NoToken
        } else {
            Error::Token {
                path: path.display().to_string(),
                source: e,
            }
        }
    })?;
    let token = text.trim();
    if token.is_empty() {
        return Err(Error::NoToken);
    }
    Ok(token.to_string())
}

/// Stores the user's token. Only the app calls this, with what the user typed.
pub fn save_token(value: &str) -> Result<(), Error> {
    save_token_in(&token_path()?, value)
}

/// Stores the user's token in a config folder.
pub fn save_token_in(path: &Path, value: &str) -> Result<(), Error> {
    let token = value.trim();
    if token.is_empty() {
        return Err(Error::Token {
            path: path.display().to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidInput, "the token is empty"),
        });
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Error::Io {
            path: dir.display().to_string(),
            source: e,
        })?;
    }
    write_private(path, token.as_bytes())
}

/// Forgets the user's token.
pub fn clear_token() -> Result<(), Error> {
    clear_token_in(&token_path()?)
}

/// Forgets the token in a config folder.
pub fn clear_token_in(path: &Path) -> Result<(), Error> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::Io {
            path: path.display().to_string(),
            source: e,
        }),
    }
}

/// Whether a token is there (without reading it into an answer).
pub fn has_token() -> bool {
    match token_path() {
        Ok(path) => has_token_in(&path),
        Err(_) => false,
    }
}

/// Whether a token is there in a config folder.
pub fn has_token_in(path: &Path) -> bool {
    token_in(path).is_ok()
}

/// Writes a file only the user can read.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| Error::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|e| Error::Io {
                path: path.display().to_string(),
                source: e,
            })?;
    }
    std::io::Write::write_all(&mut file, bytes).map_err(|e| Error::Io {
        path: path.display().to_string(),
        source: e,
    })
}

/// Searches Freesound for sounds. `duration_max_ms` keeps only the short ones. A commercial
/// project sees only the sounds its licence clears; the second number is how many were hidden.
pub fn search(
    query: &str,
    duration_max_ms: Option<u32>,
    page: u32,
    commercial: bool,
) -> Result<(Vec<Sound>, usize), Error> {
    let token = token()?;
    let query = query.trim();
    if query.is_empty() {
        return Err(Error::Malformed("the search needs words".into()));
    }
    let mut url = format!(
        "{}/search/text/?query={}&fields={}&sort=rating_desc&page={}&page_size={PAGE_SIZE}",
        api_url(),
        http::encode_query_value(query),
        http::encode_query_value("id,name,duration,license,username,previews"),
        page.max(1)
    );
    if let Some(max) = duration_max_ms {
        // The API's duration filter counts in seconds.
        let filter = format!("duration:[0 TO {}]", format_seconds(max as f64 / 1000.0));
        url.push_str(&format!("&filter={}", http::encode_query_value(&filter)));
    }
    url.push_str(&format!("&token={}", http::encode_query_value(&token)));
    let body = get_json(&url)?;
    let answer: SearchAnswer =
        serde_json::from_str(&body).map_err(|e| Error::Malformed(e.to_string()))?;
    let all: Vec<Sound> = answer.results.into_iter().map(Sound::from).collect();
    let total = all.len();
    let shown: Vec<Sound> = all
        .into_iter()
        .filter(|s| s.license.allowed(commercial).is_ok())
        .collect();
    let hidden = total - shown.len();
    Ok((shown, hidden))
}

/// One sound, with everything the search may not have returned.
pub fn sound(id: u64) -> Result<Sound, Error> {
    let token = token()?;
    let url = format!(
        "{}/sounds/{id}/?fields=id,name,duration,license,username,previews&token={}",
        api_url(),
        http::encode_query_value(&token)
    );
    let body = get_json(&url)?;
    let raw: RawSound = serde_json::from_str(&body).map_err(|e| Error::Malformed(e.to_string()))?;
    Ok(Sound::from(raw))
}

/// Downloads the preview of a sound into the project's `media/` folder and writes its licence
/// sidecar next to it. The file is never overwritten: a free name is found instead. A sound the
/// project's `commercial` setting does not clear is refused before anything is downloaded.
pub fn import(
    project: &Path,
    id: u64,
    name: Option<&str>,
    commercial: bool,
) -> Result<Imported, Error> {
    let sound = sound(id)?;
    if sound.preview_url.is_empty() {
        return Err(Error::NoPreview(id));
    }
    let allow = sound_license::Allowlist::new(HOSTS, std::env::var(URL_ENV).ok().as_deref());
    Ok(sound_license::store(
        project,
        sound_license::Provider::Freesound,
        &sound.candidate(),
        &allow,
        name,
        commercial,
    )?)
}

/// The API's answer to a search.
#[derive(Debug, Deserialize)]
struct SearchAnswer {
    #[serde(default)]
    results: Vec<RawSound>,
}

/// A sound as the API sends it.
#[derive(Debug, Deserialize)]
struct RawSound {
    id: u64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    license: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    previews: Previews,
}

#[derive(Debug, Default, Deserialize)]
struct Previews {
    #[serde(rename = "preview-hq-mp3", default)]
    hq: String,
    #[serde(rename = "preview-lq-mp3", default)]
    lq: String,
}

impl From<RawSound> for Sound {
    fn from(raw: RawSound) -> Sound {
        Sound {
            id: raw.id,
            name: raw.name,
            duration_ms: (raw.duration.max(0.0) * 1000.0).round() as u64,
            license: License::from_name(&raw.license, ""),
            author: raw.username,
            preview_url: if raw.previews.hq.is_empty() {
                raw.previews.lq
            } else {
                raw.previews.hq
            },
            source_url: format!("https://freesound.org/s/{}/", raw.id),
        }
    }
}

/// Asks the API and hands back the answer's body.
fn get_json(url: &str) -> Result<String, Error> {
    let response = http::get(url, MAX_SEARCH_BYTES)?;
    if response.status != 200 {
        return Err(Error::Api {
            status: response.status,
            detail: detail_of(&response.body),
        });
    }
    String::from_utf8(response.body).map_err(|e| Error::Malformed(e.to_string()))
}

/// The `detail` an API error carries, or a short description of the answer.
fn detail_of(body: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("detail").and_then(|d| d.as_str()).map(str::to_string))
        .unwrap_or_else(|| "no reason given".to_string())
}

/// A number of seconds for the API's filter, without a trailing `.0` noise.
fn format_seconds(seconds: f64) -> String {
    let rounded = (seconds * 1000.0).round() / 1000.0;
    let mut text = format!("{rounded:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn raw(license: &str) -> RawSound {
        RawSound {
            id: 42,
            name: "Bell".into(),
            duration: 1.5,
            license: license.into(),
            username: "Someone".into(),
            previews: Previews {
                hq: "https://cdn.freesound.org/previews/42_hq.mp3".into(),
                lq: String::new(),
            },
        }
    }

    #[test]
    fn licenses_are_normalised_from_freesound_names_and_deed_addresses() {
        assert_eq!(
            Sound::from(raw("Creative Commons 0")).license.spdx,
            "CC0-1.0"
        );
        assert_eq!(Sound::from(raw("Attribution")).license.spdx, "CC-BY-4.0");
        let nc = Sound::from(raw("Attribution NonCommercial")).license;
        assert_eq!(nc.spdx, "CC-BY-NC-4.0");
        assert!(!nc.commercial_ok);
        let sampling = Sound::from(raw("Sampling+")).license;
        assert!(!sampling.commercial_ok && sampling.derivatives_ok);
        let by3 = Sound::from(raw("http://creativecommons.org/licenses/by/3.0/")).license;
        assert_eq!(by3.spdx, "CC-BY-3.0");
        assert!(Sound::from(raw("Something else")).license.spdx.is_empty());
    }

    #[test]
    fn a_sound_carries_its_attribution() {
        let sound = Sound::from(raw("Creative Commons 0"));
        assert_eq!(sound.duration_ms, 1500);
        assert_eq!(
            sound.attribution(),
            "\"Bell\" by Someone (https://freesound.org/s/42/), licensed under CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/)"
        );
        let candidate = sound.candidate();
        assert_eq!(candidate.id, "42");
        assert_eq!(candidate.provider, "freesound");
        assert_eq!(candidate.quality, "preview");
    }

    #[test]
    fn seconds_are_short() {
        assert_eq!(format_seconds(1.5), "1.5");
        assert_eq!(format_seconds(2.0), "2");
    }

    #[test]
    fn a_preview_must_come_from_freesound() {
        let allow = sound_license::Allowlist::new(HOSTS, None);
        let check = |u: &str| allow.check(sound_license::Provider::Freesound, "1", u);
        assert!(check("https://cdn.freesound.org/x").is_ok());
        assert!(check("https://example.com/x").is_err());
        assert!(check("https://notfreesound.org/x").is_err());
    }
}
