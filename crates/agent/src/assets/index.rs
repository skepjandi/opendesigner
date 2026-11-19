//! Finding assets: the bundled Lucide set, the generators and every installed pack, searched by
//! name and tags, and turned into the drawable thing an import needs.

use std::path::Path;
use std::sync::OnceLock;

use serde::Serialize;

use super::catalog::{self, GENERATORS, LUCIDE};
use super::container::{self, Entry};
use super::{Result, msg, packs, raster};

static LUCIDE_ENTRIES: OnceLock<Vec<Entry>> = OnceLock::new();

/// The bundled icon set, inflated once.
fn lucide() -> &'static [Entry] {
    LUCIDE_ENTRIES.get_or_init(|| {
        container::decode(include_bytes!("../../assets/lucide.oda")).unwrap_or_default()
    })
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hit {
    /// Give this to `import_asset` (or, for a generator, the generator name to `make_asset`).
    pub id: String,
    pub name: String,
    pub kind: String,
    pub source: String,
    pub licence: String,
    pub tags: Vec<String>,
    /// `import_asset` or `make_asset`.
    pub use_with: &'static str,
}

/// What an id names, ready to draw or copy.
#[derive(Debug, Clone)]
pub enum Payload {
    Svg(String),
    File { ext: String, data: Vec<u8> },
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub id: String,
    pub name: String,
    pub source: String,
    pub licence: String,
    pub licence_url: String,
    pub attribution_required: bool,
    pub attribution: String,
    pub payload: Payload,
}

fn words(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// How well `terms` match a name and its tags; `None` when one term matches neither.
fn score(terms: &[String], name: &str, tags: &[String]) -> Option<u32> {
    let parts = words(name);
    let mut total: u32 = 0;
    for t in terms {
        let s = if name == t {
            100
        } else if parts.iter().any(|p| p == t) {
            60
        } else if name.contains(t.as_str()) {
            35
        } else if tags.iter().any(|g| g == t) {
            15
        } else if tags.iter().any(|g| g.contains(t.as_str())) {
            5
        } else {
            return None;
        };
        total += s;
    }
    // the whole query is the name; otherwise a name with fewer extra words reads closer
    if parts == terms {
        total += 200;
    }
    Some(total.saturating_sub(parts.len().saturating_sub(terms.len()) as u32 * 3))
}

fn split_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

/// Searches everything available. `kind` narrows to icon, shape, blob, background, pattern,
/// illustration, font, sound or 3d (the last two kinds the bundled set and generators lack).
pub fn search(root: &Path, query: &str, kind: Option<&str>, limit: usize) -> Vec<Hit> {
    let terms = words(query);
    let mut scored: Vec<(u32, Hit)> = Vec::new();
    let mut push = |s: Option<u32>, hit: Hit| {
        if let Some(s) = s
            && kind.is_none_or(|k| k == hit.kind)
        {
            scored.push((s, hit));
        }
    };
    let all = terms.is_empty();
    for e in lucide() {
        let tags = split_tags(&e.tags);
        push(
            if all {
                Some(1)
            } else {
                score(&terms, &e.name, &tags)
            },
            Hit {
                id: format!("{}:{}", LUCIDE.id, e.name),
                name: e.name.clone(),
                kind: "icon".into(),
                source: LUCIDE.name.into(),
                licence: LUCIDE.licence.into(),
                tags: tags.into_iter().take(6).collect(),
                use_with: "import_asset",
            },
        );
    }
    for (name, kind, about) in GENERATORS {
        let tags = words(about);
        push(
            if all {
                Some(1)
            } else {
                score(&terms, name, &tags)
            },
            Hit {
                id: (*name).into(),
                name: (*name).into(),
                kind: (*kind).into(),
                source: "OpenDesigner generator".into(),
                licence: "none needed (made from a seed)".into(),
                tags: vec![(*about).to_string()],
                use_with: "make_asset",
            },
        );
    }
    for id in packs::installed_ids(root) {
        let (Some(spec), Ok(entries)) = (catalog::pack(&id), packs::load(root, &id)) else {
            continue;
        };
        for e in entries.iter().filter(|e| e.name != "LICENSE") {
            let rel = catalog::relative(spec, &e.name);
            let rel = rel.rsplit_once('.').map_or(rel, |(s, _)| s);
            let stem = rel.rsplit('/').next().unwrap_or(rel);
            // the folders below the pack's own (a color, a size) are tags, never the name
            let tags: Vec<String> = rel
                .rsplit_once('/')
                .map(|(dirs, _)| dirs.split('/').flat_map(words).collect())
                .unwrap_or_default();
            push(
                if all {
                    Some(1)
                } else {
                    score(&terms, &stem.to_ascii_lowercase(), &tags)
                },
                Hit {
                    id: format!("{id}:{}", e.name),
                    name: stem.to_string(),
                    kind: spec.kind.into(),
                    source: spec.name.into(),
                    licence: spec.licence.into(),
                    tags: tags.into_iter().take(6).collect(),
                    use_with: "import_asset",
                },
            );
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    scored.into_iter().take(limit).map(|(_, h)| h).collect()
}

/// The asset an id names: `lucide:<name>` or `<pack id>:<path in the pack>`.
pub fn resolve(root: &Path, id: &str) -> Result<Resolved> {
    let Some((set, name)) = id.split_once(':') else {
        return msg(format!(
            "{id:?} is not an asset id; take one from search_assets (lucide:<icon> or <pack>:<file>)"
        ));
    };
    if set == LUCIDE.id {
        let Some(e) = lucide().iter().find(|e| e.name == name) else {
            return msg(format!(
                "there is no Lucide icon {name:?}; search_assets finds names"
            ));
        };
        return Ok(Resolved {
            id: id.into(),
            name: e.name.clone(),
            source: LUCIDE.name.into(),
            licence: LUCIDE.licence.into(),
            licence_url: LUCIDE.licence_url.into(),
            attribution_required: true,
            attribution: format!(
                "{}, {}, {}",
                LUCIDE.credit, LUCIDE.licence, LUCIDE.licence_url
            ),
            payload: Payload::Svg(raster::lucide_svg(&String::from_utf8_lossy(&e.body))),
        });
    }
    let Some(spec) = catalog::pack(set) else {
        return msg(format!(
            "there is no asset set {set:?}; list_asset_sources names them"
        ));
    };
    if !packs::installed_ids(root).iter().any(|p| p == set) {
        return msg(format!(
            "the pack {set} is not installed; install_asset_pack asks the user and downloads it ({} bytes)",
            spec.download_bytes
        ));
    }
    let entries = packs::load(root, set)?;
    let Some(e) = entries.into_iter().find(|e| e.name == name) else {
        return msg(format!(
            "{name:?} is not in the pack {set}; search_assets finds files"
        ));
    };
    let rel = catalog::relative(spec, &e.name);
    let (stem, ext) = rel.rsplit_once('.').unwrap_or((rel, ""));
    let stem = stem.replace('/', "-");
    let payload = if ext == "svg" {
        Payload::Svg(String::from_utf8_lossy(&e.body).into_owned())
    } else {
        Payload::File {
            ext: ext.to_ascii_lowercase(),
            data: e.body.clone(),
        }
    };
    Ok(Resolved {
        id: id.into(),
        name: stem,
        source: spec.name.into(),
        licence: spec.licence.into(),
        licence_url: spec.licence_url.into(),
        attribution_required: spec.attribution_required,
        attribution: format!("{}, {}, {}", spec.credit, spec.licence, spec.page),
        payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_set_is_found_by_name_and_by_tag() {
        let root = std::path::Path::new("/nonexistent-pack-root");
        assert!(lucide().len() > 2000);
        let hits = search(root, "arrow right", Some("icon"), 5);
        assert_eq!(hits[0].id, "lucide:arrow-right");
        assert!(
            search(root, "home", Some("icon"), 50)
                .iter()
                .any(|h| h.id == "lucide:house")
        );
        // a tag, not a name
        assert!(!search(root, "alphabet", Some("icon"), 5).is_empty());
        assert!(search(root, "zzzzqq", None, 5).is_empty());
        let kinds = search(root, "blob", None, 5);
        assert!(
            kinds
                .iter()
                .any(|h| h.id == "blob" && h.use_with == "make_asset")
        );
        assert!(search(root, "", Some("sound"), 5).is_empty());
    }

    #[test]
    fn an_id_resolves_to_a_wrapped_svg_with_its_licence() {
        let root = std::path::Path::new("/nonexistent-pack-root");
        let r = resolve(root, "lucide:arrow-right").unwrap();
        let Payload::Svg(svg) = &r.payload else {
            panic!("not svg")
        };
        assert!(svg.starts_with("<svg") && svg.contains("currentColor") && svg.contains("<path"));
        assert!(r.attribution.contains("ISC"));
        assert!(resolve(root, "lucide:nope").is_err());
        assert!(
            resolve(root, "tabler-icons:icons/outline/x.svg")
                .unwrap_err()
                .to_string()
                .contains("not installed")
        );
        assert!(resolve(root, "bad").is_err());
    }
}
