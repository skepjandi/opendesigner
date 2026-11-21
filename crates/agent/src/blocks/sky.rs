//! The sky: a gradient with an optional sun or moon, a lone light source with its glow, a
//! starfield with nebulae and shooting stars, and drifting clouds that tile across the video. Every
//! element that can live does: halos pulse, stars twinkle at their own pace, clouds drift in loops
//! that end where they began, so a video of any length never shows a seam.

use od_core::animation::{AnimProp, Repeat};
use od_core::{Ease, LineCap, NewNode, NodeKind, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Style, at, ellipse, rect};
use super::registry::{Block, Category, Layer, Place};
use super::scene::{
    Palette, alpha, blend, circle, darken, lighten, linear, num, palette, path_node, period,
    play_part, polyline, radial, slowness, soft_shadow, solid, windowed,
};
use crate::scene_motion::{Rng, loop_ramp, loop_sine};

const SEED: Param = Param::count(
    "seed",
    "The same seed always draws the same scene.",
    3,
    0,
    100_000,
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Light {
    Sun,
    Moon,
    Glow,
}

/// The colors of a sun or moon: the disc and the glow around it.
fn light_colors(p: &Palette, kind: Light) -> (Rgba, Rgba) {
    match kind {
        Light::Moon => (
            lighten(blend(p.fg, p.accent, 0.08), 0.1),
            blend(p.accent, p.fg, 0.4),
        ),
        _ => (
            lighten(blend(p.fg, p.accent, 0.2), 0.08),
            blend(p.accent, p.fg, 0.3),
        ),
    }
}

fn rays_path(size: f64, inner: f64, count: u32) -> String {
    let (c, outer) = (size / 2.0, size / 2.0);
    let half = std::f64::consts::PI / f64::from(count) * 0.42;
    (0..count)
        .map(|i| {
            let a = std::f64::consts::TAU * f64::from(i) / f64::from(count);
            let tip = |r: f64, da: f64| (c + r * (a + da).cos(), c + r * (a + da).sin());
            let points = [
                tip(inner, -half * 0.35),
                tip(outer, -half),
                tip(outer, half),
                tip(inner, half * 0.35),
            ];
            polyline(&points, true)
        })
        .collect()
}

/// A sun, moon or bare glow centered at (`cx`, `cy`) with disc radius `r`: two halos that breathe,
/// slow rays that turn, and the disc. Parts are named `light-*`.
fn light(k: &mut Kit, p: &Palette, cx: f64, cy: f64, r: f64, kind: Light, glow: f64) {
    let (core, halo) = light_colors(p, kind);
    k.begin("light");
    let square = |size: f64| at(cx - size / 2.0, cy - size / 2.0, size, size);
    let wide = radial(
        0.5,
        0.5,
        0.5,
        &[
            (0.0, alpha(halo, 0.5 * glow)),
            (0.3, alpha(halo, 0.22 * glow)),
            (1.0, alpha(halo, 0.0)),
        ],
    );
    k.add("light-halo", ellipse(square(r * 7.0), wide));
    let tight = radial(
        0.5,
        0.5,
        0.5,
        &[(0.0, alpha(core, 0.55 * glow)), (1.0, alpha(core, 0.0))],
    );
    k.add("light-glow", ellipse(square(r * 3.4), tight));
    if kind == Light::Sun {
        let size = r * 5.6;
        let fill = radial(
            0.5,
            0.5,
            0.5,
            &[(0.2, alpha(core, 0.2)), (1.0, alpha(core, 0.0))],
        );
        let d = rays_path(size, r * 1.1, 14);
        k.add("light-rays", path_node(d, square(size), Some(fill)));
        play_part(
            k,
            "light-rays",
            &loop_ramp(AnimProp::Rotation, 0.0, 360.0, 120_000),
        );
    }
    if kind != Light::Glow {
        let disc_fill = radial(
            0.38,
            0.34,
            0.8,
            &[
                (0.0, lighten(core, 0.4)),
                (0.6, core),
                (1.0, blend(core, p.accent, 0.3)),
            ],
        );
        let mut disc = ellipse(square(r * 2.0), disc_fill);
        disc.shadow = Some(soft_shadow(alpha(halo, 0.7 * glow), 0.0, r * 0.7));
        k.add("light-disc", disc);
    }
    if kind == Light::Moon {
        moon_craters(k, core, cx, cy, r);
    }
    k.end_with(|g| g.depth = Some(0.05));
    play_part(
        k,
        "light-halo",
        &loop_sine(AnimProp::Scale, 1.0, 0.05, 7_000, 0.0),
    );
    play_part(
        k,
        "light-glow",
        &loop_sine(AnimProp::Scale, 1.0, 0.07, 5_200, 0.35),
    );
}

fn moon_craters(k: &mut Kit, core: Rgba, cx: f64, cy: f64, r: f64) {
    let shade = alpha(darken(core, 0.22), 0.45);
    for (i, (dx, dy, s)) in [(-0.35, -0.2, 0.28), (0.3, 0.1, 0.2), (-0.1, 0.42, 0.16)]
        .into_iter()
        .enumerate()
    {
        let d = r * 2.0 * s;
        let crater = ellipse(
            at(cx + dx * r - d / 2.0, cy + dy * r - d / 2.0, d, d),
            solid(shade),
        );
        k.add(&format!("light-crater-{}", i + 1), crater);
    }
}

fn kind_of(name: &str) -> Light {
    match name {
        "moon" => Light::Moon,
        "glow" => Light::Glow,
        _ => Light::Sun,
    }
}

const SKY: &[Param] = &[
    Param::number(
        "angle",
        "Direction of the gradient in degrees: 90 runs from the top down, 0 from the left.",
        90.0,
        0.0,
        180.0,
    ),
    Param::count("stops", "Two to four color stops.", 3, 2, 4),
    Param::choice(
        "light",
        "A sun or a moon with a soft glow in the sky, or none.",
        "none",
        &["none", "sun", "moon"],
    ),
    Param::number(
        "light_x",
        "Where the disc sits across the box, in percent.",
        72.0,
        0.0,
        100.0,
    ),
    Param::number(
        "light_y",
        "Where the disc sits down the box, in percent.",
        30.0,
        0.0,
        100.0,
    ),
    Param::number(
        "light_size",
        "Disc radius as a percent of the shorter side.",
        9.0,
        2.0,
        30.0,
    ),
];

fn sky_stops(p: &Palette, count: i64) -> Vec<(f64, Rgba)> {
    let low = blend(p.accent, p.fg, 0.35);
    match count {
        2 => vec![(0.0, p.bg), (1.0, low)],
        3 => vec![(0.0, p.bg), (0.55, blend(p.bg, p.accent, 0.6)), (1.0, low)],
        _ => vec![
            (0.0, p.bg),
            (0.45, blend(p.bg, p.accent, 0.45)),
            (0.8, low),
            (1.0, blend(p.accent, p.fg, 0.62)),
        ],
    }
}

/// A wide soft glow low in the sky that slowly breathes and drifts, so the gradient is never dead.
fn horizon_glow(k: &mut Kit, p: &Palette) {
    let b = k.b;
    let (w, h) = (b.width * 1.3, b.height * 0.55);
    let (x, y) = (b.x - b.width * 0.15, b.y + b.height - h * 0.55);
    let c = lighten(blend(p.accent, p.fg, 0.4), 0.1);
    let fill = radial(
        0.5,
        0.5,
        0.5,
        &[(0.0, alpha(c, 0.32)), (1.0, alpha(c, 0.0))],
    );
    let mut glow = ellipse(at(x, y, w, h), fill);
    glow.depth = Some(0.0);
    k.add("sky-glow", glow);
    play_part(
        k,
        "sky-glow",
        &loop_sine(AnimProp::Opacity, 0.65, 0.35, 9_000, 0.0),
    );
    play_part(
        k,
        "sky-glow",
        &loop_sine(AnimProp::X, x, b.width * 0.03, 15_000, 0.3),
    );
}

fn sky_gradient(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let fill = linear(a.num("angle"), &sky_stops(&p, a.num("stops") as i64));
    let mut sky = rect(k.b, fill);
    sky.depth = Some(0.0);
    k.add("sky", sky);
    horizon_glow(k, &p);
    let kind = a.text("light");
    let (cx, cy) = (
        k.b.x + k.b.width * a.num("light_x") / 100.0,
        k.b.y + k.b.height * a.num("light_y") / 100.0,
    );
    let r = k.s(a.num("light_size"));
    if kind != "none" {
        light(k, &p, cx, cy, r, kind_of(kind), 1.0);
    }
    k.fade_root(0, 700);
    Ok(())
}

const SUN: &[Param] = &[
    Param::choice(
        "kind",
        "sun (disc, glow and slow rays), moon (disc with craters and glow) or glow (a bare halo).",
        "sun",
        &["sun", "moon", "glow"],
    ),
    Param::number(
        "x",
        "Where it sits across the box, in percent.",
        70.0,
        0.0,
        100.0,
    ),
    Param::number(
        "y",
        "Where it sits down the box, in percent.",
        30.0,
        0.0,
        100.0,
    ),
    Param::number(
        "size",
        "Disc radius as a percent of the shorter side.",
        9.0,
        2.0,
        30.0,
    ),
    Param::number("glow", "How strong the halo is.", 1.0, 0.2, 1.5),
];

fn sun_glow(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let (cx, cy) = (
        k.b.x + k.b.width * a.num("x") / 100.0,
        k.b.y + k.b.height * a.num("y") / 100.0,
    );
    let (r, glow, kind) = (k.s(a.num("size")), a.num("glow"), kind_of(a.text("kind")));
    windowed(k, |k| light(k, &p, cx, cy, r, kind, glow));
    k.fade_root(0, 900);
    Ok(())
}

const STARS: &[Param] = &[
    Param::count(
        "count",
        "Stars (a third are large and twinkle most).",
        140,
        20,
        220,
    ),
    Param::count(
        "shooting",
        "Shooting stars that streak across now and then.",
        2,
        0,
        3,
    ),
    Param::count(
        "nebula",
        "Soft colored clouds of light behind the stars.",
        2,
        0,
        3,
    ),
    Param::count(
        "loop_ms",
        "How often a shooting star comes; the first one waits a fraction of this.",
        12_000,
        4_000,
        60_000,
    ),
    SEED,
];

struct StarClass {
    name: &'static str,
    share: f64,
    groups: usize,
}

const CLASSES: [StarClass; 3] = [
    StarClass {
        name: "small",
        share: 0.6,
        groups: 3,
    },
    StarClass {
        name: "medium",
        share: 0.3,
        groups: 2,
    },
    StarClass {
        name: "large",
        share: 0.1,
        groups: 2,
    },
];

fn sparkle(r: f64) -> String {
    let (c, i) = (r, r * 0.22);
    polyline(
        &[
            (c, 0.0),
            (c + i, c - i),
            (2.0 * r, c),
            (c + i, c + i),
            (c, 2.0 * r),
            (c - i, c + i),
            (0.0, c),
            (c - i, c - i),
        ],
        true,
    )
}

fn star(k: &mut Kit, rng: &mut Rng, p: &Palette, class: usize, name: &str) {
    let b = k.b;
    let tint = if rng.chance(0.28) {
        lighten(p.accent, 0.55)
    } else {
        p.fg
    };
    let (x, y) = (
        b.x + rng.unit() * b.width,
        b.y + rng.unit().powf(1.25) * b.height,
    );
    let radius = [k.s(0.14).max(0.9), k.s(0.23).max(1.4), k.s(0.55).max(3.5)][class];
    let r = radius * rng.range(0.8, 1.25);
    let node = if class == 2 {
        let mut n = path_node(
            sparkle(r),
            at(x - r, y - r, 2.0 * r, 2.0 * r),
            Some(solid(tint)),
        );
        n.shadow = Some(soft_shadow(alpha(tint, 0.7), 0.0, r * 0.9));
        n
    } else {
        ellipse(
            at(x - r, y - r, 2.0 * r, 2.0 * r),
            solid(alpha(tint, rng.range(0.55, 1.0))),
        )
    };
    k.add(name, node);
}

fn twinkle_groups(k: &mut Kit, rng: &mut Rng, p: &Palette, count: usize) {
    let mut serial = 0;
    for (class, c) in CLASSES.iter().enumerate() {
        let total = ((count as f64 * c.share).round() as usize).max(1);
        let mut per_group = vec![0; c.groups];
        (0..total).for_each(|i| per_group[i % c.groups] += 1);
        for (g, n) in per_group.into_iter().enumerate() {
            let group = format!("stars-{}-{}", c.name, g + 1);
            k.begin(&group);
            for _ in 0..n {
                serial += 1;
                star(k, rng, p, class, &format!("star-{serial}"));
            }
            k.end_with(|n| n.depth = Some(0.04));
            let base = [0.72, 0.74, 0.76][class];
            let wave = loop_sine(
                AnimProp::Opacity,
                base,
                1.0 - base,
                rng.count(2_200, 5_200),
                rng.unit(),
            );
            play_part(k, &group, &wave);
        }
    }
}

fn nebulae(k: &mut Kit, rng: &mut Rng, p: &Palette, count: usize) {
    let b = k.b;
    let tints = [
        p.accent,
        blend(p.accent, p.muted, 0.55),
        blend(p.accent, p.fg, 0.3),
    ];
    for i in 0..count {
        let (w, h) = (
            b.width * rng.range(0.55, 0.9),
            b.height * rng.range(0.4, 0.65),
        );
        let (x, y) = (
            b.x + rng.unit() * (b.width - w),
            b.y + rng.unit() * (b.height - h),
        );
        let c = tints[i % tints.len()];
        let fill = radial(
            0.5,
            0.5,
            0.5,
            &[
                (0.0, alpha(c, 0.36)),
                (0.55, alpha(c, 0.13)),
                (1.0, alpha(c, 0.0)),
            ],
        );
        let mut n = ellipse(at(x, y, w, h), fill);
        n.depth = Some(0.02);
        let name = format!("nebula-{}", i + 1);
        k.add(&name, n);
        let drift = loop_sine(
            AnimProp::X,
            x,
            b.width * 0.02,
            rng.count(24_000, 36_000),
            rng.unit(),
        );
        play_part(k, &name, &drift);
        let breathe = loop_sine(
            AnimProp::Opacity,
            0.85,
            0.15,
            rng.count(9_000, 15_000),
            rng.unit(),
        );
        play_part(k, &name, &breathe);
    }
}

fn shooting_star(k: &mut Kit, rng: &mut Rng, p: &Palette, index: usize, every_ms: u64) {
    let b = k.b;
    let len = k.s(13.0);
    let group = format!("shoot-{index}");
    let line_part = format!("shoot-{index}-line");
    let (x0, y0) = (
        b.x + b.width * rng.range(0.5, 0.95),
        b.y + b.height * rng.range(0.04, 0.35),
    );
    let mut line = path_node(
        polyline(&[(len, 0.0), (0.0, len * 0.38)], false),
        at(0.0, 0.0, len, len * 0.38),
        None,
    )
    .outline(solid(p.fg), k.s(0.34).max(2.0));
    if let Some(s) = line.stroke.as_mut() {
        s.cap = LineCap::Round;
    }
    line.shadow = Some(soft_shadow(alpha(p.fg, 0.8), 0.0, k.s(0.8)));
    k.begin(&group);
    k.add(&line_part, line);
    k.end_with(|g| {
        g.x = x0;
        g.y = y0;
        g.depth = Some(0.1);
    });
    let cycle = period(every_ms, 1.0 + 0.4 * index as f64) / 100 * 100;
    let hit = (cycle as f64 * rng.range(0.18, 0.4)) as u64 / 100 * 100;
    let (travel_x, travel_y) = (-b.width * 0.26, b.width * 0.26 * 0.38);
    let track = |prop, keys: &[(u64, f64, Ease)]| {
        let t = k.part(&group);
        (t, prop, keys.to_vec())
    };
    let moves = [
        track(
            AnimProp::X,
            &[
                (0, x0, Ease::Linear),
                (hit, x0, Ease::Linear),
                (hit + 800, x0 + travel_x, Ease::EaseOut),
                (cycle - 200, x0 + travel_x, Ease::Linear),
                (cycle, x0, Ease::Linear),
            ],
        ),
        track(
            AnimProp::Y,
            &[
                (0, y0, Ease::Linear),
                (hit, y0, Ease::Linear),
                (hit + 800, y0 + travel_y, Ease::EaseOut),
                (cycle - 200, y0 + travel_y, Ease::Linear),
                (cycle, y0, Ease::Linear),
            ],
        ),
        track(
            AnimProp::Opacity,
            &[
                (0, 0.0, Ease::Linear),
                (hit, 0.0, Ease::Linear),
                (hit + 80, 1.0, Ease::EaseOut),
                (hit + 600, 1.0, Ease::Linear),
                (hit + 800, 0.0, Ease::EaseIn),
                (cycle, 0.0, Ease::Linear),
            ],
        ),
    ];
    for (target, prop, keys) in moves {
        k.track(target, prop, &keys, Repeat::Loop);
    }
    let end = k.part(&line_part);
    let trims = [
        (
            AnimProp::TrimEnd,
            [
                (0, 0.0, Ease::Linear),
                (hit, 0.0, Ease::Linear),
                (hit + 300, 1.0, Ease::EaseOut),
                (cycle - 200, 1.0, Ease::Linear),
                (cycle, 0.0, Ease::Linear),
            ],
        ),
        (
            AnimProp::TrimStart,
            [
                (0, 0.0, Ease::Linear),
                (hit + 300, 0.0, Ease::Linear),
                (hit + 800, 1.0, Ease::EaseIn),
                (cycle - 200, 1.0, Ease::Linear),
                (cycle, 0.0, Ease::Linear),
            ],
        ),
    ];
    for (prop, keys) in trims {
        k.track(end.clone(), prop, &keys, Repeat::Loop);
    }
}

fn starfield(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let mut rng = Rng::new(a.num("seed") as u64);
    nebulae(k, &mut rng, &p, a.num("nebula") as usize);
    twinkle_groups(k, &mut rng, &p, a.num("count") as usize);
    for i in 0..a.num("shooting") as usize {
        shooting_star(k, &mut rng, &p, i + 1, a.num("loop_ms") as u64);
    }
    k.fade_root(0, 900);
    Ok(())
}

const CLOUDS: &[Param] = &[
    Param::count(
        "layers",
        "Depth layers of clouds; farther ones are smaller, paler and slower.",
        3,
        1,
        3,
    ),
    Param::count(
        "per_layer",
        "Clouds in one screen width of a layer.",
        3,
        2,
        5,
    ),
    Param::count(
        "loop_ms",
        "How long the nearest layer takes to drift one screen width; the others are slower. The drift repeats seamlessly.",
        90_000,
        20_000,
        300_000,
    ),
    Param::number(
        "min_depth",
        "Camera parallax of the farthest layer (0 stays on the screen, 1 moves with the scene).",
        0.15,
        0.0,
        3.0,
    ),
    Param::number(
        "max_depth",
        "Camera parallax of the nearest layer.",
        0.55,
        0.0,
        3.0,
    ),
    SEED,
];

/// A cloud as one path: a flat-bottomed pill and three bumps, filled as their union.
fn cloud(rng: &mut Rng, w: f64, top: Rgba, bottom: Rgba, shadow: Rgba) -> NewNode {
    let h = w * 0.56;
    let r = w * 0.11;
    let pill = format!(
        "M{},{}H{}A{r},{r} 0 0 1 {},{}H{}A{r},{r} 0 0 1 {},{}Z",
        num(w * 0.04 + r),
        num(w * 0.34),
        num(w * 0.96 - r),
        num(w * 0.96 - r),
        num(w * 0.56),
        num(w * 0.04 + r),
        num(w * 0.04 + r),
        num(w * 0.34),
        r = num(r),
    );
    let bumps = [(0.30, 0.40, 0.14), (0.52, 0.30, 0.21), (0.74, 0.40, 0.15)];
    let mut d = pill;
    for (cx, cy, br) in bumps {
        let s = rng.range(0.9, 1.1);
        let br = (br * s).min(cy - 0.02);
        d.push_str(&circle(w * (cx + rng.range(-0.02, 0.02)), w * cy, w * br));
    }
    let mut n = path_node(
        d,
        at(0.0, 0.0, w, h),
        Some(linear(90.0, &[(0.0, top), (1.0, bottom)])),
    );
    n.shadow = Some(soft_shadow(shadow, w * 0.03, w * 0.05));
    n
}

fn cloud_tile(
    k: &mut Kit,
    rng: &mut Rng,
    p: &Palette,
    t: f64,
    per_layer: usize,
    tile: f64,
    row: (f64, f64),
) -> Vec<NewNode> {
    let sky = blend(p.bg, p.accent, 0.3 + 0.9 * (row.0 + row.1) / 2.0);
    let haze = (1.0 - t) * 0.55;
    let top = blend(blend(p.fg, p.accent, 0.18), sky, haze);
    let bottom = blend(blend(p.fg, p.accent, 0.55), sky, haze);
    let shadow = alpha(darken(p.accent, 0.6), 0.28);
    let slot = tile / per_layer as f64;
    (0..per_layer)
        .map(|i| {
            let w = (k.b.width * (0.075 + 0.1 * t) * rng.range(0.8, 1.2))
                .min(slot * 1.4)
                .min(tile * 0.4);
            let x = (slot * i as f64 + rng.unit() * (slot - w).max(0.0)).min(tile - w);
            let y = k.b.y + k.b.height * rng.range(row.0, row.1);
            let mut c = cloud(rng, w, top, bottom, shadow);
            c.x = k.b.x + x;
            c.y = y;
            c.opacity = Some(0.55 + 0.45 * t);
            c
        })
        .collect()
}

fn clouds(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let mut rng = Rng::new(a.num("seed") as u64);
    let layers = a.num("layers") as usize;
    let tile = k.b.width;
    let (lo, hi) = (
        a.num("min_depth"),
        a.num("max_depth").max(a.num("min_depth")),
    );
    let base = a.num("loop_ms") as u64;
    windowed(k, |k| {
        for i in 0..layers {
            let t = if layers == 1 {
                1.0
            } else {
                i as f64 / (layers - 1) as f64
            };
            let row = (0.04 + 0.1 * (1.0 - t), 0.3 + 0.12 * t);
            let clouds = cloud_tile(k, &mut rng, &p, t, a.num("per_layer") as usize, tile, row);
            let name = format!("layer-{}", i + 1);
            k.begin(&name);
            for copy in 0..4 {
                let tile_name = format!("{name}-tile-{}", copy + 1);
                let mut g = NewNode::new(NodeKind::Group);
                g.x = (f64::from(copy) - 2.0) * tile;
                g.children = clouds.clone();
                for (j, c) in g.children.iter_mut().enumerate() {
                    c.name = Some(k.full(&format!("{tile_name}-cloud-{}", j + 1)));
                }
                k.add(&tile_name, g);
            }
            k.end_with(|g| g.depth = Some(lo + (hi - lo) * t));
            let one_tile = period(base, slowness(i, layers) as f64);
            play_part(k, &name, &loop_ramp(AnimProp::X, 0.0, tile, one_tile));
            let bob = loop_sine(
                AnimProp::Y,
                0.0,
                k.s(0.5),
                period(one_tile, [0.5, 0.25][rng.count(0, 1) as usize]),
                rng.unit(),
            );
            play_part(k, &name, &bob);
        }
    });
    k.fade_root(0, 900);
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "sky_gradient",
        category: Category::Backgrounds,
        about: "A soft multi-stop sky gradient (vertical or diagonal) from the palette, with an optional sun or moon disc and glow that breathes. The base of a flat-vector scene; add parallax_hills, clouds or starfield over it.",
        tags: &[
            "sky",
            "gradient",
            "background",
            "sun",
            "moon",
            "scene",
            "backdrop",
            "dusk",
            "illustration",
        ],
        params: SKY,
        place: Place::Frame,
        layer: Layer::Scenery(0),
        build: sky_gradient,
    },
    Block {
        id: "sun_glow",
        category: Category::Backgrounds,
        about: "A sun, moon or bare glow: a disc with a gradient, two halos that breathe and (for the sun) slow rays that turn. Place it anywhere in the box.",
        tags: &[
            "sun",
            "moon",
            "glow",
            "light",
            "halo",
            "rays",
            "scene",
            "illustration",
        ],
        params: SUN,
        place: Place::Frame,
        layer: Layer::Scenery(2),
        build: sun_glow,
    },
    Block {
        id: "starfield",
        category: Category::Backgrounds,
        about: "Stars of three sizes that twinkle at their own pace, soft nebula glows and shooting stars that streak across in loops. Up to about 240 nodes.",
        tags: &[
            "stars",
            "space",
            "night",
            "twinkle",
            "nebula",
            "shooting",
            "galaxy",
            "background",
            "scene",
        ],
        params: STARS,
        place: Place::Frame,
        layer: Layer::Scenery(1),
        build: starfield,
    },
    Block {
        id: "clouds",
        category: Category::Backgrounds,
        about: "Soft layered clouds drifting at different speeds in loops that tile seamlessly for the whole video; far layers are paler and move less with the camera.",
        tags: &[
            "clouds",
            "sky",
            "drift",
            "parallax",
            "weather",
            "background",
            "scene",
            "loop",
        ],
        params: CLOUDS,
        place: Place::Frame,
        layer: Layer::Scenery(3),
        build: clouds,
    },
];
