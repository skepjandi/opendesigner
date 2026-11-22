//! Motion along curves and between shapes (`docs/wiki/video-editing.md#paths-and-morphing`):
//! `animate_arc` (a node travels on a curve instead of a straight line, and `arc` bends any move
//! that travels in x and y), `animate_keyframes` (CSS-style percentage keyframes over several
//! properties), `text_on_path` (a line of text laid along a path, a letter per node) and
//! `morph_to` (one path node turns into another).
//!
//! All four write ordinary keyframes (and, for a morph, the shapes a path node morphs through),
//! so every change is attributed, checkpointed and undoable in one step.

pub mod arc;
mod keyframes;
mod morph;
#[cfg(test)]
mod tests;
mod text;

use od_core::animation::{AnimProp, Key, Repeat};
use od_core::timebase::snap_within;
use od_core::video::{MAX_VIDEO_MS, Video};
use od_core::{Document, Ease, Node, NodeId, NodeKind, Operation};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::scene_motion::Track;
use crate::scene_tool::merge_output;
use crate::tools::{ApplyArgs, CallCtx, Output, ToolError, apply_ops, node_id, parse, tool};
use arc::{ArcSpec, Pt};

pub const NAMES: [&str; 4] = [
    "animate_arc",
    "animate_keyframes",
    "text_on_path",
    "morph_to",
];

const MIN_MS: u64 = 100;
const MAX_MS: u64 = 600_000;

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    let turn = id("Groups related calls into one turn; omit and each call is its own turn.");
    let summary = id("One line, in the user's terms, naming what this turn does.");
    let ease = |d: &str| {
        json!({"type": "string", "description": format!(
            "{d} One of {} or a tuned one: {}.", Ease::NAMES.join(", "), od_core::ease::GRAMMAR)})
    };
    let point = |d: &str| {
        json!({
            "description": d,
            "oneOf": [
                {"type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2},
                {"type": "object", "properties": {"x": {"type": "number"}, "y": {"type": "number"}}, "required": ["x", "y"]}
            ]
        })
    };
    vec![
        tool(
            "animate_arc",
            "Move a node from one point to another along an arc instead of a straight line: a \
             quadratic Bezier whose control point is the start plus the run times `peak` plus the \
             perpendicular times `strength` times the distance, sampled into ordinary x and y keys \
             every ~33 ms on the video's frame grid. The bulge stays on one side whichever way the \
             node goes (up by default). `from` and `to` are the node's x and y (its top left). \
             `orient` also turns the node to face along the curve (rotation by the heading, \
             unwrapped, so it never jumps by a turn). `ease` shapes how far along the curve the \
             node is at each moment. Keys outside the move are kept, so moves chain. The same \
             `arc` object on animate_preset bends any scene move that travels in x and y.",
            json!({
                "video": id("Id of a video from list_videos."),
                "node": id("Id of the node to move."),
                "from": point("Where the move starts, [x, y] or {x, y} (default: where the node is at at_ms)."),
                "to": point("Where the move ends, [x, y] or {x, y}."),
                "at_ms": {"type": "integer", "minimum": 0, "description": "When the move starts."},
                "duration_ms": {"type": "integer", "minimum": MIN_MS, "maximum": MAX_MS, "description": "How long it runs."},
                "ease": ease("How far along the curve the node is over time (default ease_in_out)."),
                "arc": arc::schema(),
                "orient": {"type": "boolean", "description": "Turn the node to face along the curve."},
                "turn": turn.clone(),
                "summary": summary.clone()
            }),
            &["video", "node", "to", "at_ms", "duration_ms"],
            false,
        ),
        tool(
            "animate_keyframes",
            "CSS-style percentage keyframes over several properties of one node, written as \
             ordinary keys. `keyframes` maps '0%', '40%', '100%' (also 'from' and 'to') to the \
             values the properties have there: {\"0%\": {\"x\": 0, \"y\": 0}, \"40%\": {\"x\": 200, \
             \"y\": -80, \"ease\": \"ease_out\"}, \"100%\": {\"x\": 400, \"y\": 0}}. Each \
             property has its own keys: a keyframe that leaves a property out does not touch it. \
             A keyframe's `ease` shapes the move that LEAVES it (as in CSS); the last keyframe's \
             ease is unused (the result says so). A property whose first keyframe is after 0% \
             starts from its current value and goes there over the time before it. Percentages \
             are of duration_ms; past 100% needs `allow_over`. Camera properties (camera_x...) \
             need no node. fill_color takes '#rrggbb'.",
            json!({
                "video": id("Id of a video from list_videos."),
                "node": id("Id of the node (not needed when only camera properties are keyed)."),
                "keyframes": {"type": "object", "description": "Percentage -> {property: value, ease?}, e.g. {\"0%\": {\"x\": 0}, \"100%\": {\"x\": 400}}.", "additionalProperties": {"type": "object"}},
                "at_ms": {"type": "integer", "minimum": 0, "description": "When 0% is (default 0)."},
                "duration_ms": {"type": "integer", "minimum": MIN_MS, "maximum": MAX_MS, "description": "How long 100% takes."},
                "ease": ease("The ease of a segment whose leaving keyframe names none (default ease_in_out)."),
                "allow_over": {"type": "boolean", "description": "Allow percentages past 100% (the move runs longer than duration_ms)."},
                "turn": turn.clone(),
                "summary": summary.clone()
            }),
            &["video", "keyframes", "duration_ms"],
            false,
        ),
        tool(
            "text_on_path",
            "Lay a line of text along a path node's curve: each glyph (the renderer's own shaping \
             and advances, as split_text uses) becomes a text node placed by arc length and \
             turned to the curve's tangent, standing on it. The pieces are a group named like the \
             text, in reading order, spaces keeping their width but not being nodes. `align` is \
             start (default), center or end along the path; `start_offset` shifts the start in \
             document units. A closed path wraps; on an open one a glyph that would fall off the \
             end is left out (the result counts them). `reveal` brings the letters in one by one \
             with a scene move (default pop_in, 40 ms apart, from `video`); animate the group \
             afterwards. The original is removed (`mode: replace`) or hidden (`keep`). Place and \
             style the path node first: the glyphs follow its flattened curve in the text's \
             parent's coordinates (an ancestor's rotation or scale is not applied).",
            json!({
                "text_node": id("Id of the text node."),
                "path_node": id("Id of the path node to follow."),
                "start_offset": {"type": "number", "description": "Document units from where align puts the start (default 0)."},
                "align": {"type": "string", "enum": ["start", "center", "end"], "description": "Where the text sits along the path (default start)."},
                "mode": {"type": "string", "enum": ["keep", "replace"], "description": "keep hides the original text; replace (default) removes it and the group takes its name."},
                "reveal": {
                    "type": "object",
                    "additionalProperties": false,
                    "description": "Bring the letters in one by one.",
                    "properties": {
                        "video": id("Id of a video from list_videos."),
                        "preset": id("The scene move for each letter: pop_in (default), fade_up, zoom_in..."),
                        "each_ms": {"type": "integer", "minimum": 0, "maximum": 5000, "description": "Milliseconds from one letter to the next (default 40)."},
                        "at_ms": {"type": "integer", "minimum": 0, "description": "When the first letter starts (default 0)."},
                        "duration_ms": {"type": "integer", "minimum": MIN_MS, "maximum": MAX_MS, "description": "How long each letter takes (the move's own default)."},
                        "ease": ease("Overrides the move's ease.")
                    },
                    "required": ["video"]
                },
                "turn": turn.clone(),
                "summary": summary.clone()
            }),
            &["text_node", "path_node"],
            false,
        ),
        tool(
            "morph_to",
            "Turn one path node into another over time. Both outlines are cut by arc length into \
             the same number of points (about one per 3 document units, at least 16, at most \
             256); two closed shapes get matching windings and the start point that moves the \
             points least, so the shape does not twist; contours are paired largest first and one \
             with no partner shrinks to its middle (or grows from it). The node keeps its own \
             path data and gains the shapes it morphs through (`path_morph`) and a \
             `morph_progress` animation: 0 is its own shape, 1 the target's, drawn as a polyline \
             of those points, so it looks the same everywhere frames are made. The target stays \
             where it is: the node's shape lands on the target's place. Calling morph_to again \
             on the same node (starting after the first ends) continues the morph to a third \
             shape, with the same contours. The morph does not follow later edits of the node's \
             path: run it again (with `reset`) after changing it. `ease` shapes the progress.",
            json!({
                "video": id("Id of a video from list_videos."),
                "node": id("Id of the path node that morphs."),
                "to_path": id("Id of the path node whose shape it becomes."),
                "at_ms": {"type": "integer", "minimum": 0, "description": "When the morph starts."},
                "duration_ms": {"type": "integer", "minimum": MIN_MS, "maximum": MAX_MS, "description": "How long it runs."},
                "ease": ease("How the morph progresses over time (default ease_in_out)."),
                "reset": {"type": "boolean", "description": "Start over from the node's own path, dropping an earlier morph and its progress keys."},
                "turn": turn,
                "summary": summary
            }),
            &["video", "node", "to_path", "at_ms", "duration_ms"],
            false,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "animate_arc" => animate_arc(ctx, parse(args)?),
        "animate_keyframes" => keyframes::call(ctx, parse(args)?),
        "text_on_path" => text::call(ctx, parse(args)?),
        "morph_to" => morph::call(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

/// A point as an argument gives it: `[x, y]` or `{x, y}`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum Point {
    Pair([f64; 2]),
    Named { x: f64, y: f64 },
}

impl Point {
    fn get(self) -> Pt {
        match self {
            Point::Pair([x, y]) => (x, y),
            Point::Named { x, y } => (x, y),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArcArgs {
    video: String,
    node: String,
    #[serde(default)]
    from: Option<Point>,
    to: Point,
    at_ms: u64,
    duration_ms: u64,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    arc: Option<ArcSpec>,
    #[serde(default)]
    orient: Option<bool>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// The video a call names.
fn video_of<'a>(doc: &'a Document, id: &str) -> Result<&'a Video, ToolError> {
    doc.video(&node_id(id)?)
        .ok_or_else(|| ToolError(format!("no video {id}")))
}

/// The node a call names, one a move can act on.
fn mover<'a>(doc: &'a Document, id: &str) -> Result<&'a Node, ToolError> {
    let node = doc
        .node(&node_id(id)?)
        .ok_or_else(|| ToolError(format!("no node {id}")))?;
    if node.kind == NodeKind::Frame {
        return Err(ToolError(format!(
            "{id} is a frame: move a node inside the frame"
        )));
    }
    Ok(node)
}

/// What `prop` of `node` is at `at_ms`: its track's value there, or its own.
fn value_of(video: &Video, node: &Node, prop: AnimProp, at_ms: u64, fallback: f64) -> f64 {
    video
        .base_track(&node.id, prop)
        .and_then(|a| a.value_at(at_ms as f64))
        .or_else(|| prop.read(node))
        .unwrap_or(fallback)
}

fn check_span(at_ms: u64, duration_ms: u64) -> Result<(), ToolError> {
    if !(MIN_MS..=MAX_MS).contains(&duration_ms) || at_ms + duration_ms > MAX_VIDEO_MS {
        return Err(ToolError(format!(
            "at_ms {at_ms} and duration_ms {duration_ms} must give a move of {MIN_MS} to {MAX_MS} ms that ends by {MAX_VIDEO_MS} ms"
        )));
    }
    Ok(())
}

/// `animate` operations for tracks of `node`: the keys on the video's frame grid, and a track
/// that plays once keeps the keys the node already had outside its stretch of time, so moves chain.
pub(crate) fn track_ops(video: &Video, node: &NodeId, tracks: Vec<Track>) -> Vec<Operation> {
    tracks
        .into_iter()
        .map(|mut t| {
            for k in &mut t.keys {
                k.at_ms = snap_within(k.at_ms, video.fps, 0, MAX_VIDEO_MS).unwrap_or(k.at_ms);
            }
            t.keys.dedup_by_key(|k| k.at_ms);
            let (keys, repeat) = match video.base_track(node, t.prop) {
                Some(e) if t.repeat == Repeat::Once => {
                    let first = t.keys.first().map_or(0, |k| k.at_ms);
                    let last = t.keys.last().map_or(0, |k| k.at_ms);
                    let mut keys: Vec<Key> = e
                        .keys
                        .iter()
                        .filter(|k| k.at_ms < first || k.at_ms > last)
                        .copied()
                        .collect();
                    keys.extend(t.keys);
                    keys.sort_by_key(|k| k.at_ms);
                    (keys, e.repeat)
                }
                _ => (t.keys, t.repeat),
            };
            Operation::Animate {
                additive: false,
                video: video.id.clone(),
                node: node.clone(),
                prop: t.prop,
                keys,
                repeat,
            }
        })
        .collect()
}

fn animate_arc(ctx: &mut CallCtx, a: ArcArgs) -> Result<Output, ToolError> {
    let doc = ctx.ws.session.document();
    let video = video_of(doc, &a.video)?;
    let node = mover(doc, &a.node)?;
    check_span(a.at_ms, a.duration_ms)?;
    let from = a.from.map(Point::get).unwrap_or_else(|| {
        (
            value_of(video, node, AnimProp::X, a.at_ms, node.x),
            value_of(video, node, AnimProp::Y, a.at_ms, node.y),
        )
    });
    let to = a.to.get();
    let curve = arc::curve(from, to, &a.arc.unwrap_or_default())?;
    let ease = a.ease.unwrap_or(Ease::EaseInOut);
    let samples = arc::sample_times(a.at_ms, a.duration_ms, video.fps);
    let progress: Vec<f64> = samples
        .iter()
        .map(|(_, f)| ease.apply_in(*f, a.duration_ms as f64))
        .collect();
    let orient = a
        .orient
        .unwrap_or(false)
        .then(|| value_of(video, node, AnimProp::Rotation, a.at_ms, 0.0));
    let tracks = arc::tracks(&curve, &samples, &progress, orient);
    let ops = track_ops(video, &node.id, tracks);
    let (start, end) = (
        samples.first().map_or(a.at_ms, |s| s.0),
        samples.last().map_or(a.at_ms, |s| s.0),
    );
    let report = json!({
        "node": node.id.to_string(),
        "from": [from.0, from.1],
        "to": [to.0, to.1],
        "control": [curve.control.0, curve.control.1],
        "start_ms": start,
        "end_ms": end,
        "keys": samples.len(),
        "orient": a.orient.unwrap_or(false),
    });
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: a.summary,
        },
    )?;
    Ok(merge_output(out, report))
}
