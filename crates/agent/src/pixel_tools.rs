//! The tools for pixel-art game assets: `export_atlas` writes a sprite sheet or atlas with its JSON
//! manifest, `quantize_image` snaps a picture to a palette (`docs/wiki/store-assets.md#game-assets`).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::atlas::{self, AtlasPlan, FrameMeta, NineSlice, Pivot, Source};
use crate::quantize::{self, PaletteSpec};
use crate::tools::{CallCtx, Output, ToolError, node_id, parse, tool};

pub const NAMES: [&str; 2] = ["export_atlas", "quantize_image"];

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    let int = |d: &str, max: u32| json!({"type": "integer", "minimum": 1, "maximum": max, "description": d});
    let pivot = json!({
        "type": "object",
        "properties": {"x": {"type": "number"}, "y": {"type": "number"}},
        "required": ["x", "y"], "additionalProperties": false,
        "description": "Where the frame's origin is, in pixels from the cell's top-left corner."
    });
    let nine = json!({
        "type": "object",
        "properties": {
            "l": {"type": "integer", "minimum": 0}, "t": {"type": "integer", "minimum": 0},
            "r": {"type": "integer", "minimum": 0}, "b": {"type": "integer", "minimum": 0}
        },
        "required": ["l", "t", "r", "b"], "additionalProperties": false,
        "description": "9-slice margins in pixels: the corners that stay fixed when the frame is stretched."
    });
    vec![
        tool(
            "export_atlas",
            "Write a sprite sheet or atlas for a game: exports/<name>.png and exports/<name>.json, a \
             manifest {image, size:{w,h}, frames:[{name, x, y, w, h, pivot?, nine_slice?}], palette?, \
             generator, created}. Two ways: GRID cuts ONE frame into `cols` x `rows` cells of \
             `cell_w` x `cell_h` output pixels (the frame must be exactly that big at `scale`), for \
             example a 144x192 frame of 3 x 4 cells of 48x48 is a character sheet with 3 frames per \
             direction and 4 directions; cells are listed row by row. PACK takes a list of `frames` \
             (each one a frame node, such as a 32x32 icon) and lays them in a grid of `columns`, every \
             cell as big as the biggest frame. Transparency is always kept. `names` gives every cell \
             its name (otherwise <name>-00, -01, ... for a grid and the frame's node name when \
             packing); `pivot` and `nine_slice` apply to every frame, `frame_meta` overrides them \
             per name. Everything is rendered and checked before anything is written. Pass \
             `pixel_art: true` for hard pixel edges (no anti-aliasing, nearest-neighbour images) and \
             use an integer `scale`.",
            json!({
                "name": id("File name stem, letters, digits . _ - (gives hero.png and hero.json)."),
                "frame": id("GRID: id of the frame to cut."),
                "cols": int("GRID: cells per row.", 1024),
                "rows": int("GRID: rows of cells.", 1024),
                "cell_w": int("GRID: width of a cell in output pixels.", atlas::MAX_SIDE),
                "cell_h": int("GRID: height of a cell in output pixels.", atlas::MAX_SIDE),
                "frames": {"type": "array", "items": id("Id of a frame node."), "minItems": 1, "maxItems": atlas::MAX_CELLS, "description": "PACK: the frames to pack, in order."},
                "columns": int("PACK: cells per row (default 8).", 1024),
                "padding": {"type": "integer", "minimum": 0, "maximum": 64, "description": "PACK: transparent pixels around every cell (default 0)."},
                "names": {"type": "array", "items": {"type": "string"}, "description": "One name per cell, row by row (grid) or in order (pack)."},
                "pivot": pivot,
                "nine_slice": nine,
                "frame_meta": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "properties": {"pivot": pivot, "nine_slice": nine},
                        "additionalProperties": false
                    },
                    "description": "Per frame name: pivot and nine_slice that replace the defaults."
                },
                "scale": {"type": "number", "minimum": 0.01, "maximum": 16},
                "pixel_art": {"type": "boolean", "description": "No anti-aliasing and nearest-neighbour images (default false)."}
            }),
            &["name"],
            false,
        ),
        tool(
            "quantize_image",
            "Snap a picture in media/ to a palette: every pixel becomes the nearest palette colour \
             (nearest in OKLab) and a pixel with alpha under `alpha_threshold` becomes fully \
             transparent. Writes a NEW file media/<name>-q.png and leaves the original alone; use it \
             as an image node (with image_smoothing nearest). `palette` is a list of token names \
             and #rrggbb colours, or one style pack id from list_style_packs. Returns the size, how \
             many colours the picture had and has, and the pixels per colour. No dithering.",
            json!({
                "file": id("media/<name>.png (or .jpg, .webp) from list_media."),
                "palette": {
                    "oneOf": [
                        {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": quantize::MAX_PALETTE, "description": "Token names (brand.gold) and #rrggbb colours."},
                        {"type": "string", "description": "A style pack id."}
                    ]
                },
                "alpha_threshold": {"type": "integer", "minimum": 1, "maximum": 255, "description": "Alpha at or above which a pixel stays (default 128)."},
                "dither": {"type": "string", "enum": ["none"], "description": "Only none: flat colours."}
            }),
            &["file", "palette"],
            false,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "export_atlas" => export_atlas(ctx, parse(args)?),
        "quantize_image" => quantize_image(ctx, parse(args)?),
        other => fail(format!("unknown tool {other:?}")),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AtlasArgs {
    name: String,
    #[serde(default)]
    frame: Option<String>,
    #[serde(default)]
    cols: Option<u32>,
    #[serde(default)]
    rows: Option<u32>,
    #[serde(default)]
    cell_w: Option<u32>,
    #[serde(default)]
    cell_h: Option<u32>,
    #[serde(default)]
    frames: Option<Vec<String>>,
    #[serde(default)]
    columns: Option<u32>,
    #[serde(default)]
    padding: Option<u32>,
    #[serde(default)]
    names: Option<Vec<String>>,
    #[serde(default)]
    pivot: Option<Pivot>,
    #[serde(default)]
    nine_slice: Option<NineSlice>,
    #[serde(default)]
    frame_meta: BTreeMap<String, FrameMeta>,
    #[serde(default)]
    scale: Option<f32>,
    #[serde(default)]
    pixel_art: bool,
}

fn export_atlas(ctx: &mut CallCtx, a: AtlasArgs) -> Result<Output, ToolError> {
    let frame_id;
    let frame_ids;
    let source = match (&a.frame, &a.frames) {
        (Some(frame), None) => {
            let (Some(cols), Some(rows), Some(cell_w), Some(cell_h)) =
                (a.cols, a.rows, a.cell_w, a.cell_h)
            else {
                return fail("a grid needs frame, cols, rows, cell_w and cell_h");
            };
            frame_id = node_id(frame)?;
            Source::Grid {
                frame: &frame_id,
                cols,
                rows,
                cell_w,
                cell_h,
            }
        }
        (None, Some(frames)) => {
            frame_ids = frames
                .iter()
                .map(|f| node_id(f))
                .collect::<Result<Vec<_>, _>>()?;
            Source::Frames {
                frames: &frame_ids,
                columns: a.columns.unwrap_or(8),
                padding: a.padding.unwrap_or(0),
            }
        }
        _ => return fail("give either `frame` (cut a grid) or `frames` (pack them), not both"),
    };
    let plan = AtlasPlan {
        name: &a.name,
        source,
        names: a.names.as_deref(),
        default_meta: FrameMeta {
            pivot: a.pivot,
            nine_slice: a.nine_slice,
        },
        overrides: &a.frame_meta,
        scale: a.scale.unwrap_or(1.0),
        pixel_art: a.pixel_art,
    };
    let out = atlas::export_atlas(ctx.ws, &plan)?;
    let manifest = serde_json::to_value(&out.manifest).map_err(|e| ToolError(e.to_string()))?;
    Ok(Output::json(&json!({
        "image": out.image_path,
        "manifest_path": out.manifest_path,
        "width": out.manifest.size.w,
        "height": out.manifest.size.h,
        "bytes": out.bytes,
        "frame_count": out.manifest.frames.len(),
        "manifest": manifest,
        "warnings": out.warnings,
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuantizeArgs {
    file: String,
    palette: PaletteSpec,
    #[serde(default)]
    alpha_threshold: Option<u8>,
    /// Only `none` exists; accepted so an agent can say so.
    #[serde(default)]
    dither: Option<String>,
}

fn quantize_image(ctx: &mut CallCtx, a: QuantizeArgs) -> Result<Output, ToolError> {
    let Some(dir) = ctx.ws.dir() else {
        return fail("the project has no folder on disk, so there is nowhere to put the picture");
    };
    if let Some(dither) = a.dither.as_deref().filter(|d| *d != "none") {
        return fail(format!(
            "dither {dither:?} does not exist: quantize_image only writes flat colours (dither \"none\")"
        ));
    }
    let palette = quantize::resolve_palette(&a.palette, &ctx.ws.session.document().tokens)?;
    let threshold = a
        .alpha_threshold
        .unwrap_or(quantize::DEFAULT_ALPHA_THRESHOLD);
    if threshold == 0 {
        return fail("alpha_threshold must be between 1 and 255");
    }
    let report = quantize::quantize_file(dir, &a.file, &palette, threshold)?;
    let v = serde_json::to_value(&report).map_err(|e| ToolError(e.to_string()))?;
    Ok(Output::json(&v))
}
