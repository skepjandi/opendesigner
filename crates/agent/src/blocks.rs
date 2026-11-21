//! A catalog of reusable motion and design blocks (`docs/wiki/store-assets.md#blocks`). A block is
//! a named, parameterized, deterministic generator: it expands into ordinary nodes and, where it
//! has motion, ordinary `animate` keyframes, all applied as one attributed, undoable transaction
//! through the same path as `apply_ops`. Nothing in a document says a block was ever used except
//! the name prefix `block:<id>#<n>` on the group it makes, so a block can be found, edited as
//! plain nodes, or deleted whole.

mod args;
mod art;
mod backdrops;
mod bubbles;
mod character_motion;
mod characters;
mod data;
mod files;
mod guides;
mod kit;
mod land;
mod micro;
mod overlays;
mod plan;
mod props;
mod props_science;
mod registry;
mod scene;
mod sky;
mod social;
mod titles;
mod wrappers;

#[cfg(test)]
mod character_tests;
#[cfg(test)]
mod scenery_tests;
#[cfg(test)]
mod tests;

use od_core::OpError;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, ToolErrorSource, apply_ops, parse, tool,
};

pub use plan::{BoxArgs, Colors, Plan, Request, plan, write_assets};
pub use registry::{Block, Category, all, find, search};

pub const NAMES: [&str; 3] = ["list_blocks", "get_block", "add_block"];

/// A block that cannot be made. The text goes back to the agent.
#[derive(Debug, Error)]
pub enum BlockError {
    #[error("{0}")]
    Param(String),
    #[error("{0}")]
    Fit(String),
    #[error("{0}")]
    Asset(String),
    #[error("{0}")]
    Target(String),
    #[error("block bug: {0}")]
    Internal(String),
    #[error(transparent)]
    Op(#[from] OpError),
}

impl ToolErrorSource for BlockError {}

pub fn definitions() -> Vec<Value> {
    let categories: Vec<&str> = Category::ALL.iter().map(|c| c.as_str()).collect();
    let text = |d: &str| json!({"type": "string", "description": d});
    vec![
        tool(
            "list_blocks",
            "The block catalog: ready-made titles, lower thirds, overlays, backgrounds, charts, store \
             headlines, motion wrappers, rigged flat-vector characters, speech and thought bubbles and animated props that expand into ordinary nodes (and keyframes on a video). \
             Search by what you need (`query`: words matched against names, tags and descriptions) \
             and/or `category`. Each result has the block id, its category and what it is for. Then \
             get_block for its parameters and add_block to use it. Scenery blocks (sky_gradient, \
             parallax_hills, starfield, clouds, sun_glow, ocean_waves, city_skyline, cells_backdrop, \
             particles, grid_floor, connector, highlight_ring) build layered, always-moving \
             flat-vector scenes: read get_playbook `scenery` first.",
            json!({
                "query": text("Words describing what you need, e.g. 'lower third name' or 'bar chart'."),
                "category": {"type": "string", "enum": categories},
            }),
            &[],
            true,
        ),
        tool(
            "get_block",
            "One block's full description: what it makes, every parameter (type, range or options, \
             default), the default box it lays itself out in, whether it moves, and the standard \
             parameters every block takes.",
            json!({"block": text("A block id from list_blocks.")}),
            &["block"],
            true,
        ),
        tool(
            "add_block",
            "Add a block to a frame as one undoable step. It makes a group named `block:<id>#<n>` \
             with named parts inside (find them with get_tree, change them with apply_ops, delete the \
             group to remove the block); with a `video` it also keyframes the parts using the same \
             animations as animate_preset. Colors come from the document's tokens (or tokens the block \
             creates once, named block.background/foreground/accent/muted; set_token changes them \
             everywhere) unless you pass `colors`. The block's defaults are placeholders: write your \
             own copy and colors, then check the result with render_preview and review_frame. The \
             result lists the created root and its parts. Use `dry_run` to see the operations without \
             applying them.",
            json!({
                "block": text("A block id from list_blocks."),
                "frame": text("Id of the frame to put it in (optional when the document has one frame)."),
                "video": text("Id of a video from list_videos to keyframe in (optional when there is one video; without one the block is static)."),
                "start_ms": {"type": "integer", "minimum": 0, "description": "When it starts, from the start of the video (default 0)."},
                "hold_ms": {"type": "integer", "minimum": 0, "description": "Fade the block out this long after start_ms (needs a video; at least as long as its entrance). Omit to keep it on screen."},
                "box": {"type": "object", "description": "Where it lays itself out, in frame coordinates; omitted sides use the block's default box.",
                    "properties": {"x": {"type": "number"}, "y": {"type": "number"}, "width": {"type": "number"}, "height": {"type": "number"}},
                    "additionalProperties": false},
                "colors": {"type": "object", "description": "Override the roles with #rrggbb or a {token}.",
                    "properties": {"background": {"type": "string"}, "foreground": {"type": "string"}, "accent": {"type": "string"}, "muted": {"type": "string"}},
                    "additionalProperties": false},
                "font": text("One font family for all of the block's text (default: the bundled one)."),
                "params": {"type": "object", "description": "The block's own parameters (see get_block)."},
                "dry_run": {"type": "boolean", "description": "Return the operations without applying them."},
                "turn": text("Groups related calls into one turn; omit and each call is its own turn."),
                "summary": text("One line, in the user's terms, naming what this turn does."),
            }),
            &["block"],
            false,
        ),
    ]
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    category: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GetArgs {
    block: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddArgs {
    block: String,
    #[serde(default)]
    frame: Option<String>,
    #[serde(default)]
    video: Option<String>,
    #[serde(default)]
    start_ms: Option<u64>,
    #[serde(default)]
    hold_ms: Option<u64>,
    #[serde(default, rename = "box")]
    region: Option<BoxArgs>,
    #[serde(default)]
    colors: Option<Colors>,
    #[serde(default)]
    font: Option<String>,
    #[serde(default)]
    params: Option<Map<String, Value>>,
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "list_blocks" => list(parse(args)?),
        "get_block" => describe(parse(args)?),
        "add_block" => add(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

fn list(a: ListArgs) -> Result<Output, ToolError> {
    let category = a
        .category
        .as_deref()
        .map(|c| {
            Category::parse(c).ok_or_else(|| {
                ToolError(format!(
                    "no category {c:?}; categories: {}",
                    Category::ALL.map(|c| c.as_str()).join(", ")
                ))
            })
        })
        .transpose()?;
    let found = search(a.query.as_deref().unwrap_or(""), category);
    Ok(Output::json(&json!({
        "count": found.len(),
        "blocks": found.iter().map(|b| b.summary()).collect::<Vec<_>>(),
        "next": "get_block for a block's parameters, add_block to use it",
    })))
}

fn describe(a: GetArgs) -> Result<Output, ToolError> {
    let block = find(&a.block)?;
    Ok(Output::json(&block.describe()))
}

fn add(ctx: &mut CallCtx, a: AddArgs) -> Result<Output, ToolError> {
    let request = Request {
        block: a.block,
        frame: a.frame,
        video: a.video,
        start_ms: a.start_ms,
        hold_ms: a.hold_ms,
        region: a.region,
        colors: a.colors,
        font: a.font,
        params: a.params,
    };
    let plan = plan(ctx.ws.session.document(), &request)?;
    if a.dry_run {
        let mut v = plan.report();
        v["dry_run"] = json!(true);
        v["ops"] = json!(plan.ops);
        return Ok(Output::json(&v));
    }
    if !plan.assets.is_empty() {
        let dir = ctx.ws.dir().ok_or_else(|| {
            ToolError(format!(
                "{} needs pictures in the project's media folder, and this workspace has no project folder",
                plan.block
            ))
        })?;
        write_assets(dir, &plan)?;
    }
    let summary = a.summary.unwrap_or_else(|| format!("add {}", plan.name));
    let applied = apply_ops(
        ctx,
        ApplyArgs {
            ops: plan.ops.clone(),
            turn: a.turn,
            summary: Some(summary),
        },
    )?;
    let mut v = plan.report();
    if let Some(Content::Text(t)) = applied.content.first()
        && let Ok(Value::Object(receipt)) = serde_json::from_str(t)
    {
        v["transaction"] = receipt["transaction"].clone();
        v["checkpoint_before"] = receipt["checkpoint_before"].clone();
    }
    Ok(Output::json(&v))
}
