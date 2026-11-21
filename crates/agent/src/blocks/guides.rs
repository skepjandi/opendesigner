//! Lines that explain: a connector that draws itself between two points or nodes (with an arrowhead,
//! flowing dots and a label), and a ring that draws itself around a node and then breathes.

use od_core::animation::AnimProp;
use od_core::geometry::Rect;
use od_core::{Ease, LineCap, LineJoin, NewNode, Paint};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Role, Style, at, center, ellipse, frame_line, rect};
use super::registry::{Block, Category, Layer, Place};
use super::scene::{alpha, lap, lighten, num, path_node, play_part, polyline, soft_shadow, solid};
use crate::scene_motion::{loop_keys, loop_samples, loop_sine};

const COLORS: &[&str] = &["accent", "foreground", "muted"];

fn role_of(name: &str) -> Role {
    match name {
        "foreground" => Role::Foreground,
        "muted" => Role::Muted,
        _ => Role::Accent,
    }
}

const START_MS: u64 = 300;

const CONNECTOR: &[Param] = &[
    Param::region(
        "from",
        "Where it starts: a node id, or {x, y} in the frame (default: left of the box).",
    ),
    Param::region(
        "to",
        "Where it ends: a node id, or {x, y} (default: right of the box).",
    ),
    Param::choice(
        "curve",
        "straight, arc (one bow) or s_curve (an S).",
        "arc",
        &["straight", "arc", "s_curve"],
    ),
    Param::number(
        "bend",
        "How far a curve bows, as a percent of its length; negative bows the other way.",
        22.0,
        -60.0,
        60.0,
    ),
    Param::count(
        "draw_ms",
        "How long the line takes to draw itself.",
        1_000,
        200,
        5_000,
    ),
    Param::flag(
        "arrow",
        "An arrowhead that pops in when the line arrives.",
        true,
    ),
    Param::flag(
        "flow",
        "Dots that stream along the line once it is drawn, in a loop.",
        true,
    ),
    Param::text(
        "label",
        "Text at the middle of the line (none by default).",
        "",
        40,
    ),
    Param::choice("color", "accent, foreground or muted.", "accent", COLORS),
    Param::number(
        "width",
        "Line width, as a percent of the frame's shorter side.",
        0.5,
        0.15,
        2.0,
    ),
];

/// The curve of a connector: a cubic from `p0` to `p1` (a straight line and an arc are cubics
/// too), with its point and heading at any fraction.
struct Curve {
    p: [(f64, f64); 4],
}

impl Curve {
    fn new(kind: &str, p0: (f64, f64), p1: (f64, f64), bend: f64) -> Curve {
        let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
        let len = dx.hypot(dy).max(1.0);
        let n = (-dy / len, dx / len);
        let bow = bend / 100.0 * len;
        let along = |t: f64, off: f64| (p0.0 + dx * t + n.0 * off, p0.1 + dy * t + n.1 * off);
        let p = match kind {
            "straight" => [p0, along(1.0 / 3.0, 0.0), along(2.0 / 3.0, 0.0), p1],
            "s_curve" => [p0, along(0.35, bow), along(0.65, -bow), p1],
            _ => [
                p0,
                along(0.2, bow * 4.0 / 3.0),
                along(0.8, bow * 4.0 / 3.0),
                p1,
            ],
        };
        Curve { p }
    }

    fn at(&self, u: f64) -> (f64, f64) {
        let v = 1.0 - u;
        let w = [v * v * v, 3.0 * v * v * u, 3.0 * v * u * u, u * u * u];
        (
            self.p.iter().zip(w).map(|(p, w)| p.0 * w).sum(),
            self.p.iter().zip(w).map(|(p, w)| p.1 * w).sum(),
        )
    }

    /// The heading at `u`, in degrees, 0 toward the right.
    fn heading(&self, u: f64) -> f64 {
        let (a, b) = (self.at((u - 0.01).max(0.0)), self.at((u + 0.01).min(1.0)));
        (b.1 - a.1).atan2(b.0 - a.0).to_degrees()
    }

    fn samples(&self) -> Vec<(f64, f64)> {
        (0..=24).map(|i| self.at(f64::from(i) / 24.0)).collect()
    }

    fn bounds(&self) -> Rect {
        let pts = self.samples();
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in pts {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        at(x0, y0, x1 - x0, y1 - y0)
    }
}

/// Where a line from the middle of `r` toward `toward` leaves `r`, plus a gap.
fn leave(r: Rect, toward: (f64, f64), gap: f64) -> (f64, f64) {
    let c = center(r);
    let (dx, dy) = (toward.0 - c.0, toward.1 - c.1);
    let len = dx.hypot(dy).max(1.0);
    let (ux, uy) = (dx / len, dy / len);
    let reach = |half: f64, u: f64| {
        if u.abs() < 1e-6 {
            f64::MAX
        } else {
            half / u.abs()
        }
    };
    let t = reach(r.width / 2.0, ux).min(reach(r.height / 2.0, uy));
    let t = if r.width < 4.0 && r.height < 4.0 {
        0.0
    } else {
        t + gap
    };
    (c.0 + ux * t, c.1 + uy * t)
}

fn path_bez(c: &Curve, origin: (f64, f64)) -> String {
    let pts: Vec<(f64, f64)> =
        c.p.iter()
            .map(|p| (p.0 - origin.0, p.1 - origin.1))
            .collect();
    format!(
        "M{},{}C{},{} {},{} {},{}",
        num(pts[0].0),
        num(pts[0].1),
        num(pts[1].0),
        num(pts[1].1),
        num(pts[2].0),
        num(pts[2].1),
        num(pts[3].0),
        num(pts[3].1),
    )
}

fn arrowhead(size: f64, color: Paint, tip: (f64, f64), heading: f64) -> NewNode {
    let a = size;
    let shape = polyline(
        &[
            (1.9 * a, a),
            (0.35 * a, 0.2 * a),
            (0.75 * a, a),
            (0.35 * a, 1.8 * a),
        ],
        true,
    );
    let center = (
        tip.0 - 0.9 * a * heading.to_radians().cos(),
        tip.1 - 0.9 * a * heading.to_radians().sin(),
    );
    let mut n = path_node(
        shape,
        at(center.0 - a, center.1 - a, 2.0 * a, 2.0 * a),
        Some(color.clone()),
    )
    .outline(color, a * 0.14);
    if let Some(s) = n.stroke.as_mut() {
        s.join = LineJoin::Round;
    }
    n.rotation = Some(heading);
    n
}

fn connector(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let safe = k.b;
    let from_r = a.region("from");
    let to_r = a.region("to");
    let default_from = (safe.x + safe.width * 0.14, safe.y + safe.height * 0.66);
    let default_to = (safe.x + safe.width * 0.86, safe.y + safe.height * 0.34);
    let (c0, c1) = (
        from_r.map_or(default_from, center),
        to_r.map_or(default_to, center),
    );
    let gap = k.s(1.2);
    let p0 = from_r.map_or(c0, |r| leave(r, c1, gap));
    let p1 = to_r.map_or(c1, |r| leave(r, c0, gap));
    if (p1.0 - p0.0).hypot(p1.1 - p0.1) < k.s(3.0) {
        return Err(BlockError::Param(
            "`from` and `to` are too close for a connector: move them apart".into(),
        ));
    }
    let role = role_of(a.text("color"));
    let paint = k.paint(role);
    let width = k.s(a.num("width"));
    let curve = Curve::new(a.text("curve"), p0, p1, a.num("bend"));
    let draw = a.num("draw_ms") as u64;
    let arrive = START_MS + draw;
    let pad = width * 2.0 + k.s(0.4);
    let mut box_ = curve.bounds();
    box_ = at(
        box_.x - pad,
        box_.y - pad,
        box_.width + 2.0 * pad,
        box_.height + 2.0 * pad,
    );
    let mut line =
        path_node(path_bez(&curve, (box_.x, box_.y)), box_, None).outline(paint.clone(), width);
    if let Some(s) = line.stroke.as_mut() {
        s.cap = LineCap::Round;
        s.join = LineJoin::Round;
    }
    k.add("line", line);
    k.keys(
        k.part("line"),
        AnimProp::TrimEnd,
        &[
            (START_MS, 0.0, Ease::Linear),
            (arrive, 1.0, Ease::EaseInOut),
        ],
    );
    if a.flag("flow") {
        flow(k, &curve, &paint, width, arrive);
    }
    if a.flag("arrow") {
        let size = (width * 3.2).max(k.s(1.3));
        k.add("arrow", arrowhead(size, paint, p1, curve.heading(1.0)));
        let part = k.part("arrow");
        k.keys(
            part.clone(),
            AnimProp::Opacity,
            &[
                (arrive - 150, 0.0, Ease::Linear),
                (arrive + 100, 1.0, Ease::EaseOut),
            ],
        );
        k.keys(
            part.clone(),
            AnimProp::ScaleX,
            &[
                (arrive - 150, 0.3, Ease::Linear),
                (arrive + 450, 1.0, Ease::EaseOutBack),
            ],
        );
        k.keys(
            part,
            AnimProp::ScaleY,
            &[
                (arrive - 150, 0.3, Ease::Linear),
                (arrive + 450, 1.0, Ease::EaseOutBack),
            ],
        );
    }
    let label = a.text("label").to_string();
    if !label.is_empty() {
        label_pill(k, &curve, &label, arrive)?;
    }
    Ok(())
}

/// Dots that run along the line after it is drawn, spaced evenly, each fading in and out at the
/// ends so the loop shows no jump.
fn flow(k: &mut Kit, curve: &Curve, paint: &Paint, width: f64, arrive: u64) {
    let d = width * 2.4;
    let color = k.rgba(paint);
    k.begin("flow");
    for j in 0..3 {
        let first = 0.12 + 0.3 * f64::from(j);
        let start = curve.at(first);
        let name = format!("dot-{}", j + 1);
        k.add(
            &name,
            ellipse(
                at(start.0 - d / 2.0, start.1 - d / 2.0, d, d),
                solid(lighten(color, 0.55)),
            ),
        );
        let series = lap(first, 3_000);
        let of =
            |f: &dyn Fn(f64) -> f64| series.iter().map(|&(t, u)| (t, f(u))).collect::<Vec<_>>();
        play_part(
            k,
            &name,
            &loop_keys(AnimProp::X, &of(&|u| curve.at(u).0 - d / 2.0)),
        );
        play_part(
            k,
            &name,
            &loop_keys(AnimProp::Y, &of(&|u| curve.at(u).1 - d / 2.0)),
        );
        let ends = |u: f64| ((u / 0.14).clamp(0.0, 1.0)).min(((1.0 - u) / 0.14).clamp(0.0, 1.0));
        play_part(k, &name, &loop_keys(AnimProp::Opacity, &of(&ends)));
    }
    k.end();
    k.keys(
        k.part("flow"),
        AnimProp::Opacity,
        &[
            (arrive, 0.0, Ease::Linear),
            (arrive + 500, 1.0, Ease::EaseOut),
        ],
    );
}

/// A label on a pill beside the middle of the line, on the side that is higher on screen.
fn label_pill(k: &mut Kit, curve: &Curve, text: &str, arrive: u64) -> Result<(), BlockError> {
    let mid = curve.at(0.5);
    let ahead = curve.at(0.55);
    let behind = curve.at(0.45);
    let (tx, ty) = (ahead.0 - behind.0, ahead.1 - behind.1);
    let len = tx.hypot(ty).max(1.0);
    let mut n = (-ty / len, tx / len);
    if n.1 > 0.0 {
        n = (-n.0, -n.1);
    }
    let ty_ = k.ty(k.s(2.3), 600.0, Role::Foreground);
    let (tw, th) = k.measure(text, &ty_, 0.0);
    let (padx, pady) = (k.s(1.4), k.s(0.8));
    let (w, h) = (tw.ceil() + 2.0 * padx, th.ceil() + 2.0 * pady);
    let off = h * 0.5 + k.s(1.6);
    let f = k.frame;
    let x = (mid.0 + n.0 * off - w / 2.0).clamp(k.margin, (f.width - w - k.margin).max(k.margin));
    let y = (mid.1 + n.1 * off - h / 2.0).clamp(k.margin, (f.height - h - k.margin).max(k.margin));
    let bg = k.paint(Role::Background);
    let bg = alpha(k.rgba(&bg), 0.88);
    k.begin("label");
    k.add(
        "label-pill",
        rect(at(x, y, w, h), solid(bg)).radius(h / 2.0),
    );
    k.text(
        "label-text",
        text,
        &ty_,
        x + padx,
        y + pady,
        tw.ceil() + 2.0,
    );
    k.end();
    k.reveal(k.part("label"), arrive, 450, k.s(0.8));
    Ok(())
}

const RING: &[Param] = &[
    Param::region(
        "target",
        "The node (or {x, y, width, height} in the frame) to ring; default is the middle of the box.",
    ),
    Param::choice(
        "shape",
        "ring (an ellipse), rounded (a rounded box) or auto: an ellipse for roughly square targets, a rounded box for wide ones.",
        "auto",
        &["auto", "ring", "rounded"],
    ),
    Param::number(
        "padding",
        "Space between the target and the ring, as a percent of the target's shorter side.",
        16.0,
        0.0,
        60.0,
    ),
    Param::choice("color", "accent, foreground or muted.", "accent", COLORS),
    Param::count(
        "draw_ms",
        "How long the ring takes to draw itself.",
        800,
        200,
        4_000,
    ),
    Param::flag(
        "pulse",
        "The ring breathes and a faint echo spreads outward, in a loop.",
        true,
    ),
];

/// The ring's box: the target grown by the padding, kept inside the frame.
fn ring_box(k: &Kit, target: Rect, shape_ellipse: bool, pad: f64) -> Rect {
    let grow = if shape_ellipse { 1.2 } else { 1.0 };
    let (w, h) = (
        target.width * grow + 2.0 * pad,
        target.height * grow + 2.0 * pad,
    );
    let c = center(target);
    let f = k.frame;
    let (w, h) = (w.min(f.width - 4.0), h.min(f.height - 4.0));
    at(
        (c.0 - w / 2.0).clamp(2.0, f.width - w - 2.0),
        (c.1 - h / 2.0).clamp(2.0, f.height - h - 2.0),
        w,
        h,
    )
}

fn ring_shape(r: Rect, ellipse_shape: bool, paint: Paint, width: f64) -> NewNode {
    if ellipse_shape {
        super::kit::ring(r, paint, width)
    } else {
        frame_line(r, paint, width).radius(r.height.min(r.width) * 0.3)
    }
}

fn highlight_ring(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let b = k.b;
    let side = k.s(28.0);
    let target = a.region("target").unwrap_or_else(|| {
        let c = center(b);
        at(c.0 - side / 2.0, c.1 - side / 2.0, side, side)
    });
    let target = if target.width < 4.0 && target.height < 4.0 {
        at(target.x - side / 2.0, target.y - side / 2.0, side, side)
    } else {
        target
    };
    let wide =
        target.width / target.height.max(1.0) > 1.6 || target.height / target.width.max(1.0) > 1.6;
    let ellipse_shape = match a.text("shape") {
        "ring" => true,
        "rounded" => false,
        _ => !wide,
    };
    let pad = target.width.min(target.height) * a.num("padding") / 100.0 + k.s(0.8);
    let r = ring_box(k, target, ellipse_shape, pad);
    let paint = k.paint(role_of(a.text("color")));
    let stroke = k.s(0.55);
    let color = k.rgba(&paint);
    let draw = a.num("draw_ms") as u64;
    let arrive = START_MS + draw;
    let mut main = ring_shape(r, ellipse_shape, paint.clone(), stroke);
    main.shadow = Some(soft_shadow(alpha(color, 0.6), 0.0, pad.max(k.s(1.2))));
    if let Some(s) = main.stroke.as_mut() {
        s.cap = LineCap::Round;
    }
    k.add("ring", main);
    k.keys(
        k.part("ring"),
        AnimProp::TrimEnd,
        &[
            (START_MS, 0.0, Ease::Linear),
            (arrive, 1.0, Ease::EaseInOut),
        ],
    );
    if a.flag("pulse") {
        play_part(
            k,
            "ring",
            &loop_sine(AnimProp::Scale, 1.0, 0.025, 2_000, 0.0),
        );
        k.begin("echo");
        k.add(
            "echo-ring",
            ring_shape(r, ellipse_shape, solid(alpha(color, 0.6)), stroke * 0.6),
        );
        k.end();
        let t = k.part("echo");
        k.keys(
            t,
            AnimProp::Opacity,
            &[
                (0, 0.0, Ease::Linear),
                (arrive, 0.0, Ease::Linear),
                (arrive + 400, 1.0, Ease::EaseOut),
            ],
        );
        let swell = |u: f64| {
            if u < 0.85 {
                1.0 + 0.35 * u / 0.85
            } else {
                1.35 - 0.35 * (u - 0.85) / 0.15
            }
        };
        play_part(
            k,
            "echo-ring",
            &loop_samples(AnimProp::Scale, 2_000, 20, swell),
        );
        let fade = loop_samples(AnimProp::Opacity, 2_000, 12, |u| {
            0.7 * (1.0 - u) * (1.0 - u).sqrt() * ((u / 0.08).min(1.0))
        });
        play_part(k, "echo-ring", &fade);
    }
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "connector",
        category: Category::Motion,
        about: "An arrow or line between two points or two existing nodes, straight or curved, that draws itself on with an eased trim, then pops an arrowhead, streams dots along it and shows an optional label.",
        tags: &[
            "connector",
            "arrow",
            "line",
            "link",
            "flow",
            "diagram",
            "draw",
            "annotation",
            "path",
        ],
        params: CONNECTOR,
        place: Place::Safe,
        layer: Layer::Front,
        build: connector,
    },
    Block {
        id: "highlight_ring",
        category: Category::Motion,
        about: "A ring (or rounded box) that draws itself on around a target node with a trim, then breathes while a faint echo spreads outward: points the viewer at one thing.",
        tags: &[
            "highlight",
            "ring",
            "circle",
            "pulse",
            "attention",
            "focus",
            "annotation",
            "draw",
        ],
        params: RING,
        place: Place::Safe,
        layer: Layer::Front,
        build: highlight_ring,
    },
];
