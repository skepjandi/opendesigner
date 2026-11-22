//! `morph_to`: a path node turns into another. The outlines are matched point for point by
//! `od_core::morph`; this module reads them off the two nodes, pairs the contours, and writes the
//! node's `path_morph` and its `morph_progress` keys.

use od_core::animation::{AnimProp, Key, Repeat};
use od_core::morph::{self, PathMorph};
use od_core::path::Point;
use od_core::timebase::snap_within;
use od_core::video::MAX_VIDEO_MS;
use od_core::{Document, Ease, Node, NodeKind, Operation, Property, vector};
use serde::Deserialize;
use serde_json::json;

use crate::scene_tool::{merge_output, origin_of};
use crate::tools::{ApplyArgs, CallCtx, Output, ToolError, apply_ops, node_id};

use super::{check_span, video_of};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MorphArgs {
    video: String,
    node: String,
    to_path: String,
    at_ms: u64,
    duration_ms: u64,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    reset: bool,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// One outline of a shape: its points and whether it closes.
#[derive(Debug, Clone, PartialEq)]
pub struct Outline {
    pub points: Vec<Point>,
    pub closed: bool,
}

impl Outline {
    fn length(&self) -> f64 {
        morph::outline_length(&self.points, self.closed)
    }
}

fn path_node<'a>(doc: &'a Document, id: &str) -> Result<&'a Node, ToolError> {
    let node = doc
        .node(&node_id(id)?)
        .ok_or_else(|| ToolError(format!("no node {id}")))?;
    if node.kind != NodeKind::Path {
        return Err(ToolError(format!(
            "{id} is a {} node; only a path node can morph",
            node.kind.as_str()
        )));
    }
    Ok(node)
}

/// The outlines of a path node in its own box coordinates, largest first.
pub fn outlines(node: &Node) -> Result<Vec<Outline>, ToolError> {
    let parsed = vector::node_path(node)
        .ok_or_else(|| ToolError(format!("{} has no usable path data", node.id)))?;
    let fitted = vector::fit_to_box(&parsed, node.view_box, node.width, node.height);
    let step = (node.width.max(node.height) / 200.0).max(0.25);
    let mut found: Vec<Outline> = fitted
        .flatten(step)
        .into_iter()
        .filter_map(|c| {
            let mut points = c.points;
            if c.closed && points.len() > 1 && points.first() == points.last() {
                points.pop();
            }
            let outline = Outline {
                points,
                closed: c.closed,
            };
            (outline.points.len() >= 2 && outline.length() > 0.0).then_some(outline)
        })
        .collect();
    if found.is_empty() {
        return Err(ToolError(format!("{} has no outline to morph", node.id)));
    }
    found.sort_by(|a, b| {
        morph::area(&b.points)
            .total_cmp(&morph::area(&a.points))
            .then(b.length().total_cmp(&a.length()))
    });
    Ok(found)
}

fn shifted(outlines: Vec<Outline>, by: (f64, f64)) -> Vec<Outline> {
    outlines
        .into_iter()
        .map(|o| Outline {
            points: o
                .points
                .iter()
                .map(|p| [p[0] + by.0, p[1] + by.1])
                .collect(),
            closed: o.closed,
        })
        .collect()
}

/// A morph from `a` to `b`, contour by contour in order of size. A contour with no partner is
/// paired with its own middle.
pub fn first_morph(
    a: &[Outline],
    b: &[Outline],
    width: f64,
    height: f64,
) -> Result<PathMorph, ToolError> {
    let count = a.len().max(b.len());
    if count > morph::MAX_CONTOURS {
        return Err(ToolError(format!(
            "a morph holds at most {} contours; these shapes have {count}",
            morph::MAX_CONTOURS
        )));
    }
    let mut closed = Vec::with_capacity(count);
    let (mut from, mut to) = (Vec::with_capacity(count), Vec::with_capacity(count));
    for i in 0..count {
        match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) => {
                let m = morph::match_pair(&x.points, x.closed, &y.points, y.closed);
                closed.push(m.closed);
                from.push(m.from);
                to.push(m.to);
            }
            (Some(x), None) => {
                let n = morph::point_count(x.length(), x.length());
                closed.push(x.closed);
                from.push(morph::resample(&x.points, x.closed, n));
                to.push(morph::collapsed(&x.points, n));
            }
            (None, Some(y)) => {
                let n = morph::point_count(y.length(), y.length());
                closed.push(y.closed);
                from.push(morph::collapsed(&y.points, n));
                to.push(morph::resample(&y.points, y.closed, n));
            }
            (None, None) => {}
        }
    }
    Ok(PathMorph {
        width,
        height,
        closed,
        stops: vec![from, to],
    })
}

/// `morph` with one more stop: the shape `b`, cut into the points the morph already has and lined
/// up with its last stop.
pub fn next_stop(morph: &PathMorph, b: &[Outline]) -> Result<PathMorph, ToolError> {
    if b.len() > morph.closed.len() {
        return Err(ToolError(format!(
            "the target has {} contours and the morph on this node only {}; use reset to start over",
            b.len(),
            morph.closed.len()
        )));
    }
    let last = morph
        .stops
        .last()
        .ok_or_else(|| ToolError("the morph has no stops".into()))?;
    let mut stop = Vec::with_capacity(last.len());
    for (c, previous) in last.iter().enumerate() {
        let n = previous.len();
        stop.push(match b.get(c) {
            None => morph::collapsed(previous, n),
            Some(y) if morph.closed[c] => {
                let ring = morph::resample(&y.points, true, n);
                morph::align_ring(previous, &ring, true)
            }
            Some(y) => {
                let mut line = y.points.clone();
                if y.closed
                    && let Some(first) = line.first().copied()
                {
                    line.push(first);
                }
                morph::resample(&line, false, n)
            }
        });
    }
    let mut next = morph.clone();
    next.stops.push(stop);
    Ok(next)
}

fn keys_of(start: u64, end: u64, from: f64, to: f64, ease: Ease) -> Vec<Key> {
    vec![
        Key {
            at_ms: start,
            value: from,
            ease: Ease::Linear,
        },
        Key {
            at_ms: end,
            value: to,
            ease,
        },
    ]
}

pub fn call(ctx: &mut CallCtx, a: MorphArgs) -> Result<Output, ToolError> {
    let doc = ctx.ws.session.document();
    let video = video_of(doc, &a.video)?;
    let node = path_node(doc, &a.node)?;
    let target = path_node(doc, &a.to_path)?;
    if node.id == target.id {
        return Err(ToolError("a node cannot morph into itself".into()));
    }
    check_span(a.at_ms, a.duration_ms)?;
    if node.width <= 0.0 || node.height <= 0.0 {
        return Err(ToolError(format!(
            "{} has no size; a path that is a straight horizontal or vertical line cannot morph",
            node.id
        )));
    }
    let origin = |n: &Node| -> Result<(f64, f64), ToolError> {
        let o =
            origin_of(doc, &n.id).ok_or_else(|| ToolError(format!("{} is not in a page", n.id)))?;
        Ok((o.0 + n.x, o.1 + n.y))
    };
    let (here, there) = (origin(node)?, origin(target)?);
    let b = shifted(outlines(target)?, (there.0 - here.0, there.1 - here.1));
    let fps = video.fps;
    let start = snap_within(a.at_ms, fps, 0, MAX_VIDEO_MS).unwrap_or(a.at_ms);
    let end = snap_within(a.at_ms + a.duration_ms, fps, 0, MAX_VIDEO_MS)
        .unwrap_or(a.at_ms + a.duration_ms)
        .max(start + 1);
    let ease = a.ease.unwrap_or(Ease::EaseInOut);
    let track = video.base_track(&node.id, AnimProp::MorphProgress);
    let chain = node.morph.as_ref().filter(|_| !a.reset && track.is_some());
    let (morph, keys, note) = match (chain, track) {
        (Some(m), Some(t)) => {
            let last_at = t.keys.last().map_or(0, |k| k.at_ms);
            if start < last_at {
                return Err(ToolError(format!(
                    "the morph on {} runs until {last_at} ms; start the next one at or after it, or pass reset",
                    node.id
                )));
            }
            let held = m.max_progress();
            let mut keys = t.keys.clone();
            if start > last_at {
                keys.push(Key {
                    at_ms: start,
                    value: held,
                    ease: Ease::Linear,
                });
            }
            keys.push(Key {
                at_ms: end,
                value: held + 1.0,
                ease,
            });
            (next_stop(m, &b)?, keys, None)
        }
        _ => {
            let a_out = outlines(node)?;
            let note = (node.morph.is_some() && !a.reset).then(|| {
                "the node had a morph but no progress keys, so it started over from its own path"
                    .to_string()
            });
            (
                first_morph(&a_out, &b, node.width, node.height)?,
                keys_of(start, end, 0.0, 1.0, ease),
                note,
            )
        }
    };
    let report = json!({
        "node": node.id.to_string(),
        "to_path": target.id.to_string(),
        "contours": morph.closed.len(),
        "points": morph.stops[0].iter().map(Vec::len).collect::<Vec<_>>(),
        "stops": morph.stops.len(),
        "start_ms": start,
        "end_ms": end,
        "progress_from": keys.first().map(|k| k.value),
        "progress_to": keys.last().map(|k| k.value),
    });
    let set = Operation::Set {
        id: node.id.clone(),
        prop: Property::PathMorph(Some(morph)),
    };
    let animate = Operation::Animate {
        additive: false,
        video: video.id.clone(),
        node: node.id.clone(),
        prop: AnimProp::MorphProgress,
        keys,
        repeat: Repeat::Once,
    };
    // starting over replaces the keys first, so no key is ever past the stops the node holds
    let ops = if track.is_some() && chain.is_none() {
        vec![animate, set]
    } else {
        vec![set, animate]
    };
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: a.summary,
        },
    )?;
    let mut report = report;
    if let Some(note) = note {
        report["note"] = json!(note);
    }
    Ok(merge_output(out, report))
}
