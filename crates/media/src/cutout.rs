//! Background removal by `rembg`, a program the user installed
//! (`docs/wiki/security.md#external-programs`).
//!
//! `rembg i <in> <out>` reads a project image and writes a PNG with the background made
//! transparent (or, with `-om`, only the mask). The result is written into a scratch folder first;
//! only a PNG that passes the checks is copied into `media/` under a name that is free, next to a
//! sidecar that says where it came from.

use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::external::{ExternalError, Limits, Scratch, run};

/// The largest picture that is cut out (what a project image may be).
pub const MAX_INPUT_BYTES: u64 = 25 * 1024 * 1024;
/// The largest result that is kept.
pub const MAX_OUTPUT_BYTES: u64 = 100 * 1024 * 1024;
const MAX_STREAM_BYTES: usize = 64 * 1024;
const MAX_MODEL_NAME: usize = 40;
const MAX_STEM_CHARS: usize = 60;
const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
/// The sidecar's file name ending, after the PNG's name.
pub const SIDECAR_SUFFIX: &str = ".rembg.json";

/// A rembg model name (`u2net`, `isnet-general-use`, `birefnet-general`): plain characters only.
pub fn model_name(value: &str) -> Result<&str, ExternalError> {
    let ok = value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value.len() <= MAX_MODEL_NAME
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'));
    if ok {
        Ok(value)
    } else {
        Err(ExternalError::Failed(format!(
            "{value:?} is not a rembg model name (lowercase letters, digits, - and _, for example u2net or isnet-general-use)"
        )))
    }
}

/// What to cut out.
pub struct Job<'a> {
    pub program: &'a Path,
    /// The plain file in `media/` to read.
    pub input: &'a Path,
    /// Its reference, `media/<name>`, for the sidecar.
    pub reference: &'a str,
    pub project: &'a Path,
    pub model: Option<&'a str>,
    pub mask_only: bool,
    pub timeout: Duration,
    pub cancel: Option<&'a AtomicBool>,
}

/// What was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cutout {
    /// `media/<name>.png`
    pub file: String,
    pub sidecar: String,
    pub bytes: u64,
}

/// The arguments: `i`, the model and mask flags, then the input and output paths.
pub fn args(job: &Job, output: &Path) -> Vec<OsString> {
    let mut args = vec![OsString::from("i")];
    if let Some(model) = job.model {
        args.extend([OsString::from("-m"), OsString::from(model)]);
    }
    if job.mask_only {
        args.push(OsString::from("-om"));
    }
    args.push(job.input.into());
    args.push(output.into());
    args
}

/// The file name stem of `input` made safe for the new file's name.
fn stem_of(input: &Path) -> String {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("image");
    let cleaned: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ' ') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches(['-', ' ']);
    let trimmed: String = trimmed.chars().take(MAX_STEM_CHARS).collect();
    if trimmed.is_empty() {
        "image".into()
    } else {
        trimmed
    }
}

fn io_error(path: &Path, e: &std::io::Error) -> ExternalError {
    ExternalError::Failed(format!("{}: {e}", path.display()))
}

/// Writes a file that must not exist yet.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ExternalError> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| io_error(path, &e))?;
    file.write_all(bytes).map_err(|e| io_error(path, &e))
}

fn free_name(media: &Path, stem: &str, kind: &str) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                format!("{stem}-{kind}")
            } else {
                format!("{stem}-{kind}-{n}")
            }
        })
        .find(|name| {
            !media.join(format!("{name}.png")).exists()
                && !media.join(format!("{name}{SIDECAR_SUFFIX}")).exists()
        })
        .unwrap_or_default()
}

fn checked_png(path: &Path) -> Result<Vec<u8>, ExternalError> {
    let meta = std::fs::metadata(path)
        .map_err(|_| ExternalError::Failed("rembg wrote no picture".into()))?;
    if !meta.is_file() || meta.len() > MAX_OUTPUT_BYTES {
        return Err(ExternalError::Failed(
            "rembg's picture is not a file or is too large".into(),
        ));
    }
    let bytes = std::fs::read(path).map_err(|e| io_error(path, &e))?;
    if bytes.starts_with(&PNG_MAGIC) {
        Ok(bytes)
    } else {
        Err(ExternalError::Failed("rembg did not write a PNG".into()))
    }
}

/// Runs rembg and writes `media/<stem>-cutout.png` (or `-mask.png`) with its sidecar.
pub fn remove_background(job: &Job) -> Result<Cutout, ExternalError> {
    let size = std::fs::metadata(job.input)
        .map_err(|e| io_error(job.input, &e))?
        .len();
    if size > MAX_INPUT_BYTES {
        return Err(ExternalError::Failed(format!(
            "{} is {size} bytes, over the {MAX_INPUT_BYTES} byte limit for a cutout",
            job.reference
        )));
    }
    let scratch = Scratch::new()?;
    let output = scratch.path().join("out.png");
    let limits = Limits {
        timeout: job.timeout,
        max_stdout: MAX_STREAM_BYTES,
        max_stderr: MAX_STREAM_BYTES,
        cancel: job.cancel,
    };
    let done = run(job.program, &args(job, &output), &limits)?;
    if !done.success {
        return Err(ExternalError::Failed(format!(
            "rembg failed: {}",
            done.last_error_line()
        )));
    }
    let png = checked_png(&output)?;
    let media = job.project.join("media");
    let name = free_name(
        &media,
        &stem_of(job.input),
        if job.mask_only { "mask" } else { "cutout" },
    );
    let png_path: PathBuf = media.join(format!("{name}.png"));
    write_new(&png_path, &png)?;
    let sidecar_name = format!("{name}{SIDECAR_SUFFIX}");
    let sidecar = serde_json::json!({
        "source": job.reference,
        "tool": "rembg",
        "model": job.model.unwrap_or("rembg default"),
        "mask_only": job.mask_only,
        "processed": "locally, by the rembg program on this computer",
    });
    let text =
        serde_json::to_string_pretty(&sidecar).map_err(|e| ExternalError::Failed(e.to_string()))?;
    if let Err(e) = write_new(&media.join(&sidecar_name), text.as_bytes()) {
        let _ = std::fs::remove_file(&png_path);
        return Err(e);
    }
    Ok(Cutout {
        file: format!("media/{name}.png"),
        sidecar: format!("media/{sidecar_name}"),
        bytes: png.len() as u64,
    })
}
