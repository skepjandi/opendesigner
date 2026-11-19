//! Finding images and videos anywhere on the user's computer and copying one into the project
//! (`docs/wiki/security.md#device-access`).
//!
//! The user chose to let the agent reach their files (a switch in the Connect tab; on by
//! default). Reading is the only thing it can do: a search lists names, sizes and dates under the
//! home folder, and an import copies one file into the project's `media/` folder after the same
//! content check the user's own imports get. Nothing is ever written outside the project and no
//! file is returned to the agent except as a copy in `media/`.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::token;

const OFF_MARKER: &str = "device-access-off";
const IMAGE_EXT: [&str; 4] = ["png", "jpg", "jpeg", "webp"];
const VIDEO_EXT: [&str; 3] = ["mp4", "mov", "m4v"];
/// Folders that hold programs, caches or other people's data, not the user's pictures.
const SKIP_DIRS: [&str; 9] = [
    "node_modules",
    "target",
    "Library",
    "AppData",
    ".git",
    "venv",
    "__pycache__",
    "Pods",
    "DerivedData",
];
pub const MAX_RESULTS: usize = 200;
const MAX_DEPTH: usize = 7;
const MAX_VISITED: usize = 200_000;

/// Whether the user lets the agent look for files (`config` is the config folder).
pub fn enabled_in(config: &Path) -> bool {
    !config.join(OFF_MARKER).exists()
}

pub fn enabled() -> bool {
    token::config_dir().map_or(true, |c| enabled_in(&c))
}

pub fn set_enabled_in(config: &Path, on: bool) -> std::io::Result<()> {
    let marker = config.join(OFF_MARKER);
    if on {
        match std::fs::remove_file(&marker) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        std::fs::create_dir_all(config)?;
        std::fs::write(marker, "off\n")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    Any,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "image" => Some(Kind::Image),
            "video" => Some(Kind::Video),
            "any" | "" => Some(Kind::Any),
            _ => None,
        }
    }

    fn matches(self, ext: &str) -> Option<&'static str> {
        let image = IMAGE_EXT.contains(&ext);
        let video = VIDEO_EXT.contains(&ext);
        match self {
            Kind::Image if image => Some("image"),
            Kind::Video if video => Some("video"),
            Kind::Any if image => Some("image"),
            Kind::Any if video => Some("video"),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub path: PathBuf,
    pub kind: &'static str,
    pub bytes: u64,
    pub modified_secs: u64,
}

/// The user's home folder.
pub fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from)
}

/// Files of `kind` under `roots` whose name contains `query` (case-insensitive), newest first,
/// at most [`MAX_RESULTS`]. Hidden folders, caches and package folders are skipped, symlinks are
/// not followed.
pub fn find(roots: &[PathBuf], kind: Kind, query: &str) -> Vec<Found> {
    let query = query.to_lowercase();
    let mut out = Vec::new();
    let mut visited = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = roots.iter().map(|r| (r.clone(), 0)).collect();
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_VISITED {
                return sorted(out);
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                if depth < MAX_DEPTH
                    && !name.starts_with('.')
                    && !SKIP_DIRS.contains(&name.as_str())
                {
                    stack.push((entry.path(), depth + 1));
                }
                continue;
            }
            if !ft.is_file() || name.starts_with('.') {
                continue;
            }
            let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase());
            let Some(found_kind) = ext.as_deref().and_then(|e| kind.matches(e)) else {
                continue;
            };
            if !query.is_empty() && !name.to_lowercase().contains(&query) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            out.push(Found {
                path: entry.path(),
                kind: found_kind,
                bytes: meta.len(),
                modified_secs: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_secs()),
            });
        }
    }
    sorted(out)
}

fn sorted(mut out: Vec<Found>) -> Vec<Found> {
    out.sort_by(|a, b| {
        b.modified_secs
            .cmp(&a.modified_secs)
            .then(a.path.cmp(&b.path))
    });
    out.truncate(MAX_RESULTS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_finds_pictures_and_videos_by_name_newest_first_and_skips_hidden_and_cache_folders() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        for f in [
            "Pictures/logo.png",
            "Pictures/trip/beach.JPG",
            "Movies/demo.mov",
            "notes/readme.txt",
            "node_modules/pkg/icon.png",
            ".hidden/secret.png",
            "Library/Caches/thumb.png",
            "Pictures/.dot.png",
        ] {
            let path = p.join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        let names = |k, q| -> Vec<String> {
            find(&[p.to_path_buf()], k, q)
                .iter()
                .map(|f| f.path.strip_prefix(p).unwrap().display().to_string())
                .collect()
        };
        let mut images = names(Kind::Image, "");
        images.sort();
        assert_eq!(images, ["Pictures/logo.png", "Pictures/trip/beach.JPG"]);
        assert_eq!(names(Kind::Video, ""), ["Movies/demo.mov"]);
        assert_eq!(names(Kind::Any, "BEACH"), ["Pictures/trip/beach.JPG"]);
        assert!(names(Kind::Image, "secret").is_empty());
    }

    #[test]
    fn the_switch_is_a_marker_in_the_config_folder() {
        let d = tempfile::tempdir().unwrap();
        assert!(enabled_in(d.path()));
        set_enabled_in(d.path(), false).unwrap();
        assert!(!enabled_in(d.path()));
        set_enabled_in(d.path(), true).unwrap();
        assert!(enabled_in(d.path()));
        set_enabled_in(d.path(), true).unwrap();
    }

    #[test]
    fn kinds_parse() {
        assert_eq!(Kind::parse("image"), Some(Kind::Image));
        assert_eq!(Kind::parse(""), Some(Kind::Any));
        assert_eq!(Kind::parse("audio"), None);
    }
}
