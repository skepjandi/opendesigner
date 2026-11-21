//! Flat-vector props with life of their own: a rocket with a flickering flame, a glowing
//! lightbulb, a spinning gear, a swaying tree, a house with smoke and lit windows and a laptop
//! whose screen is busy. The science props (atom, planet, cell, DNA) are in `props_science`.
//! Every prop draws in a square (or its box) from the palette, sized as a share of that box.

use od_core::animation::{AnimProp, Repeat};
use od_core::geometry::Rect;
use od_core::{Ease, FillRule, NewNode, Paint, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::art::{
    BLACK, Grp, Key, Named, Rng, WHITE, blob_d, blurred, centered, cycles, ellipse, faded, glow,
    linear, lit, ms, path, pen, rect_of, rounded, shade, shadow, solid, sway, tint, turned,
};
use super::kit::{Kit, Role, Target, at, mix};
use super::props_science;
use super::registry::{Block, Category, Layer, Place};

pub(super) const COLOR: Param = Param::text(
    "color",
    "The main color: #rrggbb or {token}; empty takes the accent role (or the natural color of a tree, house or planet).",
    "",
    40,
);
pub(super) const ACCENT: Param = Param::text(
    "accent_color",
    "The second color: #rrggbb or {token}; empty picks one that goes with the first.",
    "",
    40,
);
pub(super) const LOOPS: Param = Param::count(
    "loops",
    "How many cycles the motion plays: 0 repeats for as long as the video plays.",
    0,
    0,
    40,
);
pub(super) const SEED: Param = Param::count(
    "seed",
    "Desynchronises the motion of several props of one kind.",
    1,
    0,
    9999,
);

fn prop_box(frame: Rect, _margin: f64) -> Rect {
    let side = frame.width.min(frame.height) * 0.5;
    at(
        frame.x + (frame.width - side) / 2.0,
        frame.y + (frame.height - side) / 2.0,
        side,
        side,
    )
}

/// The palette, the repetition count and the seed of one prop, and the square it draws in.
pub(super) struct Ctx {
    pub s: Rect,
    pub main: Paint,
    pub main_c: Rgba,
    pub accent: Paint,
    pub accent_c: Rgba,
    pub loops: u32,
    pub rng: Rng,
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

impl Ctx {
    /// `main` falls back to `natural` (a hex) when given, else to the accent role; `accent` to `pair`.
    pub(super) fn read(
        k: &mut Kit,
        a: &Args,
        natural: Option<&str>,
        pair: &str,
    ) -> Result<Ctx, BlockError> {
        let main = match (colour(k, a, "color")?, natural) {
            (Some(p), _) => p,
            (None, Some(hex)) => solid(hex),
            (None, None) => k.paint(Role::Accent),
        };
        let main_c = k.rgba(&main);
        let accent = colour(k, a, "accent_color")?.unwrap_or_else(|| solid(pair));
        let accent_c = k.rgba(&accent);
        let side = k.b.width.min(k.b.height);
        let s = centered(
            k.b.x + k.b.width / 2.0,
            k.b.y + k.b.height / 2.0,
            side,
            side,
        );
        Ok(Ctx {
            s,
            main,
            main_c,
            accent,
            accent_c,
            loops: a.num("loops") as u32,
            rng: Rng::new(a.num("seed") as u64, 31),
        })
    }

    pub(super) fn w(&self) -> f64 {
        self.s.width
    }

    /// A looping track (written out when `loops` is set).
    pub(super) fn track(
        &self,
        k: &mut Kit,
        part: &str,
        prop: AnimProp,
        keys: &[Key],
        mode: Repeat,
    ) -> Result<(), BlockError> {
        let (keys, repeat) = cycles(keys, mode, self.loops)?;
        k.track(Target::Part(k.full(part)), prop, &keys, repeat);
        Ok(())
    }

    pub(super) fn sway(
        &self,
        k: &mut Kit,
        part: &str,
        prop: AnimProp,
        range: (f64, f64),
        span: f64,
    ) -> Result<(), BlockError> {
        let (keys, mode) = sway(range.0, range.1, span);
        self.track(k, part, prop, &keys, mode)
    }
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "prop_rocket",
        category: Category::Characters,
        about: "A flat rocket (nose, window, fins) with a flickering flame, exhaust puffs and a gentle hover.",
        tags: &["rocket", "space", "launch", "startup", "prop", "ship"],
        params: &[COLOR, ACCENT, LOOPS, SEED],
        place: Place::Custom(prop_box),
        layer: Layer::Front,
        build: rocket,
    },
    Block {
        id: "prop_lightbulb",
        category: Category::Characters,
        about: "A glowing lightbulb with rays; the glow and the rays pulse.",
        tags: &["lightbulb", "idea", "light", "bulb", "prop", "glow"],
        params: &[COLOR, ACCENT, LOOPS, SEED],
        place: Place::Custom(prop_box),
        layer: Layer::Front,
        build: lightbulb,
    },
    Block {
        id: "prop_gear",
        category: Category::Characters,
        about: "A gear whose teeth are a path (`teeth` of them) and that spins continuously.",
        tags: &[
            "gear",
            "cog",
            "settings",
            "machine",
            "engineering",
            "prop",
            "spin",
        ],
        params: &[
            COLOR,
            ACCENT,
            GEAR_TEETH,
            GEAR_PERIOD,
            GEAR_CLOCKWISE,
            LOOPS,
            SEED,
        ],
        place: Place::Custom(prop_box),
        layer: Layer::Front,
        build: gear,
    },
    Block {
        id: "prop_tree",
        category: Category::Characters,
        about: "A flat tree (round or pine) whose crown and leaves sway in the wind, with a soft ground shadow.",
        tags: &["tree", "nature", "forest", "plant", "prop", "sway", "pine"],
        params: &[COLOR, ACCENT, TREE_KIND, LOOPS, SEED],
        place: Place::Custom(prop_box),
        layer: Layer::Front,
        build: tree,
    },
    Block {
        id: "prop_house",
        category: Category::Characters,
        about: "A cottage with a roof, door, lit windows that glow and a chimney with rising smoke.",
        tags: &["house", "home", "building", "village", "prop", "smoke"],
        params: &[COLOR, ACCENT, LOOPS, SEED],
        place: Place::Custom(prop_box),
        layer: Layer::Front,
        build: house,
    },
    Block {
        id: "prop_laptop_2d",
        category: Category::Characters,
        about: "A flat illustration of a laptop (not the 3D device) whose screen shows animated lines and a blinking cursor.",
        tags: &["laptop", "computer", "work", "code", "prop", "screen", "2d"],
        params: &[COLOR, ACCENT, LOOPS, SEED],
        place: Place::Custom(prop_box),
        layer: Layer::Front,
        build: laptop,
    },
    props_science::ATOM,
    props_science::PLANET,
    props_science::CELL,
    props_science::DNA,
];

const GEAR_TEETH: Param = Param::count("teeth", "How many teeth.", 10, 5, 24);
const GEAR_PERIOD: Param = Param::count(
    "period_ms",
    "How long one turn takes.",
    12_000,
    1_000,
    120_000,
);
const GEAR_CLOCKWISE: Param =
    Param::flag("clockwise", "Turn clockwise (off: counterclockwise).", true);
const TREE_KIND: Param = Param::choice(
    "kind",
    "A round crown or a pine.",
    "round",
    &["round", "pine"],
);

/// Invisible until its animation shows it.
pub(super) fn hidden(mut n: NewNode) -> NewNode {
    n.opacity = Some(0.0);
    n
}

/// A soft contact shadow on the ground under a prop.
pub(super) fn ground(k: &Kit, cx: f64, y: f64, w: f64) -> NewNode {
    let h = w * 0.14;
    blurred(ellipse(centered(cx, y, w, h), faded(BLACK, 0.24)), h * 0.45).named(k, "ground_shadow")
}

fn rocket(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, Some("#f3f6fb"), "#ff5d6c")?;
    let (w, cx) = (c.w(), c.s.x + c.w() / 2.0);
    let (bw, bh) = (w * 0.34, w * 0.56);
    let top = c.s.y + w * 0.05;
    let body = rect_of(cx - bw / 2.0, top, bw, bh);
    let mut g = Grp::new(k, "rocket", rect_of(c.s.x, c.s.y, w, w)).pivot(cx, top + bh * 0.5);
    g = g.shadowed(shadow(0.0, w * 0.012, w * 0.025, 0.2));
    let fin_w = bw * 0.5;
    let fin_h = bh * 0.46;
    let fin_y = top + bh * 0.58;
    let fin_l = rect_of(body.x - fin_w * 0.85, fin_y, fin_w, fin_h);
    let fin_r = rect_of(body.x + bw - fin_w * 0.15, fin_y, fin_w, fin_h);
    let (fw, fh) = (fin_w, fin_h);
    let left = format!(
        "M{fw:.2},0 C{:.2},{:.2} 0,{:.2} 0,{fh:.2} L{fw:.2},{:.2} Z",
        fw * 0.45,
        fh * 0.1,
        fh * 0.45,
        fh * 0.8
    );
    let right = format!(
        "M0,0 C{:.2},{:.2} {fw:.2},{:.2} {fw:.2},{fh:.2} L0,{:.2} Z",
        fw * 0.55,
        fh * 0.1,
        fh * 0.45,
        fh * 0.8
    );
    let fin_paint = lit(&c.accent, c.accent_c);
    g.add(path(fin_l, left, Some(fin_paint.clone())).named(k, "fin_l"));
    g.add(path(fin_r, right, Some(fin_paint)).named(k, "fin_r"));
    g.add(
        rounded(
            rect_of(cx - bw * 0.07, top + bh * 0.7, bw * 0.14, bh * 0.38),
            bw * 0.07,
            shade(c.accent_c, 0.25),
        )
        .named(k, "fin_mid"),
    );
    let shape = |fill: Paint| path(body, bullet_d(bw, bh), Some(fill));
    g.add(shape(lit(&c.main, c.main_c)).named(k, "rocket_body"));
    let mut nose = Grp::new(k, "rocket_nose", rect_of(body.x, top, bw, bh * 0.3));
    nose.node.clip = Some(true);
    nose.add(shape(lit(&c.accent, c.accent_c)).named(k, "rocket_nose_shape"));
    g.add_group(nose);
    let band = rect_of(body.x, top + bh * 0.8, bw, bh * 0.07);
    g.add(rounded(band, bh * 0.02, faded(c.accent_c, 0.9)).named(k, "rocket_band"));
    window(k, &mut g, cx, top + bh * 0.42, w * 0.085, &c);
    let nozzle_y = top + bh - bh * 0.01;
    g.add(
        rounded(
            rect_of(cx - bw * 0.27, nozzle_y, bw * 0.54, w * 0.045),
            w * 0.015,
            solid("#3a4260"),
        )
        .named(k, "nozzle"),
    );
    let flame_h = w * 0.25;
    let fbox = rect_of(cx - w * 0.1, nozzle_y + w * 0.035, w * 0.2, flame_h);
    let mut flame = Grp::new(k, "flame", fbox).pivot(cx, fbox.y);
    let hot = linear(90.0, &[(0.0, solid("#ffd54a")), (1.0, solid("#ff6a3d"))]);
    flame.add(path(fbox, flame_d(fbox.width, fbox.height), Some(hot)).named(k, "flame_outer"));
    let inner = rect_of(cx - w * 0.05, fbox.y, w * 0.1, flame_h * 0.62);
    flame.add(
        path(
            inner,
            flame_d(inner.width, inner.height),
            Some(solid("#fff4bf")),
        )
        .named(k, "flame_inner"),
    );
    g.add_group(flame);
    let puffs = puff_group(k, &c, cx, fbox.y + flame_h * 0.7);
    k.add("exhaust", puffs.node);
    let rest_y = g.node.y;
    k.add("rocket", g.node);
    rocket_motion(k, &mut c, rest_y)
}

/// A pointed capsule filling `w` x `h`.
fn bullet_d(w: f64, h: f64) -> String {
    format!(
        "M{:.2},0 C{:.2},{:.2} {w:.2},{:.2} {:.2},{h:.2} L{:.2},{h:.2} C0,{:.2} {:.2},{:.2} {:.2},0 Z",
        w / 2.0,
        w * 0.92,
        h * 0.12,
        h * 0.45,
        w * 0.88,
        w * 0.12,
        h * 0.45,
        w * 0.08,
        h * 0.12,
        w / 2.0
    )
}

fn flame_d(w: f64, h: f64) -> String {
    format!(
        "M{:.2},0 L{:.2},0 C{w:.2},{:.2} {:.2},{:.2} {:.2},{h:.2} C{:.2},{:.2} 0,{:.2} {:.2},0 Z",
        w * 0.12,
        w * 0.88,
        h * 0.45,
        w * 0.7,
        h * 0.8,
        w / 2.0,
        w * 0.3,
        h * 0.8,
        h * 0.45,
        w * 0.12
    )
}

fn window(k: &Kit, g: &mut Grp, cx: f64, cy: f64, r: f64, c: &Ctx) {
    g.add(ellipse(centered(cx, cy, r * 2.0, r * 2.0), shade(c.main_c, 0.5)).named(k, "window_rim"));
    let glass = super::art::radial(
        0.35,
        0.3,
        0.8,
        &[(0.0, solid("#d9f3ff")), (1.0, solid("#4aa8e8"))],
    );
    g.add(ellipse(centered(cx, cy, r * 1.6, r * 1.6), glass).named(k, "window_glass"));
    g.add(
        turned(
            ellipse(
                centered(cx - r * 0.3, cy - r * 0.35, r * 0.6, r * 0.34),
                faded(WHITE, 0.7),
            ),
            -30.0,
        )
        .named(k, "window_glint"),
    );
}

fn puff_group(k: &Kit, c: &Ctx, cx: f64, y: f64) -> Grp {
    let w = c.w();
    let mut g = Grp::new(k, "exhaust", rect_of(cx - w * 0.12, y, w * 0.24, w * 0.2));
    for i in 0..3 {
        let r = w * (0.035 + 0.01 * f64::from(i));
        let x = cx + w * 0.05 * (f64::from(i) - 1.0);
        g.add(
            hidden(ellipse(centered(x, y, r * 2.0, r * 2.0), faded(WHITE, 0.7)))
                .named(k, &format!("puff_{i}")),
        );
    }
    g
}

fn rocket_motion(k: &mut Kit, c: &mut Ctx, rest_y: f64) -> Result<(), BlockError> {
    let w = c.w();
    let jitter = c.rng.range(0.85, 1.2);
    let t = |v: f64| ms(v * jitter);
    let flick = |vals: [f64; 4], times: [f64; 4]| -> Vec<Key> {
        let mut keys = vec![(0, 1.0, Ease::EaseInOut)];
        keys.extend(
            times
                .iter()
                .zip(vals)
                .map(|(tm, v)| (t(*tm), v, Ease::EaseInOut)),
        );
        keys.push((t(430.0), 1.0, Ease::EaseInOut));
        keys
    };
    c.track(
        k,
        "flame",
        AnimProp::ScaleY,
        &flick([1.2, 0.9, 1.14, 0.96], [80.0, 190.0, 290.0, 360.0]),
        Repeat::Loop,
    )?;
    c.track(
        k,
        "flame",
        AnimProp::ScaleX,
        &flick([0.9, 1.07, 0.94, 1.05], [110.0, 200.0, 300.0, 380.0]),
        Repeat::Loop,
    )?;
    c.track(
        k,
        "flame_inner",
        AnimProp::ScaleY,
        &flick([0.8, 1.18, 0.9, 1.1], [60.0, 160.0, 260.0, 350.0]),
        Repeat::Loop,
    )?;
    c.sway(
        k,
        "rocket",
        AnimProp::Y,
        (rest_y, rest_y - w * 0.014),
        900.0 * jitter,
    )?;
    c.sway(
        k,
        "rocket",
        AnimProp::Rotation,
        (-1.5, 1.5),
        1700.0 * jitter,
    )?;
    for i in 0..3u64 {
        let delay = 520 * i;
        let rest = puff_rest(c, i);
        let part = format!("puff_{i}");
        let end = 1560;
        let fade: Vec<Key> = vec![
            (0, 0.0, Ease::Linear),
            (delay.max(40), 0.0, Ease::Linear),
            (delay + 120, 0.55, Ease::EaseOut),
            (end, 0.0, Ease::EaseIn),
        ];
        let drop: Vec<Key> = vec![
            (0, rest, Ease::Linear),
            (delay.max(40), rest, Ease::Linear),
            (end, rest + w * 0.2, Ease::EaseOut),
        ];
        let grow: Vec<Key> = vec![
            (0, 0.6, Ease::Linear),
            (delay.max(40), 0.6, Ease::Linear),
            (end, 1.7, Ease::EaseOut),
        ];
        c.track(k, &part, AnimProp::Opacity, &fade, Repeat::Loop)?;
        c.track(k, &part, AnimProp::Y, &drop, Repeat::Loop)?;
        c.track(k, &part, AnimProp::Scale, &grow, Repeat::Loop)?;
    }
    Ok(())
}

/// A puff's resting `y`, relative to the exhaust group (whose box starts at the flame).
fn puff_rest(c: &Ctx, i: u64) -> f64 {
    let r = c.w() * (0.035 + 0.01 * i as f64);
    -r
}

fn lightbulb(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, Some("#ffd84a"), "#ff9f2e")?;
    let (w, cx) = (c.w(), c.s.x + c.w() / 2.0);
    let (bw, bh) = (w * 0.5, w * 0.58);
    let top = c.s.y + w * 0.1;
    let cy = top + bh * 0.5;
    k.add(
        "glow",
        ellipse(centered(cx, cy, w * 0.95, w * 0.95), glow(c.main_c, 0.6)).named(k, "glow"),
    );
    let mut rays = Grp::new(k, "rays", rect_of(c.s.x, c.s.y, w, w));
    for i in 0..7 {
        let deg = -195.0 + 35.0 * f64::from(i);
        let (sin, cos) = deg.to_radians().sin_cos();
        let r = w * 0.38;
        let len = w * 0.1;
        let ray = rounded(
            centered(cx + cos * r, cy + sin * r, w * 0.024, len),
            w * 0.012,
            tint(c.main_c, 0.3),
        );
        rays.add(turned(ray, deg - 90.0).named(k, &format!("ray_{i}")));
    }
    k.add("rays", rays.node);
    let body = rect_of(cx - bw / 2.0, top, bw, bh);
    let mut bulb = Grp::new(k, "bulb", body);
    let glass = super::art::radial(
        0.4,
        0.35,
        0.75,
        &[
            (0.0, tint(c.main_c, 0.75)),
            (0.55, Paint::Color(c.main_c)),
            (1.0, shade(c.main_c, 0.12)),
        ],
    );
    bulb.add(path(body, blob_d(bw, bh, 0.42), Some(glass)).named(k, "glass"));
    bulb.add(
        turned(
            ellipse(
                rect_of(body.x + bw * 0.16, body.y + bh * 0.12, bw * 0.2, bh * 0.28),
                faded(WHITE, 0.6),
            ),
            -25.0,
        )
        .named(k, "glass_shine"),
    );
    let fil = rect_of(cx - bw * 0.2, cy - bh * 0.02, bw * 0.4, bh * 0.3);
    let (fw, fh) = (fil.width, fil.height);
    let d = format!(
        "M0,{fh:.2} L{:.2},{:.2} L{:.2},{fh:.2} L{:.2},{:.2} L{fw:.2},{fh:.2}",
        fw * 0.2,
        fh * 0.1,
        fw * 0.4,
        fw * 0.6,
        fh * 0.1
    );
    bulb.add(super::art::line(fil, d, solid("#c8571f"), w * 0.014).named(k, "filament"));
    k.add("bulb", bulb.node);
    screw_base(k, &c, cx, top + bh - w * 0.02);
    c.sway(k, "glow", AnimProp::Scale, (0.9, 1.14), 800.0)?;
    c.sway(k, "glow", AnimProp::Opacity, (0.65, 1.0), 800.0)?;
    c.sway(k, "rays", AnimProp::Opacity, (0.3, 1.0), 800.0)?;
    let flicker = 400.0 + c.rng.range(0.0, 200.0);
    c.sway(k, "filament", AnimProp::Opacity, (0.7, 1.0), flicker)
}

fn screw_base(k: &mut Kit, c: &Ctx, cx: f64, y: f64) {
    let w = c.w();
    let mut base = Grp::new(k, "base", rect_of(cx - w * 0.12, y, w * 0.24, w * 0.22));
    let metal = Rgba {
        r: 150,
        g: 160,
        b: 178,
        a: 255,
    };
    for i in 0..3 {
        let inset = w * 0.012 * f64::from(i);
        let r = rect_of(
            cx - w * 0.115 + inset,
            y + w * 0.055 * f64::from(i),
            w * 0.23 - 2.0 * inset,
            w * 0.06,
        );
        let tone = if i % 2 == 0 {
            tint(metal, 0.1)
        } else {
            shade(metal, 0.18)
        };
        base.add(rounded(r, w * 0.03, tone).named(k, &format!("base_ring_{i}")));
    }
    base.add(
        rounded(
            rect_of(cx - w * 0.05, y + w * 0.17, w * 0.1, w * 0.04),
            w * 0.02,
            solid("#3a4260"),
        )
        .named(k, "base_tip"),
    );
    k.add("base", base.node);
}

fn gear_d(r_out: f64, r_in: f64, hole: f64, teeth: u32) -> String {
    let c = r_out;
    let step = std::f64::consts::TAU / f64::from(teeth);
    let pt = |r: f64, a: f64| (c + r * a.cos(), c + r * a.sin());
    let mut d = String::new();
    for i in 0..teeth {
        let a = f64::from(i) * step - std::f64::consts::FRAC_PI_2;
        let pts = [
            pt(r_in, a - step * 0.3),
            pt(r_out, a - step * 0.17),
            pt(r_out, a + step * 0.17),
            pt(r_in, a + step * 0.3),
        ];
        let cmd = if i == 0 { "M" } else { "A" };
        if i == 0 {
            d += &format!("{cmd}{:.2},{:.2} ", pts[0].0, pts[0].1);
        } else {
            d += &format!(
                "A{r_in:.2},{r_in:.2} 0 0 1 {:.2},{:.2} ",
                pts[0].0, pts[0].1
            );
        }
        for p in &pts[1..] {
            d += &format!("L{:.2},{:.2} ", p.0, p.1);
        }
    }
    let first = pt(r_in, -std::f64::consts::FRAC_PI_2 - step * 0.3);
    d += &format!(
        "A{r_in:.2},{r_in:.2} 0 0 1 {:.2},{:.2} Z ",
        first.0, first.1
    );
    d += &format!(
        "M{:.2},{c:.2} A{hole:.2},{hole:.2} 0 1 0 {:.2},{c:.2} A{hole:.2},{hole:.2} 0 1 0 {:.2},{c:.2} Z",
        c - hole,
        c + hole,
        c - hole
    );
    d
}

fn gear(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let c = Ctx::read(k, a, None, "#ffbe3d")?;
    let teeth = a.num("teeth") as u32;
    let period = a.num("period_ms");
    let r = c.w() * 0.46;
    let cx = c.s.x + c.w() / 2.0;
    let box_ = centered(cx, c.s.y + c.w() / 2.0, r * 2.0, r * 2.0);
    let mut g = Grp::new(k, "gear", box_).pivot(box_.x + r, box_.y + r);
    g = g.shadowed(shadow(0.0, r * 0.05, r * 0.1, 0.22));
    let mut body = path(
        box_,
        gear_d(r, r * 0.8, r * 0.42, teeth),
        Some(lit(&c.main, c.main_c)),
    );
    body.fill_rule = Some(FillRule::Evenodd);
    g.add(body.named(k, "gear_body"));
    g.add(
        ellipse(
            centered(box_.x + r, box_.y + r, r * 0.62, r * 0.62),
            lit(&c.accent, c.accent_c),
        )
        .named(k, "gear_hub"),
    );
    g.add(
        ellipse(
            centered(box_.x + r, box_.y + r, r * 0.2, r * 0.2),
            shade(c.accent_c, 0.45),
        )
        .named(k, "gear_bolt"),
    );
    g.add(
        turned(
            ellipse(
                rect_of(box_.x + r * 0.3, box_.y + r * 0.28, r * 0.5, r * 0.26),
                glow(WHITE, 0.4),
            ),
            -35.0,
        )
        .named(k, "gear_shine"),
    );
    k.add("gear", g.node);
    let end = if a.flag("clockwise") { 360.0 } else { -360.0 };
    c.track(
        k,
        "gear",
        AnimProp::Rotation,
        &[(0, 0.0, Ease::Linear), (ms(period), end, Ease::Linear)],
        Repeat::Loop,
    )
}

fn tree(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, Some("#43b86a"), "#8a5a3c")?;
    let (w, cx) = (c.w(), c.s.x + c.w() / 2.0);
    let base = c.s.y + w * 0.96;
    k.add("ground", ground(k, cx, base, w * 0.5));
    let trunk = rect_of(cx - w * 0.065, c.s.y + w * 0.5, w * 0.13, w * 0.46);
    let d = format!(
        "M{:.2},0 L{:.2},0 L{:.2},{:.2} L0,{:.2} Z",
        trunk.width * 0.2,
        trunk.width * 0.8,
        trunk.width,
        trunk.height,
        trunk.height
    );
    k.add("trunk", path(trunk, d, Some(lit(&c.accent, c.accent_c))));
    let crown_box = rect_of(cx - w * 0.36, c.s.y + w * 0.04, w * 0.72, w * 0.58);
    let mut crown = Grp::new(k, "crown", crown_box).pivot(cx, trunk.y);
    if a.text("kind") == "pine" {
        pine_tiers(k, &c, &mut crown, cx);
    } else {
        round_leaves(k, &c, &mut crown, cx);
    }
    k.add("crown", crown.node);
    let span = c.rng.range(1500.0, 2100.0);
    c.sway(k, "crown", AnimProp::Rotation, (-2.4, 2.4), span)?;
    for i in 0..3 {
        let part = format!("leaves_{i}");
        let amp = 2.0 + f64::from(i);
        let s = c.rng.range(1100.0, 1900.0);
        c.sway(k, &part, AnimProp::Rotation, (-amp, amp), s)?;
    }
    Ok(())
}

fn round_leaves(k: &Kit, c: &Ctx, crown: &mut Grp, cx: f64) {
    let w = c.w();
    let y = c.s.y;
    let blobs = [
        (
            "leaves_0",
            centered(cx - w * 0.17, y + w * 0.34, w * 0.4, w * 0.34),
            0.18,
        ),
        (
            "leaves_1",
            centered(cx + w * 0.17, y + w * 0.34, w * 0.4, w * 0.34),
            0.1,
        ),
        (
            "leaves_2",
            centered(cx, y + w * 0.22, w * 0.5, w * 0.42),
            0.0,
        ),
    ];
    for (name, r, dark) in blobs {
        let mut leaf = Grp::new(k, name, r);
        let paint = lit(
            &Paint::Color(shade_c(c.main_c, dark)),
            shade_c(c.main_c, dark),
        );
        leaf.add(ellipse(r, paint).named(k, &format!("{name}_shape")));
        leaf.add(
            turned(
                ellipse(
                    rect_of(
                        r.x + r.width * 0.16,
                        r.y + r.height * 0.1,
                        r.width * 0.3,
                        r.height * 0.22,
                    ),
                    glow(WHITE, 0.4),
                ),
                -25.0,
            )
            .named(k, &format!("{name}_shine")),
        );
        crown.add_group(leaf);
    }
}

fn shade_c(c: Rgba, t: f64) -> Rgba {
    Rgba::parse_hex(&mix(c, BLACK, t)).unwrap_or(c)
}

fn pine_tiers(k: &Kit, c: &Ctx, crown: &mut Grp, cx: f64) {
    let w = c.w();
    for (i, (width, top, height)) in [(0.62, 0.3, 0.3), (0.5, 0.18, 0.28), (0.36, 0.06, 0.26)]
        .into_iter()
        .enumerate()
    {
        let r = rect_of(cx - w * width / 2.0, c.s.y + w * top, w * width, w * height);
        let tone = shade_c(c.main_c, 0.16 - 0.05 * i as f64);
        let mut tier = Grp::new(k, &format!("leaves_{i}"), r);
        let d = format!(
            "M{:.2},0 L{:.2},{:.2} L0,{:.2} Z",
            r.width / 2.0,
            r.width,
            r.height,
            r.height
        );
        let mut shape = path(r, d, Some(lit(&Paint::Color(tone), tone)));
        shape.stroke = Some(pen(Paint::Color(tone), w * 0.03));
        tier.add(shape.named(k, &format!("leaves_{i}_shape")));
        crown.add_group(tier);
    }
}

fn house(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, Some("#f2d7b6"), "#d9534f")?;
    let (w, cx) = (c.w(), c.s.x + c.w() / 2.0);
    let base = c.s.y + w * 0.94;
    k.add("ground", ground(k, cx, base, w * 0.8));
    let wall = rect_of(
        cx - w * 0.3,
        c.s.y + w * 0.46,
        w * 0.6,
        base - c.s.y - w * 0.46,
    );
    let chimney = rect_of(cx + w * 0.12, c.s.y + w * 0.12, w * 0.09, w * 0.26);
    k.add(
        "chimney",
        rounded(
            chimney,
            w * 0.01,
            lit(
                &Paint::Color(shade_c(c.accent_c, 0.3)),
                shade_c(c.accent_c, 0.3),
            ),
        ),
    );
    k.add("wall", rounded(wall, w * 0.02, lit(&c.main, c.main_c)));
    let roof = rect_of(cx - w * 0.4, c.s.y + w * 0.16, w * 0.8, w * 0.34);
    let rd = format!(
        "M{:.2},0 L{:.2},{:.2} L0,{:.2} Z",
        roof.width / 2.0,
        roof.width,
        roof.height,
        roof.height
    );
    let mut roof_shape = path(roof, rd, Some(lit(&c.accent, c.accent_c)));
    roof_shape.stroke = Some(pen(Paint::Color(shade_c(c.accent_c, 0.12)), w * 0.03));
    k.add("roof", roof_shape);
    let door = rect_of(cx - w * 0.07, base - w * 0.25, w * 0.14, w * 0.25);
    k.add(
        "door",
        rounded(
            door,
            w * 0.03,
            lit(
                &Paint::Color(shade_c(c.accent_c, 0.35)),
                shade_c(c.accent_c, 0.35),
            ),
        ),
    );
    k.add(
        "door_knob",
        ellipse(
            centered(
                door.x + door.width * 0.78,
                door.y + door.height * 0.55,
                w * 0.02,
                w * 0.02,
            ),
            solid("#ffd54a"),
        ),
    );
    for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
        window_light(k, &c, cx + sign * w * 0.19, c.s.y + w * 0.62, i);
    }
    let smoke = smoke_group(k, &c, chimney);
    k.add("smoke", smoke.node);
    for i in 0..2 {
        let s = c.rng.range(900.0, 1500.0);
        c.sway(
            k,
            &format!("window_glow_{i}"),
            AnimProp::Opacity,
            (0.45, 1.0),
            s,
        )?;
    }
    smoke_motion(k, &c, chimney)
}

fn window_light(k: &mut Kit, c: &Ctx, cx: f64, cy: f64, i: usize) {
    let w = c.w();
    k.add(
        &format!("window_glow_{i}"),
        ellipse(
            centered(cx, cy, w * 0.3, w * 0.3),
            glow(solid_rgba("#ffd36a"), 0.7),
        ),
    );
    let r = centered(cx, cy, w * 0.14, w * 0.14);
    let pane = linear(90.0, &[(0.0, solid("#fff0a8")), (1.0, solid("#ffc24a"))]);
    k.add(&format!("window_{i}"), rounded(r, w * 0.015, pane));
    let bar = shade_c(c.accent_c, 0.3);
    k.add(
        &format!("window_bar_v_{i}"),
        rect_of(cx - w * 0.004, r.y, w * 0.008, r.height).with_fill(Paint::Color(bar)),
    );
    k.add(
        &format!("window_bar_h_{i}"),
        rect_of(r.x, cy - w * 0.004, r.width, w * 0.008).with_fill(Paint::Color(bar)),
    );
}

trait WithFill {
    fn with_fill(self, p: Paint) -> NewNode;
}

impl WithFill for Rect {
    fn with_fill(self, p: Paint) -> NewNode {
        rounded(self, 0.0, p)
    }
}

fn solid_rgba(hex: &str) -> Rgba {
    Rgba::parse_hex(hex).unwrap_or(WHITE)
}

fn smoke_group(k: &Kit, c: &Ctx, chimney: Rect) -> Grp {
    let w = c.w();
    let mut g = Grp::new(
        k,
        "smoke",
        rect_of(
            chimney.x - w * 0.05,
            chimney.y - w * 0.12,
            w * 0.2,
            w * 0.14,
        ),
    );
    for i in 0..3 {
        let r = w * (0.03 + 0.008 * f64::from(i));
        g.add(
            hidden(ellipse(
                centered(
                    chimney.x + chimney.width / 2.0,
                    chimney.y - r,
                    r * 2.0,
                    r * 2.0,
                ),
                faded(WHITE, 0.8),
            ))
            .named(k, &format!("smoke_{i}")),
        );
    }
    g
}

fn smoke_motion(k: &mut Kit, c: &Ctx, chimney: Rect) -> Result<(), BlockError> {
    let w = c.w();
    let (cycle, rise) = (2400u64, w * 0.12);
    for i in 0..3u64 {
        let delay = 800 * i;
        let part = format!("smoke_{i}");
        let r = w * (0.03 + 0.008 * i as f64);
        let y0 = (chimney.y - r) - (chimney.y - w * 0.12);
        let sway_x = w * 0.03 * if i % 2 == 0 { 1.0 } else { -1.0 };
        let x0 = chimney.x + chimney.width / 2.0 - r - (chimney.x - w * 0.05);
        let hold = delay.max(40);
        c.track(
            k,
            &part,
            AnimProp::Opacity,
            &[
                (0, 0.0, Ease::Linear),
                (hold, 0.0, Ease::Linear),
                (delay + 300, 0.6, Ease::EaseOut),
                (cycle, 0.0, Ease::EaseIn),
            ],
            Repeat::Loop,
        )?;
        c.track(
            k,
            &part,
            AnimProp::Y,
            &[
                (0, y0, Ease::Linear),
                (hold, y0, Ease::Linear),
                (cycle, y0 - rise, Ease::EaseOut),
            ],
            Repeat::Loop,
        )?;
        c.track(
            k,
            &part,
            AnimProp::X,
            &[
                (0, x0, Ease::Linear),
                (hold, x0, Ease::Linear),
                (cycle, x0 + sway_x, Ease::EaseInOut),
            ],
            Repeat::Loop,
        )?;
        c.track(
            k,
            &part,
            AnimProp::Scale,
            &[
                (0, 0.6, Ease::Linear),
                (hold, 0.6, Ease::Linear),
                (cycle, 1.6, Ease::EaseOut),
            ],
            Repeat::Loop,
        )?;
    }
    Ok(())
}

fn laptop(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let mut c = Ctx::read(k, a, None, "#ffbe3d")?;
    let (w, cx) = (c.w(), c.s.x + c.w() / 2.0);
    let screen = rect_of(cx - w * 0.4, c.s.y + w * 0.16, w * 0.8, w * 0.52);
    k.add("ground", ground(k, cx, c.s.y + w * 0.8, w * 0.95));
    let bezel = Rgba {
        r: 38,
        g: 44,
        b: 66,
        a: 255,
    };
    k.add(
        "screen_bezel",
        rounded(screen, w * 0.035, lit(&Paint::Color(bezel), bezel)),
    );
    let inner = rect_of(
        screen.x + w * 0.03,
        screen.y + w * 0.03,
        screen.width - w * 0.06,
        screen.height - w * 0.06,
    );
    let face = linear(
        55.0,
        &[(0.0, tint(c.main_c, 0.18)), (1.0, shade(c.main_c, 0.35))],
    );
    k.add("screen", rounded(inner, w * 0.015, face));
    for i in 0..3 {
        let dot = centered(
            inner.x + w * 0.035 + w * 0.03 * f64::from(i),
            inner.y + w * 0.03,
            w * 0.018,
            w * 0.018,
        );
        k.add(&format!("screen_dot_{i}"), ellipse(dot, faded(WHITE, 0.8)));
    }
    let widths = [0.62, 0.45, 0.54, 0.34];
    for (i, share) in widths.into_iter().enumerate() {
        let r = rect_of(
            inner.x + w * 0.04,
            inner.y + w * (0.08 + 0.05 * i as f64),
            inner.width * share,
            w * 0.022,
        );
        k.add(
            &format!("code_line_{i}"),
            rounded(r, w * 0.011, faded(WHITE, 0.85)),
        );
    }
    let cursor = rect_of(
        inner.x + w * 0.04 + inner.width * 0.36,
        inner.y + w * 0.08 + w * 0.05 * 3.0 - w * 0.004,
        w * 0.012,
        w * 0.03,
    );
    k.add(
        "cursor",
        rounded(cursor, w * 0.004, Paint::Color(c.accent_c)),
    );
    let base = rect_of(cx - w * 0.47, c.s.y + w * 0.68, w * 0.94, w * 0.07);
    let d = format!(
        "M{:.2},0 L{:.2},0 L{:.2},{:.2} Q{:.2},{:.2} {:.2},{:.2} L{:.2},{:.2} Q0,{:.2} 0,{:.2} Z",
        base.width * 0.02,
        base.width * 0.98,
        base.width,
        base.height * 0.5,
        base.width,
        base.height,
        base.width * 0.97,
        base.height,
        base.width * 0.03,
        base.height,
        base.height,
        base.height * 0.5
    );
    let metal = Rgba {
        r: 205,
        g: 212,
        b: 226,
        a: 255,
    };
    k.add(
        "laptop_base",
        path(base, d, Some(lit(&Paint::Color(metal), metal))),
    );
    k.add(
        "trackpad_notch",
        rounded(
            rect_of(cx - w * 0.07, base.y, w * 0.14, w * 0.014),
            w * 0.007,
            shade(metal, 0.25),
        ),
    );
    for (i, share) in widths.into_iter().enumerate() {
        let s = c.rng.range(1100.0, 2100.0);
        let full = inner.width * share;
        c.sway(
            k,
            &format!("code_line_{i}"),
            AnimProp::Width,
            (full * 0.35, full),
            s,
        )?;
    }
    c.track(
        k,
        "cursor",
        AnimProp::Opacity,
        &[
            (0, 1.0, Ease::Linear),
            (500, 1.0, Ease::Linear),
            (501, 0.0, Ease::Linear),
            (1000, 0.0, Ease::Linear),
        ],
        Repeat::Loop,
    )
}
