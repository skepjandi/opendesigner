//! `export_timeline`: a video's cuts as an edit decision list (`.edl`) and its captions as
//! subtitles (`.srt`, `.vtt`) in `exports/`, for editors that continue where OpenDesigner stops
//! (`docs/wiki/video-editing.md#timeline-export`). The text itself is made by `od_core::interchange`;
//! this module checks the arguments, asks ffmpeg (when there is one) which footage carries sound,
//! and writes the file the way every export is written: a plain name, no link followed.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use od_core::interchange;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{CallCtx, Output, ToolError, node_id, parse, tool};

pub const NAMES: [&str; 1] = ["export_timeline"];

const FORMATS: [&str; 3] = ["edl", "srt", "vtt"];
const MAX_NAME_CHARS: usize = 100;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub fn definitions() -> Vec<Value> {
    vec![tool(
        "export_timeline",
        "Write a video's timeline for another editor, to exports/<file>: `edl` is a CMX 3600 edit \
         decision list (Final Cut Pro, Premiere Pro, DaVinci Resolve and others import it): one \
         event per clip with its source file, source in and out and record in and out, footage \
         speed, dissolves for transitions, and sound tracks. `srt` and `vtt` are the video's \
         captions as subtitles. What an EDL cannot hold (design-frame clips, overlays, captions, \
         keyframes, grades, speed ramps, the kind of a transition) is listed in `warnings`: tell \
         the user. Design-frame clips are placeholders; export_video renders the finished video \
         for them. Writes only the exports folder.",
        json!({
            "video": {"type": "string", "description": "Id of a video from list_videos."},
            "format": {"type": "string", "enum": FORMATS, "description": "edl, srt or vtt."},
            "file": {"type": "string", "description": "A plain file name ending in the format, for example cut.edl (default: timeline.<format>)."}
        }),
        &["video", "format"],
        false,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "export_timeline" => export(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    video: String,
    format: String,
    #[serde(default)]
    file: Option<String>,
}

/// `name` when it is a plain file name ending in `.<ext>`.
fn plain_name<'a>(name: &'a str, ext: &str) -> Result<&'a str, ToolError> {
    let ok = name.strip_suffix(&format!(".{ext}")).is_some_and(|stem| {
        !stem.is_empty()
            && name.chars().count() <= MAX_NAME_CHARS
            && !name.contains("..")
            && stem
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    });
    if ok {
        Ok(name)
    } else {
        fail(format!(
            "file must be a plain name like cut.{ext} (letters, digits . _ -)"
        ))
    }
}

/// Writes `text` to `exports/<name>` in `dir`, refusing a link at the folder or the file.
fn write_text(dir: &Path, name: &str, text: &str) -> Result<(), ToolError> {
    let exports = dir.join("exports");
    let target = exports.join(name);
    for path in [&exports, &target] {
        if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return fail(format!(
                "{} is a link; nothing is written through it",
                path.display()
            ));
        }
    }
    fs::create_dir_all(&exports)?;
    fs::write(&target, text)?;
    Ok(())
}

fn export(ctx: &mut CallCtx, a: Args) -> Result<Output, ToolError> {
    if !FORMATS.contains(&a.format.as_str()) {
        return fail(format!("format is edl, srt or vtt, not {:?}", a.format));
    }
    let default = format!("timeline.{}", a.format);
    let name = plain_name(a.file.as_deref().unwrap_or(&default), &a.format)?.to_string();
    let Some(dir) = ctx.ws.dir().map(Path::to_path_buf) else {
        return fail("the project has no folder on disk, so there is nowhere to export to");
    };
    let id = node_id(&a.video)?;
    let doc = ctx.ws.session.document();
    let Some(video) = doc.video(&id) else {
        return fail(format!("video {id} does not exist; list_videos shows them"));
    };
    let mut warnings: Vec<String> = Vec::new();
    let (text, events) = match a.format.as_str() {
        "edl" => {
            if video.clips.is_empty() {
                return fail("the video has no clips, so there is nothing to list");
            }
            let ffmpeg = od_media::Ffmpeg::locate().ok();
            if ffmpeg.is_none() {
                warnings.push(
                    "ffmpeg was not found, so footage is listed as picture only: the sound of a clip is not in the EDL"
                        .into(),
                );
            }
            let known = std::cell::RefCell::new(HashMap::<String, bool>::new());
            let edl = interchange::edl(doc, video, &|file| {
                let Some(ff) = &ffmpeg else { return false };
                *known
                    .borrow_mut()
                    .entry(file.to_string())
                    .or_insert_with(|| {
                        od_media::footage_path(&dir, file)
                            .ok()
                            .is_some_and(|p| ff.probe_file(&p).is_ok_and(|info| info.has_audio))
                    })
            });
            warnings.extend(edl.warnings);
            (edl.text, Some(edl.events))
        }
        format => {
            if video.captions.cues.is_empty() {
                return fail("the video has no captions: add some, or transcribe the speech");
            }
            let text = if format == "srt" {
                interchange::srt(&video.captions)
            } else {
                interchange::vtt(&video.captions)
            };
            (text, None)
        }
    };
    write_text(&dir, &name, &text)?;
    let mut out = json!({
        "file": format!("exports/{name}"),
        "format": a.format,
        "bytes": text.len(),
        "warnings": warnings,
    });
    if let Some(events) = events {
        out["events"] = json!(events);
        out["next"] = json!(
            "tell the user what is in warnings; in the other editor, link the footage by file name"
        );
    } else {
        out["cues"] = json!(video.captions.cues.len());
    }
    Ok(Output::json(&out))
}

#[cfg(test)]
mod tests;
