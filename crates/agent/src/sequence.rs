//! Two tools for putting beats in time: `sequence`, which places a list of moves against each
//! other with a position grammar (`<`, `<+=50`, `intro+=200`), and `spring_to`, which sends one
//! property to a value on a spring and says when it is done (`docs/wiki/video-editing.md#sequencing`).
//!
//! Both write ordinary keyframes through the same path as `animate_preset`, so every change is
//! attributed, checkpointed and undoable in one step.

use std::collections::BTreeMap;
use std::str::FromStr;

use od_core::animation::AnimProp;
use od_core::timebase::snap_within;
use od_core::video::MAX_VIDEO_MS;
use od_core::{Document, Ease, NodeId, Operation};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::scene_motion::Named;
use crate::scene_tool::distribution::{Distribution, StaggerArgs};
use crate::scene_tool::{self, merge_output};
use crate::tools::{ApplyArgs, CallCtx, Output, ToolError, apply_ops, node_id, parse, tool};

pub mod position;
pub mod spring;
#[cfg(test)]
mod tests;

pub use position::{Position, PositionError, TimelineState};

pub const NAMES: [&str; 2] = ["sequence", "spring_to"];

/// The move arguments a sequence item may carry, the ones `animate_preset` takes.
const MOVE_ARGS: [&str; 13] = [
    "duration_ms",
    "strength",
    "intensity",
    "ease",
    "repeat",
    "seed",
    "direction",
    "amount",
    "center_x",
    "center_y",
    "radius",
    "path",
    "orient",
];

const MAX_ITEMS: usize = 60;
const MAX_NODES: usize = 60;
/// The shortest a move may be.
const MIN_MOVE_MS: u64 = 100;
const DEFAULT_STEP_MS: f64 = 120.0;

/// One step of a sequence: a move, the nodes it moves, and where it starts.
#[derive(Debug, Clone, Deserialize)]
pub struct Item {
    /// A name for this moment, to place later steps against (`at: "intro+=200"`).
    #[serde(default)]
    pub label: Option<String>,
    /// Where the step starts: a number of milliseconds, or a phrase.
    #[serde(default)]
    pub at: Option<Position>,
    #[serde(default)]
    pub node: Option<String>,
    #[serde(default)]
    pub nodes: Vec<String>,
    pub preset: String,
    /// The arguments of the move itself.
    #[serde(flatten)]
    pub params: Map<String, Value>,
    /// How the nodes of this step are spread over time.
    #[serde(default)]
    pub stagger: Option<StaggerArgs>,
}

impl Item {
    /// The nodes the move touches, in order.
    fn targets(&self) -> Result<Vec<String>, ToolError> {
        let mut ids = self.nodes.clone();
        if let Some(one) = &self.node {
            ids.push(one.clone());
        }
        if ids.is_empty() {
            return Err(ToolError(format!(
                "{} needs `node` or `nodes`",
                self.preset
            )));
        }
        if ids.len() > MAX_NODES {
            return Err(ToolError(format!(
                "one step moves at most {MAX_NODES} nodes"
            )));
        }
        Ok(ids)
    }

    /// The move's own arguments, without the ones a sequence sets itself.
    fn move_params(&self) -> Map<String, Value> {
        self.params
            .iter()
            .filter(|(k, _)| MOVE_ARGS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// The arguments that are not the move's own are refused, so a misspelt one is not ignored.
    fn check_params(&self) -> Result<(), ToolError> {
        if let Some(name) = self
            .params
            .keys()
            .find(|k| !MOVE_ARGS.contains(&k.as_str()))
        {
            return Err(ToolError(format!(
                "{name:?} is not an argument of a move; a step takes {}",
                MOVE_ARGS.join(", ")
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SequenceArgs {
    pub video: String,
    pub items: Vec<Item>,
    /// Fit the whole sequence into this many milliseconds.
    #[serde(default)]
    pub fit_to_ms: Option<u64>,
    #[serde(default)]
    pub turn: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

/// A step as it lands on the timeline.
struct Planned {
    item: Item,
    /// The nodes, each with its delay inside the step.
    nodes: Vec<(String, u64)>,
    start_ms: f64,
    duration_ms: u64,
    /// The duration asked for, when a spring's rest time made it longer.
    stretched_from: Option<u64>,
    /// The value a range gives the node, which scales the move.
    values: Option<Vec<f64>>,
}

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    let presets: Vec<Value> = crate::scene_tool::names()
        .into_iter()
        .filter(|n| *n != "stagger")
        .map(Value::from)
        .collect();
    let item_props = json!({
        "label": {"type": "string", "description": "A name for this moment, to place later steps against it."},
        "at": {"description": "Where the step starts: a number of milliseconds, or a phrase: `<` (the end of the step before), `<<` (its start), `+=n`, `-=n`, `*=n`, `<+=n`, `<<+=n`, or `label+=n`. No `at` puts the step after the longest one so far."},
        "node": id("The node to move."),
        "nodes": {"type": "array", "items": {"type": "string"}, "maxItems": MAX_NODES, "description": "The nodes to move, in order (a stagger spreads them over time)."},
        "preset": {"type": "string", "enum": presets},
        "duration_ms": {"type": "integer", "minimum": 100, "maximum": 600000, "description": "How long the move runs (a scene move has its own default)."},
        "strength": {"type": "number", "description": "How far the move goes; 1 is as designed."},
        "intensity": {"type": "number", "description": "How far the move goes; 1 is as designed."},
        "ease": {"type": "string", "description": "Overrides the move's own ease; a tuned spring stretches the move to the time it needs to rest."},
        "repeat": {"type": "string", "enum": ["once", "loop", "pingpong"]},
        "seed": {"type": "integer", "minimum": 0},
        "direction": {"type": "string", "enum": ["left", "right", "up", "down"]},
        "amount": {"type": "number"},
        "center_x": {"type": "number"},
        "center_y": {"type": "number"},
        "stagger": crate::scene_tool::distribution::schema(),
    });
    vec![
        tool(
            "sequence",
            "Several moves placed against each other in time, written as one turn and one undo step. \
             Give each step a `preset`, the `node` or `nodes` it moves, and where it starts: a \
             number of milliseconds, or a phrase against what is already there (`<` the end of the \
             step before, `<<` its start, `<+=50` to overlap it by 50 ms, `intro+=200` after a step \
             named intro). A step may name itself with `label` and spread its own nodes with a \
             `stagger` object. `fit_to_ms` scales the whole sequence to a length. The result echoes \
             the table of resolved starts and ends, so the agent can see the timing it got.",
            json!({
                "video": id("Id of a video from list_videos."),
                "items": {"type": "array", "items": {"type": "object", "properties": item_props}, "minItems": 1, "maxItems": MAX_ITEMS},
                "fit_to_ms": {"type": "integer", "minimum": 100, "maximum": 600000, "description": "Scale the whole sequence to this many milliseconds."},
                "turn": id("Groups related calls into one turn; omit and each call is its own turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["video", "items"],
            false,
        ),
        tool(
            "spring_to",
            "Send one property of a node to a value on a spring: it accelerates, overshoots and \
             settles. The spring is `visual_ms` long (how long it looks like it takes) and `bounce` \
             (0 settles straight, 0.3 playful, 0.5 bouncy). The keys run from where the property is \
             now to the value, over the time the spring needs to come to rest. The result says when \
             it looks done and when it has really stopped, so the next beat can be placed after it.",
            json!({
                "video": id("Id of a video from list_videos."),
                "node": id("Id of the node to move (the video's camera props need none)."),
                "prop": {"type": "string", "description": "The property to move, such as x, y, opacity, scale_x, rotation, trim_end, camera_zoom."},
                "to": {"type": "number", "description": "Where the property ends up."},
                "at_ms": {"type": "integer", "minimum": 0, "description": "When the spring starts (default: after the last key of this property)."},
                "visual_ms": {"type": "integer", "minimum": 50, "maximum": 5000, "description": "How long the spring looks like it takes (default 400)."},
                "bounce": {"type": "number", "minimum": -0.9, "maximum": 0.95, "description": "How much it overshoots: 0 settles straight, 0.3 playful, 0.5 bouncy (default 0)."},
                "turn": id("Groups related calls into one turn; omit and each call is its own turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["video", "prop", "to"],
            false,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "sequence" => sequence(ctx, parse(args)?),
        "spring_to" => spring::call(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

fn sequence(ctx: &mut CallCtx, a: SequenceArgs) -> Result<Output, ToolError> {
    if a.items.is_empty() || a.items.len() > MAX_ITEMS {
        return Err(ToolError(format!(
            "a sequence needs 1 to {MAX_ITEMS} steps"
        )));
    }
    let video_id = node_id(&a.video)?;
    let doc = ctx.ws.session.document();
    if doc.video(&video_id).is_none() {
        return Err(ToolError(format!("no video {}", a.video)));
    }
    let mut planned = plan(doc, &a.items)?;
    let mut note: Option<String> = None;
    let stretched: Vec<(String, u64, u64)> = planned
        .iter()
        .filter_map(|p| {
            p.stretched_from
                .map(|from| (p.item.preset.clone(), from, p.duration_ms))
        })
        .collect();
    if let Some(fit) = a.fit_to_ms {
        retime(&mut planned, fit)?;
        note = Some(format!("the sequence was scaled to fit {fit} ms"));
    }
    let ops = build(doc, &video_id, &planned)?;
    if ops.is_empty() {
        return Err(ToolError("this sequence would not move anything".into()));
    }
    let table: Vec<Value> = planned
        .iter()
        .flat_map(|p| {
            p.nodes
                .iter()
                .map(|(node, offset)| {
                    let start = p.start_ms + *offset as f64;
                    let end = start + p.duration_ms as f64;
                    json!({
                        "label": p.item.label,
                        "node": node,
                        "preset": p.item.preset,
                        "start_ms": start.round() as u64,
                        "end_ms": end.round() as u64,
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let labels: BTreeMap<&str, u64> = planned
        .iter()
        .filter_map(|p| {
            p.item
                .label
                .as_deref()
                .map(|l| (l, p.start_ms.round() as u64))
        })
        .collect();
    let total = table
        .iter()
        .filter_map(|row| row["end_ms"].as_f64())
        .fold(0.0, f64::max);
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: a.summary,
        },
    )?;
    let mut extra = json!({
        "sequence": table,
        "labels": labels,
        "total_ms": total.round() as u64,
    });
    let mut notes: Vec<String> = Vec::new();
    if let Some(note) = note {
        notes.push(note);
    }
    notes.extend(stretched.iter().map(|(preset, from, to)| {
        format!("{preset} ran {from} ms, stretched to {to} ms so its spring ease can come to rest")
    }));
    if !notes.is_empty() {
        extra["note"] = json!(notes.join("; "));
    }
    Ok(merge_output(out, extra))
}

/// Places every step: the start of each against the ones before it, and the spread of its nodes.
fn plan(doc: &Document, items: &[Item]) -> Result<Vec<Planned>, ToolError> {
    let mut timeline = TimelineState::default();
    let mut planned = Vec::new();
    for item in items {
        let named = Named::parse(&item.preset)
            .ok_or_else(|| ToolError(format!("{:?} is not a scene move", item.preset)))?;
        item.check_params()?;
        let ids = item.targets()?;
        if named.is_camera() && ids.len() > 1 {
            return Err(ToolError(format!(
                "{} moves the camera, not a node",
                item.preset
            )));
        }
        let params = item.move_params();
        let ease = params
            .get("ease")
            .and_then(|e| e.as_str())
            .map(str::parse::<Ease>)
            .transpose()
            .map_err(|e| ToolError(e.to_string()))?;
        let asked = params.get("duration_ms").and_then(|d| d.as_u64());
        let timing = scene_tool::timing(Some(named), asked, ease)
            .ok_or_else(|| ToolError("duration_ms is needed for this move".into()))?;
        let seed = params.get("seed").and_then(|s| s.as_u64()).unwrap_or(0);
        let spread: Distribution = item.stagger.clone().unwrap_or_default().distribute(
            doc,
            &ids,
            seed,
            DEFAULT_STEP_MS,
        )?;
        let spread_ms = spread.offsets_ms.iter().copied().max().unwrap_or(0);
        let start = position::resolve_position(&timeline, item.at.as_ref())
            .map_err(|e| ToolError(e.to_string()))?;
        let duration = timing.duration_ms;
        let span = spread_ms as f64 + duration as f64;
        timeline.push(start, span);
        if let Some(label) = &item.label {
            timeline
                .set_label(label, start)
                .map_err(|e| ToolError(e.to_string()))?;
        }
        planned.push(Planned {
            item: item.clone(),
            nodes: ids.into_iter().zip(spread.offsets_ms).collect(),
            start_ms: start,
            duration_ms: duration,
            stretched_from: timing.stretched_from,
            values: spread.values,
        });
    }
    Ok(planned)
}

/// Scales the whole sequence so it lasts `fit_ms`.
fn retime(planned: &mut [Planned], fit_ms: u64) -> Result<(), ToolError> {
    let old = planned
        .iter()
        .map(|p| {
            let last = p.nodes.iter().map(|(_, o)| *o as f64).fold(0.0, f64::max);
            p.start_ms + last + p.duration_ms as f64
        })
        .fold(0.0, f64::max);
    if old <= 0.0 {
        return Err(ToolError("there is nothing to fit".into()));
    }
    let k = fit_ms as f64 / old;
    for p in planned.iter_mut() {
        p.start_ms *= k;
        p.duration_ms = ((p.duration_ms as f64 * k).round() as u64).max(1);
        if p.duration_ms < MIN_MOVE_MS {
            return Err(ToolError(format!(
                "fitting to {fit_ms} ms would make {} last less than {MIN_MOVE_MS} ms",
                p.item.preset
            )));
        }
        for (_, offset) in &mut p.nodes {
            *offset = (*offset as f64 * k).round() as u64;
        }
    }
    Ok(())
}

/// The `animate` operations of every step, each step measured against the document the steps
/// before it left behind.
fn build(
    doc: &Document,
    video_id: &NodeId,
    planned: &[Planned],
) -> Result<Vec<Operation>, ToolError> {
    let mut scratch = doc.clone();
    let mut ops: Vec<Operation> = Vec::new();
    for p in planned {
        for (i, (node, offset)) in p.nodes.iter().enumerate() {
            let mut args = json!({
                "video": video_id.to_string(),
                "preset": p.item.preset,
                "node": node,
                "start_ms": (p.start_ms + *offset as f64).round() as u64,
                "duration_ms": p.duration_ms,
            });
            let params = p.item.move_params();
            for (k, v) in &params {
                if k == "duration_ms" {
                    continue;
                }
                args[k] = v.clone();
            }
            if let Some(values) = &p.values {
                let intensity = args
                    .get("intensity")
                    .or_else(|| args.get("strength"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(1.0);
                args["intensity"] = json!(intensity * values[i]);
            }
            if p.nodes.len() > 1 {
                let seed = params.get("seed").and_then(|s| s.as_u64()).unwrap_or(0);
                args["seed"] = json!(seed.wrapping_add(i as u64 * 7919));
            }
            let step: Vec<Operation> = scene_tool::operations_from_json(&scratch, args)?;
            if step.is_empty() {
                return Err(ToolError(format!(
                    "{} would not move {node} from where it is",
                    p.item.preset
                )));
            }
            for op in &step {
                od_core::ops::apply(&mut scratch, op, false)
                    .map_err(|e| ToolError(format!("{}: {e}", p.item.preset)))?;
            }
            ops.extend(step);
        }
    }
    Ok(keep_the_last(ops))
}

/// The last operation for each node and property: an earlier one for the same pair is replaced by
/// the later keys.
fn keep_the_last(ops: Vec<Operation>) -> Vec<Operation> {
    let mut out: Vec<Operation> = Vec::new();
    let mut seen: Vec<(NodeId, AnimProp, usize)> = Vec::new();
    for op in ops {
        let key = match &op {
            Operation::Animate { node, prop, .. } => Some((node.clone(), *prop)),
            _ => None,
        };
        let Some(key) = key else {
            out.push(op);
            continue;
        };
        match seen.iter().position(|(n, p, _)| *n == key.0 && *p == key.1) {
            Some(at) => out[seen[at].2] = op,
            None => {
                seen.push((key.0, key.1, out.len()));
                out.push(op);
            }
        }
    }
    out
}

/// The frame a time lands on.
pub fn snap(ms: u64, fps: u32) -> u64 {
    snap_within(ms, fps, 0, MAX_VIDEO_MS).unwrap_or(ms)
}

/// Parses an ease the way a tool argument gives it.
pub fn parse_ease(text: &str) -> Result<Ease, ToolError> {
    Ease::from_str(text).map_err(|e| ToolError(e.to_string()))
}
