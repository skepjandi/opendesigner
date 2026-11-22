//! Two keyless music sources: Incompetech (Kevin MacLeod, every piece CC BY 4.0) and ccMixter
//! (tracks under the licence each upload names) (`docs/wiki/security.md#network`).
//!
//! Incompetech publishes its whole catalogue as one file, `pieces.json`; a search reads it and
//! filters here, and a piece is the mp3 of the same name under `mp3-royaltyfree/`. ccMixter has a
//! query API that answers JSON; every upload carries its `license_url` and its files with a
//! download address. Both are fetched over https from their own site only
//! (`https://ccmixter.org` answers over https; its older `http://` address is never used).
//! `OPENDESIGNER_INCOMPETECH_URL` (the address of `pieces.json`) and `OPENDESIGNER_CCMIXTER_URL`
//! (the query address) point the clients at a test server.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::http;
use crate::sound_license::{
    self, Allowlist, Candidate, Error, Imported, License, Provider, credit_line,
};

pub const INCOMPETECH_URL: &str = "https://incompetech.com/music/royalty-free/pieces.json";
pub const INCOMPETECH_ENV: &str = "OPENDESIGNER_INCOMPETECH_URL";
pub const CCMIXTER_URL: &str = "https://ccmixter.org/api/query";
pub const CCMIXTER_ENV: &str = "OPENDESIGNER_CCMIXTER_URL";
const INCOMPETECH_HOSTS: &[&str] = &["incompetech.com"];
const CCMIXTER_HOSTS: &[&str] = &["ccmixter.org"];
/// `pieces.json` is the whole catalogue, a bit under 1 MB.
const MAX_CATALOGUE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ANSWER_BYTES: u64 = 2 * 1024 * 1024;
const PAGE_SIZE: usize = 15;
const BY_4: &str = "https://creativecommons.org/licenses/by/4.0/";

fn incompetech_url() -> String {
    std::env::var(INCOMPETECH_ENV).unwrap_or_else(|_| INCOMPETECH_URL.to_string())
}

fn ccmixter_url() -> String {
    std::env::var(CCMIXTER_ENV).unwrap_or_else(|_| CCMIXTER_URL.to_string())
}

/// `00:05:07`, `4:31` or `45` as milliseconds; 0 when it cannot be read.
fn clock_ms(text: &str) -> u64 {
    let mut seconds = 0u64;
    for part in text.trim().split(':') {
        match part.trim().parse::<u64>() {
            Ok(n) => seconds = seconds * 60 + n,
            Err(_) => return 0,
        }
    }
    seconds * 1000
}

fn text(v: Option<String>) -> String {
    v.map(|s| s.trim().to_string()).unwrap_or_default()
}

fn malformed(provider: Provider, detail: impl ToString) -> Error {
    Error::Malformed {
        provider: provider.name(),
        detail: detail.to_string(),
    }
}

#[derive(Debug, Deserialize)]
struct RawPiece {
    title: Option<String>,
    filename: Option<String>,
    length: Option<String>,
    instruments: Option<String>,
    feel: Option<String>,
    description: Option<String>,
    isrc: Option<String>,
}

/// The three lines Kevin MacLeod asks every video to show.
fn incompetech_credit(title: &str) -> String {
    format!(
        "\"{title}\" Kevin MacLeod (incompetech.com)\nLicensed under Creative Commons: By Attribution 4.0\n{BY_4}"
    )
}

impl RawPiece {
    fn into_candidate(self, audio_base: &str) -> Option<Candidate> {
        let filename = text(self.filename);
        let plain = !filename.is_empty()
            && !filename.contains(['/', '\\'])
            && filename.to_ascii_lowercase().ends_with(".mp3");
        if !plain {
            return None;
        }
        let title = match text(self.title) {
            t if t.is_empty() => filename.trim_end_matches(".mp3").to_string(),
            t => t,
        };
        let landing_url = match text(self.isrc) {
            i if i.is_empty() => "https://incompetech.com/music/royalty-free/".to_string(),
            i => format!(
                "https://incompetech.com/music/royalty-free/index.html?isrc={}",
                http::encode_query_value(&i)
            ),
        };
        Some(Candidate {
            provider: "incompetech",
            id: filename.clone(),
            attribution: incompetech_credit(&title),
            title,
            creator: "Kevin MacLeod".to_string(),
            creator_url: "https://incompetech.com".to_string(),
            duration_ms: clock_ms(&text(self.length)),
            license: License::creative_commons("by", "4.0"),
            download_url: format!("{audio_base}{}", http::encode_query_value(&filename)),
            landing_url,
            origin: "incompetech".to_string(),
            quality: "original",
        })
    }

    /// Whether every word of the query is in the piece's title, mood, instruments or notes.
    fn matches(&self, words: &[String]) -> bool {
        let hay = [
            &self.title,
            &self.filename,
            &self.instruments,
            &self.feel,
            &self.description,
        ]
        .iter()
        .filter_map(|f| f.as_deref())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
        words.iter().all(|w| hay.contains(w.as_str()))
    }
}

/// `…/pieces.json` -> `…/mp3-royaltyfree/`
fn audio_base(pieces_url: &str) -> String {
    let dir = pieces_url
        .rsplit_once('/')
        .map_or(pieces_url, |(dir, _)| dir);
    format!("{dir}/mp3-royaltyfree/")
}

/// Filters the catalogue: the pieces matching every word of `query`, those with the query in
/// their title first, one page of them.
pub fn parse_incompetech(
    body: &str,
    pieces_url: &str,
    query: &str,
    duration_max_ms: Option<u32>,
    page: u32,
) -> Result<Vec<Candidate>, Error> {
    let pieces: Vec<RawPiece> =
        serde_json::from_str(body).map_err(|e| malformed(Provider::Incompetech, e))?;
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if words.is_empty() {
        return Err(malformed(Provider::Incompetech, "the search needs words"));
    }
    let base = audio_base(pieces_url);
    let mut hits: Vec<(bool, Candidate)> = pieces
        .into_iter()
        .filter(|p| p.matches(&words))
        .filter_map(|p| {
            let in_title = p
                .title
                .as_deref()
                .is_some_and(|t| words.iter().all(|w| t.to_lowercase().contains(w.as_str())));
            p.into_candidate(&base).map(|c| (in_title, c))
        })
        .filter(|(_, c)| {
            duration_max_ms.is_none_or(|max| c.duration_ms == 0 || c.duration_ms <= u64::from(max))
        })
        .collect();
    hits.sort_by_key(|(in_title, _)| !*in_title);
    let skip = (page.max(1) as usize - 1) * PAGE_SIZE;
    Ok(hits
        .into_iter()
        .skip(skip)
        .take(PAGE_SIZE)
        .map(|(_, c)| c)
        .collect())
}

/// Searches Incompetech. Every piece is CC BY 4.0, which a commercial project may use.
pub fn incompetech_search(
    query: &str,
    duration_max_ms: Option<u32>,
    page: u32,
) -> Result<Vec<Candidate>, Error> {
    let url = incompetech_url();
    let body = sound_license::get_json(Provider::Incompetech, &url, MAX_CATALOGUE_BYTES)?;
    parse_incompetech(&body, &url, query, duration_max_ms, page)
}

/// One piece by its file name, as `incompetech_search` returned it as `id`.
pub fn incompetech_item(id: &str) -> Result<Candidate, Error> {
    let url = incompetech_url();
    let body = sound_license::get_json(Provider::Incompetech, &url, MAX_CATALOGUE_BYTES)?;
    let pieces: Vec<RawPiece> =
        serde_json::from_str(&body).map_err(|e| malformed(Provider::Incompetech, e))?;
    let base = audio_base(&url);
    pieces
        .into_iter()
        .find(|p| p.filename.as_deref().map(str::trim) == Some(id))
        .and_then(|p| p.into_candidate(&base))
        .ok_or_else(|| Error::NotFound {
            provider: "incompetech",
            id: id.to_string(),
        })
}

pub fn incompetech_import(
    project: &Path,
    id: &str,
    name: Option<&str>,
    commercial: bool,
) -> Result<Imported, Error> {
    let candidate = incompetech_item(id)?;
    let allow = Allowlist::new(
        INCOMPETECH_HOSTS,
        std::env::var(INCOMPETECH_ENV).ok().as_deref(),
    );
    sound_license::store(
        project,
        Provider::Incompetech,
        &candidate,
        &allow,
        name,
        commercial,
    )
}

#[derive(Debug, Deserialize)]
struct RawUpload {
    upload_id: u64,
    upload_name: Option<String>,
    user_name: Option<String>,
    user_real_name: Option<String>,
    file_page_url: Option<String>,
    artist_page_url: Option<String>,
    license_url: Option<String>,
    license_name: Option<String>,
    #[serde(default)]
    files: Vec<RawFile>,
}

#[derive(Debug, Deserialize)]
struct RawFile {
    download_url: Option<String>,
    file_nicname: Option<String>,
    file_format_info: Option<Value>,
}

impl RawUpload {
    fn into_candidate(self) -> Option<Candidate> {
        let file = self.files.iter().find(|f| {
            let url = f.download_url.as_deref().unwrap_or("").to_ascii_lowercase();
            f.file_nicname.as_deref() == Some("mp3") || url.ends_with(".mp3")
        })?;
        let download_url = text(file.download_url.clone());
        if download_url.is_empty() {
            return None;
        }
        let duration_ms = file
            .file_format_info
            .as_ref()
            .and_then(|i| i.get("ps"))
            .and_then(Value::as_str)
            .map_or(0, clock_ms);
        let license = License::from_name(&text(self.license_name), &text(self.license_url));
        let title = match text(self.upload_name) {
            t if t.is_empty() => format!("ccMixter {}", self.upload_id),
            t => t,
        };
        let creator = match text(self.user_real_name) {
            c if c.is_empty() => text(self.user_name),
            c => c,
        };
        let landing_url = text(self.file_page_url);
        Some(Candidate {
            provider: "ccmixter",
            id: self.upload_id.to_string(),
            attribution: credit_line(&title, &creator, &landing_url, &license),
            title,
            creator,
            creator_url: text(self.artist_page_url),
            duration_ms,
            license,
            download_url,
            landing_url,
            origin: "ccmixter".to_string(),
            quality: "original",
        })
    }
}

fn search_url(base: &str, query: &str, page: u32) -> Result<String, Error> {
    let query = query.trim();
    if query.is_empty() {
        return Err(malformed(Provider::Ccmixter, "the search needs words"));
    }
    let offset = (page.max(1) as usize - 1) * PAGE_SIZE;
    Ok(format!(
        "{base}?f=json&search={}&lic=by&limit={PAGE_SIZE}&offset={offset}",
        http::encode_query_value(query)
    ))
}

/// Reads a ccMixter answer: uploads with an mp3, a commercial project keeping only the licences
/// it may use (the second number counts those hidden).
pub fn parse_ccmixter(body: &str, commercial: bool) -> Result<(Vec<Candidate>, usize), Error> {
    let uploads: Vec<RawUpload> =
        serde_json::from_str(body).map_err(|e| malformed(Provider::Ccmixter, e))?;
    let all: Vec<Candidate> = uploads
        .into_iter()
        .filter_map(RawUpload::into_candidate)
        .collect();
    let total = all.len();
    let shown: Vec<Candidate> = all
        .into_iter()
        .filter(|c| c.license.allowed(commercial).is_ok())
        .collect();
    let hidden = total - shown.len();
    Ok((shown, hidden))
}

/// Searches ccMixter for tracks under an Attribution licence (`lic=by`).
pub fn ccmixter_search(
    query: &str,
    duration_max_ms: Option<u32>,
    page: u32,
    commercial: bool,
) -> Result<(Vec<Candidate>, usize), Error> {
    let url = search_url(&ccmixter_url(), query, page)?;
    let body = sound_license::get_json(Provider::Ccmixter, &url, MAX_ANSWER_BYTES)?;
    let (mut found, hidden) = parse_ccmixter(&body, commercial)?;
    if let Some(max) = duration_max_ms {
        found.retain(|c| c.duration_ms == 0 || c.duration_ms <= u64::from(max));
    }
    Ok((found, hidden))
}

/// One upload by its id.
pub fn ccmixter_item(id: &str) -> Result<Candidate, Error> {
    let plain = !id.is_empty() && id.len() <= 12 && id.bytes().all(|b| b.is_ascii_digit());
    if !plain {
        return Err(Error::NotFound {
            provider: "ccmixter",
            id: id.to_string(),
        });
    }
    let url = format!("{}?f=json&ids={id}", ccmixter_url());
    let body = sound_license::get_json(Provider::Ccmixter, &url, MAX_ANSWER_BYTES)?;
    let uploads: Vec<RawUpload> =
        serde_json::from_str(&body).map_err(|e| malformed(Provider::Ccmixter, e))?;
    uploads
        .into_iter()
        .find(|u| u.upload_id.to_string() == id)
        .and_then(RawUpload::into_candidate)
        .ok_or_else(|| Error::NotFound {
            provider: "ccmixter",
            id: id.to_string(),
        })
}

pub fn ccmixter_import(
    project: &Path,
    id: &str,
    name: Option<&str>,
    commercial: bool,
) -> Result<Imported, Error> {
    let candidate = ccmixter_item(id)?;
    sound_license::check_license(&candidate, commercial)?;
    let allow = Allowlist::new(CCMIXTER_HOSTS, std::env::var(CCMIXTER_ENV).ok().as_deref());
    sound_license::store(
        project,
        Provider::Ccmixter,
        &candidate,
        &allow,
        name,
        commercial,
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    const PIECES: &str = r#"[
      {"title":"The Britons","filename":"The Britons.mp3","length":"00:05:07","instruments":"Lute, Recorder","genre":"22","bpm":"180","description":"Tavern music like from the olden days.","feel":"Ren Faire, Medieval","isrc":"USUAN2600004","collection":null},
      {"title":"Calm Piano","filename":"Calm Piano.mp3","length":"00:01:30","instruments":"Piano","feel":"Peaceful","description":null,"isrc":null},
      {"title":"Quiet","filename":"Quiet.mp3","length":"00:00:45","instruments":"Piano, Strings","feel":"Calm","description":"calm and piano"},
      {"title":"Bad name","filename":"../evil.mp3","length":"00:00:10"},
      {"title":"Not mp3","filename":"x.wav","length":"00:00:10"}
    ]"#;

    #[test]
    fn the_incompetech_credit_is_three_exact_lines() {
        assert_eq!(
            incompetech_credit("The Britons"),
            "\"The Britons\" Kevin MacLeod (incompetech.com)\nLicensed under Creative Commons: By Attribution 4.0\nhttps://creativecommons.org/licenses/by/4.0/"
        );
    }

    #[test]
    fn the_catalogue_is_searched_by_every_word_and_titles_come_first() {
        let url = "https://incompetech.com/music/royalty-free/pieces.json";
        let found = parse_incompetech(PIECES, url, "calm piano", None, 1).unwrap();
        let titles: Vec<&str> = found.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, vec!["Calm Piano", "Quiet"]);
        let first = &found[0];
        assert_eq!(first.id, "Calm Piano.mp3");
        assert_eq!(first.duration_ms, 90_000);
        assert_eq!(first.license.spdx, "CC-BY-4.0");
        assert_eq!(first.creator, "Kevin MacLeod");
        assert_eq!(
            first.download_url,
            "https://incompetech.com/music/royalty-free/mp3-royaltyfree/Calm%20Piano.mp3"
        );
        assert_eq!(first.attribution, incompetech_credit("Calm Piano"));
        let short = parse_incompetech(PIECES, url, "piano", Some(60_000), 1).unwrap();
        assert_eq!(short.len(), 1);
        assert_eq!(short[0].title, "Quiet");
        let britons = parse_incompetech(PIECES, url, "medieval lute", None, 1).unwrap();
        assert_eq!(britons[0].duration_ms, 307_000);
        assert!(britons[0].landing_url.ends_with("?isrc=USUAN2600004"));
        assert!(
            parse_incompetech(PIECES, url, "evil", None, 1)
                .unwrap()
                .is_empty()
        );
        assert!(parse_incompetech(PIECES, url, "  ", None, 1).is_err());
        assert!(parse_incompetech("{", url, "x", None, 1).is_err());
    }

    #[test]
    fn a_test_server_gets_its_own_audio_directory() {
        assert_eq!(
            audio_base("http://127.0.0.1:9/music/pieces.json"),
            "http://127.0.0.1:9/music/mp3-royaltyfree/"
        );
    }

    const UPLOADS: &str = r#"[
      {"upload_id":71387,"upload_name":"Greedy","user_name":"Radioontheshelf","user_real_name":"Radio on the Shelf",
       "file_page_url":"https://ccmixter.org/files/Radioontheshelf/71387",
       "artist_page_url":"https://ccmixter.org/people/Radioontheshelf",
       "license_url":"http://creativecommons.org/licenses/by/2.5/","license_name":"Attribution",
       "files":[{"file_id":1,"download_url":"https://ccmixter.org/content/Radioontheshelf/Radioontheshelf_-_Greedy.mp3","file_nicname":"mp3","file_format_info":{"ps":"4:31","media-type":"audio"}}]},
      {"upload_id":2,"upload_name":"NC one","user_name":"x","license_url":"http://creativecommons.org/licenses/by-nc/3.0/","license_name":"Attribution Noncommercial",
       "files":[{"download_url":"https://ccmixter.org/content/x/nc.mp3","file_nicname":"mp3"}]},
      {"upload_id":3,"upload_name":"No audio","user_name":"y","license_url":"http://creativecommons.org/licenses/by/3.0/",
       "files":[{"download_url":"https://ccmixter.org/content/y/a.zip","file_nicname":"zip"}]},
      {"upload_id":4,"upload_name":"Strange","user_name":"z","license_url":"http://example.com/mine",
       "files":[{"download_url":"https://ccmixter.org/content/z/s.mp3"}]}
    ]"#;

    #[test]
    fn ccmixter_uploads_are_read_with_their_licence_and_filtered() {
        let (all, hidden) = parse_ccmixter(UPLOADS, false).unwrap();
        assert_eq!(hidden, 0);
        let ids: Vec<&str> = all.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["71387", "2", "4"]);
        let greedy = &all[0];
        assert_eq!(greedy.license.spdx, "CC-BY-2.5");
        assert_eq!(greedy.creator, "Radio on the Shelf");
        assert_eq!(greedy.duration_ms, 271_000);
        assert_eq!(
            greedy.attribution,
            "\"Greedy\" by Radio on the Shelf (https://ccmixter.org/files/Radioontheshelf/71387), licensed under CC BY 2.5 (https://creativecommons.org/licenses/by/2.5/)"
        );
        let (commercial, hidden) = parse_ccmixter(UPLOADS, true).unwrap();
        assert_eq!(commercial.len(), 1);
        assert_eq!(hidden, 2, "the NC one and the one with an unknown licence");
    }

    #[test]
    fn the_ccmixter_query_is_https_and_asks_for_attribution_licences() {
        assert_eq!(
            search_url(CCMIXTER_URL, "soft piano", 3).unwrap(),
            "https://ccmixter.org/api/query?f=json&search=soft%20piano&lic=by&limit=15&offset=30"
        );
        assert!(search_url(CCMIXTER_URL, "", 1).is_err());
        assert!(matches!(
            ccmixter_item("1;drop"),
            Err(Error::NotFound { .. })
        ));
    }

    #[test]
    fn downloads_come_from_the_provider_s_own_site_over_https() {
        let inc = Allowlist::new(INCOMPETECH_HOSTS, None);
        assert!(
            inc.check(
                Provider::Incompetech,
                "x",
                "https://incompetech.com/music/royalty-free/mp3-royaltyfree/a.mp3"
            )
            .is_ok()
        );
        assert!(
            inc.check(Provider::Incompetech, "x", "https://evil.example/a.mp3")
                .is_err()
        );
        let mix = Allowlist::new(CCMIXTER_HOSTS, None);
        assert!(
            mix.check(
                Provider::Ccmixter,
                "1",
                "https://ccmixter.org/content/a.mp3"
            )
            .is_ok()
        );
        assert!(
            mix.check(Provider::Ccmixter, "1", "http://ccmixter.org/content/a.mp3")
                .is_err()
        );
        assert!(
            mix.check(Provider::Ccmixter, "1", "https://files.example/a.mp3")
                .is_err()
        );
    }

    #[test]
    fn clock_times_are_read() {
        assert_eq!(clock_ms("00:05:07"), 307_000);
        assert_eq!(clock_ms("4:31"), 271_000);
        assert_eq!(clock_ms("45"), 45_000);
        assert_eq!(clock_ms("n/a"), 0);
        assert_eq!(clock_ms(""), 0);
    }
}
