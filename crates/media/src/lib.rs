//! Encoding video by driving the FFmpeg program (`docs/wiki/video-editing.md#ffmpeg`).
//!
//! The editor draws every frame itself and pipes the raw pixels to `ffmpeg`, which compresses
//! them to an H.264 MP4 with the platform's own encoder where there is one (VideoToolbox on macOS,
//! Media Foundation on Windows, and so on), so no GPL encoder library is needed. FFmpeg runs as a
//! separate program: nothing is linked into this process, and the app never sends media anywhere.

pub mod retime;

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread::JoinHandle;

use thiserror::Error;

pub mod analysis;
pub mod cutout;
pub mod external;
pub mod mix;
pub mod speech;

pub use analysis::Pause;
pub use mix::{AudioPiece, DuckSettings, PieceRole};

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
/// How long a file lasts, from ffmpeg's log.
pub fn parse_duration(text: &str) -> Option<u64> {
    text.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("Duration: ")?;
        let t = rest.split(',').next()?;
        let mut parts = t.split(':');
        let (h, m, sec) = (parts.next()?, parts.next()?, parts.next()?);
        let secs: f64 = h.parse::<f64>().ok()? * 3600.0
            + m.parse::<f64>().ok()? * 60.0
            + sec.parse::<f64>().ok()?;
        Some((secs * 1000.0).round() as u64)
    })
}

pub fn parse_info(text: &str) -> Option<FileInfo> {
    let duration_ms = parse_duration(text)?;
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

    /// How long a file lasts, in milliseconds. For an audio file this is the sound alone.
    pub fn probe_duration(&self, file: &Path) -> Result<u64, MediaError> {
        let out = Command::new(&self.path)
            .args(["-hide_banner", "-i"])
            .arg(file)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| MediaError::Io(e.to_string()))?;
        let text = String::from_utf8_lossy(&out.stderr);
        parse_duration(&text).ok_or_else(|| {
            MediaError::Failed(format!(
                "{} is not a file ffmpeg can read: {}",
                file.display(),
                text.lines().last().unwrap_or("").trim()
            ))
        })
    }
}

struct Reader {
    child: Child,
    /// The tick of the next picture the reader will read.
    next_tick: u64,
    last: Vec<u8>,
    done: bool,
}

/// Decodes footage a picture at a time on the project's frame grid. A moment is the picture
/// that file shows at the nearest tick (a whole number of `1 / fps` seconds from the start of the
/// file), and that picture is the same however the reader got there. Playing forward keeps one
/// `ffmpeg` per file running; anything else starts it again at the moment asked for.
pub struct FootageDecoder {
    ffmpeg: Ffmpeg,
    /// The folder the `media/<name>` references are relative to.
    project: PathBuf,
    fps: u32,
    readers: HashMap<(String, u32, u32), Reader>,
}

const MAX_READERS: usize = 4;
/// Pictures read and thrown away to reach a later moment before starting `ffmpeg` there instead.
const MAX_SKIPPED_FRAMES: u64 = 90;
/// How far before a tick `ffmpeg` starts decoding, so the picture shown at it is among those
/// decoded (a picture is shown until the next one, which may be seconds in a slow file).
const SEEK_MARGIN_S: f64 = 2.0;

impl FootageDecoder {
    pub fn new(ffmpeg: Ffmpeg, project: PathBuf, fps: u32) -> FootageDecoder {
        FootageDecoder {
            ffmpeg,
            project,
            fps: fps.max(1),
            readers: HashMap::new(),
        }
    }

    fn tick_of(&self, at_ms: u64) -> u64 {
        (at_ms * u64::from(self.fps) + 500) / 1000
    }

    /// Starts `ffmpeg` writing the pictures shown at ticks `tick`, `tick + 1`, ...: the file's
    /// pictures resampled to `fps` with each tick showing the last picture at or before it,
    /// those before `tick` dropped before they are scaled.
    fn start(&self, path: &Path, tick: u64, w: u32, h: u32) -> Result<Reader, MediaError> {
        let at = tick as f64 / f64::from(self.fps);
        let filter = format!(
            "fps={}:round=up,select=gte(t\\,{:.6}),scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h}",
            self.fps,
            at - 0.0005
        );
        let child = Command::new(&self.ffmpeg.path)
            .args(["-hide_banner", "-loglevel", "error", "-ss"])
            .arg(format!("{:.6}", (at - SEEK_MARGIN_S).max(0.0)))
            .args(["-copyts", "-i"])
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
            next_tick: tick,
            last: Vec::new(),
            done: false,
        })
    }

    /// How many pictures the reader must read to reach `tick`: none to repeat the last one it
    /// gave (slow motion, or the same moment twice), one for the next, more to skip some (fast
    /// motion). `None` when `tick` is before the last picture or too far ahead for reading to
    /// beat starting again.
    fn steps(r: &Reader, tick: u64) -> Option<u64> {
        let steps = (tick + 1).checked_sub(r.next_tick)?;
        (steps <= MAX_SKIPPED_FRAMES).then_some(steps)
    }

    fn discard(&mut self, key: &(String, u32, u32)) {
        if let Some(mut old) = self.readers.remove(key) {
            let _ = old.child.kill();
            let _ = old.child.wait();
        }
    }

    /// The RGBA picture of `file` (`media/<name>`) at `at_ms`, scaled to cover `w` x `h` and
    /// cropped in the middle. Past the end it is the last picture. A clip played faster skips
    /// pictures and one played slower repeats them, all from one running `ffmpeg`.
    pub fn frame(&mut self, file: &str, at_ms: u64, w: u32, h: u32) -> Result<Vec<u8>, MediaError> {
        let path = footage_path(&self.project, file)?;
        let key = (file.to_string(), w, h);
        let tick = self.tick_of(at_ms);
        let reuse = self.readers.get(&key).and_then(|r| Self::steps(r, tick));
        let steps = match reuse {
            Some(steps) => steps,
            None => {
                self.discard(&key);
                if self.readers.len() >= MAX_READERS {
                    if let Some(k) = self.readers.keys().next().cloned() {
                        self.discard(&k);
                    }
                }
                let reader = self.start(&path, tick, w, h)?;
                self.readers.insert(key.clone(), reader);
                1
            }
        };
        let size = w as usize * h as usize * 4;
        let reader = self
            .readers
            .get_mut(&key)
            .ok_or_else(|| MediaError::Io("no reader".into()))?;
        reader.next_tick += steps;
        let mut buf = Vec::new();
        for _ in 0..steps {
            if reader.done {
                break;
            }
            buf.resize(size, 0);
            let got = match reader.child.stdout.as_mut() {
                Some(out) => read_full(out, &mut buf).map_err(|e| MediaError::Io(e.to_string()))?,
                None => 0,
            };
            if got == size {
                std::mem::swap(&mut buf, &mut reader.last);
            } else {
                reader.done = true;
            }
        }
        if reader.last.is_empty() {
            return Err(MediaError::Failed(format!(
                "{file} has no picture at {at_ms} ms (is that past its end?)"
            )));
        }
        Ok(reader.last.clone())
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

/// Where an audio file of the project is: a plain file name directly inside `media/`. The name
/// itself is checked by `core`, which is where the rule lives.
pub fn audio_path(project: &Path, reference: &str) -> Result<PathBuf, MediaError> {
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
    for p in pieces {
        cmd.arg("-i").arg(&p.file);
    }
    let filter = mix::filter_graph(pieces);
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

/// Mixes `pieces` into one stereo 44.1 kHz 16-bit WAV at `output`, `duration_ms` long: the sound
/// the editor plays under its preview. The same graph as [`add_audio`], so
/// what is heard is what exports. Input 0 is silence, which keeps the graph's numbering.
pub fn mix_wav(
    ffmpeg: &Ffmpeg,
    pieces: &[AudioPiece],
    duration_ms: u64,
    output: &Path,
) -> Result<(), MediaError> {
    let seconds = format!("{:.3}", duration_ms as f64 / 1000.0);
    let mut cmd = Command::new(&ffmpeg.path);
    cmd.args([
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-t",
    ])
    .arg(&seconds)
    .args(["-i", "anullsrc=r=44100:cl=stereo"]);
    for p in pieces {
        cmd.arg("-i").arg(&p.file);
    }
    let filter = mix::filter_graph(pieces);
    let out = cmd
        .args(["-filter_complex", &filter, "-map", "[a]", "-t"])
        .arg(&seconds)
        .args(["-ar", "44100", "-ac", "2", "-c:a", "pcm_s16le", "-f", "wav"])
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

#[cfg(test)]
mod decoder_tests {
    use super::*;

    fn footage(ffmpeg: &Ffmpeg, dir: &Path, rate: u32) -> bool {
        std::fs::create_dir_all(dir.join("media")).unwrap();
        Command::new(&ffmpeg.path)
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i"])
            .arg(format!("testsrc=size=64x48:rate={rate}:duration=3"))
            .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "12"])
            .arg(dir.join("media/a.mp4"))
            .status()
            .is_ok_and(|s| s.success())
    }

    /// The moment of frame `n` at `fps`, as the renderer asks for it.
    fn at(n: u64, fps: u64) -> u64 {
        (n * 1000 + fps / 2) / fps
    }

    #[test]
    fn a_picture_does_not_depend_on_where_the_reader_started() {
        let Ok(ffmpeg) = Ffmpeg::locate() else {
            return;
        };
        for (source_rate, fps) in [(30, 30), (24, 30), (30, 24), (25, 25)] {
            let dir = tempfile::tempdir().unwrap();
            if !footage(&ffmpeg, dir.path(), source_rate) {
                return;
            }
            let moments: Vec<u64> = (0..60).map(|n| 400 + at(n, fps)).collect();
            let mut straight =
                FootageDecoder::new(ffmpeg.clone(), dir.path().to_path_buf(), fps as u32);
            let reference: Vec<Vec<u8>> = moments
                .iter()
                .map(|&t| straight.frame("media/a.mp4", t, 64, 48).unwrap())
                .collect();
            for start in [3, 7, 16, 41] {
                let mut fresh =
                    FootageDecoder::new(ffmpeg.clone(), dir.path().to_path_buf(), fps as u32);
                for n in start..(start + 4).min(60) {
                    let got = fresh.frame("media/a.mp4", moments[n], 64, 48).unwrap();
                    assert!(
                        got == reference[n],
                        "{source_rate} fps file at {fps} fps: moment {n} differs when started at {start}"
                    );
                }
            }
        }
    }

    #[test]
    fn faster_playback_skips_pictures_and_slower_repeats_them_with_one_reader() {
        let Ok(ffmpeg) = Ffmpeg::locate() else {
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        if !footage(&ffmpeg, dir.path(), 30) {
            return;
        }
        let mut one = FootageDecoder::new(ffmpeg.clone(), dir.path().to_path_buf(), 30);
        let seek = |t| {
            let mut fresh = FootageDecoder::new(ffmpeg.clone(), dir.path().to_path_buf(), 30);
            fresh.frame("media/a.mp4", t, 64, 48).unwrap()
        };
        // twice as fast: every other picture, read by one running ffmpeg
        for n in 0..10 {
            let t = 500 + at(n, 15);
            assert!(
                one.frame("media/a.mp4", t, 64, 48).unwrap() == seek(t),
                "fast {n}"
            );
        }
        // half speed: each picture twice
        let mut slow = FootageDecoder::new(ffmpeg.clone(), dir.path().to_path_buf(), 30);
        let a = slow.frame("media/a.mp4", 500, 64, 48).unwrap();
        let b = slow.frame("media/a.mp4", 508, 64, 48).unwrap();
        let c = slow.frame("media/a.mp4", 533, 64, 48).unwrap();
        assert!(a == b && a != c);
        assert!(c == seek(533));
    }

    #[test]
    fn pieces_mix_into_a_wav_of_the_videos_length_when_ffmpeg_is_there() {
        let Ok(ff) = Ffmpeg::locate() else {
            eprintln!("skipped: no ffmpeg on this machine");
            return;
        };
        let tmp = tempfile::tempdir().unwrap();
        let tone = tmp.path().join("tone.wav");
        let made = Command::new(&ff.path)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi"])
            .args(["-i", "sine=frequency=440:duration=1"])
            .arg(&tone)
            .output()
            .unwrap();
        assert!(made.status.success());
        let piece = AudioPiece {
            file: tone,
            role: PieceRole::Music,
            pan: 0.0,
            duck: None,
            in_ms: 0,
            start_ms: 500,
            duration_ms: 1000,
            gain: 0.5,
            fade_in_ms: 100,
            fade_out_ms: 100,
        };
        let wav = tmp.path().join("mix.wav");
        mix_wav(&ff, std::slice::from_ref(&piece), 2000, &wav).unwrap();
        let bytes = std::fs::read(&wav).unwrap();
        assert_eq!(&bytes[..4], b"RIFF", "a wav file");
        assert_eq!(&bytes[8..12], b"WAVE");
        // 44 bytes of header, 44100 samples a second, two channels of 16 bits
        let samples = (bytes.len() - 44) as f64 / (44100.0 * 4.0);
        assert!((1.9..=2.1).contains(&samples), "{samples}");
        let missing = AudioPiece {
            file: tmp.path().join("none.wav"),
            ..piece
        };
        assert!(mix_wav(&ff, &[missing], 2000, &tmp.path().join("x.wav")).is_err());
        assert!(
            !tmp.path().join("x.wav").exists(),
            "a failed mix leaves no file"
        );
    }
}
