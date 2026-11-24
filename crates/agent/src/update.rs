//! The update channel: asks GitHub whether a newer release of OpenDesigner exists.
//!
//! It is off until the user turns it on (AGENTS.md invariant 1), except for an explicit "Check
//! now". The check is one `GET` of the latest release's description from `api.github.com`; nothing
//! about the user, the machine or any project is sent. Nothing is downloaded or installed by the
//! app: the answer is a version and the release page, which the user opens in a browser, so no
//! code the app fetched ever runs without the user installing it themselves.
//!
//! What GitHub answers is untrusted input: the tag must be a plain version, the page must be on
//! this repository's releases, and the notes are capped and shown as plain text.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::http;

const RELEASE_API: &str = "https://api.github.com/repos/skepjandi/opendesigner/releases/latest";
const RELEASE_PAGE_PREFIX: &str = "https://github.com/skepjandi/opendesigner/releases/";
const URL_ENV: &str = "OPENDESIGNER_UPDATE_URL";
const SETTINGS_FILE: &str = "updates.json";
/// The most bytes of the release description that are read.
const MAX_ANSWER: u64 = 256 * 1024;
/// The most characters of release notes that are kept.
const MAX_NOTES: usize = 4000;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Http(#[from] http::HttpError),
    #[error("the release description is not what was expected: {0}")]
    Answer(String),
    #[error("cannot read or write the update settings: {0}")]
    Settings(String),
}

/// A published release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Release {
    /// `1.2.3`, without a leading `v`.
    pub version: String,
    /// The release page on github.com, where the downloads are.
    pub url: String,
    pub notes: String,
}

/// `major.minor.patch`; a leading `v` is allowed, anything else (a pre-release suffix, a missing
/// part, a non-number) is not a version.
pub fn parse_version(text: &str) -> Option<[u64; 3]> {
    let mut parts = text.strip_prefix('v').unwrap_or(text).split('.');
    let mut out = [0u64; 3];
    for slot in &mut out {
        let part = parts.next()?;
        if part.is_empty() || part.len() > 9 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *slot = part.parse().ok()?;
    }
    parts.next().is_none().then_some(out)
}

/// Whether `candidate` is a higher version than `current`. A version that does not parse is
/// never newer.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(c), Some(k)) => c > k,
        _ => false,
    }
}

/// Reads GitHub's answer for the latest release.
pub fn parse_release(body: &[u8]) -> Result<Release, Error> {
    let json: Value = serde_json::from_slice(body).map_err(|e| Error::Answer(e.to_string()))?;
    let text = |key: &str| json.get(key).and_then(Value::as_str);
    let tag = text("tag_name").ok_or_else(|| Error::Answer("no tag_name".into()))?;
    parse_version(tag)
        .ok_or_else(|| Error::Answer(format!("`{}` is not a version", clip(tag, 40))))?;
    let url = text("html_url").ok_or_else(|| Error::Answer("no html_url".into()))?;
    if !url.starts_with(RELEASE_PAGE_PREFIX) || url.chars().any(|c| c.is_control() || c == ' ') {
        return Err(Error::Answer(
            "the release page is not on this project's releases".into(),
        ));
    }
    Ok(Release {
        version: tag.strip_prefix('v').unwrap_or(tag).to_string(),
        url: url.to_string(),
        notes: clip(text("body").unwrap_or(""), MAX_NOTES),
    })
}

fn clip(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Asks GitHub for the latest release. `OPENDESIGNER_UPDATE_URL` replaces the address, for tests.
pub fn latest() -> Result<Release, Error> {
    let url = std::env::var(URL_ENV).unwrap_or_else(|_| RELEASE_API.to_string());
    let answer = http::get(&url, MAX_ANSWER)?;
    parse_release(&answer.body)
}

/// The newer release, if there is one.
pub fn check(current: &str) -> Result<Option<Release>, Error> {
    let release = latest()?;
    Ok(is_newer(&release.version, current).then_some(release))
}

/// What the user chose about checking for updates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    /// Check once when the editor starts.
    #[serde(default)]
    pub check_on_start: bool,
}

fn settings_path() -> Result<PathBuf, Error> {
    crate::token::config_dir()
        .map(|dir| dir.join(SETTINGS_FILE))
        .map_err(|e| Error::Settings(e.to_string()))
}

/// The saved settings; off when nothing is saved or the file is unreadable.
pub fn settings() -> Settings {
    settings_path()
        .map(|path| settings_in(&path))
        .unwrap_or_default()
}

pub fn settings_in(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) -> Result<(), Error> {
    save_settings_in(&settings_path()?, settings)
}

pub fn save_settings_in(path: &Path, settings: &Settings) -> Result<(), Error> {
    let io = |e: std::io::Error| Error::Settings(e.to_string());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(io)?;
    }
    let text = serde_json::to_vec_pretty(settings).map_err(|e| Error::Settings(e.to_string()))?;
    std::fs::write(path, text).map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{"tag_name":"v0.1.0","html_url":"https://github.com/skepjandi/opendesigner/releases/tag/v0.1.0","body":"Faster export","draft":false}"#;

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(is_newer("0.0.10", "0.0.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("0.0.1", "0.0.1"));
        assert!(!is_newer("0.0.1", "0.1.0"));
    }

    #[test]
    fn only_plain_versions_count() {
        for bad in [
            "",
            "1",
            "1.2",
            "1.2.3.4",
            "1.2.3-rc1",
            "latest",
            "1..3",
            "1.2.x",
            "9999999999.0.0",
        ] {
            assert!(parse_version(bad).is_none(), "{bad}");
            assert!(!is_newer(bad, "0.0.1"), "{bad}");
        }
    }

    #[test]
    fn a_release_answer_is_read() {
        let release = parse_release(ANSWER.as_bytes()).unwrap();
        assert_eq!(release.version, "0.1.0");
        assert_eq!(release.notes, "Faster export");
        assert!(release.url.ends_with("/tag/v0.1.0"));
    }

    #[test]
    fn a_hostile_answer_is_refused() {
        let page = |url: &str| format!(r#"{{"tag_name":"v0.1.0","html_url":"{url}","body":""}}"#);
        for url in [
            "https://evil.example/skepjandi/opendesigner/releases/tag/v0.1.0",
            "http://github.com/skepjandi/opendesigner/releases/tag/v0.1.0",
            "https://github.com/skepjandi/opendesigner-fork/releases/tag/v0.1.0",
            "https://github.com/skepjandi/opendesigner/releases/ x",
        ] {
            assert!(parse_release(page(url).as_bytes()).is_err(), "{url}");
        }
        let tag = r#"{"tag_name":"v1.0.0-<script>","html_url":"https://github.com/skepjandi/opendesigner/releases/x"}"#;
        assert!(parse_release(tag.as_bytes()).is_err());
        assert!(parse_release(b"not json").is_err());
    }

    #[test]
    fn long_notes_are_cut() {
        let body = "x".repeat(10_000);
        let answer = format!(
            r#"{{"tag_name":"v0.1.0","html_url":"{RELEASE_PAGE_PREFIX}tag/v0.1.0","body":"{body}"}}"#
        );
        assert_eq!(
            parse_release(answer.as_bytes()).unwrap().notes.len(),
            MAX_NOTES
        );
    }

    #[test]
    fn checking_is_off_until_the_user_turns_it_on() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config").join(SETTINGS_FILE);
        assert!(!settings_in(&path).check_on_start);
        save_settings_in(
            &path,
            &Settings {
                check_on_start: true,
            },
        )
        .unwrap();
        assert!(settings_in(&path).check_on_start);
        std::fs::write(&path, "garbage").unwrap();
        assert!(!settings_in(&path).check_on_start);
    }
}
