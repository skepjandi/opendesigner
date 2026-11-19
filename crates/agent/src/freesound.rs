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
/// The most a preview may take.
const MAX_AUDIO_BYTES: u64 = 32 * 1024 * 1024;
/// How many sounds a search returns.
const PAGE_SIZE: u32 = 15;

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
    #[error("the preview of the sound {id} is on {host}, which is not Freesound")]
    ForeignHost { id: u64, host: String },
    #[error("the file name {0:?} is not a plain name")]
    BadName(String),
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
    /// The license as it reads for a person: "CC0 1.0", "Attribution", ...
    pub license: String,
    /// Where that license is written, empty when Freesound named one we do not know.
    pub license_url: String,
    /// The uploader's name.
    pub author: String,
    /// The preview to download (an mp3, not the original recording).
    pub preview_url: String,
    /// The page a person can listen to it on.
    pub source_url: String,
}

impl Sound {
    /// The one line the user must show wherever the sound is used.
    pub fn attribution(&self) -> String {
        format!(
            "{} by {}, {}, {}",
            self.name, self.author, self.license, self.source_url
        )
    }
}

/// What is written next to an imported sound: `media/<name>.freesound.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub id: u64,
    pub title: String,
    pub author: String,
    pub license: String,
    pub license_url: String,
    pub source_url: String,
}

impl Sidecar {
    /// The one line the user must show.
    pub fn attribution(&self) -> String {
        format!(
            "{} by {}, {}, {}",
            self.title, self.author, self.license, self.source_url
        )
    }
}

/// A sound in the project: the file, what it is and the line to show.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Imported {
    /// `media/<name>.mp3`
    pub file: String,
    pub sidecar: String,
    pub attribution: String,
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

/// Searches Freesound for sounds. `duration_max_ms` keeps only the short ones.
pub fn search(query: &str, duration_max_ms: Option<u32>, page: u32) -> Result<Vec<Sound>, Error> {
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
    Ok(answer.results.into_iter().map(Sound::from).collect())
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

/// Downloads the preview of a sound into the project's `media/` folder and writes the sidecar
/// next to it. The file is never overwritten: a free name is found instead.
pub fn import(project: &Path, id: u64, name: Option<&str>) -> Result<Imported, Error> {
    let sound = sound(id)?;
    if sound.preview_url.is_empty() {
        return Err(Error::NoPreview(id));
    }
    // The preview must come from the same site as the API: no other host may be asked for it.
    let preview = http::parse_url(&sound.preview_url)?;
    let api = http::parse_url(&api_url())?;
    if !api.same_site(&preview) {
        return Err(Error::ForeignHost {
            id,
            host: preview.host.clone(),
        });
    }
    let media = project.join("media");
    std::fs::create_dir_all(&media).map_err(|e| Error::Io {
        path: media.display().to_string(),
        source: e,
    })?;
    let stem = match name {
        Some(n) => clean_name(n)?,
        None => clean_name(&sound.name).unwrap_or_else(|_| format!("sound-{}", sound.id)),
    };
    let preview = http::get(&sound.preview_url, MAX_AUDIO_BYTES)?;
    if preview.status != 200 {
        return Err(Error::Api {
            status: preview.status,
            detail: "the preview could not be downloaded".to_string(),
        });
    }
    let bytes = preview.body;
    if bytes.is_empty() {
        return Err(Error::Malformed("the preview is empty".into()));
    }
    let file_name = free_stem(&media, &stem)?;
    let path = media.join(format!("{file_name}.mp3"));
    if let Err(e) = write_new(&path, &bytes) {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    let sidecar = Sidecar {
        id: sound.id,
        title: sound.name.clone(),
        author: sound.author.clone(),
        license: sound.license.clone(),
        license_url: sound.license_url.clone(),
        source_url: sound.source_url.clone(),
    };
    let sidecar_name = format!("{file_name}.freesound.json");
    let sidecar_path = media.join(&sidecar_name);
    let text =
        serde_json::to_string_pretty(&sidecar).map_err(|e| Error::Malformed(e.to_string()))?;
    if let Err(e) = write_new(&sidecar_path, text.as_bytes()) {
        let _ = std::fs::remove_file(&path);
        return Err(e);
    }
    Ok(Imported {
        file: format!("media/{file_name}.mp3"),
        sidecar: format!("media/{sidecar_name}"),
        attribution: sidecar.attribution(),
    })
}

/// The sounds already imported in this project, with the line to show for each.
pub fn list_imported(project: &Path) -> Result<Vec<Imported>, Error> {
    let media = project.join("media");
    let entries = match std::fs::read_dir(&media) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(Error::Io {
                path: media.display().to_string(),
                source: e,
            });
        }
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".freesound.json") {
            continue;
        }
        let path = entry.path();
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let Ok(sidecar) = serde_json::from_str::<Sidecar>(&text) else {
            continue;
        };
        // The sidecar sits next to the preview, which is always an mp3.
        let audio = format!("{}.mp3", name.trim_end_matches(".freesound.json"));
        out.push(Imported {
            file: format!("media/{audio}"),
            sidecar: format!("media/{name}"),
            attribution: sidecar.attribution(),
        });
    }
    out.sort_by(|a, b| a.file.cmp(&b.file));
    Ok(out)
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
        let (license, license_url) = license_of(&raw.license);
        Sound {
            id: raw.id,
            name: raw.name,
            duration_ms: (raw.duration.max(0.0) * 1000.0).round() as u64,
            license,
            license_url,
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

/// The license as a person reads it, and where it is written.
fn license_of(name: &str) -> (String, String) {
    match name.trim() {
        "Creative Commons 0" => (
            "CC0 1.0".to_string(),
            "https://creativecommons.org/publicdomain/zero/1.0/".to_string(),
        ),
        "Attribution" => (
            "Attribution".to_string(),
            "https://creativecommons.org/licenses/by/4.0/".to_string(),
        ),
        "Attribution NonCommercial" => (
            "Attribution NonCommercial".to_string(),
            "https://creativecommons.org/licenses/by-nc/4.0/".to_string(),
        ),
        other => (other.to_string(), String::new()),
    }
}

/// A plain file name: letters, digits, space, `.`, `_` and `-`.
fn clean_name(name: &str) -> Result<String, Error> {
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

/// A file name in `media/` that is not taken yet, for the sound and its sidecar both.
fn free_stem(media: &Path, stem: &str) -> Result<String, Error> {
    for n in 1.. {
        let name = if n == 1 {
            stem.to_string()
        } else {
            format!("{stem}-{n}")
        };
        if !media.join(format!("{name}.mp3")).exists()
            && !media.join(format!("{name}.freesound.json")).exists()
        {
            return Ok(name);
        }
    }
    Err(Error::BadName(stem.to_string()))
}

/// Writes a file that must not exist yet.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    let mut file = options.open(path).map_err(|e| Error::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    std::io::Write::write_all(&mut file, bytes).map_err(|e| Error::Io {
        path: path.display().to_string(),
        source: e,
    })
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

    #[test]
    fn licenses_are_named_for_people() {
        assert_eq!(
            license_of("Creative Commons 0"),
            (
                "CC0 1.0".to_string(),
                "https://creativecommons.org/publicdomain/zero/1.0/".to_string()
            )
        );
        assert_eq!(license_of("Attribution").0, "Attribution");
        assert_eq!(license_of("Something else").1, "");
    }

    #[test]
    fn a_sound_carries_its_attribution() {
        let sound = Sound {
            id: 42,
            name: "Bell".into(),
            duration_ms: 1000,
            license: "CC0 1.0".into(),
            license_url: String::new(),
            author: "Someone".into(),
            preview_url: String::new(),
            source_url: "https://freesound.org/s/42/".into(),
        };
        assert_eq!(
            sound.attribution(),
            "Bell by Someone, CC0 1.0, https://freesound.org/s/42/"
        );
    }

    #[test]
    fn names_are_plain_and_seconds_are_short() {
        assert_eq!(clean_name("Bell / 2.wav").unwrap(), "Bell - 2.wav");
        assert!(clean_name("   ").is_err());
        assert_eq!(format_seconds(1.5), "1.5");
        assert_eq!(format_seconds(2.0), "2");
    }

    #[test]
    fn a_preview_must_come_from_the_api_s_own_site() {
        let api = http::parse_url("https://freesound.org/apiv2").unwrap();
        assert!(api.same_site(&http::parse_url("https://cdn.freesound.org/x").unwrap()));
        assert!(!api.same_site(&http::parse_url("https://example.com/x").unwrap()));
        assert!(!api.same_site(&http::parse_url("https://notfreesound.org/x").unwrap()));
    }
}
