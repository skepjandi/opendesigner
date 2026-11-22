//! Where things sit in a scene: the boxes the camera moves, the gaps between a child and its
//! parent, the nodes under a point and the pixels a node really changes.

use od_core::geometry::Rect;
use od_core::{Affine, Camera, Document, Node, NodeId, NodeKind, Video};
use serde::Deserialize;
use serde_json::{Value, json};

use super::structure::{LayerEntry, resolve};
use super::{Calibration, Gate, Timeline, frame_height, last_ms, spread, tidy_times};
use crate::tools::ToolError;
use crate::video_export::Source;

/// The gap a child may have from its parent, in pixels at 1080p.
pub const DEFAULT_GAP_PX: f64 = 2.0;
/// The height the gap tolerance is written for.
pub const GAP_REFERENCE_HEIGHT: f64 = 1080.0;
/// A channel must differ by more than this for a pixel to count as changed.
const PIXEL_DELTA: u8 = 8;
/// The longest side a footprint is measured at.
const FOOTPRINT_SIDE: f64 = 512.0;

/// The camera extremes the plan declares, checked as extra poses.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CameraExtremes {
    #[serde(default)]
    pub x_max_px: Option<f64>,
    #[serde(default)]
    pub y_max_px: Option<f64>,
    #[serde(default)]
    pub zoom_min: Option<f64>,
    #[serde(default)]
    pub zoom_max: Option<f64>,
    #[serde(default)]
    pub rotation_max_deg: Option<f64>,
}

/// Which node's footprint to measure.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FootprintArgs {
    pub node: String,
    #[serde(default)]
    pub t_ms: Option<f64>,
}

/// A node's place in a frame: its box, how far the camera moves it and how it is turned.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    /// The node's box in the frame's own units.
    pub rect: Rect,
    /// How far the camera moves it: the depths of its parents and itself, multiplied.
    pub depth: f64,
    /// The map from the node's own units to the frame's.
    pub affine: Affine,
    /// Its own turn plus its parents'.
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub ancestors: Vec<NodeId>,
}

/// The box of a node in the frame's units, and the map that puts it there.
pub fn place(doc: &Document, frame: &NodeId, id: &NodeId) -> Option<Placed> {
    let loc_f = doc.locate(frame)?;
    let loc_n = doc.locate(id)?;
    if loc_f.page != loc_n.page || !loc_n.path.starts_with(&loc_f.path) {
        return None;
    }
    let frame_node = doc.node(frame)?;
    let below = loc_n.path.get(loc_f.path.len()..)?;
    let mut node = frame_node;
    let mut affine = Affine::IDENTITY;
    let mut depth = 1.0;
    let mut rotation = frame_node.rotation;
    let mut ancestors = vec![frame.clone()];
    for &i in below {
        let child = node.children.get(i)?;
        affine = affine.after(child.local_affine());
        depth *= child.depth;
        rotation += child.rotation;
        ancestors.push(child.id.clone());
        node = child;
    }
    let rect = world_rect(node, affine)?;
    Some(Placed {
        rect,
        depth,
        affine,
        rotation,
        scale_x: node.scale_x,
        scale_y: node.scale_y,
        ancestors,
    })
}

/// The box of a node (or of a group's children) in the frame's units.
fn world_rect(node: &Node, affine: Affine) -> Option<Rect> {
    if node.kind == NodeKind::Group && !(node.clip && node.layout_box().is_some()) {
        return node
            .children
            .iter()
            .filter(|c| c.visible)
            .filter_map(|c| world_rect(c, affine.after(c.local_affine())))
            .reduce(Rect::union);
    }
    let own = node.layout_box()?;
    Some(affine.bounds_of(Rect {
        x: 0.0,
        y: 0.0,
        ..own
    }))
}

/// Whether `ancestor` holds `id` (or is it).
pub fn is_ancestor_of(doc: &Document, ancestor: &NodeId, id: &NodeId) -> bool {
    let (Some(a), Some(b)) = (doc.locate(ancestor), doc.locate(id)) else {
        return false;
    };
    a.page == b.page && b.path.starts_with(&a.path)
}

/// The landmarks of a node, by name: a foot or a hand.
pub fn landmarks(doc: &Document, frame: &NodeId, id: &NodeId) -> Vec<(&'static str, Rect)> {
    let mut out = Vec::new();
    let Some(p) = place(doc, frame, id) else {
        return out;
    };
    let Some(node) = doc.node(id) else {
        return out;
    };
    walk_landmarks(node, p.affine, &mut out);
    out
}

/// The landmarks below a node, by name.
fn walk_landmarks(node: &Node, affine: Affine, out: &mut Vec<(&'static str, Rect)>) {
    for c in &node.children {
        let a = affine.after(c.local_affine());
        if let Some(kind) = landmark_kind(&c.name)
            && let Some(r) = world_rect(c, a)
        {
            out.push((kind, r));
        }
        walk_landmarks(c, a, out);
    }
}

/// What a node's name says it is, when it is a foot or a hand.
fn landmark_kind(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    if ["foot", "feet", "leg", "hoof", "shoe", "boot"]
        .iter()
        .any(|k| n.contains(k))
    {
        Some("foot")
    } else if ["hand", "arm", "paw", "wing"].iter().any(|k| n.contains(k)) {
        Some("hand")
    } else {
        None
    }
}

/// The distance between two boxes: zero when they touch or overlap.
pub fn gap(a: Rect, b: Rect) -> f64 {
    let dx = (a.x - b.right()).max(0.0).max(b.x - a.right());
    let dy = (a.y - b.bottom()).max(0.0).max(b.y - a.bottom());
    (dx * dx + dy * dy).sqrt()
}

/// How far a layer falls short of covering the frame under a camera pose, in the frame's units.
/// Zero when it covers.
pub fn cover_gap(camera: &Camera, depth: f64, fw: f64, fh: f64, rect: Rect) -> Option<f64> {
    let cam = camera.affine(depth, fw / 2.0, fh / 2.0);
    let inv = cam.inverse()?;
    let mut worst: f64 = 0.0;
    let points = [
        (0.0, 0.0),
        (fw / 2.0, 0.0),
        (fw, 0.0),
        (fw, fh / 2.0),
        (fw, fh),
        (fw / 2.0, fh),
        (0.0, fh),
        (0.0, fh / 2.0),
    ];
    for (x, y) in points {
        let (px, py) = inv.apply(x, y);
        let dx = (rect.x - px).max(0.0).max(px - rect.right());
        let dy = (rect.y - py).max(0.0).max(py - rect.bottom());
        worst = worst.max(dx.max(dy));
    }
    Some(worst)
}

/// The camera poses to check a scene under: every keyframe of the camera, the moments between
/// them, and the extremes the plan declares.
pub fn camera_poses(
    doc: &Document,
    video: &Video,
    extremes: Option<&CameraExtremes>,
) -> Vec<(String, Camera, f64)> {
    let mut out: Vec<(String, Camera, f64)> = Vec::new();
    let mut keys: Vec<f64> = Vec::new();
    for a in &video.animations {
        if a.prop.is_camera() {
            for k in &a.keys {
                keys.push(k.at_ms as f64);
            }
        }
    }
    keys.sort_by(f64::total_cmp);
    keys.dedup();
    let mids: Vec<f64> = keys.windows(2).map(|w| (w[0] + w[1]) / 2.0).collect();
    for t in keys.iter().chain(mids.iter()) {
        out.push(("camera key".into(), video.camera_at(*t), *t));
    }
    if let Some(e) = extremes {
        let (fx, fy) = units_per_pixel(doc, video);
        let x = e.x_max_px.unwrap_or(0.0) * fx;
        let y = e.y_max_px.unwrap_or(0.0) * fy;
        let zoom_min = e.zoom_min.unwrap_or(1.0);
        let zoom_max = e.zoom_max.unwrap_or(1.0);
        let rot = e.rotation_max_deg.unwrap_or(0.0);
        for (label, camera) in [
            (
                "declared pan left",
                Camera {
                    x: -x,
                    ..Camera::default()
                },
            ),
            (
                "declared pan right",
                Camera {
                    x,
                    ..Camera::default()
                },
            ),
            (
                "declared pan up",
                Camera {
                    y: -y,
                    ..Camera::default()
                },
            ),
            (
                "declared pan down",
                Camera {
                    y,
                    ..Camera::default()
                },
            ),
            (
                "declared zoom out",
                Camera {
                    zoom: zoom_min,
                    ..Camera::default()
                },
            ),
            (
                "declared zoom in",
                Camera {
                    zoom: zoom_max,
                    ..Camera::default()
                },
            ),
            (
                "declared turn",
                Camera {
                    rotation: rot,
                    ..Camera::default()
                },
            ),
        ] {
            out.push((label.into(), camera, 0.0));
        }
    }
    out
}

/// The frame's own units in one pixel of the video.
fn units_per_pixel(doc: &Document, video: &Video) -> (f64, f64) {
    let h = frame_height(doc, video);
    let w = super::shown_frames(video)
        .iter()
        .find_map(|(f, _, _)| doc.node(f))
        .map(|f| f.width)
        .unwrap_or(h);
    let vw = f64::from(video.width.max(1));
    let vh = f64::from(video.height.max(1));
    ((w / vw).min(h / vh), (h / vh).min(w / vw))
}

/// The bleed gate: a layer that must cover the frame covers it under every camera pose.
pub fn bleed_gate(
    doc: &Document,
    video: &Video,
    plan: Option<&[LayerEntry]>,
    camera: Option<&CameraExtremes>,
) -> Gate {
    let mut gate = Gate::new("bleed", Calibration::Derived);
    let Some(plan) = plan else {
        gate.unknowns
            .push("no layer plan was given, so no layer is known to bleed".into());
        return gate;
    };
    let bleeding: Vec<&LayerEntry> = plan.iter().filter(|e| e.bleed).collect();
    if bleeding.is_empty() {
        gate.unknowns
            .push("no layer is flagged bleed:true, so nothing had to cover the frame".into());
        return gate;
    }
    let poses = camera_poses(doc, video, camera);
    let mut timeline = Timeline::new(doc, video);
    for entry in bleeding {
        let Some((id, how)) = resolve(doc, entry) else {
            gate.failures.push(format!(
                "{}: no node in the document is called {:?}",
                entry.id,
                entry.node_ref.as_deref().or(entry.name.as_deref())
            ));
            continue;
        };
        let Some((frame, _)) = super::frame_of(doc, video, &id) else {
            gate.failures.push(format!(
                "{} ({id}, found by {how}): the node is in no scene the video shows",
                entry.id
            ));
            continue;
        };
        let Some(frame_node) = doc.node(&frame) else {
            continue;
        };
        let (fw, fh) = (frame_node.width, frame_node.height);
        for (label, cam, t) in &poses {
            let at = timeline.at(*t);
            let Some(p) = place(at, &frame, &id) else {
                continue;
            };
            let Some(short) = cover_gap(cam, p.depth, fw, fh, p.rect) else {
                gate.failures.push(format!(
                    "{}: the camera turns it flat, so it cannot cover the frame",
                    entry.id
                ));
                continue;
            };
            if short > 0.0 {
                gate.failures.push(format!(
                    "{}: under the {label} the layer falls {short:.1} of the frame's units short of covering it (depth {})",
                    entry.id, p.depth
                ));
            }
        }
    }
    gate
}

/// The attach gate: a child sits within its gap tolerance of its parent, at every moment.
pub fn attach_gate(doc: &Document, video: &Video, plan: Option<&[LayerEntry]>) -> Gate {
    let mut gate = Gate::new("attach", Calibration::Derived);
    let Some(plan) = plan else {
        gate.unknowns
            .push("no layer plan was given, so no attachment could be looked at".into());
        return gate;
    };
    let mut pairs: Vec<(&LayerEntry, &LayerEntry)> = Vec::new();
    for entry in plan {
        let Some(parent_id) = entry.parent.as_deref() else {
            continue;
        };
        let Some(parent) = plan.iter().find(|o| o.id == parent_id) else {
            gate.failures.push(format!(
                "{}: the parent {} is not in the plan",
                entry.id, parent_id
            ));
            continue;
        };
        pairs.push((entry, parent));
    }
    if pairs.is_empty() {
        gate.unknowns
            .push("no layer declares a parent to attach to".into());
        return gate;
    }
    let h = frame_height(doc, video);
    let mut timeline = Timeline::new(doc, video);
    for (child, parent) in pairs {
        let (Some((cid, _)), Some((pid, _))) = (resolve(doc, child), resolve(doc, parent)) else {
            gate.failures
                .push(format!("{}: the node it names was not found", child.id));
            continue;
        };
        let tolerance = child
            .attach
            .as_ref()
            .and_then(|a| a.gap_tolerance_px)
            .unwrap_or(DEFAULT_GAP_PX)
            * (h / GAP_REFERENCE_HEIGHT);
        let ids = vec![cid.clone(), pid.clone()];
        let Some(cframe) = super::frame_of(doc, video, &cid).map(|(f, _)| f) else {
            gate.failures.push(format!(
                "{}: the node it names is in no scene the video shows",
                child.id
            ));
            continue;
        };
        let Some(pframe) = super::frame_of(doc, video, &pid).map(|(f, _)| f) else {
            gate.failures.push(format!(
                "{}: the parent it names is in no scene the video shows",
                parent.id
            ));
            continue;
        };
        let times = super::structure::sample_times(doc, video, &ids);
        let mut worst = 0.0;
        let mut worst_at = 0.0;
        for t in &times {
            let at = timeline.at(*t);
            let Some(c) = place(at, &cframe, &cid) else {
                continue;
            };
            let Some(p) = place(at, &pframe, &pid) else {
                continue;
            };
            let g = gap(c.rect, p.rect);
            if g > worst {
                worst = g;
                worst_at = *t;
            }
        }
        if worst > tolerance {
            gate.failures.push(format!(
                "{}: it sits {worst:.2} from {} at {worst_at:.0} ms, over the tolerance {tolerance:.2}",
                child.id, parent.id
            ));
        }
    }
    gate
}

/// The nodes under a point at a moment, topmost first.
pub fn hit_list(doc: &Document, video: &Video, x: f64, y: f64, t: f64) -> Result<Value, ToolError> {
    let at = doc.at_time(video, t);
    let doc = at.as_ref().unwrap_or(doc);
    let frame = frame_at(video, t).ok_or_else(|| {
        ToolError(format!(
            "no scene is on screen at {t:.0} ms, so there is nothing to hit"
        ))
    })?;
    let mut work = doc.clone();
    let mut ids: Vec<String> = Vec::new();
    for _ in 0..64 {
        let Some(hit) = od_core::geometry::hit_test(&work, &frame, x, y) else {
            break;
        };
        if hit == frame {
            ids.push(hit.as_str().to_string());
            break;
        }
        ids.push(hit.as_str().to_string());
        let Some(loc) = work.locate(&hit) else {
            break;
        };
        let Some(node) = work.node_at_mut(&loc) else {
            break;
        };
        node.visible = false;
    }
    Ok(json!({
        "video": video.id.as_str(),
        "frame": frame.as_str(),
        "t_ms": t,
        "x": x,
        "y": y,
        "nodes": ids,
        "note": "The point is in the frame's own units, as the design shows it; a moving camera is not applied.",
    }))
}

/// The frame the video shows at a moment.
pub fn frame_at(video: &Video, t: f64) -> Option<NodeId> {
    let time = t.max(0.0) as u64;
    let mut found: Option<NodeId> = None;
    for (frame, start, end) in super::shown_frames(video) {
        if start <= time && time < end {
            found = Some(frame);
        }
    }
    found
}

/// The pixels a node really changes in the picture: the picture with it and the picture without
/// it, compared.
pub fn footprint(src: &Source, video: &Video, args: &FootprintArgs) -> Result<Value, ToolError> {
    let id = od_core::NodeId::parse(&args.node)
        .map_err(|_| ToolError(format!("{} is not a node id", args.node)))?;
    let t = args.t_ms.unwrap_or(0.0);
    let at = src.doc.at_time(video, t);
    let doc = at.as_ref().unwrap_or(&src.doc);
    let frame = frame_at(video, t)
        .ok_or_else(|| ToolError(format!("no scene is on screen at {t:.0} ms")))?;
    let Some(node) = doc.node(&id) else {
        return Err(ToolError(format!("node {} does not exist", args.node)));
    };
    let scale = (FOOTPRINT_SIDE / node.width.max(1.0)).min(1.0) as f32;
    let full = od_render::render_frame(doc, &frame, scale, src.dir())
        .map_err(|e| ToolError(e.to_string()))?;
    let mut without = doc.clone();
    if let Some(loc) = without.locate(&id)
        && let Some(n) = without.node_at_mut(&loc)
    {
        n.visible = false;
    }
    let hidden = od_render::render_frame(&without, &frame, scale, src.dir())
        .map_err(|e| ToolError(e.to_string()))?;
    let (a, b) = (full.pixmap.data(), hidden.pixmap.data());
    let changed = a
        .chunks(4)
        .zip(b.chunks(4))
        .filter(|(p, q)| {
            p.iter()
                .zip(q.iter())
                .any(|(x, y)| x.abs_diff(*y) > PIXEL_DELTA)
        })
        .count();
    let total = full.pixmap.width() as usize * full.pixmap.height() as usize;
    Ok(json!({
        "node": id.as_str(),
        "frame": frame.as_str(),
        "t_ms": t,
        "pixels": changed,
        "frame_pixels": total,
        "share": if total > 0 { changed as f64 / total as f64 } else { 0.0 },
        "note": "The pixels that differ between the picture with the node and the picture without it.",
    }))
}

/// The moments a scene is looked at: its keyframes and 12 even moments.
pub fn scene_times(video: &Video, keys: &[f64]) -> Vec<f64> {
    let end = last_ms(video);
    let mut times = keys.to_vec();
    times.extend(spread(0.0, end, 12));
    tidy_times(times)
}
