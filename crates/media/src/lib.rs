//! Encoding video by driving the FFmpeg program (`docs/wiki/video-editing.md#ffmpeg`).
//!
//! The editor draws every frame itself and pipes the raw pixels to `ffmpeg`, which compresses
//! them to an H.264 MP4 with the platform's own encoder where there is one (VideoToolbox on macOS,
//! Media Foundation on Windows, and so on), so no GPL encoder library is needed. FFmpeg runs as a
//! separate program: nothing is linked into this process, and the app never sends media anywhere.

use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::JoinHandle;

use thiserror::Error;

/// Names the `OPENDESIGNER_FFMPEG` environment variable gives a specific `ffmpeg` to use.
pub const FFMPEG_ENV: &str = "OPENDESIGNER_FFMPEG";

#[derive(Debug, Error)]
pub enum MediaError {
    #[error(
        "ffmpeg was not found. Install FFmpeg (for example `brew install ffmpeg`, `apt install ffmpeg` or `winget install ffmpeg`) or set {FFMPEG_ENV} to the program's path"
    )]
    NotFound,
    #[error("{path} did not run as ffmpeg: {reason}")]
    NotFfmpeg { path: String, reason: String },
    #[error(
        "this ffmpeg has no H.264 encoder that can be used ({0}); install a build with VideoToolbox, Media Foundation, OpenH264, NVENC, QSV, VAAPI or libx264"
    )]
    NoEncoder(String),
    #[error("ffmpeg failed: {0}")]
    Failed(String),
    #[error("{0}")]
    Io(String),
}

/// H.264 encoders in the order they are preferred: the ones the operating system or a
/// permissively licensed library provides first, GPL `libx264` last.
pub const H264_ENCODERS: &[&str] = &[
    "h264_videotoolbox",
    "h264_mf",
    "libopenh264",
    "h264_nvenc",
    "h264_qsv",
    "h264_amf",
    "h264_vaapi",
    "libx264",
];

/// A usable `ffmpeg` program.
#[derive(Debug, Clone)]
pub struct Ffmpeg {
    pub path: PathBuf,
    /// The first line of `ffmpeg -version`.
    pub version: String,
    /// Built with `--enable-gpl`: it may be used, but is not the build the app would ship.
    pub gpl: bool,
    encoders: HashSet<String>,
}

fn run(path: &Path, args: &[&str]) -> Result<String, MediaError> {
    let out = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| MediaError::NotFfmpeg {
            path: path.display().to_string(),
            reason: e.to_string(),
        })?;
    if !out.status.success() {
        return Err(MediaError::NotFfmpeg {
            path: path.display().to_string(),
            reason: format!("exited with {}", out.status),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

impl Ffmpeg {
    /// Runs `path -version` and `-encoders` to learn what it is.
    pub fn probe(path: &Path) -> Result<Ffmpeg, MediaError> {
        let version_text = run(path, &["-hide_banner", "-version"])?;
        let version = version_text.lines().next().unwrap_or("").to_string();
        if !version.starts_with("ffmpeg version") {
            return Err(MediaError::NotFfmpeg {
                path: path.display().to_string(),
                reason: format!("its version line is {version:?}"),
            });
        }
        let encoders = run(path, &["-hide_banner", "-encoders"])?
            .lines()
            .filter_map(|l| {
                let mut parts = l.split_whitespace();
                let flags = parts.next()?;
                (flags.starts_with('V') && flags.len() == 6)
                    .then(|| parts.next().map(str::to_string))
                    .flatten()
            })
            .collect();
        Ok(Ffmpeg {
            path: path.to_path_buf(),
            gpl: version_text.contains("--enable-gpl"),
            version,
            encoders,
        })
    }

    /// Looks for `ffmpeg`: `OPENDESIGNER_FFMPEG`, next to the app (and in a macOS bundle's
    /// `Resources`), then `PATH`.
    pub fn locate() -> Result<Ffmpeg, MediaError> {
        let name = if cfg!(windows) {
            "ffmpeg.exe"
        } else {
            "ffmpeg"
        };
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Some(p) = std::env::var_os(FFMPEG_ENV) {
            // an explicit choice that does not work is an error, not a reason to look elsewhere
            return Ffmpeg::probe(Path::new(&p));
        }
        if let Some(dir) = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
        {
            candidates.push(dir.join(name));
            candidates.push(dir.join("../Resources").join(name));
        }
        if let Some(path) = std::env::var_os("PATH") {
            candidates.extend(std::env::split_paths(&path).map(|d| d.join(name)));
        }
        candidates
            .into_iter()
            .filter(|c| c.is_file())
            .find_map(|c| Ffmpeg::probe(&c).ok())
            .ok_or(MediaError::NotFound)
    }

    pub fn has_encoder(&self, name: &str) -> bool {
        self.encoders.contains(name)
    }

    /// The best H.264 encoder this ffmpeg has.
    pub fn h264_encoder(&self) -> Result<&'static str, MediaError> {
        H264_ENCODERS
            .iter()
            .copied()
            .find(|e| self.has_encoder(e))
            .ok_or_else(|| MediaError::NoEncoder(self.version.clone()))
    }
}

/// What to encode.
#[derive(Debug, Clone)]
pub struct EncodeSettings {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    /// Where the MP4 goes. It is written in full or not at all: encoding goes to a `.part` file
    /// next to it, renamed when `ffmpeg` finishes cleanly.
    pub output: PathBuf,
}

/// Bits per second for a picture of `width` x `height` at `fps`: about a tenth of a bit per pixel
/// per frame, never under 2 Mbit/s. Flat design frames compress well, so this is generous.
pub fn bitrate(width: u32, height: u32, fps: u32) -> u64 {
    (u64::from(width) * u64::from(height) * u64::from(fps) / 10).max(2_000_000)
}

/// A running encode: write each frame's RGBA pixels in order, then `finish`.
pub struct VideoWriter {
    child: Child,
    stdin: Option<ChildStdin>,
    errors: Option<JoinHandle<String>>,
    part: PathBuf,
    output: PathBuf,
    frame_bytes: usize,
    pub encoder: &'static str,
}

impl VideoWriter {
    pub fn start(ffmpeg: &Ffmpeg, settings: &EncodeSettings) -> Result<VideoWriter, MediaError> {
        let encoder = ffmpeg.h264_encoder()?;
        let part = {
            let mut name = settings
                .output
                .file_name()
                .unwrap_or_default()
                .to_os_string();
            name.push(".part");
            settings.output.with_file_name(name)
        };
        let mut command = Command::new(&ffmpeg.path);
        command
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgba"])
            .args(["-s", &format!("{}x{}", settings.width, settings.height)])
            .args(["-r", &settings.fps.to_string(), "-i", "-", "-an"])
            .args(["-c:v", encoder])
            .args([
                "-b:v",
                &bitrate(settings.width, settings.height, settings.fps).to_string(),
            ])
            .args([
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ])
            .arg(&part)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| MediaError::Io(format!("cannot start {}: {e}", ffmpeg.path.display())))?;
        let stdin = child.stdin.take();
        let errors = child.stderr.take().map(|mut s| {
            std::thread::spawn(move || {
                let mut text = String::new();
                let _ = s.read_to_string(&mut text);
                text
            })
        });
        Ok(VideoWriter {
            child,
            stdin,
            errors,
            part,
            output: settings.output.clone(),
            frame_bytes: settings.width as usize * settings.height as usize * 4,
            encoder,
        })
    }

    /// Sends one frame of `width * height * 4` bytes.
    pub fn write_frame(&mut self, rgba: &[u8]) -> Result<(), MediaError> {
        if rgba.len() != self.frame_bytes {
            return Err(MediaError::Io(format!(
                "a frame is {} bytes, expected {}",
                rgba.len(),
                self.frame_bytes
            )));
        }
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| MediaError::Io("the encoder is closed".into()))?;
        stdin.write_all(rgba).map_err(|e| self.fail(e.to_string()))
    }

    fn fail(&mut self, io: String) -> MediaError {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
        let said = self
            .errors
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        let _ = std::fs::remove_file(&self.part);
        MediaError::Failed(if said.trim().is_empty() {
            io
        } else {
            said.trim().to_string()
        })
    }

    /// Ends the input, waits for `ffmpeg` and moves the finished file into place.
    pub fn finish(mut self) -> Result<u64, MediaError> {
        drop(self.stdin.take());
        let status = self
            .child
            .wait()
            .map_err(|e| MediaError::Io(e.to_string()))?;
        let said = self
            .errors
            .take()
            .and_then(|h| h.join().ok())
            .unwrap_or_default();
        if !status.success() {
            let _ = std::fs::remove_file(&self.part);
            return Err(MediaError::Failed(format!("{status}: {}", said.trim())));
        }
        std::fs::rename(&self.part, &self.output).map_err(|e| MediaError::Io(e.to_string()))?;
        std::fs::metadata(&self.output)
            .map(|m| m.len())
            .map_err(|e| MediaError::Io(e.to_string()))
    }

    /// Stops the encode and removes what was written.
    pub fn abort(mut self) {
        self.stdin = None;
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.part);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_scales_with_the_picture_and_has_a_floor() {
        assert_eq!(bitrate(100, 100, 10), 2_000_000);
        assert_eq!(bitrate(1920, 1080, 30), 6_220_800);
    }

    #[test]
    fn a_program_that_is_not_ffmpeg_is_refused_and_a_missing_one_is_not_found() {
        let err = Ffmpeg::probe(Path::new("/definitely/not/here")).unwrap_err();
        assert!(matches!(err, MediaError::NotFfmpeg { .. }), "{err:?}");
        #[cfg(unix)]
        {
            let err = Ffmpeg::probe(Path::new("/bin/echo")).unwrap_err();
            assert!(err.to_string().contains("version line"), "{err}");
        }
    }
}
