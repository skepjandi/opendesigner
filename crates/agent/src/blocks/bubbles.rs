//! `speech_bubble` and `thought_bubble`: a bubble sized to its text, a tail (or a trail of
//! circles) that points at a node or a point, a soft shadow, a pop-in with overshoot from the
//! tail's tip and a gentle float for ever.

use od_core::animation::AnimProp;
use od_core::geometry::Rect;
use od_core::{Ease, NewNode, NodeKind, Paint, TextAlign};

use super::BlockError;
use super::args::{Args, Param};
use super::art::{
    Grp, Key, Named, ellipse, linear, ordered, path, rect_of, rounded, shade, shadow, sway, tint,
};
use super::kit::{Kit, Role, Ty, at};
use super::registry::{Block, Category, Layer, Place};

const MAX_LINES: u32 = 4;
const SHAPES: &[&str] = &["rounded", "oval"];

const SPEECH_PARAMS: &[Param] = &[
    Param::text(
        "text",
        "What is said (up to four lines).",
        "Did you know?",
        140,
    ),
    Param::region(
        "target",
        "A node id or {x, y} the tail points at (a node: its upper middle). Without one it points down.",
    ),
    Param::choice(
        "shape",
        "rounded rectangle or oval body.",
        "rounded",
        SHAPES,
    ),
    Param::number(
        "size",
        "Text size as a percent of the frame's shorter side.",
        5.0,
        1.5,
        12.0,
    ),
    Param::text(
        "bubble_color",
        "#rrggbb or {token}; empty takes the foreground role.",
        "",
        40,
    ),
    Param::text(
        "text_color",
        "#rrggbb or {token}; empty takes whichever reads best.",
        "",
        40,
    ),
    Param::count(
        "pop_ms",
        "How long the pop-in takes (0: it is simply there).",
        420,
        0,
        3000,
    ),
    Param::flag(
        "float",
        "A gentle bob for as long as the video plays.",
        true,
    ),
];

const THOUGHT_PARAMS: &[Param] = &[
    Param::text(
        "text",
        "What is thought (up to four lines).",
        "Hmm, what if...",
        140,
    ),
    Param::region(
        "target",
        "A node id or {x, y} the trail of circles leads to. Without one it leads down.",
    ),
    Param::number(
        "size",
        "Text size as a percent of the frame's shorter side.",
        5.0,
        1.5,
        12.0,
    ),
    Param::text(
        "bubble_color",
        "#rrggbb or {token}; empty takes the foreground role.",
        "",
        40,
    ),
    Param::text(
        "text_color",
        "#rrggbb or {token}; empty takes whichever reads best.",
        "",
        40,
    ),
    Param::count(
        "pop_ms",
        "How long the pop-in takes (0: it is simply there).",
        480,
        0,
        3000,
    ),
    Param::flag(
        "float",
        "A gentle bob for as long as the video plays.",
        true,
    ),
];

fn bubble_box(frame: Rect, margin: f64) -> Rect {
    let portrait = frame.height > frame.width * 1.1;
    let w = frame.width * if portrait { 0.8 } else { 0.42 };
    let h = frame.height * if portrait { 0.16 } else { 0.24 };
    at(
        frame.x + (frame.width - w) / 2.0,
        frame.y + margin * 1.4,
        w,
        h,
    )
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "speech_bubble",
        category: Category::Characters,
        about: "A speech bubble sized to its text, with a tail that points at a character or point; \
                it pops in with overshoot from the tail's tip and bobs gently.",
        tags: &[
            "speech", "bubble", "dialogue", "say", "comic", "callout", "talk",
        ],
        params: SPEECH_PARAMS,
        place: Place::Custom(bubble_box),
        layer: Layer::Front,
        build: speech,
    },
    Block {
        id: "thought_bubble",
        category: Category::Characters,
        about: "A thought bubble: an oval around the text and a trail of shrinking circles toward a \
                character or point; the circles pop in one after another.",
        tags: &["thought", "bubble", "think", "idea", "cloud", "comic"],
        params: THOUGHT_PARAMS,
        place: Place::Custom(bubble_box),
        layer: Layer::Front,
        build: thought,
    },
];

fn speech(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    build(k, a, a.text("shape") == "oval", false)
}

fn thought(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    build(k, a, true, true)
}

fn colour(k: &Kit, a: &Args, name: &'static str) -> Result<Option<Paint>, BlockError> {
    let s = a.text(name);
    if s.is_empty() {
        return Ok(None);
    }
    let paint = Paint::parse(s)?;
    paint.resolve(&k.doc.tokens)?;
    Ok(Some(paint))
}

/// The point the tail aims at: a node's upper middle, or the given point.
fn aim(a: &Args) -> Option<(f64, f64)> {
    a.region("target").map(|r| {
        if r.height > 0.0 {
            (r.x + r.width / 2.0, r.y + r.height * 0.18)
        } else {
            (r.x, r.y)
        }
    })
}

/// The bubble body and where it sits: hugging its text, pushed toward the target inside the box.
struct Body {
    rect: Rect,
    ty: Ty,
    text_box: Rect,
    text: String,
}

fn body(
    k: &Kit,
    a: &Args,
    oval: bool,
    target: Option<(f64, f64)>,
    ink: &Paint,
) -> Result<Body, BlockError> {
    let size = k.s(a.num("size"));
    let pad = size * if oval { 1.0 } else { 0.7 };
    let inner = if oval { 0.74 } else { 1.0 };
    let max_w = (k.b.width * inner - 2.0 * pad).max(size);
    let max_h = (k.b.height * inner - 2.0 * pad).max(size);
    let mut ty = Ty::of(size, 600.0, ink.clone())
        .align(TextAlign::Center)
        .line(1.15);
    let text = a.text("text").to_string();
    let (fitted, h) = k.fit(&text, &ty, max_w, max_h, MAX_LINES)?;
    ty = fitted;
    let widest = k.measure(&text, &ty, max_w).0.ceil() + 6.0;
    let (tw, th) = (widest.min(max_w), h);
    let (bw, bh) = ((tw + 2.0 * pad) / inner, (th + 2.0 * pad) / inner);
    let (cx, cy) = (k.b.x + k.b.width / 2.0, k.b.y + k.b.height / 2.0);
    let lean = |t: f64, c: f64, room: f64| match target {
        Some(_) if t > c + room => 1.0,
        Some(_) if t < c - room => -1.0,
        _ => 0.0,
    };
    let (tx, ty_) = target.unwrap_or((cx, cy + k.b.height));
    let x = cx - bw / 2.0 + lean(tx, cx, bw / 2.0) * (k.b.width - bw) / 2.0;
    let y = cy - bh / 2.0 + lean(ty_, cy, bh / 2.0) * (k.b.height - bh) / 2.0;
    let rect = rect_of(x, y, bw, bh);
    let text_box = rect_of(rect.x + (bw - tw) / 2.0, rect.y + (bh - th) / 2.0, tw, th);
    Ok(Body {
        rect,
        ty,
        text_box,
        text,
    })
}

fn text_node(k: &Kit, b: &Body, ink: &Paint) -> NewNode {
    let mut n = NewNode::new(NodeKind::Text);
    n.x = b.text_box.x;
    n.y = b.text_box.y;
    n.width = b.text_box.width;
    n.height = b.text_box.height.ceil();
    n.text = Some(b.text.clone());
    n.font_size = Some(b.ty.size);
    n.font_weight = Some(b.ty.weight);
    n.font_family = k.font.clone();
    n.text_align = Some(TextAlign::Center);
    n.line_height = Some(b.ty.line);
    n.fill = Some(ink.clone());
    n
}

/// Where the tail leaves the body and where its tip lands.
struct Tail {
    base: (f64, f64),
    tip: (f64, f64),
    along_x: bool,
}

fn tail_of(k: &Kit, rect: Rect, target: Option<(f64, f64)>, oval: bool) -> Tail {
    let (cx, cy) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
    let (tx, ty) = target.unwrap_or((cx, rect.y + rect.height * 2.0));
    let inside =
        tx > rect.x && tx < rect.x + rect.width && ty > rect.y && ty < rect.y + rect.height;
    let (tx, ty) = if inside {
        (cx, rect.y + rect.height * 2.0)
    } else {
        (tx, ty)
    };
    let (dx, dy) = (tx - cx, ty - cy);
    let along_x = dx.abs() * rect.height > dy.abs() * rect.width;
    let reach = (k.s(14.0)).min(dx.hypot(dy) * 0.6);
    let inset = if oval { 0.78 } else { 1.0 };
    let base = if along_x {
        let y = (cy + dy * 0.3).clamp(rect.y + rect.height * 0.25, rect.y + rect.height * 0.75);
        (cx + dx.signum() * rect.width / 2.0 * inset, y)
    } else {
        let x = (cx + dx * 0.3).clamp(rect.x + rect.width * 0.25, rect.x + rect.width * 0.75);
        (x, cy + dy.signum() * rect.height / 2.0 * inset)
    };
    let (bx, by) = (tx - base.0, ty - base.1);
    let len = bx.hypot(by).max(1.0);
    let reach = reach.max(k.s(5.0)).min(len);
    let mut tip = (base.0 + bx / len * reach, base.1 + by / len * reach);
    let m = k.margin * 0.5;
    tip = (
        tip.0.clamp(k.frame.x + m, k.frame.x + k.frame.width - m),
        tip.1.clamp(k.frame.y + m, k.frame.y + k.frame.height - m),
    );
    Tail { base, tip, along_x }
}

/// A curved tail: a wedge whose base sits inside the body (so no seam shows) and whose sides bow
/// slightly.
fn tail_shape(k: &Kit, t: &Tail, width: f64, fill: &Paint) -> NewNode {
    let half = width / 2.0;
    let (b, tip) = (t.base, t.tip);
    let (p1, p2) = if t.along_x {
        ((b.0, b.1 - half), (b.0, b.1 + half))
    } else {
        ((b.0 - half, b.1), (b.0 + half, b.1))
    };
    let pad = width * 0.1;
    let xs = [p1.0, p2.0, tip.0];
    let ys = [p1.1, p2.1, tip.1];
    let min = |v: &[f64; 3]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64; 3]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let r = rect_of(
        min(&xs) - pad,
        min(&ys) - pad,
        (max(&xs) - min(&xs)).max(1.0) + 2.0 * pad,
        (max(&ys) - min(&ys)).max(1.0) + 2.0 * pad,
    );
    let local = |p: (f64, f64)| (p.0 - r.x, p.1 - r.y);
    let (l1, l2, lt) = (local(p1), local(p2), local(tip));
    let bow = |a: (f64, f64)| ((a.0 + lt.0) / 2.0 + (lt.1 - a.1) * 0.0, (a.1 + lt.1) / 2.0);
    let (c1, c2) = (bow(l1), bow(l2));
    let d = format!(
        "M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Z",
        l1.0, l1.1, c1.0, c1.1, lt.0, lt.1, c2.0, c2.1, l2.0, l2.1
    );
    path(r, d, Some(fill.clone())).named(k, "tail")
}

fn fills(k: &mut Kit, a: &Args) -> Result<(Paint, Paint), BlockError> {
    let base = match colour(k, a, "bubble_color")? {
        Some(p) => p,
        None => k.paint(Role::Foreground),
    };
    let ink = match colour(k, a, "text_color")? {
        Some(p) => p,
        None => k.ink_on(&base),
    };
    Ok((base, ink))
}

fn build(k: &mut Kit, a: &Args, oval: bool, thought: bool) -> Result<(), BlockError> {
    let target = aim(a);
    let (base, ink) = fills(k, a)?;
    let b = body(k, a, oval, target, &ink)?;
    let c = k.rgba(&base);
    let fill = linear(90.0, &[(0.0, tint(c, 0.12)), (1.0, shade(c, 0.07))]);
    let tail = tail_of(k, b.rect, target, oval);
    let mut group =
        Grp::new(k, "bubble", group_box(&b.rect, &tail, k)).pivot(tail.tip.0, tail.tip.1);
    group = group.shadowed(shadow(0.0, k.s(1.1), k.s(2.0), 0.24));
    if thought {
        add_trail(k, &mut group, &b.rect, &tail, &fill);
    } else {
        group.add(tail_shape(
            k,
            &tail,
            k.s(6.5).min(b.rect.height * 0.55),
            &fill,
        ));
    }
    group.add(body_shape(k, &b.rect, oval, fill).named(k, "bubble_body"));
    group.add(text_node(k, &b, &ink).named(k, "text"));
    let rest_y = group.node.y;
    k.add("bubble", group.node);
    animate(k, a, rest_y, thought);
    Ok(())
}

fn body_shape(_k: &Kit, r: &Rect, oval: bool, fill: Paint) -> NewNode {
    if oval {
        ellipse(*r, fill)
    } else {
        rounded(*r, r.height.min(r.width) * 0.28, fill)
    }
}

fn group_box(body: &Rect, tail: &Tail, k: &Kit) -> Rect {
    let m = k.s(1.0);
    let x0 = body.x.min(tail.tip.0) - m;
    let y0 = body.y.min(tail.tip.1) - m;
    let x1 = (body.x + body.width).max(tail.tip.0) + m;
    let y1 = (body.y + body.height).max(tail.tip.1) + m;
    rect_of(x0, y0, x1 - x0, y1 - y0)
}

/// Three circles shrinking from the body toward the tip.
fn add_trail(k: &Kit, g: &mut Grp, body: &Rect, tail: &Tail, fill: &Paint) {
    let (bx, by) = tail.base;
    let (dx, dy) = (tail.tip.0 - bx, tail.tip.1 - by);
    for (i, (at_, r)) in [(0.18, 0.9), (0.55, 0.62), (0.95, 0.4)]
        .into_iter()
        .enumerate()
    {
        let radius = body.height.min(k.s(8.0)) * 0.22 * r + k.s(0.5);
        let c = (bx + dx * at_, by + dy * at_);
        g.add(
            ellipse(
                rect_of(c.0 - radius, c.1 - radius, radius * 2.0, radius * 2.0),
                fill.clone(),
            )
            .named(k, &format!("trail_{i}")),
        );
    }
}

fn animate(k: &mut Kit, a: &Args, rest_y: f64, thought: bool) {
    let pop = a.num("pop_ms") as u64;
    let target = k.part("bubble");
    if pop > 0 {
        let scale: Vec<Key> = vec![(0, 0.01, Ease::Linear), (pop, 1.0, Ease::EaseOutBack)];
        k.keys(target.clone(), AnimProp::Scale, &ordered(scale));
        let text: Vec<Key> = vec![
            (0, 0.0, Ease::Linear),
            (pop / 3, 0.0, Ease::Linear),
            (pop, 1.0, Ease::EaseOut),
        ];
        k.keys(k.part("text"), AnimProp::Opacity, &ordered(text));
        if thought {
            for i in 0..3u64 {
                let part = format!("trail_{i}");
                let delay = pop * (2 - i) / 6;
                let keys: Vec<Key> = vec![
                    (0, 0.0, Ease::Linear),
                    (delay, 0.0, Ease::Linear),
                    (delay + pop / 2, 1.0, Ease::EaseOut),
                ];
                k.keys(k.part(&part), AnimProp::Opacity, &ordered(keys));
            }
        }
    }
    if a.flag("float") {
        let (keys, repeat) = sway(rest_y, rest_y - k.s(0.7), 1300.0);
        k.track(target, AnimProp::Y, &keys, repeat);
    }
}
