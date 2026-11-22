//! `text_on_path`: a line of text laid along a path node's curve, one text node per glyph, each
//! standing on the curve and turned to its tangent. Advances come from the renderer's own layout
//! (`od_render::text_lines`), the same source `split_text` cuts from.

use od_core::limits::{MAX_NODES, round4};
use od_core::model::Location;
use od_core::{Document, Ease, NewNode, Node, NodeId, NodeKind, Operation, Property, vector};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::scene_motion::{Named, point_at};
use crate::scene_tool::{self, merge_output, origin_of};
use crate::tools::{ApplyArgs, CallCtx, Output, ToolError, apply_ops, node_id};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    #[default]
    Start,
    Center,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Keep,
    #[default]
    Replace,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reveal {
    video: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    each_ms: Option<u64>,
    #[serde(default)]
    at_ms: Option<u64>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    ease: Option<Ease>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextArgs {
    text_node: String,
    path_node: String,
    #[serde(default)]
    start_offset: Option<f64>,
    #[serde(default)]
    align: Align,
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    reveal: Option<Reveal>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// The most letters a reveal staggers.
const MAX_REVEALED: usize = 60;
const DEFAULT_EACH_MS: u64 = 40;
const BASE_NAME_CHARS: usize = 150;
/// The path's curves are cut into pieces at most this share of its longer side.
const FLATTEN_SHARE: f64 = 1.0 / 256.0;

/// A glyph of the text with where its advance starts along the line of text.
#[derive(Debug, Clone, PartialEq)]
pub struct Letter {
    pub text: String,
    pub start: f64,
    pub advance: f64,
    pub blank: bool,
}

/// The glyphs of the rows in reading order, each with the advance before it. Every glyph takes
/// its width, spaces included.
pub fn letters(rows: &[od_render::PlacedLine]) -> Vec<Letter> {
    let mut at = 0.0;
    let mut out = Vec::new();
    for row in rows {
        for g in &row.glyphs {
            let text = row.text.get(g.start..g.end).unwrap_or("").to_string();
            let advance = f64::from(g.width);
            out.push(Letter {
                blank: text.chars().all(char::is_whitespace),
                text,
                start: at,
                advance,
            });
            at += advance;
        }
    }
    out
}

/// A path flattened into a polyline in the coordinates the letters are placed in.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub points: Vec<(f64, f64)>,
    pub lengths: Vec<f64>,
    pub closed: bool,
}

impl Line {
    pub fn new(points: Vec<(f64, f64)>, closed: bool) -> Line {
        let mut sum = 0.0;
        let mut lengths = vec![0.0];
        for w in points.windows(2) {
            sum += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
            lengths.push(sum);
        }
        Line {
            points,
            lengths,
            closed,
        }
    }

    pub fn total(&self) -> f64 {
        self.lengths.last().copied().unwrap_or(0.0)
    }

    /// The point `s` along the line: held to its ends when open, wrapped round when closed.
    fn point(&self, s: f64) -> (f64, f64) {
        let total = self.total();
        let s = if self.closed {
            s.rem_euclid(total)
        } else {
            s.clamp(0.0, total)
        };
        point_at(&self.points, &self.lengths, s)
    }

    /// Where a glyph whose middle is `s` along the line stands, and the heading of the chord the
    /// glyph spans (degrees, 0 toward the right).
    pub fn stand(&self, s: f64, advance: f64) -> ((f64, f64), f64) {
        let half = (advance / 2.0).max(0.25);
        let (a, b) = (self.point(s - half), self.point(s + half));
        let heading = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
        (self.point(s), heading)
    }
}

/// Where each non-blank letter stands: its index, the point and the heading. Letters off the end
/// of an open line are counted apart.
pub struct Laid {
    pub stands: Vec<(usize, (f64, f64), f64)>,
    pub off_the_end: usize,
    pub first_s: f64,
}

pub fn lay(letters: &[Letter], line: &Line, offset: f64, align: Align) -> Laid {
    let total_text: f64 = letters.last().map_or(0.0, |l| l.start + l.advance);
    let first_s = offset
        + match align {
            Align::Start => 0.0,
            Align::Center => (line.total() - total_text) / 2.0,
            Align::End => line.total() - total_text,
        };
    let mut stands = Vec::new();
    let mut off = 0;
    for (i, l) in letters.iter().enumerate().filter(|(_, l)| !l.blank) {
        let s = first_s + l.start + l.advance / 2.0;
        if !line.closed && !(0.0..=line.total()).contains(&s) {
            off += 1;
            continue;
        }
        let (p, h) = line.stand(s, l.advance);
        stands.push((i, p, h));
    }
    Laid {
        stands,
        off_the_end: off,
        first_s,
    }
}

fn path_line(doc: &Document, text: &Node, path: &Node) -> Result<Line, ToolError> {
    let parsed = vector::node_path(path)
        .ok_or_else(|| ToolError(format!("{} has no usable path data", path.id)))?;
    let fitted = vector::fit_to_box(&parsed, path.view_box, path.width, path.height);
    let step = (path.width.max(path.height) * FLATTEN_SHARE).max(0.1);
    let contour = fitted
        .flatten(step)
        .into_iter()
        .next()
        .ok_or_else(|| ToolError(format!("{} has no points", path.id)))?;
    let mut points: Vec<(f64, f64)> = contour.points.iter().map(|p| (p[0], p[1])).collect();
    if contour.closed
        && let Some(first) = points.first().copied()
    {
        points.push(first);
    }
    let from = origin_of(doc, &path.id)
        .ok_or_else(|| ToolError(format!("{} is not in a page", path.id)))?;
    let to = origin_of(doc, &text.id)
        .ok_or_else(|| ToolError(format!("{} is not in a page", text.id)))?;
    let (dx, dy) = (from.0 + path.x - to.0, from.1 + path.y - to.1);
    let points: Vec<(f64, f64)> = points.into_iter().map(|p| (p.0 + dx, p.1 + dy)).collect();
    let line = Line::new(points, contour.closed);
    if line.total() <= 0.0 {
        return Err(ToolError(format!("{} has no length", path.id)));
    }
    Ok(line)
}

fn id_at(number: u64) -> Result<NodeId, ToolError> {
    Ok(NodeId::parse(&format!("n_{number}"))?)
}

fn parent_of(doc: &Document, loc: &Location) -> Result<(NodeId, usize), ToolError> {
    let (index, above) = loc
        .path
        .split_last()
        .ok_or_else(|| ToolError("the page root cannot carry text on a path".into()))?;
    let parent = Location {
        page: loc.page,
        path: above.to_vec(),
    };
    let node = doc
        .node_at(&parent)
        .ok_or_else(|| ToolError("the node's parent is missing".into()))?;
    Ok((node.id.clone(), *index))
}

/// The text node of one glyph standing at `at`, turned to `heading` about its middle on the baseline.
fn glyph_node(
    original: &Node,
    name: String,
    letter: &Letter,
    at: (f64, f64),
    heading: f64,
) -> Result<NewNode, ToolError> {
    let run = od_render::text_run(original, &letter.text, false)
        .ok_or_else(|| ToolError(format!("{} cannot be laid out", letter.text)))?;
    let (left, right) = run
        .glyphs
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, r), g| {
            (l.min(g.x), r.max(g.x + g.width))
        });
    let (left, right) = if left.is_finite() {
        (f64::from(left), f64::from(right))
    } else {
        (0.0, letter.advance)
    };
    let width = (right + 2.0).ceil();
    let height = round4(f64::from(run.height)).max(1.0);
    let pivot_px = left + (right - left) / 2.0;
    let baseline = f64::from(run.baseline);
    let mut node = NewNode::new(NodeKind::Text);
    node.name = Some(name);
    node.text = Some(letter.text.clone());
    node.width = width;
    node.height = height;
    node.x = round4(at.0 - pivot_px);
    node.y = round4(at.1 - baseline);
    node.rotation = Some(round4(heading));
    node.pivot_x = Some(pivot_px / width);
    node.pivot_y = Some(baseline / height);
    node.font_size = original.font_size;
    node.font_family.clone_from(&original.font_family);
    node.font_weight = original.font_weight;
    node.line_height = original.line_height;
    node.fill.clone_from(&original.fill);
    node.stroke.clone_from(&original.stroke);
    Ok(node)
}

pub fn call(ctx: &mut CallCtx, a: TextArgs) -> Result<Output, ToolError> {
    let doc = ctx.ws.session.document();
    let id = node_id(&a.text_node)?;
    let original = doc
        .node(&id)
        .ok_or_else(|| ToolError(format!("no node {id}")))?;
    if original.kind != NodeKind::Text {
        return Err(ToolError(format!(
            "{id} is a {} node; text_on_path takes a text node",
            original.kind.as_str()
        )));
    }
    let path = doc
        .node(&node_id(&a.path_node)?)
        .ok_or_else(|| ToolError(format!("no node {}", a.path_node)))?;
    if path.kind != NodeKind::Path {
        return Err(ToolError(format!("{} is not a path node", path.id)));
    }
    let rows = od_render::text_lines(original)
        .ok_or_else(|| ToolError(format!("{id} has no text to draw")))?;
    let letters = letters(&rows);
    let line = path_line(doc, original, path)?;
    let laid = lay(&letters, &line, a.start_offset.unwrap_or(0.0), a.align);
    if laid.stands.is_empty() {
        return Err(ToolError(format!(
            "none of the text falls on {}: it is {:.0} long and the text {:.0}; change start_offset or align",
            path.id,
            line.total(),
            letters.last().map_or(0.0, |l| l.start + l.advance)
        )));
    }
    if laid.stands.len() > crate::text_split::MAX_PIECES {
        return Err(ToolError(format!(
            "{} letters is more than the {} a split may make",
            laid.stands.len(),
            crate::text_split::MAX_PIECES
        )));
    }
    if doc.node_count() + laid.stands.len() + 1 > MAX_NODES {
        return Err(ToolError(format!(
            "the letters need {} more nodes and the document is limited to {MAX_NODES}",
            laid.stands.len() + 1
        )));
    }
    let loc = doc
        .locate(&id)
        .ok_or_else(|| ToolError(format!("no node {id}")))?;
    let (parent, index) = parent_of(doc, &loc)?;
    let base: String = original.name.chars().take(BASE_NAME_CHARS).collect();
    let mut group = NewNode::new(NodeKind::Group);
    group.name = Some(match a.mode {
        Mode::Replace => original.name.clone(),
        Mode::Keep => format!("{base} on path"),
    });
    group.visible = Some(original.visible);
    group.opacity = Some(original.opacity);
    group.blur = Some(original.blur);
    group.shadow.clone_from(&original.shadow);
    group.depth = Some(original.depth);
    for (n, (i, at, heading)) in laid.stands.iter().enumerate() {
        let name = format!("{base}.p{n}");
        group
            .children
            .push(glyph_node(original, name, &letters[*i], *at, *heading)?);
    }
    let first = doc.next_id;
    let group_id = id_at(first)?;
    let ids: Vec<NodeId> = (0..laid.stands.len() as u64)
        .map(|n| id_at(first + 1 + n))
        .collect::<Result<_, _>>()?;
    let at = match a.mode {
        Mode::Replace => index,
        Mode::Keep => index + 1,
    };
    let mut ops = vec![Operation::Create {
        parent,
        index: Some(at),
        node: group,
    }];
    ops.push(match a.mode {
        Mode::Replace => Operation::Delete { id: id.clone() },
        Mode::Keep => Operation::Set {
            id: id.clone(),
            prop: Property::Visible(false),
        },
    });
    let mut revealed: Option<Value> = None;
    if let Some(reveal) = &a.reveal {
        let (more, summary) = reveal_ops(doc, &ops, &ids, reveal)?;
        ops.extend(more);
        revealed = Some(summary);
    }
    let summary = a
        .summary
        .unwrap_or_else(|| format!("lay {} on the path {}", original.name, path.name));
    let mut report = json!({
        "group": group_id.to_string(),
        "ids": ids.iter().map(NodeId::to_string).collect::<Vec<_>>(),
        "letters": laid.stands.iter().map(|(i, _, _)| letters[*i].text.clone()).collect::<Vec<_>>(),
        "count": laid.stands.len(),
        "off_the_end": laid.off_the_end,
        "path_length": (line.total() * 100.0).round() / 100.0,
        "text_length": (letters.last().map_or(0.0, |l| l.start + l.advance) * 100.0).round() / 100.0,
        "starts_at": (laid.first_s * 100.0).round() / 100.0,
        "closed": line.closed,
        "mode": match a.mode { Mode::Keep => "keep", Mode::Replace => "replace" },
    });
    if let Some(r) = revealed {
        report["reveal"] = r;
    }
    if laid.off_the_end > 0 {
        report["note"] = json!(format!(
            "{} letter(s) would fall off the end of the open path and were left out",
            laid.off_the_end
        ));
    }
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: Some(summary),
        },
    )?;
    Ok(merge_output(out, report))
}

/// The keys that bring the letters in: the `stagger` of a scene move over them, measured on the
/// document with the letters already made.
fn reveal_ops(
    doc: &Document,
    made: &[Operation],
    ids: &[NodeId],
    reveal: &Reveal,
) -> Result<(Vec<Operation>, Value), ToolError> {
    if ids.len() > MAX_REVEALED {
        return Err(ToolError(format!(
            "reveal staggers at most {MAX_REVEALED} letters; this text has {}. Reveal without it and animate the group, or shorten the text",
            ids.len()
        )));
    }
    let preset = reveal.preset.as_deref().unwrap_or("pop_in");
    match Named::parse(preset) {
        Some(n) if !n.is_camera() => {}
        _ => {
            return Err(ToolError(format!(
                "reveal preset {preset:?} is not a scene move for a node (try pop_in, fade_up, zoom_in)"
            )));
        }
    }
    let mut scratch = doc.clone();
    for op in made {
        od_core::ops::apply(&mut scratch, op, false)
            .map_err(|e| ToolError(format!("text_on_path: {e}")))?;
    }
    let each = reveal.each_ms.unwrap_or(DEFAULT_EACH_MS);
    let at = reveal.at_ms.unwrap_or(0);
    let mut args = json!({
        "video": reveal.video,
        "preset": "stagger",
        "of": preset,
        "nodes": ids.iter().map(NodeId::to_string).collect::<Vec<_>>(),
        "start_ms": at,
        "step_ms": each,
    });
    if let Some(d) = reveal.duration_ms {
        args["duration_ms"] = json!(d);
    }
    if let Some(e) = reveal.ease {
        args["ease"] = json!(e.to_string());
    }
    let ops = scene_tool::operations_from_json(&scratch, args)?;
    Ok((ops, json!({"preset": preset, "each_ms": each, "at_ms": at})))
}
