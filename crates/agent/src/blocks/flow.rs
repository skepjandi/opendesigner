//! `flow_strokes`: tapered strokes that follow a seeded vector field across a box: blades of
//! grass, hair, wind, water streaks. Each stroke is a filled ribbon, grouped in a few bands
//! that sway or drift out of step with each other.

use od_core::animation::AnimProp;
use od_core::geometry::Rect;

use super::BlockError;
use super::args::{Args, Param};
use super::fieldgeo::{Field, Taper, follow, ribbon};
use super::kit::Kit;
use super::registry::{Block, Category, Layer, Place};
use super::scene::{Palette, alpha, blend, lighten, palette, period, play_part, solid};
use super::sketchdraw::{Look, ink};
use super::sketchgeo::{P, Sub, stream};
use crate::scene_motion::{Rng, loop_sine};

const BANDS: usize = 5;
const STEPS: usize = 16;

const STYLE: Param = Param::choice(
    "style",
    "grass (blades up from the bottom of the box), hair (strands falling from the top), wind (long thin lines), water (short horizontal streaks) or flow (strokes everywhere along the field).",
    "grass",
    &["grass", "hair", "wind", "water", "flow"],
);
const COUNT: Param = Param::count("count", "How many strokes.", 70, 4, 140);
const LENGTH: Param = Param::number(
    "length",
    "Stroke length as a percent of the box's shorter side (wind and water scale it for their style).",
    28.0,
    3.0,
    90.0,
);
const WIDTH: Param = Param::number(
    "width",
    "Stroke width at its widest as a percent of the frame's shorter side.",
    1.0,
    0.05,
    4.0,
);
const LEAN: Param = Param::number(
    "lean",
    "Turns the whole field, in degrees: wind from the left or right for grass, the heading for flow.",
    0.0,
    -90.0,
    90.0,
);
const SWIRL: Param = Param::number(
    "swirl",
    "How much the field bends the strokes: 0 straight, 1 natural, 3 a whirl.",
    1.0,
    0.0,
    3.0,
);
const CURL: Param = Param::number(
    "curl",
    "How much each stroke curls toward its tip, in radians.",
    0.4,
    -2.0,
    2.0,
);
const COLOR: Param = Param::choice(
    "color",
    "Palette role at one end of the color range.",
    "accent",
    &["foreground", "accent", "muted", "background"],
);
const COLOR2: Param = Param::choice(
    "color2",
    "Palette role at the other end; each stroke is somewhere between.",
    "muted",
    &["foreground", "accent", "muted", "background"],
);
const SWAY: Param = Param::number(
    "sway",
    "How far the bands of strokes sway (grass, hair) or drift (the rest) in their loop, 0 for none.",
    0.5,
    0.0,
    1.0,
);
const SWAY_MS: Param = Param::count(
    "sway_ms",
    "Length of the sway loop in ms (rounded to whole seconds; bands run at slightly different speeds).",
    5_000,
    2_000,
    20_000,
);
const SEED: Param = Param::count(
    "seed",
    "The same seed always draws the same strokes.",
    5,
    0,
    100_000,
);

const FLOW: &[Param] = &[
    STYLE, COUNT, LENGTH, WIDTH, LEAN, SWIRL, CURL, COLOR, COLOR2, SWAY, SWAY_MS, SEED,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Style {
    Grass,
    Hair,
    Wind,
    Water,
    Flow,
}

impl Style {
    fn parse(name: &str) -> Style {
        match name {
            "hair" => Style::Hair,
            "wind" => Style::Wind,
            "water" => Style::Water,
            "flow" => Style::Flow,
            _ => Style::Grass,
        }
    }

    fn taper(self) -> Taper {
        match self {
            Style::Grass => Taper::Blade,
            Style::Hair => Taper::Hair,
            _ => Taper::Lens,
        }
    }

    /// The heading, in degrees, where the field is neutral.
    fn heading(self, lean: f64) -> f64 {
        lean + match self {
            Style::Grass => -90.0,
            Style::Hair => 90.0,
            _ => 0.0,
        }
    }

    fn reach(self) -> f64 {
        match self {
            Style::Wind => 2.2,
            Style::Water => 0.7,
            _ => 1.0,
        }
    }

    fn thin(self) -> f64 {
        match self {
            Style::Wind => 0.6,
            Style::Water => 0.8,
            _ => 1.0,
        }
    }

    fn faint(self) -> Option<f64> {
        matches!(self, Style::Wind | Style::Water).then_some(0.55)
    }

    /// Grass and hair grow in tufts: strokes start near a few shared places.
    fn tufted(self) -> bool {
        matches!(self, Style::Grass | Style::Hair)
    }

    fn start(self, b: Rect, x: f64, rng: &mut Rng) -> P {
        match self {
            Style::Grass => (x, b.y + b.height - rng.range(0.0, 0.03) * b.height),
            Style::Hair => (x, b.y + rng.range(0.0, 0.03) * b.height),
            Style::Wind => (
                b.x + rng.range(0.0, 0.6) * b.width,
                rng.range(b.y, b.y + b.height),
            ),
            _ => (x, rng.range(b.y, b.y + b.height)),
        }
    }
}

struct Stroke {
    outline: Vec<P>,
    color: od_core::Rgba,
    band: usize,
}

fn strand_color(pal: &Palette, a: &Args, rng: &mut Rng) -> od_core::Rgba {
    let pick = |name: &str| match name {
        "foreground" => pal.fg,
        "muted" => pal.muted,
        "background" => pal.bg,
        _ => pal.accent,
    };
    let mixed = blend(pick(a.text("color")), pick(a.text("color2")), rng.unit());
    lighten(mixed, rng.range(0.0, 0.18))
}

fn strokes(k: &Kit, a: &Args, pal: &Palette, style: Style) -> Vec<Stroke> {
    let seed = a.num("seed") as u64;
    let b = k.b;
    let field = Field::new(
        &mut stream(seed, 1),
        style.heading(a.num("lean")),
        a.num("swirl") * if style == Style::Water { 0.35 } else { 1.0 },
        b.width.min(b.height) / 3.0,
    );
    let reach = b.width.min(b.height) * a.num("length") / 100.0 * style.reach();
    let width = k.s(a.num("width")) * style.thin();
    let count = a.num("count") as usize;
    let inside = |p: P| p.0 >= b.x && p.0 <= b.x + b.width && p.1 >= b.y && p.1 <= b.y + b.height;
    let mut tufts = stream(seed, 2);
    let centers: Vec<f64> = (0..(count / 7).max(2))
        .map(|_| tufts.range(b.x, b.x + b.width))
        .collect();
    let mut out = Vec::new();
    for i in 0..count {
        let mut rng = stream(seed, 100 + i as u64);
        let x = if style.tufted() {
            let c = centers[i % centers.len()];
            (c + (rng.unit() + rng.unit() - 1.0) * b.width * 0.05).clamp(b.x, b.x + b.width)
        } else {
            rng.range(b.x, b.x + b.width)
        };
        let start = style.start(b, x, &mut rng);
        let len = reach * rng.range(0.55, 1.0);
        let tilt = if style.tufted() {
            rng.range(-0.3, 0.3)
        } else {
            0.0
        };
        let line = follow(
            &field,
            start,
            STEPS,
            (len / STEPS as f64, tilt, a.num("curl")),
            inside,
        );
        let outline = ribbon(&line, width * rng.range(0.75, 1.25), style.taper());
        if outline.len() < 6 {
            continue;
        }
        let across = ((start.0 - b.x) / b.width.max(1.0)).clamp(0.0, 0.999);
        let band = match style {
            Style::Grass | Style::Hair => (across * BANDS as f64) as usize,
            _ => i % BANDS,
        };
        out.push(Stroke {
            outline,
            color: strand_color(pal, a, &mut rng),
            band,
        });
    }
    out
}

fn sway(k: &mut Kit, a: &Args, style: Style, band: usize) {
    let amount = a.num("sway");
    if amount <= 0.0 {
        return;
    }
    let name = format!("band-{}", band + 1);
    let factor = 0.8 + 0.1 * band as f64;
    let ms = period(a.num("sway_ms") as u64, factor);
    let phase = band as f64 / BANDS as f64;
    let track = match style {
        Style::Grass | Style::Hair => loop_sine(AnimProp::Rotation, 0.0, 3.0 * amount, ms, phase),
        _ => loop_sine(AnimProp::X, 0.0, k.s(1.4) * amount, ms, phase),
    };
    play_part(k, &name, &track);
}

fn flow_strokes(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pal = palette(k);
    let style = Style::parse(a.text("style"));
    let all = strokes(k, a, &pal, style);
    let pivot = match style {
        Style::Grass => Some(1.0),
        Style::Hair => Some(0.0),
        _ => None,
    };
    for band in (0..BANDS).filter(|b| all.iter().any(|s| s.band == *b)) {
        k.begin(&format!("band-{}", band + 1));
        for (i, s) in all.iter().enumerate().filter(|(_, s)| s.band == band) {
            let look = Look::filled(solid(alpha(s.color, 1.0)));
            let look = Look {
                opacity: style.faint(),
                ..look
            };
            ink(
                k,
                &format!("stroke-{}", i + 1),
                &[Sub::Closed(s.outline.clone())],
                look,
            );
        }
        k.end_with(|g| g.pivot_y = pivot);
        sway(k, a, style, band);
    }
    Ok(())
}

pub const BLOCKS: &[Block] = &[Block {
    id: "flow_strokes",
    category: Category::Backgrounds,
    about: "Curved, tapered strokes that follow a seeded vector field across the box: grass, hair, wind lines or water streaks, each a filled ribbon, in bands that sway or drift in a loop. Deterministic by seed; at most 145 nodes.",
    tags: &[
        "grass",
        "hair",
        "wind",
        "water",
        "streaks",
        "flow",
        "field",
        "strokes",
        "texture",
        "hand made",
        "brush",
        "illustration",
    ],
    params: FLOW,
    place: Place::Safe,
    layer: Layer::Back,
    build: flow_strokes,
}];
