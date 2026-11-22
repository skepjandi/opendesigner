//! `fit_clip_to_media`: make a clip as long as a recording or a sound, the way a Remotion
//! composition takes its length from `calculateMetadata()` (`docs/wiki/video-editing.md#fit-to-media`).
//! One `set_clip` through `apply_ops`: attributed, checkpointed, undoable.

use od_core::video::{MAX_CLIP_MS, MIN_CLIP_MS};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops, node_id, parse, tool,
};

pub const NAMES: [&str; 1] = ["fit_clip_to_media"];

const MAX_PAD_MS: u64 = 10_000;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub fn definitions() -> Vec<Value> {
    vec![tool(
        "fit_clip_to_media",
        "Set a clip's length from a file instead of guessing it. A footage clip with no `file` \
         plays the rest of its own file from its in point (at its speed). With `file` (a media/ \
         audio or footage file) the clip lasts as long as that file: a title card held for the \
         whole voice-over, a design frame under a music bed. `pad_ms` adds a hold after the end \
         (default 0). A clip is at most 60 s: a longer file is capped and the answer says so. \
         Changing a clip's length moves everything after it; overlays, captions, sound tracks and \
         keyframes keep their times, so check them. A footage clip with a speed ramp is refused \
         (its length is not one division). Needs ffmpeg.",
        json!({
            "video": {"type": "string", "description": "Id of a video from list_videos."},
            "clip": {"type": "string", "description": "Id of the clip, from list_videos."},
            "file": {"type": "string", "description": "media/<name> of an audio (.mp3 .wav .m4a .aac .ogg) or footage (.mp4 .mov .m4v) file."},
            "pad_ms": {"type": "integer", "minimum": 0, "maximum": MAX_PAD_MS, "description": "Extra milliseconds after the end of the file (default 0)."},
            "turn": {"type": "string"},
            "summary": {"type": "string"}
        }),
        &["video", "clip"],
        false,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "fit_clip_to_media" => fit(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    video: String,
    clip: String,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    pad_ms: Option<u64>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// How long a clip plays its own footage from `in_ms` to the end of a file of `file_ms`, at
/// constant `speed`.
fn rest_of_file_ms(file_ms: u64, in_ms: u64, speed: f64) -> Option<u64> {
    (file_ms > in_ms && speed > 0.0).then(|| ((file_ms - in_ms) as f64 / speed).round() as u64)
}

fn fit(ctx: &mut CallCtx, a: Args) -> Result<Output, ToolError> {
    let video = node_id(&a.video)?;
    let clip_id = node_id(&a.clip)?;
    let pad = a.pad_ms.unwrap_or(0);
    if pad > MAX_PAD_MS {
        return fail(format!("pad_ms is 0 to {MAX_PAD_MS}, not {pad}"));
    }
    let (clip, before) = {
        let doc = ctx.ws.session.document();
        let v = doc.video(&video).ok_or_else(|| {
            ToolError(format!(
                "video {video} does not exist; list_videos shows them"
            ))
        })?;
        let clip = v
            .clips
            .iter()
            .find(|c| c.id == clip_id)
            .ok_or_else(|| ToolError(format!("clip {clip_id} is not in video {video}")))?
            .clone();
        (clip, v.duration_ms())
    };
    if !clip.speed_ramp.is_empty() && a.file.is_none() {
        return fail(
            "this clip has a speed ramp: set its length with set_clip, or give `file` to match another file",
        );
    }
    let (reference, own) = match (&a.file, &clip.footage) {
        (Some(f), Some(ft)) => (f.clone(), *f == ft.file),
        (Some(f), None) => (f.clone(), false),
        (None, Some(ft)) => (ft.file.clone(), true),
        (None, None) => {
            return fail(
                "a design-frame clip needs `file`: the audio or footage it should last as long as",
            );
        }
    };
    let dir = ctx
        .ws
        .dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder, so no media/".into()))?;
    let path = if od_core::media::audio_name(&reference).is_ok() {
        od_media::audio_path(&dir, &reference)
    } else if od_core::media::footage_name(&reference).is_ok() {
        od_media::footage_path(&dir, &reference)
    } else {
        return fail(format!(
            "{reference:?} is not an audio or footage file in media/ (.mp3 .wav .m4a .aac .ogg .mp4 .mov .m4v)"
        ));
    }
    .map_err(|e| ToolError(e.to_string()))?;
    let ffmpeg = od_media::Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))?;
    let file_ms = ffmpeg
        .probe_duration(&path)
        .map_err(|e| ToolError(e.to_string()))?;
    let wanted = if own {
        let in_ms = clip.footage.as_ref().map_or(0, |f| f.in_ms);
        rest_of_file_ms(file_ms, in_ms, clip.speed).ok_or_else(|| {
            ToolError(format!(
                "{reference} is {file_ms} ms long and the clip starts at {in_ms} ms: nothing is left to play"
            ))
        })?
    } else {
        file_ms
    } + pad;
    let duration = wanted.clamp(u64::from(MIN_CLIP_MS), u64::from(MAX_CLIP_MS)) as u32;
    let op: od_core::Operation = serde_json::from_value(json!({
        "op": "set_clip", "video": video, "clip": clip_id,
        "prop": {"name": "duration_ms", "value": duration},
    }))
    .map_err(|e| ToolError(format!("invalid arguments: {e}")))?;
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops: vec![op],
            turn: a.turn,
            summary: a.summary,
        },
    )?;
    let after = ctx
        .ws
        .session
        .document()
        .video(&video)
        .map_or(0, od_core::Video::duration_ms);
    let mut receipt: Value = match out.content.first() {
        Some(Content::Text(t)) => serde_json::from_str(t).unwrap_or(Value::Null),
        _ => Value::Null,
    };
    if let Some(r) = receipt.as_object_mut() {
        r.insert(
            "fit".into(),
            json!({
                "file": reference, "file_ms": file_ms, "clip_before_ms": clip.duration_ms,
                "clip_ms": duration, "capped": wanted > u64::from(MAX_CLIP_MS),
                "video_ms_before": before, "video_ms_after": after,
                "next": "overlays, captions and sound tracks keep their times: check what comes after this clip",
            }),
        );
    }
    Ok(Output::json(&receipt))
}

#[cfg(test)]
mod tests;
