//! The MCP tool surface: thin wrappers over `core`, `render` and `store`. Arguments are parsed
//! into closed types and every rejection carries a reason the agent can act on
//! (`docs/wiki/agent-integration.md#the-mcp-server`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_core::{Actor, AnnotationStatus, Node, NodeId, NodeKind, OpError, Operation, Workspace};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

const DEFAULT_PREVIEW_SIDE: f64 = 1024.0;
const MAX_PREVIEW_SIDE: f64 = 2048.0;
const MAX_DEPTH_ARG: usize = 10;
const MAX_FIND_RESULTS: usize = 200;
const MAX_VALIDATE_FILES: usize = 100;
const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

/// A rejected tool call. The text goes back to the agent as the tool result.
#[derive(Debug)]
pub struct ToolError(pub String);

impl<E: std::fmt::Display + ToolErrorSource> From<E> for ToolError {
    fn from(e: E) -> Self {
        ToolError(e.to_string())
    }
}

/// Marker for the error types that may become a tool error.
pub trait ToolErrorSource {}
impl ToolErrorSource for OpError {}
impl ToolErrorSource for od_core::ProjectError {}
impl ToolErrorSource for od_render::RenderError {}
impl ToolErrorSource for std::io::Error {}

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub enum Content {
    Text(String),
    Image { data: String, mime: &'static str },
}

pub struct Output {
    pub content: Vec<Content>,
}

impl Output {
    fn text(s: impl Into<String>) -> Output {
        Output {
            content: vec![Content::Text(s.into())],
        }
    }

    fn json(v: &Value) -> Output {
        Output::text(v.to_string())
    }
}

/// Per-server bookkeeping that tool calls share: which agent turns already have a checkpoint.
#[derive(Default)]
pub struct Turns {
    checkpoint_by_turn: HashMap<String, u64>,
    counter: u64,
    seen: HashSet<String>,
}

pub struct CallCtx<'a> {
    pub ws: &'a mut Workspace,
    pub actor: Actor,
    pub turns: &'a mut Turns,
}

fn tool(name: &str, description: &str, props: Value, required: &[&str], read_only: bool) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": props,
            "required": required,
            "additionalProperties": false,
        },
        "annotations": { "readOnlyHint": read_only, "openWorldHint": false },
    })
}

/// The `tools/list` result.
pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    vec![
        tool(
            "get_guidance",
            "Read this first. Returns the pages, the nodes the user has selected (with their data), \
             the open notes the user pinned, and the edits the user made after your last turn. \
             'Make this bigger' means the selection.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_selection",
            "The nodes the user has selected, with their data.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_annotations",
            "The notes the user pinned on nodes or page regions.",
            json!({"include_resolved": {"type": "boolean", "description": "Also list resolved notes."}}),
            &[],
            true,
        ),
        tool(
            "resolve_annotation",
            "Mark a note resolved once you have acted on it. Say what you did in the user's terms.",
            json!({"id": {"type": "integer"}, "note": id("What you changed.")}),
            &["id"],
            false,
        ),
        tool(
            "list_pages",
            "Pages and their top-level nodes.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "get_node",
            "One node with its properties and, to `depth` levels, its children.",
            json!({"id": id("Node id, for example n_7."), "depth": {"type": "integer", "minimum": 0, "maximum": 10}}),
            &["id"],
            true,
        ),
        tool(
            "get_tree",
            "A page as an indented outline: id, kind, name, position and size.",
            json!({"page": id("Page id; defaults to the first page."), "depth": {"type": "integer", "minimum": 1, "maximum": 10}}),
            &[],
            true,
        ),
        tool(
            "find_nodes",
            "Find nodes by name and kind.",
            json!({
                "name_contains": {"type": "string"},
                "kind": {"type": "string", "enum": ["frame", "group", "rect", "ellipse", "text", "image"]},
                "limit": {"type": "integer", "minimum": 1, "maximum": 200}
            }),
            &[],
            true,
        ),
        tool(
            "list_tokens",
            "Design tokens: named colors usable as paints like {brand.gold}.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "apply_ops",
            "Change the design with a list of operations, applied together or not at all. \
             Operations: {op:'create', parent, index?, node:{kind, name?, x, y, width, height, fill?, stroke?, \
             corner_radius?, opacity?, visible?, text?, font_size?, image?, children?}}, \
             {op:'set', id, prop:{name, value}} with name one of name, x, y, width, height, visible, opacity, \
             fill, stroke, corner_radius, text, font_size, image, {op:'move', id, x, y}, \
             {op:'reparent', id, parent, index?}, {op:'delete', id}. Colors are '#rrggbb', '#rrggbbaa' or \
             '{token.name}'. Ids are minted for you; the result lists the created ids. A checkpoint is taken \
             before your first change in a turn so the user can restore it.",
            json!({
                "ops": {"type": "array", "minItems": 1, "maxItems": 1000, "items": {"type": "object"}},
                "turn": id("Groups related calls into one turn; omit and each call is its own turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["ops"],
            false,
        ),
        tool(
            "list_checkpoints",
            "Named points the design can be restored to.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "diff_checkpoint",
            "The nodes changed since a checkpoint (those that still exist).",
            json!({"checkpoint": {"type": "integer"}}),
            &["checkpoint"],
            true,
        ),
        tool(
            "restore_checkpoint",
            "Return the design to a checkpoint. This appends an undoable change; nothing is lost.",
            json!({"checkpoint": {"type": "integer"}}),
            &["checkpoint"],
            false,
        ),
        tool(
            "list_export_presets",
            "Store export presets: sizes, formats and rules.",
            json!({}),
            &[],
            true,
        ),
        tool(
            "render_preview",
            "Render a frame to an image so you can look at the result.",
            json!({
                "frame": id("Id of a frame node."),
                "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
                "max_side": {"type": "integer", "minimum": 16, "maximum": 2048}
            }),
            &["frame"],
            true,
        ),
        tool(
            "export",
            "Render a frame to exports/<file> in the project, validated against a store preset. \
             Nothing is written when the image does not meet the preset.",
            json!({
                "frame": id("Id of a frame node."),
                "file": id("File name ending in .png, for example shot-01.png."),
                "preset": id("A preset id from list_export_presets."),
                "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
                "alpha": {"type": "boolean", "description": "Keep transparency (ignored when the preset forbids it)."}
            }),
            &["frame", "file"],
            false,
        ),
        tool(
            "validate_export",
            "Check exported PNGs in exports/ as one set against a store preset.",
            json!({"preset": id("A preset id."), "files": {"type": "array", "items": {"type": "string"}, "maxItems": 100}}),
            &["preset", "files"],
            true,
        ),
    ]
}

fn parse<T: DeserializeOwned>(args: Value) -> Result<T, ToolError> {
    let args = if args.is_null() { json!({}) } else { args };
    serde_json::from_value(args).map_err(|e| ToolError(format!("invalid arguments: {e}")))
}

fn persist(ws: &Workspace) -> Result<(), ToolError> {
    if ws.dir().is_some() {
        ws.save()?;
    }
    Ok(())
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "get_guidance" => Ok(Output::json(&to_json(&ctx.ws.guidance())?)),
        "get_selection" => Ok(Output::json(&to_json(&ctx.ws.guidance().selection)?)),
        "get_annotations" => get_annotations(ctx, parse(args)?),
        "resolve_annotation" => resolve_annotation(ctx, parse(args)?),
        "list_pages" => list_pages(ctx),
        "get_node" => get_node(ctx, parse(args)?),
        "get_tree" => get_tree(ctx, parse(args)?),
        "find_nodes" => find_nodes(ctx, parse(args)?),
        "list_tokens" => Ok(Output::json(&to_json(&ctx.ws.session.document().tokens)?)),
        "apply_ops" => apply_ops(ctx, parse(args)?),
        "list_checkpoints" => Ok(Output::json(&to_json(ctx.ws.session.checkpoints())?)),
        "diff_checkpoint" => diff_checkpoint(ctx, parse(args)?),
        "restore_checkpoint" => restore_checkpoint(ctx, parse(args)?),
        "list_export_presets" => Ok(Output::json(&presets_json())),
        "render_preview" => render_preview(ctx, parse(args)?),
        "export" => export(ctx, parse(args)?),
        "validate_export" => validate_export(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

fn to_json<T: serde::Serialize + ?Sized>(v: &T) -> Result<Value, ToolError> {
    serde_json::to_value(v).map_err(|e| ToolError(e.to_string()))
}

fn node_id(s: &str) -> Result<NodeId, ToolError> {
    Ok(NodeId::parse(s)?)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AnnotationsArgs {
    #[serde(default)]
    include_resolved: bool,
}

fn get_annotations(ctx: &mut CallCtx, a: AnnotationsArgs) -> Result<Output, ToolError> {
    let list: Vec<&od_core::Annotation> = ctx
        .ws
        .annotations
        .all()
        .iter()
        .filter(|n| a.include_resolved || n.status == AnnotationStatus::Open)
        .collect();
    Ok(Output::json(&to_json(&list)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ResolveArgs {
    id: u64,
    #[serde(default)]
    note: Option<String>,
}

fn resolve_annotation(ctx: &mut CallCtx, a: ResolveArgs) -> Result<Output, ToolError> {
    ctx.ws
        .annotations
        .resolve(a.id, ctx.actor.clone(), a.note.as_deref())?;
    persist(ctx.ws)?;
    Ok(Output::text(format!("resolved annotation {}", a.id)))
}

fn node_summary(n: &Node) -> Value {
    json!({"id": n.id, "kind": n.kind.as_str(), "name": n.name,
           "x": n.x, "y": n.y, "width": n.width, "height": n.height})
}

fn list_pages(ctx: &mut CallCtx) -> Result<Output, ToolError> {
    let pages: Vec<Value> = ctx
        .ws
        .session
        .document()
        .pages
        .iter()
        .map(|p| {
            json!({
                "id": p.id, "name": p.name, "root": p.root.id,
                "nodes": p.root.children.iter().map(node_summary).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(Output::json(&Value::Array(pages)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeArgs {
    id: String,
    #[serde(default)]
    depth: Option<usize>,
}

fn node_json(n: &Node, depth: usize) -> Result<Value, ToolError> {
    let mut shallow = n.clone();
    let children = std::mem::take(&mut shallow.children);
    let mut v = to_json(&shallow)?;
    if let Value::Object(map) = &mut v {
        if depth > 0 {
            let kids: Result<Vec<Value>, ToolError> =
                children.iter().map(|c| node_json(c, depth - 1)).collect();
            if !children.is_empty() {
                map.insert("children".into(), Value::Array(kids?));
            }
        } else if !children.is_empty() {
            map.insert("child_count".into(), json!(children.len()));
        }
    }
    Ok(v)
}

fn get_node(ctx: &mut CallCtx, a: NodeArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.id)?;
    let doc = ctx.ws.session.document();
    let node = doc
        .node(&id)
        .ok_or_else(|| ToolError(OpError::NoSuchNode(a.id.clone()).to_string()))?;
    let depth = a.depth.unwrap_or(2).min(MAX_DEPTH_ARG);
    Ok(Output::json(&node_json(node, depth)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TreeArgs {
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    depth: Option<usize>,
}

fn outline(n: &Node, depth: usize, max: usize, out: &mut String) {
    out.push_str(&format!(
        "{}{} {} {:?} at {},{} {}x{}\n",
        "  ".repeat(depth),
        n.id,
        n.kind.as_str(),
        n.name,
        n.x,
        n.y,
        n.width,
        n.height
    ));
    if depth + 1 >= max {
        if !n.children.is_empty() {
            out.push_str(&format!("{}...\n", "  ".repeat(depth + 1)));
        }
        return;
    }
    for c in &n.children {
        outline(c, depth + 1, max, out);
    }
}

fn get_tree(ctx: &mut CallCtx, a: TreeArgs) -> Result<Output, ToolError> {
    let doc = ctx.ws.session.document();
    let page = match &a.page {
        Some(p) => doc
            .page(&node_id(p)?)
            .ok_or_else(|| ToolError(OpError::NoSuchPage(p.clone()).to_string()))?,
        None => doc
            .pages
            .first()
            .ok_or_else(|| ToolError("the project has no pages".into()))?,
    };
    let max = a.depth.unwrap_or(4).clamp(1, MAX_DEPTH_ARG);
    let mut out = format!("page {} {:?} (root {})\n", page.id, page.name, page.root.id);
    for c in &page.root.children {
        outline(c, 1, max + 1, &mut out);
    }
    Ok(Output::text(out))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindArgs {
    #[serde(default)]
    name_contains: Option<String>,
    #[serde(default)]
    kind: Option<NodeKind>,
    #[serde(default)]
    limit: Option<usize>,
}

fn collect(n: &Node, page: &NodeId, a: &FindArgs, limit: usize, out: &mut Vec<Value>) {
    if out.len() >= limit {
        return;
    }
    let name_ok = a
        .name_contains
        .as_ref()
        .is_none_or(|s| n.name.to_lowercase().contains(&s.to_lowercase()));
    if name_ok && a.kind.is_none_or(|k| k == n.kind) {
        let mut v = node_summary(n);
        if let Value::Object(m) = &mut v {
            m.insert("page".into(), json!(page));
        }
        out.push(v);
    }
    for c in &n.children {
        collect(c, page, a, limit, out);
    }
}

fn find_nodes(ctx: &mut CallCtx, a: FindArgs) -> Result<Output, ToolError> {
    let limit = a.limit.unwrap_or(50).min(MAX_FIND_RESULTS);
    let mut out = Vec::new();
    for page in &ctx.ws.session.document().pages {
        for c in &page.root.children {
            collect(c, &page.id, &a, limit, &mut out);
        }
    }
    Ok(Output::json(&Value::Array(out)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplyArgs {
    ops: Vec<Operation>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn apply_ops(ctx: &mut CallCtx, a: ApplyArgs) -> Result<Output, ToolError> {
    let turn = match a.turn {
        Some(t) => t,
        None => {
            ctx.turns.counter += 1;
            format!("t{}", ctx.turns.counter)
        }
    };
    let key = format!("{}/{turn}", ctx.actor);
    if ctx.turns.seen.insert(key.clone()) {
        let name = match &a.summary {
            Some(s) => format!("before: {s}"),
            None => format!("before {turn}"),
        };
        let name: String = name.chars().take(od_core::limits::MAX_NAME_LEN).collect();
        let id = ctx.ws.session.checkpoint(ctx.actor.clone(), &name)?;
        ctx.turns.checkpoint_by_turn.insert(key.clone(), id);
    }
    let checkpoint = ctx.turns.checkpoint_by_turn.get(&key).copied();
    let receipt = ctx
        .ws
        .session
        .apply(ctx.actor.clone(), Some(turn.clone()), &a.ops)?;
    persist(ctx.ws)?;
    Ok(Output::json(&json!({
        "transaction": receipt.id,
        "turn": turn,
        "checkpoint_before": checkpoint,
        "created": receipt.created,
        "touched": receipt.touched,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CheckpointArgs {
    checkpoint: u64,
}

fn diff_checkpoint(ctx: &mut CallCtx, a: CheckpointArgs) -> Result<Output, ToolError> {
    let position = ctx
        .ws
        .session
        .checkpoints()
        .iter()
        .find(|c| c.id == a.checkpoint)
        .map(|c| c.position)
        .ok_or_else(|| ToolError(OpError::NoSuchCheckpoint(a.checkpoint).to_string()))?;
    let changed = ctx.ws.session.changed_since(position);
    Ok(Output::json(
        &json!({"checkpoint": a.checkpoint, "changed": changed}),
    ))
}

fn restore_checkpoint(ctx: &mut CallCtx, a: CheckpointArgs) -> Result<Output, ToolError> {
    let receipt = ctx.ws.session.restore(ctx.actor.clone(), a.checkpoint)?;
    persist(ctx.ws)?;
    Ok(Output::json(&json!({
        "restored": a.checkpoint,
        "transaction": receipt.id,
        "touched": receipt.touched,
    })))
}

fn presets_json() -> Value {
    let list: Vec<Value> = od_store::PRESETS
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "description": p.description,
                "size": p.describe_size(),
                "formats": p.formats.iter().map(|f| f.name()).collect::<Vec<_>>(),
                "allow_alpha": p.allow_alpha,
                "min_count": p.min_count,
                "max_count": p.max_count,
                "source": p.source,
                "verified_against_primary_source":
                    matches!(p.verification, od_store::Verification::PrimaryChecked(_)),
            })
        })
        .collect();
    Value::Array(list)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewArgs {
    frame: String,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    max_side: Option<u32>,
}

fn render_preview(ctx: &mut CallCtx, a: PreviewArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.frame)?;
    let doc = ctx.ws.session.document();
    let node = doc
        .node(&id)
        .ok_or_else(|| ToolError(OpError::NoSuchNode(a.frame.clone()).to_string()))?;
    let scale = match a.scale {
        Some(s) => s,
        None => {
            let side = a
                .max_side
                .map_or(DEFAULT_PREVIEW_SIDE, f64::from)
                .min(MAX_PREVIEW_SIDE);
            let longest = node.width.max(node.height).max(1.0);
            // render sizes round up, so aim half a pixel under the limit
            if longest <= side {
                1.0
            } else {
                ((side - 0.5) / longest) as f32
            }
        }
    };
    let rendered = od_render::render_frame(doc, &id, scale)?;
    let png = od_render::encode_png(&rendered.pixmap, false)?;
    let mut note = format!(
        "{}x{} preview of {} at scale {scale:.3}",
        rendered.pixmap.width(),
        rendered.pixmap.height(),
        a.frame
    );
    if !rendered.warnings.is_empty() {
        note.push_str(&format!(
            "; not drawn yet: {}",
            rendered
                .warnings
                .iter()
                .map(|w| format!("{w:?}"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(Output {
        content: vec![
            Content::Image {
                data: STANDARD.encode(png),
                mime: "image/png",
            },
            Content::Text(note),
        ],
    })
}

/// File names only: `shot-01.png`. No separators, no leading dot, no traversal.
fn export_name(file: &str) -> Result<&str, ToolError> {
    let ok = file.len() <= 100
        && file.ends_with(".png")
        && !file.contains("..")
        && file
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && file
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok {
        Ok(file)
    } else {
        fail("file must be a plain name like shot-01.png (letters, digits . _ -)")
    }
}

fn exports_dir(ws: &Workspace) -> Result<PathBuf, ToolError> {
    match ws.dir() {
        Some(d) => Ok(d.join("exports")),
        None => fail("the project has no folder on disk, so there is nowhere to export to"),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExportArgs {
    frame: String,
    file: String,
    #[serde(default)]
    preset: Option<String>,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    alpha: bool,
}

fn preset(id: &str) -> Result<&'static od_store::Preset, ToolError> {
    od_store::find(id).ok_or_else(|| {
        let ids: Vec<&str> = od_store::PRESETS.iter().map(|p| p.id).collect();
        ToolError(format!(
            "unknown preset {id:?}; available: {}",
            ids.join(", ")
        ))
    })
}

/// What an export wrote.
pub struct ExportOutcome {
    /// Relative to the project folder, for example `exports/shot-01.png`.
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub not_drawn: Vec<String>,
    pub preset_verified: Option<bool>,
}

/// Renders `frame` into `<project>/exports/<file>`, validated against `preset` when given. Nothing
/// is written when the image does not meet the preset. Shared by the `export` tool and the editor.
pub fn export_frame(
    ws: &Workspace,
    frame: &NodeId,
    file: &str,
    preset_id: Option<&str>,
    scale: Option<f32>,
    alpha: bool,
) -> Result<ExportOutcome, ToolError> {
    let file = export_name(file)?;
    let dir = exports_dir(ws)?;
    let target = preset_id.map(preset).transpose()?;
    let rendered = od_render::render_frame(ws.session.document(), frame, scale.unwrap_or(1.0))?;
    let keep_alpha = alpha && target.is_none_or(|p| p.allow_alpha);
    let png = od_render::encode_png(&rendered.pixmap, keep_alpha)?;
    if let Some(p) = target {
        let facts = od_store::inspect_png(&png).map_err(|e| ToolError(e.to_string()))?;
        let violations = p.validate_image(&facts);
        if !violations.is_empty() {
            let list: Vec<String> = violations.iter().map(|v| format!("- {v}")).collect();
            return fail(format!(
                "not written: the export does not meet {} ({}):\n{}",
                p.id,
                p.source,
                list.join("\n")
            ));
        }
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(file), &png)?;
    Ok(ExportOutcome {
        path: format!("exports/{file}"),
        width: rendered.pixmap.width(),
        height: rendered.pixmap.height(),
        bytes: png.len(),
        not_drawn: rendered.warnings.iter().map(|w| format!("{w:?}")).collect(),
        preset_verified: target
            .map(|p| matches!(p.verification, od_store::Verification::PrimaryChecked(_))),
    })
}

fn export(ctx: &mut CallCtx, a: ExportArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.frame)?;
    let out = export_frame(ctx.ws, &id, &a.file, a.preset.as_deref(), a.scale, a.alpha)?;
    Ok(Output::json(&json!({
        "path": out.path,
        "width": out.width,
        "height": out.height,
        "bytes": out.bytes,
        "preset": a.preset,
        "preset_verified_against_primary_source": out.preset_verified,
        "not_drawn": out.not_drawn,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidateArgs {
    preset: String,
    files: Vec<String>,
}

fn validate_export(ctx: &mut CallCtx, a: ValidateArgs) -> Result<Output, ToolError> {
    if a.files.len() > MAX_VALIDATE_FILES {
        return fail(format!("at most {MAX_VALIDATE_FILES} files"));
    }
    let p = preset(&a.preset)?;
    let dir = exports_dir(ctx.ws)?;
    let mut facts = Vec::with_capacity(a.files.len());
    for f in &a.files {
        let path = dir.join(export_name(f)?);
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_IMAGE_BYTES {
            return fail(format!("{f} is larger than {MAX_IMAGE_BYTES} bytes"));
        }
        let bytes = std::fs::read(&path)?;
        facts.push(od_store::inspect_png(&bytes).map_err(|e| ToolError(format!("{f}: {e}")))?);
    }
    let violations = p.validate_set(&facts);
    let list: Vec<Value> = violations
        .iter()
        .map(|(i, v)| match i {
            Some(i) => json!({"file": a.files[*i], "problem": v.to_string()}),
            None => json!({"file": null, "problem": v.to_string()}),
        })
        .collect();
    Ok(Output::json(&json!({
        "preset": p.id,
        "ok": violations.is_empty(),
        "violations": list,
        "source": p.source,
    })))
}
