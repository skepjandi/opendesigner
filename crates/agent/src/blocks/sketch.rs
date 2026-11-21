//! Hand-drawn outlines and fills: `sketch` draws shapes (nodes from the document, or a shape given
//! in the call) as a pen would, over and over, and `boil` redraws them a dozen times a second.

use super::BlockError;
use super::args::{Args, Param};
use super::kit::Kit;
use super::registry::{Block, Category, Layer, Place};
use super::sketchdraw::{
    ANGLE, BOWING, COLOR, FILL_COLOR, FILL_OPACITY, FILL_STYLE, GAP, PASSES, ROUGHNESS, SEED,
    SHAPE, STROKE_WIDTH, TARGETS, WEIGHT, cadence, draw_on, draw_subject, pen, rect_of, step_track,
    subjects,
};

const DRAW_ON: Param = Param::flag(
    "draw_on",
    "The sketch draws itself: each stroke runs along its line in turn, then the fill.",
    true,
);
const DURATION: Param = Param::count(
    "duration_ms",
    "How long the whole drawing takes to draw on (from the block's start_ms).",
    1_600,
    200,
    20_000,
);
const VARIANTS: Param = Param::count(
    "variants",
    "How many differently seeded drawings take turns: 2 is a nervous shiver, 4 a loose boil.",
    3,
    2,
    4,
);
const FPS: Param = Param::number(
    "fps",
    "How many times a second the drawing changes (animators on twos run at 12).",
    12.0,
    4.0,
    24.0,
);
const ONE_PASS: Param = Param::count(
    "passes",
    "How many times the pen goes over the outline in each drawing.",
    1,
    1,
    2,
);

const SKETCH: &[Param] = &[
    TARGETS,
    SHAPE,
    ROUGHNESS,
    BOWING,
    STROKE_WIDTH,
    PASSES,
    COLOR,
    FILL_STYLE,
    FILL_COLOR,
    ANGLE,
    GAP,
    WEIGHT,
    FILL_OPACITY,
    DRAW_ON,
    DURATION,
    SEED,
];

const BOIL: &[Param] = &[
    TARGETS,
    SHAPE,
    ROUGHNESS,
    BOWING,
    STROKE_WIDTH,
    ONE_PASS,
    COLOR,
    FILL_STYLE,
    FILL_COLOR,
    ANGLE,
    GAP,
    WEIGHT,
    FILL_OPACITY,
    VARIANTS,
    FPS,
    SEED,
];

fn sketch(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pen = pen(k, a);
    let shapes = subjects(k, a, rect_of)?;
    let (animate, duration) = (a.flag("draw_on"), a.num("duration_ms") as u64);
    let mut parts = Vec::new();
    for (i, rings) in shapes.iter().enumerate() {
        parts.extend(draw_subject(k, &pen, rings, &format!("s{}", i + 1), 0));
    }
    if animate {
        draw_on(k, &parts, 0, duration);
    }
    Ok(())
}

fn boil(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pen = pen(k, a);
    let shapes = subjects(k, a, rect_of)?;
    let count = a.num("variants") as u64;
    let beat = cadence(k, a.num("fps"), count);
    for v in 0..count {
        let group = format!("boil-{}", v + 1);
        k.begin(&group);
        for (i, rings) in shapes.iter().enumerate() {
            draw_subject(k, &pen, rings, &format!("v{}s{}", v + 1, i + 1), v);
        }
        k.end_with(|g| g.opacity = (v > 0).then_some(0.0));
        step_track(k, &group, v, count, beat.unit, beat.slot, 1.0);
    }
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "sketch",
        category: Category::Overlays,
        about: "Draws nodes or a shape by hand: wobbly, bowed outlines gone over twice with different jitter and overshooting their corners, and an optional hachure, cross-hatch, zigzag, dots, dashed or loose solid fill. It draws itself on. Deterministic by seed.",
        tags: &[
            "sketch",
            "hand drawn",
            "hand-drawn",
            "pencil",
            "doodle",
            "outline",
            "hachure",
            "rough",
            "wobbly",
            "draw on",
            "illustration",
        ],
        params: SKETCH,
        place: Place::Safe,
        layer: Layer::Front,
        build: sketch,
    },
    Block {
        id: "boil",
        category: Category::Motion,
        about: "A sketch that never sits still: 2 to 4 differently seeded drawings of the same shapes take turns about 12 times a second (hard cuts), the line boil of hand-drawn animation. Loops forever.",
        tags: &[
            "boil",
            "line boil",
            "wobble",
            "hand drawn",
            "sketch",
            "twos",
            "jitter",
            "loop",
            "animation",
        ],
        params: BOIL,
        place: Place::Safe,
        layer: Layer::Front,
        build: boil,
    },
];
