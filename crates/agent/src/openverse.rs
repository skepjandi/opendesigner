//! Openverse: a keyless search over openly licensed audio (`docs/wiki/security.md#network`).
//!
//! Openverse (https://openverse.org) indexes sounds that live on other sites. The API at
//! `https://api.openverse.org/v1/audio/` answers a plain `GET` with no key; every item carries
//! its licence, its creator, a ready-made `attribution` sentence and the address of the file. The
//! files are hosted by the original sites, so a download is only made from the hosts in
//! [`HOSTS`] (Freesound, Jamendo, Wikimedia), over https. `OPENDESIGNER_OPENVERSE_URL` points
//! the client at another server; it exists for the tests, which never touch the network.

use std::path::Path;

use serde::Deserialize;

use crate::http;
use crate::sound_license::{
    self, Allowlist, Candidate, Error, Imported, License, Provider, credit_line,
};

/// Where the API lives.
pub const API_URL: &str = "https://api.openverse.org/v1";
/// The environment variable that points the client at another server (for tests).
pub const URL_ENV: &str = "OPENDESIGNER_OPENVERSE_URL";
/// The sites Openverse's audio is hosted on.
pub const HOSTS: &[&str] = &["freesound.org", "jamendo.com", "wikimedia.org"];
const MAX_ANSWER_BYTES: u64 = 2 * 1024 * 1024;
const PAGE_SIZE: u32 = 15;

fn api_url() -> String {
    std::env::var(URL_ENV).unwrap_or_else(|_| API_URL.to_string())
}

/// The search address. A commercial project asks Openverse for sounds whose licence allows
/// commercial use and changes, and the answer is checked again here.
fn search_url(
    base: &str,
    query: &str,
    source: Option<&str>,
    page: u32,
    commercial: bool,
) -> Result<String, Error> {
    let query = query.trim();
    if query.is_empty() {
        return Err(Error::Malformed {
            provider: "openverse",
            detail: "the search needs words".into(),
        });
    }
    let mut url = format!(
        "{base}/audio/?q={}&page={}&page_size={PAGE_SIZE}",
        http::encode_query_value(query),
        page.max(1)
    );
    if let Some(source) = source.map(str::trim).filter(|s| !s.is_empty()) {
        let plain = source
            .bytes()
            .all(|b| b.is_ascii_lowercase() || matches!(b, b'_' | b','));
        if !plain {
            return Err(Error::Malformed {
                provider: "openverse",
                detail: format!(
                    "source {source:?} is not a site name such as freesound, jamendo or wikimedia_audio"
                ),
            });
        }
        url.push_str(&format!("&source={source}"));
    }
    if commercial {
        url.push_str("&license_type=commercial,modification");
    }
    Ok(url)
}

/// Searches Openverse for sounds. `source` narrows it to one site (`freesound`, `jamendo`,
/// `wikimedia_audio`). A commercial project sees only the sounds its licence clears; the second
/// number is how many were hidden.
pub fn search(
    query: &str,
    source: Option<&str>,
    duration_max_ms: Option<u32>,
    page: u32,
    commercial: bool,
) -> Result<(Vec<Candidate>, usize), Error> {
    let url = search_url(&api_url(), query, source, page, commercial)?;
    let body = sound_license::get_json(Provider::Openverse, &url, MAX_ANSWER_BYTES)?;
    let (mut found, hidden) = parse_search(&body, commercial)?;
    if let Some(max) = duration_max_ms {
        found.retain(|c| c.duration_ms == 0 || c.duration_ms <= u64::from(max));
    }
    Ok((found, hidden))
}

/// Reads a search answer. Items without a file are dropped, and so are the ones a commercial
/// project may not use (counted in the second number).
pub fn parse_search(body: &str, commercial: bool) -> Result<(Vec<Candidate>, usize), Error> {
    let answer: SearchAnswer = serde_json::from_str(body).map_err(|e| Error::Malformed {
        provider: "openverse",
        detail: e.to_string(),
    })?;
    let all: Vec<Candidate> = answer
        .results
        .into_iter()
        .filter_map(RawItem::into_candidate)
        .collect();
    let total = all.len();
    let shown: Vec<Candidate> = all
        .into_iter()
        .filter(|c| c.license.allowed(commercial).is_ok())
        .collect();
    let hidden = total - shown.len();
    Ok((shown, hidden))
}

/// One sound by its Openverse id.
pub fn item(id: &str) -> Result<Candidate, Error> {
    let plain = !id.is_empty()
        && id.len() <= 64
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
    if !plain {
        return Err(Error::NotFound {
            provider: "openverse",
            id: id.to_string(),
        });
    }
    let url = format!("{}/audio/{id}/", api_url());
    let body = match sound_license::get_json(Provider::Openverse, &url, MAX_ANSWER_BYTES) {
        Err(Error::Api { status: 404, .. }) => {
            return Err(Error::NotFound {
                provider: "openverse",
                id: id.to_string(),
            });
        }
        other => other?,
    };
    let raw: RawItem = serde_json::from_str(&body).map_err(|e| Error::Malformed {
        provider: "openverse",
        detail: e.to_string(),
    })?;
    raw.into_candidate()
        .ok_or_else(|| Error::NoDownload(id.to_string()))
}

/// Downloads a sound into the project's `media/` folder with its licence sidecar. The file
/// must be on one of [`HOSTS`]; a sound the project's `commercial` setting does not clear is
/// refused before anything is downloaded.
pub fn import(
    project: &Path,
    id: &str,
    name: Option<&str>,
    commercial: bool,
) -> Result<Imported, Error> {
    let candidate = item(id)?;
    sound_license::check_license(&candidate, commercial)?;
    let allow = Allowlist::new(HOSTS, std::env::var(URL_ENV).ok().as_deref());
    sound_license::store(
        project,
        Provider::Openverse,
        &candidate,
        &allow,
        name,
        commercial,
    )
}

#[derive(Debug, Deserialize)]
struct SearchAnswer {
    #[serde(default)]
    results: Vec<RawItem>,
}

/// An item as the API sends it; most fields may be null.
#[derive(Debug, Deserialize)]
struct RawItem {
    id: String,
    title: Option<String>,
    creator: Option<String>,
    creator_url: Option<String>,
    license: Option<String>,
    license_version: Option<String>,
    license_url: Option<String>,
    url: Option<String>,
    foreign_landing_url: Option<String>,
    /// Milliseconds.
    duration: Option<f64>,
    source: Option<String>,
    provider: Option<String>,
    attribution: Option<String>,
}

fn text(v: Option<String>) -> String {
    v.map(|s| s.trim().to_string()).unwrap_or_default()
}

impl RawItem {
    fn into_candidate(self) -> Option<Candidate> {
        let download_url = text(self.url);
        if download_url.is_empty() {
            return None;
        }
        let kind = text(self.license);
        let version = text(self.license_version);
        let license_url = text(self.license_url);
        let mut license = License::creative_commons(&kind, &version);
        if license.spdx.is_empty() {
            license = License::from_url(&license_url);
        }
        let title = match text(self.title) {
            t if t.is_empty() => "Untitled".to_string(),
            t => t,
        };
        let creator = text(self.creator);
        let landing_url = text(self.foreign_landing_url);
        let origin = match text(self.source) {
            s if s.is_empty() => text(self.provider),
            s => s,
        };
        let attribution = match text(self.attribution) {
            a if a.is_empty() => credit_line(&title, &creator, &landing_url, &license),
            a => a,
        };
        Some(Candidate {
            provider: "openverse",
            id: self.id,
            title,
            creator,
            creator_url: text(self.creator_url),
            duration_ms: self.duration.map_or(0, |d| d.max(0.0).round() as u64),
            license,
            attribution,
            download_url,
            landing_url,
            quality: if origin == "freesound" {
                "preview"
            } else {
                "original"
            },
            origin,
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const ANSWER: &str = r#"{
      "result_count": 5, "page_count": 1, "page_size": 15, "page": 1,
      "results": [
        {"id":"2c51e4eb-dbe7-468c-ba6c-d1c85a1b71ae","title":"Piano Melody",
         "foreign_landing_url":"https://freesound.org/people/benpm/sounds/186942",
         "url":"https://cdn.freesound.org/previews/186/186942_2594536-hq.mp3",
         "creator":"benpm","creator_url":"https://freesound.org/people/benpm",
         "license":"by","license_version":"4.0",
         "license_url":"https://creativecommons.org/licenses/by/4.0/",
         "provider":"freesound","source":"freesound","duration":10193,
         "attribution":"\"Piano Melody\" by benpm is licensed under CC BY 4.0. To view a copy of this license, visit https://creativecommons.org/licenses/by/4.0/."},
        {"id":"aaaa-1","title":"Calm","url":"https://prod-1.storage.jamendo.com/?trackid=1&format=mp32",
         "creator":null,"license":"by-nc-sa","license_version":"3.0",
         "foreign_landing_url":"https://www.jamendo.com/track/1",
         "source":"jamendo","provider":"jamendo","duration":264000,"attribution":null},
        {"id":"aaaa-2","title":"Rain","url":"https://cdn.freesound.org/previews/38/38487_392832-hq.mp3",
         "creator":"larval","license":"cc0","license_version":"1.0","source":"freesound",
         "duration":154090,"attribution":"\"Rain\" by larval is marked CC0 1.0."},
        {"id":"aaaa-3","title":"Nothing","url":null,"license":"by","license_version":"4.0"},
        {"id":"aaaa-4","title":"Odd","url":"https://upload.wikimedia.org/wikipedia/commons/0/0f/x.wav",
         "license":"weird","license_version":"1","license_url":null,"source":"wikimedia_audio","duration":null}
      ]
    }"#;

    #[test]
    fn items_are_read_with_their_own_attribution_and_a_normalised_licence() {
        let (found, hidden) = parse_search(ANSWER, false).unwrap();
        assert_eq!(hidden, 0);
        assert_eq!(found.len(), 4, "the item without a file is dropped");
        let piano = &found[0];
        assert_eq!(piano.id, "2c51e4eb-dbe7-468c-ba6c-d1c85a1b71ae");
        assert_eq!(piano.title, "Piano Melody");
        assert_eq!(piano.creator, "benpm");
        assert_eq!(piano.duration_ms, 10_193);
        assert_eq!(piano.license.spdx, "CC-BY-4.0");
        assert_eq!(
            piano.attribution,
            "\"Piano Melody\" by benpm is licensed under CC BY 4.0. To view a copy of this license, visit https://creativecommons.org/licenses/by/4.0/."
        );
        assert_eq!(piano.origin, "freesound");
        assert_eq!(piano.quality, "preview");
        assert_eq!(
            piano.landing_url,
            "https://freesound.org/people/benpm/sounds/186942"
        );
        let calm = &found[1];
        assert_eq!(calm.license.spdx, "CC-BY-NC-SA-3.0");
        assert!(!calm.license.commercial_ok && calm.license.share_alike);
        assert_eq!(calm.quality, "original");
        assert_eq!(calm.creator, "");
        assert_eq!(
            calm.attribution,
            "\"Calm\" (https://www.jamendo.com/track/1), licensed under CC BY-NC-SA 3.0 (https://creativecommons.org/licenses/by-nc-sa/3.0/)"
        );
        assert_eq!(found[2].license.spdx, "CC0-1.0");
        assert!(found[3].license.spdx.is_empty());
        assert_eq!(found[3].duration_ms, 0);
    }

    #[test]
    fn a_commercial_project_does_not_see_nc_nd_or_unknown_licences() {
        let (found, hidden) = parse_search(ANSWER, true).unwrap();
        let titles: Vec<&str> = found.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["Piano Melody", "Rain"]);
        assert_eq!(hidden, 2, "the by-nc-sa one and the unknown one");
    }

    #[test]
    fn the_query_asks_for_commercial_licences_only_when_the_project_is() {
        let url = search_url("http://x/v1", "soft piano", Some("jamendo"), 2, true).unwrap();
        assert_eq!(
            url,
            "http://x/v1/audio/?q=soft%20piano&page=2&page_size=15&source=jamendo&license_type=commercial,modification"
        );
        let open = search_url("http://x/v1", "rain", None, 1, false).unwrap();
        assert!(!open.contains("license_type") && !open.contains("source"));
        assert!(search_url("http://x/v1", "  ", None, 1, true).is_err());
        assert!(search_url("http://x/v1", "rain", Some("a&b=c"), 1, true).is_err());
    }

    #[test]
    fn only_the_hosting_sites_are_downloaded_from() {
        let allow = Allowlist::new(HOSTS, None);
        let ok = |u: &str| allow.check(Provider::Openverse, "x", u).is_ok();
        assert!(ok("https://cdn.freesound.org/previews/1.mp3"));
        assert!(ok(
            "https://prod-1.storage.jamendo.com/?trackid=1&format=mp32"
        ));
        assert!(ok(
            "https://upload.wikimedia.org/wikipedia/commons/0/0f/x.wav"
        ));
        assert!(!ok("https://evil.example/x.mp3"));
        assert!(!ok("https://jamendo.com.evil.example/x.mp3"));
        assert!(!ok("http://cdn.freesound.org/previews/1.mp3"));
        assert!(!ok("https://169.254.169.254/latest"));
    }

    #[test]
    fn a_hostile_or_broken_answer_is_an_error_not_a_panic() {
        assert!(parse_search("{not json", true).is_err());
        assert!(parse_search(r#"{"results":[{"title":"no id"}]}"#, true).is_err());
        let (none, _) = parse_search("{}", true).unwrap();
        assert!(none.is_empty());
        assert!(matches!(item("../etc"), Err(Error::NotFound { .. })));
        assert!(matches!(item(""), Err(Error::NotFound { .. })));
    }
}
