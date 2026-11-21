//! Small things that fill a scene with life: the soft, translucent cells of a microscope's view, and
//! particles (bubbles, dust, confetti, snow, embers, sparkles) that rise, fall or twinkle in loops
//! that repeat exactly.

use od_core::animation::AnimProp;
use od_core::{LineJoin, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Style, at, ellipse, rect};
use super::registry::{Block, Category, Layer, Place};
use super::scene::{
    Palette, alpha, blend, darken, lighten, palette, path_node, play_part, polyline, radial,
    smooth, solid,
};
use crate::scene_motion::{Rng, loop_ramp, loop_sine};

const SEED: Param = Param::count(
    "seed",
    "The same seed always draws the same scene.",
    3,
    0,
    100_000,
);

const CELLS: &[Param] = &[
    Param::count("count", "Cells across all depth planes.", 16, 6, 30),
    Param::count(
        "planes",
        "Depth planes; the far and near ones are blurred like an out-of-focus microscope view.",
        3,
        2,
        3,
    ),
    Param::flag("blur", "Blur the far and near planes.", true),
    Param::count(
        "loop_ms",
        "Base period of the drift; each cell has its own fraction of it.",
        16_000,
        4_000,
        120_000,
    ),
    SEED,
];

/// A closed organic outline of mean radius `r` around the origin.
fn blob_points(rng: &mut Rng, r: f64, wobble: f64) -> Vec<(f64, f64)> {
    let n = 9;
    let turn = rng.unit() * std::f64::consts::TAU;
    (0..n)
        .map(|i| {
            let a = turn + std::f64::consts::TAU * f64::from(i) / f64::from(n);
            let radius = r * (1.0 + rng.range(-wobble, wobble));
            (r * 1.3 + radius * a.cos(), r * 1.3 + radius * a.sin())
        })
        .collect()
}

fn cell_group(k: &mut Kit, rng: &mut Rng, p: &Palette, name: &str, r: f64, tint: Rgba) {
    let boxed = r * 2.6;
    let body_fill = radial(
        0.38,
        0.32,
        0.8,
        &[
            (0.0, alpha(lighten(tint, 0.3), 0.6)),
            (1.0, alpha(tint, 0.3)),
        ],
    );
    let membrane = solid(alpha(lighten(tint, 0.5), 0.75));
    let mut body = path_node(
        smooth(&blob_points(rng, r, 0.16), true),
        at(-boxed / 2.0, -boxed / 2.0, boxed, boxed),
        Some(body_fill),
    )
    .outline(membrane, (r * 0.07).max(1.2));
    if let Some(s) = body.stroke.as_mut() {
        s.join = LineJoin::Round;
    }
    k.add(&format!("{name}-body"), body);
    let nucleus_r = r * rng.range(0.28, 0.4);
    let nucleus_box = nucleus_r * 2.6;
    let (nx, ny) = (rng.range(-0.25, 0.25) * r, rng.range(-0.25, 0.25) * r);
    let nucleus_fill = radial(
        0.4,
        0.35,
        0.8,
        &[
            (0.0, alpha(lighten(tint, 0.15), 0.8)),
            (1.0, alpha(darken(blend(tint, p.bg, 0.4), 0.1), 0.75)),
        ],
    );
    k.add(
        &format!("{name}-nucleus"),
        path_node(
            smooth(&blob_points(rng, nucleus_r, 0.12), true),
            at(
                nx - nucleus_box / 2.0,
                ny - nucleus_box / 2.0,
                nucleus_box,
                nucleus_box,
            ),
            Some(nucleus_fill),
        ),
    );
    let g = r * 0.22;
    k.add(
        &format!("{name}-glint"),
        ellipse(at(-r * 0.5, -r * 0.55, g * 1.6, g), solid(alpha(p.fg, 0.4))),
    );
}

fn cells_backdrop(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let mut rng = Rng::new(a.num("seed") as u64);
    let b = k.b;
    super::scene::windowed(k, |k| {
        let mut fluid = rect(
            b,
            radial(
                0.5,
                0.45,
                0.75,
                &[
                    (0.0, blend(p.accent, p.bg, 0.55)),
                    (0.6, blend(p.bg, p.accent, 0.22)),
                    (1.0, darken(p.bg, 0.15)),
                ],
            ),
        );
        fluid.depth = Some(0.0);
        k.add("fluid", fluid);
        let planes = a.num("planes") as usize;
        let count = a.num("count") as usize;
        let tints = [
            p.accent,
            blend(p.accent, p.muted, 0.5),
            blend(p.accent, p.fg, 0.35),
            blend(p.muted, p.accent, 0.3),
        ];
        let (depths, sizes) = if planes == 2 {
            (vec![0.2, 1.3], vec![5.0, 12.0])
        } else {
            (vec![0.15, 0.55, 1.4], vec![4.5, 8.0, 13.0])
        };
        let shares = if planes == 2 {
            vec![0.6, 0.4]
        } else {
            vec![0.4, 0.35, 0.25]
        };
        let mut serial = 0;
        for plane in 0..planes {
            let plane_name = format!("plane-{}", plane + 1);
            k.begin(&plane_name);
            let blurred = a.flag("blur") && (plane == 0 || plane + 1 == planes);
            let mut cells = Vec::new();
            for _ in 0..((count as f64 * shares[plane]).round() as usize).max(1) {
                serial += 1;
                let name = format!("cell-{serial}");
                let r = k.s(sizes[plane]) * rng.range(0.7, 1.25);
                let (x, y) = (
                    b.x + r + rng.unit() * (b.width - 2.0 * r),
                    b.y + r + rng.unit() * (b.height - 2.0 * r),
                );
                let tint = tints[rng.count(0, 3) as usize];
                k.begin(&name);
                cell_group(k, &mut rng, &p, &name, r, tint);
                k.end_with(|g| {
                    g.x = x;
                    g.y = y;
                    g.opacity = Some(if plane == 1 { 0.9 } else { 0.7 });
                });
                cells.push((name, x, y, r));
            }
            k.end_with(|g| {
                g.depth = Some(depths[plane]);
                if blurred {
                    g.blur = Some(b.height.min(b.width) * if plane == 0 { 0.006 } else { 0.012 });
                }
            });
            let base = a.num("loop_ms") as u64;
            for (name, x, y, r) in cells {
                let secs = |f: f64| ((base as f64 * f / 1000.0).round() as u64).max(2) * 1000;
                play_part(
                    k,
                    &name,
                    &loop_sine(
                        AnimProp::X,
                        x,
                        r * rng.range(0.4, 0.9),
                        secs(rng.range(0.6, 1.4)),
                        rng.unit(),
                    ),
                );
                play_part(
                    k,
                    &name,
                    &loop_sine(
                        AnimProp::Y,
                        y,
                        r * rng.range(0.4, 0.9),
                        secs(rng.range(0.6, 1.4)),
                        rng.unit(),
                    ),
                );
                play_part(
                    k,
                    &name,
                    &loop_sine(
                        AnimProp::Rotation,
                        0.0,
                        rng.range(4.0, 9.0),
                        secs(rng.range(0.8, 1.6)),
                        rng.unit(),
                    ),
                );
                play_part(
                    k,
                    &format!("{name}-nucleus"),
                    &loop_sine(
                        AnimProp::Rotation,
                        0.0,
                        rng.range(8.0, 16.0),
                        secs(rng.range(0.5, 1.0)),
                        rng.unit(),
                    ),
                );
            }
        }
    });
    k.fade_root(0, 900);
    Ok(())
}

const KINDS: &[&str] = &["bubbles", "dust", "confetti", "snow", "embers", "sparkles"];

const PARTICLES: &[Param] = &[
    Param::choice(
        "kind",
        "bubbles (rise and wobble), dust (slow motes), confetti (falls and turns), snow, embers (rise and flicker) or sparkles (twinkle in place).",
        "bubbles",
        KINDS,
    ),
    Param::count("count", "Particles on screen.", 36, 8, 70),
    Param::count(
        "loop_ms",
        "The loop, rounded to a multiple of four seconds: everything repeats exactly then.",
        16_000,
        4_000,
        64_000,
    ),
    Param::number(
        "size",
        "Particle size, as a multiple of the default.",
        1.0,
        0.4,
        3.0,
    ),
    Param::number(
        "min_depth",
        "Camera parallax of the farthest plane of particles.",
        0.4,
        0.0,
        3.0,
    ),
    Param::number(
        "max_depth",
        "Camera parallax of the nearest plane.",
        1.3,
        0.0,
        3.0,
    ),
    SEED,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Bubbles,
    Dust,
    Confetti,
    Snow,
    Embers,
    Sparkles,
}

impl Kind {
    fn parse(s: &str) -> Kind {
        match s {
            "dust" => Kind::Dust,
            "confetti" => Kind::Confetti,
            "snow" => Kind::Snow,
            "embers" => Kind::Embers,
            "sparkles" => Kind::Sparkles,
            _ => Kind::Bubbles,
        }
    }

    /// How many times faster each plane (far, middle, near) cycles than the loop.
    fn speeds(self) -> [u64; 3] {
        match self {
            Kind::Bubbles => [1, 2, 4],
            Kind::Dust | Kind::Sparkles => [1, 1, 2],
            Kind::Snow => [2, 2, 4],
            Kind::Confetti | Kind::Embers => [2, 4, 4],
        }
    }

    /// `-1` rises, `1` falls, `0` stays.
    fn fall(self) -> f64 {
        match self {
            Kind::Bubbles | Kind::Embers => -1.0,
            Kind::Confetti | Kind::Snow => 1.0,
            Kind::Dust | Kind::Sparkles => 0.0,
        }
    }
}

/// A whole-second wobble period that divides `plane_ms`, two seconds or longer when there is one.
fn wobble_ms(rng: &mut Rng, plane_ms: u64) -> u64 {
    let secs = plane_ms / 1_000;
    let divisors: Vec<u64> = (1..=8).filter(|s| secs % s == 0).collect();
    let long: Vec<u64> = divisors.iter().copied().filter(|s| *s >= 2).collect();
    let pool = if long.is_empty() { divisors } else { long };
    pool[rng.count(0, pool.len() as u64 - 1) as usize] * 1_000
}

fn sparkle_path(r: f64) -> String {
    let (c, i) = (r, r * 0.2);
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

/// Where a particle is, what it is called and how fast its plane cycles.
struct Spot<'a> {
    kind: Kind,
    name: &'a str,
    pos: (f64, f64),
    unit: f64,
    plane_ms: u64,
}

/// One particle: its node and the loops that give it life.
fn particle(k: &mut Kit, rng: &mut Rng, p: &Palette, spot: &Spot) {
    let Spot {
        kind,
        name,
        pos: (x, y),
        unit,
        plane_ms,
    } = *spot;
    let wp = wobble_ms(rng, plane_ms);
    let warm = blend(
        p.accent,
        Rgba {
            r: 255,
            g: 170,
            b: 90,
            a: 255,
        },
        0.55,
    );
    let (node, wobble) = match kind {
        Kind::Bubbles => {
            let d = unit * rng.range(1.0, 2.6);
            let c = lighten(blend(p.accent, p.fg, 0.4), 0.3);
            let fill = radial(
                0.35,
                0.3,
                0.7,
                &[
                    (0.0, alpha(c, 0.05)),
                    (0.75, alpha(c, 0.14)),
                    (1.0, alpha(c, 0.45)),
                ],
            );
            (
                ellipse(at(x - d / 2.0, y - d / 2.0, d, d), fill)
                    .outline(solid(alpha(c, 0.7)), (d * 0.05).max(1.0)),
                d * 0.7,
            )
        }
        Kind::Dust => {
            let d = unit * rng.range(0.35, 0.9);
            (
                ellipse(
                    at(x - d / 2.0, y - d / 2.0, d, d),
                    solid(alpha(p.fg, rng.range(0.25, 0.6))),
                ),
                unit * 5.0,
            )
        }
        Kind::Snow => {
            let d = unit * rng.range(0.5, 1.6);
            (
                ellipse(
                    at(x - d / 2.0, y - d / 2.0, d, d),
                    solid(alpha(p.fg, rng.range(0.6, 0.95))),
                ),
                d * 2.5,
            )
        }
        Kind::Confetti => {
            let (w, h) = (unit * rng.range(2.4, 4.0), unit * rng.range(1.2, 2.0));
            let colors = [
                p.accent,
                lighten(p.accent, 0.5),
                p.fg,
                blend(p.accent, p.muted, 0.5),
                blend(p.fg, p.accent, 0.6),
            ];
            let c = colors[rng.count(0, 4) as usize];
            (
                rect(at(x - w / 2.0, y - h / 2.0, w, h), solid(c)).radius(h * 0.2),
                unit * 2.5,
            )
        }
        Kind::Embers => {
            let d = unit * rng.range(2.4, 6.0);
            let fill = radial(
                0.5,
                0.5,
                0.5,
                &[
                    (0.0, lighten(warm, 0.55)),
                    (0.35, warm),
                    (1.0, alpha(warm, 0.0)),
                ],
            );
            (ellipse(at(x - d / 2.0, y - d / 2.0, d, d), fill), d * 0.9)
        }
        Kind::Sparkles => {
            let r = unit * rng.range(0.9, 1.8);
            (
                path_node(
                    sparkle_path(r),
                    at(x - r, y - r, 2.0 * r, 2.0 * r),
                    Some(solid(lighten(p.fg, 0.1))),
                ),
                0.0,
            )
        }
    };
    k.add(name, node);
    let phase = rng.unit();
    if kind == Kind::Sparkles {
        play_part(k, name, &loop_sine(AnimProp::Scale, 0.7, 0.3, wp, phase));
        play_part(k, name, &loop_sine(AnimProp::Opacity, 0.6, 0.4, wp, phase));
        return;
    }
    play_part(k, name, &loop_sine(AnimProp::X, x, wobble, wp, phase));
    if kind == Kind::Dust {
        play_part(
            k,
            name,
            &loop_sine(
                AnimProp::Y,
                y,
                wobble * 0.6,
                wobble_ms(rng, plane_ms),
                rng.unit(),
            ),
        );
    }
    if kind == Kind::Confetti {
        let from = rng.range(0.0, 360.0);
        play_part(
            k,
            name,
            &loop_ramp(AnimProp::Rotation, from, from + 360.0, wp),
        );
    }
    if kind == Kind::Embers {
        play_part(
            k,
            name,
            &loop_sine(
                AnimProp::Opacity,
                0.75,
                0.25,
                wobble_ms(rng, plane_ms),
                rng.unit(),
            ),
        );
    }
}

fn particles(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let mut rng = Rng::new(a.num("seed") as u64);
    let kind = Kind::parse(a.text("kind"));
    let b = k.b;
    let loop_ms = (a.num("loop_ms") as u64 / 4_000).max(1) * 4_000;
    let (lo, hi) = (
        a.num("min_depth"),
        a.num("max_depth").max(a.num("min_depth")),
    );
    let shares = [0.4, 0.35, 0.25];
    let sizes = [0.65, 1.0, 1.5];
    let unit = k.s(0.9) * a.num("size");
    super::scene::windowed(k, |k| {
        for plane in 0..3 {
            let plane_ms = loop_ms / kind.speeds()[plane];
            let n = ((a.num("count") * shares[plane]).round() as usize).max(2);
            let plane_name = format!("plane-{}", plane + 1);
            let scroll = format!("{plane_name}-scroll");
            k.begin(&plane_name);
            k.begin(&scroll);
            let spots: Vec<(f64, f64)> = (0..n)
                .map(|_| (b.x + rng.unit() * b.width, b.y + rng.unit() * b.height))
                .collect();
            let copies: &[f64] = if kind.fall() == 0.0 {
                &[0.0]
            } else {
                &[0.0, -kind.fall()]
            };
            let seed = rng.next_u64();
            for (c, offset) in copies.iter().enumerate() {
                let mut same = Rng::new(seed);
                for (i, &(x, y)) in spots.iter().enumerate() {
                    let name = format!("p{}-{}{}", plane + 1, i + 1, ["a", "b"][c]);
                    let spot = Spot {
                        kind,
                        name: &name,
                        pos: (x, y + offset * b.height),
                        unit: unit * sizes[plane],
                        plane_ms,
                    };
                    particle(k, &mut same, &p, &spot);
                }
            }
            k.end_with(|_| {});
            k.end_with(|g| g.depth = Some(lo + (hi - lo) * plane as f64 / 2.0));
            if kind.fall() != 0.0 {
                let to = kind.fall() * b.height;
                play_part(k, &scroll, &loop_ramp(AnimProp::Y, 0.0, to, plane_ms));
            }
        }
    });
    k.fade_root(0, 900);
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "cells_backdrop",
        category: Category::Backgrounds,
        about: "A microscope or biology backdrop: translucent organic cells with membranes and nuclei drifting and turning in depth planes, the far and near ones blurred. Each cell is a group with its own slow loop.",
        tags: &[
            "cells",
            "biology",
            "microscope",
            "organic",
            "blobs",
            "membrane",
            "science",
            "background",
            "scene",
        ],
        params: CELLS,
        place: Place::Frame,
        layer: Layer::Scenery(1),
        build: cells_backdrop,
    },
    Block {
        id: "particles",
        category: Category::Backgrounds,
        about: "Seeded particles in three depth planes: bubbles rising with a wobble, dust motes, confetti falling and turning, snow, embers that flicker, or sparkles that twinkle. At most about 150 nodes; the loop repeats exactly every loop_ms.",
        tags: &[
            "particles",
            "bubbles",
            "dust",
            "confetti",
            "snow",
            "embers",
            "sparkles",
            "ambient",
            "loop",
            "scene",
        ],
        params: PARTICLES,
        place: Place::Frame,
        layer: Layer::Scenery(6),
        build: particles,
    },
];
