//! The `import_svg_vector` tool: an SVG (inline, a file in `media/`, or an installed asset) becomes a
//! group of shape nodes in one attributed, undoable change (`docs/wiki/agent-integration.md#vector-import`).
//! The conversion is [`crate::svg_vector`]; this adds where the SVG comes from and the operation.

use std::path::Path;

use od_core::{Document, Node, NodeId, Operation};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::assets::index::{self, Payload};
use crate::assets::packs;
use crate::assets::project::{self, Sidecar};
use crate::svg_vector::{self, Imported, MAX_SVG_BYTES, Options};
use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops, node_id, parse, tool,
};

pub const NAMES: [&str; 1] = ["import_svg_vector"];

pub fn definitions() -> Vec<Value> {
    let text = |d: &str| json!({"type": "string", "description": d});
    vec![tool(
        "import_svg_vector",
        "Bring an SVG illustration in as editable vector nodes: one node per shape (a path, rect or \
         ellipse with its fill, gradient, stroke and opacity), placed exactly as drawn, inside one group, \
         as a single undoable change. Parts named by SVG ids become node names, and an SVG group with an \
         id or an opacity stays a group, so an arm, an eye or a wheel can be found with find_nodes and \
         moved, recolored or animated on its own. Give exactly one source: `svg` (the whole SVG text), \
         `file` (media/<name>.svg) or `asset` (an id from search_assets, an installed pack's illustration \
         or a Lucide icon; its credit line is kept). Pictures, scripts, outside references and drawings \
         over 500 shapes are refused; text, clip paths, masks, filters and pattern fills are not imported \
         and are listed in `notes`. `width` scales the whole drawing; the result gives its size and the \
         id of every part. Use import_svg instead for a logo or icon that should stay one picture.",
        json!({
            "parent": text("Id of the frame or group to put the drawing in."),
            "svg": text("The whole SVG document, with a viewBox."),
            "file": text("media/<name>.svg of the project."),
            "asset": text("An asset id from search_assets (lucide:<icon> or <pack>:<file>)."),
            "name": text("Name of the group (default: the SVG's own id, else 'illustration')."),
            "x": {"type": "number", "description": "Where the drawing's top-left goes in the parent (default 0)."},
            "y": {"type": "number", "description": "Where the drawing's top-left goes in the parent (default 0)."},
            "width": {"type": "number", "exclusiveMinimum": 0, "maximum": 16384, "description": "Width of the whole drawing in document units; the height follows (default: the SVG's own size)."},
            "index": {"type": "integer", "minimum": 0, "description": "Position among the parent's children (default: on top)."},
            "turn": text("Groups related calls into one turn; omit and each call is its own turn."),
            "summary": text("One line, in the user's terms, naming what this turn does.")
        }),
        &["parent"],
        false,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "import_svg_vector" => import(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

/// What to import, where it goes and how it is placed. The same for the tool and the command line.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub parent: String,
    #[serde(default)]
    pub svg: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub x: Option<f64>,
    #[serde(default)]
    pub y: Option<f64>,
    #[serde(default)]
    pub width: Option<f64>,
    #[serde(default)]
    pub index: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    #[serde(flatten)]
    request: Request,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

/// The SVG text and, for an installed asset, where it came from.
pub struct Source {
    pub text: String,
    pub credit: Option<Value>,
}

/// An import ready to apply: the operation, and what it will make.
pub struct Plan {
    pub ops: Vec<Operation>,
    pub imported: Imported,
}

fn fail(e: impl ToString) -> ToolError {
    ToolError(e.to_string())
}

/// Reads the SVG the request names. Exactly one of `svg`, `file`, `asset`.
pub fn read_source(project_dir: Option<&Path>, r: &Request) -> Result<Source, ToolError> {
    match (&r.svg, &r.file, &r.asset) {
        (Some(svg), None, None) => Ok(Source {
            text: svg.clone(),
            credit: None,
        }),
        (None, Some(file), None) => Ok(Source {
            text: read_media_svg(project_dir, file)?,
            credit: None,
        }),
        (None, None, Some(id)) => read_asset(project_dir, id),
        _ => Err(fail("give exactly one of svg, file or asset")),
    }
}

fn read_media_svg(project_dir: Option<&Path>, file: &str) -> Result<String, ToolError> {
    let dir = project_dir.ok_or_else(|| fail("the project has no folder"))?;
    let name = file.strip_prefix("media/").unwrap_or(file);
    if name.contains(['/', '\\']) || name.starts_with('.') || !name.ends_with(".svg") {
        return Err(fail("file is media/<name>.svg"));
    }
    let path = dir.join("media").join(name);
    let meta = std::fs::symlink_metadata(&path).map_err(fail)?;
    if !meta.is_file() || meta.len() > MAX_SVG_BYTES as u64 {
        return Err(fail(format!(
            "{file} is not a plain file of at most {MAX_SVG_BYTES} bytes"
        )));
    }
    std::fs::read_to_string(&path).map_err(fail)
}

/// An asset's SVG. Its credit line goes into the project beside the SVG, like any asset's.
fn read_asset(project_dir: Option<&Path>, id: &str) -> Result<Source, ToolError> {
    let root = packs::root().map_err(fail)?;
    let found = index::resolve(&root, id).map_err(fail)?;
    let Payload::Svg(text) = found.payload else {
        return Err(fail(format!("{id} is not an SVG drawing")));
    };
    let mut credit = json!({
        "source": found.source, "licence": found.licence,
        "attribution_required": found.attribution_required,
    });
    if found.attribution_required {
        credit["attribution"] = json!(found.attribution);
    }
    if let Some(dir) = project_dir {
        let sidecar = Sidecar {
            id: found.id.clone(),
            source: found.source.clone(),
            licence: found.licence.clone(),
            licence_url: found.licence_url.clone(),
            attribution_required: found.attribution_required,
            attribution: found.attribution.clone(),
            recipe: None,
        };
        let placed = project::place(dir, &found.name, "svg", text.as_bytes(), &sidecar, None)
            .map_err(fail)?;
        credit["file"] = json!(placed.file);
    }
    Ok(Source {
        text,
        credit: Some(credit),
    })
}

/// The `create` operation for `source`, checked against `doc` for the parent.
pub fn plan(doc: &Document, source: &Source, r: &Request) -> Result<Plan, ToolError> {
    let parent = node_id(&r.parent)?;
    if doc.node(&parent).is_none() {
        return Err(fail(format!("node {} does not exist", r.parent)));
    }
    let options = Options {
        name: r.name.clone(),
        x: r.x.unwrap_or(0.0),
        y: r.y.unwrap_or(0.0),
        width: r.width,
    };
    let imported = svg_vector::import(&source.text, &options).map_err(fail)?;
    let ops = vec![Operation::Create {
        parent,
        index: r.index,
        node: imported.node.clone(),
    }];
    Ok(Plan { ops, imported })
}

/// The report of an import: the group, its size, every part with its id, and what was left out.
pub fn report(imported: &Imported, group: &Node, credit: Option<&Value>) -> Value {
    fn parts(node: &Node, out: &mut Vec<Value>) {
        out.push(json!({"id": node.id, "name": node.name, "kind": node.kind.as_str()}));
        for child in &node.children {
            parts(child, out);
        }
    }
    let mut list = Vec::new();
    parts(group, &mut list);
    let mut v = json!({
        "group": group.id,
        "width": imported.size.0,
        "height": imported.size.1,
        "nodes": list.len(),
        "parts": list,
        "notes": imported.notes,
        "next": "render_preview the frame to check it, then move, recolor or animate the parts by id.",
    });
    if let Some(c) = credit {
        v["credit"] = c.clone();
    }
    v
}

fn import(ctx: &mut CallCtx, a: Args) -> Result<Output, ToolError> {
    let dir = ctx.ws.dir().map(Path::to_path_buf);
    let source = read_source(dir.as_deref(), &a.request)?;
    let plan = plan(ctx.ws.session.document(), &source, &a.request)?;
    let summary = a.summary.unwrap_or_else(|| {
        format!(
            "import {}",
            plan.imported.node.name.clone().unwrap_or_default()
        )
    });
    let applied = apply_ops(
        ctx,
        ApplyArgs {
            ops: plan.ops.clone(),
            turn: a.turn,
            summary: Some(summary),
        },
    )?;
    let Some(Content::Text(text)) = applied.content.first() else {
        return Err(fail("the import was applied but gave no receipt"));
    };
    let receipt: Value = serde_json::from_str(text).map_err(fail)?;
    let created: Vec<NodeId> = serde_json::from_value(receipt["created"].clone()).map_err(fail)?;
    let group = created
        .first()
        .and_then(|id| ctx.ws.session.document().node(id))
        .ok_or_else(|| fail("the imported group is missing"))?;
    let mut v = report(&plan.imported, group, source.credit.as_ref());
    for key in ["transaction", "checkpoint_before", "saved"] {
        v[key] = receipt[key].clone();
    }
    Ok(Output::json(&v))
}
