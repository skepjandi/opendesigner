//! The choices a user can make before the agent starts (a look, colors, type, mood, motion,
//! imagery) and the text they become in the prompt. The window shows `catalog_json` as a menu;
//! `prompt` turns what was picked into a block the agent is told to follow
//! (`docs/wiki/agent-integration.md#direction-picker`).
//!
//! Fonts are ones macOS ships, so a pick renders as chosen; the renderer falls back to the
//! bundled Lato for a family the machine does not have, and the prompt says to check the preview.

use serde::Deserialize;
use serde_json::{Value, json};

pub struct Style {
    pub id: &'static str,
    pub name: &'static str,
    pub rules: &'static str,
}

pub const STYLES: &[Style] = &[
    Style {
        id: "swiss",
        name: "Swiss grid and kinetic type",
        rules: "one typeface, a 12-column grid with 4% margins, one signal color on a neutral field, type slides in by snaps, no shadows or overshoot",
    },
    Style {
        id: "flat-explainer",
        name: "Flat vector explainer",
        rules: "four or five flat colors, no gradients, small overshoot pops, staggered groups, scene changes carried by a shape, one analogy",
    },
    Style {
        id: "dark-keynote",
        name: "Dark tech keynote",
        rules: "near-black background, hairlines at 10% opacity, one accent on one thing, slow long eases, one giant number or phrase, no glow",
    },
    Style {
        id: "screencast",
        name: "Living UI screencast",
        rules: "real captures and copy in device frames, one action every 3 to 4 seconds, push in on the detail then back out, highlights as overlays",
    },
    Style {
        id: "data-story",
        name: "Data story",
        rules: "one reading per screen, bars and lines grown from the baseline, only real numbers, large labels, one annotation that says why it matters",
    },
    Style {
        id: "warm-editorial",
        name: "Warm editorial (print feel)",
        rules: "paper-toned background, two or three inks, a serif display with a clean sans, generous margins, asymmetric layout, calm motion",
    },
    Style {
        id: "soft-product",
        name: "Soft product (light and friendly)",
        rules: "light background with one soft tint, a large rounded device, 28 to 40 px corner radii, rounded type, sentence-case headlines",
    },
    Style {
        id: "neo-brutalist",
        name: "Neo-brutalist",
        rules: "hard 3 to 4 px black outlines, flat saturated blocks, offset hard shadows, heavy or mono type, hard cuts",
    },
    Style {
        id: "luxury-minimal",
        name: "Luxury minimal",
        rules: "lots of empty space, one serif or thin sans, one metallic or deep accent, tiny precise captions, very slow fades, nothing decorative",
    },
    Style {
        id: "playful-bold",
        name: "Playful and bold",
        rules: "big rounded shapes, saturated color blocks, chunky type, stickers and badges, bouncy pops with back easing, short punchy lines",
    },
    Style {
        id: "riso-print",
        name: "Risograph print",
        rules: "two or three spot inks that overlap, grainy flat shapes, slightly off-register layers, warm paper, no gradients",
    },
    Style {
        id: "glass-gradient",
        name: "Soft gradient and glass (restrained)",
        rules: "one smooth two-tone gradient background from the palette, frosted translucent cards with a hairline border, large soft type, a single glow at most",
    },
];

pub struct Palette {
    pub id: &'static str,
    pub name: &'static str,
    /// background, surface, text, accent, muted
    pub colors: [&'static str; 5],
}

pub const PALETTES: &[Palette] = &[
    Palette {
        id: "midnight-gold",
        name: "Midnight and gold",
        colors: ["#0b1020", "#151c33", "#f4efe4", "#e8b86a", "#8b93ad"],
    },
    Palette {
        id: "warm-paper",
        name: "Warm paper",
        colors: ["#f6f1e7", "#ebe3d3", "#1f1a14", "#c4542d", "#7d7466"],
    },
    Palette {
        id: "ocean-blue",
        name: "Ocean blue",
        colors: ["#071399", "#0442e6", "#ffffff", "#b7c6ff", "#5b7eec"],
    },
    Palette {
        id: "forest",
        name: "Forest",
        colors: ["#0f2a21", "#173a2f", "#eef3e8", "#9fd356", "#7f9a8c"],
    },
    Palette {
        id: "sunset",
        name: "Sunset",
        colors: ["#2b1237", "#4a1d4f", "#fff3e6", "#ff7a59", "#c7a3c9"],
    },
    Palette {
        id: "mono-signal",
        name: "Mono with signal red",
        colors: ["#101010", "#1d1d1d", "#f5f5f5", "#ff3b30", "#8a8a8a"],
    },
    Palette {
        id: "pastel-soft",
        name: "Soft pastel",
        colors: ["#fdf6f0", "#f6e6e0", "#3b2f3a", "#8fa8f5", "#a99aa3"],
    },
    Palette {
        id: "neon-night",
        name: "Neon night",
        colors: ["#07070d", "#12121f", "#f2f2ff", "#38f5c9", "#7b7ba8"],
    },
    Palette {
        id: "citrus",
        name: "Citrus",
        colors: ["#fffbe8", "#fff0b8", "#1d2a12", "#f2641c", "#7a8564"],
    },
    Palette {
        id: "slate-mint",
        name: "Slate and mint",
        colors: ["#1c2430", "#273241", "#eef2f6", "#5ee0b0", "#8795a8"],
    },
    Palette {
        id: "lavender-ink",
        name: "Lavender and ink",
        colors: ["#f4f1ff", "#e6e0fb", "#1b1535", "#6a4cf0", "#7e77a3"],
    },
    Palette {
        id: "terracotta",
        name: "Terracotta and sand",
        colors: ["#f3e7d8", "#e8d3b9", "#2c1b12", "#b5502e", "#8a6f5a"],
    },
    Palette {
        id: "cobalt-cream",
        name: "Cobalt and cream",
        colors: ["#f7f3e8", "#ece5d2", "#0e1a40", "#1f3fff", "#6c7391"],
    },
    Palette {
        id: "emerald-ivory",
        name: "Emerald and ivory",
        colors: ["#06382b", "#0b4a39", "#f8f5ec", "#d9b45a", "#86a79a"],
    },
    Palette {
        id: "berry",
        name: "Berry",
        colors: ["#2a0a1f", "#431030", "#fde8f0", "#ff4f9a", "#b57a97"],
    },
    Palette {
        id: "sky-light",
        name: "Sky light",
        colors: ["#eef6ff", "#dbeafe", "#0b1b33", "#2563eb", "#64748b"],
    },
];

pub struct TypePair {
    pub id: &'static str,
    pub name: &'static str,
    pub heading: &'static str,
    pub body: &'static str,
}

pub const TYPE_PAIRS: &[TypePair] = &[
    TypePair {
        id: "helvetica",
        name: "Helvetica Neue, clean",
        heading: "Helvetica Neue",
        body: "Helvetica Neue",
    },
    TypePair {
        id: "avenir",
        name: "Avenir Next, friendly",
        heading: "Avenir Next",
        body: "Avenir Next",
    },
    TypePair {
        id: "georgia-helv",
        name: "Georgia headlines, Helvetica Neue text",
        heading: "Georgia",
        body: "Helvetica Neue",
    },
    TypePair {
        id: "didot-avenir",
        name: "Didot headlines, Avenir Next text (fashion)",
        heading: "Didot",
        body: "Avenir Next",
    },
    TypePair {
        id: "futura",
        name: "Futura, geometric",
        heading: "Futura",
        body: "Futura",
    },
    TypePair {
        id: "baskerville-gill",
        name: "Baskerville headlines, Gill Sans text (classic)",
        heading: "Baskerville",
        body: "Gill Sans",
    },
    TypePair {
        id: "menlo-helv",
        name: "Menlo (mono) for numbers, Helvetica Neue text (technical)",
        heading: "Menlo",
        body: "Helvetica Neue",
    },
    TypePair {
        id: "impact-avenir",
        name: "Impact headlines, Avenir Next text (loud)",
        heading: "Impact",
        body: "Avenir Next",
    },
    TypePair {
        id: "lato",
        name: "Lato (bundled, always available)",
        heading: "Lato",
        body: "Lato",
    },
];

pub const MOODS: &[(&str, &str)] = &[
    ("calm", "calm"),
    ("energetic", "energetic"),
    ("premium", "premium"),
    ("friendly", "friendly"),
    ("technical", "technical and precise"),
    ("bold", "bold and confident"),
    ("playful", "playful"),
    ("trustworthy", "trustworthy"),
    ("urgent", "urgent"),
    ("nostalgic", "nostalgic"),
];

pub const MOTION: &[(&str, &str, &str)] = &[
    (
        "none",
        "None (still images)",
        "No motion: this is for still images.",
    ),
    (
        "subtle",
        "Subtle",
        "Subtle motion: fades and short slides, slow pushes of at most 5%, long holds.",
    ),
    (
        "standard",
        "Standard",
        "Standard motion: eased slides and pops, staggered entrances, something new every 2 to 4 seconds.",
    ),
    (
        "kinetic",
        "Kinetic",
        "Kinetic motion: fast snaps, type that moves in on every beat, cuts on the rhythm, a change every 1 to 2 seconds.",
    ),
];

pub const DENSITY: &[(&str, &str, &str)] = &[
    ("airy", "Airy", "very generous space, one idea per frame"),
    (
        "balanced",
        "Balanced",
        "balanced space with a clear hierarchy",
    ),
    (
        "dense",
        "Dense",
        "information-rich, still with a clear hierarchy and grid",
    ),
];

pub const IMAGERY: &[(&str, &str, &str)] = &[
    (
        "screens",
        "Real app screens in device frames",
        "real captures of the product inside device frames",
    ),
    (
        "typographic",
        "Typography only",
        "typography and simple shapes only, no pictures",
    ),
    (
        "abstract",
        "Abstract shapes",
        "abstract geometric shapes and gradients as the visual",
    ),
    (
        "photo",
        "Photography",
        "photographs the user has provided (find them with list_media or find_files), never invented",
    ),
    (
        "icons",
        "Icons and badges",
        "simple consistent icons and badges as the visual",
    ),
];

/// What the menu offers, as JSON for the window.
pub fn catalog_json() -> Value {
    json!({
        "styles": STYLES.iter().map(|s| json!({"id": s.id, "name": s.name, "note": s.rules})).collect::<Vec<_>>(),
        "palettes": PALETTES.iter().map(|p| json!({"id": p.id, "name": p.name, "colors": p.colors})).collect::<Vec<_>>(),
        "types": TYPE_PAIRS.iter().map(|t| json!({"id": t.id, "name": t.name, "heading": t.heading, "body": t.body})).collect::<Vec<_>>(),
        "moods": MOODS.iter().map(|(id, n)| json!({"id": id, "name": n})).collect::<Vec<_>>(),
        "motion": MOTION.iter().map(|(id, n, _)| json!({"id": id, "name": n})).collect::<Vec<_>>(),
        "density": DENSITY.iter().map(|(id, n, _)| json!({"id": id, "name": n})).collect::<Vec<_>>(),
        "imagery": IMAGERY.iter().map(|(id, n, _)| json!({"id": id, "name": n})).collect::<Vec<_>>(),
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Selection {
    pub styles: Vec<String>,
    pub palette: Option<String>,
    /// Colors the user typed instead of a listed palette (hex, comma or space separated).
    pub custom_colors: String,
    pub type_pair: Option<String>,
    pub moods: Vec<String>,
    pub motion: Option<String>,
    pub density: Option<String>,
    pub imagery: Vec<String>,
    pub audience: String,
    pub notes: String,
    /// Search installed skills and the web for good ones before starting.
    pub research: bool,
    /// Offer a few small directions to pick from before the full build.
    pub options_first: bool,
    /// Write the choices and what is learned into the agent memory folder.
    pub remember: bool,
}

fn hex_list(s: &str) -> Vec<String> {
    s.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|w| {
            let h = w.strip_prefix('#').unwrap_or("");
            matches!(h.len(), 3 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(str::to_lowercase)
        .collect()
}

/// The block that goes into the prompt, or an empty string when nothing was chosen.
pub fn prompt(sel: &Selection) -> String {
    let mut lines: Vec<String> = Vec::new();
    let styles: Vec<&Style> = sel
        .styles
        .iter()
        .filter_map(|id| STYLES.iter().find(|s| s.id == *id))
        .collect();
    if !styles.is_empty() {
        let names: Vec<&str> = styles.iter().map(|s| s.name).collect();
        let rules: Vec<String> = styles
            .iter()
            .map(|s| format!("{}: {}", s.name, s.rules))
            .collect();
        lines.push(format!(
            "- Look: {}. Read get_playbook style and follow these rules. {}",
            names.join(" with "),
            rules.join(" / ")
        ));
    }
    let custom = hex_list(&sel.custom_colors);
    if !custom.is_empty() {
        lines.push(format!(
            "- Colors: use exactly these (define them with set_token, name each role): {}.",
            custom.join(", ")
        ));
    } else if let Some(p) = sel
        .palette
        .as_deref()
        .and_then(|id| PALETTES.iter().find(|p| p.id == id))
    {
        lines.push(format!(
            "- Colors: the palette \"{}\" (define these with set_token and use only them): background {}, surface {}, text {}, accent {}, muted {}.",
            p.name, p.colors[0], p.colors[1], p.colors[2], p.colors[3], p.colors[4]
        ));
    }
    if let Some(t) = sel
        .type_pair
        .as_deref()
        .and_then(|id| TYPE_PAIRS.iter().find(|t| t.id == id))
    {
        lines.push(format!(
            "- Type: headings in {}, text in {} (set font_family; if a family is missing the renderer falls back to Lato, so check the preview).",
            t.heading, t.body
        ));
    }
    let moods: Vec<&str> = sel
        .moods
        .iter()
        .filter_map(|id| MOODS.iter().find(|(m, _)| m == id).map(|(_, n)| *n))
        .collect();
    if !moods.is_empty() {
        lines.push(format!("- Mood: {}.", moods.join(", ")));
    }
    if let Some((_, _, text)) = sel
        .motion
        .as_deref()
        .and_then(|id| MOTION.iter().find(|(m, _, _)| *m == id))
    {
        lines.push(format!("- Motion: {text}"));
    }
    if let Some((_, _, text)) = sel
        .density
        .as_deref()
        .and_then(|id| DENSITY.iter().find(|(m, _, _)| *m == id))
    {
        lines.push(format!("- Layout: {text}."));
    }
    let imagery: Vec<&str> = sel
        .imagery
        .iter()
        .filter_map(|id| IMAGERY.iter().find(|(m, _, _)| m == id).map(|(_, _, t)| *t))
        .collect();
    if !imagery.is_empty() {
        lines.push(format!("- Imagery: {}.", imagery.join("; ")));
    }
    let audience = sel.audience.trim();
    if !audience.is_empty() {
        lines.push(format!("- Audience and goal: {audience}"));
    }
    let notes = sel.notes.trim();
    if !notes.is_empty() {
        lines.push(format!("- Also: {notes}"));
    }
    if lines.is_empty() && !sel.research && !sel.options_first && !sel.remember {
        return String::new();
    }
    let mut out = String::new();
    if !lines.is_empty() {
        out.push_str("Direction chosen by the user (follow it; it overrides your own taste, not the brand's facts):\n");
        out.push_str(&lines.join("\n"));
        out.push('\n');
    }
    if sel.research {
        out.push_str("Before you design: look for skills and references that fit this work. List the skills you have and use the best matches (app store design, motion graphics, ffmpeg editing, front-end design); if you can search the web or a skills directory, find and read good examples and use what applies. Say which you used.\n");
    }
    if sel.options_first {
        out.push_str("First show me three small directions (a hero frame each, made with this direction as the base): build each as its own clearly named frame, call offer_variants so they appear in my Variants panel (not on the canvas), tell me to pick there, then wait. Call get_variants for my pick before you build the full set.\n");
    }
    if sel.remember {
        out.push_str("Record these choices and what you learn in the agent memory (write_memory) so the next task starts from them.\n");
    }
    out.trim_end().to_string()
}

/// `prompt` for a selection given as JSON by the window.
pub fn prompt_from_json(json: &str) -> Result<String, String> {
    serde_json::from_str::<Selection>(json)
        .map(|s| prompt(&s))
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_colors_are_hex() {
        let mut ids: Vec<&str> = STYLES.iter().map(|s| s.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), STYLES.len());
        for p in PALETTES {
            assert_eq!(hex_list(&p.colors.join(" ")).len(), 5, "{}", p.id);
        }
        assert!(MOTION.iter().all(|(id, _, _)| !id.is_empty()));
    }

    #[test]
    fn the_styles_name_looks_the_style_playbook_has() {
        let playbook = crate::playbook::topic("style").unwrap();
        for id in [
            "swiss",
            "flat-explainer",
            "dark-keynote",
            "screencast",
            "data-story",
            "warm-editorial",
            "soft-product",
            "neo-brutalist",
        ] {
            let s = STYLES.iter().find(|s| s.id == id).unwrap();
            let key = s.name.split(" (").next().unwrap().to_lowercase();
            let first = key.split_whitespace().next().unwrap();
            assert!(
                playbook.to_lowercase().contains(first),
                "{} is not in the style playbook",
                s.name
            );
        }
    }

    #[test]
    fn nothing_chosen_adds_nothing_and_choices_become_instructions() {
        assert_eq!(prompt(&Selection::default()), "");
        let sel: Selection = serde_json::from_value(json!({
            "styles": ["dark-keynote", "nope"], "palette": "midnight-gold", "type_pair": "helvetica",
            "moods": ["premium"], "motion": "subtle", "density": "airy", "imagery": ["screens"],
            "audience": "indie developers", "research": true, "options_first": true, "remember": true
        })).unwrap();
        let p = prompt(&sel);
        for needle in [
            "Dark tech keynote",
            "#0b1020",
            "#e8b86a",
            "Helvetica Neue",
            "premium",
            "Subtle motion",
            "very generous space",
            "device frames",
            "indie developers",
            "look for skills",
            "three small directions",
            "write_memory",
        ] {
            assert!(p.contains(needle), "{needle} missing in:\n{p}");
        }
        assert!(!p.contains("nope"));
    }

    #[test]
    fn typed_colors_beat_a_listed_palette_and_junk_is_ignored() {
        let sel = Selection {
            palette: Some("forest".into()),
            custom_colors: "#FF0000, #00f; red nonsense #12".into(),
            ..Selection::default()
        };
        let p = prompt(&sel);
        assert!(
            p.contains("#ff0000, #00f") && !p.contains("Forest") && !p.contains("nonsense"),
            "{p}"
        );
        assert!(prompt_from_json("{").is_err());
        assert!(prompt_from_json("{\"unknown\": 1}").unwrap().is_empty());
    }
}
