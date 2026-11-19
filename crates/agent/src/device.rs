//! Finding images, videos and sounds anywhere on the user's computer and copying one into the project
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
const AUDIO_EXT: [&str; 5] = ["mp3", "wav", "m4a", "aac", "ogg"];
/// The largest sound file that is copied into a project.
pub const MAX_AUDIO_BYTES: u64 = 512 * 1024 * 1024;
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
    Audio,
    Any,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Kind> {
        match s {
            "image" => Some(Kind::Image),
            "video" => Some(Kind::Video),
            "audio" => Some(Kind::Audio),
            "any" | "" => Some(Kind::Any),
            _ => None,
        }
    }

    fn matches(self, ext: &str) -> Option<&'static str> {
        let image = IMAGE_EXT.contains(&ext);
        let video = VIDEO_EXT.contains(&ext);
        let audio = AUDIO_EXT.contains(&ext);
        match self {
            Kind::Image if image => Some("image"),
            Kind::Video if video => Some("video"),
            Kind::Audio if audio => Some("audio"),
            Kind::Any if image => Some("image"),
            Kind::Any if video => Some("video"),
            Kind::Any if audio => Some("audio"),
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

/// What a sound file is, from its first bytes: the extension it must have.
pub fn sniff_audio(head: &[u8]) -> Option<&'static str> {
    if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WAVE" {
        Some("wav")
    } else if head.starts_with(b"OggS") {
        Some("ogg")
    } else if head.len() >= 12 && &head[4..8] == b"ftyp" {
        // a QuickTime or MP4 video is a "ftyp" file as well: only the audio brands count
        matches!(&head[8..12], b"M4A " | b"M4B " | b"M4P ").then_some("m4a")
    } else if head.starts_with(b"ID3") {
        Some("mp3")
    } else if head.len() >= 2 && head[0] == 0xFF && head[1] & 0xF0 == 0xF0 && head[1] & 0x06 == 0 {
        // ADTS: sync word 0xFFF, layer bits 00
        Some("aac")
    } else if head.len() >= 2 && head[0] == 0xFF && head[1] & 0xE0 == 0xE0 && head[1] & 0x06 != 0 {
        // an MPEG audio frame: sync word, layer bits not 00
        Some("mp3")
    } else {
        None
    }
}

/// Copies a sound the user has into `<project>/media/` and returns its reference. The file must
/// be what its name says (checked by its first bytes); it is never overwritten or changed.
pub fn import_audio(project: &Path, src: &Path) -> Result<String, String> {
    use std::io::Read;
    let meta = std::fs::metadata(src).map_err(|e| format!("{}: {e}", src.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", src.display()));
    }
    if meta.len() > MAX_AUDIO_BYTES {
        return Err(format!(
            "{} is {} bytes; the most a sound may take is {MAX_AUDIO_BYTES}",
            src.display(),
            meta.len()
        ));
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| AUDIO_EXT.contains(&e.as_str()))
        .ok_or_else(|| {
            format!(
                "{} is not an .mp3, .wav, .m4a, .aac or .ogg file",
                src.display()
            )
        })?;
    let mut head = [0u8; 12];
    let read = std::fs::File::open(src)
        .and_then(|mut f| f.read(&mut head))
        .map_err(|e| format!("{}: {e}", src.display()))?;
    match sniff_audio(&head[..read]) {
        Some(kind) if kind == ext => {}
        Some(kind) => {
            return Err(format!(
                "{} is named .{ext} but its content is {kind} audio; rename it",
                src.display()
            ));
        }
        None => {
            return Err(format!(
                "{} is not a sound file this app can use",
                src.display()
            ));
        }
    }
    let stem: String = src
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sound")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let stem = stem.trim_matches(['-', ' ']);
    let stem: String = if stem.is_empty() {
        "sound".into()
    } else {
        stem.chars().take(60).collect()
    };
    let media = project.join(od_core::media::MEDIA_DIR);
    std::fs::create_dir_all(&media).map_err(|e| format!("{}: {e}", media.display()))?;
    for n in 1..10_000 {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let dest = media.join(&name);
        let mut out = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("{}: {e}", dest.display())),
        };
        let copied =
            std::fs::File::open(src).and_then(|mut from| std::io::copy(&mut from, &mut out));
        return match copied {
            Ok(_) => Ok(format!("media/{name}")),
            Err(e) => {
                drop(out);
                let _ = std::fs::remove_file(&dest);
                Err(format!("{}: {e}", src.display()))
            }
        };
    }
    Err("no free file name in media/".into())
}

/// The sound files in `<project>/media/` (plain files only, never through a symlink), by name:
/// `(reference, bytes)`.
pub fn list_audio(project: &Path) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(project.join(od_core::media::MEDIA_DIR)) else {
        return Vec::new();
    };
    let mut out: Vec<(String, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = std::fs::symlink_metadata(e.path())
                .ok()
                .filter(|m| m.is_file())?;
            let reference = format!("media/{}", e.file_name().to_string_lossy());
            od_core::media::audio_name(&reference).ok()?;
            Some((reference, meta.len()))
        })
        .collect();
    out.sort();
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
        assert_eq!(Kind::parse("audio"), Some(Kind::Audio));
        assert_eq!(Kind::parse("sound"), None);
    }

    #[test]
    fn it_finds_sounds_and_sniffs_them_by_their_first_bytes() {
        let d = tempfile::tempdir().unwrap();
        for f in [
            "Music/song.MP3",
            "Music/a.wav",
            "Music/notes.txt",
            "Music/clip.mov",
        ] {
            let path = d.path().join(f);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        let mut found: Vec<String> = find(&[d.path().to_path_buf()], Kind::Audio, "")
            .iter()
            .map(|f| {
                format!(
                    "{} {}",
                    f.kind,
                    f.path.file_name().unwrap().to_string_lossy()
                )
            })
            .collect();
        found.sort();
        assert_eq!(found, ["audio a.wav", "audio song.MP3"]);
        assert_eq!(find(&[d.path().to_path_buf()], Kind::Any, "").len(), 3);

        assert_eq!(sniff_audio(b"RIFF\x24\0\0\0WAVEfmt "), Some("wav"));
        assert_eq!(sniff_audio(b"RIFF\x24\0\0\0WEBPVP8 "), None);
        assert_eq!(sniff_audio(b"OggS\0\x02\0\0\0\0\0\0"), Some("ogg"));
        assert_eq!(sniff_audio(b"ID3\x04\0\0\0\0\0\0\0\0"), Some("mp3"));
        assert_eq!(
            sniff_audio(&[0xFF, 0xFB, 0x90, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Some("mp3")
        );
        assert_eq!(
            sniff_audio(&[0xFF, 0xF1, 0x50, 0x80, 0, 0, 0, 0, 0, 0, 0, 0]),
            Some("aac")
        );
        assert_eq!(sniff_audio(b"\0\0\0\x20ftypM4A \0\0\0\0"), Some("m4a"));
        assert_eq!(
            sniff_audio(b"\0\0\0\x20ftypisom\0\0\0\0"),
            None,
            "a video is not a sound"
        );
        assert_eq!(sniff_audio(b"\x89PNG\r\n\x1a\n\0\0\0\0"), None);
    }

    #[test]
    fn importing_a_sound_checks_the_content_and_never_overwrites() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        let wav = home.path().join("My Song!.wav");
        std::fs::write(&wav, b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0").unwrap();
        let first = import_audio(project.path(), &wav).unwrap();
        assert_eq!(first, "media/My Song.wav");
        assert_eq!(
            import_audio(project.path(), &wav).unwrap(),
            "media/My Song-2.wav"
        );
        assert_eq!(
            std::fs::read(project.path().join(&first)).unwrap(),
            std::fs::read(&wav).unwrap()
        );
        // a fake, a wrong name, a wrong kind of file
        let fake = home.path().join("fake.mp3");
        std::fs::write(&fake, b"this is text, not sound").unwrap();
        assert!(import_audio(project.path(), &fake).is_err());
        let wrong = home.path().join("wrong.mp3");
        std::fs::write(&wrong, b"RIFF\x24\0\0\0WAVEfmt \x10\0\0\0").unwrap();
        assert!(
            import_audio(project.path(), &wrong)
                .unwrap_err()
                .contains("wav")
        );
        let txt = home.path().join("a.txt");
        std::fs::write(&txt, b"x").unwrap();
        assert!(import_audio(project.path(), &txt).is_err());
        assert!(import_audio(project.path(), home.path()).is_err());
        let listed: Vec<String> = list_audio(project.path())
            .into_iter()
            .map(|(f, _)| f)
            .collect();
        assert_eq!(listed, ["media/My Song-2.wav", "media/My Song.wav"]);
    }
}
