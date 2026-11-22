//! Scene seeds (`docs/wiki/agent-integration.md#scene-seeds`): six starter scenes built from the
//! same blocks `add_block` adds, so an agent starts from a composed, moving, planned scene
//! instead of an empty frame.
//!
//! `apply_seed` is a deterministic recipe run as ONE attributed turn: a frame, a video with one
//! clip, the blocks (each through the `add_block` code path), ambient motion and a camera drift,
//! and a matching SceneSpec (layer plan, beat map, palette). If any step fails the turn is rolled
//! back to its checkpoint, so a seed never leaves half a scene behind.

mod recipes;
mod spec;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use od_core::{NodeId, NodeKind, Operation, Rgba};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::scene_spec::{self, schema::Format, status, validate};
use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops_receipt, node_id, parse, tool,
};
use recipes::{Brand, Recipe, SEEDS, Seed, Step};

pub const NAMES: [&str; 2] = ["list_seeds", "apply_seed"];

const DEFAULT_SIZE: (u32, u32, u32) = (1280, 720, 30);
const FRAME_GAP: f64 = 100.0;
const DEFAULT_TOKENS: [(&str, &str); 4] = [
    ("bg", "#0d0f14"),
    ("fg", "#f5f6f8"),
    ("accent", "#7c6cff"),
    ("muted", "#9aa3b2"),
];

pub fn definitions() -> Vec<Value> {
    let ids: Vec<&str> = SEEDS.iter().map(|s| s.id).collect();
    let text = |d: &str| json!({"type": "string", "description": d});
    vec![
        tool(
            "list_seeds",
            "The six scene seeds: starter scenes built from blocks (explainer opener with scenery and \
             a character, product reveal with a 3D phone, kinetic type quote, stats callout, before \
             and after, logo sting). Each says what it builds, how long it is, the beats it is \
             planned around and its layout notes. apply_seed builds one. Read get_playbook seeds.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "apply_seed",
            "Build a seed as ONE attributed, undoable turn: a frame, a video with one clip, the seed's \
             blocks (the add_block code path, with the applied style pack's tokens when one is set), \
             ambient motion and a slow camera drift, and a SceneSpec in scene_specs/ with the layer \
             plan, beat map and palette. Returns the layouts notes, the beat times (place sounds by \
             beat name), the open P0 gates and the next call. Applying twice makes a second, \
             separate scene. Then swap the copy, check with validate_document and review_video, and \
             record the review.",
            json!({
                "id": {"type": "string", "enum": ids, "description": "A seed id from list_seeds."},
                "video": text("Id of an existing EMPTY video to put the scene in (its size is used). Omit to create one (the workspace's chosen video format, else 1280x720 at 30 fps)."),
                "brand": {"type": "object", "description": "Copy and colors to swap in; a seed's own words fill what you leave out. Colors are #rrggbb.",
                    "properties": {
                        "name": text("Brand or speaker name (eyebrow, wordmark or name plate)."),
                        "headline": text("The main line: headline, quote, stat label or gain label."),
                        "tagline": text("The supporting line: bubble, tagline or role."),
                        "cta": text("Button label."),
                        "accent": text("Accent color."),
                        "background": text("Frame background color."),
                        "foreground": text("Text color."),
                    },
                    "additionalProperties": false},
            }),
            &["id"],
            false,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "list_seeds" => {
            let _: Empty = parse(args)?;
            Ok(Output::json(&list()))
        }
        "apply_seed" => apply(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyArgsIn {
    id: String,
    #[serde(default)]
    video: Option<String>,
    #[serde(default)]
    brand: Option<Brand>,
}

fn list() -> Value {
    let seeds: Vec<Value> = SEEDS
        .iter()
        .map(|s| {
            let recipe = (s.recipe)(&Brand::default());
            let blocks: Vec<&str> = recipe
                .steps
                .iter()
                .filter_map(|st| match st {
                    Step::Block { block, .. } => Some(*block),
                    Step::Device { .. } => Some("device3d"),
                    Step::Move { .. } => None,
                })
                .collect();
            json!({
                "id": s.id,
                "name": s.name,
                "about": s.about,
                "duration_ms": s.duration_ms,
                "blocks": blocks,
                "beats": recipe.beats.iter().map(|b| json!({"id": b.id, "at_ms": b.at, "takeaway": b.takeaway})).collect::<Vec<_>>(),
                "layouts": s.layouts,
            })
        })
        .collect();
    json!({"count": seeds.len(), "seeds": seeds, "next": "apply_seed {\"id\": ...}"})
}

fn fail<T>(m: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(m.into()))
}

/// Replaces every `"@layer"` string in `v` with the node id built for that layer.
fn resolve(v: &Value, roots: &BTreeMap<String, String>) -> Result<Value, ToolError> {
    Ok(match v {
        Value::String(s) => {
            match s.strip_prefix('@') {
                Some(layer) => Value::String(roots.get(layer).cloned().ok_or_else(|| {
                    ToolError(format!("seed bug: layer {layer:?} is not built yet"))
                })?),
                None => v.clone(),
            }
        }
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, x)| Ok((k.clone(), resolve(x, roots)?)))
                .collect::<Result<_, ToolError>>()?,
        ),
        other => other.clone(),
    })
}

fn text_of(out: &Output) -> Result<Value, ToolError> {
    match out.content.first() {
        Some(Content::Text(t)) => serde_json::from_str(t).map_err(|e| ToolError(e.to_string())),
        _ => fail("a seed step returned no text"),
    }
}

struct Geo {
    width: f64,
    height: f64,
}

impl Geo {
    fn area(&self, a: [f64; 4]) -> Value {
        let r = |v: f64| (v * 10.0).round() / 10.0;
        json!({
            "x": r(a[0] * self.width), "y": r(a[1] * self.height),
            "width": r(a[2] * self.width), "height": r(a[3] * self.height),
        })
    }
}

/// How the blocks are colored: tokens of the applied style pack, plus any brand colors.
struct Look {
    colors: Option<Value>,
    frame_fill: String,
    tokens: Vec<(String, String)>,
    style_pack: Option<String>,
}

fn look(ctx: &CallCtx, brand: &Brand) -> Result<Look, ToolError> {
    for (what, v) in [
        ("accent", &brand.accent),
        ("background", &brand.background),
        ("foreground", &brand.foreground),
    ] {
        if let Some(c) = v
            && Rgba::parse_hex(c).is_err()
        {
            return fail(format!("brand.{what} {c:?} is not a color; use #rrggbb"));
        }
    }
    let pack = ctx
        .ws
        .dir()
        .and_then(|d| crate::formats::load_brief(d).style_pack)
        .and_then(|p| crate::style_pack::find(&p.id));
    let doc_tokens = &ctx.ws.session.document().tokens;
    let mut tokens: Vec<(String, String)> = match pack {
        Some(p) => crate::style_pack::tokens(p)
            .into_iter()
            .map(|(n, v)| (n.to_string(), v))
            .collect(),
        None => DEFAULT_TOKENS
            .iter()
            .map(|(n, v)| ((*n).to_string(), (*v).to_string()))
            .collect(),
    };
    let tokened = pack.is_some()
        && ["bg", "fg", "accent", "muted"]
            .iter()
            .all(|t| doc_tokens.contains_key(*t));
    let mut colors = serde_json::Map::new();
    if tokened {
        for (role, token) in [
            ("background", "bg"),
            ("foreground", "fg"),
            ("accent", "accent"),
            ("muted", "muted"),
        ] {
            colors.insert(role.into(), json!(format!("{{{token}}}")));
        }
    }
    for (role, token, given) in [
        ("accent", "accent", &brand.accent),
        ("background", "bg", &brand.background),
        ("foreground", "fg", &brand.foreground),
    ] {
        if let Some(c) = given {
            colors.insert(role.into(), json!(c));
            if let Some(t) = tokens.iter_mut().find(|(n, _)| n == token) {
                t.1.clone_from(c);
            }
        }
    }
    let frame_fill = match (&brand.background, tokened) {
        (Some(c), _) => c.clone(),
        (None, true) => "{bg}".to_string(),
        (None, false) => DEFAULT_TOKENS[0].1.to_string(),
    };
    Ok(Look {
        colors: (!colors.is_empty()).then_some(Value::Object(colors)),
        frame_fill,
        tokens,
        style_pack: pack.map(|p| p.id.to_string()),
    })
}

/// The video the scene goes in: an empty one the caller names, or a new one in the workspace's
/// chosen video format.
enum Target {
    Existing(String, Format),
    New(Format),
}

fn target(ctx: &CallCtx, video: Option<&str>) -> Result<Target, ToolError> {
    let doc = ctx.ws.session.document();
    if let Some(v) = video {
        let id = node_id(v)?;
        let found = doc.video(&id).ok_or_else(|| {
            ToolError(format!("video {v} does not exist; list_videos shows them"))
        })?;
        if !found.clips.is_empty() {
            return fail(format!(
                "video {v} already has clips; a seed builds a whole scene, so give it an empty video or omit `video` to make one"
            ));
        }
        return Ok(Target::Existing(
            v.to_string(),
            Format {
                width: found.width,
                height: found.height,
                fps: found.fps,
            },
        ));
    }
    let chosen = ctx
        .ws
        .dir()
        .and_then(|d| crate::formats::load_brief(d).format)
        .filter(|f| f.kind == "video");
    Ok(Target::New(match chosen {
        Some(f) => Format {
            width: f.width,
            height: f.height,
            fps: f.fps.unwrap_or(DEFAULT_SIZE.2),
        },
        None => Format {
            width: DEFAULT_SIZE.0,
            height: DEFAULT_SIZE.1,
            fps: DEFAULT_SIZE.2,
        },
    }))
}

struct Built {
    frame: String,
    video: String,
    roots: BTreeMap<String, String>,
    checkpoint: Option<u64>,
}

struct Turn<'a> {
    seed: &'a Seed,
    turn: String,
    summary: String,
}

impl Turn<'_> {
    fn ops(&self, ctx: &mut CallCtx, ops: Vec<Operation>) -> Result<Value, ToolError> {
        apply_ops_receipt(
            ctx,
            ApplyArgs {
                ops,
                turn: Some(self.turn.clone()),
                summary: Some(self.summary.clone()),
            },
        )
    }
}

fn first_created(receipt: &Value) -> Result<String, ToolError> {
    receipt["created"]
        .as_array()
        .and_then(|c| c.first())
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or_else(|| ToolError("a seed step created nothing".into()))
}

#[allow(clippy::too_many_arguments)]
fn build(
    ctx: &mut CallCtx,
    t: &Turn,
    recipe: &Recipe,
    geo: &Geo,
    look: &Look,
    target: &Target,
    frame_name: &str,
    built: &mut Built,
) -> Result<(), ToolError> {
    let doc = ctx.ws.session.document();
    let root = doc.pages[0].root.id.clone();
    let right = doc.pages[0]
        .root
        .children
        .iter()
        .filter(|n| n.kind == NodeKind::Frame)
        .map(|n| n.x + n.width)
        .fold(0.0_f64, f64::max);
    let x = if right > 0.0 { right + FRAME_GAP } else { 0.0 };
    let mut ops = vec![Operation::Create {
        parent: root,
        index: None,
        node: serde_json::from_value(json!({
            "kind": "frame", "name": frame_name, "x": x, "y": 0,
            "width": geo.width, "height": geo.height, "fill": look.frame_fill,
        }))
        .map_err(|e| ToolError(e.to_string()))?,
    }];
    if let Target::New(f) = target {
        ops.push(Operation::CreateVideo {
            name: t.seed.name.to_string(),
            fps: f.fps,
            width: f.width,
            height: f.height,
        });
    }
    let receipt = t.ops(ctx, ops)?;
    built.checkpoint = receipt["checkpoint_before"].as_u64();
    let doc = ctx.ws.session.document();
    let frame = doc.pages[0]
        .root
        .children
        .iter()
        .rfind(|n| n.name == frame_name)
        .map(|n| n.id.clone())
        .ok_or_else(|| ToolError("the seed's frame was not created".into()))?;
    let video: NodeId = match target {
        Target::Existing(v, _) => node_id(v)?,
        Target::New(_) => doc
            .videos
            .last()
            .map(|v| v.id.clone())
            .ok_or_else(|| ToolError("the seed's video was not created".into()))?,
    };
    built.frame = frame.to_string();
    built.video = video.to_string();
    t.ops(
        ctx,
        vec![Operation::AddClip {
            video: video.clone(),
            frame: Some(frame.clone()),
            footage: None,
            duration_ms: u32::try_from(t.seed.duration_ms).map_err(|e| ToolError(e.to_string()))?,
            index: None,
            transition: None,
            motion: None,
        }],
    )?;

    for step in &recipe.steps {
        match step {
            Step::Block {
                layer,
                block,
                at,
                area,
                params,
                ..
            } => {
                let mut args = json!({
                    "block": block, "frame": built.frame, "video": built.video,
                    "start_ms": at, "params": resolve(params, &built.roots)?,
                    "turn": t.turn, "summary": t.summary,
                });
                if let Some(a) = area {
                    args["box"] = geo.area(*a);
                }
                if let Some(c) = &look.colors {
                    args["colors"] = c.clone();
                }
                let out = crate::blocks::call(ctx, "add_block", args)
                    .map_err(|e| ToolError(format!("{block}: {}", e.0)))?;
                let root = text_of(&out)?["root"]
                    .as_str()
                    .map(String::from)
                    .ok_or_else(|| ToolError(format!("{block} built no root")))?;
                built.roots.insert((*layer).to_string(), root);
            }
            Step::Device {
                layer,
                model,
                place,
                ..
            } => {
                let tpl = crate::motion::call(
                    ctx,
                    "device3d_template",
                    json!({"model": model, "screen_width": 300}),
                )?;
                let mut node = text_of(&tpl)?["node"].clone();
                let (w, h) = (
                    node["width"].as_f64().unwrap_or(1.0),
                    node["height"].as_f64().unwrap_or(1.0),
                );
                let scale = place[2] * geo.height / h.max(1.0);
                node["width"] = json!((w * scale).round());
                node["height"] = json!((h * scale).round());
                node["x"] = json!((place[0] * geo.width).round());
                node["y"] = json!((place[1] * geo.height).round());
                node["name"] = json!("Device");
                let receipt = t.ops(
                    ctx,
                    vec![Operation::Create {
                        parent: frame.clone(),
                        index: None,
                        node: serde_json::from_value(node).map_err(|e| ToolError(e.to_string()))?,
                    }],
                )?;
                built
                    .roots
                    .insert((*layer).to_string(), first_created(&receipt)?);
            }
            Step::Move {
                layer,
                preset,
                at,
                duration,
                strength,
                ..
            } => {
                let mut args = json!({
                    "video": built.video, "preset": preset, "start_ms": at,
                    "turn": t.turn, "summary": t.summary,
                });
                if let Some(l) = layer {
                    args["node"] =
                        json!(built.roots.get(*l).ok_or_else(|| ToolError(format!(
                            "seed bug: layer {l:?} is not built"
                        )))?);
                }
                if let Some(d) = duration {
                    args["duration_ms"] = json!(d);
                }
                if let Some(s) = strength {
                    args["strength"] = json!(s);
                }
                crate::motion::call(ctx, "animate_preset", args)
                    .map_err(|e| ToolError(format!("{preset}: {}", e.0)))?;
            }
        }
    }
    Ok(())
}

fn apply(ctx: &mut CallCtx, a: ApplyArgsIn) -> Result<Output, ToolError> {
    let Some(seed) = recipes::find(&a.id) else {
        return fail(format!(
            "no seed {:?}; the seeds are {}",
            a.id,
            SEEDS.iter().map(|s| s.id).collect::<Vec<_>>().join(", ")
        ));
    };
    let brand = a.brand.unwrap_or_default();
    let look = look(ctx, &brand)?;
    let target = target(ctx, a.video.as_deref())?;
    let format = match &target {
        Target::Existing(_, f) | Target::New(f) => f.clone(),
    };
    let doc = ctx.ws.session.document();
    let taken = doc.pages[0]
        .root
        .children
        .iter()
        .filter(|n| n.name.starts_with(seed.name))
        .count();
    let frame_name = format!("{} {}", seed.name, taken + 1);
    let recipe = (seed.recipe)(&brand);
    let t = Turn {
        seed,
        turn: format!(
            "seed-{}-{}",
            seed.id,
            ctx.ws.session.checkpoints().len() + 1
        ),
        summary: format!("apply the {} seed", seed.name.to_lowercase()),
    };
    let geo = Geo {
        width: f64::from(format.width),
        height: f64::from(format.height),
    };
    let mut built = Built {
        frame: String::new(),
        video: String::new(),
        roots: BTreeMap::new(),
        checkpoint: None,
    };
    if let Err(e) = build(
        ctx,
        &t,
        &recipe,
        &geo,
        &look,
        &target,
        &frame_name,
        &mut built,
    ) {
        let rolled = match built.checkpoint {
            Some(c) => {
                crate::tools::call(ctx, "restore_checkpoint", json!({"checkpoint": c})).is_ok()
            }
            None => true,
        };
        return fail(format!(
            "the {} seed could not be built: {}{}",
            seed.name,
            e.0,
            if rolled {
                "; everything it did was rolled back"
            } else {
                "; rolling back failed, restore_checkpoint to the last 'before' checkpoint"
            }
        ));
    }

    let facts = spec::Facts {
        seed,
        recipe: &recipe,
        video: &built.video,
        format,
        duration_ms: seed.duration_ms,
        roots: &built.roots,
        tokens: look.tokens.clone(),
        style_pack: look.style_pack.as_deref(),
    };
    let (spec_name, scene) = write_spec(ctx, &facts)?;
    let report = validate::validate(&scene);
    let findings = crate::doctools::lint_workspace(ctx.ws, Vec::new());
    let (errors, warnings, _) = od_core::lint::counts(&findings);
    let (strict_open, status_json) = match (ctx.ws.dir(), &spec_name) {
        (Some(dir), Some(n)) => (
            report
                .quality
                .iter()
                .map(|i| format!("{}: {}", i.path, i.message))
                .collect::<Vec<_>>(),
            Some(status::status(&scene, dir, n)),
        ),
        _ => (Vec::new(), None),
    };
    let video = built.video.clone();
    let checklist = json!([
        {"gate": "document lints clean", "tool": "validate_document",
         "status": if errors == 0 { "pass" } else { "fail" },
         "detail": format!("{errors} errors, {warnings} warnings at build time")},
        {"gate": "scene spec is valid", "tool": "scene_spec_validate",
         "status": if report.ok(false) { "pass" } else { "fail" },
         "detail": report.errors.iter().map(|i| format!("{}: {}", i.path, i.message)).collect::<Vec<_>>()},
        {"gate": "scene spec passes strict (a build needs it)", "tool": "scene_spec_validate",
         "status": if report.ok(true) { "pass" } else { "open" }, "detail": strict_open},
        {"gate": "no picture backdrop, more than text moves (RV001, RV002)", "tool": "review_video",
         "status": "run", "call": {"tool": "review_video", "arguments": {"video": video}}},
        {"gate": "copy, colors and logo are the project's own", "tool": "apply_ops", "status": "todo",
         "detail": "replace the placeholder words; keep to the palette tokens"},
        {"gate": "you looked at it", "tool": "video_contact_sheet", "status": "todo",
         "detail": "one contact sheet, then record_review"},
        {"gate": "sound under the beats", "tool": "make_sound", "status": "todo",
         "detail": "place each cue at its beat time below"},
    ]);
    let beat_times = scene_spec::beats::beat_times(&scene);
    Ok(Output::json(&json!({
        "seed": seed.id,
        "name": seed.name,
        "frame": built.frame,
        "video": built.video,
        "layers": built.roots,
        "spec": spec_name.as_ref().map(|n| format!("{}/{n}.json", scene_spec::FOLDER)),
        "spec_note": if spec_name.is_none() { "this workspace has no project folder, so the SceneSpec was not saved" } else { "saved" },
        "beat_times": beat_times,
        "layouts": seed.layouts,
        "p0_gates": checklist,
        "status": status_json,
        "undo": "one turn: undo it, or restore_checkpoint to the 'before' checkpoint, to remove the whole scene",
        "next": match &spec_name {
            Some(n) => json!({"tool": "scene_status", "arguments": {"name": n}}),
            None => json!({"tool": "review_video", "arguments": {"video": built.video}}),
        },
    })))
}

/// Saves the spec under a name no earlier seed used; `None` when the workspace has no folder.
fn write_spec(
    ctx: &CallCtx,
    facts: &spec::Facts,
) -> Result<(Option<String>, scene_spec::schema::SceneSpec), ToolError> {
    let Some(dir) = ctx.ws.dir() else {
        return Ok((None, spec::build(facts, facts.seed.id)));
    };
    for n in 1..=99 {
        let name = if n == 1 {
            facts.seed.id.to_string()
        } else {
            format!("{}-{n}", facts.seed.id)
        };
        if scene_spec::load(dir, &name).is_ok() {
            continue;
        }
        let scene = spec::build(facts, &name);
        scene_spec::save(dir, &name, &scene)?;
        return Ok((Some(name), scene));
    }
    fail("99 specs of this seed already exist; delete some from scene_specs/")
}
