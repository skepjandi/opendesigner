//! `emote`: the painted marks a cartoon uses instead of words (an exclamation, a sweat drop,
//! hearts, a lightbulb, drifting zzz) placed beside a character's head. Each mark pops in with an
//! overshoot at `start_ms`, stays for `hold_ms` and has its own idle motion that never stops.

use od_core::animation::{AnimProp, Repeat};
use od_core::geometry::Rect;
use od_core::{Ease, NewNode, Paint, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::art::{
    Grp, Key, Named, Rng, Tracks, centered, ellipse, glow, line, lit, ms, path, pen, rect_of,
    rounded, shade, solid, sway, tint, turned,
};
use super::characters::{parse, solid_rgba};
use super::faces::{heart_d, star_d};
use super::kit::{Kit, Role, at, mix};
use super::registry::{Block, Category, Layer, Place};

const MARKS: &[&str] = &[
    "exclamation",
    "question",
    "sweat",
    "heart",
    "lightbulb",
    "zzz",
    "anger",
    "steam",
    "sparkle",
    "music",
    "rain_cloud",
    "dizzy",
    "stars",
];
const SIDES: &[&str] = &[
    "auto",
    "above",
    "upper_right",
    "upper_left",
    "right",
    "left",
];

const PARAMS: &[Param] = &[
    Param::choice("mark", "Which mark.", "exclamation", MARKS),
    Param::region(
        "target",
        "A node id (a character's head, say) or {x, y} the mark sits beside. Its size follows the node's width. Without a target the mark fills the box.",
    ),
    Param::choice(
        "side",
        "Where it sits around the target; auto picks the usual place for the mark.",
        "auto",
        SIDES,
    ),
    Param::number(
        "scale",
        "Size as a multiple of the usual (about half the target's width).",
        1.0,
        0.4,
        3.0,
    ),
    Param::count(
        "count",
        "How many drops, hearts, notes, sparkles, stars or spirals (0: the usual number for the mark; up to 6).",
        0,
        0,
        6,
    ),
    Param::text(
        "color",
        "#rrggbb or {token}. Empty: the accent role, and a fitting hue for drops, hearts, anger and rain.",
        "",
        40,
    ),
    Param::count(
        "pop_ms",
        "How long the pop-in takes (0: it is simply there).",
        380,
        0,
        3000,
    ),
    Param::count(
        "seed",
        "Desynchronises the idle motion of several marks.",
        1,
        0,
        9999,
    ),
];

fn emote_box(frame: Rect, margin: f64) -> Rect {
    let size = frame.width.min(frame.height) * 0.2;
    at(
        frame.x + (frame.width - size) / 2.0,
        frame.y + margin * 1.5,
        size,
        size,
    )
}

pub const BLOCKS: &[Block] = &[Block {
    id: "emote",
    category: Category::Characters,
    about: "A painted mark instead of text that a cartoon character wears: exclamation, question, \
            sweat drops, hearts, lightbulb, drifting zzz, anger vein, steam, sparkles, music \
            notes, rain cloud, dizzy spirals or orbiting stars. Give a `target` (the character's \
            head) and it sits beside it; with a video it pops in with overshoot at start_ms, \
            lives hold_ms and keeps its own idle motion (a wobble, a slide, a twinkle, a drift).",
    tags: &[
        "emote",
        "emoji",
        "reaction",
        "exclamation",
        "question",
        "sweat",
        "heart",
        "lightbulb",
        "idea",
        "zzz",
        "sleep",
        "anger",
        "sparkle",
        "music",
        "rain",
        "dizzy",
        "stars",
        "mark",
        "thought",
    ],
    params: PARAMS,
    place: Place::Custom(emote_box),
    layer: Layer::Front,
    build: build_emote,
}];

/// What a mark needs to draw and move itself.
struct Cx<'a> {
    k: &'a Kit<'a>,
    rect: Rect,
    /// The mark's unit: about its height.
    unit: f64,
    fill: Paint,
    fill_c: Rgba,
    edge: Paint,
    edge_w: f64,
    count: usize,
    seed: u64,
}

impl Cx<'_> {
    /// `r` in fractions of the mark's rectangle.
    fn cell(&self, fx: f64, fy: f64, fw: f64, fh: f64) -> Rect {
        let r = self.rect;
        rect_of(
            r.x + r.width * fx,
            r.y + r.height * fy,
            r.width * fw,
            r.height * fh,
        )
    }

    /// A group the size of `r` that turns and scales about `pivot` (fractions of `r`).
    fn piece(&self, part: &str, r: Rect, pivot: (f64, f64)) -> Grp {
        Grp::new(self.k, part, r).pivot(r.x + r.width * pivot.0, r.y + r.height * pivot.1)
    }

    /// Where a piece rests, relative to the mark's group.
    fn rel(&self, r: Rect) -> (f64, f64) {
        (r.x - self.rect.x, r.y - self.rect.y)
    }

    fn filled(&self, n: NewNode) -> NewNode {
        let mut n = n;
        n.stroke = Some(pen(self.edge.clone(), self.edge_w));
        n
    }

    fn shape(&self, r: Rect, d: String, name: &str) -> NewNode {
        self.filled(path(r, d, Some(self.fill.clone())).named(self.k, name))
    }

    /// An open curve drawn with an outline: the wide edge colour under the narrower fill colour.
    fn stroked(&self, r: Rect, d: &str, width: f64, name: &str) -> Vec<NewNode> {
        vec![
            line(
                r,
                d.to_string(),
                self.edge.clone(),
                width + 2.0 * self.edge_w,
            )
            .named(self.k, &format!("{name}_edge")),
            line(r, d.to_string(), self.fill.clone(), width).named(self.k, name),
        ]
    }
}

fn read_color(k: &Kit, a: &Args) -> Result<Option<Paint>, BlockError> {
    let s = a.text("color");
    if s.is_empty() {
        return Ok(None);
    }
    let paint = Paint::parse(s)?;
    paint.resolve(&k.doc.tokens)?;
    Ok(Some(paint))
}

/// The hue a mark has by nature (drops are blue, hearts red, a lightbulb yellow), eased toward
/// the accent so it belongs to the scene's palette. They stand out from a character painted in
/// the accent.
fn natural(mark: &str) -> &'static str {
    match mark {
        "exclamation" => "#ffc933",
        "question" => "#58b6ff",
        "sweat" => "#5fc3ff",
        "heart" => "#ff4f76",
        "lightbulb" | "dizzy" | "stars" => "#ffdc4d",
        "zzz" => "#bcd6ff",
        "anger" => "#ff4a3d",
        "steam" => "#eef2f8",
        "sparkle" => "#ffe97a",
        "music" => "#f4f7ff",
        _ => "#8da3c0",
    }
}

fn default_count(mark: &str) -> usize {
    match mark {
        "sweat" => 2,
        "heart" => 3,
        "zzz" => 3,
        "sparkle" => 3,
        "music" => 2,
        "dizzy" => 2,
        "stars" => 3,
        "steam" => 3,
        _ => 1,
    }
}

/// Width and height of the mark as multiples of its unit, and where it sits by default.
fn footprint(mark: &str) -> (f64, f64, &'static str) {
    match mark {
        "exclamation" => (0.5, 1.0, "upper_right"),
        "question" => (0.7, 1.0, "upper_right"),
        "sweat" => (0.9, 1.0, "upper_left"),
        "heart" => (1.2, 1.1, "upper_right"),
        "lightbulb" => (0.9, 1.2, "above"),
        "zzz" => (1.2, 1.2, "upper_right"),
        "anger" => (0.9, 0.9, "upper_left"),
        "steam" => (1.2, 0.9, "upper_right"),
        "sparkle" => (1.2, 1.2, "upper_right"),
        "music" => (1.3, 1.1, "upper_right"),
        "rain_cloud" => (1.7, 1.5, "above"),
        "dizzy" => (1.7, 0.8, "above"),
        _ => (1.8, 0.7, "above"),
    }
}

/// The rectangle the mark takes and the point it grows out of.
fn place(k: &Kit, a: &Args, mark: &str) -> (Rect, (f64, f64)) {
    let (fw, fh, usual) = footprint(mark);
    let scale = a.num("scale");
    let side_name = a.text("side");
    let Some(target) = a.region("target") else {
        let unit = (k.b.width / fw).min(k.b.height / fh) * 0.92 * scale.min(1.0);
        let (w, h) = (unit * fw, unit * fh);
        let r = rect_of(
            k.b.x + (k.b.width - w) / 2.0,
            k.b.y + (k.b.height - h) / 2.0,
            w,
            h,
        );
        return (r, (r.x + w / 2.0, r.y + h));
    };
    let node = target.width > 0.0;
    let base = if node { target.width * 0.7 } else { k.s(7.0) };
    let unit = (base * scale).clamp(k.s(3.0), k.s(30.0));
    let (w, h) = (unit * fw, unit * fh);
    let side = if side_name == "auto" {
        usual
    } else {
        side_name
    };
    let (cx, top, right) = (
        target.x + target.width / 2.0,
        target.y,
        target.x + target.width,
    );
    let (x, y, pivot) = match side {
        "above" => (cx - w / 2.0, top - h * 0.95, (0.5, 1.0)),
        "upper_left" => (
            target.x + target.width * 0.22 - w,
            top - h * 0.55,
            (1.0, 1.0),
        ),
        "right" => (
            right + w * 0.05,
            target.y + target.height / 2.0 - h / 2.0,
            (0.0, 0.5),
        ),
        "left" => (
            target.x - w * 1.05,
            target.y + target.height / 2.0 - h / 2.0,
            (1.0, 0.5),
        ),
        _ => (target.x + target.width * 0.78, top - h * 0.55, (0.0, 1.0)),
    };
    let safe = k.safe();
    let x = x.clamp(safe.x, (safe.x + safe.width - w).max(safe.x));
    let y = y.clamp(safe.y, (safe.y + safe.height - h).max(safe.y));
    (rect_of(x, y, w, h), (x + w * pivot.0, y + h * pivot.1))
}

fn build_emote(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mark = a.text("mark");
    let (rect, grow) = place(k, a, mark);
    let accent = k.paint(Role::Accent);
    let accent_c = k.rgba(&accent);
    let given = read_color(k, a)?;
    let (fill, fill_c) = match given {
        Some(p) => {
            let c = k.rgba(&p);
            (p, c)
        }
        None => {
            let c = parse(&mix(solid_rgba(natural(mark)), accent_c, 0.12));
            (Paint::Color(c), c)
        }
    };
    let unit = rect.height / footprint(mark).1;
    let cx = Cx {
        k,
        rect,
        unit,
        fill: lit(&fill, fill_c),
        fill_c,
        edge: shade(fill_c, 0.55),
        edge_w: (unit * 0.04).max(1.2),
        count: match a.num("count") as usize {
            0 => default_count(mark),
            n => n,
        },
        seed: a.num("seed") as u64,
    };
    let mut root = Grp::new(k, "mark", rect).pivot(grow.0, grow.1);
    let mut t = Tracks::default();
    draw(&cx, mark, &mut root, &mut t);
    let mut group = root.node;
    group.shadow = Some(super::art::shadow(0.0, unit * 0.04, unit * 0.06, 0.22));
    let pop = a.num("pop_ms") as u64;
    k.add("mark", group);
    pop_in(&mut t, pop);
    t.emit(k)
}

fn pop_in(t: &mut Tracks, pop: u64) {
    if pop == 0 {
        return;
    }
    let scale: Vec<Key> = vec![(0, 0.01, Ease::Linear), (pop, 1.0, Ease::EaseOutBack)];
    t.set("mark", AnimProp::Scale, scale, Repeat::Once);
}

fn draw(c: &Cx, mark: &str, root: &mut Grp, t: &mut Tracks) {
    match mark {
        "exclamation" => exclamation(c, root, t),
        "question" => question(c, root, t),
        "sweat" => sweat(c, root, t),
        "heart" => hearts(c, root, t),
        "lightbulb" => lightbulb(c, root, t),
        "zzz" => zzz(c, root, t),
        "anger" => anger(c, root, t),
        "steam" => steam(c, root, t),
        "sparkle" => sparkles(c, root, t),
        "music" => music(c, root, t),
        "rain_cloud" => rain_cloud(c, root, t),
        "dizzy" => dizzy(c, root, t),
        _ => stars(c, root, t),
    }
}

/// A staggered phase for the `i`th piece of a mark so its pieces never move in step.
fn phase(c: &Cx, i: usize, lo: f64, hi: f64) -> f64 {
    Rng::new(c.seed.wrapping_add(i as u64 * 31), 11).range(lo, hi)
}

/// A looping track that waits `delay` ms, then runs `cycle` (first key at its rest value) and
/// rests until `period`.
fn loop_after(delay: u64, period: u64, cycle: &[Key]) -> Vec<Key> {
    let mut keys: Vec<Key> = Vec::new();
    if delay > 0 {
        let (first_v, _) = cycle.first().map_or((0.0, Ease::Linear), |k| (k.1, k.2));
        keys.push((0, first_v, Ease::Linear));
    }
    keys.extend(cycle.iter().map(|&(at, v, e)| (delay + at, v, e)));
    let last_v = cycle.last().map_or(0.0, |k| k.1);
    keys.push((
        period.max(delay + cycle.last().map_or(0, |k| k.0) + 40),
        last_v,
        Ease::Linear,
    ));
    keys
}

fn exclamation(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let bar = c.cell(0.1, 0.0, 0.8, 0.72);
    let (w, h) = (bar.width, bar.height);
    let d = format!(
        "M{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} L{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Z",
        w * 0.04,
        h * 0.14,
        w * 0.5,
        h * -0.1,
        w * 0.96,
        h * 0.14,
        w * 0.7,
        h * 0.93,
        w * 0.5,
        h * 1.02,
        w * 0.3,
        h * 0.93
    );
    let mut bar_group = c.piece("item_0", bar, (0.5, 1.0));
    bar_group.add(c.shape(bar, d, "bar"));
    let dot_side = c.rect.width * 0.5;
    let dot = centered(
        c.rect.x + c.rect.width / 2.0,
        c.rect.y + c.rect.height * 0.86,
        dot_side,
        dot_side,
    );
    let mut dot_group = c.piece("item_1", dot, (0.5, 0.5));
    dot_group.add(c.filled(ellipse(dot, c.fill.clone()).named(c.k, "dot")));
    root.add_group(bar_group);
    root.add_group(dot_group);
    t.looping(
        "item_0",
        AnimProp::Rotation,
        sway(-6.0, 6.0, 260.0 + phase(c, 0, 0.0, 60.0)),
    );
    let (_, y) = c.rel(dot);
    t.looping("item_1", AnimProp::Y, sway(y, y - c.unit * 0.04, 260.0));
}

fn question(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let hook = c.cell(0.05, 0.0, 0.9, 0.7);
    let (w, h) = (hook.width, hook.height);
    let d = format!(
        "M{:.2},{:.2} C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} L{:.2},{:.2}",
        w * 0.12,
        h * 0.3,
        w * 0.1,
        h * -0.02,
        w * 0.9,
        h * -0.02,
        w * 0.88,
        h * 0.3,
        w * 0.86,
        h * 0.52,
        w * 0.5,
        h * 0.52,
        w * 0.5,
        h * 0.74,
        w * 0.5,
        h * 0.94
    );
    let stroke = c.unit * 0.17;
    let mut hook_group = c.piece("item_0", hook, (0.5, 1.0));
    for n in c.stroked(hook, &d, stroke, "hook") {
        hook_group.add(n);
    }
    let dot = centered(
        c.rect.x + c.rect.width * 0.5,
        c.rect.y + c.rect.height * 0.9,
        stroke * 1.3,
        stroke * 1.3,
    );
    let mut dot_group = c.piece("item_1", dot, (0.5, 0.5));
    dot_group.add(c.filled(ellipse(dot, c.fill.clone()).named(c.k, "dot")));
    root.add_group(hook_group);
    root.add_group(dot_group);
    t.looping(
        "item_0",
        AnimProp::Rotation,
        sway(-9.0, 9.0, 420.0 + phase(c, 0, 0.0, 80.0)),
    );
    let (_, y) = c.rel(dot);
    t.looping("item_1", AnimProp::Y, sway(y, y - c.unit * 0.05, 420.0));
}

/// A drop with a pointed top, in `r`.
fn drop_d(w: f64, h: f64) -> String {
    format!(
        "M{:.2},0 C{:.2},{:.2} {w:.2},{:.2} {w:.2},{:.2} C{w:.2},{:.2} {:.2},{h:.2} {:.2},{h:.2} \
         C{:.2},{h:.2} 0,{:.2} 0,{:.2} C0,{:.2} {:.2},{:.2} {:.2},0 Z",
        w / 2.0,
        w * 0.55,
        h * 0.3,
        h * 0.52,
        h * 0.7,
        h * 0.88,
        w * 0.78,
        w / 2.0,
        w * 0.22,
        h * 0.88,
        h * 0.7,
        h * 0.52,
        w * 0.45,
        h * 0.3,
        w / 2.0
    )
}

fn sweat(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    for i in 0..c.count.min(3) {
        let (fx, fy, s) = [(0.5, 0.08, 0.5), (0.2, 0.3, 0.38), (0.8, 0.22, 0.32)][i];
        let h = c.rect.height * s;
        let r = rect_of(
            c.rect.x + c.rect.width * fx - h * 0.37,
            c.rect.y + c.rect.height * fy,
            h * 0.74,
            h,
        );
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 0.0));
        g.add(c.shape(r, drop_d(r.width, r.height), &format!("drop_{i}")));
        let glint = centered(
            r.x + r.width * 0.34,
            r.y + r.height * 0.62,
            r.width * 0.2,
            r.height * 0.22,
        );
        g.add(ellipse(glint, solid("#ffffff")).named(c.k, &format!("drop_shine_{i}")));
        root.add_group(g);
        let period = ms(phase(c, i, 760.0, 980.0));
        let (_, y) = c.rel(r);
        let fall = (c.rect.y + c.rect.height - (r.y + r.height)).clamp(0.0, c.unit * 0.3);
        let slide = loop_after(
            0,
            period,
            &[(0, y, Ease::Linear), (period - 40, y + fall, Ease::EaseIn)],
        );
        let fade = loop_after(
            0,
            period,
            &[
                (0, 0.0, Ease::Linear),
                (period / 5, 1.0, Ease::EaseOut),
                (period * 3 / 5, 1.0, Ease::Linear),
                (period - 40, 0.0, Ease::EaseIn),
            ],
        );
        t.set(&part, AnimProp::Y, slide, Repeat::Loop);
        t.set(&part, AnimProp::Opacity, fade, Repeat::Loop);
    }
}

fn hearts(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let n = c.count.max(1);
    for i in 0..n {
        let big = if n == 1 {
            1.0
        } else {
            [1.0, 0.7, 0.52, 0.42, 0.36, 0.3][i.min(5)]
        };
        let (w, h) = (c.rect.width * 0.62 * big, c.rect.height * 0.6 * big);
        let cx = c.rect.x + c.rect.width * [0.5, 0.24, 0.76, 0.38, 0.64, 0.14][i.min(5)];
        let bottom = c.rect.y + c.rect.height * if n == 1 { 0.8 } else { 0.96 };
        let r = rect_of(cx - w / 2.0, bottom - h, w, h);
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 1.0));
        g.add(c.shape(r, heart_d(r.width, r.height), &format!("heart_{i}")));
        let shine = centered(
            r.x + r.width * 0.28,
            r.y + r.height * 0.28,
            r.width * 0.2,
            r.height * 0.14,
        );
        g.add(
            turned(ellipse(shine, solid("#ffffff")), -30.0).named(c.k, &format!("heart_shine_{i}")),
        );
        root.add_group(g);
        if n == 1 {
            t.looping(&part, AnimProp::Scale, sway(1.0, 1.14, 330.0));
            continue;
        }
        let period = ms(phase(c, i, 1100.0, 1500.0));
        let (_, y) = c.rel(r);
        let up = y.max(0.0).min(c.unit * 0.45);
        let rise = loop_after(
            0,
            period,
            &[(0, y, Ease::Linear), (period - 40, y - up, Ease::EaseOut)],
        );
        let fade = loop_after(
            0,
            period,
            &[
                (0, 0.0, Ease::Linear),
                (period / 6, 1.0, Ease::EaseOut),
                (period * 2 / 3, 1.0, Ease::Linear),
                (period - 40, 0.0, Ease::EaseIn),
            ],
        );
        let grow = loop_after(
            0,
            period,
            &[
                (0, 0.6, Ease::Linear),
                (period / 4, 1.0, Ease::EaseOutBack),
                (period - 40, 1.1, Ease::Linear),
            ],
        );
        t.set(&part, AnimProp::Y, rise, Repeat::Loop);
        t.set(&part, AnimProp::Opacity, fade, Repeat::Loop);
        t.set(&part, AnimProp::Scale, grow, Repeat::Loop);
    }
}

fn lightbulb(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let globe = c.cell(0.12, 0.12, 0.76, 0.64);
    let halo = c.cell(0.0, 0.0, 1.0, 0.82);
    let mut glow_group = c.piece("item_0", halo, (0.5, 0.5));
    glow_group.add(ellipse(halo, glow(c.fill_c, 0.55)).named(c.k, "halo"));
    root.add_group(glow_group);
    let mut rays = c.piece("item_1", halo, (0.5, 0.5));
    let (rx, ry) = (halo.width / 2.0, halo.height / 2.0);
    let centre = (globe.x + globe.width / 2.0, globe.y + globe.height / 2.0);
    for i in 0..5 {
        let angle = (-160.0 + 35.0 * f64::from(i)).to_radians();
        let (near, far) = (0.78, 0.97);
        let p = |k: f64| {
            (
                centre.0 + angle.cos() * rx * k,
                centre.1 + angle.sin() * ry * k,
            )
        };
        let (a, b) = (p(near), p(far));
        let r = rect_of(
            a.0.min(b.0) - 2.0,
            a.1.min(b.1) - 2.0,
            (a.0 - b.0).abs() + 4.0,
            (a.1 - b.1).abs() + 4.0,
        );
        let d = format!(
            "M{:.2},{:.2} L{:.2},{:.2}",
            a.0 - r.x,
            a.1 - r.y,
            b.0 - r.x,
            b.1 - r.y
        );
        rays.add(line(r, d, c.fill.clone(), c.unit * 0.07).named(c.k, &format!("ray_{i}")));
    }
    root.add_group(rays);
    let neck = c.cell(0.3, 0.74, 0.4, 0.14);
    let base = c.cell(0.36, 0.86, 0.28, 0.12);
    let grey = Paint::Color(parse(&mix(solid_rgba("#c9d1dc"), c.fill_c, 0.1)));
    let mut bulb = c.piece("item_2", c.cell(0.0, 0.0, 1.0, 1.0), (0.5, 0.5));
    for (r, name) in [(neck, "neck"), (base, "base")] {
        let mut n = rounded(r, r.height * 0.4, grey.clone()).named(c.k, name);
        n.stroke = Some(pen(c.edge.clone(), c.edge_w));
        bulb.add(n);
    }
    bulb.add(c.filled(ellipse(globe, linear_bulb(c)).named(c.k, "globe")));
    let filament = c.cell(0.34, 0.3, 0.32, 0.36);
    let (w, h) = (filament.width, filament.height);
    let d = format!(
        "M{:.2},{h:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{h:.2}",
        w * 0.15,
        w * 0.15,
        h * 0.35,
        w * 0.4,
        h * 0.1,
        w * 0.7,
        h * 0.35,
        w * 0.85
    );
    bulb.add(line(filament, d, c.edge.clone(), c.unit * 0.04).named(c.k, "filament"));
    let shine = c.cell(0.26, 0.2, 0.18, 0.16);
    bulb.add(turned(ellipse(shine, solid("#ffffff")), -30.0).named(c.k, "bulb_shine"));
    root.add_group(bulb);
    t.looping(
        "item_0",
        AnimProp::Opacity,
        sway(0.45, 1.0, 360.0 + phase(c, 0, 0.0, 80.0)),
    );
    t.looping(
        "item_1",
        AnimProp::Scale,
        sway(0.92, 1.1, 300.0 + phase(c, 1, 0.0, 80.0)),
    );
    t.looping("item_1", AnimProp::Rotation, sway(-6.0, 6.0, 900.0));
}

fn linear_bulb(c: &Cx) -> Paint {
    super::art::radial(
        0.38,
        0.35,
        0.75,
        &[
            (0.0, tint(c.fill_c, 0.8)),
            (0.55, tint(c.fill_c, 0.2)),
            (1.0, Paint::Color(c.fill_c)),
        ],
    )
}

fn zzz(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let n = c.count.clamp(1, 4);
    for i in 0..n {
        let k = 0.55 + 0.4 * (i as f64);
        let size = c.rect.height * 0.36 * k.min(1.3);
        let (x, y) = (
            c.rect.x + c.rect.width * (0.05 + 0.2 * i as f64),
            c.rect.y + c.rect.height * (0.78 - 0.24 * i as f64) - size * 0.5,
        );
        let r = rect_of(x, y, size, size);
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 0.5));
        let d = format!(
            "M{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2}",
            size * 0.12,
            size * 0.14,
            size * 0.88,
            size * 0.14,
            size * 0.12,
            size * 0.86,
            size * 0.88,
            size * 0.86
        );
        for node in c.stroked(r, &d, size * 0.22, &format!("z_{i}")) {
            g.add(node);
        }
        root.add_group(g);
        let period = ms(phase(c, i, 1700.0, 2100.0));
        let delay = ms(i as f64 * 420.0);
        let (rx, ry) = c.rel(r);
        let up = ry.max(0.0).min(c.unit * 0.3);
        let right = (c.rect.width - rx - size).max(0.0).min(c.unit * 0.2);
        let cycle_x = loop_after(
            delay,
            period + delay,
            &[
                (0, rx, Ease::Linear),
                (period - 80, rx + right, Ease::EaseOut),
            ],
        );
        let cycle_y = loop_after(
            delay,
            period + delay,
            &[(0, ry, Ease::Linear), (period - 80, ry - up, Ease::EaseOut)],
        );
        let fade = loop_after(
            delay,
            period + delay,
            &[
                (0, 0.0, Ease::Linear),
                (period / 4, 1.0, Ease::EaseOut),
                (period * 2 / 3, 1.0, Ease::Linear),
                (period - 80, 0.0, Ease::EaseIn),
            ],
        );
        t.set(&part, AnimProp::X, cycle_x, Repeat::Loop);
        t.set(&part, AnimProp::Y, cycle_y, Repeat::Loop);
        t.set(&part, AnimProp::Opacity, fade, Repeat::Loop);
        t.looping(
            &part,
            AnimProp::Rotation,
            sway(-10.0, 10.0, 700.0 + 120.0 * i as f64),
        );
    }
}

fn anger(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let whole = c.cell(0.08, 0.08, 0.84, 0.84);
    let mut veins = c.piece("item_0", whole, (0.5, 0.5));
    let radius = whole.width.min(whole.height) / 2.0;
    let (cx, cy) = (whole.width / 2.0, whole.height / 2.0);
    let at_angle = |degrees: f64, r: f64| {
        let a = degrees.to_radians();
        (cx + a.cos() * r, cy + a.sin() * r)
    };
    let mut d = String::new();
    for i in 0..4 {
        let middle = 45.0 + 90.0 * f64::from(i);
        let (a, b) = (
            at_angle(middle - 32.0, radius),
            at_angle(middle + 32.0, radius),
        );
        let ctrl = at_angle(middle, radius * 0.3);
        d.push_str(&format!(
            "M{:.2},{:.2} A{radius:.2},{radius:.2} 0 0 1 {:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} Z ",
            a.0, a.1, b.0, b.1, ctrl.0, ctrl.1, a.0, a.1
        ));
    }
    veins.add(c.shape(whole, d, "veins"));
    root.add_group(veins);
    t.looping(
        "item_0",
        AnimProp::Scale,
        sway(0.88, 1.12, 230.0 + phase(c, 0, 0.0, 40.0)),
    );
    t.looping("item_0", AnimProp::Rotation, sway(-5.0, 5.0, 300.0));
}

fn steam(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let n = c.count.clamp(1, 4);
    let cloud = Paint::Color(c.fill_c);
    for i in 0..n {
        let size = c.rect.height * (0.5 - 0.06 * i as f64);
        let x = c.rect.x + c.rect.width * (0.15 + 0.24 * i as f64);
        let r = rect_of(x, c.rect.y + c.rect.height - size * 1.1, size, size);
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 0.5));
        let lobes = [(0.0, 0.1, 0.62), (0.34, 0.0, 0.62), (0.2, 0.3, 0.7)];
        for (j, (lx, ly, ls)) in lobes.into_iter().enumerate() {
            let lr = rect_of(
                r.x + r.width * lx,
                r.y + r.height * ly,
                r.width * ls,
                r.height * ls,
            );
            let mut n = ellipse(lr, cloud.clone()).named(c.k, &format!("puff_{i}_{j}"));
            n.stroke = Some(pen(c.edge.clone(), c.edge_w * 0.8));
            g.add(n);
        }
        let blob = rect_of(
            r.x + r.width * 0.05,
            r.y + r.height * 0.1,
            r.width * 0.9,
            r.height * 0.8,
        );
        g.add(ellipse(blob, cloud.clone()).named(c.k, &format!("puff_core_{i}")));
        root.add_group(g);
        let period = ms(phase(c, i, 900.0, 1200.0));
        let delay = ms(i as f64 * 260.0);
        let (_, y) = c.rel(r);
        let up = y.max(0.0).min(c.unit * 0.5);
        let rise = loop_after(
            delay,
            period + delay,
            &[(0, y, Ease::Linear), (period - 60, y - up, Ease::EaseOut)],
        );
        let fade = loop_after(
            delay,
            period + delay,
            &[
                (0, 0.0, Ease::Linear),
                (period / 5, 0.95, Ease::EaseOut),
                (period - 60, 0.0, Ease::EaseIn),
            ],
        );
        let grow = loop_after(
            delay,
            period + delay,
            &[(0, 0.5, Ease::Linear), (period - 60, 1.25, Ease::EaseOut)],
        );
        t.set(&part, AnimProp::Y, rise, Repeat::Loop);
        t.set(&part, AnimProp::Opacity, fade, Repeat::Loop);
        t.set(&part, AnimProp::Scale, grow, Repeat::Loop);
    }
}

fn sparkles(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let n = c.count.clamp(1, 6);
    let spots = [
        (0.62, 0.38, 0.62),
        (0.18, 0.7, 0.4),
        (0.82, 0.78, 0.34),
        (0.3, 0.18, 0.3),
        (0.5, 0.86, 0.28),
        (0.9, 0.12, 0.26),
    ];
    for (i, (fx, fy, s)) in spots.into_iter().take(n).enumerate() {
        let size = c.rect.height * s;
        let r = centered(
            c.rect.x + c.rect.width * fx,
            c.rect.y + c.rect.height * fy,
            size,
            size,
        );
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 0.5));
        g.add(c.shape(r, star_d(r.width, r.height), &format!("sparkle_{i}")));
        root.add_group(g);
        let span = phase(c, i, 330.0, 560.0);
        t.looping(&part, AnimProp::Scale, sway(0.45, 1.15, span));
        t.looping(&part, AnimProp::Rotation, sway(-18.0, 18.0, span * 2.0));
    }
}

fn music(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let n = c.count.clamp(1, 3);
    for i in 0..n {
        let size = c.rect.height * [0.72, 0.56, 0.44][i];
        let (fx, fy) = [(0.28, 0.5), (0.74, 0.4), (0.5, 0.78)][i];
        let r = centered(
            c.rect.x + c.rect.width * fx,
            c.rect.y + c.rect.height * fy,
            size,
            size,
        );
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 0.8));
        note(c, &mut g, r, i == 0);
        root.add_group(g);
        let span = phase(c, i, 620.0, 900.0);
        let (_, y) = c.rel(r);
        t.looping(&part, AnimProp::Y, sway(y, y - c.unit * 0.12, span));
        t.looping(&part, AnimProp::Rotation, sway(-9.0, 9.0, span * 1.3));
    }
}

/// An eighth note, or a beamed pair, in `r`.
fn note(c: &Cx, g: &mut Grp, r: Rect, pair: bool) {
    let head_w = r.width * 0.34;
    let heads: &[f64] = if pair { &[0.0, 0.52] } else { &[0.0] };
    let stem_w = r.width * 0.075;
    for (j, off) in heads.iter().enumerate() {
        let head = centered(
            r.x + r.width * (0.26 + off),
            r.y + r.height * 0.8,
            head_w,
            head_w * 0.76,
        );
        g.add(c.filled(
            turned(ellipse(head, c.fill.clone()), -22.0).named(c.k, &format!("note_head_{j}")),
        ));
        let stem = rect_of(
            head.x + head.width - stem_w * 1.2,
            r.y + r.height * 0.12,
            stem_w,
            r.height * 0.7,
        );
        g.add(c.filled(
            rounded(stem, stem_w / 2.0, c.fill.clone()).named(c.k, &format!("note_stem_{j}")),
        ));
    }
    if pair {
        let x0 = r.x + r.width * 0.26 + head_w / 2.0 - stem_w * 1.2;
        let beam = rect_of(
            x0,
            r.y + r.height * 0.1,
            r.width * 0.52 + stem_w,
            r.height * 0.13,
        );
        g.add(c.filled(rounded(beam, beam.height * 0.4, c.fill.clone()).named(c.k, "note_beam")));
    } else {
        let flag = rect_of(
            r.x + r.width * 0.5,
            r.y + r.height * 0.1,
            r.width * 0.4,
            r.height * 0.4,
        );
        let (w, h) = (flag.width, flag.height);
        let d = format!(
            "M0,0 C{:.2},{:.2} {w:.2},{:.2} {:.2},{h:.2} C{:.2},{:.2} 0,{:.2} 0,{:.2} Z",
            w * 0.3,
            h * 0.35,
            h * 0.3,
            w * 0.85,
            w * 0.55,
            h * 0.4,
            h * 0.3,
            h * 0.28
        );
        g.add(c.shape(flag, d, "note_flag"));
    }
}

fn cloud_paint(c: &Cx) -> Paint {
    super::art::linear(
        90.0,
        &[(0.0, tint(c.fill_c, 0.25)), (1.0, shade(c.fill_c, 0.12))],
    )
}

fn rain_cloud(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let body = c.cell(0.0, 0.0, 1.0, 0.58);
    let mut cloud = c.piece("item_0", body, (0.5, 0.5));
    let lobes = [
        (0.02, 0.38, 0.42, 0.6),
        (0.24, 0.05, 0.42, 0.78),
        (0.52, 0.2, 0.36, 0.62),
        (0.7, 0.4, 0.3, 0.56),
    ];
    for (i, (fx, fy, fw, fh)) in lobes.into_iter().enumerate() {
        let r = rect_of(
            body.x + body.width * fx,
            body.y + body.height * fy,
            body.width * fw,
            body.height * fh,
        );
        cloud.add(c.filled(ellipse(r, cloud_paint(c)).named(c.k, &format!("cloud_edge_{i}"))));
    }
    let base = rect_of(
        body.x + body.width * 0.12,
        body.y + body.height * 0.5,
        body.width * 0.76,
        body.height * 0.5,
    );
    cloud.add(c.filled(rounded(base, base.height / 2.0, cloud_paint(c)).named(c.k, "cloud_base")));
    for (i, (fx, fy, fw, fh)) in lobes.into_iter().enumerate() {
        let r = rect_of(
            body.x + body.width * fx + 1.5,
            body.y + body.height * fy + 1.5,
            body.width * fw - 3.0,
            body.height * fh - 3.0,
        );
        cloud.add(ellipse(r, cloud_paint(c)).named(c.k, &format!("cloud_fill_{i}")));
    }
    let cover = rect_of(
        base.x + 1.5,
        base.y + 1.5,
        base.width - 3.0,
        base.height - 3.0,
    );
    cloud.add(rounded(cover, cover.height / 2.0, cloud_paint(c)).named(c.k, "cloud_cover"));
    root.add_group(cloud);
    let (_, cy) = c.rel(body);
    t.looping("item_0", AnimProp::Y, sway(cy, cy + c.unit * 0.04, 900.0));
    for i in 0..5 {
        rain(c, root, t, i, body);
    }
}

fn rain(c: &Cx, root: &mut Grp, t: &mut Tracks, i: usize, body: Rect) {
    let w = c.rect.width * 0.07;
    let x = c.rect.x + c.rect.width * (0.2 + 0.15 * i as f64);
    let y0 = body.y + body.height * 0.95;
    let r = rect_of(x, y0, w, w * 2.1);
    let part = format!("item_{}", i + 1);
    let mut g = c.piece(&part, r, (0.5, 0.0));
    let paint = Paint::Color(parse(&mix(solid_rgba("#66b8ff"), c.fill_c, 0.12)));
    g.add(path(r, drop_d(r.width, r.height), Some(paint)).named(c.k, &format!("rain_{i}")));
    root.add_group(g);
    let period = ms(520.0 + 60.0 * i as f64);
    let delay = ms(phase(c, i, 0.0, 400.0));
    let (_, y) = c.rel(r);
    let fall = (c.rect.y + c.rect.height - (r.y + r.height)).max(0.0);
    let drop = loop_after(
        delay,
        period + delay,
        &[(0, y, Ease::Linear), (period - 40, y + fall, Ease::EaseIn)],
    );
    let fade = loop_after(
        delay,
        period + delay,
        &[
            (0, 0.0, Ease::Linear),
            (period / 6, 1.0, Ease::EaseOut),
            (period * 3 / 4, 1.0, Ease::Linear),
            (period - 40, 0.0, Ease::EaseIn),
        ],
    );
    t.set(&part, AnimProp::Y, drop, Repeat::Loop);
    t.set(&part, AnimProp::Opacity, fade, Repeat::Loop);
}

/// A spiral of `turns` turns that fits a circle of radius `radius`, as a line path.
fn spiral_d(radius: f64, turns: f64) -> String {
    let steps = 36;
    let mut d = String::new();
    for i in 0..=steps {
        let u = f64::from(i) / f64::from(steps);
        let (a, r) = (u * turns * std::f64::consts::TAU, u * radius);
        let (x, y) = (radius + a.cos() * r, radius + a.sin() * r);
        d.push_str(&format!("{}{x:.2},{y:.2} ", if i == 0 { "M" } else { "L" }));
    }
    d
}

fn dizzy(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let n = c.count.clamp(1, 3);
    let size = c.rect.height * 0.7;
    for i in 0..n {
        let cx = if n == 1 {
            c.rect.x + c.rect.width / 2.0
        } else {
            c.rect.x + size / 2.0 + (c.rect.width - size) * i as f64 / (n - 1) as f64
        };
        let r = centered(cx, c.rect.y + c.rect.height * 0.5, size, size);
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 0.5));
        let width = size * 0.1;
        for node in c.stroked(r, &spiral_d(size / 2.0, 2.4), width, &format!("spiral_{i}")) {
            g.add(node);
        }
        root.add_group(g);
        let (turn, span) = if i % 2 == 0 {
            (360.0, 1100.0)
        } else {
            (-360.0, 1500.0)
        };
        let keys: Vec<Key> = vec![(0, 0.0, Ease::Linear), (ms(span), turn, Ease::Linear)];
        t.set(&part, AnimProp::Rotation, keys, Repeat::Loop);
    }
}

/// A five-pointed star filling `w` x `h`.
fn star5_d(w: f64, h: f64) -> String {
    let (cx, cy) = (w / 2.0, h * 0.55);
    let mut d = String::new();
    for i in 0..10 {
        let a = -std::f64::consts::FRAC_PI_2 + f64::from(i) * std::f64::consts::PI / 5.0;
        let r = if i % 2 == 0 { 0.5 } else { 0.22 };
        let (x, y) = (cx + a.cos() * r * w * 1.0, cy + a.sin() * r * h * 1.0);
        d.push_str(&format!("{}{x:.2},{y:.2} ", if i == 0 { "M" } else { "L" }));
    }
    d.push('Z');
    d
}

fn stars(c: &Cx, root: &mut Grp, t: &mut Tracks) {
    let n = c.count.clamp(1, 6);
    let size = c.rect.height * 0.5;
    let (rx, ry) = (c.rect.width * 0.38, c.rect.height * 0.16);
    let centre = (
        c.rect.x + c.rect.width / 2.0,
        c.rect.y + c.rect.height * 0.5,
    );
    let period = 2200.0;
    for i in 0..n {
        let start = std::f64::consts::TAU * i as f64 / n as f64;
        let r = centered(
            centre.0 + start.cos() * rx,
            centre.1 + start.sin() * ry,
            size,
            size,
        );
        let part = format!("item_{i}");
        let mut g = c.piece(&part, r, (0.5, 0.5));
        g.add(c.shape(r, star5_d(r.width, r.height), &format!("star_{i}")));
        root.add_group(g);
        let (x0, y0) = c.rel(r);
        let (mut xs, mut ys): (Vec<Key>, Vec<Key>) = (Vec::new(), Vec::new());
        for step in 0..=12u32 {
            let a = start + std::f64::consts::TAU * f64::from(step) / 12.0;
            let at = ms(period * f64::from(step) / 12.0);
            xs.push((at, x0 + a.cos() * rx - start.cos() * rx, Ease::Linear));
            ys.push((at, y0 + a.sin() * ry - start.sin() * ry, Ease::Linear));
        }
        t.set(&part, AnimProp::X, xs, Repeat::Loop);
        t.set(&part, AnimProp::Y, ys, Repeat::Loop);
        t.looping(
            &part,
            AnimProp::Rotation,
            sway(-14.0, 14.0, 500.0 + 90.0 * i as f64),
        );
    }
}
