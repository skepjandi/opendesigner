//! Where nodes land in the picture: the map from a node's own coordinates to pixels, composed
//! from the node's position, rotation and scale about its pivot, its parents', and the video
//! camera (`docs/wiki/document-format.md#transform-and-effects`).

use od_core::{Affine, Camera, Node};
use tiny_skia::Transform;

pub(crate) fn to_transform(a: Affine) -> Transform {
    Transform::from_row(
        a.a as f32, a.b as f32, a.c as f32, a.d as f32, a.e as f32, a.f as f32,
    )
}

/// `parent` (the map from the parent's coordinates) extended by the node's own place in it: a
/// plain translation unless the node is turned or scaled.
pub(crate) fn node_chain(parent: Transform, node: &Node) -> Transform {
    if node.is_transformed() {
        parent.pre_concat(to_transform(node.local_affine()))
    } else {
        parent.pre_translate(node.x as f32, node.y as f32)
    }
}

/// Whether `t` only scales (by positive factors) and translates: text can be drawn straight
/// onto the pixmap, a box stays a box.
pub(crate) fn is_upright(t: Transform) -> bool {
    t.kx == 0.0 && t.ky == 0.0 && t.sx > 0.0 && t.sy > 0.0
}

/// How many pixels one unit of the node's own coordinates spans (a mean over both axes when the
/// map is not a plain scale).
pub(crate) fn scale_of(t: Transform) -> f32 {
    if t.kx == 0.0 && t.ky == 0.0 {
        t.sx.abs().max(t.sy.abs())
    } else {
        (t.sx * t.sy - t.kx * t.ky).abs().sqrt()
    }
}

/// The upright rectangle `(left, top, right, bottom)` around the box `(x, y, w, h)` after `t`.
pub(crate) fn device_rect(t: Transform, x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32, f32) {
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)]
        .map(|(px, py)| (t.sx * px + t.kx * py + t.tx, t.ky * px + t.sy * py + t.ty));
    corners.iter().fold(
        (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ),
        |(l, t, r, b), &(cx, cy)| (l.min(cx), t.min(cy), r.max(cx), b.max(cy)),
    )
}

/// The video camera in the pixels of the picture being drawn.
#[derive(Clone, Copy)]
pub(crate) struct CameraView {
    camera: Camera,
    cx: f64,
    cy: f64,
}

impl CameraView {
    /// `None` for a camera at rest. `base` maps the frame's document units to pixels and
    /// `(w, h)` is the frame's size in them.
    pub(crate) fn new(camera: Camera, base: Transform, w: f64, h: f64) -> Option<CameraView> {
        if camera.is_still() {
            return None;
        }
        let s = f64::from(base.sx);
        Some(CameraView {
            camera: Camera {
                x: camera.x * s,
                y: camera.y * s,
                ..camera
            },
            cx: w / 2.0 * s + f64::from(base.tx),
            cy: h / 2.0 * s + f64::from(base.ty),
        })
    }

    /// What the camera does to a node `depth` deep: it follows `depth` times as far.
    pub(crate) fn transform(&self, depth: f32) -> Transform {
        to_transform(self.camera.affine(f64::from(depth), self.cx, self.cy))
    }

    /// How much the camera magnifies a node `depth` deep.
    pub(crate) fn zoom(&self, depth: f32) -> f32 {
        self.camera.zoom.powf(f64::from(depth)) as f32
    }
}
