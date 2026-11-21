//! Vector shapes: path nodes, and the fill and stroke every shape node is painted with (solid
//! colors and gradients, caps, joins, dashes, trims). All of it draws through the `Transform` it
//! is given, so a node's place, scale and turn apply to shape and shader alike.

use std::collections::BTreeMap;

use od_core::vector::{self, FillRule, LineCap, LineJoin};
use od_core::{Node, Paint, Stroke};
use tiny_skia::{
    FillRule as SkFillRule, LineCap as SkCap, LineJoin as SkJoin, Mask, Paint as SkPaint, Path,
    PathBuilder, Pixmap, Stroke as SkStroke, StrokeDash, Transform,
};

use crate::{RenderError, gradient, solid, trim};

/// Finest piece (in device pixels) a trimmed outline is cut into.
const TRIM_STEP_PIXELS: f32 = 0.5;

/// The outline of a path node in the node's box, or `None` when there is nothing to draw (the
/// data cannot be parsed, or it is a single point).
pub(crate) fn path_shape(node: &Node) -> Option<Path> {
    let path = vector::node_path(node)?;
    let fitted = vector::fit_to_box(&path, node.view_box, node.width, node.height);
    let mut builder = PathBuilder::new();
    for seg in fitted.segments() {
        use od_core::path::Seg;
        match *seg {
            Seg::MoveTo(p) => builder.move_to(p[0] as f32, p[1] as f32),
            Seg::LineTo(p) => builder.line_to(p[0] as f32, p[1] as f32),
            Seg::QuadTo(c, p) => {
                builder.quad_to(c[0] as f32, c[1] as f32, p[0] as f32, p[1] as f32)
            }
            Seg::CubicTo(a, b, p) => builder.cubic_to(
                a[0] as f32,
                a[1] as f32,
                b[0] as f32,
                b[1] as f32,
                p[0] as f32,
                p[1] as f32,
            ),
            Seg::Close => builder.close(),
        }
    }
    builder.finish()
}

/// Fills and strokes `shape` (in the node's box) as the node's fill and stroke say.
pub(crate) fn paint_shape(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    shape: &Path,
    node: &Node,
    tokens: &BTreeMap<String, String>,
    transform: Transform,
    opacity: f32,
) -> Result<(), RenderError> {
    let size = (node.width as f32, node.height as f32);
    if let Some(fill) = &node.fill {
        let paint = sk_paint(fill, tokens, size, opacity)?;
        pixmap.fill_path(shape, &paint, sk_fill_rule(node), transform, mask);
    }
    if let Some(stroke) = node.stroke.as_ref().filter(|s| s.width > 0.0) {
        stroke_shape(
            pixmap, mask, shape, stroke, tokens, size, transform, opacity,
        )?;
    }
    Ok(())
}

fn sk_fill_rule(node: &Node) -> SkFillRule {
    match node.fill_rule.unwrap_or_default() {
        FillRule::Nonzero => SkFillRule::Winding,
        FillRule::Evenodd => SkFillRule::EvenOdd,
    }
}

fn sk_paint(
    paint: &Paint,
    tokens: &BTreeMap<String, String>,
    size: (f32, f32),
    opacity: f32,
) -> Result<SkPaint<'static>, RenderError> {
    match gradient::shader(paint, tokens, size, opacity)? {
        Some(shader) => {
            let mut sk = SkPaint {
                shader,
                ..SkPaint::default()
            };
            sk.anti_alias = true;
            Ok(sk)
        }
        None => Ok(solid(paint.resolve(tokens)?, opacity)),
    }
}

#[allow(clippy::too_many_arguments)]
fn stroke_shape(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    shape: &Path,
    stroke: &Stroke,
    tokens: &BTreeMap<String, String>,
    size: (f32, f32),
    transform: Transform,
    opacity: f32,
) -> Result<(), RenderError> {
    let paint = sk_paint(&stroke.paint, tokens, size, opacity)?;
    let (lo, hi) = (
        stroke.trim_start.min(stroke.trim_end),
        stroke.trim_start.max(stroke.trim_end),
    );
    let trimmed;
    let shape = if lo > 0.0 || hi < 1.0 {
        let (sx, sy) = transform.get_scale();
        let step = TRIM_STEP_PIXELS / sx.max(sy).max(1e-3);
        match trim::trim(shape, lo, hi, f64::from(step)) {
            Some(path) => {
                trimmed = path;
                &trimmed
            }
            None => return Ok(()),
        }
    } else {
        shape
    };
    pixmap.stroke_path(shape, &paint, &sk_stroke(stroke), transform, mask);
    Ok(())
}

fn sk_stroke(stroke: &Stroke) -> SkStroke {
    SkStroke {
        width: stroke.width as f32,
        line_cap: match stroke.cap {
            LineCap::Butt => SkCap::Butt,
            LineCap::Round => SkCap::Round,
            LineCap::Square => SkCap::Square,
        },
        line_join: match stroke.join {
            LineJoin::Miter => SkJoin::Miter,
            LineJoin::Round => SkJoin::Round,
            LineJoin::Bevel => SkJoin::Bevel,
        },
        dash: dash_of(&stroke.dash),
        ..SkStroke::default()
    }
}

/// The dash pattern; an odd number of entries repeats itself, as in SVG.
fn dash_of(dash: &[f64]) -> Option<StrokeDash> {
    if dash.is_empty() {
        return None;
    }
    let mut array: Vec<f32> = dash.iter().map(|d| *d as f32).collect();
    if array.len() % 2 == 1 {
        array.extend_from_within(..);
    }
    StrokeDash::new(array, 0.0)
}
