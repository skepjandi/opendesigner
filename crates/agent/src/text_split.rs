//! `split_text` and `text_reveal` (`docs/wiki/agent-integration.md#kinetic-type`): a text node
//! becomes a group of one text node per character, word or line, each drawn where the original
//! glyphs were, so the split reads the same at rest and the pieces can be animated one by one.
//!
//! Positions come from the renderer's own layout (`od_render::text_lines`), never from a guess,
//! so the lines are the ones the box width really wraps to. A split is one `create` (the whole
//! group) plus a `delete` or a `visible: false` on the original, applied as one transaction, so
//! one undo puts the text back.

mod pieces;
mod reveal;
#[cfg(test)]
mod tests;

use od_core::animation::{AnimProp, Key, Repeat};
use od_core::limits::MAX_NODES;
use od_core::model::Location;
use od_core::video::MAX_VIDEO_MS;
use od_core::{Document, Ease, NewNode, NodeId, NodeKind, Operation, Property, TextAlign};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops, node_id, parse, tool,
};
use pieces::{By, HEADLINE_CHARS};
use reveal::{Rest, Style};

pub const NAMES: [&str; 2] = ["split_text", "text_reveal"];

/// The most pieces one split makes: each is a node (two when masked) and a reveal writes up to
/// three tracks per piece, all in one transaction.
pub const MAX_PIECES: usize = 300;
const MAX_EACH_MS: u64 = 5_000;
const MAX_DURATION_MS: u64 = 60_000;
const MASK_PAD_SHARE: f64 = 0.08;
const BASE_NAME_CHARS: usize = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum Mode {
    /// The original is hidden (`visible: false`), so it can be shown again.
    Keep,
    /// The original is removed; the group takes its name and place.
    #[default]
    Replace,
}

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    let by = json!({"type": "string", "enum": ["chars", "words", "lines"]});
    let wrap = json!({"type": "boolean", "description": "Put each piece in its own clipping group (a mask box the size of the piece's line) so it can rise or slide in from outside the box. The animated node is then the text inside the mask."});
    let mode = json!({"type": "string", "enum": ["keep", "replace"], "description": "keep hides the original text (visible false) so it can be shown again; replace (the default) removes it and the group takes its name. One undo restores it either way."});
    vec![
        tool(
            "split_text",
            "Split a text node into a group of one text node per character, word or line, each drawn where the original glyphs were (the renderer's own shaping and wrapping, so the lines are the ones the box width really breaks to) with the original font, size, weight, line height, fill and stroke. Pieces are named '<name>.c0..' (chars), '<name>.w0..' (words) or '<name>.l0..' (lines), in reading order; spaces are not nodes. The group takes the original's box, transform, opacity, blur and shadow. Returns the piece ids in order: pass them to animate_preset stagger as nodes, or use text_reveal to split and animate in one call. One attributed, undoable turn. Limits: a ligature is one glyph and so one character piece, and the kerning between two pieces is lost; a piece is set flush left at the place its glyphs were drawn (a line keeps the text's own alignment), so a centred or right-aligned text may sit a pixel off; position is rounded to whole pixels, so a left-aligned text is drawn exactly as before and other alignments match to within a pixel; right-to-left text is ordered by logical position and only pure runs are exact.",
            json!({
                "node": id("Id of the text node."),
                "by": by,
                "wrap": wrap,
                "mode": mode,
                "turn": id("Groups related calls into one turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["node", "by"],
            false,
        ),
        tool(
            "text_reveal",
            "Bring text in piece by piece: splits the text node first (as split_text does, unless it is already a split group) and writes the keyframes, as one undoable turn. Styles: rise (pieces rise 450 ms with power4_out, from below their mask box; masked by default), fade (400 ms ease_out), pop (scale 0.5 to 1 with a small overshoot, 350 ms), slide (from the left, 500 ms power4_out), typewriter (each piece appears at once, 33 ms apart per character). Stagger defaults: 40 ms per character, 90 ms per word, 140 ms per line (typewriter 33 / 120 / 400). `by` defaults to characters for a headline of 12 characters or fewer on one line, else words (lines past 4 lines or 40 words). Never reveal body copy per character. Keys replace any track a piece already has. Returns the resolved timing (each piece's start and end), when the reveal ends and how long to hold it for it to be readable (1 s plus 0.4 s per short word, review check RV015).",
            json!({
                "video": id("Id of a video from list_videos."),
                "node": id("Id of the text node, or of a group split_text made."),
                "style": {"type": "string", "enum": ["rise", "fade", "pop", "slide", "typewriter"]},
                "by": by,
                "wrap": wrap,
                "mode": mode,
                "each_ms": {"type": "integer", "minimum": 0, "maximum": MAX_EACH_MS, "description": "Milliseconds from one piece to the next."},
                "at_ms": {"type": "integer", "minimum": 0, "description": "When the first piece starts (default 0)."},
                "duration_ms": {"type": "integer", "minimum": 1, "maximum": MAX_DURATION_MS, "description": "How long each piece takes (style default; ignored by typewriter)."},
                "ease": {"type": "string", "description": format!("Overrides the style's ease: one of {} or a tuned one: {}.", Ease::NAMES.join(", "), od_core::ease::GRAMMAR)},
                "turn": id("Groups related calls into one turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["video", "node", "style"],
            false,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "split_text" => split(ctx, parse(args)?),
        "text_reveal" => reveal_text(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SplitArgs {
    node: String,
    by: By,
    #[serde(default)]
    wrap: bool,
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RevealArgs {
    video: String,
    node: String,
    style: Style,
    #[serde(default)]
    by: Option<By>,
    #[serde(default)]
    wrap: Option<bool>,
    #[serde(default)]
    mode: Mode,
    #[serde(default)]
    each_ms: Option<u64>,
    #[serde(default)]
    at_ms: Option<u64>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

struct Spec {
    by: Option<By>,
    typewriter: bool,
    wrap: bool,
    mode: Mode,
}

/// One piece as made: its node, the mask around it when there is one, and where it rests.
struct Made {
    id: NodeId,
    mask: Option<NodeId>,
    text: String,
    rest: Rest,
    line: usize,
    /// The outermost node's box (the mask's, or the text's own) in the group's coordinates.
    outer: [f64; 4],
}

struct Plan {
    ops: Vec<Operation>,
    group: NodeId,
    by: By,
    wrap: bool,
    made: Vec<Made>,
    original: NodeId,
    mode: Mode,
}

fn id_at(number: u64) -> Result<NodeId, ToolError> {
    Ok(NodeId::parse(&format!("n_{number}"))?)
}

fn short(name: &str) -> String {
    name.chars().take(BASE_NAME_CHARS).collect()
}

fn piece_node(
    original: &od_core::Node,
    name: String,
    text: String,
    width: f64,
    height: f64,
    align: Option<TextAlign>,
) -> NewNode {
    let mut node = NewNode::new(NodeKind::Text);
    node.name = Some(name);
    node.width = width;
    node.height = height;
    node.text = Some(text);
    node.font_size = original.font_size;
    node.font_family.clone_from(&original.font_family);
    node.font_weight = original.font_weight;
    node.line_height = original.line_height;
    node.text_align = align;
    node.fill.clone_from(&original.fill);
    node.stroke.clone_from(&original.stroke);
    node
}

fn group_node(original: &od_core::Node, name: String) -> NewNode {
    let mut group = NewNode::new(NodeKind::Group);
    group.name = Some(name);
    group.x = original.x;
    group.y = original.y;
    group.width = original.width;
    group.height = original.height;
    group.visible = Some(original.visible);
    group.opacity = Some(original.opacity);
    group.rotation = Some(original.rotation);
    group.scale_x = Some(original.scale_x);
    group.scale_y = Some(original.scale_y);
    group.pivot_x = Some(original.pivot_x);
    group.pivot_y = Some(original.pivot_y);
    group.blur = Some(original.blur);
    group.shadow.clone_from(&original.shadow);
    group.depth = Some(original.depth);
    group
}

fn parent_of(doc: &Document, loc: &Location) -> Result<(NodeId, usize), ToolError> {
    let (index, above) = loc
        .path
        .split_last()
        .ok_or_else(|| ToolError("the page root cannot be split".into()))?;
    let parent = Location {
        page: loc.page,
        path: above.to_vec(),
    };
    let node = doc
        .node_at(&parent)
        .ok_or_else(|| ToolError("the node's parent is missing".into()))?;
    Ok((node.id.clone(), *index))
}

fn plan(doc: &Document, id: &NodeId, spec: &Spec) -> Result<Plan, ToolError> {
    let original = doc
        .node(id)
        .ok_or_else(|| ToolError(format!("no node {id}")))?;
    if original.kind != NodeKind::Text {
        return Err(ToolError(format!(
            "{id} is a {} node; only a text node can be split",
            original.kind.as_str()
        )));
    }
    let rows = od_render::text_lines(original)
        .ok_or_else(|| ToolError(format!("{id} has no text to draw")))?;
    let by = spec.by.unwrap_or_else(|| {
        if spec.typewriter {
            typewriter_by(&rows)
        } else {
            pieces::default_by(&rows)
        }
    });
    let cut = pieces::split(&rows, by);
    if cut.is_empty() {
        return Err(ToolError(format!("{id} has nothing to split")));
    }
    if cut.len() > MAX_PIECES {
        return Err(ToolError(format!(
            "{} {} is more than the {MAX_PIECES} a split may make; split by words or lines instead",
            cut.len(),
            by.as_str()
        )));
    }
    let nodes = 1 + cut.len() * if spec.wrap { 2 } else { 1 };
    if doc.node_count() + nodes > MAX_NODES {
        return Err(ToolError(format!(
            "the split needs {nodes} more nodes and the document is limited to {MAX_NODES}"
        )));
    }
    let style_baseline = rows.first().map(|r| r.baseline).unwrap_or(0.0);
    let centred = matches!(
        original.text_align,
        Some(TextAlign::Center) | Some(TextAlign::Right)
    );
    let loc = doc
        .locate(id)
        .ok_or_else(|| ToolError(format!("no node {id}")))?;
    let (parent, index) = parent_of(doc, &loc)?;
    let base = short(&original.name);
    let group_name = match spec.mode {
        Mode::Replace => original.name.clone(),
        Mode::Keep => short(&format!("{base} split")),
    };
    let size = original.font_size.unwrap_or(16.0);
    let pad = (size * MASK_PAD_SHARE).round().max(1.0);
    let first = doc.next_id;
    let mut group = group_node(original, group_name);
    let mut made = Vec::with_capacity(cut.len());
    let mut next = first + 1;
    let r = original.x - original.x.round();
    for (i, piece) in cut.iter().enumerate() {
        let name = format!("{base}.{}{i}", by.prefix());
        let own = own_glyphs(original, &piece.text, by == By::Lines);
        let (shift, own_baseline, own_left, own_right) = match &own {
            Some((glyphs, baseline)) if glyphs.len() == piece.glyphs.len() => (
                pieces::column_offset(&piece.glyphs, glyphs) as f64,
                *baseline,
                glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min),
                glyphs
                    .iter()
                    .map(|g| g.x + g.width)
                    .fold(f32::NEG_INFINITY, f32::max),
            ),
            _ => (0.0, style_baseline, 0.0, 0.0),
        };
        let x = shift - r;
        let y = pieces::row_offset(piece, own_baseline) as f64;
        let width = if by == By::Lines && centred {
            original.width
        } else {
            f64::from((own_right - own_left).ceil() + 2.0_f32)
        };
        let height = od_core::limits::round4(f64::from(piece.height));
        let align = if by == By::Lines {
            original.text_align
        } else {
            None
        };
        let mut text = piece_node(
            original,
            name.clone(),
            piece.text.clone(),
            width,
            height,
            align,
        );
        let (text_id, mask_id, rest, outer) = if spec.wrap {
            let (_ink_left, ink_right) = piece.edges();
            let mut mask = NewNode::new(NodeKind::Group);
            mask.name = Some(format!("{name}.mask"));
            mask.clip = Some(true);
            mask.x = x - pad;
            mask.y = f64::from(piece.top);
            mask.width = f64::from((ink_right - x as f32).ceil() + 2.0_f32 * pad as f32);
            mask.height = height;
            text.x = pad;
            text.y = y - f64::from(piece.top);
            let rest = Rest {
                x: pad,
                y: y - f64::from(piece.top),
                size,
                height,
                width: mask.width,
                masked: true,
            };
            let outer = [mask.x, mask.y, mask.width, mask.height];
            let mask_id = id_at(next)?;
            let text_id = id_at(next + 1)?;
            next += 2;
            mask.children.push(text);
            group.children.push(mask);
            (text_id, Some(mask_id), rest, outer)
        } else {
            text.x = x;
            text.y = y;
            let rest = Rest {
                x,
                y,
                size,
                height,
                width,
                masked: false,
            };
            let outer = [x, y, width, height];
            let text_id = id_at(next)?;
            next += 1;
            group.children.push(text);
            (text_id, None, rest, outer)
        };
        made.push(Made {
            id: text_id,
            mask: mask_id,
            text: piece.text.clone(),
            rest,
            line: piece.line,
            outer,
        });
    }
    let at = match spec.mode {
        Mode::Replace => index,
        Mode::Keep => index + 1,
    };
    let mut ops = vec![Operation::Create {
        parent,
        index: Some(at),
        node: group,
    }];
    ops.push(match spec.mode {
        Mode::Replace => Operation::Delete { id: id.clone() },
        Mode::Keep => Operation::Set {
            id: id.clone(),
            prop: Property::Visible(false),
        },
    });
    Ok(Plan {
        ops,
        group: id_at(first)?,
        by,
        wrap: spec.wrap,
        made,
        original: id.clone(),
        mode: spec.mode,
    })
}

fn typewriter_by(rows: &[od_render::PlacedLine]) -> By {
    let chars = pieces::split(rows, By::Chars).len();
    if chars <= TYPEWRITER_CHARS {
        By::Chars
    } else {
        By::Words
    }
}

const TYPEWRITER_CHARS: usize = 80;

/// The glyphs of `text` laid out on their own in the style of `node`, and the baseline they sit
/// on.
fn own_glyphs(
    original: &od_core::Node,
    text: &str,
    aligned: bool,
) -> Option<(Vec<pieces::Glyph>, f32)> {
    od_render::text_run(original, text, aligned).map(|run| {
        let glyphs = run
            .glyphs
            .into_iter()
            .map(|g| pieces::Glyph {
                x: g.x,
                y: g.y,
                width: g.width,
            })
            .collect();
        (glyphs, run.baseline)
    })
}

fn made_rows(made: &[Made]) -> Vec<Value> {
    made.iter()
        .map(|m| {
            let [x, y, width, height] = m.outer;
            json!({"id": m.id, "text": m.text, "line": m.line, "x": x, "y": y, "width": width, "height": height})
        })
        .collect()
}

fn plan_report(plan: &Plan) -> Value {
    json!({
        "group": plan.group,
        "by": plan.by.as_str(),
        "wrap": plan.wrap,
        "mode": match plan.mode { Mode::Keep => "keep", Mode::Replace => "replace" },
        "original": plan.original,
        "count": plan.made.len(),
        "ids": plan.made.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        "masks": plan.made.iter().filter_map(|m| m.mask.clone()).collect::<Vec<_>>(),
        "pieces": made_rows(&plan.made),
    })
}

fn commit(
    ctx: &mut CallCtx,
    ops: Vec<Operation>,
    turn: Option<String>,
    summary: String,
    mut report: Value,
) -> Result<Output, ToolError> {
    let applied = apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn,
            summary: Some(summary),
        },
    )?;
    if let Some(Content::Text(t)) = applied.content.first()
        && let Ok(Value::Object(receipt)) = serde_json::from_str(t)
    {
        report["transaction"] = receipt["transaction"].clone();
        report["checkpoint_before"] = receipt["checkpoint_before"].clone();
        report["turn"] = receipt["turn"].clone();
    }
    Ok(Output::json(&report))
}

fn name_of(doc: &Document, id: &NodeId) -> String {
    doc.node(id).map(|n| n.name.clone()).unwrap_or_default()
}

fn split(ctx: &mut CallCtx, a: SplitArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.node)?;
    let doc = ctx.ws.session.document();
    let planned = plan(
        doc,
        &id,
        &Spec {
            by: Some(a.by),
            typewriter: false,
            wrap: a.wrap,
            mode: a.mode,
        },
    )?;
    let summary = a
        .summary
        .unwrap_or_else(|| format!("split {} by {}", name_of(doc, &id), a.by.as_str()));
    let report = plan_report(&planned);
    commit(ctx, planned.ops, a.turn, summary, report)
}

/// The pieces of a group `split_text` made, read back from the document.
fn existing(doc: &Document, id: &NodeId) -> Option<(Vec<Made>, bool)> {
    let group = doc.node(id).filter(|n| n.kind == NodeKind::Group)?;
    if group.children.is_empty() {
        return None;
    }
    let mut made = Vec::new();
    let mut masked = None;
    for (line, child) in group.children.iter().enumerate() {
        let (text, mask) = match child.kind {
            NodeKind::Text => (child, None),
            NodeKind::Group if child.clip && child.children.len() == 1 => (
                child
                    .children
                    .first()
                    .filter(|t| t.kind == NodeKind::Text)?,
                Some(child),
            ),
            _ => return None,
        };
        if *masked.get_or_insert(mask.is_some()) != mask.is_some() {
            return None;
        }
        let outer = mask.unwrap_or(text);
        made.push(Made {
            id: text.id.clone(),
            mask: mask.map(|m| m.id.clone()),
            text: text.text.clone().unwrap_or_default(),
            rest: Rest {
                x: text.x,
                y: text.y,
                size: text.font_size.unwrap_or(16.0),
                height: text.height,
                width: outer.width,
                masked: mask.is_some(),
            },
            line,
            outer: [outer.x, outer.y, outer.width, outer.height],
        });
    }
    Some((made, masked.unwrap_or(false)))
}

/// What a reveal animates: the pieces of a fresh split (with the operations that make it), or
/// of a group split earlier.
struct Source {
    ops: Vec<Operation>,
    group: NodeId,
    mode: Option<Mode>,
    by: By,
    wrap: bool,
    made: Vec<Made>,
    text: String,
    original: Option<NodeId>,
}

impl Source {
    fn already_split(id: &NodeId, made: Vec<Made>, wrap: bool, by: Option<By>) -> Source {
        let text = made
            .iter()
            .map(|m| m.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Source {
            ops: Vec::new(),
            group: id.clone(),
            mode: None,
            by: by.unwrap_or(By::Words),
            wrap,
            made,
            text,
            original: None,
        }
    }

    fn planned(plan: Plan, text: Option<String>) -> Source {
        Source {
            ops: plan.ops,
            group: plan.group,
            mode: Some(plan.mode),
            by: plan.by,
            wrap: plan.wrap,
            made: plan.made,
            text: text.unwrap_or_default(),
            original: Some(plan.original),
        }
    }
}

struct Timing {
    at_ms: u64,
    each_ms: u64,
    duration_ms: u64,
    ease: Ease,
}

fn check_range(what: &str, value: u64, max: u64) -> Result<u64, ToolError> {
    if value > max {
        return Err(ToolError(format!("{what} {value} is more than {max}")));
    }
    Ok(value)
}

fn reveal_text(ctx: &mut CallCtx, a: RevealArgs) -> Result<Output, ToolError> {
    let video = node_id(&a.video)?;
    let id = node_id(&a.node)?;
    let doc = ctx.ws.session.document();
    if doc.video(&video).is_none() {
        return Err(ToolError(format!(
            "no video {video}; list_videos names them"
        )));
    }
    let style = a.style;
    let source = match existing(doc, &id) {
        Some((made, wrap)) => Source::already_split(&id, made, wrap, a.by),
        None => {
            let planned = plan(
                doc,
                &id,
                &Spec {
                    by: a.by,
                    typewriter: style == Style::Typewriter,
                    wrap: a.wrap.unwrap_or(style.default_wrap()),
                    mode: a.mode,
                },
            )?;
            Source::planned(planned, doc.node(&id).and_then(|n| n.text.clone()))
        }
    };
    let Source {
        mut ops,
        group,
        mode,
        by,
        wrap,
        made,
        text: shown_text,
        original,
    } = source;
    let timing = Timing {
        at_ms: check_range("at_ms", a.at_ms.unwrap_or(0), MAX_VIDEO_MS)?,
        each_ms: check_range(
            "each_ms",
            a.each_ms.unwrap_or_else(|| style.default_each_ms(by)),
            MAX_EACH_MS,
        )?,
        duration_ms: check_range(
            "duration_ms",
            a.duration_ms.unwrap_or_else(|| style.default_duration_ms()),
            MAX_DURATION_MS,
        )?,
        ease: a.ease.unwrap_or_else(|| style.default_ease()),
    };
    let starts: Vec<u64> = (0..made.len() as u64)
        .map(|i| timing.at_ms + i * timing.each_ms)
        .collect();
    let mut rows = Vec::with_capacity(made.len());
    let mut ends_at = timing.at_ms;
    let mut props: Vec<&'static str> = Vec::new();
    for (piece, &start) in made.iter().zip(&starts) {
        let built = reveal::tracks(style, piece.rest, start, timing.duration_ms, timing.ease);
        let end = built
            .iter()
            .flat_map(|(_, keys): &(AnimProp, Vec<Key>)| keys.last())
            .map(|k| k.at_ms)
            .max()
            .unwrap_or(start);
        if end > MAX_VIDEO_MS {
            return Err(ToolError(format!(
                "the reveal runs to {end} ms and a video is at most {MAX_VIDEO_MS} ms; start earlier, or use a smaller each_ms"
            )));
        }
        ends_at = ends_at.max(end);
        for (prop, keys) in built {
            let name = prop_name(prop);
            if !props.contains(&name) {
                props.push(name);
            }
            ops.push(Operation::Animate {
                additive: false,
                video: video.clone(),
                node: piece.id.clone(),
                prop,
                keys,
                repeat: Repeat::Once,
            });
        }
        rows.push(json!({"id": piece.id, "text": piece.text, "start_ms": start, "end_ms": end}));
    }
    let hold = reveal::hold_ms(&shown_text);
    let mut report = json!({
        "style": style.as_str(),
        "group": group,
        "by": by.as_str(),
        "wrap": wrap,
        "ids": made.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        "masks": made.iter().filter_map(|m| m.mask.clone()).collect::<Vec<_>>(),
        "at_ms": timing.at_ms,
        "each_ms": timing.each_ms,
        "duration_ms": timing.duration_ms,
        "ease": timing.ease.to_string(),
        "tracks": props,
        "pieces": rows,
        "ends_at_ms": ends_at,
        "hold_ms": hold,
        "leave_no_earlier_than_ms": ends_at + hold,
        "warnings": reveal_warnings(style, by, &made, timing.each_ms),
    });
    if let Some(mode) = mode {
        report["mode"] = json!(match mode {
            Mode::Keep => "keep",
            Mode::Replace => "replace",
        });
        report["original"] = json!(original);
    } else {
        report["split"] = json!("already split: its pieces were animated as they are");
    }
    let summary = a
        .summary
        .unwrap_or_else(|| format!("{} reveal of {}", style.as_str(), name_of(doc, &id)));
    commit(ctx, ops, a.turn, summary, report)
}

fn prop_name(prop: AnimProp) -> &'static str {
    match prop {
        AnimProp::X => "x",
        AnimProp::Y => "y",
        AnimProp::Opacity => "opacity",
        AnimProp::Scale => "scale",
        _ => "other",
    }
}

fn reveal_warnings(style: Style, by: By, made: &[Made], each_ms: u64) -> Vec<String> {
    let mut out = Vec::new();
    if by == By::Chars && style != Style::Typewriter && made.len() > HEADLINE_CHARS {
        out.push(format!(
            "{} characters revealed one by one: per-character reveals are for headline moments of {HEADLINE_CHARS} or fewer; use words for anything longer",
            made.len()
        ));
    }
    let band = match by {
        By::Chars => Some((30, 50)),
        By::Words => Some((80, 120)),
        By::Lines => None,
    };
    if let Some((low, high)) = band
        && style != Style::Typewriter
        && !(low..=high).contains(&each_ms)
    {
        out.push(format!(
            "each_ms {each_ms} is outside the {low} to {high} ms that reads well for {}",
            by.as_str()
        ));
    }
    out
}
