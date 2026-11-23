//! `animate_noise`: organic motion (a drift, a shake, a breathing camera, a flicker) written as
//! ordinary keyframes from smooth seeded noise (`od_core::noise`;
//! `docs/wiki/video-editing.md#noise-motion`). One attributed, undoable step.

use od_core::animation::MAX_KEYS;
use od_core::noise::{MAX_OCTAVES, Noise};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::audio_reactive::frame_ms;
use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops, node_id, parse, tool,
};

pub const NAMES: [&str; 1] = ["animate_noise"];

const DEFAULT_FREQUENCY: f64 = 1.5;
const DEFAULT_KEYS_PER_SECOND: u32 = 15;
const MAX_KEYS_PER_SECOND: u32 = 30;
const MAX_PROPS: usize = 4;
/// Streams of one call differ by this much in seed, so x and y do not move in step.
const SEED_STRIDE: u32 = 7_919;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub fn definitions() -> Vec<Value> {
    vec![tool(
        "animate_noise",
        "Organic, never-repeating motion on one node: smooth seeded noise written as keyframes, \
         `amplitude` big around `center`. Examples: a handheld shake is props [x, y] with amplitude \
         6 and frequency 6; a drifting cloud is x with amplitude 40 and frequency 0.2; a breathing \
         glow is opacity with amplitude 0.15, center 0.85 and `additive: false`; a camera sway is \
         camera_x and camera_y (node = the video's own id). The same seed writes the same curve on \
         every machine; `props` takes up to 4 properties, each with its own stream. `additive` \
         (default true) adds the wobble to wherever the node otherwise is, so it rides on a \
         travel; with false the values are absolute, around `center`. `loop: true` makes the track \
         end where it began and repeat, so a clip can loop without a jump. Replaces that node's \
         track for the property (of the same kind). At most 500 keys a track. Check with \
         video_strip.",
        json!({
            "video": {"type": "string", "description": "Id of a video from list_videos."},
            "node": {"type": "string", "description": "Id of the node (the video's own id for camera_* properties)."},
            "prop": {"type": "string", "description": "One property, as animate takes it: x, y, rotation, scale, opacity, blur, camera_x ..."},
            "props": {"type": "array", "maxItems": MAX_PROPS, "items": {"type": "string"}, "description": "Several properties at once, each with its own noise."},
            "amplitude": {"type": "number", "description": "How far from the center the value goes (in the property's own unit)."},
            "center": {"type": "number", "description": "The value the noise moves around (default 0, right for additive)."},
            "frequency": {"type": "number", "minimum": 0.05, "maximum": 20, "description": "About how many times a second the value changes direction (default 1.5)."},
            "octaves": {"type": "integer", "minimum": 1, "maximum": MAX_OCTAVES, "description": "Layers of finer detail (default 2)."},
            "seed": {"type": "integer", "minimum": 0, "description": "The same seed gives the same motion (default 1)."},
            "from_ms": {"type": "integer", "minimum": 0, "description": "Start on the video's timeline (default 0)."},
            "to_ms": {"type": "integer", "minimum": 1, "description": "End (default: the end of the video)."},
            "keys_per_second": {"type": "integer", "minimum": 2, "maximum": MAX_KEYS_PER_SECOND, "description": "How often a key is written (default 15)."},
            "additive": {"type": "boolean", "description": "Add to the node's own value (default true)."},
            "loop": {"type": "boolean", "description": "Seamless loop over from_ms to to_ms, repeating for the rest of the video."},
            "turn": {"type": "string"},
            "summary": {"type": "string"}
        }),
        &["video", "node", "amplitude"],
        false,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "animate_noise" => animate(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    video: String,
    node: String,
    #[serde(default)]
    prop: Option<String>,
    #[serde(default)]
    props: Option<Vec<String>>,
    amplitude: f64,
    #[serde(default)]
    center: Option<f64>,
    #[serde(default)]
    frequency: Option<f64>,
    #[serde(default)]
    octaves: Option<u32>,
    #[serde(default)]
    seed: Option<u32>,
    #[serde(default)]
    from_ms: Option<u64>,
    #[serde(default)]
    to_ms: Option<u64>,
    #[serde(default)]
    keys_per_second: Option<u32>,
    #[serde(default)]
    additive: Option<bool>,
    #[serde(default, rename = "loop")]
    looping: bool,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// The keys of `noise` over `from_ms..=to_ms` on the frame grid, every `step` frames.
fn noise_keys(
    noise: &Noise,
    amplitude: f64,
    center: f64,
    from_ms: u64,
    to_ms: u64,
    step: u64,
    fps: u32,
) -> Vec<(u64, f64)> {
    let first = (from_ms * u64::from(fps)).div_ceil(1000);
    let mut keys: Vec<(u64, f64)> = (0..)
        .map(|i| frame_ms(first + i * step, fps))
        .take_while(|t| *t <= to_ms)
        .map(|t| {
            let seconds = (t - from_ms) as f64 / 1000.0;
            let value = center + amplitude * noise.at(seconds);
            (t, (value * 1000.0).round() / 1000.0)
        })
        .collect();
    let head = keys.first().map(|k| k.1);
    if noise.loop_seconds.is_some()
        && let (Some(head), Some(last)) = (head, keys.last_mut())
    {
        last.1 = head;
    }
    keys
}

fn animate(ctx: &mut CallCtx, a: Args) -> Result<Output, ToolError> {
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
    let props: Vec<String> = match (&a.prop, &a.props) {
        (Some(p), None) => vec![p.clone()],
        (None, Some(list)) if (1..=MAX_PROPS).contains(&list.len()) => list.clone(),
        (None, Some(_)) => return fail(format!("props holds 1 to {MAX_PROPS} properties")),
        _ => {
            return fail(
                "give `prop` (one property) or `props` (a list), not both and not neither",
            );
        }
    };
    if !a.amplitude.is_finite() || a.amplitude == 0.0 {
        return fail("amplitude must be a number other than 0");
    }
    let frequency = a.frequency.unwrap_or(DEFAULT_FREQUENCY);
    if !(0.05..=20.0).contains(&frequency) {
        return fail(format!("frequency is 0.05 to 20, not {frequency}"));
    }
    let octaves = a.octaves.unwrap_or(2);
    if !(1..=MAX_OCTAVES).contains(&octaves) {
        return fail(format!("octaves is 1 to {MAX_OCTAVES}, not {octaves}"));
    }
    let kps = a.keys_per_second.unwrap_or(DEFAULT_KEYS_PER_SECOND);
    if !(2..=MAX_KEYS_PER_SECOND).contains(&kps) {
        return fail(format!(
            "keys_per_second is 2 to {MAX_KEYS_PER_SECOND}, not {kps}"
        ));
    }
    let center = a.center.unwrap_or(0.0);
    if !center.is_finite() {
        return fail("center must be a number");
    }
    let (from_ms, to_ms) = (
        a.from_ms.unwrap_or(0),
        a.to_ms.unwrap_or(video_ms).min(video_ms),
    );
    if video_ms == 0 || from_ms >= to_ms {
        return fail(format!(
            "the range {from_ms} to {to_ms} ms is empty; the video is {video_ms} ms long"
        ));
    }
    let step = (u64::from(fps) / u64::from(kps)).max(1);
    let step_ms = step as f64 * 1000.0 / f64::from(fps);
    let expected = ((to_ms - from_ms) as f64 / step_ms).ceil() as usize + 1;
    if expected > MAX_KEYS {
        let fits = (MAX_KEYS as f64 / ((to_ms - from_ms) as f64 / 1000.0)).floor() as u32;
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
    let seed = a.seed.unwrap_or(1);
    let loop_seconds = a.looping.then(|| (to_ms - from_ms) as f64 / 1000.0);
    let additive = a.additive.unwrap_or(true);
    let mut ops = Vec::new();
    let mut key_count = 0;
    let mut used_frequency = frequency;
    for (i, prop) in props.iter().enumerate() {
        let noise = Noise {
            seed: seed.wrapping_add(SEED_STRIDE.wrapping_mul(i as u32)),
            frequency,
            octaves,
            loop_seconds,
        };
        used_frequency = noise.effective_frequency();
        let keys = noise_keys(&noise, a.amplitude, center, from_ms, to_ms, step, fps);
        key_count = keys.len();
        let mut op = json!({
            "op": "animate", "video": video, "node": node, "prop": prop,
            "keys": keys.iter().map(|(t, v)| json!({"at_ms": t, "value": v, "ease": "linear"})).collect::<Vec<_>>(),
            "additive": additive,
        });
        if a.looping {
            op["repeat"] = json!("loop");
        }
        ops.push(
            serde_json::from_value::<od_core::Operation>(op)
                .map_err(|e| ToolError(format!("invalid arguments: {e}")))?,
        );
    }
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops,
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
            "noise".into(),
            json!({
                "props": props, "keys_each": key_count, "seed": seed,
                "frequency_used": used_frequency, "octaves": octaves,
                "from_ms": from_ms, "to_ms": to_ms, "additive": additive, "loop": a.looping,
                "next": "video_strip a second of it: a shake wants frequency 4 to 8, a drift 0.1 to 0.5, and amplitude small enough that nothing leaves the frame",
            }),
        );
    }
    Ok(Output::json(&receipt))
}

#[cfg(test)]
mod tests;
