//! `paper_grain`: a film-and-paper overlay that sits over everything: a faint tint, fine speckle
//! and a vignette. The speckle is one picture per variant (a generated grain on a transparent
//! ground, named by its recipe) rather than thousands of shapes, and with a loop the variants
//! take turns about twelve times a second so the grain boils like film.

use od_core::{ImageFit, Paint, Stop};

use super::BlockError;
use super::args::{Args, Param};
use super::files::Asset;
use super::kit::{Kit, Style, picture, rect};
use super::registry::{Block, Category, Layer, Place};
use super::scene::{BLACK, Palette, alpha, blend, darken, lighten, palette, solid};
use super::sketchdraw::{cadence, step_track};

const TONE: Param = Param::choice(
    "tone",
    "Specks of the foreground color (light on dark), of the background color darkened (dark on light) or both, like film.",
    "both",
    &["both", "light", "dark"],
);
const GRAIN: Param = Param::count(
    "grain",
    "Size of a speck in the picture, 1 fine to 6 coarse.",
    2,
    1,
    6,
);
const STRENGTH: Param = Param::number(
    "strength",
    "How strongly the grain shows, 0.05 to 1.",
    0.35,
    0.05,
    1.0,
);
const TINT: Param = Param::number(
    "tint",
    "A slight paper tint over the frame (the foreground warmed toward the accent), 0 none to 1.",
    0.25,
    0.0,
    1.0,
);
const VIGNETTE: Param = Param::number(
    "vignette",
    "How much the corners darken, 0 none to 1.",
    0.35,
    0.0,
    1.0,
);
const LOOP_MS: Param = Param::count(
    "loop_ms",
    "Length of the grain's loop in ms: its variants take turns until it starts over. 0 holds one still grain.",
    1_000,
    0,
    8_000,
);
const VARIANTS: Param = Param::count(
    "variants",
    "How many different grains take turns (2 to 4); each is a picture in media/.",
    2,
    2,
    4,
);
const FPS: Param = Param::number(
    "fps",
    "How many times a second the grain changes.",
    12.0,
    4.0,
    24.0,
);
const SEED: Param = Param::count(
    "seed",
    "The same seed always makes the same grain.",
    3,
    0,
    100_000,
);

const PAPER: &[Param] = &[
    TONE, GRAIN, STRENGTH, TINT, VIGNETTE, LOOP_MS, VARIANTS, FPS, SEED,
];

fn tint_layer(k: &mut Kit, p: &Palette, tint: f64) {
    let warm = blend(p.fg, p.accent, 0.3);
    k.add(
        "tint",
        rect(k.b, solid(alpha(warm, 1.0))).alpha(0.16 * tint),
    );
}

fn vignette_layer(k: &mut Kit, p: &Palette, amount: f64) {
    let dark = blend(p.bg, BLACK, 0.75);
    let stops = vec![
        Stop {
            offset: 0.55,
            color: Paint::Color(alpha(dark, 0.0)),
        },
        Stop {
            offset: 1.0,
            color: Paint::Color(alpha(dark, 0.6 * amount)),
        },
    ];
    let fill = Paint::Radial {
        cx: 0.5,
        cy: 0.5,
        r: 0.78,
        stops,
    };
    k.add("vignette", rect(k.b, fill));
}

fn specks(k: &mut Kit, color: String, seed: u64, speck: u32, part: &str, opacity: f64) {
    let asset = Asset::grain_overlay(color, seed, k.b.width, k.b.height, speck);
    let file = k.asset(asset);
    k.add(part, picture(k.b, file).fit(ImageFit::Cover).alpha(opacity));
}

/// One grain picture or two (light and dark specks), in a group named `grain-<n>`.
fn grain_variant(k: &mut Kit, a: &Args, pal: &Palette, n: u64) {
    let (seed, speck) = (a.num("seed") as u64 * 8 + n, a.num("grain") as u32);
    let strength = a.num("strength");
    let tone = a.text("tone");
    k.begin(&format!("grain-{}", n + 1));
    if tone != "dark" {
        let color = lighten(pal.fg, 0.4).to_hex();
        specks(
            k,
            color,
            seed * 2,
            speck,
            &format!("specks-light-{}", n + 1),
            strength * 0.8,
        );
    }
    if tone != "light" {
        let base = darken(pal.bg, 0.75).to_hex();
        specks(
            k,
            base,
            seed * 2 + 1,
            speck,
            &format!("specks-dark-{}", n + 1),
            strength,
        );
    }
    k.end_with(|g| g.opacity = (n > 0).then_some(0.0));
}

fn paper_grain(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pal = palette(k);
    let (tint, vignette) = (a.num("tint"), a.num("vignette"));
    if tint > 0.0 {
        tint_layer(k, &pal, tint);
    }
    let loop_ms = a.num("loop_ms") as u64;
    let count = if loop_ms == 0 {
        1
    } else {
        a.num("variants") as u64
    };
    for n in 0..count {
        grain_variant(k, a, &pal, n);
    }
    if vignette > 0.0 {
        vignette_layer(k, &pal, vignette);
    }
    let beat = cadence(k, a.num("fps"), count);
    if loop_ms > 0 {
        let rounds = ((loop_ms as f64 / beat.slot / beat.unit as f64).round() as u64).max(1);
        for n in 0..count {
            step_track(
                k,
                &format!("grain-{}", n + 1),
                n,
                count,
                rounds * beat.unit,
                beat.slot,
                1.0,
            );
        }
    }
    Ok(())
}

pub const BLOCKS: &[Block] = &[Block {
    id: "paper_grain",
    category: Category::Overlays,
    about: "A full-frame paper and film overlay for the very top: a faint paper tint, fine speckle (a generated picture in media/, light and dark specks) and a vignette. With a loop its grains take turns about 12 times a second so the grain boils. Put it last.",
    tags: &[
        "grain",
        "paper",
        "film",
        "noise",
        "texture",
        "vignette",
        "overlay",
        "speckle",
        "tint",
        "hand made",
        "analog",
    ],
    params: PAPER,
    place: Place::Frame,
    layer: Layer::Front,
    build: paper_grain,
}];
