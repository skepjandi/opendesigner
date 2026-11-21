//! `watercolor_wash`: a painted patch of color. Many translucent copies of one outline, each
//! deformed a little differently, pile up where they overlap (so the middle is deeper and the
//! rim irregular); a blur softens the edge, a darker band pools along it, and a faint, wider
//! halo bleeds into the paper.

use od_core::animation::AnimProp;
use od_core::{Ease, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::fieldgeo::deform;
use super::kit::Kit;
use super::registry::{Block, Category, Layer, Place};
use super::scene::{Palette, alpha, blend, darken, lighten, palette, solid};
use super::sketchdraw::{Look, SEED, SHAPE, TARGETS, ellipse_of, ink, subjects};
use super::sketchgeo::{P, Ring, Sub, bounds, resample, stream};
use crate::scene_motion::Rng;

const COLOR: Param = Param::choice(
    "color",
    "Which palette role the paint is mixed from.",
    "accent",
    &["foreground", "accent", "muted", "background"],
);
const LAYERS: Param = Param::count(
    "layers",
    "How many translucent copies of the outline are stacked.",
    9,
    6,
    12,
);
const BLEED: Param = Param::number(
    "bleed",
    "How far a faint halo of pigment spreads past the edge into the paper, 0 to 1.",
    0.5,
    0.0,
    1.0,
);
const SOFTNESS: Param = Param::number(
    "softness",
    "How soft the edge is (a blur), 0 hard to 1 very soft.",
    0.5,
    0.0,
    1.0,
);
const VARIATION: Param = Param::number(
    "variation",
    "How much the pigment varies from one layer to the next in lightness and hue, 0 to 1.",
    0.5,
    0.0,
    1.0,
);
const EDGE: Param = Param::number(
    "edge",
    "How dark the band of pooled pigment along the edge is, 0 for none to 1 for strong.",
    0.6,
    0.0,
    1.0,
);
const DENSITY: Param = Param::number(
    "density",
    "How much paint the stack lays down in the middle, 0 thin to 1 almost opaque.",
    0.6,
    0.0,
    1.0,
);
const DEFORM: Param = Param::number(
    "deform",
    "How irregular the edge is: 0.3 gently uneven, 1 a real blot, 2 wild.",
    1.0,
    0.2,
    2.0,
);
const BLOOM: Param = Param::flag(
    "bloom",
    "The wash blooms in: it fades up and swells a little as the paint spreads.",
    true,
);
const DURATION: Param = Param::count(
    "duration_ms",
    "How long the bloom takes.",
    1_400,
    300,
    8_000,
);

const WASH: &[Param] = &[
    TARGETS, SHAPE, COLOR, LAYERS, BLEED, SOFTNESS, VARIATION, EDGE, DENSITY, DEFORM, BLOOM,
    DURATION, SEED,
];

fn pigment(p: &Palette, role: &str) -> Rgba {
    match role {
        "foreground" => p.fg,
        "muted" => p.muted,
        "background" => p.bg,
        _ => p.accent,
    }
}

/// The paint of one layer: the base nudged lighter or darker and toward the muted tone.
fn layer_color(base: Rgba, p: &Palette, rng: &mut Rng, variation: f64) -> Rgba {
    let shade = rng.range(-1.0, 1.0) * variation * 0.14;
    let tinted = blend(base, p.muted, rng.range(0.0, 0.16) * variation);
    if shade >= 0.0 {
        darken(tinted, shade)
    } else {
        lighten(tinted, -shade)
    }
}

/// The alpha each of `n` layers needs so the stack's middle reaches `coverage`.
fn layer_alpha(n: usize, coverage: f64) -> f64 {
    1.0 - (1.0 - coverage).powf(1.0 / n as f64)
}

fn grown(poly: &[P], about: P, by: f64) -> Vec<P> {
    poly.iter()
        .map(|p| {
            (
                about.0 + (p.0 - about.0) * by,
                about.1 + (p.1 - about.1) * by,
            )
        })
        .collect()
}

struct Brush {
    base: Rgba,
    palette: Palette,
    layers: usize,
    variation: f64,
    deform: f64,
    seed: u64,
    coverage: f64,
}

/// The outline of each ring of a shape, deformed once as a master shape every layer starts from.
fn masters(rings: &[Ring], b: &Brush, shape: u64) -> Vec<Vec<P>> {
    rings
        .iter()
        .enumerate()
        .map(|(i, ring)| {
            let mut rng = stream(b.seed, 5_000 + shape * 64 + i as u64);
            deform(&resample(ring, 10), 3, 0.16 * b.deform, &mut rng)
        })
        .collect()
}

fn layer_outline(masters: &[Vec<P>], b: &Brush, shape: u64, j: usize) -> Vec<Vec<P>> {
    masters
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let mut rng = stream(b.seed, 9_000 + shape * 4_096 + (j * 64 + i) as u64);
            deform(m, 3, 0.13 * b.deform, &mut rng)
        })
        .collect()
}

/// Uneven pools of paint inside the shape: smaller copies, some deeper and some paler, so the
/// middle is mottled rather than flat.
fn inner_pools(k: &mut Kit, b: &Brush, masters: &[Vec<P>], shape: u64) {
    let mid = masters
        .iter()
        .flatten()
        .fold((0.0_f64, 0.0_f64, 0.0_f64), |(x, y, n), p| {
            (x + p.0, y + p.1, n + 1.0)
        });
    let about = (mid.0 / mid.2.max(1.0), mid.1 / mid.2.max(1.0));
    for j in 0..3 {
        let mut rng = stream(b.seed, 7_000 + shape * 64 + j);
        let polys: Vec<Sub> = masters
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let mut r = stream(b.seed, 7_500 + shape * 64 + (j * 8 + i as u64));
                Sub::Closed(deform(
                    &grown(m, about, rng.range(0.4, 0.85)),
                    3,
                    0.14 * b.deform,
                    &mut r,
                ))
            })
            .collect();
        let tone = lighten(b.base, 0.2 + 0.1 * j as f64);
        ink(
            k,
            &format!("w{shape}-pool-{}", j + 1),
            &polys,
            Look::filled(solid(alpha(tone, 1.0))).alpha(0.08 + 0.05 * rng.unit()),
        );
    }
}

fn paint_shape(k: &mut Kit, b: &Brush, a: &Args, rings: &[Ring], shape: u64) -> Vec<Vec<Vec<P>>> {
    let masters = masters(rings, b, shape);
    let each = layer_alpha(b.layers, b.coverage);
    let halo = a.num("bleed");
    let mut outlines = Vec::new();
    if halo > 0.0 {
        let mid = bounds(rings).map_or((0.0, 0.0), |(lo, hi)| {
            ((lo.0 + hi.0) / 2.0, (lo.1 + hi.1) / 2.0)
        });
        let subs: Vec<Sub> = masters
            .iter()
            .map(|m| Sub::Closed(grown(m, mid, 1.0 + 0.1 * halo)))
            .collect();
        let look = Look::filled(solid(alpha(b.base, 1.0))).alpha(0.07 + 0.1 * halo);
        ink(
            k,
            &format!("w{shape}-bleed"),
            &subs,
            Look {
                blur: Some(k.s(1.6) * halo + 2.0),
                ..look
            },
        );
    }
    inner_pools(k, b, &masters, shape);
    for j in 0..b.layers {
        let mut rng = stream(b.seed, 3_000 + shape * 64 + j as u64);
        let color = layer_color(b.base, &b.palette, &mut rng, b.variation);
        let polys = layer_outline(&masters, b, shape, j);
        let subs: Vec<Sub> = polys.iter().cloned().map(Sub::Closed).collect();
        let strength = each * rng.range(0.6, 1.4);
        ink(
            k,
            &format!("w{shape}-layer-{}", j + 1),
            &subs,
            Look::filled(solid(alpha(color, 1.0))).alpha(strength),
        );
        outlines.push(polys);
    }
    outlines
}

/// The darker band where pigment pools: strokes along two of the layers' outlines.
fn pool_edge(k: &mut Kit, b: &Brush, edge: f64, outlines: &[Vec<Vec<P>>], shape: u64) {
    let ink_color = darken(b.base, 0.3 + 0.2 * edge);
    for (i, polys) in outlines.iter().rev().take(4).enumerate() {
        let subs: Vec<Sub> = polys.iter().cloned().map(Sub::Closed).collect();
        let look =
            Look::line(solid(alpha(ink_color, 1.0)), k.s(0.55 + 0.3 * i as f64)).alpha(0.24 * edge);
        ink(k, &format!("w{shape}-edge-{}", i + 1), &subs, look);
    }
}

fn watercolor_wash(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pal = palette(k);
    let b = Brush {
        base: pigment(&pal, a.text("color")),
        palette: pal,
        layers: a.num("layers") as usize,
        variation: a.num("variation"),
        deform: a.num("deform"),
        seed: a.num("seed") as u64,
        coverage: 0.16 + 0.34 * a.num("density"),
    };
    let shapes = subjects(k, a, ellipse_of)?;
    let (soft, edge) = (a.num("softness"), a.num("edge"));
    let mut edges = Vec::new();
    k.begin("wash");
    k.begin("paint");
    for (i, rings) in shapes.iter().enumerate() {
        edges.push(paint_shape(k, &b, a, rings, i as u64 + 1));
    }
    let paint_blur = Some(k.s(1.3) * soft).filter(|v| *v > 0.0);
    k.end_with(|g| g.blur = paint_blur);
    if edge > 0.0 {
        k.begin("pool");
        for (i, outlines) in edges.iter().enumerate() {
            pool_edge(k, &b, edge, outlines, i as u64 + 1);
        }
        let pool_blur = k.s(0.9) * soft + 0.5;
        k.end_with(|g| g.blur = Some(pool_blur));
    }
    k.end();
    let (bloom, ms) = (a.flag("bloom"), a.num("duration_ms") as u64);
    if bloom {
        let wash = k.part("wash");
        k.keys(
            wash.clone(),
            AnimProp::Opacity,
            &[(0, 0.0, Ease::Linear), (ms, 1.0, Ease::EaseOut)],
        );
        k.keys(
            wash,
            AnimProp::Scale,
            &[(0, 0.94, Ease::Linear), (ms, 1.0, Ease::EaseOut)],
        );
    }
    Ok(())
}

pub const BLOCKS: &[Block] = &[Block {
    id: "watercolor_wash",
    category: Category::Backgrounds,
    about: "A painted wash of color: 6 to 12 stacked translucent copies of an outline, each deformed differently, softened by a blur, with a darker pooled edge and a faint bleed into the paper. For a shape, a node or the box. Blooms in.",
    tags: &[
        "watercolor",
        "watercolour",
        "wash",
        "paint",
        "painted",
        "paper",
        "hand made",
        "blot",
        "stain",
        "illustration",
        "background",
    ],
    params: WASH,
    place: Place::Safe,
    layer: Layer::Back,
    build: watercolor_wash,
}];
