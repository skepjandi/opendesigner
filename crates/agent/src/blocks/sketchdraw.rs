//! What the hand-made blocks share: the shapes they draw (nodes from the document or a shape
//! written in the call), the pen's settings, ink nodes, and the step tracks that make a drawing
//! boil. The geometry itself is in `sketchgeo`.

use od_core::animation::{AnimProp, Repeat};
use od_core::geometry::Rect;
use od_core::{Ease, FillRule, LineCap, LineJoin, Node, NodeKind, Paint, Stroke};
use serde_json::Value;

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Role, at, inset};
use super::scene::path_node;
use super::sketchgeo::{
    self as geo, Hand, P, Ring, Sub, compose, dots, hachure, loose_ring, outline_pass,
    roughen_lines, stream, zigzag,
};

const MAX_SHAPES: usize = 12;
const MAX_POINTS: usize = 64;
const MAX_COORD: f64 = 100_000.0;
const MAX_PATH_TEXT: usize = 4_096;

pub const SEED: Param = Param::count(
    "seed",
    "The same seed always draws the same lines; another seed draws them again, differently.",
    7,
    0,
    100_000,
);
pub const TARGETS: Param = Param::targets(
    "targets",
    "Ids of nodes in the frame to redraw by hand (rectangles, ellipses and paths keep their shape; other nodes use their box). The originals stay: hide or delete them to keep only the drawing.",
);
pub const SHAPE: Param = Param::data(
    "shape",
    "A shape to draw instead of nodes, in frame coordinates: {kind: rect, x, y, width, height, radius?}, {kind: ellipse, x, y, width, height}, {kind: circle, cx, cy, r}, {kind: polygon, points: [[x, y], ...]}, {kind: line, from: [x, y], to: [x, y]}, {kind: polyline, points}, {kind: path, d: SVG path data}, or a list of up to 12 of them. Neither given: a rectangle in the box.",
    "null",
);
pub const ROUGHNESS: Param = Param::number(
    "roughness",
    "How far the pen strays from the true line: 0 is a ruled line, 1 a careful sketch, 3 a scrawl.",
    1.0,
    0.0,
    4.0,
);
pub const BOWING: Param = Param::number(
    "bowing",
    "How much a straight stroke bows to one side, like a hand that swings from the shoulder.",
    1.0,
    0.0,
    4.0,
);
pub const STROKE_WIDTH: Param = Param::number(
    "stroke_width",
    "Pen width as a percent of the frame's shorter side.",
    0.4,
    0.05,
    3.0,
);
pub const PASSES: Param = Param::count(
    "passes",
    "How many times the pen goes over the outline, each time a little differently.",
    2,
    1,
    3,
);
pub const COLOR: Param = Param::choice(
    "color",
    "Which palette role the pen draws in.",
    "foreground",
    &["foreground", "accent", "muted", "background"],
);
pub const FILL_STYLE: Param = Param::choice(
    "fill_style",
    "How the inside is filled: none, solid (a loose wash of color), hachure (parallel lines), cross_hatch (two crossing sets), zigzag (one continuous back-and-forth line), dots (stipple) or dashed (broken hachure).",
    "none",
    &[
        "none",
        "solid",
        "hachure",
        "cross_hatch",
        "zigzag",
        "dots",
        "dashed",
    ],
);
pub const COLOR_ACCENT: Param = Param::choice(
    "color",
    "Which palette role the pen draws in.",
    "accent",
    &["foreground", "accent", "muted", "background"],
);
pub const FILL_COLOR: Param = Param::choice(
    "fill_color",
    "Which palette role the fill is drawn in.",
    "accent",
    &["foreground", "accent", "muted", "background"],
);
pub const ANGLE: Param = Param::number(
    "angle",
    "Direction of the hachure lines in degrees (0 is horizontal, -45 rises to the right).",
    -40.0,
    -90.0,
    90.0,
);
pub const GAP: Param = Param::number(
    "gap",
    "Distance between hachure lines or dots, as a percent of the frame's shorter side (kept wide enough for about a hundred lines).",
    1.8,
    0.3,
    10.0,
);
pub const WEIGHT: Param = Param::number(
    "weight",
    "Width of the fill's lines or dots as a percent of the frame's shorter side.",
    0.3,
    0.05,
    2.0,
);
pub const FILL_OPACITY: Param = Param::number(
    "fill_opacity",
    "How strongly the fill shows, 0.05 to 1.",
    0.75,
    0.05,
    1.0,
);

pub fn role_of(name: &str) -> Role {
    match name {
        "accent" => Role::Accent,
        "muted" => Role::Muted,
        "background" => Role::Background,
        _ => Role::Foreground,
    }
}

// ---------------------------------------------------------------- shapes

fn bad(why: impl std::fmt::Display) -> BlockError {
    BlockError::Param(format!("shape: {why}"))
}

fn number(v: &Value, key: &str) -> Result<f64, BlockError> {
    let n = v
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| bad(format!("{key} must be a number")))?;
    if n.is_finite() && n.abs() <= MAX_COORD {
        Ok(n)
    } else {
        Err(bad(format!("{key} is out of range")))
    }
}

fn point(v: &Value) -> Result<P, BlockError> {
    let pair = v
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| bad("a point is [x, y]"))?;
    let at = |i: usize| {
        pair[i]
            .as_f64()
            .filter(|n| n.is_finite() && n.abs() <= MAX_COORD)
            .ok_or_else(|| bad("a point needs finite numbers in range"))
    };
    Ok((at(0)?, at(1)?))
}

fn points(v: &Value, key: &str) -> Result<Vec<P>, BlockError> {
    let list = v
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| bad(format!("{key} must be a list of [x, y]")))?;
    if list.len() < 2 || list.len() > MAX_POINTS {
        return Err(bad(format!("{key} takes 2 to {MAX_POINTS} points")));
    }
    list.iter().map(point).collect()
}

fn size(v: &Value) -> Result<(f64, f64, f64, f64), BlockError> {
    let (w, h) = (number(v, "width")?, number(v, "height")?);
    if w <= 0.0 || h <= 0.0 {
        return Err(bad("width and height must be positive"));
    }
    Ok((number(v, "x")?, number(v, "y")?, w, h))
}

fn spec_rings(v: &Value) -> Result<Vec<Ring>, BlockError> {
    let kind = v
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| bad("every shape needs a kind"))?;
    let one = |ring: Option<Ring>| {
        ring.map(|r| vec![r])
            .ok_or_else(|| bad("the points make no shape"))
    };
    match kind {
        "rect" => {
            let (x, y, w, h) = size(v)?;
            let radius = v.get("radius").and_then(Value::as_f64).unwrap_or(0.0);
            Ok(vec![geo::rect_ring(
                x,
                y,
                w,
                h,
                radius.clamp(0.0, MAX_COORD),
            )])
        }
        "ellipse" => {
            let (x, y, w, h) = size(v)?;
            Ok(vec![geo::ellipse_ring(
                x + w / 2.0,
                y + h / 2.0,
                w / 2.0,
                h / 2.0,
            )])
        }
        "circle" => {
            let r = number(v, "r")?;
            if r <= 0.0 {
                return Err(bad("r must be positive"));
            }
            Ok(vec![geo::ellipse_ring(
                number(v, "cx")?,
                number(v, "cy")?,
                r,
                r,
            )])
        }
        "polygon" => one(geo::polygon_ring(&points(v, "points")?, true)),
        "polyline" => one(geo::polygon_ring(&points(v, "points")?, false)),
        "line" => one(geo::polygon_ring(
            &[
                point(v.get("from").unwrap_or(&Value::Null))?,
                point(v.get("to").unwrap_or(&Value::Null))?,
            ],
            false,
        )),
        "path" => {
            let d = v
                .get("d")
                .and_then(Value::as_str)
                .filter(|d| d.len() <= MAX_PATH_TEXT)
                .ok_or_else(|| {
                    bad(format!(
                        "d must be path data of at most {MAX_PATH_TEXT} characters"
                    ))
                })?;
            let rings = geo::path_rings(d).map_err(bad)?;
            if rings.is_empty() {
                return Err(bad("the path draws nothing"));
            }
            Ok(rings)
        }
        other => Err(bad(format!(
            "no shape kind {other:?}; use rect, ellipse, circle, polygon, polyline, line or path"
        ))),
    }
}

/// The rings a node is drawn with: its own shape, or its box.
pub fn node_rings(node: &Node, bounds: Rect) -> Vec<Ring> {
    let boxed = || {
        vec![geo::rect_ring(
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            if node.kind == NodeKind::Rect {
                node.corner_radius
            } else {
                0.0
            },
        )]
    };
    match node.kind {
        NodeKind::Ellipse => vec![geo::ellipse_ring(
            bounds.x + bounds.width / 2.0,
            bounds.y + bounds.height / 2.0,
            bounds.width / 2.0,
            bounds.height / 2.0,
        )],
        NodeKind::Path => od_core::vector::node_path(node)
            .map(|p| od_core::vector::fit_to_box(&p, node.view_box, bounds.width, bounds.height))
            .map(|p| geo::translated(&geo::rings_of(&p), bounds.x, bounds.y))
            .filter(|r| !r.is_empty())
            .unwrap_or_else(boxed),
        _ => boxed(),
    }
}

/// What a call draws: one ring set per target node or shape. With neither, `fallback` of the
/// block's box.
pub fn subjects(
    k: &Kit,
    a: &Args,
    fallback: impl FnOnce(Rect) -> Vec<Ring>,
) -> Result<Vec<Vec<Ring>>, BlockError> {
    let mut out: Vec<Vec<Ring>> = Vec::new();
    for (id, bounds) in a.targets("targets") {
        if let Some(node) = k.doc.node(id) {
            out.push(node_rings(node, *bounds));
        }
    }
    match a.data("shape") {
        Value::Null => {}
        Value::Array(list) if list.len() <= MAX_SHAPES => {
            for v in list {
                out.push(spec_rings(v)?);
            }
        }
        Value::Array(_) => return Err(bad(format!("at most {MAX_SHAPES} shapes"))),
        one => out.push(spec_rings(one)?),
    }
    if out.is_empty() {
        out.push(fallback(inset(k.b, 0.08 * k.b.width.min(k.b.height))));
    }
    Ok(out)
}

pub fn rect_of(r: Rect) -> Vec<Ring> {
    vec![geo::rect_ring(r.x, r.y, r.width, r.height, 0.0)]
}

pub fn ellipse_of(r: Rect) -> Vec<Ring> {
    vec![geo::ellipse_ring(
        r.x + r.width / 2.0,
        r.y + r.height / 2.0,
        r.width / 2.0,
        r.height / 2.0,
    )]
}

// ---------------------------------------------------------------- ink

/// How an ink node is painted.
#[derive(Default)]
pub struct Look {
    pub stroke: Option<(Paint, f64)>,
    pub fill: Option<Paint>,
    pub dash: Vec<f64>,
    pub opacity: Option<f64>,
    pub blur: Option<f64>,
}

impl Look {
    pub fn line(paint: Paint, width: f64) -> Look {
        Look {
            stroke: Some((paint, width)),
            ..Look::default()
        }
    }

    pub fn filled(paint: Paint) -> Look {
        Look {
            fill: Some(paint),
            ..Look::default()
        }
    }

    pub fn alpha(mut self, a: f64) -> Look {
        self.opacity = Some(a);
        self
    }
}

/// Adds one path node holding `subs` (held inside the frame); `false` when there was nothing.
pub fn ink(k: &mut Kit, part: &str, subs: &[Sub], look: Look) -> bool {
    let (lo, hi) = ((0.0, 0.0), (k.frame.width, k.frame.height));
    let kept: Vec<Sub> = subs.iter().map(|s| s.clamped(lo, hi)).collect();
    let Some(d) = compose(&kept) else {
        return false;
    };
    let mut node = path_node(d.d, at(d.x, d.y, d.width, d.height), look.fill.clone());
    if let Some((paint, width)) = look.stroke {
        let mut stroke = Stroke::new(paint, width);
        stroke.cap = LineCap::Round;
        stroke.join = LineJoin::Round;
        stroke.dash = look.dash;
        node.stroke = Some(stroke);
    }
    if look.fill.is_some() {
        node.fill_rule = Some(FillRule::Evenodd);
    }
    node.opacity = look.opacity;
    node.blur = look.blur;
    k.add(part, node);
    true
}

// ---------------------------------------------------------------- the pen

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillStyle {
    None,
    Solid,
    Hachure,
    CrossHatch,
    Zigzag,
    Dots,
    Dashed,
}

/// Everything a sketch is drawn with, read once from the call.
pub struct Pen {
    pub hand: Hand,
    pub width: f64,
    pub passes: u64,
    pub seed: u64,
    pub color: Paint,
    pub fill_color: Paint,
    pub fill: FillStyle,
    pub angle: f64,
    pub gap: f64,
    pub weight: f64,
    pub fill_opacity: f64,
}

pub fn pen(k: &mut Kit, a: &Args) -> Pen {
    let width = k.s(a.num("stroke_width"));
    let fill = match a.text("fill_style") {
        "solid" => FillStyle::Solid,
        "hachure" => FillStyle::Hachure,
        "cross_hatch" => FillStyle::CrossHatch,
        "zigzag" => FillStyle::Zigzag,
        "dots" => FillStyle::Dots,
        "dashed" => FillStyle::Dashed,
        _ => FillStyle::None,
    };
    Pen {
        hand: Hand {
            roughness: a.num("roughness"),
            bowing: a.num("bowing"),
            unit: width,
        },
        width,
        passes: a.num("passes") as u64,
        seed: a.num("seed") as u64,
        color: k.paint(role_of(a.text("color"))),
        fill_color: k.paint(role_of(a.text("fill_color"))),
        fill,
        angle: a.num("angle"),
        gap: k.s(a.num("gap")),
        weight: k.s(a.num("weight")),
        fill_opacity: a.num("fill_opacity"),
    }
}

/// A node of a drawing and how long it takes to draw, for sequencing a draw-on.
pub struct Part {
    pub name: String,
    /// Share of the drawing time, roughly the length of line it lays down.
    pub weight: f64,
    /// A stroke that draws on along its length; otherwise the node fades in.
    pub trims: bool,
    pub opacity: f64,
}

fn length_of(subs: &[Sub]) -> f64 {
    subs.iter()
        .map(|s| match s {
            Sub::Open(p) | Sub::Closed(p) | Sub::Poly(p) => p
                .windows(2)
                .map(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1))
                .sum(),
            Sub::Quad([a, _, b]) => (a.0 - b.0).hypot(a.1 - b.1),
            Sub::Dot(..) => 1.0,
        })
        .sum::<f64>()
        .max(1.0)
}

pub fn added(k: &mut Kit, name: String, subs: &[Sub], look: Look, trims: bool) -> Option<Part> {
    let weight = length_of(subs);
    let opacity = look.opacity.unwrap_or(1.0);
    ink(k, &name, subs, look).then_some(Part {
        name,
        weight,
        trims,
        opacity,
    })
}

/// The outline: `passes` strokes over the rings, each with its own wobble.
fn outline(k: &mut Kit, p: &Pen, rings: &[Ring], tag: &str, variant: u64) -> Vec<Part> {
    (0..p.passes)
        .filter_map(|pass| {
            let lines = outline_pass(rings, p.seed, variant * 16 + pass, p.hand);
            let subs: Vec<Sub> = lines.into_iter().map(Sub::Open).collect();
            let thin = 1.0 - 0.18 * pass as f64;
            let look = Look::line(p.color.clone(), p.width * thin).alpha(1.0 - 0.12 * pass as f64);
            added(k, format!("{tag}-line-{}", pass + 1), &subs, look, true)
        })
        .collect()
}

fn lined(p: &Pen, dashed: bool) -> Look {
    let mut look = Look::line(p.fill_color.clone(), p.weight).alpha(p.fill_opacity);
    if dashed {
        look.dash = vec![p.weight * 5.0, p.weight * 3.5];
    }
    look
}

fn hatch(
    k: &mut Kit,
    p: &Pen,
    rings: &[Ring],
    tag: &str,
    name: &str,
    variant: u64,
) -> Option<Part> {
    let angles: &[f64] = if p.fill == FillStyle::CrossHatch {
        &[0.0, 90.0]
    } else {
        &[0.0]
    };
    let mut subs = Vec::new();
    for (i, turn) in angles.iter().enumerate() {
        let gap = p.gap * if i == 0 { 1.0 } else { 1.15 };
        let lines = hachure(rings, p.angle + turn, gap);
        let mut rng = stream(p.seed, variant * 16 + 9 + i as u64);
        let amp = p.hand.roughness * p.weight * 0.9;
        let inset = p.hand.roughness * p.weight * 1.5 + p.weight;
        subs.extend(
            roughen_lines(&lines, &mut rng, amp, inset)
                .into_iter()
                .map(Sub::Quad),
        );
    }
    added(
        k,
        format!("{tag}-{name}"),
        &subs,
        lined(p, p.fill == FillStyle::Dashed),
        true,
    )
}

fn fill_parts(k: &mut Kit, p: &Pen, rings: &[Ring], tag: &str, variant: u64) -> Vec<Part> {
    match p.fill {
        FillStyle::None => Vec::new(),
        FillStyle::Solid => {
            let regions: Vec<Sub> = rings
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let mut rng = stream(p.seed, variant * 16 + 12 + i as u64);
                    Sub::Closed(loose_ring(r, &mut rng, p.hand))
                })
                .collect();
            let look = Look::filled(p.fill_color.clone()).alpha(p.fill_opacity * 0.55);
            added(k, format!("{tag}-fill"), &regions, look, false)
                .into_iter()
                .collect()
        }
        FillStyle::Hachure | FillStyle::CrossHatch | FillStyle::Dashed => {
            hatch(k, p, rings, tag, "fill", variant)
                .into_iter()
                .collect()
        }
        FillStyle::Zigzag => {
            let mut rng = stream(p.seed, variant * 16 + 10);
            let jitter = p.hand.roughness * p.weight * 1.2;
            let lines: Vec<Sub> = zigzag(rings, p.angle, p.gap)
                .into_iter()
                .map(|line| {
                    Sub::Poly(
                        line.into_iter()
                            .map(|(x, y)| {
                                (
                                    x + rng.range(-jitter, jitter),
                                    y + rng.range(-jitter, jitter),
                                )
                            })
                            .collect(),
                    )
                })
                .collect();
            added(k, format!("{tag}-fill"), &lines, lined(p, false), true)
                .into_iter()
                .collect()
        }
        FillStyle::Dots => {
            let mut rng = stream(p.seed, variant * 16 + 11);
            let r = (p.weight * 0.9).max(1.0);
            let specks: Vec<Sub> = dots(rings, p.gap, &mut rng)
                .into_iter()
                .map(|c| Sub::Dot(c, r * rng.range(0.7, 1.2)))
                .collect();
            let look = Look::filled(p.fill_color.clone()).alpha(p.fill_opacity);
            added(k, format!("{tag}-fill"), &specks, look, false)
                .into_iter()
                .collect()
        }
    }
}

/// One drawing of `rings`: the fill (under) and the outline passes. The outline parts come first
/// in the list, then the fill, which is the order they draw on in.
pub fn draw_subject(k: &mut Kit, p: &Pen, rings: &[Ring], tag: &str, variant: u64) -> Vec<Part> {
    let fill = fill_parts(k, p, rings, tag, variant);
    let mut parts = outline(k, p, rings, tag, variant);
    parts.extend(fill);
    parts
}

/// Animates parts one after another over `duration` ms: strokes draw on along their length,
/// fills without a stroke to follow fade in. A part never takes less than two frames of the
/// slowest video (its two keys must land on different frames), so a drawing of very many strokes
/// takes longer than `duration`.
pub fn draw_on(k: &mut Kit, parts: &[Part], start: u64, duration: u64) {
    let total: f64 = parts.iter().map(|p| p.weight).sum::<f64>().max(1.0);
    let least = 2.0 * frame_ms(k);
    let mut at = start as f64;
    for part in parts {
        let end = at + (duration as f64 * part.weight / total).max(least);
        let (from, to) = (at.round() as u64, end.round() as u64);
        let target = k.part(&part.name);
        if part.trims {
            k.keys(
                target,
                AnimProp::TrimEnd,
                &[(from, 0.0, Ease::Linear), (to, 1.0, Ease::EaseInOut)],
            );
        } else {
            k.keys(
                target,
                AnimProp::Opacity,
                &[(from, 0.0, Ease::Linear), (to, part.opacity, Ease::EaseOut)],
            );
        }
        at = end;
    }
}

// ---------------------------------------------------------------- boiling

/// The slowest frame rate among the document's videos (30 when there is none), so keys placed
/// whole frames apart are different frames whichever video the block is keyed in.
fn slowest_fps(k: &Kit) -> u64 {
    k.doc
        .videos
        .iter()
        .map(|v| u64::from(v.fps))
        .min()
        .unwrap_or(30)
        .max(1)
}

fn frame_ms(k: &Kit) -> f64 {
    1_000.0 / slowest_fps(k) as f64
}

/// How a boiling loop is cut: each drawing stays up for `slot` ms (a whole number of frames, at
/// least two, so a hard cut has a frame to hold before and after), and `unit` slots, a whole number
/// of rounds of the variants, make a loop that is a whole number of milliseconds long, so the
/// loop's repeats never drift off the frame grid.
pub struct Cadence {
    pub slot: f64,
    pub unit: u64,
}

pub fn cadence(k: &Kit, fps: f64, variants: u64) -> Cadence {
    let rate = slowest_fps(k);
    let frames = ((rate as f64 / fps).round().max(2.0)) as u64;
    let unit = (1..=rate)
        .map(|round| round * variants)
        .find(|slots| (slots * frames * 1_000) % rate == 0)
        .unwrap_or(rate * variants);
    Cadence {
        slot: frames as f64 * 1_000.0 / rate as f64,
        unit,
    }
}

/// A looping opacity track that shows `variant` of `count` in turn, one `slot` each, with hard
/// edges: a key one frame before the change holds the old value, so a frame shows either the old
/// or the new value and never a mix (there is no stepped ease to ask for). `slots` is the length
/// of the loop; it ends on the value it begins with.
pub fn step_track(
    k: &mut Kit,
    part: &str,
    variant: u64,
    count: u64,
    slots: u64,
    slot: f64,
    shown: f64,
) {
    let (frame, at) = (frame_ms(k), |i: u64, back: f64| {
        (i as f64 * slot - back).round() as u64
    });
    let value = |i: u64| if i % count == variant { shown } else { 0.0 };
    let mut keys = vec![(0, value(0), Ease::Linear)];
    for i in 1..=slots {
        let (before, now) = (value(i - 1), value(i % slots));
        if (before - now).abs() > f64::EPSILON {
            keys.push((at(i, frame), before, Ease::Linear));
            keys.push((at(i, 0.0), now, Ease::Linear));
        }
    }
    let end = at(slots, 0.0);
    if keys.last().is_some_and(|last| last.0 < end) {
        keys.push((end, value(0), Ease::Linear));
    }
    let target = k.part(part);
    k.track(target, AnimProp::Opacity, &keys, Repeat::Loop);
}
