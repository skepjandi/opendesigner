//! Backgrounds, drawn by the procedural generators in `crate::assets::generate` and placed
//! behind everything else in the frame. The colors are the palette's, baked into the picture.

use super::BlockError;
use super::args::{Args, Param};
use super::files::Asset;
use super::kit::{Kit, Role, Style, mix, picture};
use super::registry::{Block, Category, Layer, Place};
use od_core::ImageFit;

const SEED: Param = Param::count(
    "seed",
    "The same seed always draws the same picture.",
    3,
    0,
    100_000,
);

fn place_backdrop(k: &mut Kit, asset: Asset) {
    let file = k.asset(asset);
    let b = k.b;
    k.add("picture", picture(b, file).fit(ImageFit::Cover));
    k.fade_root(0, 700);
}

fn rgba(k: &mut Kit, role: Role) -> od_core::Rgba {
    let p = k.paint(role);
    k.rgba(&p)
}

const GRADIENT: &[Param] = &[
    Param::choice(
        "style",
        "linear (at an angle from the seed) or radial.",
        "linear",
        &["linear", "radial"],
    ),
    Param::number(
        "strength",
        "How far the wash travels from the background toward the accent.",
        0.6,
        0.1,
        1.0,
    ),
    Param::count("stops", "Two colors, or three with a midpoint.", 3, 2, 3),
    SEED,
];

fn gradient_wash(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let (bg, accent) = (rgba(k, Role::Background), rgba(k, Role::Accent));
    let end = mix(bg, accent, a.num("strength"));
    let mut colors = vec![bg.to_hex()];
    if a.num("stops") >= 3.0 {
        colors.push(mix(bg, accent, a.num("strength") * 0.45));
    }
    colors.push(end);
    let variant = (a.text("style") == "radial").then_some("radial");
    place_backdrop(
        k,
        Asset::backdrop(
            "gradient",
            variant,
            colors,
            a.num("seed") as u64,
            k.b.width,
            k.b.height,
            None,
        ),
    );
    Ok(())
}

const AURORA: &[Param] = &[SEED];

fn aurora_backdrop(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let (bg, accent, fg) = (
        rgba(k, Role::Background),
        rgba(k, Role::Accent),
        rgba(k, Role::Foreground),
    );
    let colors = vec![
        bg.to_hex(),
        accent.to_hex(),
        mix(accent, bg, 0.5),
        mix(accent, fg, 0.35),
    ];
    place_backdrop(
        k,
        Asset::backdrop(
            "aurora",
            None,
            colors,
            a.num("seed") as u64,
            k.b.width,
            k.b.height,
            None,
        ),
    );
    Ok(())
}

const GRID: &[Param] = &[
    Param::number(
        "cell",
        "Grid cell size, as a percent of the frame's shorter side.",
        6.0,
        3.0,
        20.0,
    ),
    Param::number(
        "contrast",
        "How visible the lines are against the background.",
        0.14,
        0.04,
        0.5,
    ),
];

fn grid_backdrop(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let (bg, muted) = (rgba(k, Role::Background), rgba(k, Role::Muted));
    let colors = vec![bg.to_hex(), mix(bg, muted, a.num("contrast"))];
    let cell = k.s(a.num("cell")).round().max(8.0) as u32;
    place_backdrop(
        k,
        Asset::backdrop(
            "pattern",
            Some("grid"),
            colors,
            0,
            k.b.width,
            k.b.height,
            Some(cell),
        ),
    );
    Ok(())
}

const NOISE: &[Param] = &[
    Param::count(
        "grain",
        "Grain size in pixels at the frame's size.",
        2,
        1,
        8,
    ),
    SEED,
];

fn noise_backdrop(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let (bg, muted) = (rgba(k, Role::Background), rgba(k, Role::Muted));
    let colors = vec![bg.to_hex(), mix(bg, muted, 0.5)];
    place_backdrop(
        k,
        Asset::backdrop(
            "grain",
            None,
            colors,
            a.num("seed") as u64,
            k.b.width,
            k.b.height,
            Some(a.num("grain") as u32),
        ),
    );
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "gradient_wash",
        category: Category::Backgrounds,
        about: "A smooth color wash from the background color toward the accent, linear or radial, behind everything.",
        tags: &[
            "gradient",
            "background",
            "wash",
            "backdrop",
            "color",
            "ramp",
        ],
        params: GRADIENT,
        place: Place::Frame,
        layer: Layer::Back,
        build: gradient_wash,
    },
    Block {
        id: "aurora_backdrop",
        category: Category::Backgrounds,
        about: "Soft, blurred clouds of the accent color on the background color: a modern hero backdrop.",
        tags: &[
            "aurora",
            "background",
            "backdrop",
            "glow",
            "blur",
            "mesh",
            "hero",
        ],
        params: AURORA,
        place: Place::Frame,
        layer: Layer::Back,
        build: aurora_backdrop,
    },
    Block {
        id: "grid_backdrop",
        category: Category::Backgrounds,
        about: "A fine grid of lines on the background color, for technical and developer-facing scenes.",
        tags: &[
            "grid",
            "background",
            "backdrop",
            "lines",
            "pattern",
            "technical",
            "blueprint",
        ],
        params: GRID,
        place: Place::Frame,
        layer: Layer::Back,
        build: grid_backdrop,
    },
    Block {
        id: "noise_backdrop",
        category: Category::Backgrounds,
        about: "Fine film grain on the background color: texture that keeps flat colors from looking digital.",
        tags: &[
            "noise",
            "grain",
            "texture",
            "background",
            "backdrop",
            "film",
        ],
        params: NOISE,
        place: Place::Frame,
        layer: Layer::Back,
        build: noise_backdrop,
    },
];
