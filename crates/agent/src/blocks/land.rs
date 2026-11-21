//! The ground of a scene: layered hills or mountains that follow the camera by their depth,
//! scrolling ocean waves, a skyline whose windows blink, and a perspective grid floor. Layers are
//! drawn far to near with falling haze and rising contrast; everything that moves does so in a loop
//! that ends where it began.

use od_core::animation::AnimProp;
use od_core::geometry::Rect;
use od_core::{LineJoin, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{Kit, Style, at, ellipse, rect};
use super::registry::{Block, Category, Layer, Place};
use super::scene::{
    Palette, alpha, blend, darken, desaturate, lap, lighten, linear, palette, path_node, period,
    play_part, polyline, radial, rect_sub, slowness, smooth, soft_shadow, solid, windowed,
};
use crate::scene_motion::{Rng, loop_keys, loop_ramp, loop_sine};

const SEED: Param = Param::count(
    "seed",
    "The same seed always draws the same scene.",
    3,
    0,
    100_000,
);

fn depth_of(lo: f64, hi: f64, t: f64) -> f64 {
    lo + (hi.max(lo) - lo) * t
}

/// How far a layer at `depth` reaches past the box on each side, as a share of its width, so a
/// camera pan never uncovers its edge.
fn overscan(depth: f64) -> f64 {
    (depth * 0.25).clamp(0.03, 0.45)
}

fn share(i: usize, n: usize) -> f64 {
    if n <= 1 {
        1.0
    } else {
        i as f64 / (n - 1) as f64
    }
}

const HILLS: &[Param] = &[
    Param::count("layers", "Layers of ridge, far to near.", 4, 3, 5),
    Param::choice(
        "style",
        "hills (smooth) or mountains (peaks with softened tips).",
        "hills",
        &["hills", "mountains"],
    ),
    Param::number(
        "height",
        "How far up the box the farthest ridge reaches, in percent of its height.",
        48.0,
        10.0,
        90.0,
    ),
    Param::number("roughness", "How uneven the ridges are.", 0.5, 0.1, 1.0),
    Param::number(
        "min_depth",
        "Camera parallax of the farthest layer: 0 stays on the screen, 1 moves with the scene.",
        0.2,
        0.0,
        3.0,
    ),
    Param::number(
        "max_depth",
        "Camera parallax of the nearest layer.",
        1.0,
        0.0,
        3.0,
    ),
    Param::number(
        "haze",
        "How much the far layers fade into the sky.",
        0.6,
        0.0,
        1.0,
    ),
    Param::flag(
        "shadow",
        "A soft shadow each layer casts on the one behind it.",
        true,
    ),
    Param::flag(
        "sway",
        "A slow, gentle drift of each layer, out of step with the others.",
        true,
    ),
    Param::count(
        "loop_ms",
        "Period of the sway; each layer's own is a fraction of it.",
        16_000,
        4_000,
        120_000,
    ),
    SEED,
];

/// The ridge of layer `t` (0 far, 1 near): points across `width`, as offsets from the ridge line.
fn ridge(rng: &mut Rng, width: f64, amp: f64, t: f64, peaks: bool, rough: f64) -> Vec<(f64, f64)> {
    let count = ((width / (amp * if peaks { 2.2 } else { 3.4 })).round() as usize).clamp(6, 40);
    let (f1, f2) = (rng.range(0.7, 1.4) + t * 0.6, rng.range(1.8, 3.2));
    let (p1, p2) = (
        rng.unit() * std::f64::consts::TAU,
        rng.unit() * std::f64::consts::TAU,
    );
    (0..=count)
        .map(|j| {
            let u = j as f64 / count as f64;
            let x = width * u;
            if peaks {
                let peak = j % 2 == 1;
                let step = width / count as f64;
                let tall = if rng.chance(0.2) { 1.5 } else { 1.0 };
                let y = if peak {
                    -amp * rng.range(0.45, 1.1) * tall * rough.max(0.3) * 1.2
                } else {
                    amp * 0.45 * rng.range(0.1, 1.0)
                };
                let edge = j == 0 || j == count;
                return (
                    x + if edge {
                        0.0
                    } else {
                        rng.range(-0.4, 0.4) * step
                    },
                    y,
                );
            }
            let wave = 0.6 * (std::f64::consts::TAU * f1 * u + p1).sin()
                + 0.3 * (std::f64::consts::TAU * f2 * u + p2).sin()
                + 0.25 * rough * rng.range(-1.0, 1.0);
            (x, wave * amp)
        })
        .collect()
}

fn layer_colors(p: &Palette, t: f64, haze: f64) -> (Rgba, Rgba) {
    let far = blend(p.accent, p.bg, 0.3);
    let near = darken(blend(p.bg, p.accent, 0.3), 0.5);
    let horizon = blend(p.accent, p.fg, 0.35);
    let mut base = blend(far, near, t.powf(0.85));
    base = desaturate(base, (1.0 - t) * 0.3 * haze);
    base = blend(base, horizon, haze * (1.0 - t).powf(1.3));
    (lighten(base, 0.1), darken(base, 0.14))
}

fn hills(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let mut rng = Rng::new(a.num("seed") as u64);
    let layers = a.num("layers") as usize;
    let (lo, hi, haze) = (a.num("min_depth"), a.num("max_depth"), a.num("haze"));
    let peaks = a.text("style") == "mountains";
    let b = k.b;
    windowed(k, |k| {
        for i in 0..layers {
            let t = share(i, layers);
            let depth = depth_of(lo, hi, t);
            let o = overscan(depth);
            let reach = b.height * a.num("height") / 100.0 * (1.0 - 0.62 * t);
            let amp = b.height * (0.05 + 0.05 * (1.0 - t)) * (0.5 + a.num("roughness"));
            let top_y = b.y + b.height - reach - amp;
            let width = b.width * (1.0 + 2.0 * o);
            let rel = ridge(&mut rng, width, amp, t, peaks, a.num("roughness"));
            let line_y = amp * 1.3;
            let mut pts: Vec<(f64, f64)> = rel.iter().map(|&(x, y)| (x, y + line_y)).collect();
            let height = (b.y + b.height - top_y) + amp * 0.5;
            let d = if peaks {
                let mut d = polyline(&pts, false);
                d.push_str(&format!(
                    "L{},{}L0,{}Z",
                    super::scene::num(width),
                    super::scene::num(height),
                    super::scene::num(height)
                ));
                d
            } else {
                let mut d = smooth(&pts, false);
                d.push_str(&format!(
                    "L{},{}L0,{}Z",
                    super::scene::num(width),
                    super::scene::num(height),
                    super::scene::num(height)
                ));
                d
            };
            pts.clear();
            let (top, bottom) = layer_colors(&p, t, haze);
            let mut node = path_node(
                d,
                at(b.x - o * b.width, top_y, width, height),
                Some(linear(90.0, &[(0.0, top), (1.0, bottom)])),
            );
            if peaks {
                node = node.outline(solid(top), k.s(0.45));
                if let Some(s) = node.stroke.as_mut() {
                    s.join = LineJoin::Round;
                }
            }
            if a.flag("shadow") {
                node.shadow = Some(soft_shadow(
                    alpha(darken(bottom, 0.5), 0.4),
                    -b.height * 0.012,
                    b.height * 0.035,
                ));
            }
            let (layer, ridge_part) = (format!("layer-{}", i + 1), format!("ridge-{}", i + 1));
            k.begin(&layer);
            k.add(&ridge_part, node);
            if haze > 0.05 && t < 1.0 {
                let fog = blend(p.accent, p.fg, 0.35);
                let strength = 0.3 * haze * (1.0 - t);
                let mist = rect(
                    at(b.x - o * b.width, top_y + amp * 0.5, width, reach * 0.55),
                    linear(90.0, &[(0.0, alpha(fog, 0.0)), (1.0, alpha(fog, strength))]),
                );
                k.add(&format!("mist-{}", i + 1), mist);
            }
            k.end_with(|g| g.depth = Some(depth));
            if a.flag("sway") {
                let base = a.num("loop_ms") as u64;
                let swing = b.width * 0.004 * (0.5 + t);
                play_part(
                    k,
                    &layer,
                    &loop_sine(
                        AnimProp::X,
                        0.0,
                        swing,
                        period(base, rng.range(0.6, 1.3)),
                        rng.unit(),
                    ),
                );
                play_part(
                    k,
                    &layer,
                    &loop_sine(
                        AnimProp::Y,
                        0.0,
                        b.height * 0.003,
                        period(base, rng.range(0.5, 1.1)),
                        rng.unit(),
                    ),
                );
            }
        }
    });
    k.fade_root(0, 900);
    Ok(())
}

const OCEAN: &[Param] = &[
    Param::count("layers", "Layers of waves, far to near.", 4, 3, 6),
    Param::number(
        "horizon",
        "Where the water starts, in percent from the top of the box.",
        55.0,
        20.0,
        90.0,
    ),
    Param::number(
        "wave_height",
        "Wave height as a percent of the box height.",
        5.0,
        1.0,
        15.0,
    ),
    Param::count(
        "loop_ms",
        "How long the nearest layer takes to scroll one screen width; farther layers are slower. The scroll repeats seamlessly.",
        14_000,
        3_000,
        120_000,
    ),
    Param::choice(
        "direction",
        "Which way the water travels.",
        "left",
        &["left", "right"],
    ),
    Param::flag("foam", "A pale line along every crest.", true),
    SEED,
];

/// A wave across four tiles with `per_tile` whole cycles in each, so the pattern repeats exactly
/// every tile and a scroll of one tile ends where it began.
fn wave_points(rng: &mut Rng, tile: f64, per_tile: usize, amp: f64) -> Vec<(f64, f64)> {
    let (p1, p2) = (rng.unit(), rng.unit());
    let cycles = (per_tile * 4) as f64;
    let count = per_tile * 4 * 10;
    (0..=count)
        .map(|j| {
            let u = j as f64 / count as f64;
            let y = (std::f64::consts::TAU * (cycles * u + p1)).sin()
                + 0.4 * (std::f64::consts::TAU * (2.0 * cycles * u + p2)).sin();
            (tile * 4.0 * u, y * amp)
        })
        .collect()
}

fn ocean(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let mut rng = Rng::new(a.num("seed") as u64);
    let layers = a.num("layers") as usize;
    let b = k.b;
    let tile = b.width;
    let top_water = b.y + b.height * a.num("horizon") / 100.0;
    let span = b.y + b.height - top_water;
    let light = blend(p.accent, p.fg, 0.3);
    let deep = blend(p.accent, p.bg, 0.55);
    let rightward = a.text("direction") == "right";
    windowed(k, |k| {
        for i in 0..layers {
            let t = share(i, layers);
            let cycles = [3, 3, 2, 2, 2, 1][i];
            let amp = b.height * a.num("wave_height") / 100.0 * (0.5 + 0.8 * t);
            let line = top_water + span * (0.05 + 0.6 * t);
            let rel = wave_points(&mut rng, tile, cycles, amp);
            let node_top = line - amp * 1.6;
            let node_h = b.y + b.height + amp - node_top;
            let pts: Vec<(f64, f64)> = rel.iter().map(|&(x, y)| (x, y + amp * 1.6)).collect();
            let mut d = smooth(&pts, false);
            d.push_str(&format!(
                "L{},{}L0,{}Z",
                super::scene::num(tile * 4.0),
                super::scene::num(node_h),
                super::scene::num(node_h)
            ));
            let top = blend(light, deep, t);
            let fill = linear(90.0, &[(0.0, top), (1.0, blend(top, p.bg, 0.55))]);
            let mut node = path_node(d, at(b.x - tile, node_top, tile * 4.0, node_h), Some(fill));
            if a.flag("foam") {
                node = node.outline(solid(alpha(lighten(top, 0.6), 0.55)), k.s(0.2).max(1.2));
            }
            let name = format!("wave-{}", i + 1);
            k.add(&name, node);
            let one_tile = period(a.num("loop_ms") as u64, slowness(i, layers) as f64);
            let (from, to) = if rightward {
                (b.x - 2.0 * tile, b.x - tile)
            } else {
                (b.x - tile, b.x - 2.0 * tile)
            };
            play_part(k, &name, &loop_ramp(AnimProp::X, from, to, one_tile));
            let bob = loop_sine(
                AnimProp::Y,
                node_top,
                b.height * 0.006 * (0.6 + t),
                period(one_tile, [0.5, 0.25][rng.count(0, 1) as usize]),
                rng.unit(),
            );
            play_part(k, &name, &bob);
        }
    });
    k.fade_root(0, 900);
    Ok(())
}

const CITY: &[Param] = &[
    Param::count("layers", "Depth layers of buildings, far to near.", 3, 2, 3),
    Param::count(
        "buildings",
        "Buildings across one screen width of the nearest layer.",
        8,
        4,
        10,
    ),
    Param::number(
        "height",
        "How tall the tallest building is, in percent of the box height.",
        46.0,
        15.0,
        85.0,
    ),
    Param::number(
        "lit",
        "Share of windows that are lit, in percent.",
        55.0,
        5.0,
        100.0,
    ),
    Param::number(
        "min_depth",
        "Camera parallax of the farthest layer.",
        0.3,
        0.0,
        3.0,
    ),
    Param::number(
        "max_depth",
        "Camera parallax of the nearest layer.",
        0.9,
        0.0,
        3.0,
    ),
    Param::count(
        "loop_ms",
        "Base period of the window blinks; each group of windows has its own.",
        9_000,
        3_000,
        60_000,
    ),
    SEED,
];

struct Building {
    x: f64,
    width: f64,
    top: f64,
}

fn city_buildings(rng: &mut Rng, width: f64, avg: f64, tallest: f64, floor: f64) -> Vec<Building> {
    let mut out = Vec::new();
    let mut x = -avg * rng.unit();
    while x < width {
        let w = avg * rng.range(0.7, 1.35);
        let h = tallest * rng.range(0.4, 1.0);
        out.push(Building {
            x,
            width: w,
            top: floor - h,
        });
        x += w + avg * rng.range(0.0, 0.12);
    }
    out
}

fn building_nodes(k: &mut Kit, rng: &mut Rng, c: &Building, floor: f64, base: Rgba, name: &str) {
    let (x0, w, h) = (c.x, c.width, floor - c.top);
    let body = linear(
        90.0,
        &[(0.0, lighten(base, 0.07)), (1.0, darken(base, 0.1))],
    );
    k.add(&format!("{name}-body"), rect(at(x0, c.top, w, h), body));
    k.add(
        &format!("{name}-shade"),
        rect(
            at(x0 + w * 0.7, c.top, w * 0.3, h),
            solid(darken(base, 0.2)),
        ),
    );
    if rng.chance(0.4) {
        let (rw, rh) = (w * 0.55, h * 0.1);
        k.add(
            &format!("{name}-roof"),
            rect(
                at(x0 + w * 0.2, c.top - rh, rw, rh),
                solid(lighten(base, 0.02)),
            ),
        );
    } else if rng.chance(0.3) {
        let (aw, ah) = ((w * 0.04).max(1.5), h * 0.18);
        k.add(
            &format!("{name}-mast"),
            rect(
                at(x0 + w * 0.5, c.top - ah, aw, ah),
                solid(darken(base, 0.1)),
            ),
        );
    }
}

/// Window squares of one building, as subpaths in a layer's three blink groups.
fn building_windows(
    rng: &mut Rng,
    c: &Building,
    floor: f64,
    cell: f64,
    lit: f64,
    groups: &mut [String; 3],
    origin: (f64, f64),
) {
    let (cols, rows) = (
        ((c.width * 0.72) / (cell * 2.0)) as usize,
        (((floor - c.top) * 0.8) / (cell * 2.6)) as usize,
    );
    let (gx, gy) = (
        c.width * 0.72 / cols.max(1) as f64,
        (floor - c.top) * 0.8 / rows.max(1) as f64,
    );
    for r in 0..rows {
        for col in 0..cols {
            if !rng.chance(lit) {
                continue;
            }
            let (x, y) = (
                c.x + c.width * 0.08 + gx * (col as f64 + 0.25) - origin.0,
                c.top + (floor - c.top) * 0.1 + gy * (r as f64 + 0.2) - origin.1,
            );
            groups[rng.count(0, 2) as usize].push_str(&rect_sub(x, y, cell, cell * 1.35));
        }
    }
}

fn blink(period_ms: u64, center: f64, low: f64) -> impl Fn(f64) -> f64 {
    let _ = period_ms;
    move |u: f64| {
        let d = ((u - center + 0.5).rem_euclid(1.0) - 0.5).abs();
        let dip = (1.0 - d / 0.07).clamp(0.0, 1.0);
        1.0 - (1.0 - low) * dip * dip * (3.0 - 2.0 * dip)
    }
}

fn skyline(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let mut rng = Rng::new(a.num("seed") as u64);
    let layers = a.num("layers") as usize;
    let (lo, hi) = (a.num("min_depth"), a.num("max_depth"));
    let b = k.b;
    let floor = b.y + b.height;
    let lit = a.num("lit") / 100.0;
    let glow = blend(
        lighten(p.accent, 0.6),
        Rgba {
            r: 255,
            g: 226,
            b: 160,
            a: 255,
        },
        0.5,
    );
    windowed(k, |k| {
        for i in 0..layers {
            let t = share(i, layers);
            let depth = depth_of(lo, hi, t);
            let o = overscan(depth);
            let width = b.width * (1.0 + 2.0 * o);
            let x0 = b.x - o * b.width;
            let avg = b.width / a.num("buildings") * (0.55 + 0.5 * t);
            let tallest = b.height * a.num("height") / 100.0 * (1.0 - 0.3 * t);
            let base = blend(
                blend(
                    blend(p.accent, p.fg, 0.1),
                    blend(p.accent, p.bg, 0.45),
                    0.6 * t.powf(0.7),
                ),
                darken(blend(p.bg, p.accent, 0.2), 0.3),
                t.powf(1.4),
            );
            let base = desaturate(base, (1.0 - t) * 0.25);
            let homes = city_buildings(&mut rng, width, avg, tallest, floor + b.height * 0.02);
            let layer = format!("skyline-{}", i + 1);
            k.begin(&layer);
            let mut windows = [String::new(), String::new(), String::new()];
            let cell = k.s(0.42) * (0.7 + 0.6 * t);
            for (j, c) in homes.iter().enumerate() {
                let moved = Building {
                    x: c.x + x0,
                    width: c.width,
                    top: c.top,
                };
                building_nodes(
                    k,
                    &mut rng,
                    &moved,
                    floor + b.height * 0.02,
                    base,
                    &format!("{layer}-b{}", j + 1),
                );
                if t >= 0.4 {
                    building_windows(
                        &mut rng,
                        &moved,
                        floor,
                        cell,
                        lit,
                        &mut windows,
                        (x0, floor - tallest * 1.2),
                    );
                }
            }
            let box_top = floor - tallest * 1.2;
            for (g, data) in windows
                .into_iter()
                .enumerate()
                .filter(|(_, d)| !d.is_empty())
            {
                let name = format!("{layer}-windows-{}", g + 1);
                k.add(
                    &name,
                    path_node(
                        data,
                        at(x0, box_top, width, floor - box_top),
                        Some(solid(glow)),
                    ),
                );
                let every = period(a.num("loop_ms") as u64, rng.range(0.7, 1.5)) / 100 * 100;
                let pulse = blink(every, rng.unit(), rng.range(0.1, 0.35));
                play_part(
                    k,
                    &name,
                    &crate::scene_motion::loop_samples(AnimProp::Opacity, every, 24, pulse),
                );
            }
            k.end_with(|g| g.depth = Some(depth));
        }
    });
    k.fade_root(0, 900);
    Ok(())
}

const GRID: &[Param] = &[
    Param::number(
        "horizon",
        "Where the floor meets the sky, in percent from the top of the box.",
        52.0,
        25.0,
        85.0,
    ),
    Param::count(
        "lines",
        "Lines running across the floor, which scroll toward the viewer.",
        12,
        6,
        20,
    ),
    Param::count("columns", "Lines running away from the viewer.", 14, 6, 24),
    Param::count(
        "loop_ms",
        "How long a line takes to travel from the horizon to the viewer; the scroll repeats seamlessly.",
        4_000,
        3_000,
        30_000,
    ),
];

fn fade_curve(u: f64) -> f64 {
    let rise = (u / 0.3).clamp(0.0, 1.0);
    let fall = ((1.0 - u) / 0.04).clamp(0.0, 1.0);
    0.85 * rise * rise * (3.0 - 2.0 * rise) * fall
}

fn rays(b: Rect, horizon: f64, columns: usize) -> String {
    let cx = b.width / 2.0;
    let height = b.height - horizon;
    (0..=columns)
        .map(|c| {
            let far = (c as f64 / columns as f64 - 0.5) * b.width * 3.2;
            let (mut x, mut y) = (cx + far, height);
            if x < 0.0 || x > b.width {
                let edge = if x < 0.0 { 0.0 } else { b.width };
                y = height * (edge - cx) / far;
                x = edge;
            }
            polyline(&[(cx, 0.0), (x, y)], false)
        })
        .collect()
}

fn grid_floor(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let p = palette(k);
    let b = k.b;
    let horizon = b.y + b.height * a.num("horizon") / 100.0;
    let floor = Rect {
        x: b.x,
        y: horizon,
        width: b.width,
        height: b.y + b.height - horizon,
    };
    let line = lighten(p.accent, 0.4);
    k.add(
        "floor",
        rect(
            floor,
            linear(
                90.0,
                &[
                    (0.0, blend(p.bg, p.accent, 0.4)),
                    (0.5, blend(p.bg, p.accent, 0.12)),
                    (1.0, p.bg),
                ],
            ),
        ),
    );
    let (gw, gh) = (b.width, b.height * 0.3);
    k.add(
        "horizon-glow",
        ellipse(
            at(b.x + (b.width - gw) / 2.0, horizon - gh / 2.0, gw, gh),
            radial(
                0.5,
                0.5,
                0.5,
                &[(0.0, alpha(line, 0.55)), (1.0, alpha(line, 0.0))],
            ),
        ),
    );
    let columns = path_node(rays(floor, 0.0, a.num("columns") as usize), floor, None)
        .outline(solid(alpha(line, 0.4)), k.s(0.16).max(1.0));
    k.add("columns", columns);
    k.add(
        "horizon-line",
        rect(
            at(b.x, horizon - k.s(0.1), b.width, k.s(0.2).max(1.5)),
            solid(alpha(line, 0.7)),
        ),
    );
    let n = a.num("lines") as usize;
    let loop_ms = a.num("loop_ms") as u64;
    let thick = k.s(0.2).max(1.4);
    for i in 0..n {
        let first = 0.1 + 0.9 * i as f64 / n as f64;
        let (y_of, h_of) = (
            |u: f64| horizon + floor.height * u * u,
            |u: f64| thick * (0.35 + 1.65 * u),
        );
        let name = format!("row-{}", i + 1);
        k.add(
            &name,
            rect(at(b.x, y_of(first), b.width, h_of(first)), solid(line)).alpha(fade_curve(first)),
        );
        let path = lap(first, loop_ms);
        let series =
            |f: &dyn Fn(f64) -> f64| path.iter().map(|&(t, u)| (t, f(u))).collect::<Vec<_>>();
        play_part(k, &name, &loop_keys(AnimProp::Y, &series(&|u| y_of(u))));
        play_part(
            k,
            &name,
            &loop_keys(AnimProp::Height, &series(&|u| h_of(u))),
        );
        play_part(
            k,
            &name,
            &loop_keys(AnimProp::Opacity, &series(&fade_curve)),
        );
    }
    k.fade_root(0, 900);
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "parallax_hills",
        category: Category::Backgrounds,
        about: "Three to five layered hill or mountain silhouettes made of path nodes, far to near, with falling haze and rising contrast. Each layer is a group with its own `depth`, so a camera move (camera_pan, camera_push_in, camera_drift) parallaxes them; an optional slow sway keeps them alive.",
        tags: &[
            "hills",
            "mountains",
            "landscape",
            "parallax",
            "layers",
            "silhouette",
            "scene",
            "background",
            "depth",
        ],
        params: HILLS,
        place: Place::Frame,
        layer: Layer::Scenery(4),
        build: hills,
    },
    Block {
        id: "ocean_waves",
        category: Category::Backgrounds,
        about: "Layered wave paths that scroll horizontally in seamless loops at different speeds, with foam on the crests and a slow bob; far layers are lighter.",
        tags: &[
            "ocean",
            "sea",
            "waves",
            "water",
            "scroll",
            "loop",
            "scene",
            "background",
        ],
        params: OCEAN,
        place: Place::Frame,
        layer: Layer::Scenery(5),
        build: ocean,
    },
    Block {
        id: "city_skyline",
        category: Category::Backgrounds,
        about: "Two or three layers of flat-shaded buildings, far to near, whose lit windows blink out of step; each layer has its own camera depth.",
        tags: &[
            "city",
            "skyline",
            "buildings",
            "urban",
            "windows",
            "night",
            "scene",
            "background",
            "parallax",
        ],
        params: CITY,
        place: Place::Frame,
        layer: Layer::Scenery(5),
        build: skyline,
    },
    Block {
        id: "grid_floor",
        category: Category::Backgrounds,
        about: "A perspective-faked ground grid with a glowing horizon: lines converge toward a vanishing point and rows scroll toward the viewer in a seamless loop. A tech or synthwave floor.",
        tags: &[
            "grid",
            "floor",
            "perspective",
            "tech",
            "synthwave",
            "retro",
            "scroll",
            "scene",
            "background",
        ],
        params: GRID,
        place: Place::Frame,
        layer: Layer::Scenery(5),
        build: grid_floor,
    },
];
