//! `animate_keyframes`: CSS-style percentage keyframes over several properties of one node,
//! turned into ordinary keys. A keyframe's `ease` shapes the move that leaves it (the CSS and
//! anime.js rule); OpenDesigner's own key eases shape the move that arrives, so the eases shift
//! one key along.

use od_core::animation::{AnimProp, Key, Repeat};
use od_core::timebase::snap_within;
use od_core::video::MAX_VIDEO_MS;
use od_core::{Ease, NodeId, Rgba};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::scene_motion::Track;
use crate::scene_tool::merge_output;
use crate::tools::{ApplyArgs, CallCtx, Output, ToolError, apply_ops};

use super::{check_span, mover, track_ops, value_of, video_of};

const MAX_KEYFRAMES: usize = 60;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyframesArgs {
    video: String,
    #[serde(default)]
    node: Option<String>,
    keyframes: Map<String, Value>,
    #[serde(default)]
    at_ms: u64,
    duration_ms: u64,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    allow_over: bool,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// One keyframe's value for a property, with the ease the keyframe names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub percent: f64,
    pub value: f64,
    pub ease: Option<Ease>,
}

/// `"40%"`, `"from"` and `"to"` as a percentage.
pub fn parse_percent(name: &str) -> Result<f64, ToolError> {
    let bad = || {
        ToolError(format!(
            "keyframe {name:?} is not a percentage like \"40%\" (or from, to)"
        ))
    };
    let text = name.trim();
    let percent = match text {
        "from" => 0.0,
        "to" => 100.0,
        _ => text
            .strip_suffix('%')
            .and_then(|n| n.trim().parse::<f64>().ok())
            .ok_or_else(bad)?,
    };
    if !percent.is_finite() || percent < 0.0 {
        return Err(bad());
    }
    Ok(percent)
}

/// The keys for one property, from its keyframes. A first keyframe after 0% starts the property
/// from `current`. Each key's ease is the one the keyframe before it names, or `default`.
pub fn percent_keys(
    start_ms: u64,
    duration_ms: u64,
    entries: &[Entry],
    current: f64,
    default: Ease,
) -> Vec<Key> {
    let mut points: Vec<Entry> = entries.to_vec();
    points.sort_by(|a, b| a.percent.total_cmp(&b.percent));
    if points.first().is_some_and(|p| p.percent > 0.0) {
        points.insert(
            0,
            Entry {
                percent: 0.0,
                value: current,
                ease: None,
            },
        );
    }
    points
        .iter()
        .enumerate()
        .map(|(i, p)| Key {
            at_ms: start_ms + (duration_ms as f64 * p.percent / 100.0).round() as u64,
            value: p.value,
            ease: if i == 0 {
                Ease::Linear
            } else {
                points[i - 1].ease.unwrap_or(default)
            },
        })
        .collect()
}

fn value_of_json(prop: AnimProp, v: &Value) -> Result<f64, ToolError> {
    if prop == AnimProp::FillColor {
        if let Some(hex) = v.as_str() {
            return Ok(Rgba::parse_hex(hex)?.pack());
        }
    }
    v.as_f64()
        .filter(|n| n.is_finite())
        .ok_or_else(|| ToolError(format!("{} takes a number, not {v}", prop.as_str())))
}

fn prop_named(name: &str) -> Result<AnimProp, ToolError> {
    serde_json::from_value::<AnimProp>(Value::String(name.to_string())).map_err(|_| {
        ToolError(format!(
            "{name:?} is not an animatable property; use one of {}",
            AnimProp::ALL.map(AnimProp::as_str).join(", ")
        ))
    })
}

/// The keyframes as entries per property, in the order the properties are first named.
fn entries_by_prop(
    keyframes: &Map<String, Value>,
    allow_over: bool,
) -> Result<Vec<(AnimProp, Vec<Entry>)>, ToolError> {
    if keyframes.is_empty() || keyframes.len() > MAX_KEYFRAMES {
        return Err(ToolError(format!(
            "give 1 to {MAX_KEYFRAMES} keyframes, such as {{\"0%\": {{\"x\": 0}}, \"100%\": {{\"x\": 400}}}}"
        )));
    }
    let mut seen: Vec<f64> = Vec::new();
    let mut out: Vec<(AnimProp, Vec<Entry>)> = Vec::new();
    for (name, body) in keyframes {
        let percent = parse_percent(name)?;
        if percent > 100.0 && !allow_over {
            return Err(ToolError(format!(
                "keyframe {name} is past 100%; pass allow_over to let the move run longer than duration_ms"
            )));
        }
        if seen.contains(&percent) {
            return Err(ToolError(format!("two keyframes are at {percent}%")));
        }
        seen.push(percent);
        let body = body
            .as_object()
            .ok_or_else(|| ToolError(format!("keyframe {name} must be an object of properties")))?;
        let ease = body
            .get("ease")
            .map(|e| {
                e.as_str()
                    .ok_or_else(|| ToolError(format!("keyframe {name}: ease must be a string")))
                    .and_then(crate::sequence::parse_ease)
            })
            .transpose()?;
        for (key, value) in body.iter().filter(|(k, _)| k.as_str() != "ease") {
            let prop = prop_named(key)?;
            let entry = Entry {
                percent,
                value: value_of_json(prop, value)?,
                ease,
            };
            match out.iter_mut().find(|(p, _)| *p == prop) {
                Some((_, list)) => list.push(entry),
                None => out.push((prop, vec![entry])),
            }
        }
    }
    if out.is_empty() {
        return Err(ToolError("the keyframes name no property".into()));
    }
    Ok(out)
}

pub fn call(ctx: &mut CallCtx, a: KeyframesArgs) -> Result<Output, ToolError> {
    let doc = ctx.ws.session.document();
    let video = video_of(doc, &a.video)?;
    check_span(a.at_ms, a.duration_ms)?;
    let entries = entries_by_prop(&a.keyframes, a.allow_over)?;
    let default = a.ease.unwrap_or(Ease::EaseInOut);
    let node = if entries.iter().any(|(p, _)| !p.is_camera()) {
        let id = a
            .node
            .as_deref()
            .ok_or_else(|| ToolError("these properties need `node`".into()))?;
        Some(mover(doc, id)?)
    } else {
        None
    };
    let mut ops = Vec::new();
    let mut report = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    for (prop, list) in &entries {
        let current = match (prop.is_camera(), node) {
            (true, _) => {
                let camera = video.camera_at(a.at_ms as f64);
                match prop {
                    AnimProp::CameraX => camera.x,
                    AnimProp::CameraY => camera.y,
                    AnimProp::CameraZoom => camera.zoom,
                    _ => camera.rotation,
                }
            }
            (false, Some(n)) => {
                if prop.read(n).is_none() {
                    return Err(ToolError(format!(
                        "{} has no {} to animate",
                        n.id,
                        prop.as_str()
                    )));
                }
                value_of(video, n, *prop, a.at_ms, 0.0)
            }
            (false, None) => 0.0,
        };
        let keys = percent_keys(a.at_ms, a.duration_ms, list, current, default);
        if let Some(last) = list
            .iter()
            .max_by(|x, y| x.percent.total_cmp(&y.percent))
            .filter(|l| l.ease.is_some())
        {
            notes.push(format!(
                "the ease on the last {} keyframe ({}%) is unused: a keyframe's ease shapes the move that leaves it",
                prop.as_str(),
                last.percent
            ));
        }
        if keys.last().is_some_and(|k| k.at_ms > MAX_VIDEO_MS) {
            return Err(ToolError(format!(
                "the keyframes of {} run past {MAX_VIDEO_MS} ms",
                prop.as_str()
            )));
        }
        let target: NodeId = if prop.is_camera() {
            video.id.clone()
        } else {
            node.map(|n| n.id.clone())
                .ok_or_else(|| ToolError("these properties need `node`".into()))?
        };
        let frames: Vec<u64> = keys
            .iter()
            .map(|k| snap_within(k.at_ms, video.fps, 0, MAX_VIDEO_MS).unwrap_or(k.at_ms))
            .collect();
        if frames.windows(2).any(|w| w[0] >= w[1]) {
            return Err(ToolError(format!(
                "two {} keyframes land on the same frame; space them further apart or lengthen duration_ms",
                prop.as_str()
            )));
        }
        let track = Track {
            prop: *prop,
            keys,
            repeat: Repeat::Once,
        };
        ops.extend(track_ops(video, &target, vec![track]));
    }
    for op in &ops {
        if let od_core::Operation::Animate { prop, keys, .. } = op {
            report.push(json!({
                "prop": prop.as_str(),
                "keys": keys.iter().map(|k| json!({"at_ms": k.at_ms, "value": k.value, "ease": k.ease.to_string()})).collect::<Vec<_>>(),
            }));
        }
    }
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: a.summary,
        },
    )?;
    let mut extra = json!({ "tracks": report });
    if !notes.is_empty() {
        extra["note"] = json!(notes.join("; "));
    }
    Ok(merge_output(out, extra))
}
