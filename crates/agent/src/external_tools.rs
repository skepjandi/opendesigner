//! MCP tools that run programs the user installed (`docs/wiki/security.md#external-programs`):
//! `get_capabilities` says which ones are there, `transcribe` turns speech into word-timed text
//! (and, on request, captions) and `remove_background` cuts a subject out of a project image.
//!
//! The mechanics (finding a program, running it as a child process with limits, parsing what it
//! prints) live in `od_media::{external, speech, cutout}`. This module validates what an agent
//! sent, hands over only checked values and turns the result into a tool answer. Captions go
//! through the ordinary `import_captions` tool, so they are one attributed, undoable change.

use std::path::{Path, PathBuf};
use std::time::Duration;

use od_core::{NodeId, captions};
use od_media::external::{
    Capabilities, ExternalError, REMBG_ENV, Scratch, Search, WHISPER_ENV, WHISPER_MODEL_ENV,
    Whisper, WhisperFlavor, locate_rembg_in,
};
use od_media::speech::{self, Model, TimedWord};
use od_media::{Ffmpeg, cutout};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{CallCtx, Output, ToolError, node_id, parse, tool};

pub const NAMES: [&str; 3] = ["get_capabilities", "transcribe", "remove_background"];

const DEFAULT_TRANSCRIBE_S: u64 = 300;
const MAX_TRANSCRIBE_S: u64 = 1_800;
const DEFAULT_CUTOUT_S: u64 = 180;
const MAX_CUTOUT_S: u64 = 900;
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(300);
const DEFAULT_PAUSE_MS: u64 = 500;
/// The most words and pauses a transcript answer lists; the count says how many there were.
const MAX_LISTED_WORDS: usize = 3_000;
const MAX_LISTED_PAUSES: usize = 500;

const INSTALL_FIRST: &str = "Call get_capabilities first. When a program is missing, do not try the tool: tell the user what to install (the report has the exact command) and carry on without it.";

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

impl From<ExternalError> for ToolError {
    fn from(e: ExternalError) -> Self {
        ToolError(e.to_string())
    }
}

pub fn definitions() -> Vec<Value> {
    vec![
        tool(
            "get_capabilities",
            "Which optional programs the user has installed, with their versions: ffmpeg (video \
             export, pauses, waveforms), a speech-to-text program (whisper-cli or whisper, for \
             transcribe) and rembg (for remove_background), each with the exact install hint when \
             it is missing. Read-only: it runs only `--version`. Call it before transcribe, \
             remove_background or export_video. If something you need is missing, ask the user to \
             install it instead of trying. OpenDesigner never installs or downloads these.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "transcribe",
            &format!(
                "Speech to text, on the user's computer: runs the user's whisper program over a \
                 media/ audio or video file (`file`) or over a video's own sound mix (`video`) and \
                 returns the words with their times in ms (the same shape import_captions reads \
                 as words_json), the plain text, and gaps between words (like detect_pauses). \
                 With `apply: true` the words also become the video's captions with word \
                 highlighting, as one undoable change (`video` is then required; with `file` \
                 add `offset_ms` where the file starts on the timeline). Nothing is uploaded by \
                 OpenDesigner. Slow: minutes for long speech. {INSTALL_FIRST} whisper-cli \
                 (whisper.cpp) needs a model file: the user sets OPENDESIGNER_WHISPER_MODEL, or you \
                 pass `model` as a plain file name the user placed in the app's models folder or in \
                 media/; the Python whisper takes a model name such as base or small.en."
            ),
            json!({
                "file": {"type": "string", "description": "A media/ audio (.mp3 .wav .m4a .aac .ogg) or footage (.mp4 .mov .m4v) file. Give this or `video`."},
                "video": {"type": "string", "description": "Id of a video from list_videos: its sound mix is transcribed, so the times are timeline times. Required with apply."},
                "language": {"type": "string", "description": "`auto` (default) or a two- or three-letter code such as en, de, es."},
                "model": {"type": "string", "description": "whisper-cli: a plain model file name in the app's models folder or media/. whisper: a model name (base, small.en, large-v3). Default: OPENDESIGNER_WHISPER_MODEL, or base for whisper."},
                "apply": {"type": "boolean", "description": "Also set the words as the video's captions (replaces its cues)."},
                "offset_ms": {"type": "integer", "description": "With file and apply: where the file starts on the video's timeline (default 0)."},
                "min_pause_ms": {"type": "integer", "minimum": 100, "maximum": 10000, "description": "The shortest gap between words reported as a pause (default 500)."},
                "timeout_s": {"type": "integer", "minimum": 10, "maximum": MAX_TRANSCRIBE_S, "description": "Stop the program after this long (default 300)."},
                "turn": {"type": "string"},
                "summary": {"type": "string"}
            }),
            &[],
            false,
        ),
        tool(
            "remove_background",
            &format!(
                "Cut the subject out of an image with the user's rembg, on their computer: reads \
                 a media/ image (png, jpg, webp) and writes media/<name>-cutout.png with a \
                 transparent background (or, with `mask_only`, media/<name>-mask.png, the \
                 black-and-white mask), plus a .rembg.json sidecar naming the source, tool and \
                 model. Nothing is overwritten and the source stays as it is. The result is an \
                 ordinary image asset: place it with an image node. Nothing is uploaded by \
                 OpenDesigner. {INSTALL_FIRST}"
            ),
            json!({
                "file": {"type": "string", "description": "media/<name> of a png, jpg or webp."},
                "model": {"type": "string", "description": "A rembg model name such as u2net, isnet-general-use or birefnet-general (the user's rembg may download it the first time). Default: rembg's own. Model weights have their own licences and some (for example bria-rmbg) are non-commercial: when the project is commercial, name isnet-general-use or u2net."},
                "mask_only": {"type": "boolean", "description": "Write only the mask."},
                "timeout_s": {"type": "integer", "minimum": 10, "maximum": MAX_CUTOUT_S, "description": "Stop the program after this long (default 180)."}
            }),
            &["file"],
            false,
        ),
    ]
}

/// Where the programs are looked for and which speech model is configured: the process's
/// environment in use, a fixed set in tests.
struct Programs {
    whisper: Search,
    rembg: Search,
    model: Option<String>,
}

impl Programs {
    fn from_env() -> Programs {
        Programs {
            whisper: Search::from_env(WHISPER_ENV),
            rembg: Search::from_env(REMBG_ENV),
            model: std::env::var(WHISPER_MODEL_ENV)
                .ok()
                .filter(|v| !v.is_empty()),
        }
    }
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    call_with(ctx, name, args, &Programs::from_env())
}

fn call_with(
    ctx: &mut CallCtx,
    name: &str,
    args: Value,
    programs: &Programs,
) -> Result<Output, ToolError> {
    match name {
        "get_capabilities" => Ok(Output::json(&capabilities_json(&Capabilities::probe()))),
        "transcribe" => transcribe(ctx, parse(args)?, programs),
        "remove_background" => remove_background(ctx, parse(args)?, programs),
        other => fail(format!("unknown tool {other:?}")),
    }
}

/// The report of `get_capabilities` and `opendesigner doctor --json`.
pub fn capabilities_json(caps: &Capabilities) -> Value {
    let has = |n: &str| caps.program(n).is_some_and(|p| p.found.is_some());
    let programs: Vec<Value> = caps
        .programs
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "found": p.found.is_some(),
                "path": p.found.as_ref().map(|f| f.display().to_string()),
                "version": p.version,
                "used_for": p.purpose,
                "override_env": p.env,
                "install": (p.found.is_none()).then_some(p.install_hint),
                "problem": p.problem,
            })
        })
        .collect();
    let speech_program = has("whisper-cli") || has("whisper");
    json!({
        "programs": programs,
        "speech_model": caps.whisper_model,
        "can": {
            "export_video": has("ffmpeg"),
            "transcribe": speech_program && has("ffmpeg"),
            "remove_background": has("rembg"),
        },
        "missing": caps.missing(),
        "note": "These are programs the user installs themselves; OpenDesigner runs them locally as separate processes and never installs, downloads or uploads anything. A program may fetch a model on its own if the user set it up that way. If one is missing, ask the user to install it.",
    })
}

/// A line for `get_guidance` when optional programs are missing.
pub fn missing_note() -> Option<String> {
    let missing = Capabilities::probe_quick().missing();
    (!missing.is_empty()).then(|| {
        format!(
            "Optional programs not installed: {}. Call get_capabilities for what each is for and how to install it, and ask the user before using the tools that need them.",
            missing.join(", ")
        )
    })
}

fn project_dir(ctx: &CallCtx) -> Result<PathBuf, ToolError> {
    match ctx.ws.dir() {
        Some(d) => Ok(d.to_path_buf()),
        None => fail("the project has no folder, so no media/"),
    }
}

/// `media/<name>` as a plain file inside the project (never a link), or why not.
fn media_path(project: &Path, reference: &str) -> Result<PathBuf, ToolError> {
    let name = reference
        .strip_prefix("media/")
        .filter(|n| !n.is_empty() && !n.contains(['/', '\\']) && !n.starts_with('.'))
        .ok_or_else(|| ToolError(format!("{reference:?} is not a media/<file> reference")))?;
    let path = project.join("media").join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() => Ok(path),
        Ok(_) => fail(format!("{reference} is not a plain file")),
        Err(e) => fail(format!("{reference}: {e}")),
    }
}

fn timeout(secs: Option<u64>, default: u64, max: u64) -> Result<Duration, ToolError> {
    let secs = secs.unwrap_or(default);
    if (10..=max).contains(&secs) {
        Ok(Duration::from_secs(secs))
    } else {
        fail(format!("timeout_s is from 10 to {max}, not {secs}"))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TranscribeArgs {
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    video: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    apply: bool,
    #[serde(default)]
    offset_ms: Option<i64>,
    #[serde(default)]
    min_pause_ms: Option<u64>,
    #[serde(default)]
    timeout_s: Option<u64>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// Where the speech comes from.
enum Source {
    File(PathBuf),
    Mix(NodeId),
}

fn source_of(ctx: &CallCtx, a: &TranscribeArgs) -> Result<Source, ToolError> {
    match (&a.file, &a.video) {
        (Some(_), Some(_)) if !a.apply => {
            fail("give `file` or `video`, not both (both only with apply)")
        }
        (Some(f), _) => {
            let known =
                od_core::media::audio_name(f).is_ok() || od_core::media::footage_name(f).is_ok();
            if !known {
                return fail(format!(
                    "{f:?} is not an audio or footage file in media/ (.mp3 .wav .m4a .aac .ogg .mp4 .mov .m4v)"
                ));
            }
            Ok(Source::File(media_path(&project_dir(ctx)?, f)?))
        }
        (None, Some(v)) => Ok(Source::Mix(node_id(v)?)),
        (None, None) => {
            fail("give `file` (a media/ audio or footage file) or `video` (its sound mix)")
        }
    }
}

/// The model setting to use: the argument, else the environment.
fn model_for(
    ctx: &CallCtx,
    flavor: WhisperFlavor,
    arg: Option<&str>,
    configured: Option<&str>,
) -> Result<Model, ToolError> {
    match (flavor, arg) {
        (WhisperFlavor::Cpp, Some(name)) => {
            let mut folders = Vec::new();
            if let Ok(config) = crate::token::config_dir() {
                folders.push(config.join("models"));
            }
            folders.push(project_dir(ctx)?.join("media"));
            let file = speech::model_file_in(name, &folders)?;
            Ok(Model::File(file))
        }
        (_, Some(name)) => Ok(Model::for_flavor(flavor, Some(name))?),
        (_, None) => Ok(Model::for_flavor(flavor, configured)?),
    }
}

/// Writes the 16 kHz mono WAV of the source into `scratch` and returns its path.
fn speech_wav(
    ctx: &CallCtx,
    source: &Source,
    ffmpeg: &Ffmpeg,
    scratch: &Path,
) -> Result<PathBuf, ToolError> {
    let input = match source {
        Source::File(path) => path.clone(),
        Source::Mix(video) => {
            let src = crate::video_export::Source::of(ctx.ws);
            match crate::preview_audio::prepare(&src, video, ffmpeg, scratch)? {
                Some(mix) => mix,
                None => return fail(format!("video {video} has no sound to transcribe")),
            }
        }
    };
    let wav = scratch.join("speech.wav");
    ffmpeg.extract_speech_wav(&input, &wav, EXTRACT_TIMEOUT, None)?;
    Ok(wav)
}

fn word_rows(words: &[TimedWord]) -> Vec<Value> {
    words
        .iter()
        .take(MAX_LISTED_WORDS)
        .map(|w| json!({"text": w.text, "start_ms": w.start_ms, "end_ms": w.end_ms}))
        .collect()
}

fn pause_rows(words: &[TimedWord], min_ms: u64) -> Vec<Value> {
    speech::gaps(words, min_ms)
        .into_iter()
        .take(MAX_LISTED_PAUSES)
        .map(|(s, e)| json!({"start_ms": s, "end_ms": e, "duration_ms": e - s}))
        .collect()
}

fn answer(words: &[TimedWord], language: &str, flavor: WhisperFlavor, min_pause_ms: u64) -> Value {
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    json!({
        "program": flavor.name(),
        "language": language,
        "word_count": words.len(),
        "text": text,
        "words": word_rows(words),
        "words_truncated": words.len() > MAX_LISTED_WORDS,
        "pauses": pause_rows(words, min_pause_ms),
        "pauses_note": "gaps between spoken words, from the transcript; times are in the sound's own time (timeline time with `video`)",
        "processed": "locally; nothing was uploaded",
    })
}

fn transcribe(
    ctx: &mut CallCtx,
    a: TranscribeArgs,
    programs: &Programs,
) -> Result<Output, ToolError> {
    let language = speech::language_arg(a.language.as_deref().unwrap_or("auto"))?.to_string();
    let min_pause = a.min_pause_ms.unwrap_or(DEFAULT_PAUSE_MS);
    if !(100..=10_000).contains(&min_pause) {
        return fail(format!(
            "min_pause_ms is from 100 to 10000, not {min_pause}"
        ));
    }
    let limit = timeout(a.timeout_s, DEFAULT_TRANSCRIBE_S, MAX_TRANSCRIBE_S)?;
    if a.apply && a.video.is_none() {
        return fail("apply needs `video`: the video whose captions are set");
    }
    let source = source_of(ctx, &a)?;
    let (words, flavor) = recognize(ctx, &source, &language, a.model.as_deref(), limit, programs)?;
    let mut out = answer(&words, &language, flavor, min_pause);
    if a.apply {
        out["captions"] = apply_captions(ctx, &a, &words)?;
    }
    Ok(Output::json(&out))
}

/// The words spoken in the sound of `source`, by the user's speech program, and which program
/// it was.
fn recognize(
    ctx: &CallCtx,
    source: &Source,
    language: &str,
    model: Option<&str>,
    limit: Duration,
    programs: &Programs,
) -> Result<(Vec<TimedWord>, WhisperFlavor), ToolError> {
    let program = Whisper::locate_in(&programs.whisper)?;
    let model = model_for(ctx, program.flavor, model, programs.model.as_deref())?;
    let ffmpeg = Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))?;
    let scratch = Scratch::new()?;
    let wav = speech_wav(ctx, source, &ffmpeg, scratch.path())?;
    let words = speech::transcribe(
        &program,
        &speech::Request {
            wav: &wav,
            model: &model,
            language,
            scratch: scratch.path(),
            timeout: limit,
            cancel: None,
        },
    )?;
    drop(scratch);
    if words.is_empty() {
        return fail("the speech program heard no words (is there speech in the sound?)");
    }
    Ok((words, program.flavor))
}

/// The words spoken in a `media/` audio or footage file, for the footage index.
pub(crate) fn words_of_media(
    ctx: &CallCtx,
    file: &str,
    language: &str,
    model: Option<&str>,
    timeout_s: Option<u64>,
) -> Result<Vec<TimedWord>, ToolError> {
    let language = speech::language_arg(language)?;
    let limit = timeout(timeout_s, DEFAULT_TRANSCRIBE_S, MAX_TRANSCRIBE_S)?;
    let source = Source::File(media_path(&project_dir(ctx)?, file)?);
    recognize(ctx, &source, language, model, limit, &Programs::from_env()).map(|(words, _)| words)
}

/// Sets the words as the video's captions through the ordinary `import_captions` tool.
fn apply_captions(
    ctx: &mut CallCtx,
    a: &TranscribeArgs,
    words: &[TimedWord],
) -> Result<Value, ToolError> {
    let json = speech::words_json(words);
    captions::parse_word_timings(&json)?;
    let mut args = json!({
        "video": a.video,
        "format": "words_json",
        "text": json,
        "offset_ms": a.offset_ms.unwrap_or(0),
    });
    for (key, value) in [("turn", &a.turn), ("summary", &a.summary)] {
        if let Some(v) = value {
            args[key] = json!(v);
        }
    }
    let out = crate::audio_tools::call(ctx, "import_captions", args)?;
    Ok(match out.content.first() {
        Some(crate::tools::Content::Text(t)) => serde_json::from_str(t).unwrap_or(Value::Null),
        _ => Value::Null,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CutoutArgs {
    file: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    mask_only: bool,
    #[serde(default)]
    timeout_s: Option<u64>,
}

fn remove_background(
    ctx: &mut CallCtx,
    a: CutoutArgs,
    programs: &Programs,
) -> Result<Output, ToolError> {
    od_core::media::reference_name(&a.file)?;
    let model = a.model.as_deref().map(cutout::model_name).transpose()?;
    let limit = timeout(a.timeout_s, DEFAULT_CUTOUT_S, MAX_CUTOUT_S)?;
    let project = project_dir(ctx)?;
    let input = media_path(&project, &a.file)?;
    let program = locate_rembg_in(&programs.rembg)?;
    let made = cutout::remove_background(&cutout::Job {
        program: &program,
        input: &input,
        reference: &a.file,
        project: &project,
        model,
        mask_only: a.mask_only,
        timeout: limit,
        cancel: None,
    })?;
    Ok(Output::json(&json!({
        "file": made.file,
        "sidecar": made.sidecar,
        "bytes": made.bytes,
        "mask_only": a.mask_only,
        "source": a.file,
        "processed": "locally by rembg; nothing was uploaded",
        "next": "place it with an image node whose source is the file",
    })))
}

#[cfg(all(test, unix))]
mod tests;
