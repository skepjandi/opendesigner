//! `audio_reactive`: a number that follows how loud a sound is (`docs/wiki/video-editing.md#audio-reactive-motion`).
//!
//! The sound of a media file is measured in short windows by `ffmpeg` (all of it, or only the
//! bass, the middle or the treble), turned into a smooth 0 to 1 curve, scaled to the range the
//! agent chose and written as ordinary keyframes on one property of one node, through `apply_ops`:
//! one attributed, undoable step, and nothing is played back live. A bar that rises with the
//! kick, a glow that breathes with the music, a mouth that opens with the voice are all this tool
//! on a different property.

use od_core::animation::MAX_KEYS;
use od_media::footage::{Band, normalize};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops, node_id, parse, tool,
};

pub const NAMES: [&str; 1] = ["audio_reactive"];

const DEFAULT_KEYS_PER_SECOND: u32 = 15;
const MAX_KEYS_PER_SECOND: u32 = 30;
const DEFAULT_FLOOR: f32 = 0.1;
const DEFAULT_ATTACK: f32 = 0.6;
const DEFAULT_RELEASE: f32 = 0.25;
/// Two keys closer than this in value, with the same value on both sides, are one flat stretch.
const FLAT: f64 = 0.0005;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub fn definitions() -> Vec<Value> {
    vec![tool(
        "audio_reactive",
        "Make one property of one node follow a sound: the sound of a media/ audio or footage file \
         is measured (`band` level, or only low = bass and kick, mid = voice, high = hats and \
         sibilants), smoothed and written as keyframes between `min` (silence) and `max` (the \
         loudest moment). Examples: a bar's `scale_y` from 0.2 to 1 with the bass, a glow's \
         `opacity` from 0.3 to 1 with the music, a mouth's `scale_y` from 0.1 to 1 with a voice \
         track (lip flap), a logo's `scale` from 0 to 0.12 on the kick (`additive: true` keeps \
         its travel). `start_ms` is where the sound starts on the video's timeline, `from_ms` and \
         `to_ms` the part of the file used. At most 500 keys a track, so a long sound needs \
         fewer `keys_per_second`. One undoable step; replaces any track on that node and property \
         unless `additive`. Needs ffmpeg and a sound; check the result with video_strip.",
        json!({
            "video": {"type": "string", "description": "Id of a video from list_videos."},
            "node": {"type": "string", "description": "Id of the node to move, from get_tree."},
            "prop": {"type": "string", "description": "The property to drive, as animate takes it: x, y, width, height, opacity, rotation, scale, scale_x, scale_y, blur, stroke_width, trim_end, and so on."},
            "file": {"type": "string", "description": "media/<name> of an audio (.mp3 .wav .m4a .aac .ogg) or footage (.mp4 .mov .m4v) file with sound."},
            "band": {"type": "string", "enum": ["level", "low", "mid", "high"], "description": "Which part of the sound to follow (default level)."},
            "min": {"type": "number", "description": "The value in silence (default 0)."},
            "max": {"type": "number", "description": "The value at the loudest moment (default 1)."},
            "start_ms": {"type": "integer", "minimum": 0, "description": "Where the sound starts on the video's timeline (default 0)."},
            "from_ms": {"type": "integer", "minimum": 0, "description": "Where in the file to start measuring (default 0)."},
            "to_ms": {"type": "integer", "minimum": 1, "description": "Where in the file to stop (default: as long as the video lasts after start_ms, or the file's end)."},
            "keys_per_second": {"type": "integer", "minimum": 2, "maximum": MAX_KEYS_PER_SECOND, "description": "How often a key is written (default 15)."},
            "floor": {"type": "number", "minimum": 0, "maximum": 0.9, "description": "Sound quieter than this share of the loudest counts as silence, so room noise does not move anything (default 0.1)."},
            "attack": {"type": "number", "minimum": 0.05, "maximum": 1, "description": "How quickly the value rises with the sound, 1 is at once (default 0.6)."},
            "release": {"type": "number", "minimum": 0.05, "maximum": 1, "description": "How quickly it falls back, lower is a longer tail (default 0.25)."},
            "additive": {"type": "boolean", "description": "Add the values to the node's own instead of replacing it: min 0 and max 20 on x is a shake of up to 20 px (default false)."},
            "turn": {"type": "string"},
            "summary": {"type": "string"}
        }),
        &["video", "node", "prop", "file"],
        false,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "audio_reactive" => reactive(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    video: String,
    node: String,
    prop: String,
    file: String,
    #[serde(default)]
    band: Option<String>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    start_ms: Option<u64>,
    #[serde(default)]
    from_ms: Option<u64>,
    #[serde(default)]
    to_ms: Option<u64>,
    #[serde(default)]
    keys_per_second: Option<u32>,
    #[serde(default)]
    floor: Option<f32>,
    #[serde(default)]
    attack: Option<f32>,
    #[serde(default)]
    release: Option<f32>,
    #[serde(default)]
    additive: bool,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// Milliseconds of frame number `frame` at `fps`, the grid every key sits on.
pub(crate) fn frame_ms(frame: u64, fps: u32) -> u64 {
    (frame * 1000 + u64::from(fps) / 2) / u64::from(fps)
}

/// Keys at `start_frame`, `start_frame + step`, ... with the values of `curve` (0 to 1) mapped
/// onto `min..=max`. A key inside a flat stretch (the same value before and after) is left out.
fn keys_of(
    curve: &[f32],
    min: f64,
    max: f64,
    start_frame: u64,
    step: u64,
    fps: u32,
) -> Vec<(u64, f64)> {
    let values: Vec<f64> = curve
        .iter()
        .map(|&c| ((min + (max - min) * f64::from(c)) * 1000.0).round() / 1000.0)
        .collect();
    values
        .iter()
        .enumerate()
        .filter(|&(i, &v)| {
            let flat_before = i > 0 && (values[i - 1] - v).abs() < FLAT;
            let flat_after = values.get(i + 1).is_some_and(|n| (n - v).abs() < FLAT);
            !(flat_before && flat_after)
        })
        .map(|(i, &v)| (frame_ms(start_frame + i as u64 * step, fps), v))
        .collect()
}

fn check_unit(name: &str, value: f32, low: f32, high: f32) -> Result<f32, ToolError> {
    if (low..=high).contains(&value) {
        Ok(value)
    } else {
        fail(format!("{name} is {low} to {high}, not {value}"))
    }
}

fn reactive(ctx: &mut CallCtx, a: Args) -> Result<Output, ToolError> {
    let video = node_id(&a.video)?;
    let node = node_id(&a.node)?;
    let (fps, video_ms) = match ctx.ws.session.document().video(&video) {
        Some(v) => (v.fps, v.duration_ms()),
        None => {
            return fail(format!(
                "video {video} does not exist; list_videos shows them"
            ));
        }
    };
    let band = match a.band.as_deref() {
        None => Band::Level,
        Some(name) => Band::parse(name)
            .ok_or_else(|| ToolError(format!("band is level, low, mid or high, not {name:?}")))?,
    };
    let kps = a.keys_per_second.unwrap_or(DEFAULT_KEYS_PER_SECOND);
    if !(2..=MAX_KEYS_PER_SECOND).contains(&kps) {
        return fail(format!(
            "keys_per_second is 2 to {MAX_KEYS_PER_SECOND}, not {kps}"
        ));
    }
    let floor = check_unit("floor", a.floor.unwrap_or(DEFAULT_FLOOR), 0.0, 0.9)?;
    let attack = check_unit("attack", a.attack.unwrap_or(DEFAULT_ATTACK), 0.05, 1.0)?;
    let release = check_unit("release", a.release.unwrap_or(DEFAULT_RELEASE), 0.05, 1.0)?;
    let (min, max) = (a.min.unwrap_or(0.0), a.max.unwrap_or(1.0));
    if !min.is_finite() || !max.is_finite() {
        return fail("min and max must be numbers");
    }
    let start_ms = a.start_ms.unwrap_or(0);
    if video_ms == 0 || start_ms >= video_ms {
        return fail(format!(
            "start_ms {start_ms} is not inside the video, which is {video_ms} ms long"
        ));
    }
    let dir = ctx
        .ws
        .dir()
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder, so no media/".into()))?;
    let path = if od_core::media::audio_name(&a.file).is_ok() {
        od_media::audio_path(&dir, &a.file)
    } else if od_core::media::footage_name(&a.file).is_ok() {
        od_media::footage_path(&dir, &a.file)
    } else {
        return fail(format!(
            "{:?} is not an audio or footage file in media/ (.mp3 .wav .m4a .aac .ogg .mp4 .mov .m4v)",
            a.file
        ));
    }
    .map_err(|e| ToolError(e.to_string()))?;
    let ffmpeg = od_media::Ffmpeg::locate().map_err(|e| ToolError(e.to_string()))?;
    let file_ms = ffmpeg
        .probe_duration(&path)
        .map_err(|e| ToolError(e.to_string()))?;
    let from_ms = a.from_ms.unwrap_or(0);
    let to_ms = a
        .to_ms
        .unwrap_or(file_ms)
        .min(file_ms)
        .min(from_ms + (video_ms - start_ms));
    if from_ms >= to_ms {
        return fail(format!(
            "the range {from_ms} to {to_ms} ms of {} is empty; it is {file_ms} ms long",
            a.file
        ));
    }

    let step = (u64::from(fps) / u64::from(kps)).max(1);
    let step_ms = step as f64 * 1000.0 / f64::from(fps);
    let expected = ((to_ms - from_ms) as f64 / step_ms).ceil() as usize;
    if expected > MAX_KEYS {
        let seconds = (to_ms - from_ms) as f64 / 1000.0;
        let fits = (MAX_KEYS as f64 / seconds).floor() as u32;
        let advice = if fits >= 2 {
            format!("use keys_per_second {fits} or less, or a shorter range")
        } else {
            "use a shorter range".to_string()
        };
        return fail(format!(
            "{} ms at {kps} keys a second is {expected} keys and a track holds at most {MAX_KEYS}: {advice}",
            to_ms - from_ms
        ));
    }
    let envelope = ffmpeg
        .envelope(
            &path,
            band,
            from_ms,
            to_ms,
            step_ms.round().max(10.0) as u32,
        )
        .map_err(|e| ToolError(e.to_string()))?;
    let curve = normalize(&envelope, floor, attack, release);
    let start_frame = (start_ms * u64::from(fps)).div_ceil(1000);
    let keys = keys_of(&curve, min, max, start_frame, step, fps);
    if keys.len() < 2 || keys.iter().all(|(_, v)| (v - keys[0].1).abs() < FLAT) {
        return fail(
            "the sound has no movement in it to follow (it is silent, or flat): try another band or range",
        );
    }
    let peak_at = curve
        .iter()
        .enumerate()
        .fold(
            (0usize, 0.0f32),
            |best, (i, &c)| if c > best.1 { (i, c) } else { best },
        )
        .0;
    let op: od_core::Operation = serde_json::from_value(json!({
        "op": "animate", "video": video, "node": node, "prop": a.prop,
        "keys": keys.iter().map(|(t, v)| json!({"at_ms": t, "value": v, "ease": "linear"})).collect::<Vec<_>>(),
        "additive": a.additive,
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
    let mut receipt: Value = match out.content.first() {
        Some(Content::Text(t)) => serde_json::from_str(t).unwrap_or(Value::Null),
        _ => Value::Null,
    };
    if let Some(r) = receipt.as_object_mut() {
        r.insert(
            "audio_reactive".into(),
            json!({
                "file": a.file, "band": a.band.as_deref().unwrap_or("level"),
                "keys": keys.len(), "first_ms": keys[0].0, "last_ms": keys[keys.len() - 1].0,
                "peak_at_ms": frame_ms(start_frame + peak_at as u64 * step, fps),
                "range": [min, max],
                "next": "video_strip a loud and a quiet stretch to see it move, and watch for a track that fights another animation of the same property",
            }),
        );
    }
    Ok(Output::json(&receipt))
}

#[cfg(test)]
mod tests;
