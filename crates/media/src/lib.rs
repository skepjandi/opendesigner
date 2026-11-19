//! Encoding video by driving the FFmpeg program (`docs/wiki/video-editing.md#ffmpeg`).
//!
//! The editor draws every frame itself and pipes the raw pixels to `ffmpeg`, which compresses
//! them to an H.264 MP4 with the platform's own encoder where there is one (VideoToolbox on macOS,
//! Media Foundation on Windows, and so on), so no GPL encoder library is needed. FFmpeg runs as a
//! separate program: nothing is linked into this process, and the app never sends media anywhere.

use std::collections::{HashMap, HashSet};
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
                (matches!(flags.chars().next(), Some('V' | 'A')) && flags.len() == 6)
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

/// What `ffmpeg` says about a video file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileInfo {
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
}

/// Reads the facts out of the text `ffmpeg -i <file>` prints.
pub fn parse_info(text: &str) -> Option<FileInfo> {
    let duration_ms = text.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("Duration: ")?;
        let t = rest.split(',').next()?;
        let mut parts = t.split(':');
        let (h, m, sec) = (parts.next()?, parts.next()?, parts.next()?);
        let secs: f64 = h.parse::<f64>().ok()? * 3600.0
            + m.parse::<f64>().ok()? * 60.0
            + sec.parse::<f64>().ok()?;
        Some((secs * 1000.0).round() as u64)
    })?;
    let video = text.lines().find(|l| l.contains(" Video: "))?;
    let size = video
        .split([',', ' '])
        .filter_map(|w| {
            let (a, b) = w.split_once('x')?;
            Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?))
        })
        .find(|(w, h)| *w >= 16 && *h >= 16)?;
    let fps = video
        .split(',')
        .find_map(|part| part.trim().strip_suffix(" fps")?.parse::<f64>().ok())
        .unwrap_or(30.0);
    Some(FileInfo {
        duration_ms,
        width: size.0,
        height: size.1,
        fps,
        has_audio: text.lines().any(|l| l.contains(" Audio: ")),
    })
}

impl Ffmpeg {
    /// Looks at a video file: its length, picture size, frame rate and whether it has sound.
    pub fn probe_file(&self, file: &Path) -> Result<FileInfo, MediaError> {
        let out = Command::new(&self.path)
            .args(["-hide_banner", "-i"])
            .arg(file)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| MediaError::Io(e.to_string()))?;
        // ffmpeg exits with an error when given no output; the facts are in its log
        let text = String::from_utf8_lossy(&out.stderr);
        parse_info(&text).ok_or_else(|| {
            MediaError::Failed(format!(
                "{} is not a video ffmpeg can read: {}",
                file.display(),
                text.lines().last().unwrap_or("").trim()
            ))
        })
    }
}

struct Reader {
    child: Child,
    next_ms: f64,
    last: Vec<u8>,
    done: bool,
}

/// Decodes footage a picture at a time. Playing forward (what an export does) keeps one
/// `ffmpeg` per file running; asking for another moment starts it again there.
pub struct FootageDecoder {
    ffmpeg: Ffmpeg,
    /// The folder the `media/<name>` references are relative to.
    project: PathBuf,
    fps: u32,
    readers: HashMap<(String, u32, u32), Reader>,
}

const MAX_READERS: usize = 4;

impl FootageDecoder {
    pub fn new(ffmpeg: Ffmpeg, project: PathBuf, fps: u32) -> FootageDecoder {
        FootageDecoder {
            ffmpeg,
            project,
            fps: fps.max(1),
            readers: HashMap::new(),
        }
    }

    fn start(&self, path: &Path, at_ms: u64, w: u32, h: u32) -> Result<Reader, MediaError> {
        let filter = format!(
            "fps={},scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}",
            self.fps
        );
        let child = Command::new(&self.ffmpeg.path)
            .args(["-hide_banner", "-loglevel", "error", "-ss"])
            .arg(format!("{:.3}", at_ms as f64 / 1000.0))
            .arg("-i")
            .arg(path)
            .args([
                "-an", "-vf", &filter, "-f", "rawvideo", "-pix_fmt", "rgba", "-",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| MediaError::Io(format!("cannot start ffmpeg: {e}")))?;
        Ok(Reader {
            child,
            next_ms: at_ms as f64,
            last: Vec::new(),
            done: false,
        })
    }

    /// The RGBA picture of `file` (`media/<name>`) at `at_ms`, scaled to cover `w` x `h` and
    /// cropped in the middle. Past the end it is the last picture.
    pub fn frame(&mut self, file: &str, at_ms: u64, w: u32, h: u32) -> Result<Vec<u8>, MediaError> {
        let path = footage_path(&self.project, file)?;
        let key = (file.to_string(), w, h);
        let period = 1000.0 / f64::from(self.fps);
        let fits = self
            .readers
            .get(&key)
            .is_some_and(|r| (at_ms as f64 - r.next_ms).abs() <= period * 0.75 + 1.0);
        if !fits {
            if let Some(mut old) = self.readers.remove(&key) {
                let _ = old.child.kill();
                let _ = old.child.wait();
            }
            if self.readers.len() >= MAX_READERS {
                if let Some(k) = self.readers.keys().next().cloned() {
                    if let Some(mut old) = self.readers.remove(&k) {
                        let _ = old.child.kill();
                        let _ = old.child.wait();
                    }
                }
            }
            let reader = self.start(&path, at_ms, w, h)?;
            self.readers.insert(key.clone(), reader);
        }
        let size = w as usize * h as usize * 4;
        let reader = self
            .readers
            .get_mut(&key)
            .ok_or_else(|| MediaError::Io("no reader".into()))?;
        reader.next_ms += period;
        if reader.done {
            return Ok(reader.last.clone());
        }
        let mut buf = vec![0u8; size];
        let got = match reader.child.stdout.as_mut() {
            Some(out) => read_full(out, &mut buf).map_err(|e| MediaError::Io(e.to_string()))?,
            None => 0,
        };
        if got == size {
            reader.last = buf.clone();
            Ok(buf)
        } else {
            reader.done = true;
            if reader.last.is_empty() {
                return Err(MediaError::Failed(format!(
                    "{file} has no picture at {at_ms} ms (is that past its end?)"
                )));
            }
            Ok(reader.last.clone())
        }
    }
}

impl Drop for FootageDecoder {
    fn drop(&mut self) {
        for (_, mut r) in self.readers.drain() {
            let _ = r.child.kill();
            let _ = r.child.wait();
        }
    }
}

fn read_full(r: &mut impl Read, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut got = 0;
    while got < buf.len() {
        match r.read(&mut buf[got..])? {
            0 => break,
            n => got += n,
        }
    }
    Ok(got)
}

/// Where `media/<name>` is inside `project`: a plain file name only, never through a symlink.
pub fn footage_path(project: &Path, reference: &str) -> Result<PathBuf, MediaError> {
    let name = reference
        .strip_prefix("media/")
        .filter(|n| !n.is_empty() && !n.contains(['/', '\\']) && !n.starts_with('.'))
        .ok_or_else(|| MediaError::Io(format!("{reference:?} is not a media/<file> reference")))?;
    let path = project.join("media").join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() => Ok(path),
        Ok(_) => Err(MediaError::Io(format!("{reference} is not a plain file"))),
        Err(e) => Err(MediaError::Io(format!("{reference}: {e}"))),
    }
}

/// One piece of footage sound on the finished timeline.
#[derive(Debug, Clone)]
pub struct AudioPiece {
    pub file: PathBuf,
    pub in_ms: u64,
    pub start_ms: u64,
    pub duration_ms: u64,
}

/// Puts the sound of `pieces` under the picture of `video` (copied, not re-encoded) and writes
/// `output`. Each piece is cut from its file, delayed to its place and mixed with the others.
pub fn add_audio(
    ffmpeg: &Ffmpeg,
    video: &Path,
    pieces: &[AudioPiece],
    output: &Path,
) -> Result<(), MediaError> {
    let mut cmd = Command::new(&ffmpeg.path);
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(video);
    let mut filter = String::new();
    for (n, p) in pieces.iter().enumerate() {
        cmd.arg("-i").arg(&p.file);
        filter.push_str(&format!(
            "[{i}:a]atrim=start={s:.3}:duration={d:.3},asetpts=PTS-STARTPTS,adelay={delay}:all=1[a{n}];",
            i = n + 1,
            s = p.in_ms as f64 / 1000.0,
            d = p.duration_ms as f64 / 1000.0,
            delay = p.start_ms
        ));
    }
    let labels: String = (0..pieces.len()).map(|n| format!("[a{n}]")).collect();
    filter.push_str(&format!(
        "{labels}amix=inputs={}:normalize=0:duration=longest[a]",
        pieces.len()
    ));
    let out = cmd
        .args(["-filter_complex", &filter, "-map", "0:v", "-map", "[a]"])
        .args(["-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest"])
        .args(["-movflags", "+faststart", "-f", "mp4"])
        .arg(output)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| MediaError::Io(e.to_string()))?;
    if out.status.success() {
        Ok(())
    } else {
        let _ = std::fs::remove_file(output);
        Err(MediaError::Failed(
            String::from_utf8_lossy(&out.stderr).trim().to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpegs_log_is_read_for_length_size_rate_and_sound() {
        let log = "Input #0, mov,mp4, from 'a.mp4':\n  Duration: 00:01:02.50, start: 0.000000, bitrate: 1200 kb/s\n  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709, progressive), 1920x1080 [SAR 1:1 DAR 16:9], 1100 kb/s, 29.97 fps, 29.97 tbr, 30k tbn (default)\n  Stream #0:1[0x2](und): Audio: aac (LC), 48000 Hz, stereo, fltp, 128 kb/s\n";
        let info = parse_info(log).unwrap();
        assert_eq!(
            (info.duration_ms, info.width, info.height),
            (62_500, 1920, 1080)
        );
        assert!((info.fps - 29.97).abs() < 1e-9 && info.has_audio);
        assert!(parse_info("a.txt: Invalid data found when processing input").is_none());
        assert!(
            !parse_info(&log.replace(" Audio: ", " Data: "))
                .unwrap()
                .has_audio
        );
    }

    #[test]
    fn footage_paths_are_plain_media_files_only() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("media")).unwrap();
        std::fs::write(d.path().join("media/a.mp4"), b"x").unwrap();
        assert!(footage_path(d.path(), "media/a.mp4").is_ok());
        for bad in [
            "a.mp4",
            "media/../a.mp4",
            "media/",
            "media/.a.mp4",
            "media/x/a.mp4",
            "media/missing.mp4",
        ] {
            assert!(footage_path(d.path(), bad).is_err(), "{bad}");
        }
    }

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
