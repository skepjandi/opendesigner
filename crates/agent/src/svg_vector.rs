//! `import_svg_vector`: an SVG drawing turned into design nodes, one per shape, so the parts of an
//! illustration can be moved, recolored and animated (`docs/wiki/agent-integration.md#vector-import`).
//!
//! The SVG is untrusted. It is audited as XML first (scripts, event handlers, pictures and any
//! reference that leaves the file are refused, as are drawings that would expand into too many
//! shapes), parsed by `usvg` with every resource loader switched off, and converted with every
//! transform flattened into the shapes. The result is one `NewNode` tree, so a single `create`
//! operation brings it in and a single undo takes it out.

use std::collections::HashMap;

use od_core::path::{Path, Seg};
use od_core::{FillRule, LineCap, LineJoin, NewNode, NodeKind, Paint, Rgba, Stop, Stroke};
use resvg::usvg::{self, roxmltree};
use thiserror::Error;

/// Largest SVG text accepted.
pub const MAX_SVG_BYTES: usize = 4 * 1024 * 1024;
/// Most nodes (shapes and groups) one import makes.
pub const MAX_NODES: usize = 500;
/// Most shape elements an SVG may declare before it is converted.
const MAX_SHAPE_ELEMENTS: usize = 2_000;
/// Deepest nesting of the groups an import keeps.
pub const MAX_GROUP_DEPTH: usize = 12;
const MAX_ELEMENTS: u32 = 200_000;
const SHAPE_ELEMENTS: [&str; 7] = [
    "path", "rect", "circle", "ellipse", "line", "polyline", "polygon",
];
const FORBIDDEN_ELEMENTS: [(&str, &str); 7] = [
    ("script", "scripts"),
    ("foreignobject", "foreign content"),
    ("iframe", "embedded pages"),
    ("embed", "embedded content"),
    ("object", "embedded content"),
    ("audio", "media"),
    ("video", "media"),
];

#[derive(Debug, Error)]
#[error("{0}")]
pub struct ImportError(String);

fn refuse<T>(why: impl Into<String>) -> Result<T, ImportError> {
    Err(ImportError(why.into()))
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Name of the group the drawing becomes (default: the SVG's own id, else "illustration").
    pub name: Option<String>,
    /// Where the drawing's top-left corner goes, in the parent's coordinates.
    pub x: f64,
    pub y: f64,
    /// Width of the whole drawing in document units; the height follows. Default: the SVG's size.
    pub width: Option<f64>,
}

#[derive(Debug)]
pub struct Imported {
    /// The group holding the drawing, ready for a `create` operation.
    pub node: NewNode,
    /// The size of the drawing in document units.
    pub size: (f64, f64),
    /// What the SVG had that the nodes cannot carry.
    pub notes: Vec<String>,
}

pub fn import(svg: &str, options: &Options) -> Result<Imported, ImportError> {
    if svg.len() > MAX_SVG_BYTES {
        return refuse(format!("the SVG is larger than {MAX_SVG_BYTES} bytes"));
    }
    let audit = audit(svg)?;
    let tree = usvg::Tree::from_str(svg, &locked_down())
        .map_err(|e| ImportError(format!("not a drawable SVG: {e}")))?;
    let size = tree.size();
    let (sw, sh) = (f64::from(size.width()), f64::from(size.height()));
    if sw.is_nan() || sh.is_nan() || sw <= 0.0 || sh <= 0.0 {
        return refuse("the SVG has no size; give it a viewBox");
    }
    let scale = options.width.map_or(1.0, |w| w / sw);
    if !scale.is_finite() || scale <= 0.0 {
        return refuse("width must be a positive number");
    }
    let mut converter = Converter::new(scale);
    let built = converter.group(tree.root(), 0)?;
    if built.is_empty() {
        return refuse(
            "the SVG draws no shapes (text, pictures, filters and patterns are not imported)",
        );
    }
    let mut node = NewNode::new(NodeKind::Group);
    node.name = Some(group_name(options.name.as_deref(), &audit.root_id));
    node.x = options.x;
    node.y = options.y;
    node.children = built.into_iter().map(|b| b.node).collect();
    let mut notes = audit.notes();
    notes.extend(converter.notes.lines());
    Ok(Imported {
        node,
        size: (sw * scale, sh * scale),
        notes,
    })
}

fn group_name(requested: Option<&str>, root_id: &str) -> String {
    let name = requested
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(root_id);
    let name = clean_name(name);
    if name.is_empty() {
        "illustration".to_string()
    } else {
        name
    }
}

fn clean_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .take(100)
        .collect::<String>()
        .trim()
        .to_string()
}

/// `usvg` with nothing it could load from outside the SVG: no pictures from files or data URLs.
fn locked_down() -> usvg::Options<'static> {
    usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        max_depth: 64,
        ..usvg::Options::default()
    }
}

#[derive(Default)]
struct Audit {
    root_id: String,
    texts: usize,
}

impl Audit {
    fn notes(&self) -> Vec<String> {
        if self.texts == 0 {
            return Vec::new();
        }
        vec![format!(
            "{} <text> element(s) were not imported: draw lettering with text nodes",
            self.texts
        )]
    }
}

/// Reads the SVG as XML and refuses what must not come in. Nothing here draws.
fn audit(svg: &str) -> Result<Audit, ImportError> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        nodes_limit: MAX_ELEMENTS,
        ..roxmltree::ParsingOptions::default()
    };
    let doc = roxmltree::Document::parse_with_options(svg, options)
        .map_err(|e| ImportError(format!("not an SVG file: {e}")))?;
    let mut audit = Audit {
        root_id: doc
            .root_element()
            .attribute("id")
            .unwrap_or_default()
            .to_string(),
        texts: 0,
    };
    let by_id: HashMap<&str, roxmltree::Node> = doc
        .descendants()
        .filter_map(|n| Some((n.attribute("id")?, n)))
        .collect();
    let mut shapes = 0;
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        let tag = node.tag_name().name().to_ascii_lowercase();
        if let Some((_, what)) = FORBIDDEN_ELEMENTS.iter().find(|(t, _)| *t == tag) {
            return refuse(format!(
                "the SVG contains {what} (<{tag}>), which is not imported"
            ));
        }
        match tag.as_str() {
            "image" => {
                return refuse(
                    "the SVG contains a picture (<image>); a vector import has none (use import_svg or import_file for pictures)",
                );
            }
            "text" => audit.texts += 1,
            "style" => check_style(&node)?,
            "use" => shapes += 1 + check_use(&by_id, &node)?,
            t if SHAPE_ELEMENTS.contains(&t) => shapes += 1,
            _ => {}
        }
        check_attributes(&node)?;
    }
    if shapes > MAX_SHAPE_ELEMENTS {
        return refuse(format!(
            "the SVG declares {shapes} shapes; at most {MAX_SHAPE_ELEMENTS} (and {MAX_NODES} nodes) are imported, simplify it first"
        ));
    }
    Ok(audit)
}

fn check_attributes(node: &roxmltree::Node) -> Result<(), ImportError> {
    for attr in node.attributes() {
        let name = attr.name().to_ascii_lowercase();
        let value = attr.value();
        if name.len() > 2 && name.starts_with("on") {
            return refuse(format!(
                "the SVG has an event handler ({name}), which is not imported"
            ));
        }
        if name == "href" && !value.starts_with('#') {
            return refuse(format!(
                "the SVG refers outside itself ({value:.60}); only #id references are imported"
            ));
        }
        check_urls(value)?;
    }
    Ok(())
}

/// Every `url(...)` in `text` points inside the document (`url(#id)`).
fn check_urls(text: &str) -> Result<(), ImportError> {
    let lower = text.to_ascii_lowercase();
    if lower.trim_start().starts_with("javascript:") {
        return refuse("the SVG has a javascript: reference");
    }
    for part in lower.split("url(").skip(1) {
        let target = part
            .split(')')
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches(['"', '\'']);
        if !target.starts_with('#') {
            return refuse(format!(
                "the SVG refers outside itself (url({target:.60})); only #id references are imported"
            ));
        }
    }
    if lower.contains("@import") {
        return refuse("the SVG imports another stylesheet");
    }
    Ok(())
}

fn check_style(node: &roxmltree::Node) -> Result<(), ImportError> {
    let css: String = node.descendants().filter_map(|n| n.text()).collect();
    check_urls(&css)
}

/// A `<use>` may not point at something that itself uses: such chains multiply into millions of
/// shapes from a few bytes. Answers how many shapes this use draws.
fn check_use(
    by_id: &HashMap<&str, roxmltree::Node>,
    node: &roxmltree::Node,
) -> Result<usize, ImportError> {
    let target = node
        .attribute("href")
        .or_else(|| node.attribute(("http://www.w3.org/1999/xlink", "href")))
        .and_then(|h| h.strip_prefix('#'))
        .and_then(|id| by_id.get(id));
    let Some(target) = target else {
        return Ok(0);
    };
    let mut drawn = 0;
    for d in target.descendants().filter(roxmltree::Node::is_element) {
        let tag = d.tag_name().name().to_ascii_lowercase();
        if tag == "use" {
            return refuse(
                "the SVG has <use> elements that refer to other <use> elements; not imported",
            );
        }
        drawn += usize::from(SHAPE_ELEMENTS.contains(&tag.as_str()));
    }
    Ok(drawn)
}

/// A converted node and the box it covers, in the drawing's coordinates, until it is placed.
struct Built {
    node: NewNode,
    bounds: [f64; 4],
}

#[derive(Default)]
struct Dropped {
    clips: usize,
    masks: usize,
    filters: usize,
    patterns: usize,
    complex: usize,
    resampled: usize,
    elliptical: usize,
    reflected: usize,
}

impl Dropped {
    fn lines(&self) -> Vec<String> {
        let counted = [
            (
                self.clips,
                "clip path(s) were dropped: the shapes draw unclipped",
            ),
            (self.masks, "mask(s) were dropped: the shapes draw unmasked"),
            (
                self.filters,
                "filter(s) were dropped (blur, shadow): the shapes draw without them",
            ),
            (
                self.patterns,
                "pattern fill(s) were dropped: those shapes are not filled with it",
            ),
            (self.complex, "shape(s) over the path limits were skipped"),
            (
                self.resampled,
                "gradient(s) with more than 8 stops were reduced to 8",
            ),
            (
                self.elliptical,
                "stretched radial gradient(s) are drawn as circles",
            ),
            (
                self.reflected,
                "reflecting or repeating gradient(s) are drawn padded",
            ),
        ];
        counted
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, text)| format!("{n} {text}"))
            .collect()
    }
}

struct Converter {
    scale: f64,
    nodes: usize,
    unnamed: usize,
    notes: Dropped,
}

impl Converter {
    fn new(scale: f64) -> Converter {
        Converter {
            scale,
            nodes: 1,
            unnamed: 0,
            notes: Dropped::default(),
        }
    }

    fn count(&mut self) -> Result<(), ImportError> {
        self.nodes += 1;
        if self.nodes > MAX_NODES {
            return refuse(format!(
                "the drawing has more than {MAX_NODES} shapes; simplify it or merge shapes before importing"
            ));
        }
        Ok(())
    }

    /// The shapes of `group` and below. A group is kept as a group node when it has an id or an
    /// opacity of its own; any other is only a transform, which is flattened into its shapes.
    fn group(&mut self, group: &usvg::Group, depth: usize) -> Result<Vec<Built>, ImportError> {
        if depth > MAX_GROUP_DEPTH {
            return refuse(format!("groups nest deeper than {MAX_GROUP_DEPTH}"));
        }
        let mut out = Vec::new();
        for child in group.children() {
            match child {
                usvg::Node::Path(path) => out.extend(self.shape(path)?),
                usvg::Node::Group(inner) => {
                    self.note_effects(inner);
                    if keeps(inner) {
                        let kids = self.group(inner, depth + 1)?;
                        out.extend(self.wrap(inner, kids)?);
                    } else {
                        out.extend(self.group(inner, depth)?);
                    }
                }
                usvg::Node::Image(_) | usvg::Node::Text(_) => {}
            }
        }
        Ok(out)
    }

    fn note_effects(&mut self, group: &usvg::Group) {
        self.notes.clips += usize::from(group.clip_path().is_some());
        self.notes.masks += usize::from(group.mask().is_some());
        self.notes.filters += group.filters().len().min(1);
    }

    /// A group node around `kids`, placed at the top-left of what it holds.
    fn wrap(
        &mut self,
        group: &usvg::Group,
        kids: Vec<Built>,
    ) -> Result<Option<Built>, ImportError> {
        let Some(bounds) = union(&kids) else {
            return Ok(None);
        };
        self.count()?;
        let mut node = NewNode::new(NodeKind::Group);
        let name = clean_name(group.id());
        node.name = Some(if name.is_empty() {
            "group".to_string()
        } else {
            name
        });
        node.opacity = Some(f64::from(group.opacity().get())).filter(|o| *o < 1.0);
        (node.x, node.y) = (bounds[0], bounds[1]);
        for mut kid in kids {
            kid.node.x -= bounds[0];
            kid.node.y -= bounds[1];
            node.children.push(kid.node);
        }
        Ok(Some(Built { node, bounds }))
    }

    fn shape(&mut self, path: &usvg::Path) -> Result<Option<Built>, ImportError> {
        if !path.is_visible() || (path.fill().is_none() && path.stroke().is_none()) {
            return Ok(None);
        }
        let m = path
            .abs_transform()
            .post_scale(self.scale as f32, self.scale as f32);
        let Some(placed) = path.data().clone().transform(m) else {
            return Ok(None);
        };
        let outline = from_sk(&placed);
        let Some(b) = outline.bounds() else {
            return Ok(None);
        };
        let local = outline.scaled(1.0, 1.0, -b.x, -b.y);
        let data = local.to_data();
        if Path::parse(&data).is_err()
            || b.width > od_core::limits::MAX_SIZE
            || b.height > od_core::limits::MAX_SIZE
        {
            self.notes.complex += 1;
            return Ok(None);
        }
        let frame = Frame {
            origin: (b.x, b.y),
            size: (b.width, b.height),
            transform: m,
        };
        let kind = shape_kind(&local, b.width, b.height);
        let mut node = NewNode::new(kind);
        (node.x, node.y, node.width, node.height) = (b.x, b.y, b.width, b.height);
        if let Some(fill) = path.fill() {
            node.fill = self.paint(fill.paint(), fill.opacity().get(), &frame);
            if kind == NodeKind::Path && fill.rule() == usvg::FillRule::EvenOdd {
                node.fill_rule = Some(FillRule::Evenodd);
            }
        }
        node.stroke = path.stroke().and_then(|s| self.stroke(s, &frame));
        if node.fill.is_none() && node.stroke.is_none() {
            return Ok(None);
        }
        self.count()?;
        node.name = Some(self.name_of(path.id()));
        if kind == NodeKind::Path {
            node.path = Some(data);
        }
        Ok(Some(Built {
            node,
            bounds: [b.x, b.y, b.x + b.width, b.y + b.height],
        }))
    }

    fn name_of(&mut self, id: &str) -> String {
        let name = clean_name(id);
        if !name.is_empty() {
            return name;
        }
        self.unnamed += 1;
        format!("shape {}", self.unnamed)
    }

    fn stroke(&mut self, s: &usvg::Stroke, frame: &Frame) -> Option<Stroke> {
        let paint = self.paint(s.paint(), s.opacity().get(), frame)?;
        let k = f64::from(
            frame.transform.sx * frame.transform.sy - frame.transform.kx * frame.transform.ky,
        )
        .abs()
        .sqrt();
        let mut stroke = Stroke::new(paint, f64::from(s.width().get()) * k);
        stroke.cap = match s.linecap() {
            usvg::LineCap::Butt => LineCap::Butt,
            usvg::LineCap::Round => LineCap::Round,
            usvg::LineCap::Square => LineCap::Square,
        };
        stroke.join = match s.linejoin() {
            usvg::LineJoin::Round => LineJoin::Round,
            usvg::LineJoin::Bevel => LineJoin::Bevel,
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
        };
        stroke.dash = s
            .dasharray()
            .map(|d| d.iter().map(|v| f64::from(*v) * k).collect())
            .unwrap_or_default();
        Some(stroke)
    }

    fn paint(&mut self, paint: &usvg::Paint, opacity: f32, frame: &Frame) -> Option<Paint> {
        match paint {
            usvg::Paint::Color(c) => Some(Paint::Color(rgba(*c, opacity))),
            usvg::Paint::LinearGradient(g) => self.linear(g, opacity, frame),
            usvg::Paint::RadialGradient(g) => self.radial(g, opacity, frame),
            usvg::Paint::Pattern(_) => {
                self.notes.patterns += 1;
                None
            }
        }
    }

    fn linear(&mut self, g: &usvg::LinearGradient, opacity: f32, frame: &Frame) -> Option<Paint> {
        self.note_spread(g.spread_method());
        let stops = gradient_stops(g.stops(), opacity);
        let to_box = frame.to_box(g.transform());
        let (p1, p2) = (to_box(g.x1(), g.y1()), to_box(g.x2(), g.y2()));
        let (dx, dy) = (p2.0 - p1.0, p2.1 - p1.1);
        let length = dx.hypot(dy);
        let (w, h) = frame.size;
        let (cos, sin) = (dx / length, dy / length);
        let reach = (w * cos).abs() / 2.0 + (h * sin).abs() / 2.0;
        if length.is_nan() || length <= 1e-9 || reach <= 1e-9 {
            return stops.last().map(|s| s.color.clone());
        }
        let (center, a) = ((w / 2.0, h / 2.0), 2.0 * reach / length);
        let b = ((center.0 - p1.0) * cos + (center.1 - p1.1) * sin - reach) / length;
        let (stops, reduced) = reparametrize(&stops, a, b);
        self.notes.resampled += usize::from(reduced);
        Some(Paint::Linear {
            angle: dy.atan2(dx).to_degrees(),
            stops,
        })
    }

    fn radial(&mut self, g: &usvg::RadialGradient, opacity: f32, frame: &Frame) -> Option<Paint> {
        self.note_spread(g.spread_method());
        let stops = gradient_stops(g.stops(), opacity);
        let to_box = frame.to_box(g.transform());
        let (cx, cy, r) = (g.cx(), g.cy(), g.r().get());
        let c = to_box(cx, cy);
        let (rx, ry) = (dist(c, to_box(cx + r, cy)), dist(c, to_box(cx, cy + r)));
        let radius = (rx + ry) / 2.0;
        let longer = frame.size.0.max(frame.size.1);
        if radius.is_nan() || radius <= 1e-9 || longer <= 1e-9 {
            return stops.last().map(|s| s.color.clone());
        }
        if (rx - ry).abs() > 0.15 * radius {
            self.notes.elliptical += 1;
        }
        let stops = stops
            .into_iter()
            .map(|s| Stop {
                offset: s.offset.clamp(0.0, 1.0),
                ..s
            })
            .collect();
        let (w, h) = (frame.size.0.max(1e-9), frame.size.1.max(1e-9));
        Some(Paint::Radial {
            cx: c.0 / w,
            cy: c.1 / h,
            r: radius / longer,
            stops,
        })
    }

    fn note_spread(&mut self, spread: usvg::SpreadMethod) {
        self.notes.reflected += usize::from(spread != usvg::SpreadMethod::Pad);
    }
}

/// A group is its own node when the SVG named it or faded it as a whole.
fn keeps(group: &usvg::Group) -> bool {
    !group.id().is_empty() || group.opacity().get() < 1.0
}

fn union(items: &[Built]) -> Option<[f64; 4]> {
    items.iter().map(|b| b.bounds).reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].min(b[1]),
            a[2].max(b[2]),
            a[3].max(b[3]),
        ]
    })
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn rgba(c: usvg::Color, opacity: f32) -> Rgba {
    Rgba {
        r: c.red,
        g: c.green,
        b: c.blue,
        a: (opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
    }
}

/// Where a shape sits: its box in the drawing and the transform from the shape's own coordinates
/// to the drawing's.
struct Frame {
    origin: (f64, f64),
    size: (f64, f64),
    transform: usvg::tiny_skia_path::Transform,
}

impl Frame {
    /// Maps a point of a paint server (through its own transform) into the shape's box.
    fn to_box(
        &self,
        paint_transform: usvg::tiny_skia_path::Transform,
    ) -> impl Fn(f32, f32) -> (f64, f64) {
        let total = self.transform.pre_concat(paint_transform);
        let origin = self.origin;
        move |x, y| {
            let mut p = usvg::tiny_skia_path::Point::from_xy(x, y);
            total.map_point(&mut p);
            (f64::from(p.x) - origin.0, f64::from(p.y) - origin.1)
        }
    }
}

fn gradient_stops(stops: &[usvg::Stop], opacity: f32) -> Vec<Stop> {
    stops
        .iter()
        .map(|s| Stop {
            offset: f64::from(s.offset().get()),
            color: Paint::Color(rgba(s.color(), s.opacity().get() * opacity)),
        })
        .collect()
}

fn color_of(stop: &Stop) -> Rgba {
    match &stop.color {
        Paint::Color(c) => *c,
        _ => Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
    }
}

/// The color the stops give at offset `s`, held at the ends.
fn color_at(stops: &[Stop], s: f64) -> Rgba {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    };
    if s <= first.offset {
        return color_of(first);
    }
    for pair in stops.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if s <= b.offset {
            let span = b.offset - a.offset;
            let t = if span > 0.0 {
                (s - a.offset) / span
            } else {
                1.0
            };
            let (ca, cb) = (color_of(a), color_of(b));
            let mix =
                |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
            return Rgba {
                r: mix(ca.r, cb.r),
                g: mix(ca.g, cb.g),
                b: mix(ca.b, cb.b),
                a: mix(ca.a, cb.a),
            };
        }
    }
    color_of(last)
}

/// Stops given along the SVG's gradient line (offset `s`) as stops along ours (offset `t`, where
/// `s = a * t + b`), with the ends cut exactly at t = 0 and 1 and no more than eight in all. The
/// second answer says whether stops had to be dropped.
fn reparametrize(stops: &[Stop], a: f64, b: f64) -> (Vec<Stop>, bool) {
    let at = |t: f64, color: Rgba| Stop {
        offset: t,
        color: Paint::Color(color),
    };
    let mut out = vec![at(0.0, color_at(stops, b))];
    for s in stops {
        let t = (s.offset - b) / a;
        if t > 1e-6 && t < 1.0 - 1e-6 {
            out.push(at(t, color_of(s)));
        }
    }
    out.push(at(1.0, color_at(stops, a + b)));
    if out.len() <= od_core::paint::MAX_STOPS {
        return (out, false);
    }
    let last = out.len() - 1;
    let keep = od_core::paint::MAX_STOPS - 1;
    let reduced = (0..=keep).map(|i| out[i * last / keep].clone()).collect();
    (reduced, true)
}

fn from_sk(path: &usvg::tiny_skia_path::Path) -> Path {
    use usvg::tiny_skia_path::PathSegment;
    let p = |q: usvg::tiny_skia_path::Point| [f64::from(q.x), f64::from(q.y)];
    Path::from_segments(
        path.segments()
            .map(|s| match s {
                PathSegment::MoveTo(a) => Seg::MoveTo(p(a)),
                PathSegment::LineTo(a) => Seg::LineTo(p(a)),
                PathSegment::QuadTo(a, b) => Seg::QuadTo(p(a), p(b)),
                PathSegment::CubicTo(a, b, c) => Seg::CubicTo(p(a), p(b), p(c)),
                PathSegment::Close => Seg::Close,
            })
            .collect(),
    )
}

/// What kind of node draws `outline` (placed at the origin, `w` x `h` large): a rect or an ellipse
/// when it is exactly one, else a path.
fn shape_kind(outline: &Path, w: f64, h: f64) -> NodeKind {
    if is_rect(outline.segments(), w, h) {
        NodeKind::Rect
    } else if is_ellipse(outline.segments(), w, h) {
        NodeKind::Ellipse
    } else {
        NodeKind::Path
    }
}

fn near(a: f64, b: f64, size: f64) -> bool {
    (a - b).abs() <= 1e-3 * size.max(1.0)
}

/// Four corners of the box, joined by straight edges.
fn is_rect(segs: &[Seg], w: f64, h: f64) -> bool {
    let corners: Vec<[f64; 2]> = segs
        .iter()
        .filter_map(|s| match s {
            Seg::MoveTo(p) | Seg::LineTo(p) => Some(*p),
            _ => None,
        })
        .collect();
    let straight = segs
        .iter()
        .all(|s| matches!(s, Seg::MoveTo(_) | Seg::LineTo(_) | Seg::Close));
    let on_box = |p: &[f64; 2]| {
        (near(p[0], 0.0, w) || near(p[0], w, w)) && (near(p[1], 0.0, h) || near(p[1], h, h))
    };
    let distinct = |x: f64, y: f64| corners.iter().any(|p| near(p[0], x, w) && near(p[1], y, h));
    straight
        && w > 0.0
        && h > 0.0
        && corners.iter().all(on_box)
        && distinct(0.0, 0.0)
        && distinct(w, 0.0)
        && distinct(w, h)
        && distinct(0.0, h)
}

/// Four quarter-turn curves whose ends sit at the middle of the box's sides and whose handles are
/// where a circle's would be.
fn is_ellipse(segs: &[Seg], w: f64, h: f64) -> bool {
    const KAPPA: f64 = 0.552_284_75;
    let curves: Vec<[[f64; 2]; 3]> = segs
        .iter()
        .filter_map(|s| match s {
            Seg::CubicTo(a, b, c) => Some([*a, *b, *c]),
            _ => None,
        })
        .collect();
    let others = segs
        .iter()
        .filter(|s| !matches!(s, Seg::CubicTo(..)))
        .all(|s| matches!(s, Seg::MoveTo(_) | Seg::Close | Seg::LineTo(_)));
    if curves.len() != 4 || !others || w <= 0.0 || h <= 0.0 {
        return false;
    }
    let (rx, ry) = (w / 2.0, h / 2.0);
    let side = |p: [f64; 2]| {
        (near(p[0], 0.0, w) || near(p[0], w, w)) && near(p[1], ry, h)
            || (near(p[1], 0.0, h) || near(p[1], h, h)) && near(p[0], rx, w)
    };
    let handle = |p: [f64; 2]| {
        let (dx, dy) = ((p[0] - rx).abs(), (p[1] - ry).abs());
        let rim = |d: f64, r: f64| near(d, r, 2.0 * r);
        let reach = |d: f64, r: f64| near(d, r * KAPPA, 2.0 * r);
        (rim(dx, rx) && reach(dy, ry)) || (rim(dy, ry) && reach(dx, rx))
    };
    curves
        .iter()
        .all(|[a, b, c]| side(*c) && handle(*a) && handle(*b))
}
