//! What a scene (a design frame shown by a clip) is made of and what moves in it, read from the
//! document alone: the facts behind the animation lint rules `OD090` and `OD091` and the
//! `review_video` tool (`docs/wiki/lint-rules.md`, `docs/wiki/agent-integration.md#review_video`).

use std::collections::{HashMap, HashSet};

use crate::animation::{Animation, Repeat};
use crate::geometry::{self, Rect};
use crate::model::{Document, Node, NodeId, NodeKind};
use crate::video::Video;

/// Coverage is measured on a grid this many cells wide and tall.
const GRID: usize = 32;

/// How much of the frame a kind of drawing covers, as a share of its area (0 to 1).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Coverage {
    /// The largest single raster image.
    pub biggest_image: f64,
    /// Everything drawn from pictures (images, 3D devices), overlaps counted once.
    pub raster: f64,
    /// Everything drawn from paths, rectangles and ellipses, overlaps counted once.
    pub vector: f64,
}

/// The facts about one scene.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SceneStats {
    pub coverage: Coverage,
    /// Visible paths, rectangles and ellipses.
    pub shapes: usize,
    /// Visible text nodes.
    pub texts: usize,
    /// Nodes of the scene that a track of the video animates, in document order.
    pub animated: Vec<NodeId>,
    /// Animated nodes that are not text or a plain rectangle (or a group of only those).
    pub vector_animated: usize,
    /// Tracks of the scene that repeat and really change.
    pub loops: usize,
}

impl SceneStats {
    /// Something moves, and only text and rectangles do.
    pub fn text_only_motion(&self) -> bool {
        !self.animated.is_empty() && self.vector_animated == 0
    }
}

/// A full-frame texture laid over the scene (the `paper_grain` block's grain pictures) is
/// finish, not a backdrop, so it is not counted as the picture the scene is made of.
fn is_texture_overlay(node: &Node) -> bool {
    node.name.starts_with("block:paper_grain")
}

fn is_shape(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::Path | NodeKind::Rect | NodeKind::Ellipse)
}

fn visible_below<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    for c in &node.children {
        if c.visible && c.opacity > 0.0 {
            out.push(c);
            visible_below(c, out);
        }
    }
}

/// Every visible node inside `frame`, in drawing order.
pub fn visible_nodes(frame: &Node) -> Vec<&Node> {
    let mut out = Vec::new();
    visible_below(frame, &mut out);
    out
}

fn mark(grid: &mut [bool], bounds: Rect, frame: &Node) {
    if frame.width <= 0.0 || frame.height <= 0.0 {
        return;
    }
    let cell = |v: f64, size: f64| ((v / size) * GRID as f64).clamp(0.0, GRID as f64);
    let (x0, x1) = (
        cell(bounds.x, frame.width).floor() as usize,
        cell(bounds.x + bounds.width, frame.width).ceil() as usize,
    );
    let (y0, y1) = (
        cell(bounds.y, frame.height).floor() as usize,
        cell(bounds.y + bounds.height, frame.height).ceil() as usize,
    );
    for y in y0..y1.min(GRID) {
        for x in x0..x1.min(GRID) {
            grid[y * GRID + x] = true;
        }
    }
}

fn share(grid: &[bool]) -> f64 {
    grid.iter().filter(|c| **c).count() as f64 / (GRID * GRID) as f64
}

fn coverage(doc: &Document, frame: &Node, nodes: &[&Node]) -> Coverage {
    let (mut raster, mut vector) = (vec![false; GRID * GRID], vec![false; GRID * GRID]);
    let mut biggest = 0.0f64;
    let area = frame.width * frame.height;
    for n in nodes {
        let Some(b) = geometry::bounds_in_frame(doc, &frame.id, &n.id) else {
            continue;
        };
        match n.kind {
            NodeKind::Image if is_texture_overlay(n) => {}
            NodeKind::Image => {
                mark(&mut raster, b, frame);
                if area > 0.0 {
                    biggest = biggest.max((b.width * b.height / area).min(1.0));
                }
            }
            NodeKind::Device3d => mark(&mut raster, b, frame),
            k if is_shape(k) => mark(&mut vector, b, frame),
            _ => {}
        }
    }
    Coverage {
        biggest_image: biggest,
        raster: share(&raster),
        vector: share(&vector),
    }
}

fn has_vector(node: &Node) -> bool {
    matches!(node.kind, NodeKind::Path | NodeKind::Ellipse) || node.children.iter().any(has_vector)
}

fn changes(a: &Animation) -> bool {
    a.keys
        .windows(2)
        .any(|w| (w[0].value - w[1].value).abs() > f64::EPSILON)
}

/// What the scene `frame` is made of and which of `video`'s tracks move it.
pub fn scene_stats(doc: &Document, video: &Video, frame: &NodeId) -> Option<SceneStats> {
    let root = doc.node(frame)?;
    let nodes = visible_nodes(root);
    let by_id: HashMap<&NodeId, &Node> = nodes.iter().map(|n| (&n.id, *n)).collect();
    let mut seen = HashSet::new();
    let mut stats = SceneStats {
        coverage: coverage(doc, root, &nodes),
        shapes: nodes.iter().filter(|n| is_shape(n.kind)).count(),
        texts: nodes.iter().filter(|n| n.kind == NodeKind::Text).count(),
        ..SceneStats::default()
    };
    for a in video.animations.iter().filter(|a| !a.prop.is_camera()) {
        let Some(node) = by_id.get(&a.node) else {
            continue;
        };
        if a.repeat != Repeat::Once && changes(a) {
            stats.loops += 1;
        }
        if seen.insert(&a.node) {
            stats.animated.push(a.node.clone());
            stats.vector_animated += usize::from(has_vector(node));
        }
    }
    Some(stats)
}
