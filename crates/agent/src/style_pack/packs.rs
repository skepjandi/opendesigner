//! The eight curated style packs: a palette, a type scale, a shape posture, a motion
//! personality and the rules that hold them together. They are named by mood, not after any
//! product or studio, and every one of them is a different look.

use super::{Motion, Outline, Palette, Personality, Shadow, Shape, StylePack, TypeScale};

#[allow(clippy::too_many_arguments)]
const fn pack(
    id: &'static str,
    name: &'static str,
    summary: &'static str,
    mood: &'static str,
    palette: Palette,
    type_scale: TypeScale,
    shape: Shape,
    motion: Motion,
    rules: &'static [&'static str],
    anti_patterns: &'static [&'static str],
) -> StylePack {
    StylePack {
        id,
        name,
        summary,
        mood,
        palette,
        type_scale,
        shape,
        motion,
        rules,
        anti_patterns,
    }
}

/// Every style pack, in the order `list_style_packs` shows them.
pub const PACKS: &[StylePack] = &[
    pack(
        "deep-space-flat",
        "Deep space flat",
        "A deep blue-black ground, one warm amber accent and soft flat shapes: scenes that float \
         in space and stay calm.",
        "cosmic, calm, wide",
        Palette {
            bg: "#0d1226",
            surface: "#161d3a",
            fg: "#eef2ff",
            muted: "#8fa0c8",
            border: "#26325c",
            accent: "#ffb454",
        },
        TypeScale {
            display: "Helvetica Neue",
            body: "Helvetica Neue",
            ratio: 1.25,
            weights: &[400, 700],
        },
        Shape {
            corner_radius: 8.0,
            stroke_width: 0.0,
            outline: Outline::None,
            shadow: Shadow::Soft,
        },
        Motion {
            personality: Personality::Cinematic,
            default_ease: "expo_out",
            enter_ms: 420,
            exit_ms: 280,
            stagger_ms: 90,
            hold_ms_per_word: 120,
            camera_drift_pct: 6.0,
        },
        &[
            "Keep 80 percent of every frame to a proven flat-scene pattern and spend the other 20 \
             percent on one distinctive choice.",
            "Use the amber accent at most twice per screen; the rest is space and starlight.",
            "Build the scene from flat vector shapes on layered depths, never from a picture with \
             text on it.",
            "Let the camera drift at most 6 percent of the frame; the space should feel deep, not busy.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the ground is one deep blue.",
            "No emoji as icons: draw or import vector marks.",
            "No invented numbers, distances or star counts: use the user's figures.",
            "No stock-photo galaxy backdrop: the stars are vector shapes on their own depth.",
            "Do not centre everything: anchor the subject off-centre and let the space breathe.",
            "No linear easing except a looping rotation; everything else eases.",
            "Never start everything at once: stagger the depth layers in.",
        ],
    ),
    pack(
        "paper-craft",
        "Paper craft",
        "Warm cream paper, cut-out shapes with a soft shadow and a moss accent: handmade, tactile, \
         slow enough to read.",
        "warm, handmade, tactile",
        Palette {
            bg: "#f9efd6",
            surface: "#fffdf6",
            fg: "#3a2b21",
            muted: "#8a7358",
            border: "#e0cfb2",
            accent: "#5f7a3c",
        },
        TypeScale {
            display: "Georgia",
            body: "Helvetica Neue",
            ratio: 1.333,
            weights: &[400, 700],
        },
        Shape {
            corner_radius: 14.0,
            stroke_width: 2.0,
            outline: Outline::Ink,
            shadow: Shadow::Soft,
        },
        Motion {
            personality: Personality::Calm,
            default_ease: "power2_out",
            enter_ms: 480,
            exit_ms: 320,
            stagger_ms: 120,
            hold_ms_per_word: 150,
            camera_drift_pct: 3.0,
        },
        &[
            "Keep 80 percent of every frame to a proven cut-paper pattern and spend the other 20 \
             percent on one distinctive choice.",
            "Use the moss accent at most twice per screen; everything else is paper and ink.",
            "Give every shape a soft shadow and a hand-drawn outline, so it reads as a cut-out.",
            "Move slowly: let a shape arrive over about half a second and hold a word for a beat.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the ground is warm paper.",
            "No emoji as icons: cut out vector marks instead.",
            "No invented numbers or dates: use the user's figures.",
            "No stock-photo craft backdrop: the paper is a flat shape with a shadow.",
            "Do not centre everything: stack the cut-outs off-centre.",
            "No linear easing except a looping rotation; paper settles, it does not slide.",
            "Never start everything at once: stagger the layers in.",
        ],
    ),
    pack(
        "swiss-kinetic",
        "Swiss kinetic",
        "Neutral grey and white, black type and one signal red: a grid, a big number, and type that \
         moves like it means it.",
        "precise, typographic, fast",
        Palette {
            bg: "#e4e7ea",
            surface: "#ffffff",
            fg: "#0d0d0d",
            muted: "#6b6b6b",
            border: "#cfcfcb",
            accent: "#e2231a",
        },
        TypeScale {
            display: "Helvetica Neue",
            body: "Helvetica Neue",
            ratio: 1.5,
            weights: &[400, 700, 900],
        },
        Shape {
            corner_radius: 0.0,
            stroke_width: 0.0,
            outline: Outline::None,
            shadow: Shadow::None,
        },
        Motion {
            personality: Personality::Snappy,
            default_ease: "power4_out",
            enter_ms: 240,
            exit_ms: 180,
            stagger_ms: 60,
            hold_ms_per_word: 90,
            camera_drift_pct: 2.0,
        },
        &[
            "Keep 80 percent of every frame to a proven grid and one clear idea, and spend the other \
             20 percent on one distinctive choice.",
            "Use the signal red at most twice per screen; black type and white space do the rest.",
            "Set the type on a grid, flush left, with a scale that jumps by half.",
            "Move fast: a word arrives in about a quarter of a second and leaves faster still.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the ground is flat grey and white.",
            "No emoji as icons: set a real mark or draw one.",
            "No invented numbers or statistics: use the user's figures.",
            "No stock-photo backdrop: a Swiss frame is type and space.",
            "Do not centre everything: flush everything left and align it to the grid.",
            "No linear easing except a looping rotation; type snaps into place.",
            "Never start everything at once: stagger the words in, 60 ms apart.",
        ],
    ),
    pack(
        "neon-night",
        "Neon night",
        "A violet-black city night with one electric cyan: glow, hard edges and a little bounce, \
         like a sign that just switched on.",
        "electric, urban, night",
        Palette {
            bg: "#150a1f",
            surface: "#241033",
            fg: "#fff0fb",
            muted: "#b48ccf",
            border: "#3b1d52",
            accent: "#00e5ff",
        },
        TypeScale {
            display: "Helvetica Neue",
            body: "Helvetica Neue",
            ratio: 1.25,
            weights: &[400, 700],
        },
        Shape {
            corner_radius: 4.0,
            stroke_width: 0.0,
            outline: Outline::None,
            shadow: Shadow::Hard,
        },
        Motion {
            personality: Personality::Playful,
            default_ease: "spring(450,0.3)",
            enter_ms: 320,
            exit_ms: 240,
            stagger_ms: 70,
            hold_ms_per_word: 100,
            camera_drift_pct: 5.0,
        },
        &[
            "Keep 80 percent of every frame to a proven flat-vector scene and spend the other 20 \
             percent on one distinctive choice.",
            "Use the cyan accent at most twice per screen; the violet carries the rest.",
            "Give shapes hard edges and hard shadows, and let one element glow.",
            "Let things bounce: a spring that settles in about 450 ms, never a linear move.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the night is one violet-black.",
            "No emoji as icons: draw the mark as a vector shape.",
            "No invented numbers, prices or counts: use the user's figures.",
            "No stock-photo city backdrop: build the skyline from vector blocks.",
            "Do not centre everything: let the glow sit off the axis.",
            "No linear easing except a looping rotation; everything else springs.",
            "Never start everything at once: stagger the layers in.",
        ],
    ),
    pack(
        "soft-clay",
        "Soft clay",
        "Cool blue-grey clay with one dusty rose: round corners, soft shadows and a friendly pace, \
         for product stories that must not shout.",
        "gentle, friendly, soft",
        Palette {
            bg: "#dde7ee",
            surface: "#eef5fa",
            fg: "#26313f",
            muted: "#6d7c8f",
            border: "#c9dbe7",
            accent: "#b3568c",
        },
        TypeScale {
            display: "Avenir Next",
            body: "Avenir Next",
            ratio: 1.25,
            weights: &[400, 600, 700],
        },
        Shape {
            corner_radius: 20.0,
            stroke_width: 0.0,
            outline: Outline::None,
            shadow: Shadow::Soft,
        },
        Motion {
            personality: Personality::Calm,
            default_ease: "power2_out",
            enter_ms: 400,
            exit_ms: 300,
            stagger_ms: 100,
            hold_ms_per_word: 140,
            camera_drift_pct: 4.0,
        },
        &[
            "Keep 80 percent of every frame to a proven soft-product layout and spend the other 20 \
             percent on one distinctive choice.",
            "Use the dusty rose accent at most twice per screen; the clay greys do the rest.",
            "Round every corner generously and give shapes one soft shadow, never a hard one.",
            "Let the camera breathe: a slow drift of at most 4 percent, and a calm ease.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the ground is cool clay.",
            "No emoji as icons: draw or import a vector mark.",
            "No invented numbers or testimonials: use the user's figures.",
            "No stock-photo backdrop: the product stands on a flat clay shape.",
            "Do not centre everything: set the copy left, let the product breathe on the right.",
            "No linear easing except a looping rotation; everything else settles.",
            "Never start everything at once: stagger the parts in.",
        ],
    ),
    pack(
        "editorial-ink",
        "Editorial ink",
        "Paper white, black ink and one oxblood: a serif headline, a hairline rule and a page that \
         moves like print.",
        "print, serious, typographic",
        Palette {
            bg: "#efece2",
            surface: "#fffdf8",
            fg: "#1c1a17",
            muted: "#7a736a",
            border: "#d8d1c4",
            accent: "#8c2f39",
        },
        TypeScale {
            display: "Georgia",
            body: "Helvetica Neue",
            ratio: 1.333,
            weights: &[400, 700],
        },
        Shape {
            corner_radius: 0.0,
            stroke_width: 1.0,
            outline: Outline::Ink,
            shadow: Shadow::None,
        },
        Motion {
            personality: Personality::Cinematic,
            default_ease: "expo_out",
            enter_ms: 500,
            exit_ms: 300,
            stagger_ms: 120,
            hold_ms_per_word: 160,
            camera_drift_pct: 3.0,
        },
        &[
            "Keep 80 percent of every frame to a proven editorial layout and spend the other 20 \
             percent on one distinctive choice.",
            "Use the oxblood accent at most twice per screen; ink and paper carry the rest.",
            "Set the headline in a serif, the body in a plain sans, and rule the columns with a \
             hairline.",
            "Move like a page: a long ease, a beat on every word, and no bounce.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the page is paper.",
            "No emoji as icons: set a real mark or draw one.",
            "No invented numbers, names or quotes: use the user's figures.",
            "No stock-photo backdrop: an editorial frame is type and rules.",
            "Do not centre everything: set the measure left and hang the headline in the margin.",
            "No linear easing except a looping rotation; the page turns, it does not slide.",
            "Never start everything at once: stagger the columns in.",
        ],
    ),
    pack(
        "sunny-explainer",
        "Sunny explainer",
        "Sky blue, white and one deep orange: bright shapes, a clear order and the bounce of \
         something that wants to be understood.",
        "bright, teaching, friendly",
        Palette {
            bg: "#cfeaf7",
            surface: "#ffffff",
            fg: "#123a5c",
            muted: "#5b7f99",
            border: "#b3d8e9",
            accent: "#c9630a",
        },
        TypeScale {
            display: "Avenir Next",
            body: "Helvetica Neue",
            ratio: 1.25,
            weights: &[400, 700],
        },
        Shape {
            corner_radius: 12.0,
            stroke_width: 0.0,
            outline: Outline::None,
            shadow: Shadow::Soft,
        },
        Motion {
            personality: Personality::Playful,
            default_ease: "spring(450,0.3)",
            enter_ms: 380,
            exit_ms: 260,
            stagger_ms: 110,
            hold_ms_per_word: 130,
            camera_drift_pct: 5.0,
        },
        &[
            "Keep 80 percent of every frame to a proven explainer pattern and spend the other 20 \
             percent on one distinctive choice.",
            "Use the orange accent at most twice per screen; the sky and the white do the rest.",
            "One idea per beat: the shape, then the label, then the arrow, never all at once.",
            "Let shapes arrive with a light spring and hold each word long enough to be read aloud.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the sky is one bright blue.",
            "No emoji as icons: draw a real vector mark for the idea.",
            "No invented numbers, dates or facts: use the user's figures.",
            "No stock-photo backdrop: the scene is flat shapes on the sky.",
            "Do not centre everything: put the subject where the eye lands first, not in the middle.",
            "No linear easing except a looping rotation; everything else springs.",
            "Never start everything at once: stagger the shapes in.",
        ],
    ),
    pack(
        "mono-terminal",
        "Mono terminal",
        "Green on near-black, one monospace family and nothing else: a readout, a cursor and a \
         status line.",
        "technical, precise, mono",
        Palette {
            bg: "#06120c",
            surface: "#0c1c13",
            fg: "#d8ffe8",
            muted: "#7fb99a",
            border: "#173a26",
            accent: "#46e08a",
        },
        TypeScale {
            display: "Menlo",
            body: "Menlo",
            ratio: 1.2,
            weights: &[400, 700],
        },
        Shape {
            corner_radius: 2.0,
            stroke_width: 1.0,
            outline: Outline::Ink,
            shadow: Shadow::None,
        },
        Motion {
            personality: Personality::Snappy,
            default_ease: "power4_out",
            enter_ms: 200,
            exit_ms: 160,
            stagger_ms: 50,
            hold_ms_per_word: 80,
            camera_drift_pct: 1.0,
        },
        &[
            "Keep 80 percent of every frame to a proven terminal layout and spend the other 20 \
             percent on one distinctive choice.",
            "Use the phosphor green at most twice per screen; the rest is green on black.",
            "Set everything in one monospace family on a strict baseline grid.",
            "Move in steps: a line appears in 200 ms, a cursor blinks, nothing drifts.",
            "Take every colour from the tokens; a colour of your own breaks the palette.",
        ],
        &[
            "No indigo-to-purple gradient hero: the ground is black.",
            "No emoji as icons: draw the glyph or set it as text.",
            "No invented log lines, numbers or statuses: use the user's figures.",
            "No stock-photo backdrop: a terminal frame is text and rules.",
            "Do not centre everything: align every line to the left margin.",
            "No linear easing except a looping rotation; a readout snaps.",
            "Never start everything at once: print the lines 50 ms apart.",
        ],
    ),
    pack(
        "pixel-16",
        "Pixel 16",
        "A sixteen-colour pixel-art ground: night navy, crimson edges and one warm orange, hard edges and \
         no blur. Sprites, tiles and icons for a top-down game.",
        "retro, crisp, game",
        Palette {
            bg: "#1a1c2c",
            surface: "#333c57",
            fg: "#f4f4f4",
            muted: "#94b0c2",
            border: "#b13e53",
            accent: "#ef7d57",
        },
        TypeScale {
            display: "Menlo",
            body: "Menlo",
            ratio: 1.5,
            weights: &[400, 700],
        },
        Shape {
            corner_radius: 0.0,
            stroke_width: 1.0,
            outline: Outline::Ink,
            shadow: Shadow::None,
        },
        Motion {
            personality: Personality::Snappy,
            default_ease: "power2_out",
            enter_ms: 160,
            exit_ms: 120,
            stagger_ms: 60,
            hold_ms_per_word: 100,
            camera_drift_pct: 0.0,
        },
        &[
            "Keep 80 percent of every sprite, tile and icon to a proven pixel-art pattern and spend \
             the other 20 percent on one distinctive choice.",
            "Draw on a whole-pixel grid: a character sheet is 48 px cells, an icon 32 px, a tile \
             48 px; build the frame at that size and export with pixel_art on and an integer scale.",
            "Use only these sixteen colours: #1a1c2c #5d275d #b13e53 #ef7d57 #ffcd75 #a7f070 \
             #38b764 #257179 #29366f #3b5dc9 #41a6f6 #73eff7 #f4f4f4 #94b0c2 #566c86 #333c57. \
             Run quantize_image with this list on anything that is not drawn from it.",
            "Give every image node image_smoothing nearest, and never scale a picture by a \
             fraction.",
            "Shade with colour steps, not blur: no gradients, no soft shadows, no translucent edges.",
        ],
        &[
            "No indigo-to-purple gradient hero: the ground is a flat palette colour.",
            "No emoji as icons: draw the pixels of the icon.",
            "No anti-aliased edges and no blur: every pixel is one palette colour or transparent.",
            "No invented numbers or names: use the user's lorebook and figures.",
            "Do not centre everything: place sprites on the tile grid, not in the middle.",
            "No linear easing except a looping rotation; step and snap instead.",
            "Never mix pixel sizes in one sheet: one cell size, one pixel scale.",
        ],
    ),
];
