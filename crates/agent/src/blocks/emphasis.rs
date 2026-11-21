//! Hand-drawn emphasis: `marker_underline` underlines a node or box like a highlighter or a
//! pen, `scribble_circle` rings it, points an arrow at it or crosses it out. Each is one stroke
//! path that draws itself on.

use std::f64::consts::TAU;

use od_core::geometry::Rect;

use super::BlockError;
use super::args::{Args, Param};
use super::kit::Kit;
use super::registry::{Block, Category, Layer, Place};
use super::sketchdraw::{COLOR_ACCENT, Look, Part, SEED, added, draw_on, role_of};
use super::sketchgeo::{Hand, P, Run, Sub, stream, wobble};
use crate::scene_motion::Rng;

const TARGET: Param = Param::region(
    "target",
    "The node (an id in the frame) or box {x, y, width, height} to mark. Without one, a box in the middle of the block's box.",
);
const THICKNESS: Param = Param::number(
    "thickness",
    "Pen width as a percent of the frame's shorter side.",
    0.6,
    0.1,
    5.0,
);
const MARKER_THICKNESS: Param = Param::number(
    "thickness",
    "Marker or pen width as a percent of the frame's shorter side.",
    2.0,
    0.1,
    6.0,
);
const ROUGHNESS: Param = Param::number(
    "roughness",
    "How far the pen strays from a clean line, 0 to 3.",
    1.0,
    0.0,
    3.0,
);
const DURATION: Param = Param::count(
    "duration_ms",
    "How long the mark takes to draw on.",
    700,
    150,
    5_000,
);
const PAD: Param = Param::number(
    "padding",
    "Space between the target and the mark as a percent of the frame's shorter side.",
    1.2,
    0.0,
    10.0,
);

const UNDERLINE: &[Param] = &[
    TARGET,
    Param::choice(
        "style",
        "marker (a highlighter band gone over twice), double (two thin lines), wavy or scribble (back and forth).",
        "marker",
        &["marker", "double", "wavy", "scribble"],
    ),
    MARKER_THICKNESS,
    PAD,
    ROUGHNESS,
    COLOR_ACCENT,
    DURATION,
    SEED,
];

const SCRIBBLE: &[Param] = &[
    TARGET,
    Param::choice(
        "mark",
        "circle (a scribbled loop around it), arrow (pointing at it) or cross (crossed out).",
        "circle",
        &["circle", "arrow", "cross"],
    ),
    Param::number(
        "loops",
        "How many times the circle goes around, 1 to 3.",
        1.7,
        1.0,
        3.0,
    ),
    Param::number(
        "arrow_from",
        "Where the arrow comes from, as the direction from the target in degrees clockwise from east (215 is the upper left).",
        215.0,
        0.0,
        360.0,
    ),
    Param::number(
        "arrow_length",
        "Arrow length as a percent of the frame's shorter side.",
        14.0,
        3.0,
        40.0,
    ),
    THICKNESS,
    PAD,
    ROUGHNESS,
    COLOR_ACCENT,
    DURATION,
    SEED,
];

fn target_of(k: &Kit, a: &Args) -> Rect {
    a.region("target").unwrap_or_else(|| {
        let (w, h) = (k.b.width * 0.4, k.b.height * 0.1);
        Rect {
            x: k.b.x + (k.b.width - w) / 2.0,
            y: k.b.y + (k.b.height - h) / 2.0,
            width: w,
            height: h,
        }
    })
}

fn stroke(from: P, to: P, rng: &mut Rng, hand: Hand) -> Vec<P> {
    wobble(
        &Run {
            pts: vec![from, to],
            looped: false,
        },
        rng,
        hand,
    )
}

fn marker_parts(k: &mut Kit, a: &Args, r: Rect, y: f64) -> Vec<Part> {
    let (t, seed) = (k.s(a.num("thickness")), a.num("seed") as u64);
    let pad = k.s(a.num("padding"));
    let (x0, x1) = (r.x - pad, r.x + r.width + pad);
    let color = k.paint(role_of(a.text("color")));
    let hand = Hand {
        roughness: a.num("roughness"),
        bowing: 0.6,
        unit: t * 0.8,
    };
    let mut rng = stream(seed, 1);
    let mut parts = Vec::new();
    match a.text("style") {
        "double" => {
            for (i, dy) in [-0.55, 0.65].into_iter().enumerate() {
                let line = stroke(
                    (x0, y + dy * t),
                    (x1, y + dy * t * 1.2),
                    &mut rng,
                    Hand {
                        unit: t * 0.5,
                        ..hand
                    },
                );
                let look = Look::line(color.clone(), t * 0.4);
                parts.extend(added(
                    k,
                    format!("mark-{}", i + 1),
                    &[Sub::Open(line)],
                    look,
                    true,
                ));
            }
        }
        "wavy" => {
            let wave = t * 5.0;
            let n = (((x1 - x0) / (wave / 4.0)) as usize).clamp(4, 160);
            let phase = rng.range(0.0, TAU);
            let pts: Vec<P> = (0..=n)
                .map(|i| {
                    let x = x0 + (x1 - x0) * i as f64 / n as f64;
                    let swing = (TAU * (x - x0) / wave + phase).sin() * t * 0.7;
                    (
                        x,
                        y + swing + rng.range(-1.0, 1.0) * t * 0.08 * hand.roughness,
                    )
                })
                .collect();
            parts.extend(added(
                k,
                "mark-1".into(),
                &[Sub::Open(pts)],
                Look::line(color, t * 0.45),
                true,
            ));
        }
        "scribble" => {
            let step = (t * 1.6).max(4.0);
            let n = (((x1 - x0) / step) as usize).clamp(4, 200);
            let pts: Vec<P> = (0..=n)
                .map(|i| {
                    let up = if i % 2 == 0 { -1.0 } else { 1.0 };
                    let x = x0 + (x1 - x0) * i as f64 / n as f64;
                    (x, y + up * t * rng.range(0.5, 1.0))
                })
                .collect();
            parts.extend(added(
                k,
                "mark-1".into(),
                &[Sub::Poly(pts)],
                Look::line(color, t * 0.28),
                true,
            ));
        }
        _ => {
            let there = stroke((x0, y), (x1, y - t * 0.25), &mut rng, hand);
            let back = stroke((x1, y - t * 0.5), (x0 + pad, y + t * 0.1), &mut rng, hand);
            let wide = Look::line(color.clone(), t).alpha(0.75);
            parts.extend(added(k, "mark-1".into(), &[Sub::Open(there)], wide, true));
            let thin = Look::line(color, t * 0.5).alpha(0.65);
            parts.extend(added(k, "mark-2".into(), &[Sub::Open(back)], thin, true));
        }
    }
    parts
}

fn marker_underline(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let r = target_of(k, a);
    let y = r.y + r.height + k.s(a.num("padding")) * 0.6 + k.s(a.num("thickness")) * 0.9;
    let parts = marker_parts(k, a, r, y);
    draw_on(k, &parts, 0, a.num("duration_ms") as u64);
    Ok(())
}

fn rotated(p: P, about: P, turn: f64) -> P {
    let (dx, dy) = (p.0 - about.0, p.1 - about.1);
    (
        about.0 + dx * turn.cos() - dy * turn.sin(),
        about.1 + dx * turn.sin() + dy * turn.cos(),
    )
}

/// A loop around the box that does not quite close: it starts just inside, goes round `loops`
/// times with a wobbling radius and drifts outward, so the ends cross like a quick scribble.
fn loop_points(r: Rect, pad: f64, loops: f64, rough: f64, rng: &mut Rng) -> Vec<P> {
    let c = (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let (rx, ry) = (r.width / 2.0 * 1.15 + pad * 1.5, r.height / 2.0 * 1.5 + pad);
    let (start, tilt) = (rng.range(0.0, TAU), rng.range(-0.05, 0.05));
    let (p1, p2) = (rng.range(0.0, TAU), rng.range(0.0, TAU));
    let n = (34.0 * loops) as usize;
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let a = start + TAU * loops * t;
            let wobble = 1.0 + 0.035 * rough * ((3.0 * a + p1).sin() + 0.6 * (5.0 * a + p2).sin());
            let grow = 0.94 + 0.16 * t;
            let p = (
                c.0 + rx * grow * wobble * a.cos(),
                c.1 + ry * grow * wobble * a.sin(),
            );
            rotated(p, c, tilt)
        })
        .collect()
}

fn arrow_subs(r: Rect, (from, len, pad): (f64, f64, f64), rng: &mut Rng, hand: Hand) -> Vec<Sub> {
    let c = (r.x + r.width / 2.0, r.y + r.height / 2.0);
    let dir = from.to_radians();
    let (dx, dy) = (dir.cos(), dir.sin());
    let reach = (r.width / 2.0 / dx.abs().max(1e-6)).min(r.height / 2.0 / dy.abs().max(1e-6));
    let tip_at = reach + pad;
    let tip = (c.0 + dx * tip_at, c.1 + dy * tip_at);
    let tail = (tip.0 + dx * len, tip.1 + dy * len);
    let shaft = stroke(
        tail,
        tip,
        rng,
        Hand {
            bowing: 1.8,
            ..hand
        },
    );
    let arm = len * 0.28;
    let mut head = |sign: f64| {
        let back = dir + sign * 0.5;
        let end = (tip.0 + back.cos() * arm, tip.1 + back.sin() * arm);
        stroke(
            tip,
            end,
            rng,
            Hand {
                roughness: hand.roughness * 0.5,
                bowing: 0.3,
                ..hand
            },
        )
    };
    let (left, right) = (head(1.0), head(-1.0));
    vec![Sub::Open(shaft), Sub::Open(left), Sub::Open(right)]
}

fn cross_subs(r: Rect, pad: f64, rng: &mut Rng, hand: Hand) -> Vec<Sub> {
    let (x0, y0, x1, y1) = (
        r.x - pad,
        r.y - pad,
        r.x + r.width + pad,
        r.y + r.height + pad,
    );
    vec![
        Sub::Open(stroke((x0, y0), (x1, y1), rng, hand)),
        Sub::Open(stroke((x1, y0), (x0, y1), rng, hand)),
    ]
}

fn scribble_circle(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let r = target_of(k, a);
    let t = k.s(a.num("thickness"));
    let (pad, rough) = (k.s(a.num("padding")), a.num("roughness"));
    let mut rng = stream(a.num("seed") as u64, 2);
    let hand = Hand {
        roughness: rough,
        bowing: 1.0,
        unit: t,
    };
    let (loops, from, length) = (
        a.num("loops"),
        a.num("arrow_from"),
        k.s(a.num("arrow_length")),
    );
    let subs = match a.text("mark") {
        "arrow" => arrow_subs(r, (from, length, pad), &mut rng, hand),
        "cross" => cross_subs(r, pad, &mut rng, hand),
        _ => vec![Sub::Open(loop_points(r, pad, loops, rough, &mut rng))],
    };
    let color = k.paint(role_of(a.text("color")));
    let part = added(k, "mark".into(), &subs, Look::line(color, t), true);
    draw_on(
        k,
        &part.into_iter().collect::<Vec<_>>(),
        0,
        a.num("duration_ms") as u64,
    );
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "marker_underline",
        category: Category::Overlays,
        about: "A hand-drawn underline under a node or box that draws itself on: a highlighter band gone over twice, a double line, a wavy line or a back-and-forth scribble. Wobbles and overshoots like a real pen.",
        tags: &[
            "underline",
            "marker",
            "highlighter",
            "emphasis",
            "hand drawn",
            "scribble",
            "draw on",
            "highlight",
        ],
        params: UNDERLINE,
        place: Place::Safe,
        layer: Layer::Front,
        build: marker_underline,
    },
    Block {
        id: "scribble_circle",
        category: Category::Overlays,
        about: "A scribbled loop around a node or box, an arrow pointing at it, or a cross through it, drawn on like a quick pen mark. For emphasis in an explainer or a sketch-look video.",
        tags: &[
            "circle",
            "scribble",
            "arrow",
            "cross",
            "emphasis",
            "annotation",
            "hand drawn",
            "draw on",
            "highlight",
        ],
        params: SCRIBBLE,
        place: Place::Safe,
        layer: Layer::Front,
        build: scribble_circle,
    },
];
