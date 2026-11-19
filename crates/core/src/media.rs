//! The project's `media/` folder: what an image node may reference and how files get there.
//!
//! A node names a file as `media/<name>`; nothing else is a valid reference, so a project file or
//! an agent cannot point the renderer outside the folder. Files enter the folder only through
//! [`import`], which the user starts (a file dialog or a drop); no agent tool reads or copies
//! arbitrary paths.

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{OpError, ProjectError};
use crate::limits::MAX_FILE_BYTES;

pub const MEDIA_DIR: &str = "media";
const MAX_NAME_LEN: usize = 100;
const EXTENSIONS: [&str; 4] = ["png", "jpg", "jpeg", "webp"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Webp,
}

/// Recognizes the formats the renderer decodes from the first bytes of a file.
pub fn sniff(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else {
        None
    }
}

fn invalid(detail: impl Into<String>) -> OpError {
    OpError::OutOfBounds {
        what: "image",
        detail: detail.into(),
    }
}

fn name_is_safe(name: &str) -> bool {
    let ext_ok = name
        .rsplit_once('.')
        .is_some_and(|(_, ext)| EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()));
    ext_ok
        && !name.starts_with('.')
        && name.chars().count() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
}

/// The file name inside a `media/<name>` reference, or the rule the reference breaks.
pub fn reference_name(reference: &str) -> Result<&str, OpError> {
    let name = reference
        .strip_prefix("media/")
        .ok_or_else(|| invalid(format!("{reference:?} must look like media/<file name>")))?;
    if name_is_safe(name) {
        Ok(name)
    } else {
        Err(invalid(format!(
            "{name:?} must be a plain file name (letters, digits, space . _ -) ending in .png, .jpg, .jpeg or .webp"
        )))
    }
}

/// One file in `media/`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MediaFile {
    /// The reference to put in an image node: `media/<name>`.
    pub path: String,
    pub bytes: u64,
}

pub const FOOTAGE_EXTENSIONS: [&str; 3] = ["mp4", "mov", "m4v"];
/// Largest video file the user may import (footage is long; images are not).
pub const MAX_FOOTAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;

/// Whether the first bytes are an MP4/QuickTime container (`ftyp` or `moov`/`wide`/`mdat` box).
pub fn sniff_footage(head: &[u8]) -> bool {
    head.len() >= 12 && matches!(&head[4..8], b"ftyp" | b"moov" | b"wide" | b"mdat")
}

/// The file name in a footage reference `media/<name>.mp4|mov|m4v`, or the rule it breaks.
pub fn footage_name(reference: &str) -> Result<&str, OpError> {
    let bad = |detail: String| OpError::OutOfBounds {
        what: "footage",
        detail,
    };
    let name = reference
        .strip_prefix("media/")
        .ok_or_else(|| bad(format!("{reference:?} must look like media/<file name>")))?;
    let ext_ok = name
        .rsplit_once('.')
        .is_some_and(|(_, e)| FOOTAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    let chars_ok = !name.starts_with('.')
        && name.chars().count() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '));
    if ext_ok && chars_ok {
        Ok(name)
    } else {
        Err(bad(format!(
            "{name:?} must be a plain file name ending in .mp4, .mov or .m4v"
        )))
    }
}

pub const AUDIO_EXTENSIONS: [&str; 5] = ["mp3", "wav", "m4a", "aac", "ogg"];

/// The file name in an audio reference `media/<name>.mp3|wav|m4a|aac|ogg`, or the rule it breaks.
/// A sound track of a video names a file this way, like an image node names its picture.
pub fn audio_name(reference: &str) -> Result<&str, OpError> {
    let bad = |detail: String| OpError::OutOfBounds {
        what: "audio",
        detail,
    };
    let name = reference
        .strip_prefix("media/")
        .ok_or_else(|| bad(format!("{reference:?} must look like media/<file name>")))?;
    let ext_ok = name
        .rsplit_once('.')
        .is_some_and(|(_, e)| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()));
    let chars_ok = !name.starts_with('.')
        && name.chars().count() <= MAX_NAME_LEN
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '));
    if ext_ok && chars_ok {
        Ok(name)
    } else {
        Err(bad(format!(
            "{name:?} must be a plain file name ending in .mp3, .wav, .m4a, .aac or .ogg"
        )))
    }
}

/// Copies a video the user chose into `<project>/media/` and returns its reference. Only MP4 and
/// QuickTime files are accepted (checked by their first bytes).
pub fn import_footage(project_dir: &Path, src: &Path) -> Result<String, ProjectError> {
    let meta = fs::metadata(src).map_err(|e| io_err(src, e))?;
    if !meta.is_file() {
        return Err(ProjectError::Invalid(format!(
            "{} is not a file",
            src.display()
        )));
    }
    if meta.len() > MAX_FOOTAGE_BYTES {
        return Err(ProjectError::TooLarge {
            path: src.display().to_string(),
            size: meta.len(),
            limit: MAX_FOOTAGE_BYTES,
        });
    }
    let mut head = [0u8; 12];
    let mut file = File::open(src).map_err(|e| io_err(src, e))?;
    let read = file.read(&mut head).map_err(|e| io_err(src, e))?;
    if !sniff_footage(&head[..read]) {
        return Err(ProjectError::Invalid(format!(
            "{} is not an MP4 or QuickTime video",
            src.display()
        )));
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .filter(|e| FOOTAGE_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or_else(|| "mp4".to_string());
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .map(clean_name)
        .unwrap_or_else(|| "footage".to_string());
    let media = project_dir.join(MEDIA_DIR);
    fs::create_dir_all(&media).map_err(|e| io_err(&media, e))?;
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let dest = media.join(&name);
        let mut out = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_err(&dest, e)),
        };
        let mut from = File::open(src).map_err(|e| io_err(src, e))?;
        match io::copy(&mut from, &mut out) {
            Ok(_) => return Ok(format!("{MEDIA_DIR}/{name}")),
            Err(e) => {
                drop(out);
                let _ = fs::remove_file(&dest);
                return Err(io_err(src, e));
            }
        }
    }
    Err(ProjectError::Invalid("no free file name in media/".into()))
}

/// The videos in `<project>/media/`, sorted by name.
pub fn list_footage(project_dir: &Path) -> Result<Vec<MediaFile>, ProjectError> {
    let media = project_dir.join(MEDIA_DIR);
    let entries = match fs::read_dir(&media) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(&media, e)),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let meta = match fs::symlink_metadata(entry.path()) {
            Ok(m) if m.is_file() => m,
            _ => continue,
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if footage_name(&format!("{MEDIA_DIR}/{name}")).is_ok() {
            out.push(MediaFile {
                path: format!("{MEDIA_DIR}/{name}"),
                bytes: meta.len(),
            });
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Where a footage reference lives on disk (never through a symlink).
pub fn resolve_footage(project_dir: &Path, reference: &str) -> Result<PathBuf, ProjectError> {
    let name = footage_name(reference).map_err(|e| ProjectError::Invalid(e.to_string()))?;
    let path = project_dir.join(MEDIA_DIR).join(name);
    match fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() => Ok(path),
        Ok(_) => Err(ProjectError::Invalid(format!(
            "{reference} is not a plain file"
        ))),
        Err(e) => Err(io_err(&path, e)),
    }
}

fn io_err(path: &Path, source: io::Error) -> ProjectError {
    ProjectError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn clean_name(stem: &str) -> String {
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(['.', '-']).to_string();
    if cleaned.is_empty() {
        "image".to_string()
    } else {
        cleaned.chars().take(60).collect()
    }
}

/// Copies an image the user chose into `<project>/media/` under a safe, unused name and returns
/// its reference. Refuses anything that is not a PNG, JPEG or WebP, or is over the size limit.
pub fn import(project_dir: &Path, src: &Path) -> Result<String, ProjectError> {
    let meta = fs::metadata(src).map_err(|e| io_err(src, e))?;
    if !meta.is_file() {
        return Err(ProjectError::Invalid(format!(
            "{} is not a file",
            src.display()
        )));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(ProjectError::TooLarge {
            path: src.display().to_string(),
            size: meta.len(),
            limit: MAX_FILE_BYTES,
        });
    }
    let mut head = [0u8; 12];
    let mut file = File::open(src).map_err(|e| io_err(src, e))?;
    let read = file.read(&mut head).map_err(|e| io_err(src, e))?;
    let format = sniff(&head[..read]).ok_or_else(|| {
        ProjectError::Invalid(format!(
            "{} is not a PNG, JPEG or WebP image",
            src.display()
        ))
    })?;
    let ext = match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
    };
    let stem = src
        .file_stem()
        .and_then(|s| s.to_str())
        .map(clean_name)
        .unwrap_or_else(|| "image".to_string());

    let media = project_dir.join(MEDIA_DIR);
    fs::create_dir_all(&media).map_err(|e| io_err(&media, e))?;
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{n}.{ext}")
        };
        let dest = media.join(&name);
        // create_new refuses to overwrite or follow a planted symlink
        let mut out = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_err(&dest, e)),
        };
        let mut from = File::open(src).map_err(|e| io_err(src, e))?;
        let copied = io::copy(&mut (&mut from).take(MAX_FILE_BYTES + 1), &mut out);
        match copied {
            Ok(len) if len <= MAX_FILE_BYTES => return Ok(format!("{MEDIA_DIR}/{name}")),
            Ok(len) => {
                drop(out);
                let _ = fs::remove_file(&dest);
                return Err(ProjectError::TooLarge {
                    path: src.display().to_string(),
                    size: len,
                    limit: MAX_FILE_BYTES,
                });
            }
            Err(e) => {
                drop(out);
                let _ = fs::remove_file(&dest);
                return Err(io_err(src, e));
            }
        }
    }
    Err(ProjectError::Invalid("no free file name in media/".into()))
}

/// The files in `<project>/media/` that an image node may reference, sorted by name.
pub fn list(project_dir: &Path) -> Result<Vec<MediaFile>, ProjectError> {
    let media = project_dir.join(MEDIA_DIR);
    let entries = match fs::read_dir(&media) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(io_err(&media, e)),
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| io_err(&media, e))?;
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if meta.is_file() && name_is_safe(&name) {
            files.push(MediaFile {
                path: format!("{MEDIA_DIR}/{name}"),
                bytes: meta.len(),
            });
        }
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

/// The on-disk path of a reference: a regular file (not a symlink) directly inside `media/`.
pub fn resolve(project_dir: &Path, reference: &str) -> Result<PathBuf, ProjectError> {
    let name = reference_name(reference).map_err(|e| ProjectError::Invalid(e.to_string()))?;
    let path = project_dir.join(MEDIA_DIR).join(name);
    let meta = fs::symlink_metadata(&path).map_err(|e| io_err(&path, e))?;
    if !meta.is_file() {
        return Err(ProjectError::Invalid(format!(
            "{reference} is not a regular file"
        )));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(ProjectError::TooLarge {
            path: reference.to_string(),
            size: meta.len(),
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000";

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn only_plain_media_references_are_valid() {
        assert_eq!(reference_name("media/shot 1.png").unwrap(), "shot 1.png");
        for bad in [
            "shot.png",
            "media/",
            "media/../x.png",
            "media/a/b.png",
            "/etc/passwd",
            "media/.hidden.png",
            "media/x.svg",
            "media/x.png\n",
            "media\\x.png",
            "media/x:y.png",
        ] {
            assert!(reference_name(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn import_copies_sniffed_images_under_safe_unique_names() {
        let project = dir();
        let src_dir = dir();
        let src = src_dir.path().join("My Shot (final)!.PNG");
        fs::write(&src, PNG).unwrap();
        let first = import(project.path(), &src).unwrap();
        let second = import(project.path(), &src).unwrap();
        assert_eq!(first, "media/My-Shot--final.png");
        assert_eq!(second, "media/My-Shot--final-2.png");
        assert_eq!(fs::read(project.path().join(&first)).unwrap(), PNG);
        let listed = list(project.path()).unwrap();
        assert_eq!(
            listed.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            [second.as_str(), first.as_str()],
        );
        assert_eq!(listed[0].bytes, PNG.len() as u64);
        assert!(resolve(project.path(), &first).unwrap().is_file());
    }

    #[test]
    fn import_refuses_what_is_not_an_image_and_never_overwrites() {
        let project = dir();
        let src_dir = dir();
        let text = src_dir.path().join("notes.png");
        fs::write(&text, b"not an image").unwrap();
        assert!(matches!(
            import(project.path(), &text),
            Err(ProjectError::Invalid(_))
        ));
        assert!(matches!(
            import(project.path(), src_dir.path()),
            Err(ProjectError::Invalid(_))
        ));
        assert!(list(project.path()).unwrap().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_in_media_is_neither_listed_nor_resolved() {
        let project = dir();
        let outside = dir();
        let secret = outside.path().join("secret.png");
        fs::write(&secret, PNG).unwrap();
        fs::create_dir_all(project.path().join(MEDIA_DIR)).unwrap();
        std::os::unix::fs::symlink(&secret, project.path().join("media/link.png")).unwrap();
        assert!(list(project.path()).unwrap().is_empty());
        assert!(resolve(project.path(), "media/link.png").is_err());
    }

    #[test]
    fn footage_is_sniffed_named_safely_listed_apart_from_images_and_resolved_without_symlinks() {
        let d = dir();
        let src = d.path().join("My Clip!.MOV");
        fs::write(&src, b"\0\0\0\x14ftypqt  0000").unwrap();
        let r = import_footage(d.path(), &src).unwrap();
        assert_eq!(r, "media/My-Clip.mov");
        assert_eq!(
            import_footage(d.path(), &src).unwrap(),
            "media/My-Clip-2.mov"
        );
        assert_eq!(list_footage(d.path()).unwrap().len(), 2);
        assert!(list(d.path()).unwrap().is_empty(), "videos are not images");
        assert!(
            reference_name(&r).is_err(),
            "an image node cannot name a video"
        );
        assert!(resolve_footage(d.path(), &r).is_ok());
        for bad in [
            "media/../x.mp4",
            "x.mp4",
            "media/a.png",
            "media/.h.mp4",
            "media/a/b.mp4",
        ] {
            assert!(footage_name(bad).is_err(), "{bad}");
        }
        let not = d.path().join("t.mp4");
        fs::write(&not, b"just text, not video").unwrap();
        assert!(import_footage(d.path(), &not).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&src, d.path().join("media/link.mp4")).unwrap();
            assert!(resolve_footage(d.path(), "media/link.mp4").is_err());
            assert_eq!(
                list_footage(d.path()).unwrap().len(),
                2,
                "symlinks are not listed"
            );
        }
    }

    #[test]
    fn formats_are_recognized_by_their_first_bytes() {
        assert_eq!(sniff(PNG), Some(ImageFormat::Png));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(ImageFormat::Jpeg));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 "), Some(ImageFormat::Webp));
        assert_eq!(sniff(b"<svg"), None);
        assert_eq!(sniff(b""), None);
    }
}
