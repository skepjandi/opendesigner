//! What flat-vector drawing adds to a node: path data and how it fits the node's box, fill rules,
//! stroke caps, joins, dashes and trims, and the checks that keep them inside bounds
//! (`docs/wiki/document-format.md#paths`). Gradients live with the other paints in [`crate::paint`].

use std::collections::BTreeMap;
use std::fmt;
use std::mem::replace;

use serde::de::{self, Deserializer, Visitor};
use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::{self, finite_in};
use crate::model::{Node, NodeKind, Stroke};
use crate::ops::{NewNode, Property};
use crate::paint::Rgba;
use crate::path::{Bounds, Path, Point};

/// Most entries in a stroke's dash array.
pub const MAX_DASH_ENTRIES: usize = 16;

/// How a stroke ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// How a stroke turns a corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Which side of a path counts as inside when it crosses itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillRule {
    #[default]
    Nonzero,
    Evenodd,
}

/// The rectangle of path coordinates that maps onto a path node's box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl ViewBox {
    pub(crate) fn checked(self) -> Result<ViewBox, OpError> {
        let c = limits::MAX_COORD;
        Ok(ViewBox {
            x: finite_in("view_box x", self.x, -c, c)?,
            y: finite_in("view_box y", self.y, -c, c)?,
            width: positive("view_box width", self.width)?,
            height: positive("view_box height", self.height)?,
        })
    }
}

fn positive(what: &'static str, v: f64) -> Result<f64, OpError> {
    if v > 0.0 {
        finite_in(what, v, 0.0, limits::MAX_COORD)
    } else {
        Err(OpError::OutOfBounds {
            what,
            detail: format!("{v} is not a positive number"),
        })
    }
}

impl From<Bounds> for ViewBox {
    fn from(b: Bounds) -> ViewBox {
        ViewBox {
            x: b.x,
            y: b.y,
            width: b.width,
            height: b.height,
        }
    }
}

/// Sets one of the properties this module owns (`Property::Path`, `ViewBox`, `FillRule`,
/// `StrokeWidth`, `TrimStart`, `TrimEnd`) and returns the old value as a property that undoes it.
pub(crate) fn apply_property(node: &mut Node, prop: Property) -> Result<Property, OpError> {
    Ok(match prop {
        Property::Path(d) => {
            Path::parse(&d)?;
            Property::Path(node.path.replace(d).unwrap_or_default())
        }
        Property::ViewBox(v) => {
            let v = v.map(ViewBox::checked).transpose()?;
            Property::ViewBox(replace(&mut node.view_box, v))
        }
        Property::FillRule(v) => Property::FillRule(replace(&mut node.fill_rule, v)),
        Property::PathMorph(v) => {
            let v = v.map(crate::morph::PathMorph::checked).transpose()?;
            Property::PathMorph(replace(&mut node.morph, v))
        }
        Property::StrokeWidth(v) => {
            let v = finite_in("stroke width", v, 0.0, limits::MAX_STROKE_WIDTH)?;
            Property::StrokeWidth(replace(&mut stroke_of(node)?.width, v))
        }
        Property::TrimStart(v) => {
            let v = finite_in("trim_start", v, 0.0, 1.0)?;
            Property::TrimStart(replace(&mut stroke_of(node)?.trim_start, v))
        }
        Property::TrimEnd(v) => {
            let v = finite_in("trim_end", v, 0.0, 1.0)?;
            Property::TrimEnd(replace(&mut stroke_of(node)?.trim_end, v))
        }
        other => {
            return Err(OpError::Internal(format!(
                "{} is not a vector property",
                other.name()
            )));
        }
    })
}

fn stroke_of(node: &mut Node) -> Result<&mut Stroke, OpError> {
    let id = node.id.to_string();
    node.stroke.as_mut().ok_or(OpError::OutOfBounds {
        what: "stroke",
        detail: format!("node {id} has no stroke; set one first"),
    })
}

/// The value of a stroke number as stored, for the receipt of a `set`.
pub(crate) fn current_stroke_value(node: &Node, prop: &Property) -> Property {
    let s = node.stroke.as_ref();
    match prop {
        Property::StrokeWidth(_) => Property::StrokeWidth(s.map_or(0.0, |s| s.width)),
        Property::TrimStart(_) => Property::TrimStart(s.map_or(0.0, |s| s.trim_start)),
        _ => Property::TrimEnd(s.map_or(1.0, |s| s.trim_end)),
    }
}

/// The box a new node is created with. A path node given no size takes the size of its view box
/// or of its path's own bounds.
pub(crate) fn box_size(new: &NewNode) -> (f64, f64) {
    if new.kind != NodeKind::Path || new.width != 0.0 || new.height != 0.0 {
        return (new.width, new.height);
    }
    let natural = new.view_box.map(|v| (v.width, v.height)).or_else(|| {
        let b = Path::parse(new.path.as_deref()?).ok()?.bounds()?;
        Some((b.width, b.height))
    });
    natural.map_or((0.0, 0.0), |(w, h)| {
        (w.min(limits::MAX_SIZE), h.min(limits::MAX_SIZE))
    })
}

/// The properties that give a new node its path data, view box and fill rule.
pub(crate) fn path_properties(new: &NewNode) -> Result<Vec<Property>, OpError> {
    let mut props = Vec::new();
    match (&new.path, new.kind == NodeKind::Path) {
        (Some(d), _) => props.push(Property::Path(d.clone())),
        (None, true) => {
            return Err(OpError::OutOfBounds {
                what: "path",
                detail: "a path node needs path data".to_string(),
            });
        }
        (None, false) => {}
    }
    if new.view_box.is_some() {
        props.push(Property::ViewBox(new.view_box));
    }
    if new.fill_rule.is_some() {
        props.push(Property::FillRule(new.fill_rule));
    }
    Ok(props)
}

/// The path of a path node, parsed.
pub fn node_path(node: &Node) -> Option<Path> {
    node.path.as_deref().and_then(|d| Path::parse(d).ok())
}

/// `path` mapped into the box `width` x `height`: the view box (or, without one, the path's own
/// bounds) is stretched over the box, so resizing the node scales the drawing. A flat bound
/// (a straight horizontal or vertical line) keeps its scale along that axis.
pub fn fit_to_box(path: &Path, view_box: Option<ViewBox>, width: f64, height: f64) -> Path {
    let Some(v) = view_box.or_else(|| path.bounds().map(ViewBox::from)) else {
        return path.clone();
    };
    let scale = |box_len: f64, view_len: f64| {
        if view_len > 0.0 {
            box_len / view_len
        } else {
            1.0
        }
    };
    let (sx, sy) = (scale(width, v.width), scale(height, v.height));
    path.scaled(sx, sy, -v.x * sx, -v.y * sy)
}

/// How far from a path's outline a click still selects it, in document units.
const CLICK_REACH: f64 = 3.0;

/// Whether the point `(x, y)`, in the node's own coordinates, is on a path node: inside its fill
/// (by its fill rule) or within half the stroke width of its outline (at least [`CLICK_REACH`]).
pub(crate) fn hits(node: &Node, x: f64, y: f64) -> bool {
    let Some(path) = node_path(node) else {
        return false;
    };
    let fitted = fit_to_box(&path, node.view_box, node.width, node.height);
    let contours = fitted.flatten((node.width.max(node.height) / 64.0).max(0.25));
    let reach = node
        .stroke
        .as_ref()
        .map_or(CLICK_REACH, |s| (s.width / 2.0).max(CLICK_REACH));
    let near_outline = contours
        .iter()
        .any(|c| edges(c).any(|(a, b)| distance_to_segment([x, y], a, b) <= reach));
    near_outline
        || (node.fill.is_some()
            && inside_fill(&contours, [x, y], node.fill_rule.unwrap_or_default()))
}

/// The edges of a contour; a filled contour is closed whether or not it says so.
fn edges(c: &crate::path::Contour) -> impl Iterator<Item = (Point, Point)> + '_ {
    let closing = c.points.last().zip(c.points.first()).map(|(l, f)| (*l, *f));
    c.points.windows(2).map(|w| (w[0], w[1])).chain(closing)
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - (a[0] + t * dx)).hypot(p[1] - (a[1] + t * dy))
}

fn inside_fill(contours: &[crate::path::Contour], p: Point, rule: FillRule) -> bool {
    let (mut winding, mut crossings) = (0i32, 0u32);
    for (a, b) in contours.iter().flat_map(edges) {
        if (a[1] <= p[1]) == (b[1] <= p[1]) {
            continue;
        }
        let cross = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
        if cross > p[0] {
            crossings += 1;
            winding += if b[1] > a[1] { 1 } else { -1 };
        }
    }
    match rule {
        FillRule::Nonzero => winding != 0,
        FillRule::Evenodd => crossings % 2 == 1,
    }
}

/// A stroke as the document stores it: the paint checked, every number in bounds and rounded.
pub(crate) fn checked_stroke(
    s: &Stroke,
    tokens: &BTreeMap<String, String>,
) -> Result<Stroke, OpError> {
    if s.dash.len() > MAX_DASH_ENTRIES {
        return Err(OpError::Limit(format!(
            "at most {MAX_DASH_ENTRIES} dash entries"
        )));
    }
    let dash = s
        .dash
        .iter()
        .map(|d| finite_in("dash", *d, 0.0, limits::MAX_SIZE))
        .collect::<Result<Vec<_>, _>>()?;
    if !dash.is_empty() && dash.iter().sum::<f64>() <= 0.0 {
        return Err(OpError::OutOfBounds {
            what: "dash",
            detail: "needs at least one entry above 0".to_string(),
        });
    }
    Ok(Stroke {
        paint: s.paint.normalized(tokens)?,
        width: finite_in("stroke width", s.width, 0.0, limits::MAX_STROKE_WIDTH)?,
        cap: s.cap,
        join: s.join,
        dash,
        trim_start: finite_in("trim_start", s.trim_start, 0.0, 1.0)?,
        trim_end: finite_in("trim_end", s.trim_end, 0.0, 1.0)?,
    })
}

/// The path fields of a node: present, valid and in bounds on a path node, absent on the others.
pub(crate) fn check_node(node: &Node) -> Result<(), OpError> {
    let is_path = node.kind == NodeKind::Path;
    match (&node.path, is_path) {
        (Some(d), true) => {
            Path::parse(d)?;
        }
        (None, true) => {
            return Err(OpError::OutOfBounds {
                what: "path",
                detail: format!("node {} is a path without its path data", node.id),
            });
        }
        (Some(_), false) => return not_applicable("path", node),
        (None, false) => {}
    }
    if node.view_box.is_some() && !is_path {
        return not_applicable("view_box", node);
    }
    if node.fill_rule.is_some() && !is_path {
        return not_applicable("fill_rule", node);
    }
    if node.morph.is_some() && !is_path {
        return not_applicable("path_morph", node);
    }
    node.morph
        .clone()
        .map(crate::morph::PathMorph::checked)
        .transpose()?;
    node.view_box.map(ViewBox::checked).transpose()?;
    Ok(())
}

fn not_applicable(property: &'static str, node: &Node) -> Result<(), OpError> {
    Err(OpError::NotApplicable {
        property,
        kind: node.kind.as_str(),
    })
}

impl Rgba {
    /// The colour as one number, `0xRRGGBBAA`, the value a `fill_color` keyframe holds.
    pub fn pack(self) -> f64 {
        f64::from(u32::from_be_bytes([self.r, self.g, self.b, self.a]))
    }

    /// The colour a packed number stands for; out-of-range numbers saturate.
    pub fn unpack(packed: f64) -> Rgba {
        let [r, g, b, a] = (packed.round().clamp(0.0, f64::from(u32::MAX)) as u32).to_be_bytes();
        Rgba { r, g, b, a }
    }

    /// Straight-line mix of two packed colours, each channel in sRGB, `t` held to 0 to 1.
    pub fn mix_packed(a: f64, b: f64, t: f64) -> f64 {
        let (a, b, t) = (Rgba::unpack(a), Rgba::unpack(b), t.clamp(0.0, 1.0));
        let m = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
        Rgba {
            r: m(a.r, b.r),
            g: m(a.g, b.g),
            b: m(a.b, b.b),
            a: m(a.a, b.a),
        }
        .pack()
    }
}

/// A keyframe value written as a number or, for `fill_color`, as a colour `'#rrggbb[aa]'`.
pub(crate) fn number_or_color<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    struct Value;
    impl Visitor<'_> for Value {
        type Value = f64;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a number or a colour like '#ff8800'")
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
            Ok(v as f64)
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
            Ok(v as f64)
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
            Rgba::parse_hex(v).map(Rgba::pack).map_err(E::custom)
        }
    }
    d.deserialize_any(Value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_colours_round_trip_and_mix_per_channel() {
        let red = Rgba::parse_hex("#ff0000").unwrap();
        let blue = Rgba::parse_hex("#0000ff80").unwrap();
        assert_eq!(Rgba::unpack(red.pack()), red);
        let mid = Rgba::unpack(Rgba::mix_packed(red.pack(), blue.pack(), 0.5));
        assert_eq!(mid.to_hex(), "#800080c0");
        assert_eq!(Rgba::mix_packed(red.pack(), blue.pack(), 9.0), blue.pack());
    }

    #[test]
    fn a_keyframe_value_may_be_a_number_or_a_colour() {
        #[derive(Deserialize)]
        struct K(#[serde(deserialize_with = "number_or_color")] f64);
        let n: K = serde_json::from_str("12.5").unwrap();
        assert_eq!(n.0, 12.5);
        let c: K = serde_json::from_str("\"#010203\"").unwrap();
        assert_eq!(Rgba::unpack(c.0).to_hex(), "#010203");
        assert!(serde_json::from_str::<K>("\"red\"").is_err());
    }

    #[test]
    fn a_path_fits_the_box_by_its_own_bounds_or_its_view_box() {
        let path = Path::parse("M10 10 L30 50").unwrap();
        let own = fit_to_box(&path, None, 100.0, 200.0).bounds().unwrap();
        assert_eq!(
            (own.x, own.y, own.width, own.height),
            (0.0, 0.0, 100.0, 200.0)
        );
        let view = ViewBox {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let placed = fit_to_box(&path, Some(view), 50.0, 50.0).bounds().unwrap();
        assert_eq!(
            (placed.x, placed.y, placed.width, placed.height),
            (5.0, 5.0, 10.0, 20.0)
        );
    }

    #[test]
    fn a_flat_path_keeps_its_scale_along_the_flat_axis() {
        let path = Path::parse("M10 5 L30 5").unwrap();
        let b = fit_to_box(&path, None, 100.0, 40.0).bounds().unwrap();
        assert_eq!((b.x, b.width, b.height), (0.0, 100.0, 0.0));
    }
}
