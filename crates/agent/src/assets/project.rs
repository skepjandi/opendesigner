//! Putting an asset into the project: the file in `media/` and, next to it, a sidecar that says
//! where it came from and under which licence. Nothing is overwritten.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Result, io, msg};

/// What is written next to an asset: `media/<name>.asset.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sidecar {
    pub id: String,
    pub source: String,
    pub licence: String,
    pub licence_url: String,
    /// Whether the licence asks for the credit line below to be shown with the work.
    pub attribution_required: bool,
    pub attribution: String,
    /// For a generated asset, the recipe that remakes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recipe: Option<Value>,
}

/// An asset now in the project.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Placed {
    /// `media/<name>.<ext>`: what an image node (or a video's sound track) names.
    pub file: String,
    pub sidecar: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

/// A plain file stem: lowercase letters, digits, `-` and `_`.
pub fn clean_stem(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    s.chars().take(60).collect()
}

/// Writes `bytes` as `media/<stem>.<ext>` (a free stem is found) and the sidecar beside it.
pub fn place(
    project: &Path,
    stem: &str,
    ext: &str,
    bytes: &[u8],
    sidecar: &Sidecar,
    size: Option<(u32, u32)>,
) -> Result<Placed> {
    let stem = clean_stem(stem);
    if stem.is_empty() || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        return msg("the asset needs a plain name");
    }
    let media = project.join("media");
    std::fs::create_dir_all(&media).map_err(|e| io(&media, e))?;
    let mut chosen = None;
    for n in 1..10_000 {
        let candidate = if n == 1 {
            stem.clone()
        } else {
            format!("{stem}-{n}")
        };
        if !media.join(format!("{candidate}.{ext}")).exists()
            && !media.join(format!("{candidate}.asset.json")).exists()
        {
            chosen = Some(candidate);
            break;
        }
    }
    let Some(name) = chosen else {
        return msg("no free file name for the asset");
    };
    let file = media.join(format!("{name}.{ext}"));
    let side = media.join(format!("{name}.asset.json"));
    write_new(&file, bytes)?;
    let text =
        serde_json::to_string_pretty(sidecar).map_err(|e| super::Error::Msg(e.to_string()))?;
    if let Err(e) = write_new(&side, text.as_bytes()) {
        let _ = std::fs::remove_file(&file);
        return Err(e);
    }
    Ok(Placed {
        file: format!("media/{name}.{ext}"),
        sidecar: format!("media/{name}.asset.json"),
        width: size.map(|s| s.0),
        height: size.map(|s| s.1),
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| io(path, e))?;
    std::io::Write::write_all(&mut file, bytes).map_err(|e| io(path, e))
}

/// The assets already in the project and the credit line each one needs.
pub fn credits(project: &Path) -> Vec<(String, Sidecar)> {
    let Ok(entries) = std::fs::read_dir(project.join("media")) else {
        return Vec::new();
    };
    let mut out: Vec<(String, Sidecar)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".asset.json")?.to_string();
            let sidecar = serde_json::from_str(&std::fs::read_to_string(e.path()).ok()?).ok()?;
            Some((stem, sidecar))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}
