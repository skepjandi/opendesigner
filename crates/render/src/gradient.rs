//! Gradient paints as tiny-skia shaders. A gradient is laid out in the node's own box (0,0 to
//! width,height), so the shader meets the node's transform exactly as the shape does.

use std::collections::BTreeMap;

use od_core::{OpError, Paint, Rgba, Stop};
use tiny_skia::{
    Color, GradientStop, LinearGradient, Point, RadialGradient, Shader, SpreadMode, Transform,
};

/// The shader of a gradient paint at node size `w` x `h` and `opacity`; `None` for a solid paint.
/// A gradient that cannot be laid out (a box with no area) falls back to its first stop's color.
pub(crate) fn shader(
    paint: &Paint,
    tokens: &BTreeMap<String, String>,
    (w, h): (f32, f32),
    opacity: f32,
) -> Result<Option<Shader<'static>>, OpError> {
    let laid_out = match paint {
        Paint::Color(_) | Paint::Token(_) => return Ok(None),
        Paint::Linear { angle, stops } => {
            linear(*angle as f32, (w, h), &color_stops(stops, tokens, opacity)?)
        }
        Paint::Radial { cx, cy, r, stops } => radial(
            (*cx as f32, *cy as f32, *r as f32),
            (w, h),
            &color_stops(stops, tokens, opacity)?,
        ),
    };
    match laid_out {
        Some(shader) => Ok(Some(shader)),
        None => {
            let first = paint.resolve(tokens)?;
            Ok(Some(Shader::SolidColor(color_of(first, opacity))))
        }
    }
}

fn color_of(c: Rgba, opacity: f32) -> Color {
    let alpha = (f32::from(c.a) / 255.0 * opacity).clamp(0.0, 1.0);
    Color::from_rgba(
        f32::from(c.r) / 255.0,
        f32::from(c.g) / 255.0,
        f32::from(c.b) / 255.0,
        alpha,
    )
    .unwrap_or(Color::TRANSPARENT)
}

/// The stops in offset order (a stable sort keeps equal offsets as written: a hard edge).
fn color_stops(
    stops: &[Stop],
    tokens: &BTreeMap<String, String>,
    opacity: f32,
) -> Result<Vec<GradientStop>, OpError> {
    let mut sorted: Vec<&Stop> = stops.iter().collect();
    sorted.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    sorted
        .into_iter()
        .map(|s| {
            let color = color_of(s.color.resolve(tokens)?, opacity);
            Ok(GradientStop::new(s.offset as f32, color))
        })
        .collect()
}

/// A line through the box's center at `angle` degrees (0 left to right, 90 top to bottom), long
/// enough that its ends reach the corners.
fn linear(angle: f32, (w, h): (f32, f32), stops: &[GradientStop]) -> Option<Shader<'static>> {
    let (sin, cos) = angle.to_radians().sin_cos();
    let reach = (w * cos).abs() / 2.0 + (h * sin).abs() / 2.0;
    let (cx, cy) = (w / 2.0, h / 2.0);
    let start = Point::from_xy(cx - cos * reach, cy - sin * reach);
    let end = Point::from_xy(cx + cos * reach, cy + sin * reach);
    LinearGradient::new(
        start,
        end,
        stops.to_vec(),
        SpreadMode::Pad,
        Transform::identity(),
    )
}

/// A circle in the unit square stretched over the box, so a box that is not square gets an ellipse.
fn radial(
    (cx, cy, r): (f32, f32, f32),
    (w, h): (f32, f32),
    stops: &[GradientStop],
) -> Option<Shader<'static>> {
    let center = Point::from_xy(cx, cy);
    RadialGradient::new(
        center,
        0.0,
        center,
        r,
        stops.to_vec(),
        SpreadMode::Pad,
        Transform::from_scale(w, h),
    )
}
