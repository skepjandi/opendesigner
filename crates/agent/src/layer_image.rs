//! `layer_image`: one picture cut into depth layers an agent can move in a camera move
//! (`docs/wiki/agent-integration.md#layer_image`).
//!
//! The picture comes from `media/`. The grey plane that says what is near comes from the user's
//! own programs, as everything else that needs a model does here: a subject mask from
//! `remove_background` with `mask_only`, or a depth map from a depth program (brighter is
//! nearer). The cutting itself is plain pixel work in `planes`; nothing is downloaded or
//! uploaded. Each layer is a PNG in `media/` and an image node of one group, with a `depth` so
//! the video camera moves the near ones further than the far ones.

mod planes;

use std::path::{Path, PathBuf};

use od_core::{NewNode, NodeKind, Operation};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ApplyArgs, CallCtx, Output, ToolError, apply_ops_receipt, node_id, parse, tool,
};

pub const NAMES: [&str; 1] = ["layer_image"];

const MAX_PIXELS: u64 = 24_000_000;
const MAX_BANDS: u8 = 6;
const DEFAULT_GROW: u32 = 40;
const MAX_GROW: u32 = 400;
const MAX_FEATHER: u32 = 8;
const FAR_DEPTH: f64 = 0.15;
const DEFAULT_OVERSCAN: f64 = 0.06;

pub fn definitions() -> Vec<Value> {
    let text = |d: &str| json!({"type": "string", "description": d});
    vec![tool(
        "layer_image",
        "Cut a picture into depth layers for a camera move (a 2.5D parallax): the far layer first, the near layer last, each an image node of one group with a `depth` (0.15 far to 1 near), so camera_pan, camera_push_in or camera_drift move the near ones more. Give the picture (`file`, media/<name>) and exactly one grey plane from the user's own programs: `mask` (media/<name>, a subject mask or a cut-out with transparency, from remove_background with mask_only: two layers, the background and the subject) or `depth` (media/<name>, a depth map where brighter is nearer, or `invert_depth` when darker is nearer: `bands` layers of equal area). Each layer also carries `grow` pixels of the colours behind the nearer ones, grown out of its own edge, so a move does not uncover a hole, and the lowest layer fills the whole picture. The layers are overscanned by `overscan` (default 6 percent) so the camera has room. Layers are written to media/<name>-layer-<n>.png and never overwrite a file. Pictures over 24 million pixels are refused. Model licences are the user's to check: when the project is commercial, ask for rembg's isnet-general-use or u2net, and a depth model such as Depth Anything V2 Small (Apache-2.0), not the larger non-commercial ones.",
        json!({
            "file": text("media/<name> of the picture (png, jpg or webp)."),
            "mask": text("media/<name> of a subject mask or a cut-out: two layers."),
            "depth": text("media/<name> of a depth map (brighter is nearer): `bands` layers."),
            "invert_depth": {"type": "boolean", "description": "The depth map is darker where nearer."},
            "bands": {"type": "integer", "minimum": 2, "maximum": MAX_BANDS, "description": "How many layers a depth map is cut into (default 4)."},
            "mask_threshold": {"type": "integer", "minimum": 1, "maximum": 255, "description": "How bright a mask pixel must be to count as the subject (default 128)."},
            "grow": {"type": "integer", "minimum": 0, "maximum": MAX_GROW, "description": "Pixels of hidden colour grown behind each nearer layer (default 40)."},
            "feather": {"type": "integer", "minimum": 0, "maximum": MAX_FEATHER, "description": "Pixels an outer edge is softened over (default 1)."},
            "parent": text("Id of the frame or group to put the layers in."),
            "name": text("Name of the group (default: the picture's name)."),
            "x": {"type": "number", "description": "Left of the group in the parent (default 0)."},
            "y": {"type": "number", "description": "Top of the group in the parent (default 0)."},
            "width": {"type": "number", "exclusiveMinimum": 0, "maximum": 16384, "description": "Width of the picture in document units (default: the parent's width); the height follows."},
            "overscan": {"type": "number", "minimum": 0, "maximum": 0.3, "description": "Extra size around the picture, as a share of it (default 0.06)."},
            "turn": text("Groups related calls into one turn; omit and each call is its own turn."),
            "summary": text("One line, in the user's terms, naming what this turn does.")
        }),
        &["file", "parent"],
        false,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "layer_image" => layer_tool(ctx, parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    file: String,
    #[serde(default)]
    mask: Option<String>,
    #[serde(default)]
    depth: Option<String>,
    #[serde(default)]
    invert_depth: bool,
    #[serde(default)]
    bands: Option<u8>,
    #[serde(default)]
    mask_threshold: Option<u8>,
    #[serde(default)]
    grow: Option<u32>,
    #[serde(default)]
    feather: Option<u32>,
    parent: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    width: Option<f64>,
    #[serde(default)]
    overscan: Option<f64>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

fn plain_media(project: &Path, reference: &str) -> Result<PathBuf, ToolError> {
    let name = reference
        .strip_prefix("media/")
        .filter(|n| !n.is_empty() && !n.contains(['/', '\\']) && !n.starts_with('.'))
        .ok_or_else(|| ToolError(format!("{reference:?} is not a media/<file> reference")))?;
    let path = project.join("media").join(name);
    match std::fs::symlink_metadata(&path) {
        Ok(m) if m.is_file() => Ok(path),
        Ok(_) => fail(format!("{reference} is not a plain file")),
        Err(e) => fail(format!("{reference}: {e}")),
    }
}

fn stem(reference: &str) -> String {
    let file = reference.rsplit('/').next().unwrap_or(reference);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    stem.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .take(48)
        .collect()
}

fn free_stem(media: &Path, stem: &str, bands: u8) -> String {
    (1..)
        .map(|n| {
            if n == 1 {
                stem.to_string()
            } else {
                format!("{stem}-{n}")
            }
        })
        .find(|s| (0..bands).all(|k| !media.join(format!("{s}-layer-{k}.png")).exists()))
        .unwrap_or_default()
}

fn encode(rgba: &[u8], w: u32, h: u32) -> Result<Vec<u8>, ToolError> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| ToolError(e.to_string()))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| ToolError(e.to_string()))?;
    writer.finish().map_err(|e| ToolError(e.to_string()))?;
    Ok(out)
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ToolError> {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|e| ToolError(format!("{}: {e}", path.display())))
}

fn layer_tool(ctx: &mut CallCtx, a: Args) -> Result<Output, ToolError> {
    let project = ctx
        .ws
        .dir()
        .map(Path::to_path_buf)
        .ok_or_else(|| ToolError("the project has no folder, so no media/".into()))?;
    let parent = node_id(&a.parent)?;
    let parent_node = ctx
        .ws
        .session
        .document()
        .node(&parent)
        .ok_or_else(|| ToolError(format!("no node {parent}")))?;
    let default_width = parent_node.width;
    let (plane_file, invert) = match (&a.mask, &a.depth) {
        (Some(m), None) => (m.clone(), false),
        (None, Some(d)) => (d.clone(), a.invert_depth),
        _ => return fail("give exactly one of mask or depth"),
    };
    let picture = plain_media(&project, &a.file)?;
    let plane_path = plain_media(&project, &plane_file)?;
    let (w, h, rgba) = od_render::decode_rgba(&picture, MAX_PIXELS).map_err(ToolError)?;
    let (pw, ph, plane_rgba) =
        od_render::decode_rgba(&plane_path, MAX_PIXELS).map_err(ToolError)?;
    let grey = planes::grey_plane(&plane_rgba, (pw, ph), (w, h), invert);
    let count = if a.mask.is_some() {
        2
    } else {
        a.bands.unwrap_or(4).clamp(2, MAX_BANDS)
    };
    let bands = if a.mask.is_some() {
        planes::bands_from_mask(&grey, a.mask_threshold.unwrap_or(128))
    } else {
        planes::bands_from_depth(&grey, count)
    };
    let sizes = planes::band_sizes(&bands, count);
    if sizes.iter().filter(|&&n| n > 0).count() < 2 {
        return fail(
            "the plane has only one level, so there is nothing to separate: check the mask or depth map (and `invert_depth`)",
        );
    }
    let reach = planes::Reach {
        grow: a.grow.unwrap_or(DEFAULT_GROW).min(MAX_GROW),
        feather: a.feather.unwrap_or(1).min(MAX_FEATHER),
    };
    let media = project.join("media");
    let base = free_stem(&media, &stem(&a.file), count);
    let mut files = Vec::new();
    for k in 0..count {
        let pixels = planes::layer(&rgba, (w, h), &bands, k, reach);
        let name = format!("media/{base}-layer-{k}.png");
        write_new(
            &media.join(format!("{base}-layer-{k}.png")),
            &encode(&pixels, w, h)?,
        )?;
        files.push(name);
    }
    let manifest = json!({
        "source": a.file, "plane": plane_file, "bands": count,
        "grow": reach.grow, "feather": reach.feather, "layers": files,
        "processed": "locally; nothing was uploaded",
    });
    let manifest_text =
        serde_json::to_string_pretty(&manifest).map_err(|e| ToolError(e.to_string()))?;
    write_new(
        &media.join(format!("{base}-layers.json")),
        manifest_text.as_bytes(),
    )?;

    let overscan = a.overscan.unwrap_or(DEFAULT_OVERSCAN).clamp(0.0, 0.3);
    let width = a.width.unwrap_or(if default_width > 0.0 {
        default_width
    } else {
        f64::from(w)
    });
    let height = width * f64::from(h) / f64::from(w);
    let (extra_w, extra_h) = (width * overscan, height * overscan);
    let mut group = NewNode::new(NodeKind::Group);
    group.name = Some(a.name.clone().unwrap_or_else(|| base.clone()));
    group.x = a.x.unwrap_or(0.0);
    group.y = a.y.unwrap_or(0.0);
    group.width = width;
    group.height = height;
    for (k, file) in files.iter().enumerate() {
        let mut node = NewNode::new(NodeKind::Image);
        node.name = Some(format!("{base} layer {k}"));
        node.x = -extra_w / 2.0;
        node.y = -extra_h / 2.0;
        node.width = width + extra_w;
        node.height = height + extra_h;
        node.image = Some(file.clone());
        node.image_fit = Some(od_core::ImageFit::Fill);
        node.depth = Some(FAR_DEPTH + (1.0 - FAR_DEPTH) * k as f64 / f64::from(count - 1));
        group.children.push(node);
    }
    let receipt = apply_ops_receipt(
        ctx,
        ApplyArgs {
            ops: vec![Operation::Create {
                parent,
                index: None,
                node: group,
            }],
            turn: a.turn,
            summary: Some(
                a.summary
                    .unwrap_or_else(|| format!("cut {} into {count} depth layers", a.file)),
            ),
        },
    )?;
    Ok(Output::json(&json!({
        "layers": files,
        "bands": count,
        "band_pixels": sizes,
        "depths": (0..count).map(|k| FAR_DEPTH + (1.0 - FAR_DEPTH) * f64::from(k) / f64::from(count - 1)).collect::<Vec<_>>(),
        "overscan": overscan,
        "manifest": format!("media/{base}-layers.json"),
        "created": receipt["created"],
        "next": "move it with animate_preset camera_pan, camera_push_in or camera_drift (a camera move needs the layers at their depths); check it with video_contact_sheet and look for edges or holes with video_crop",
    })))
}

#[cfg(test)]
mod tests;
