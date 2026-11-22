//! `animate_to`: send one property to a value from wherever it is at a moment, without a jump
//! (`docs/wiki/video-editing.md#layers-and-implicit-from`). The new tween starts from the value
//! the node's track gives at `at_ms` (the node's own value without a track) and replaces what the
//! old track did from then on, so a second move can interrupt a first.
//!
//! The additive layer itself lives in `od_core::animation` (`Animation::additive`); this module
//! holds the replace-splice both this tool and `transition_layout` use.

use od_core::animation::{AnimProp, Animation, Key, Repeat};
use od_core::timebase::snap;
use od_core::video::MAX_VIDEO_MS;
use od_core::{Camera, Document, Ease, NodeId, Operation, Rgba};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ApplyArgs, CallCtx, Output, ToolError, apply_ops_receipt, node_id, parse, tool,
};

pub const NAMES: [&str; 1] = ["animate_to"];

/// Whether a move writes the node's own track or an offset track laid over it.
#[derive(Debug, Deserialize, Default, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    #[default]
    Replace,
    Additive,
}

/// Linear pieces a cut segment that is not linear is baked into, so it still reaches where the
/// old curve was when the new tween takes over.
const BAKE_PIECES: u64 = 8;
const MIN_TWEEN_MS: u64 = 1;

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    vec![tool(
        "animate_to",
        "Send one property of a node to a value over a stretch of time, starting from the value it \
         has at at_ms: what its track gives there, or the node's own value when it has none. There is \
         no jump when it is interrupted. The keys the node's track has from at_ms on are replaced \
         (keep_later keeps those after the new move ends), a segment of the old track that is cut \
         in the middle keeps its shape (a non-linear one is baked into 8 linear pieces), and the \
         result is one ordinary animate, so it is attributed, checkpointed and undoable in one \
         step. Returns the value it started from and how many old keys it replaced. A track that \
         loops (repeat) is refused: use apply_ops animate. The additive layer is untouched; \
         add one with apply_ops animate additive:true or animate_preset layer:additive. A colour \
         prop (fill_color) takes a '#rrggbb[aa]' string. A camera prop needs no node.",
        json!({
            "video": id("Id of a video from list_videos."),
            "node": id("Id of the node (omit for the camera_* props)."),
            "prop": {"type": "string", "enum": AnimProp::ALL.iter().map(|p| p.as_str()).collect::<Vec<_>>()},
            "to": {"description": "The value to reach: a number, or a '#rrggbb[aa]' colour for fill_color."},
            "at_ms": {"type": "integer", "minimum": 0, "description": "When the move starts, from the start of the video."},
            "duration_ms": {"type": "integer", "minimum": 1, "description": "How long the move takes."},
            "ease": {"type": "string", "description": format!("One of {} or a tuned one: {}. Default ease_in_out.", Ease::NAMES.join(", "), od_core::ease::GRAMMAR)},
            "keep_later": {"type": "boolean", "description": "Keep the old keys after the new move ends (default: they are replaced)."},
            "turn": id("Groups related calls into one turn."),
            "summary": id("One line, in the user's terms, naming what this turn does.")
        }),
        &["video", "prop", "to", "at_ms", "duration_ms"],
        false,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "animate_to" => animate_to(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToArgs {
    video: String,
    #[serde(default)]
    node: Option<String>,
    prop: AnimProp,
    to: Value,
    at_ms: u64,
    duration_ms: u64,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    keep_later: bool,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// What a tween asks of a track.
pub(crate) struct Tween {
    pub at_ms: u64,
    pub end_ms: u64,
    /// Where it starts; the track's own value at `at_ms` when none.
    pub from: Option<f64>,
    pub to: f64,
    pub ease: Ease,
    pub keep_later: bool,
}

pub(crate) struct Spliced {
    pub keys: Vec<Key>,
    pub from: f64,
    /// Old keys the tween replaced.
    pub replaced: usize,
}

/// The keys of `existing` (a track that replaces its value, or none) after `tween` takes over
/// from its start. `base` is the node's own value, what a node with no track shows.
pub(crate) fn splice_replace(
    existing: Option<&Animation>,
    base: f64,
    fps: u32,
    tween: &Tween,
) -> Result<Spliced, ToolError> {
    if existing.is_some_and(|a| a.repeat != Repeat::Once) {
        return Err(ToolError(
            "that track loops, so it has no single value to start from; replace it with apply_ops animate"
                .into(),
        ));
    }
    let (at, end) = (snap(tween.at_ms, fps), snap(tween.end_ms, fps));
    if end < at + MIN_TWEEN_MS || end > MAX_VIDEO_MS {
        return Err(ToolError(format!(
            "the move from {at} ms to {end} ms is empty or runs past the longest video"
        )));
    }
    let old: &[Key] = existing.map_or(&[], |a| &a.keys);
    let current = existing.and_then(|a| a.value_at(at as f64)).unwrap_or(base);
    let from = tween.from.unwrap_or(current);
    let mut keys: Vec<Key> = old.iter().copied().filter(|k| k.at_ms < at).collect();
    let exact = old.iter().find(|k| k.at_ms == at).copied();
    let cut_ease = exact.map(|k| k.ease);
    let continues = (from - current).abs() < f64::EPSILON * current.abs().max(1.0);
    if let (Some(track), Some(before), None) = (existing, keys.last().copied(), exact) {
        let straddles = old.iter().any(|k| k.at_ms > at);
        let curved = old
            .iter()
            .find(|k| k.at_ms > at)
            .is_some_and(|k| k.ease != Ease::Linear);
        if straddles && curved && continues {
            keys.extend(bake(track, before.at_ms, at, fps));
        }
    }
    keys.push(Key {
        at_ms: at,
        value: from,
        ease: cut_ease.filter(|_| continues).unwrap_or(Ease::Linear),
    });
    keys.push(Key {
        at_ms: end,
        value: tween.to,
        ease: tween.ease,
    });
    let later = old
        .iter()
        .copied()
        .filter(|k| tween.keep_later && k.at_ms > end);
    keys.extend(later.clone());
    let kept = old.iter().filter(|k| k.at_ms < at).count() + later.count();
    Ok(Spliced {
        keys,
        from,
        replaced: old.len() - kept,
    })
}

/// The stretch `lo..at` of `track`'s curve as linear keys, the one at `at` left to the caller.
fn bake(track: &Animation, lo: u64, at: u64, fps: u32) -> Vec<Key> {
    let mut out: Vec<Key> = Vec::new();
    let mut last = lo;
    for i in 1..BAKE_PIECES {
        let t = snap(lo + (at - lo) * i / BAKE_PIECES, fps);
        if t <= last || t >= at {
            continue;
        }
        if let Some(value) = track.value_at(t as f64) {
            out.push(Key {
                at_ms: t,
                value,
                ease: Ease::Linear,
            });
            last = t;
        }
    }
    out
}

/// What `prop` of `node` is when nothing plays: the node's own value, or the camera's rest.
pub(crate) fn rest_value(doc: &Document, node: &NodeId, prop: AnimProp) -> Result<f64, ToolError> {
    if prop.is_camera() {
        let c = Camera::default();
        return Ok(match prop {
            AnimProp::CameraX => c.x,
            AnimProp::CameraY => c.y,
            AnimProp::CameraZoom => c.zoom,
            _ => c.rotation,
        });
    }
    let n = doc
        .node(node)
        .ok_or_else(|| ToolError(format!("node {node} does not exist")))?;
    prop.read(n).ok_or_else(|| {
        ToolError(format!(
            "a {} node has no {} to animate",
            n.kind.as_str(),
            prop.as_str()
        ))
    })
}

fn target_value(prop: AnimProp, to: &Value) -> Result<f64, ToolError> {
    match (prop, to) {
        (AnimProp::FillColor, Value::String(hex)) => Ok(Rgba::parse_hex(hex)?.pack()),
        (AnimProp::FillColor, _) => Err(ToolError(
            "fill_color goes to a colour string like '#ff8800'".into(),
        )),
        (_, Value::Number(n)) => n
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| ToolError("`to` is not a finite number".into())),
        _ => Err(ToolError(format!("{} goes to a number", prop.as_str()))),
    }
}

fn animate_to(ctx: &mut CallCtx, a: ToArgs) -> Result<Output, ToolError> {
    let video_id = node_id(&a.video)?;
    let node = match (&a.node, a.prop.is_camera()) {
        (_, true) => video_id.clone(),
        (Some(n), false) => node_id(n)?,
        (None, false) => return Err(ToolError("animate_to needs `node`".into())),
    };
    let to = target_value(a.prop, &a.to)?;
    let doc = ctx.ws.session.document();
    let video = doc
        .video(&video_id)
        .ok_or_else(|| ToolError(format!("video {video_id} does not exist")))?;
    let base = rest_value(doc, &node, a.prop)?;
    let tween = Tween {
        at_ms: a.at_ms,
        end_ms: a.at_ms.saturating_add(a.duration_ms),
        from: None,
        to,
        ease: a.ease.unwrap_or_default(),
        keep_later: a.keep_later,
    };
    let spliced = splice_replace(video.base_track(&node, a.prop), base, video.fps, &tween)?;
    if (spliced.from - to).abs() < f64::EPSILON {
        return Err(ToolError(format!(
            "{} of {node} is already {to} at {} ms; give a different `to`",
            a.prop.as_str(),
            a.at_ms
        )));
    }
    let keys = spliced.keys.len();
    let op = Operation::Animate {
        video: video_id,
        node,
        prop: a.prop,
        keys: spliced.keys,
        repeat: Repeat::Once,
        additive: false,
    };
    let mut receipt = apply_ops_receipt(
        ctx,
        ApplyArgs {
            ops: vec![op],
            turn: a.turn,
            summary: a.summary,
        },
    )?;
    if let Some(body) = receipt.as_object_mut() {
        body.insert("from".into(), json!(spliced.from));
        body.insert("to".into(), json!(to));
        body.insert("keys".into(), json!(keys));
        body.insert("replaced_keys".into(), json!(spliced.replaced));
    }
    Ok(Output::json(&receipt))
}

#[cfg(test)]
mod tests;
