//! Where a node lands in a video's picture at a moment: its box in the frame, moved by the
//! camera as far as its depth says, scaled by the way the clip covers the video and by the clip's
//! slow zoom. A transition's slide or fade is not included: look at the picture for those.

use od_core::geometry::{self, Rect};
use od_core::{Document, Node, NodeId, Video};

/// The box of `id` in the pixels of the `video` picture at `time_ms`, when the picture shows it
/// (a clip or an overlay that holds its frame is on, and the node and its parents are visible).
pub fn node_rect(doc: &Document, video: &Video, id: &NodeId, time_ms: u64) -> Option<Rect> {
    let animated = doc.at_time(video, time_ms as f64);
    let doc = animated.as_ref().unwrap_or(doc);
    let (w, h) = (f64::from(video.width), f64::from(video.height));
    let camera = video.camera_at(time_ms as f64);
    let clips = video
        .placed()
        .into_iter()
        .rev()
        .filter(|p| p.start_ms <= time_ms && time_ms < p.end_ms)
        .filter_map(|p| {
            let clip = &video.clips[p.index];
            let into = (time_ms - p.start_ms) as f64 / (p.end_ms - p.start_ms) as f64;
            let zoom = clip.motion.scale_from
                + (clip.motion.scale_to - clip.motion.scale_from)
                    * clip.motion.ease.apply(into.min(1.0));
            Some((clip.frame.clone()?, zoom, true))
        });
    let overlays = video
        .overlays
        .iter()
        .rev()
        .filter(|o| o.start_ms <= time_ms && time_ms < o.start_ms + u64::from(o.duration_ms))
        .map(|o| (o.frame.clone(), 1.0, false));
    overlays
        .chain(clips)
        .find_map(|(frame, zoom, through_camera)| {
            let (rect, depth) = in_frame(doc, &frame, id)?;
            let frame_node = doc.node(&frame)?;
            let (fw, fh) = (frame_node.width, frame_node.height);
            let rect = if through_camera && !camera.is_still() {
                camera.affine(depth, fw / 2.0, fh / 2.0).bounds_of(rect)
            } else {
                rect
            };
            let scale = (w / fw).max(h / fh) * zoom;
            Some(Rect {
                x: rect.x * scale + (w - fw * scale) / 2.0,
                y: rect.y * scale + (h - fh * scale) / 2.0,
                width: rect.width * scale,
                height: rect.height * scale,
            })
        })
}

/// The node's box in its frame's units and how far the camera moves it (its depth times its
/// parents'), or `None` when it is hidden or not in the frame.
fn in_frame(doc: &Document, frame: &NodeId, id: &NodeId) -> Option<(Rect, f64)> {
    let (frame_loc, loc) = (doc.locate(frame)?, doc.locate(id)?);
    if !loc.path.starts_with(&frame_loc.path) {
        return None;
    }
    let mut node = doc.node(frame)?;
    let mut depth = 1.0;
    for &i in &loc.path[frame_loc.path.len()..] {
        node = node.children.get(i)?;
        if !shown(node) {
            return None;
        }
        depth *= node.depth;
    }
    Some((geometry::bounds_in_frame(doc, frame, id)?, depth))
}

fn shown(node: &Node) -> bool {
    node.visible && node.opacity > 0.0
}
