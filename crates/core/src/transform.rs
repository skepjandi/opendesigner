//! What 2D nodes can do to themselves: rotate and scale about a pivot, blur, cast a drop shadow,
//! clip their children, sit at a depth for the camera's parallax, and the video camera itself
//! (`docs/wiki/document-format.md#transform-and-effects`).
//!
//! Every field is optional on a node and its default means no effect, so a document that uses
//! none of them draws, and saves, exactly as before.

use std::collections::BTreeMap;
use std::mem::replace;

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::geometry::Rect;
use crate::limits::finite_in;
use crate::model::{Node, NodeKind};
use crate::ops::Property;
use crate::paint::{Paint, Rgba};

/// Rotation, in degrees either way (a hundred turns, so a spinner can keep going).
pub const MAX_ROTATION: f64 = 36_000.0;
/// Scale factors; a negative one flips.
pub const MAX_SCALE: f64 = 100.0;
/// A pivot is a fraction of the node's box; outside the box is allowed, within ten boxes.
pub const MAX_PIVOT: f64 = 10.0;
/// Gaussian blur radius in document units.
pub const MAX_BLUR: f64 = 500.0;
pub const MAX_SHADOW_OFFSET: f64 = 5_000.0;
/// How strongly the camera moves a node: 0 pins it to the screen, 1 is the scene, above 1 is
/// foreground that moves faster.
pub const MAX_PARALLAX_DEPTH: f64 = 10.0;
pub const MAX_CAMERA_PAN: f64 = 100_000.0;
pub const MIN_CAMERA_ZOOM: f64 = 0.1;
pub const MAX_CAMERA_ZOOM: f64 = 20.0;

/// A drop shadow: the node's silhouette, offset, blurred and tinted. The color's alpha is the
/// shadow's opacity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
    /// Gaussian blur radius.
    #[serde(default)]
    pub blur: f64,
    #[serde(default = "Shadow::default_color")]
    pub color: Paint,
}

impl Shadow {
    fn default_color() -> Paint {
        Paint::Color(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0x40,
        })
    }

    /// The shadow with every number checked and rounded the way `set` stores it, its color
    /// resolvable.
    pub fn checked(&self, tokens: &BTreeMap<String, String>) -> Result<Shadow, OpError> {
        self.color.resolve(tokens)?;
        Ok(Shadow {
            dx: shadow_offset("shadow dx", self.dx)?,
            dy: shadow_offset("shadow dy", self.dy)?,
            blur: shadow_blur(self.blur)?,
            color: self.color.clone(),
        })
    }
}

pub fn rotation(v: f64) -> Result<f64, OpError> {
    finite_in("rotation", v, -MAX_ROTATION, MAX_ROTATION)
}

pub fn scale(what: &'static str, v: f64) -> Result<f64, OpError> {
    finite_in(what, v, -MAX_SCALE, MAX_SCALE)
}

pub fn pivot(what: &'static str, v: f64) -> Result<f64, OpError> {
    finite_in(what, v, -MAX_PIVOT, MAX_PIVOT)
}

pub fn blur(v: f64) -> Result<f64, OpError> {
    finite_in("blur", v, 0.0, MAX_BLUR)
}

pub fn shadow_offset(what: &'static str, v: f64) -> Result<f64, OpError> {
    finite_in(what, v, -MAX_SHADOW_OFFSET, MAX_SHADOW_OFFSET)
}

pub fn shadow_blur(v: f64) -> Result<f64, OpError> {
    finite_in("shadow blur", v, 0.0, MAX_BLUR)
}

pub fn depth(v: f64) -> Result<f64, OpError> {
    finite_in("depth", v, 0.0, MAX_PARALLAX_DEPTH)
}

/// A 2D affine map: `x' = a x + c y + e`, `y' = b x + d y + f`. The same layout as
/// `tiny_skia::Transform::from_row(a, b, c, d, e, f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    pub fn translate(x: f64, y: f64) -> Affine {
        Affine {
            e: x,
            f: y,
            ..Affine::IDENTITY
        }
    }

    pub fn scale(x: f64, y: f64) -> Affine {
        Affine {
            a: x,
            d: y,
            ..Affine::IDENTITY
        }
    }

    /// A turn of `degrees` clockwise on a screen whose y axis points down.
    pub fn rotate(degrees: f64) -> Affine {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Affine {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            e: 0.0,
            f: 0.0,
        }
    }

    /// The map that applies `self` after `first`.
    pub fn after(self, first: Affine) -> Affine {
        Affine {
            a: self.a * first.a + self.c * first.b,
            b: self.b * first.a + self.d * first.b,
            c: self.a * first.c + self.c * first.d,
            d: self.b * first.c + self.d * first.d,
            e: self.a * first.e + self.c * first.f + self.e,
            f: self.b * first.e + self.d * first.f + self.f,
        }
    }

    pub fn apply(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    pub fn inverse(self) -> Option<Affine> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }

    /// The smallest upright rectangle holding `rect` after the map.
    pub fn bounds_of(self, rect: Rect) -> Rect {
        let corners = [
            self.apply(rect.x, rect.y),
            self.apply(rect.right(), rect.y),
            self.apply(rect.x, rect.bottom()),
            self.apply(rect.right(), rect.bottom()),
        ];
        let (mut left, mut top) = (f64::INFINITY, f64::INFINITY);
        let (mut right, mut bottom) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (x, y) in corners {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
        Rect {
            x: left,
            y: top,
            width: right - left,
            height: bottom - top,
        }
    }
}

/// The video camera at one moment. Its numbers move through `camera_*` animations.
///
/// The scene is panned so the point `(x, y)` document units off the frame's center sits at the
/// center, turned `rotation` degrees clockwise and magnified `zoom` times about the center.
/// A node at depth `d` follows the camera `d` times as far: it pans `d * (x, y)`, turns
/// `d * rotation` and magnifies `zoom ^ d`; depth 0 stays on the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub rotation: f64,
}

impl Default for Camera {
    fn default() -> Camera {
        Camera {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

impl Camera {
    pub fn is_still(&self) -> bool {
        *self == Camera::default()
    }

    /// Where the camera maps a point of the scene, for a node at `depth`, in a frame whose
    /// center is `(cx, cy)`.
    pub fn affine(&self, depth: f64, cx: f64, cy: f64) -> Affine {
        let about_center = Affine::translate(cx, cy)
            .after(Affine::rotate(self.rotation * depth))
            .after(Affine::scale(self.zoom.powf(depth), self.zoom.powf(depth)))
            .after(Affine::translate(-cx, -cy));
        about_center.after(Affine::translate(-self.x * depth, -self.y * depth))
    }
}

impl Node {
    /// Whether the node is turned or scaled (position aside).
    pub fn is_transformed(&self) -> bool {
        self.rotation != 0.0 || self.scale_x != 1.0 || self.scale_y != 1.0
    }

    /// Whether the node does anything beyond sitting at its position: a transform, a blur, a
    /// shadow, a clip or a depth.
    pub fn has_effects(&self) -> bool {
        self.is_transformed()
            || self.blur > 0.0
            || self.shadow.is_some()
            || self.clip
            || self.depth != 1.0
    }

    /// The box a pivot and a clip refer to, in the node's own coordinates: its size, or for a
    /// group without one the box around its children's (untransformed) boxes.
    pub fn layout_box(&self) -> Option<Rect> {
        if self.kind != NodeKind::Group || (self.width > 0.0 && self.height > 0.0) {
            return Some(Rect {
                x: 0.0,
                y: 0.0,
                width: self.width,
                height: self.height,
            });
        }
        self.children
            .iter()
            .filter(|c| c.visible)
            .filter_map(|c| c.layout_box().map(|b| b.translate(c.x, c.y)))
            .reduce(Rect::union)
    }

    /// Where the node's own coordinates sit in its parent's: the position, then the rotation and
    /// scale about the pivot.
    pub fn local_affine(&self) -> Affine {
        let place = Affine::translate(self.x, self.y);
        if !self.is_transformed() {
            return place;
        }
        let b = self.layout_box().unwrap_or(Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        });
        let (px, py) = (b.x + self.pivot_x * b.width, b.y + self.pivot_y * b.height);
        place
            .after(Affine::translate(px, py))
            .after(Affine::rotate(self.rotation))
            .after(Affine::scale(self.scale_x, self.scale_y))
            .after(Affine::translate(-px, -py))
    }
}

/// Sets one transform or effect property on `node` and returns the property that undoes it.
/// The caller has checked that the property applies to the node's kind.
pub(crate) fn apply(
    node: &mut Node,
    prop: Property,
    tokens: &BTreeMap<String, String>,
) -> Result<Property, OpError> {
    Ok(match prop {
        Property::Rotation(v) => Property::Rotation(replace(&mut node.rotation, rotation(v)?)),
        Property::ScaleX(v) => Property::ScaleX(replace(&mut node.scale_x, scale("scale_x", v)?)),
        Property::ScaleY(v) => Property::ScaleY(replace(&mut node.scale_y, scale("scale_y", v)?)),
        Property::Scale(v) => {
            let v = scale("scale", v)?;
            Property::ScaleXy {
                x: replace(&mut node.scale_x, v),
                y: replace(&mut node.scale_y, v),
            }
        }
        Property::ScaleXy { x, y } => {
            let (x, y) = (scale("scale_x", x)?, scale("scale_y", y)?);
            Property::ScaleXy {
                x: replace(&mut node.scale_x, x),
                y: replace(&mut node.scale_y, y),
            }
        }
        Property::PivotX(v) => Property::PivotX(replace(&mut node.pivot_x, pivot("pivot_x", v)?)),
        Property::PivotY(v) => Property::PivotY(replace(&mut node.pivot_y, pivot("pivot_y", v)?)),
        Property::Blur(v) => Property::Blur(replace(&mut node.blur, blur(v)?)),
        Property::DropShadow(v) => {
            let v = v.map(|s| s.checked(tokens)).transpose()?;
            Property::DropShadow(replace(&mut node.shadow, v))
        }
        Property::ShadowDx(v) => {
            let v = shadow_offset("shadow dx", v)?;
            Property::ShadowDx(replace(&mut shadow_of(node)?.dx, v))
        }
        Property::ShadowDy(v) => {
            let v = shadow_offset("shadow dy", v)?;
            Property::ShadowDy(replace(&mut shadow_of(node)?.dy, v))
        }
        Property::ShadowBlur(v) => {
            let v = shadow_blur(v)?;
            Property::ShadowBlur(replace(&mut shadow_of(node)?.blur, v))
        }
        Property::Clip(v) => Property::Clip(replace(&mut node.clip, v)),
        Property::Depth(v) => Property::Depth(replace(&mut node.depth, depth(v)?)),
        other => {
            return Err(OpError::Internal(format!(
                "{} is not a transform property",
                other.name()
            )));
        }
    })
}

/// The node's shadow, which a number of it can only change once there is one.
fn shadow_of(node: &mut Node) -> Result<&mut Shadow, OpError> {
    let id = node.id.to_string();
    node.shadow.as_mut().ok_or_else(|| OpError::OutOfBounds {
        what: "shadow",
        detail: format!("node {id} has no shadow; set drop_shadow first"),
    })
}

/// The value `prop` holds on `node` now, as the property that sets it (what `set` stored).
pub(crate) fn current(node: &Node, prop: &Property) -> Property {
    let shadow = node.shadow.as_ref();
    match prop {
        Property::Rotation(_) => Property::Rotation(node.rotation),
        Property::ScaleX(_) => Property::ScaleX(node.scale_x),
        Property::ScaleY(_) => Property::ScaleY(node.scale_y),
        Property::Scale(_) => Property::Scale(node.scale_x),
        Property::ScaleXy { .. } => Property::ScaleXy {
            x: node.scale_x,
            y: node.scale_y,
        },
        Property::PivotX(_) => Property::PivotX(node.pivot_x),
        Property::PivotY(_) => Property::PivotY(node.pivot_y),
        Property::Blur(_) => Property::Blur(node.blur),
        Property::DropShadow(_) => Property::DropShadow(node.shadow.clone()),
        Property::ShadowDx(_) => Property::ShadowDx(shadow.map_or(0.0, |s| s.dx)),
        Property::ShadowDy(_) => Property::ShadowDy(shadow.map_or(0.0, |s| s.dy)),
        Property::ShadowBlur(_) => Property::ShadowBlur(shadow.map_or(0.0, |s| s.blur)),
        Property::Clip(_) => Property::Clip(node.clip),
        Property::Depth(_) => Property::Depth(node.depth),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: (f64, f64), b: (f64, f64)) {
        assert!(
            (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9,
            "{a:?} != {b:?}"
        );
    }

    #[test]
    fn rotation_is_clockwise_on_a_screen() {
        near(Affine::rotate(90.0).apply(1.0, 0.0), (0.0, 1.0));
    }

    #[test]
    fn after_applies_the_right_map_first_and_inverse_undoes_it() {
        let m = Affine::translate(10.0, 0.0).after(Affine::scale(2.0, 3.0));
        near(m.apply(1.0, 1.0), (12.0, 3.0));
        let back = m.inverse().map(|i| i.apply(12.0, 3.0));
        near(back.unwrap_or_default(), (1.0, 1.0));
        assert!(Affine::scale(0.0, 1.0).inverse().is_none());
    }

    #[test]
    fn depth_zero_pins_to_the_screen_and_one_follows_the_camera() {
        let cam = Camera {
            x: 100.0,
            y: 0.0,
            zoom: 2.0,
            rotation: 0.0,
        };
        near(cam.affine(0.0, 50.0, 50.0).apply(7.0, 9.0), (7.0, 9.0));
        // the point 100 right of the center comes to the center, then doubles about it
        near(cam.affine(1.0, 50.0, 50.0).apply(150.0, 50.0), (50.0, 50.0));
        near(cam.affine(0.5, 50.0, 50.0).apply(100.0, 50.0), (50.0, 50.0));
    }
}
