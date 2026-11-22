//! Style packs: a palette, a type scale, a shape posture and a motion personality that a piece of
//! work commits to before it is drawn (`docs/wiki/agent-integration.md#style-packs`).
//!
//! A pack is data, not a file to load: the eight curated packs live in `packs.rs`, so there is
//! nothing to find at run time and nothing to go missing. `apply_style_pack` writes the palette
//! into the document as design tokens (one attributed, undoable transaction) and records the pack
//! in the workspace brief, so the linter can hold the work to the palette it was given.

mod color;
mod packs;

use std::collections::BTreeMap;

use od_core::lint::PALETTE_LOCK;
use od_core::{Operation, Rgba};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ApplyArgs as OpsArgs, CallCtx, Output, ToolError, apply_ops_receipt, parse, tool,
};

pub use packs::PACKS;

pub const NAMES: [&str; 3] = ["list_style_packs", "get_style_pack", "apply_style_pack"];

/// The token that says the palette is locked. Its value is a flag, not a colour to paint with.
const LOCK_VALUE: &str = "#000000";

/// The roles a colour has in a pack, in the order the tokens are declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Bg,
    Surface,
    Fg,
    Muted,
    Border,
    Accent,
}

impl Role {
    pub const ALL: [Role; 6] = [
        Role::Bg,
        Role::Surface,
        Role::Fg,
        Role::Muted,
        Role::Border,
        Role::Accent,
    ];

    /// The name of the token the role is written as.
    pub fn name(self) -> &'static str {
        match self {
            Role::Bg => "bg",
            Role::Surface => "surface",
            Role::Fg => "fg",
            Role::Muted => "muted",
            Role::Border => "border",
            Role::Accent => "accent",
        }
    }

    /// The colour the role has in a pack.
    pub fn of(self, pack: &StylePack) -> &'static str {
        match self {
            Role::Bg => pack.palette.bg,
            Role::Surface => pack.palette.surface,
            Role::Fg => pack.palette.fg,
            Role::Muted => pack.palette.muted,
            Role::Border => pack.palette.border,
            Role::Accent => pack.palette.accent,
        }
    }
}

/// The six colours every pack declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub bg: &'static str,
    pub surface: &'static str,
    pub fg: &'static str,
    pub muted: &'static str,
    pub border: &'static str,
    pub accent: &'static str,
}

/// The type of a pack: a family for display, a family for body, the scale between sizes and the
/// weights the piece may use.
#[derive(Debug, Clone, Copy)]
pub struct TypeScale {
    pub display: &'static str,
    pub body: &'static str,
    pub ratio: f64,
    pub weights: &'static [u16],
}

/// Whether a pack draws an outline around its shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outline {
    None,
    Ink,
}

impl Outline {
    pub fn as_str(self) -> &'static str {
        match self {
            Outline::None => "none",
            Outline::Ink => "ink",
        }
    }
}

/// Whether a pack casts a shadow under its shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shadow {
    None,
    Soft,
    Hard,
}

impl Shadow {
    pub fn as_str(self) -> &'static str {
        match self {
            Shadow::None => "none",
            Shadow::Soft => "soft",
            Shadow::Hard => "hard",
        }
    }
}

/// The shape posture of a pack: how round its corners are, how thick its outlines are, whether it
/// draws them and whether it casts shadows.
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    pub corner_radius: f64,
    pub stroke_width: f64,
    pub outline: Outline,
    pub shadow: Shadow,
}

/// How a pack moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Personality {
    Calm,
    Snappy,
    Playful,
    Cinematic,
}

impl Personality {
    pub const ALL: [Personality; 4] = [
        Personality::Calm,
        Personality::Snappy,
        Personality::Playful,
        Personality::Cinematic,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Personality::Calm => "calm",
            Personality::Snappy => "snappy",
            Personality::Playful => "playful",
            Personality::Cinematic => "cinematic",
        }
    }

    pub fn parse(text: &str) -> Option<Personality> {
        Personality::ALL.into_iter().find(|p| p.as_str() == text)
    }
}

/// The motion of a pack: the personality, the ease it uses, how long things take to arrive and
/// leave, how far apart they start, how long a word is held and how much the camera drifts.
#[derive(Debug, Clone, Copy)]
pub struct Motion {
    pub personality: Personality,
    pub default_ease: &'static str,
    pub enter_ms: u32,
    pub exit_ms: u32,
    pub stagger_ms: u32,
    pub hold_ms_per_word: u32,
    pub camera_drift_pct: f64,
}

/// A style pack: the look a piece of work commits to.
#[derive(Debug, Clone, Copy)]
pub struct StylePack {
    pub id: &'static str,
    pub name: &'static str,
    pub summary: &'static str,
    pub mood: &'static str,
    pub palette: Palette,
    pub type_scale: TypeScale,
    pub shape: Shape,
    pub motion: Motion,
    pub rules: &'static [&'static str],
    pub anti_patterns: &'static [&'static str],
}

/// The pack with an id, if there is one.
pub fn find(id: &str) -> Option<&'static StylePack> {
    PACKS.iter().find(|p| p.id == id)
}

/// The tokens a pack declares: the six of its palette and the two tints derived from them.
pub fn tokens(pack: &StylePack) -> Vec<(&'static str, String)> {
    let mut tokens: Vec<(&'static str, String)> = Role::ALL
        .iter()
        .map(|role| (role.name(), role.of(pack).to_string()))
        .collect();
    tokens.push((
        "accent_2",
        color::mix(pack.palette.accent, pack.palette.fg, 0.25),
    ));
    tokens.push(("shadow", color::mix(pack.palette.bg, "#000000", 0.35)));
    tokens
}

/// The names of the tokens a pack declares.
pub fn token_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Role::ALL.iter().map(|role| role.name()).collect();
    names.push("accent_2");
    names.push("shadow");
    names
}

/// How far two packs sit from each other: the mean OKLab distance between the same role in both.
/// Two packs that look alike score near zero.
pub fn palette_distance(a: &StylePack, b: &StylePack) -> Option<f64> {
    let mut total = 0.0;
    for role in Role::ALL {
        total += color::distance(role.of(a), role.of(b))?;
    }
    Some(total / f64::from(Role::ALL.len() as u32))
}

/// The biggest difference between two packs, in OKLab: the role they disagree on most.
pub fn palette_gap(a: &StylePack, b: &StylePack) -> Option<f64> {
    let mut gap: Option<f64> = None;
    for role in Role::ALL {
        let d = color::distance(role.of(a), role.of(b))?;
        gap = Some(gap.map_or(d, |seen: f64| seen.max(d)));
    }
    gap
}

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    vec![
        tool(
            "list_style_packs",
            "The style packs: a ready-made look for a piece of work. Each one carries a palette \
             (bg, surface, fg, muted, border, accent), a type scale, a shape posture and a motion \
             personality, with the rules that keep it coherent. Pick ONE and say why it fits the \
             brief; never mix two. Read the `packs` playbook before you choose.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_style_pack",
            "One style pack in full: its colours, its type, its shapes, its motion and its rules \
             written as guidance you can follow, plus what it must never do. Read it before you \
             build anything with the pack.",
            json!({"id": id("A pack id from list_style_packs.")}),
            &["id"],
            true,
        ),
        tool(
            "apply_style_pack",
            "Commit the design to a style pack: writes its palette into the document as the tokens \
             bg, surface, fg, muted, border, accent, accent_2 and shadow, locks the palette (the \
             linter then flags any colour outside it, rule OD092) and records the pack in the \
             workspace brief so later tools know it. It is ONE attributed, checkpointed, undoable \
             step: undo it and the palette goes with it. Paint with the tokens afterwards \
             ({bg}, {accent}, ...) instead of literal colours. `overrides` changes single colours \
             (for example {\"accent\": \"#ff8800\"}) when the user's brand demands it.",
            json!({
                "id": id("A pack id from list_style_packs."),
                "overrides": {
                    "type": "object",
                    "description": "Colours to change: token name (bg, surface, fg, muted, border, accent, accent_2, shadow) to a #rrggbb value.",
                    "additionalProperties": {"type": "string"},
                },
            }),
            &["id"],
            false,
        ),
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GetArgs {
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackArgs {
    id: String,
    #[serde(default)]
    overrides: BTreeMap<String, String>,
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "list_style_packs" => Ok(Output::json(&list_json())),
        "get_style_pack" => get(parse(args)?),
        "apply_style_pack" => apply(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

fn list_json() -> Value {
    Value::Array(
        PACKS
            .iter()
            .map(|pack| {
                json!({
                    "id": pack.id,
                    "name": pack.name,
                    "summary": pack.summary,
                    "mood": pack.mood,
                    "swatches": swatches(pack),
                    "motion": pack.motion.personality.as_str(),
                })
            })
            .collect(),
    )
}

fn swatches(pack: &StylePack) -> Value {
    let mut map = serde_json::Map::new();
    for (name, value) in tokens(pack) {
        map.insert(name.to_string(), Value::String(value));
    }
    Value::Object(map)
}

/// The pack as the agent reads it: everything, and the rules as guidance.
fn pack_json(pack: &StylePack) -> Value {
    json!({
        "id": pack.id,
        "name": pack.name,
        "summary": pack.summary,
        "mood": pack.mood,
        "tokens": swatches(pack),
        "type": {
            "display": pack.type_scale.display,
            "body": pack.type_scale.body,
            "scale_ratio": pack.type_scale.ratio,
            "weights": pack.type_scale.weights,
        },
        "shape": {
            "corner_radius": pack.shape.corner_radius,
            "stroke_width": pack.shape.stroke_width,
            "outline": pack.shape.outline.as_str(),
            "shadow": pack.shape.shadow.as_str(),
        },
        "motion": {
            "personality": pack.motion.personality.as_str(),
            "default_ease": pack.motion.default_ease,
            "enter_ms": pack.motion.enter_ms,
            "exit_ms": pack.motion.exit_ms,
            "stagger_ms": pack.motion.stagger_ms,
            "hold_ms_per_word": pack.motion.hold_ms_per_word,
            "camera_drift_pct": pack.motion.camera_drift_pct,
        },
        "rules": pack.rules,
        "anti_patterns": pack.anti_patterns,
        "guidance": guidance(pack),
    })
}

/// The pack as text an agent can follow line by line.
fn guidance(pack: &StylePack) -> String {
    let mut out = String::new();
    out.push_str(&format!("{} ({}) - {}\n", pack.name, pack.id, pack.summary));
    out.push_str(&format!("Mood: {}.\n", pack.mood));
    out.push_str(&format!(
        "Type: {} for display, {} for body, a scale of {:.3}, weights {}.\n",
        pack.type_scale.display,
        pack.type_scale.body,
        pack.type_scale.ratio,
        pack.type_scale
            .weights
            .iter()
            .map(|w| w.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ));
    out.push_str(&format!(
        "Shape: corner radius {}, stroke {}, outline {}, shadow {}.\n",
        pack.shape.corner_radius,
        pack.shape.stroke_width,
        pack.shape.outline.as_str(),
        pack.shape.shadow.as_str()
    ));
    out.push_str(&format!(
        "Motion: {} personality, ease {}, {} ms to arrive, {} ms to leave, {} ms between \
         arrivals, {} ms per word, camera drift {}%.\n",
        pack.motion.personality.as_str(),
        pack.motion.default_ease,
        pack.motion.enter_ms,
        pack.motion.exit_ms,
        pack.motion.stagger_ms,
        pack.motion.hold_ms_per_word,
        pack.motion.camera_drift_pct
    ));
    out.push_str("Rules:\n");
    for rule in pack.rules {
        out.push_str(&format!("- {rule}\n"));
    }
    out.push_str("Never:\n");
    for rule in pack.anti_patterns {
        out.push_str(&format!("- {rule}\n"));
    }
    out
}

fn get(a: GetArgs) -> Result<Output, ToolError> {
    let pack = find(&a.id)
        .ok_or_else(|| ToolError(format!("no style pack {:?}; packs: {}", a.id, ids())))?;
    Ok(Output::json(&pack_json(pack)))
}

fn ids() -> String {
    PACKS.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")
}

fn apply(ctx: &mut CallCtx, a: PackArgs) -> Result<Output, ToolError> {
    let pack = find(&a.id)
        .ok_or_else(|| ToolError(format!("no style pack {:?}; packs: {}", a.id, ids())))?;
    let names = token_names();
    let mut values: BTreeMap<String, String> = tokens(pack)
        .into_iter()
        .map(|(name, value)| (name.to_string(), value))
        .collect();
    for (name, value) in &a.overrides {
        if !names.contains(&name.as_str()) {
            return Err(ToolError(format!(
                "{name} is not a token of a style pack; the tokens are {}",
                names.join(", ")
            )));
        }
        if Rgba::parse_hex(value).is_err() {
            return Err(ToolError(format!("{value} is not a colour; use #rrggbb")));
        }
        values.insert(name.clone(), value.clone());
    }
    let mut ops: Vec<Operation> = values
        .iter()
        .map(|(name, value)| Operation::SetToken {
            name: name.clone(),
            value: Some(value.clone()),
        })
        .collect();
    ops.push(Operation::SetToken {
        name: PALETTE_LOCK.to_string(),
        value: Some(LOCK_VALUE.to_string()),
    });
    let mut out = apply_ops_receipt(
        ctx,
        OpsArgs {
            ops,
            turn: Some(format!("style-pack:{}", pack.id)),
            summary: Some(format!("apply the {} style pack", pack.name)),
        },
    )?;
    record(ctx, pack)?;
    let mut applied_tokens = serde_json::Map::new();
    for (name, value) in &values {
        applied_tokens.insert(name.clone(), Value::String(value.clone()));
    }
    if let Some(map) = out.as_object_mut() {
        map.insert("pack".into(), json!(pack.id));
        map.insert("pack_name".into(), json!(pack.name));
        map.insert("motion".into(), json!(pack.motion.personality.as_str()));
        map.insert("tokens".into(), Value::Object(applied_tokens));
        map.insert("rules".into(), json!(pack.rules));
        map.insert("anti_patterns".into(), json!(pack.anti_patterns));
        map.insert("guidance".into(), json!(guidance(pack)));
        map.insert(
            "note".into(),
            json!(
                "Paint with the tokens ({bg}, {surface}, {fg}, {muted}, {border}, {accent}, \
                 {accent_2}, {shadow}); a literal colour outside them is now flagged by the \
                 linter (OD092)."
            ),
        );
    }
    Ok(Output::json(&out))
}

/// Records the pack in the workspace brief, so the tools that come later know the look.
fn record(ctx: &CallCtx, pack: &StylePack) -> Result<(), ToolError> {
    let Some(dir) = ctx.ws.dir() else {
        return Ok(());
    };
    let mut brief = crate::formats::load_brief(dir);
    brief.style_pack = Some(crate::formats::BriefStylePack {
        id: pack.id.to_string(),
        name: pack.name.to_string(),
        motion: pack.motion.personality.as_str().to_string(),
    });
    crate::formats::save_brief(dir, &brief).map_err(ToolError)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The WCAG contrast a pack promises: body text on the ground, and the accent on it.
    fn contrasts(pack: &StylePack) -> (f64, f64, f64) {
        (
            color::contrast(pack.palette.fg, pack.palette.bg).unwrap_or(0.0),
            color::contrast(pack.palette.fg, pack.palette.surface).unwrap_or(0.0),
            color::contrast(pack.palette.accent, pack.palette.bg).unwrap_or(0.0),
        )
    }

    #[test]
    fn there_are_eight_packs_with_unique_ids_and_names() {
        assert_eq!(PACKS.len(), 8);
        let mut ids: Vec<&str> = PACKS.iter().map(|p| p.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), PACKS.len());
        let mut names: Vec<&str> = PACKS.iter().map(|p| p.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), PACKS.len());
        for pack in PACKS {
            assert!(!pack.rules.is_empty(), "{}", pack.id);
            assert!(!pack.anti_patterns.is_empty(), "{}", pack.id);
            assert!(
                pack.rules
                    .iter()
                    .any(|r| r.contains("80 percent") || r.contains("80%")),
                "{} has no 80/20 rule",
                pack.id
            );
            assert!(
                pack.anti_patterns
                    .iter()
                    .any(|r| r.to_lowercase().contains("emoji")),
                "{} allows emoji as icons",
                pack.id
            );
        }
    }

    #[test]
    fn every_pack_has_real_contrast() {
        for pack in PACKS {
            let (text, on_surface, accent) = contrasts(pack);
            assert!(text >= 4.5, "{}: text on bg is {text}", pack.id);
            assert!(
                on_surface >= 4.5,
                "{}: text on surface is {on_surface}",
                pack.id
            );
            assert!(accent >= 3.0, "{}: accent on bg is {accent}", pack.id);
            let muted = color::contrast(pack.palette.muted, pack.palette.bg).unwrap_or(0.0);
            assert!(muted >= 3.0, "{}: muted on bg is {muted}", pack.id);
        }
    }

    #[test]
    fn the_packs_look_different_from_each_other() {
        for (i, a) in PACKS.iter().enumerate() {
            for b in &PACKS[i + 1..] {
                let mean = palette_distance(a, b).unwrap_or(0.0);
                assert!(mean >= 0.045, "{} and {} are too close: {mean}", a.id, b.id);
                let gap = palette_gap(a, b).unwrap_or(0.0);
                assert!(
                    gap >= 0.15,
                    "{} and {} differ too little: {gap}",
                    a.id,
                    b.id
                );
            }
        }
    }

    #[test]
    fn every_pack_declares_eight_tokens() {
        for pack in PACKS {
            let tokens = tokens(pack);
            assert_eq!(tokens.len(), 8, "{}", pack.id);
            assert_eq!(tokens[0].0, "bg");
            assert_eq!(tokens[5].0, "accent");
            assert_eq!(tokens[6].0, "accent_2");
            assert_eq!(tokens[7].0, "shadow");
            for (name, value) in tokens {
                assert!(Rgba::parse_hex(&value).is_ok(), "{} of {}", name, pack.id);
            }
        }
    }
}
