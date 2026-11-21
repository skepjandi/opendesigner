//! Programs the user installed besides FFmpeg (speech to text, background removal): how they are
//! found, run and reported (`docs/wiki/security.md#external-programs`).
//!
//! Every program runs as a separate child process, never through a shell. The caller builds the
//! argument list from values it has already validated. A run has a time limit, a cap on the
//! output it may print and a cancel flag, and the child is killed when any of them trips (or when
//! the run is dropped). Nothing here uses the network; a program may do so on its own (to fetch a
//! model it was set up with), which is the user's choice when they installed it.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use thiserror::Error;

use crate::{FFMPEG_ENV, Ffmpeg};

/// Names the `OPENDESIGNER_WHISPER` environment variable gives a specific speech-to-text program.
pub const WHISPER_ENV: &str = "OPENDESIGNER_WHISPER";
/// Names the `OPENDESIGNER_WHISPER_MODEL` environment variable gives a model file (or, for the
/// Python `whisper`, a model name).
pub const WHISPER_MODEL_ENV: &str = "OPENDESIGNER_WHISPER_MODEL";
/// Names the `OPENDESIGNER_REMBG` environment variable gives a specific `rembg`.
pub const REMBG_ENV: &str = "OPENDESIGNER_REMBG";

/// How long a version question may take (the Python `whisper` imports a lot before it answers).
const VERSION_TIMEOUT: Duration = Duration::from_secs(15);
const VERSION_OUTPUT_BYTES: usize = 16 * 1024;
const VERSION_CHARS: usize = 160;
const POLL: Duration = Duration::from_millis(10);

#[derive(Debug, Error)]
pub enum ExternalError {
    #[error("{tool} was not found. {hint}")]
    NotFound {
        tool: &'static str,
        hint: &'static str,
    },
    #[error("{env} is set to {path}, which is not an executable file")]
    BadOverride { env: String, path: String },
    #[error("{program} did not finish within {secs} s and was stopped")]
    Timeout { program: String, secs: u64 },
    #[error("{program} printed more than {limit} bytes and was stopped")]
    OutputTooLarge { program: String, limit: usize },
    #[error("{program} was cancelled")]
    Cancelled { program: String },
    #[error("{0}")]
    Failed(String),
}

/// Where a program is looked for, in this order: the override, next to the app, `PATH`.
#[derive(Debug, Clone, Default)]
pub struct Search {
    /// The value of the program's override variable.
    pub explicit: Option<OsString>,
    pub app_dirs: Vec<PathBuf>,
    pub path_dirs: Vec<PathBuf>,
}

impl Search {
    /// The search the running process would make, honoring `override_env`.
    pub fn from_env(override_env: &str) -> Search {
        let app_dirs = std::env::current_exe()
            .ok()
            .and_then(|e| e.parent().map(Path::to_path_buf))
            .map(|d| vec![d.join("../Resources"), d])
            .unwrap_or_default();
        Search {
            explicit: std::env::var_os(override_env).filter(|v| !v.is_empty()),
            app_dirs,
            path_dirs: std::env::var_os("PATH")
                .map(|p| std::env::split_paths(&p).collect())
                .unwrap_or_default(),
        }
    }

    /// The first program of `names` (a name is tried in every place before the next name) that exists. An
    /// override that does not work is an error, not a reason to look elsewhere.
    pub fn find(&self, env: &str, names: &[&str]) -> Result<Option<PathBuf>, ExternalError> {
        if let Some(p) = &self.explicit {
            return if is_executable(Path::new(p)) {
                Ok(Some(PathBuf::from(p)))
            } else {
                Err(ExternalError::BadOverride {
                    env: env.to_string(),
                    path: Path::new(p).display().to_string(),
                })
            };
        }
        let dirs: Vec<&PathBuf> = self.app_dirs.iter().chain(&self.path_dirs).collect();
        Ok(names
            .iter()
            .flat_map(|n| dirs.iter().map(move |d| d.join(exe_name(n))))
            .find(|c| is_executable(c)))
    }
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// The two speech-to-text programs that are supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WhisperFlavor {
    /// whisper.cpp's `whisper-cli`: needs a model file.
    Cpp,
    /// OpenAI's Python `whisper`: takes a model name.
    OpenAi,
}

impl WhisperFlavor {
    pub fn name(self) -> &'static str {
        match self {
            WhisperFlavor::Cpp => "whisper-cli",
            WhisperFlavor::OpenAi => "whisper",
        }
    }
}

/// The names looked for, in the order they win: whisper.cpp first.
pub const WHISPER_NAMES: [&str; 3] = ["whisper-cli", "whisper-cpp", "whisper"];

pub const WHISPER_HINT: &str = "Install whisper.cpp (`brew install whisper-cpp`, which gives `whisper-cli`, plus a model file: set OPENDESIGNER_WHISPER_MODEL to it) or OpenAI's whisper (`pip install openai-whisper`), or set OPENDESIGNER_WHISPER to the program's path.";
pub const REMBG_HINT: &str = "Install rembg (`pip install \"rembg[cli]\"` or `pipx install \"rembg[cli]\"`) or set OPENDESIGNER_REMBG to its path.";
pub const FFMPEG_HINT: &str = "Install FFmpeg (`brew install ffmpeg`, `apt install ffmpeg` or `winget install ffmpeg`) or set OPENDESIGNER_FFMPEG to its path.";

/// A speech-to-text program that was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Whisper {
    pub path: PathBuf,
    pub flavor: WhisperFlavor,
}

/// Which program `path` is: a file named exactly `whisper` is the Python one, anything else
/// (`whisper-cli`, `whisper-cpp`, `main`, ...) is whisper.cpp.
pub fn flavor_of(path: &Path) -> WhisperFlavor {
    let stem = path.file_stem().and_then(OsStr::to_str).unwrap_or("");
    if stem.eq_ignore_ascii_case("whisper") {
        WhisperFlavor::OpenAi
    } else {
        WhisperFlavor::Cpp
    }
}

impl Whisper {
    pub fn locate() -> Result<Whisper, ExternalError> {
        Whisper::locate_in(&Search::from_env(WHISPER_ENV))
    }

    pub fn locate_in(search: &Search) -> Result<Whisper, ExternalError> {
        search
            .find(WHISPER_ENV, &WHISPER_NAMES)?
            .map(|path| Whisper {
                flavor: flavor_of(&path),
                path,
            })
            .ok_or(ExternalError::NotFound {
                tool: "a speech-to-text program",
                hint: WHISPER_HINT,
            })
    }
}

/// The `rembg` program.
pub fn locate_rembg() -> Result<PathBuf, ExternalError> {
    locate_rembg_in(&Search::from_env(REMBG_ENV))
}

pub fn locate_rembg_in(search: &Search) -> Result<PathBuf, ExternalError> {
    search
        .find(REMBG_ENV, &["rembg"])?
        .ok_or(ExternalError::NotFound {
            tool: "rembg",
            hint: REMBG_HINT,
        })
}

/// What one run may use up.
#[derive(Clone, Copy)]
pub struct Limits<'a> {
    pub timeout: Duration,
    /// More standard output than this stops the run with an error.
    pub max_stdout: usize,
    /// Standard error is kept up to this many bytes; the rest is dropped.
    pub max_stderr: usize,
    /// Setting the flag stops the run.
    pub cancel: Option<&'a AtomicBool>,
}

/// What a finished run printed.
#[derive(Debug)]
pub struct Captured {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Captured {
    /// The last non-empty line of the error output, for a message.
    pub fn last_error_line(&self) -> String {
        String::from_utf8_lossy(&self.stderr)
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("no message")
            .trim()
            .chars()
            .take(300)
            .collect()
    }
}

/// Kills the child when dropped, so no path out of a run leaves it running.
struct Guard(Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn drain(
    mut pipe: impl Read + Send + 'static,
    cap: usize,
    strict: bool,
    exceeded: Arc<AtomicBool>,
) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let (mut kept, mut chunk) = (Vec::new(), [0u8; 8192]);
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = cap.saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                    if n > room && strict {
                        exceeded.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
        kept
    })
}

fn display(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// Runs `path` with `args`: no shell, no input, killed on timeout, output over the cap or cancel.
pub fn run(path: &Path, args: &[OsString], limits: &Limits) -> Result<Captured, ExternalError> {
    let program = display(path);
    let mut child = Command::new(path)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ExternalError::Failed(format!("cannot start {}: {e}", path.display())))?;
    let exceeded = Arc::new(AtomicBool::new(false));
    let out = child
        .stdout
        .take()
        .map(|p| drain(p, limits.max_stdout, true, exceeded.clone()));
    let err = child
        .stderr
        .take()
        .map(|p| drain(p, limits.max_stderr, false, exceeded.clone()));
    let mut guard = Guard(child);
    let deadline = Instant::now() + limits.timeout;
    let status = loop {
        match guard.0.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(ExternalError::Failed(format!("{program}: {e}"))),
        }
        if limits.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
            return Err(ExternalError::Cancelled { program });
        }
        if exceeded.load(Ordering::Relaxed) {
            return Err(ExternalError::OutputTooLarge {
                program,
                limit: limits.max_stdout,
            });
        }
        if Instant::now() >= deadline {
            return Err(ExternalError::Timeout {
                program,
                secs: limits.timeout.as_secs(),
            });
        }
        std::thread::sleep(POLL);
    };
    let join = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    let (stdout, stderr) = (join(out), join(err));
    if exceeded.load(Ordering::Relaxed) {
        return Err(ExternalError::OutputTooLarge {
            program,
            limit: limits.max_stdout,
        });
    }
    Ok(Captured {
        success: status.success(),
        stdout,
        stderr,
    })
}

/// A scratch folder in the system's temp folder, removed when dropped. A program writes its
/// results here; only the checked result is copied into the project.
pub struct Scratch(PathBuf);

static SCRATCH_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Scratch {
    pub fn new() -> Result<Scratch, ExternalError> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!(
            "opendesigner-{}-{nanos}-{}",
            std::process::id(),
            SCRATCH_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder
            .create(&path)
            .map_err(|e| ExternalError::Failed(format!("cannot make {}: {e}", path.display())))?;
        Ok(Scratch(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The longest sound that is transcribed.
pub const MAX_SPEECH_MS: u64 = 2 * 60 * 60 * 1000;

impl Ffmpeg {
    /// Writes the sound of `input` (picture ignored) to `output` as 16 kHz mono 16-bit WAV, the
    /// form speech programs read, at most [`MAX_SPEECH_MS`] of it.
    pub fn extract_speech_wav(
        &self,
        input: &Path,
        output: &Path,
        timeout: Duration,
        cancel: Option<&AtomicBool>,
    ) -> Result<(), ExternalError> {
        let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-y", "-i"]
            .map(OsString::from)
            .into();
        args.push(input.into());
        args.extend(
            ["-vn", "-ac", "1", "-ar", "16000", "-c:a", "pcm_s16le", "-t"].map(OsString::from),
        );
        args.push((MAX_SPEECH_MS / 1000).to_string().into());
        args.push(output.into());
        let limits = Limits {
            timeout,
            max_stdout: VERSION_OUTPUT_BYTES,
            max_stderr: VERSION_OUTPUT_BYTES,
            cancel,
        };
        let done = run(&self.path, &args, &limits)?;
        if done.success && output.is_file() {
            Ok(())
        } else {
            Err(ExternalError::Failed(format!(
                "{} has no sound ffmpeg can read: {}",
                display(input),
                done.last_error_line()
            )))
        }
    }
}

/// One program in the capabilities report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramStatus {
    /// `ffmpeg`, `whisper-cli`, `whisper` or `rembg`.
    pub name: &'static str,
    pub env: &'static str,
    /// What it is for.
    pub purpose: &'static str,
    pub found: Option<PathBuf>,
    /// The first line it printed for `--version`; empty when it printed none.
    pub version: String,
    /// What the user installs when it is missing.
    pub install_hint: &'static str,
    /// Why the program set in `env` cannot be used.
    pub problem: Option<String>,
}

/// What the user's machine has, for `get_capabilities` and `opendesigner doctor`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    pub programs: Vec<ProgramStatus>,
    /// The speech model the user configured, when it is a file that exists (or a model name).
    pub whisper_model: Option<String>,
}

impl Capabilities {
    pub fn program(&self, name: &str) -> Option<&ProgramStatus> {
        self.programs.iter().find(|p| p.name == name)
    }

    /// The programs that are missing, for a one-line note.
    pub fn missing(&self) -> Vec<&'static str> {
        let has = |n| self.program(n).is_some_and(|p| p.found.is_some());
        let mut out = Vec::new();
        if !has("ffmpeg") {
            out.push("ffmpeg");
        }
        if !has("whisper-cli") && !has("whisper") {
            out.push("a speech-to-text program (whisper-cli or whisper)");
        }
        if !has("rembg") {
            out.push("rembg");
        }
        out
    }
}

/// Asks `path` for its version: the first line of what `flag` prints, within a few seconds.
pub fn version_of(path: &Path, flag: &str) -> String {
    let limits = Limits {
        timeout: VERSION_TIMEOUT,
        max_stdout: VERSION_OUTPUT_BYTES,
        max_stderr: VERSION_OUTPUT_BYTES,
        cancel: None,
    };
    let Ok(done) = run(path, &[flag.into()], &limits) else {
        return String::new();
    };
    [&done.stdout, &done.stderr]
        .iter()
        .find_map(|b| {
            String::from_utf8_lossy(b)
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.chars().take(VERSION_CHARS).collect::<String>())
        })
        .unwrap_or_default()
}

fn status(
    name: &'static str,
    env: &'static str,
    purpose: &'static str,
    hint: &'static str,
    found: Result<Option<PathBuf>, ExternalError>,
    flag: Option<&str>,
) -> ProgramStatus {
    let (found, problem) = match found {
        Ok(f) => (f, None),
        Err(e) => (None, Some(e.to_string())),
    };
    ProgramStatus {
        version: found
            .as_deref()
            .zip(flag)
            .map(|(p, flag)| version_of(p, flag))
            .unwrap_or_default(),
        name,
        env,
        purpose,
        found,
        install_hint: hint,
        problem,
    }
}

impl Capabilities {
    /// What is installed, each program asked for its version.
    pub fn probe() -> Capabilities {
        Capabilities::probe_with(
            true,
            &Search::from_env(FFMPEG_ENV),
            &Search::from_env(WHISPER_ENV),
            &Search::from_env(REMBG_ENV),
            std::env::var_os(WHISPER_MODEL_ENV),
        )
    }

    /// What is installed, found without running anything: versions stay empty.
    pub fn probe_quick() -> Capabilities {
        Capabilities::probe_with(
            false,
            &Search::from_env(FFMPEG_ENV),
            &Search::from_env(WHISPER_ENV),
            &Search::from_env(REMBG_ENV),
            std::env::var_os(WHISPER_MODEL_ENV),
        )
    }

    /// The same with the places to look given, and `versions` saying whether to ask for them.
    pub fn probe_with(
        versions: bool,
        ffmpeg: &Search,
        whisper: &Search,
        rembg: &Search,
        model: Option<OsString>,
    ) -> Capabilities {
        let whisper_for = |wanted: WhisperFlavor| match whisper.find(WHISPER_ENV, &WHISPER_NAMES) {
            Ok(Some(p)) if flavor_of(&p) != wanted => Ok(None),
            other => other,
        };
        let by_name = |names: &[&str]| {
            let named = Search {
                explicit: None,
                ..whisper.clone()
            };
            named.find(WHISPER_ENV, names)
        };
        let (cpp, py) = if whisper.explicit.is_some() {
            (
                whisper_for(WhisperFlavor::Cpp),
                whisper_for(WhisperFlavor::OpenAi),
            )
        } else {
            (
                by_name(&["whisper-cli", "whisper-cpp"]),
                by_name(&["whisper"]),
            )
        };
        Capabilities {
            programs: vec![
                status(
                    "ffmpeg",
                    FFMPEG_ENV,
                    "video export, pauses, waveforms, audio extraction for transcripts",
                    FFMPEG_HINT,
                    ffmpeg.find(FFMPEG_ENV, &["ffmpeg"]),
                    versions.then_some("-version"),
                ),
                status(
                    "whisper-cli",
                    WHISPER_ENV,
                    "transcribe: word-timed speech to text (whisper.cpp; needs a model file in OPENDESIGNER_WHISPER_MODEL)",
                    WHISPER_HINT,
                    cpp,
                    versions.then_some("--version"),
                ),
                status(
                    "whisper",
                    WHISPER_ENV,
                    "transcribe: word-timed speech to text (OpenAI's Python whisper)",
                    WHISPER_HINT,
                    py,
                    versions.then_some("--version"),
                ),
                status(
                    "rembg",
                    REMBG_ENV,
                    "remove_background: cut the subject out of an image",
                    REMBG_HINT,
                    rembg.find(REMBG_ENV, &["rembg"]),
                    versions.then_some("--version"),
                ),
            ],
            whisper_model: model.map(|m| Path::new(&m).display().to_string()),
        }
    }
}
