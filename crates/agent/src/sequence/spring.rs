//! `spring_to`: one property sent to a value on a spring, with the times the agent needs to place
//! the next beat after it (`docs/wiki/video-editing.md#sequencing`).

use od_core::animation::{AnimProp, Key, Repeat};
use od_core::{Document, Ease, NodeId, Operation};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{ApplyArgs, CallCtx, Output, ToolError, apply_ops, node_id, parse};

use super::{parse_ease, snap};

/// How long a spring looks like it takes when nothing is said.
const DEFAULT_VISUAL_MS: u64 = 400;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpringArgs {
    pub video: String,
    #[serde(default)]
    pub node: Option<String>,
    pub prop: AnimProp,
    pub to: f64,
    #[serde(default)]
    pub at_ms: Option<u64>,
    #[serde(default)]
    pub visual_ms: Option<u64>,
    #[serde(default)]
    pub bounce: Option<f64>,
    #[serde(default)]
    pub turn: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

pub fn call(ctx: &mut CallCtx, args: Value) -> Result<Output, ToolError> {
    let a: SpringArgs = parse(args)?;
    let video_id = node_id(&a.video)?;
    let doc = ctx.ws.session.document();
    let video = doc
        .video(&video_id)
        .ok_or_else(|| ToolError(format!("no video {}", a.video)))?;
    let target = if a.prop.is_camera() {
        video_id.clone()
    } else {
        let raw = a
            .node
            .as_deref()
            .ok_or_else(|| ToolError(format!("{} needs `node`", a.prop.as_str())))?;
        node_id(raw)?
    };
    let visual_ms = a.visual_ms.unwrap_or(DEFAULT_VISUAL_MS);
    let bounce = a.bounce.unwrap_or(0.0);
    let ease = parse_ease(&format!("spring({visual_ms},{bounce})"))?;
    let Ease::Tuned(spring) = ease else {
        return Err(ToolError("a spring ease is needed here".into()));
    };
    let existing = video
        .animations
        .iter()
        .find(|track| track.node == target && track.prop == a.prop);
    let at = a
        .at_ms
        .unwrap_or_else(|| existing.and_then(|a| a.keys.last()).map_or(0, |k| k.at_ms));
    let from = match existing.and_then(|track| track.value_at(at as f64)) {
        Some(v) => v,
        None => current(doc, &target, a.prop, video, at)?,
    };
    let settle_ms = u64::from(spring.settle_ms());
    let fps = video.fps.max(1);
    let start_ms = snap(at, fps);
    let end_ms = snap(at.saturating_add(settle_ms), fps).max(start_ms + 1);
    let mut keys = vec![
        Key {
            at_ms: start_ms,
            value: from,
            ease: Ease::Linear,
        },
        Key {
            at_ms: end_ms,
            value: a.to,
            ease,
        },
    ];
    if let Some(existing) = existing {
        let mut kept: Vec<Key> = existing
            .keys
            .iter()
            .filter(|k| k.at_ms < start_ms || k.at_ms > end_ms)
            .copied()
            .collect();
        kept.append(&mut keys);
        kept.sort_by_key(|k| k.at_ms);
        keys = kept;
    }
    let op = Operation::Animate {
        video: video_id.clone(),
        node: target.clone(),
        prop: a.prop,
        keys,
        repeat: existing.map_or(Repeat::Once, |e| e.repeat),
    };
    let out = apply_ops(
        ctx,
        ApplyArgs {
            ops: vec![op],
            turn: a.turn,
            summary: a.summary,
        },
    )?;
    Ok(crate::scene_tool::merge_output(
        out,
        json!({
            "node": target.to_string(),
            "prop": a.prop.as_str(),
            "from": from,
            "to": a.to,
            "at_ms": start_ms,
            "end_ms": end_ms,
            "settle_ms": end_ms - start_ms,
            "perceived_ms": visual_ms,
            "bounce": bounce,
        }),
    ))
}

/// The value a property has now.
fn current(
    doc: &Document,
    node: &NodeId,
    prop: AnimProp,
    video: &od_core::Video,
    at_ms: u64,
) -> Result<f64, ToolError> {
    if prop.is_camera() {
        let camera = video.camera_at(at_ms as f64);
        return Ok(match prop {
            AnimProp::CameraX => camera.x,
            AnimProp::CameraY => camera.y,
            AnimProp::CameraZoom => camera.zoom,
            _ => camera.rotation,
        });
    }
    let found = doc
        .node(node)
        .ok_or_else(|| ToolError(format!("no node {node}")))?;
    Ok(prop.read(found).unwrap_or(0.0))
}
