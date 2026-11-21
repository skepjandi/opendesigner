//! Speech to text by a program the user installed: `whisper-cli` (whisper.cpp) or OpenAI's Python
//! `whisper` (`docs/wiki/video-editing.md#transcripts`).
//!
//! The caller hands over a 16 kHz mono WAV in a scratch folder ([`crate::external::Scratch`]).
//! This module builds the argument list from validated values, runs the program, reads the JSON
//! it wrote into the scratch folder and returns the words with their times. Each program's JSON
//! is read in its own parser; both give the same [`TimedWord`].

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde_json::Value;

use crate::external::{ExternalError, Limits, Whisper, WhisperFlavor, run};

/// The most words one transcript keeps (the caption limit).
pub const MAX_WORDS: usize = 30_000;
/// The most JSON read back from a program's result file.
pub const MAX_RESULT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_STDOUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 64 * 1024;
const MAX_MODEL_NAME: usize = 40;
const DEFAULT_OPENAI_MODEL: &str = "base";
const MAX_WORD_CHARS: usize = 80;

/// One spoken word, in milliseconds from the start of the sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimedWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

/// The language asked for: `auto` or a two- or three-letter code (`en`, `de`, `haw`).
pub fn language_arg(value: &str) -> Result<&str, ExternalError> {
    let ok = value == "auto"
        || ((2..=3).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_lowercase()));
    if ok {
        Ok(value)
    } else {
        Err(ExternalError::Failed(format!(
            "language is `auto` or a two- or three-letter code such as `en`, not {value:?}"
        )))
    }
}

/// The speech model: a file for whisper.cpp, a name for OpenAI's whisper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Model {
    File(PathBuf),
    Name(String),
}

/// A model name OpenAI's whisper takes (`base`, `small.en`, `large-v3`): plain characters only.
pub fn model_name(value: &str) -> Result<&str, ExternalError> {
    let ok = value.starts_with(|c: char| c.is_ascii_alphanumeric())
        && value.len() <= MAX_MODEL_NAME
        && value.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'-' | b'_')
        });
    if ok {
        Ok(value)
    } else {
        Err(ExternalError::Failed(format!(
            "{value:?} is not a model name; OpenAI's whisper takes names such as base, small.en or large-v3. whisper.cpp takes a model file (set OPENDESIGNER_WHISPER_MODEL)"
        )))
    }
}

impl Model {
    /// The model for `flavor` from the configured `setting` (a file path or a name): whisper.cpp
    /// needs an existing plain file, OpenAI's whisper a name (default `base`).
    pub fn for_flavor(
        flavor: WhisperFlavor,
        setting: Option<&str>,
    ) -> Result<Model, ExternalError> {
        match (flavor, setting) {
            (WhisperFlavor::Cpp, None) => Err(ExternalError::Failed(
                "whisper-cli needs a model file: download a ggml model (for example ggml-base.en.bin from the whisper.cpp project), then set OPENDESIGNER_WHISPER_MODEL to its path, or pass `model` as a file name kept in the app's models folder or this project's media/".into(),
            )),
            (WhisperFlavor::Cpp, Some(path)) => {
                let path = PathBuf::from(path);
                if path.is_file() {
                    Ok(Model::File(path))
                } else {
                    Err(ExternalError::Failed(format!(
                        "the speech model {} is not a file",
                        path.display()
                    )))
                }
            }
            (WhisperFlavor::OpenAi, None) => Ok(Model::Name(DEFAULT_OPENAI_MODEL.into())),
            (WhisperFlavor::OpenAi, Some(name)) => Ok(Model::Name(model_name(name)?.to_string())),
        }
    }
}

/// Where a model file named by an agent may be: a plain file name, looked up in each folder of
/// `folders` in turn (the app's models folder, the project's `media/`), never through a link.
pub fn model_file_in(name: &str, folders: &[PathBuf]) -> Result<PathBuf, ExternalError> {
    let plain = !name.is_empty()
        && name.len() <= 100
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !plain {
        return Err(ExternalError::Failed(format!(
            "model {name:?} must be a plain file name (letters, digits, . _ -) in the app's models folder or the project's media/"
        )));
    }
    folders
        .iter()
        .map(|f| f.join(name))
        .find(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
        .ok_or_else(|| {
            ExternalError::Failed(format!(
                "model file {name} was not found in {}",
                folders
                    .iter()
                    .map(|f| f.display().to_string())
                    .collect::<Vec<_>>()
                    .join(" or ")
            ))
        })
}

/// What one transcription needs besides the program.
pub struct Request<'a> {
    pub wav: &'a Path,
    pub model: &'a Model,
    pub language: &'a str,
    /// A folder the program may write its result into (a [`crate::external::Scratch`]).
    pub scratch: &'a Path,
    pub timeout: Duration,
    pub cancel: Option<&'a AtomicBool>,
}

fn os(parts: &[&str]) -> Vec<OsString> {
    parts.iter().map(OsString::from).collect()
}

/// The arguments for whisper.cpp: one word per segment (`-ml 1 -sow`), JSON to `<scratch>/out.json`.
pub fn cpp_args(r: &Request, model: &Path) -> Vec<OsString> {
    let mut args = os(&["-m"]);
    args.push(model.into());
    args.extend(os(&["-f"]));
    args.push(r.wav.into());
    args.extend(os(&[
        "-l", r.language, "-ml", "1", "-sow", "-np", "-oj", "-of",
    ]));
    args.push(r.scratch.join("out").into());
    args
}

/// The arguments for OpenAI's whisper: word timestamps, JSON into the scratch folder.
pub fn openai_args(r: &Request, model: &str) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![r.wav.into()];
    args.extend(os(&["--model", model]));
    if r.language != "auto" {
        args.extend(os(&["--language", r.language]));
    }
    args.extend(os(&[
        "--word_timestamps",
        "True",
        "--output_format",
        "json",
        "--fp16",
        "False",
        "--verbose",
        "False",
        "--output_dir",
    ]));
    args.push(r.scratch.into());
    args
}

fn read_result(path: &Path) -> Result<String, ExternalError> {
    let meta = std::fs::metadata(path)
        .map_err(|_| ExternalError::Failed("the speech program wrote no result".into()))?;
    if meta.len() > MAX_RESULT_BYTES {
        return Err(ExternalError::OutputTooLarge {
            program: "the speech program".into(),
            limit: MAX_RESULT_BYTES as usize,
        });
    }
    std::fs::read_to_string(path)
        .map_err(|e| ExternalError::Failed(format!("cannot read the speech result: {e}")))
}

/// What a speech program is asked and where its answer will be.
struct Plan {
    args: Vec<OsString>,
    result: PathBuf,
    parse: fn(&str) -> Result<Vec<TimedWord>, ExternalError>,
}

fn plan(program: &Whisper, r: &Request) -> Result<Plan, ExternalError> {
    match (program.flavor, r.model) {
        (WhisperFlavor::Cpp, Model::File(m)) => Ok(Plan {
            args: cpp_args(r, m),
            result: r.scratch.join("out.json"),
            parse: parse_cpp_json,
        }),
        (WhisperFlavor::OpenAi, Model::Name(n)) => Ok(Plan {
            args: openai_args(r, n),
            result: r.scratch.join(openai_result_name(r.wav)),
            parse: parse_openai_json,
        }),
        _ => Err(ExternalError::Failed(
            "the model does not fit the speech program (a file for whisper-cli, a name for whisper)"
                .into(),
        )),
    }
}

/// Runs `program` on the WAV of `request` and returns the words it heard.
pub fn transcribe(program: &Whisper, request: &Request) -> Result<Vec<TimedWord>, ExternalError> {
    let plan = plan(program, request)?;
    let limits = Limits {
        timeout: request.timeout,
        max_stdout: MAX_STDOUT_BYTES,
        max_stderr: MAX_STDERR_BYTES,
        cancel: request.cancel,
    };
    let done = run(&program.path, &plan.args, &limits)?;
    if !done.success {
        return Err(ExternalError::Failed(format!(
            "{} failed: {}",
            program.flavor.name(),
            done.last_error_line()
        )));
    }
    (plan.parse)(&read_result(&plan.result)?)
}

/// The JSON name OpenAI's whisper gives its result: the input's name without its ending.
fn openai_result_name(wav: &Path) -> String {
    format!(
        "{}.json",
        wav.file_stem().and_then(|s| s.to_str()).unwrap_or("audio")
    )
}

fn bad_json(program: &str, why: impl std::fmt::Display) -> ExternalError {
    ExternalError::Failed(format!(
        "the {program} result is not what it should be: {why}"
    ))
}

/// A piece of text that is a marker (`[_BEG_]`, `[BLANK_AUDIO]`, `<|0.00|>`), not speech.
fn is_marker(text: &str) -> bool {
    let t = text.trim();
    (t.starts_with('[') && t.ends_with(']')) || (t.starts_with("<|") && t.ends_with("|>"))
}

fn clean(text: &str) -> String {
    text.trim().chars().take(MAX_WORD_CHARS).collect()
}

/// Adds a word, or glues a bare punctuation mark to the word before it.
fn push_word(words: &mut Vec<TimedWord>, text: &str, start_ms: u64, end_ms: u64) {
    let text = clean(text);
    if text.is_empty() || is_marker(&text) {
        return;
    }
    let end_ms = end_ms.max(start_ms);
    if !text.chars().any(char::is_alphanumeric)
        && let Some(last) = words.last_mut()
    {
        last.text.push_str(&text);
        last.end_ms = last.end_ms.max(end_ms);
        return;
    }
    words.push(TimedWord {
        text,
        start_ms,
        end_ms,
    });
}

fn finish(words: Vec<TimedWord>) -> Result<Vec<TimedWord>, ExternalError> {
    if words.len() > MAX_WORDS {
        return Err(ExternalError::Failed(format!(
            "the transcript has more than {MAX_WORDS} words"
        )));
    }
    Ok(words)
}

fn millis(v: &Value) -> Option<u64> {
    v.as_u64()
}

fn seconds(v: &Value) -> Option<u64> {
    let s = v.as_f64()?;
    (s.is_finite() && s >= 0.0).then(|| (s * 1000.0).round() as u64)
}

/// whisper.cpp's `-oj` JSON: `{"transcription": [{"offsets": {"from": ms, "to": ms}, "text": "..."}]}`
/// where each segment is one word (`-ml 1 -sow`).
pub fn parse_cpp_json(json: &str) -> Result<Vec<TimedWord>, ExternalError> {
    let root: Value = serde_json::from_str(json).map_err(|e| bad_json("whisper-cli", e))?;
    let segments = root["transcription"]
        .as_array()
        .ok_or_else(|| bad_json("whisper-cli", "no `transcription` list"))?;
    let mut words = Vec::new();
    for s in segments {
        let (from, to) = (millis(&s["offsets"]["from"]), millis(&s["offsets"]["to"]));
        let (Some(from), Some(to), Some(text)) = (from, to, s["text"].as_str()) else {
            return Err(bad_json("whisper-cli", "a segment lacks offsets or text"));
        };
        push_word(&mut words, text, from, to);
    }
    finish(words)
}

/// OpenAI whisper's JSON: `{"segments": [{"words": [{"word": " Hi", "start": 0.0, "end": 0.4}]}]}`
/// with seconds as numbers.
pub fn parse_openai_json(json: &str) -> Result<Vec<TimedWord>, ExternalError> {
    let root: Value = serde_json::from_str(json).map_err(|e| bad_json("whisper", e))?;
    let segments = root["segments"]
        .as_array()
        .ok_or_else(|| bad_json("whisper", "no `segments` list"))?;
    let mut words = Vec::new();
    for w in segments
        .iter()
        .flat_map(|s| s["words"].as_array().into_iter().flatten())
    {
        let (start, end) = (seconds(&w["start"]), seconds(&w["end"]));
        let (Some(start), Some(end), Some(text)) = (start, end, w["word"].as_str()) else {
            return Err(bad_json("whisper", "a word lacks its time or text"));
        };
        push_word(&mut words, text, start, end);
    }
    if words.is_empty() && segments.iter().any(|s| s["words"].is_null()) {
        return Err(bad_json(
            "whisper",
            "the segments carry no word timings (this whisper ignored --word_timestamps)",
        ));
    }
    finish(words)
}

/// Stretches of at least `min_ms` between two words (and before the first), a rough stand-in for
/// silence detection that costs no second pass over the sound.
pub fn gaps(words: &[TimedWord], min_ms: u64) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    for w in words {
        if w.start_ms >= at + min_ms {
            out.push((at, w.start_ms));
        }
        at = at.max(w.end_ms);
    }
    out
}

/// The words as the JSON list `captions::parse_word_timings` reads.
pub fn words_json(words: &[TimedWord]) -> String {
    let list: Vec<Value> = words
        .iter()
        .map(|w| {
            serde_json::json!({
                "text": w.text,
                "start_ms": w.start_ms.min(u64::from(u32::MAX)),
                "end_ms": w.end_ms.min(u64::from(u32::MAX)),
            })
        })
        .collect();
    Value::Array(list).to_string()
}
