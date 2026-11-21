//! MCP tools for motion: named 3D and 2D moves an agent asks for instead of writing keyframes by
//! hand, and a contact sheet to judge a move from several moments at once
//! (`docs/wiki/agent-integration.md#motion`).
//!
//! A preset is turned into ordinary `animate` operations by `od_core::animation::preset_ops` and
//! applied through the same path as `apply_ops`, so it is attributed, checkpointed and undoable.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use od_core::animation::{self, Preset, PresetRequest};
use od_core::{Ease, Operation};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{
    ApplyArgs, CallCtx, Content, Output, ToolError, apply_ops, node_id, parse, tool,
};

pub const NAMES: [&str; 3] = ["animate_preset", "video_contact_sheet", "device3d_template"];

const MAX_SHEET_PICTURES: usize = 24;

fn names<T: serde::Serialize>(all: &[T]) -> Vec<Value> {
    all.iter()
        .filter_map(|v| serde_json::to_value(v).ok())
        .collect()
}

pub fn definitions() -> Vec<Value> {
    let id = |d: &str| json!({"type": "string", "description": d});
    let eases = [
        Ease::Linear,
        Ease::EaseIn,
        Ease::EaseOut,
        Ease::EaseInOut,
        Ease::Back,
        Ease::Spring,
        Ease::EaseInOutCubic,
        Ease::EaseOutBack,
        Ease::Elastic,
        Ease::Bounce,
    ];
    vec![
        tool(
            "animate_preset",
            "Animate a node with a named move, measured from its designed pose, as keyframes you can \
             then inspect in list_videos and change with apply_ops animate. 3D device moves: spin (a \
             full turn; strength 2 is two), turntable (slow side to side), swing (away one way, then \
             the other, back), flip_in (turns from its back to face the viewer, springing), \
             hero_reveal (starts flat and square, settles into the designed tilt and turn), push_in \
             and pull_back (camera closer or settling back), orbit (the view swings around the \
             device), scroll_screen (scrolls a long capture top to bottom). Any node: rise_in, \
             drop_in, float (gentle drift; a 3D device also sways), fade_in, fade_out, \
             slide_in_left, slide_in_right. Keys already inside start_ms..start_ms+duration_ms for \
             the same properties are replaced and the rest kept, so moves chain. Times are from the \
             start of the video; the node should be in a frame the video shows at that time.",
            json!({
                "video": id("Id of a video from list_videos."),
                "node": id("Id of the node to move (a device3d for the 3D moves)."),
                "preset": {"type": "string", "enum": names(&Preset::ALL)},
                "start_ms": {"type": "integer", "minimum": 0},
                "duration_ms": {"type": "integer", "minimum": 100, "maximum": 600000},
                "strength": {"type": "number", "minimum": animation::MIN_STRENGTH, "maximum": animation::MAX_STRENGTH, "description": "How far the move goes; 1 is as designed."},
                "ease": {"type": "string", "enum": names(&eases), "description": "Overrides the preset's own ease."},
                "turn": id("Groups related calls into one turn; omit and each call is its own turn."),
                "summary": id("One line, in the user's terms, naming what this turn does.")
            }),
            &["video", "node", "preset", "start_ms", "duration_ms"],
            false,
        ),
        tool(
            "device3d_template",
            "A device in 3D (phone, tablet, laptop or monitor) as a node you can put in an \
             apply_ops create operation. The node's box is the viewport the device is drawn in; \
             the device is fitted into it with a margin, so it never runs out of the frame when \
             it turns. Give it the capture to show on the screen. Change it afterwards with \
             apply_ops set (model, rotate_x, rotate_y, rotate_z, zoom, fov, light_angle, shadow, \
             glare, screen_scroll, screen_footage).",
            json!({
                "model": {"type": "string", "enum": ["phone", "phone_punch_hole", "tablet", "laptop", "monitor"]},
                "screen_width": {"type": "number", "minimum": 50, "maximum": 8192, "description": "Width of the viewport in document units."},
                "image": id("A media/ path from list_media: the capture on the screen."),
                "rotate_y": {"type": "number", "minimum": -3600, "maximum": 3600, "description": "Turn in degrees; positive turns the right edge away."},
                "rotate_x": {"type": "number", "minimum": -3600, "maximum": 3600, "description": "Tilt in degrees; positive leans the top away."}
            }),
            &["model", "screen_width"],
            true,
        ),
        tool(
            "video_contact_sheet",
            "Several moments of a video in one image, in a grid read left to right, top to bottom: \
             the way to judge a move (its arc, its timing, where it settles) instead of one still. \
             Give `times` or let it spread `count` moments evenly from `from_ms` to `to_ms` (the whole \
             video by default).",
            json!({
                "video": id("Id of a video from list_videos."),
                "times": {"type": "array", "items": {"type": "integer", "minimum": 0}, "minItems": 1, "maxItems": MAX_SHEET_PICTURES},
                "count": {"type": "integer", "minimum": 2, "maximum": MAX_SHEET_PICTURES},
                "from_ms": {"type": "integer", "minimum": 0},
                "to_ms": {"type": "integer", "minimum": 0},
                "cell_side": {"type": "integer", "minimum": 64, "maximum": 512, "description": "Longest side of each picture (default 256)."},
                "locale": id("A language from list_locales.")
            }),
            &["video"],
            true,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "animate_preset" => animate_preset(ctx, parse(args)?),
        "video_contact_sheet" => contact_sheet(ctx, parse(args)?),
        "device3d_template" => device3d_template(parse(args)?),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeviceArgs {
    model: String,
    screen_width: f64,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    rotate_y: Option<f64>,
    #[serde(default)]
    rotate_x: Option<f64>,
}

/// A `device3d` create payload, with the viewport sized to the model's screen.
fn device3d_template(a: DeviceArgs) -> Result<Output, ToolError> {
    let model = od_core::DeviceModel::ALL
        .into_iter()
        .find(|m| m.as_str() == a.model)
        .ok_or_else(|| {
            ToolError(format!(
                "unknown 3D model {:?}; available: {}",
                a.model,
                od_core::DeviceModel::ALL
                    .iter()
                    .map(|m| m.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })?;
    if !(a.screen_width.is_finite() && (50.0..=8192.0).contains(&a.screen_width)) {
        return Err(ToolError("screen_width must be between 50 and 8192".into()));
    }
    let aspect = model.screen_aspect();
    let (w, h) = if aspect < 1.0 {
        (a.screen_width, a.screen_width / aspect)
    } else {
        (a.screen_width * aspect, a.screen_width)
    };
    let device = od_core::Device3d {
        model,
        rotate_x: a.rotate_x.unwrap_or(8.0),
        rotate_y: a.rotate_y.unwrap_or(-18.0),
        ..od_core::Device3d::default()
    };
    let mut node = od_core::NewNode::new(od_core::NodeKind::Device3d);
    node.name = Some(format!("{} 3D", model.as_str()));
    node.width = w;
    node.height = h;
    node.image = a.image;
    node.device3d = Some(device);
    Ok(Output::json(&json!({
        "node": node,
        "usage": "Put this node in an apply_ops create operation (set parent, then x and y to \
                  place it). The turn and tilt above are a product-shot pose; change them with \
                  apply_ops set.",
    })))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetArgs {
    video: String,
    node: String,
    preset: Preset,
    start_ms: u64,
    duration_ms: u64,
    #[serde(default)]
    strength: Option<f64>,
    #[serde(default)]
    ease: Option<Ease>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    summary: Option<String>,
}

fn animate_preset(ctx: &mut CallCtx, a: PresetArgs) -> Result<Output, ToolError> {
    let (video, node) = (node_id(&a.video)?, node_id(&a.node)?);
    let ops: Vec<Operation> = animation::preset_ops(
        ctx.ws.session.document(),
        &video,
        &node,
        PresetRequest {
            preset: a.preset,
            start_ms: a.start_ms,
            duration_ms: a.duration_ms,
            strength: a.strength.unwrap_or(1.0),
            ease: a.ease,
        },
    )?;
    if ops.is_empty() {
        return Err(ToolError(format!(
            "{} would not move {} from where it is; give a strength or pick another move",
            a.preset.as_str(),
            a.node
        )));
    }
    apply_ops(
        ctx,
        ApplyArgs {
            ops,
            turn: a.turn,
            summary: a.summary,
        },
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SheetArgs {
    video: String,
    #[serde(default)]
    times: Option<Vec<u64>>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    from_ms: Option<u64>,
    #[serde(default)]
    to_ms: Option<u64>,
    #[serde(default)]
    cell_side: Option<u32>,
    #[serde(default)]
    locale: Option<String>,
}

/// `count` moments spread evenly over `from..=to`.
fn spread(from: u64, to: u64, count: usize) -> Vec<u64> {
    let count = count.max(2) as u64;
    (0..count)
        .map(|i| from + (to.saturating_sub(from)) * i / (count - 1))
        .collect()
}

fn contact_sheet(ctx: &mut CallCtx, a: SheetArgs) -> Result<Output, ToolError> {
    let id = node_id(&a.video)?;
    let source = crate::video_export::Source::of(ctx.ws);
    let length = source
        .doc
        .video(&id)
        .map(od_core::Video::duration_ms)
        .ok_or_else(|| ToolError(format!("video {id} does not exist")))?;
    let times = match a.times {
        Some(t) if t.len() <= MAX_SHEET_PICTURES && !t.is_empty() => t,
        Some(_) => {
            return Err(ToolError(format!("give 1 to {MAX_SHEET_PICTURES} times")));
        }
        None => spread(
            a.from_ms.unwrap_or(0),
            a.to_ms.unwrap_or(length.saturating_sub(1)),
            a.count.unwrap_or(8).clamp(2, MAX_SHEET_PICTURES),
        ),
    };
    let sheet = crate::video_export::contact_sheet_png(
        &source,
        &id,
        &times,
        a.cell_side.unwrap_or(256).clamp(64, 512),
        a.locale.as_deref(),
    )?;
    let listed: Vec<String> = times.iter().map(|t| format!("{t} ms")).collect();
    let mut note = format!(
        "{}x{} contact sheet of {}: {} pictures, {} to a row, at {}",
        sheet.width,
        sheet.height,
        a.video,
        times.len(),
        sheet.columns,
        listed.join(", ")
    );
    if !sheet.warnings.is_empty() {
        note.push_str(&format!("; warnings: {}", sheet.warnings.join("; ")));
    }
    Ok(Output {
        content: vec![
            Content::Image {
                data: STANDARD.encode(sheet.png),
                mime: "image/png",
            },
            Content::Text(note),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_spread_evenly_and_include_both_ends() {
        assert_eq!(spread(0, 3000, 4), vec![0, 1000, 2000, 3000]);
        assert_eq!(spread(500, 500, 3), vec![500, 500, 500]);
    }

    #[test]
    fn every_preset_and_ease_is_offered_by_name() {
        let defs = definitions();
        let presets = &defs[0]["inputSchema"]["properties"]["preset"]["enum"];
        assert_eq!(presets.as_array().map(Vec::len), Some(Preset::ALL.len()));
        assert!(
            presets
                .as_array()
                .is_some_and(|p| p.contains(&json!("hero_reveal")))
        );
    }
}
