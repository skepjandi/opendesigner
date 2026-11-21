//! CPU rasterizer for OpenDesigner documents: frames, groups, rectangles, ellipses, text, images
//! and 3D devices to pixels, and PNG export. Anything it cannot draw (a missing or unreadable
//! image) is reported as a warning so an export never silently drops content.

pub mod captions;
mod device;
pub mod export;
pub mod grade;
mod images;
mod incremental;
mod layers;
mod place;
mod text;
pub mod three_d;
pub mod transitions;
pub mod video;

pub use device::Media;
pub use images::dimensions as image_dimensions;
pub use incremental::{Refresh, Retained};
pub use tiny_skia::Pixmap;

use std::collections::{BTreeMap, HashMap};
use std::path::Path as FsPath;
use std::sync::Arc;

use od_core::{Camera, Document, Node, NodeId, NodeKind, OpError, limits};
use thiserror::Error;
use tiny_skia::{
    Color, FillRule, FilterQuality, Mask, Paint as SkPaint, Path, PathBuilder, PixmapPaint, Rect,
    Stroke as SkStroke, Transform,
};

/// Largest exported image, in pixels per side and in total (about 256 MB of RGBA).
pub const MAX_SIDE: u32 = limits::MAX_SIZE as u32;
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("node {0} does not exist")]
    NoSuchNode(String),
    #[error("node {id} is a {kind}, not a frame; only frames export")]
    NotAFrame { id: String, kind: &'static str },
    #[error(
        "{width}x{height} is too large to render (at most {MAX_SIDE} per side and {MAX_PIXELS} pixels)"
    )]
    TooLarge { width: u64, height: u64 },
    #[error("{0} is empty at this scale and cannot be rendered")]
    Empty(String),
    #[error("scale must be between 0.01 and 16")]
    BadScale,
    #[error(transparent)]
    Document(#[from] OpError),
    #[error("PNG encoding failed: {0}")]
    Encode(String),
    #[error("footage could not be read: {0}")]
    Footage(String),
    #[error(
        "{width} px wide cannot be cut into {count} equal slices (use a count that divides the width)"
    )]
    BadSlices { width: u32, count: u32 },
}

/// Content the renderer could not draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    ImageNotRendered {
        id: NodeId,
        reason: String,
    },
    /// The text wraps to more rows than its box is tall: it is drawn, but runs past the box.
    TextOverflow {
        id: NodeId,
        /// What the text needs and what the box offers, in document units.
        needed: u32,
        available: u32,
    },
}

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Warning::ImageNotRendered { id, reason } => {
                write!(f, "image {id} was not drawn: {reason}")
            }
            Warning::TextOverflow {
                id,
                needed,
                available,
            } => write!(
                f,
                "text {id} overflows its box: its lines need {needed} px of height and the box is {available} px; make the box taller, or the text smaller or shorter"
            ),
        }
    }
}

pub struct Rendered {
    pub pixmap: Pixmap,
    pub warnings: Vec<Warning>,
}

struct Ctx<'a> {
    tokens: &'a BTreeMap<String, String>,
    project_dir: Option<&'a FsPath>,
    warnings: Vec<Warning>,
    view: View,
    /// The frame's 3D devices, drawn before the bands at the frame's scale.
    devices: &'a HashMap<NodeId, Arc<Pixmap>>,
    /// Pixels per document unit.
    scale: f32,
    /// The video camera, when it is not at rest.
    camera: Option<place::CameraView>,
}

impl Ctx<'_> {
    /// Maps a node's coordinates to pixels: `chain` (camera aside), moved by the camera as far as
    /// `depth` says (the frame itself stays put), then into the layer being drawn.
    fn world(&self, chain: Transform, shift: Transform, depth: f32, moved: bool) -> Transform {
        let moved = match &self.camera {
            Some(camera) if moved => camera.transform(depth).pre_concat(chain),
            _ => chain,
        };
        if shift.is_identity() {
            moved
        } else {
            shift.pre_concat(moved)
        }
    }
}

/// The widest line and the height of all lines of `node`'s text, in document units (a box with
/// no width does not wrap). `None` when the node has no text to draw.
pub fn measure_text(node: &Node) -> Option<(f32, f32)> {
    text::measure(node)
}

/// Text in `node` and below that wraps to more rows than its box is tall. A box with no width
/// does not wrap and one with no height has nothing to overflow, so neither is checked.
pub fn check_text(node: &Node) -> Vec<Warning> {
    let mut found = Vec::new();
    collect_overflows(node, &mut found);
    found
}

fn collect_overflows(node: &Node, found: &mut Vec<Warning>) {
    if !node.visible {
        return;
    }
    if node.kind == NodeKind::Text
        && node.width > 0.0
        && node.height > 0.0
        && let Some((_, text_h)) = text::measure(node)
        && f64::from(text_h) > node.height + 0.5
    {
        found.push(Warning::TextOverflow {
            id: node.id.clone(),
            needed: text_h.ceil() as u32,
            available: node.height.floor() as u32,
        });
    }
    for child in &node.children {
        collect_overflows(child, found);
    }
}

/// Renders `frame` at `scale` (1.0 = one pixel per document unit). `project_dir` is the project
/// folder whose `media/` holds the images; without one, image nodes are reported, not drawn.
pub fn render_frame(
    doc: &Document,
    frame: &NodeId,
    scale: f32,
    project_dir: Option<&FsPath>,
) -> Result<Rendered, RenderError> {
    render_frame_with(doc, frame, scale, project_dir, &mut Media::default())
}

/// [`render_frame`] with `media` for screens that play recordings.
pub fn render_frame_with(
    doc: &Document,
    frame: &NodeId,
    scale: f32,
    project_dir: Option<&FsPath>,
    media: &mut Media,
) -> Result<Rendered, RenderError> {
    render_frame_in_bands(
        doc,
        frame,
        scale,
        project_dir,
        media,
        None,
        Camera::default(),
    )
}

/// [`render_frame_with`] seen through a video's `camera`: every node moves, turns and grows by
/// the camera as far as its `depth` says; the frame's own background stays.
pub fn render_frame_camera(
    doc: &Document,
    frame: &NodeId,
    scale: f32,
    project_dir: Option<&FsPath>,
    media: &mut Media,
    camera: Camera,
) -> Result<Rendered, RenderError> {
    render_frame_in_bands(doc, frame, scale, project_dir, media, None, camera)
}

/// The frame node and its size in pixels at `scale`, or why it cannot be drawn.
fn frame_geometry<'a>(
    doc: &'a Document,
    frame: &NodeId,
    scale: f32,
) -> Result<(&'a Node, (u32, u32)), RenderError> {
    if !(0.01..=16.0).contains(&scale) || !scale.is_finite() {
        return Err(RenderError::BadScale);
    }
    let node = doc
        .node(frame)
        .ok_or_else(|| RenderError::NoSuchNode(frame.to_string()))?;
    if node.kind != NodeKind::Frame {
        return Err(RenderError::NotAFrame {
            id: frame.to_string(),
            kind: node.kind.as_str(),
        });
    }
    let width = (node.width * f64::from(scale)).ceil() as u64;
    let height = (node.height * f64::from(scale)).ceil() as u64;
    if width == 0 || height == 0 {
        return Err(RenderError::Empty(frame.to_string()));
    }
    if width > u64::from(MAX_SIDE) || height > u64::from(MAX_SIDE) || width * height > MAX_PIXELS {
        return Err(RenderError::TooLarge { width, height });
    }
    Ok((node, (width as u32, height as u32)))
}

/// [`render_frame_with`] with the number of bands chosen (`None`: by size and cores).
fn render_frame_in_bands(
    doc: &Document,
    frame: &NodeId,
    scale: f32,
    project_dir: Option<&FsPath>,
    media: &mut Media,
    bands: Option<usize>,
    camera: Camera,
) -> Result<Rendered, RenderError> {
    let (node, (w, h)) = frame_geometry(doc, frame, scale)?;
    let bands = bands.unwrap_or_else(|| band_count(node, h));
    let mut warnings = Vec::new();
    let devices = device::prepare(node, scale, &doc.tokens, project_dir, media, &mut warnings)?;
    let mut rendered = render_bands(
        doc,
        node,
        scale,
        (w, h),
        project_dir,
        &devices,
        bands,
        camera,
    )?;
    for warning in warnings {
        if !rendered.warnings.contains(&warning) {
            rendered.warnings.push(warning);
        }
    }
    Ok(rendered)
}

/// Frames with at least this many nodes are drawn in horizontal bands on several threads.
const BAND_MIN_NODES: usize = 256;
const BAND_MIN_ROWS: u32 = 64;
const MAX_BANDS: usize = 8;
/// Rows drawn past each side of a band so seams match a single pass.
const BAND_OVERLAP: u32 = 4;

fn count_nodes(node: &Node, limit: usize) -> usize {
    let mut n = 1;
    for child in &node.children {
        if n >= limit {
            break;
        }
        n += count_nodes(child, limit - n);
    }
    n
}

/// How many bands to draw `node` in: one unless the frame is busy and tall enough to split.
fn band_count(node: &Node, height: u32) -> usize {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let wanted = threads
        .min(MAX_BANDS)
        .min((height / BAND_MIN_ROWS).max(1) as usize);
    if wanted < 2 || count_nodes(node, BAND_MIN_NODES) < BAND_MIN_NODES {
        1
    } else {
        wanted
    }
}

/// Draws the top-left `size` pixels of the frame in `bands` slices of rows. Slices are
/// independent pixmaps (each skips what lies outside it), so they run in parallel and are then
/// copied side by side; layering inside a slice is unchanged.
#[allow(clippy::too_many_arguments)]
fn render_bands(
    doc: &Document,
    node: &Node,
    scale: f32,
    (w, h): (u32, u32),
    project_dir: Option<&FsPath>,
    devices: &HashMap<NodeId, Arc<Pixmap>>,
    bands: usize,
    camera: Camera,
) -> Result<Rendered, RenderError> {
    let too_large = || RenderError::TooLarge {
        width: u64::from(w),
        height: u64::from(h),
    };
    let rows = h.div_ceil(bands.max(1) as u32).max(1);
    let starts: Vec<u32> = (0..h).step_by(rows as usize).collect();
    let draw_band = |top: u32| -> Result<Band, RenderError> {
        draw_rows(
            doc,
            node,
            scale,
            (w, h),
            project_dir,
            devices,
            top,
            rows,
            camera,
        )
    };
    let parts: Vec<Result<Band, RenderError>> = if starts.len() == 1 {
        vec![draw_band(0)]
    } else {
        std::thread::scope(|scope| {
            let handles: Vec<_> = starts
                .iter()
                .map(|&top| scope.spawn(move || draw_band(top)))
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        Err(RenderError::Encode("a render thread failed".into()))
                    })
                })
                .collect()
        })
    };
    let mut pixmap = Pixmap::new(w, h).ok_or_else(too_large)?;
    let mut warnings: Vec<Warning> = Vec::new();
    for (top, part) in starts.iter().zip(parts) {
        let (band, band_warnings) = part?;
        let at = *top as usize * w as usize * 4;
        pixmap.data_mut()[at..at + band.len()].copy_from_slice(&band);
        for warning in band_warnings {
            if !warnings.contains(&warning) {
                warnings.push(warning);
            }
        }
    }
    warnings.extend(check_text(node));
    Ok(Rendered { pixmap, warnings })
}

/// Draws rows `top..top + rows` of the `(w, h)` frame. A few rows beyond the slice on each side
/// are drawn too: a shape that crosses the seam is then anti-aliased with its whole outline in
/// view, and the seam shows no difference.
#[allow(clippy::too_many_arguments)]
fn draw_rows(
    doc: &Document,
    node: &Node,
    scale: f32,
    (w, h): (u32, u32),
    project_dir: Option<&FsPath>,
    devices: &HashMap<NodeId, Arc<Pixmap>>,
    top: u32,
    rows: u32,
    camera: Camera,
) -> Result<Band, RenderError> {
    let too_large = || RenderError::TooLarge {
        width: u64::from(w),
        height: u64::from(h),
    };
    let first = top.saturating_sub(BAND_OVERLAP);
    let last = (top + rows).min(h);
    let band_h = (last + BAND_OVERLAP).min(h) - first;
    let mut pixmap = Pixmap::new(w, band_h).ok_or_else(too_large)?;
    let base = Transform::from_row(scale, 0.0, 0.0, scale, 0.0, -(first as f32));
    let mut ctx = Ctx {
        tokens: &doc.tokens,
        project_dir,
        warnings: Vec::new(),
        view: View {
            width: w as f32,
            height: band_h as f32,
        },
        devices,
        scale,
        camera: place::CameraView::new(camera, base, node.width, node.height),
    };
    let start = Place {
        parent: base,
        shift: Transform::identity(),
        depth: 1.0,
        opacity: 1.0,
    };
    draw(&mut ctx, &mut pixmap, node, start, None, true)?;
    let skip = (top - first) as usize * w as usize * 4;
    let keep = (last - top) as usize * w as usize * 4;
    Ok((pixmap.data()[skip..skip + keep].to_vec(), ctx.warnings))
}

/// A drawn band's rows (RGBA, premultiplied) and what it could not draw.
type Band = (Vec<u8>, Vec<Warning>);

/// The pixels one band covers, in its own pixmap's coordinates.
#[derive(Clone, Copy)]
struct View {
    width: f32,
    height: f32,
}

/// Where a node's parent is drawn and what the node inherits from it.
#[derive(Clone, Copy)]
struct Place {
    /// Maps the parent's coordinates to pixels, camera aside.
    parent: Transform,
    /// Moves the drawing into an effect layer's pixmap (the identity outside one).
    shift: Transform,
    /// The depths of the ancestors multiplied: how far the camera moves what is drawn.
    depth: f32,
    opacity: f32,
}

fn draw(
    ctx: &mut Ctx,
    pixmap: &mut Pixmap,
    node: &Node,
    place: Place,
    mask: Option<&Mask>,
    is_root: bool,
) -> Result<(), RenderError> {
    draw_node(ctx, pixmap, node, place, mask, is_root, true)
}

/// Draws `node` and what is below it. With `layers` off the node is drawn straight onto
/// `pixmap` even when it blurs or casts a shadow (the layer that does those is being drawn).
fn draw_node(
    ctx: &mut Ctx,
    pixmap: &mut Pixmap,
    node: &Node,
    place: Place,
    mask: Option<&Mask>,
    is_root: bool,
    layers: bool,
) -> Result<(), RenderError> {
    if !node.visible {
        return Ok(());
    }
    // The exported frame sits at the origin and ignores its own transform.
    let chain = if is_root {
        place.parent
    } else {
        place::node_chain(place.parent, node)
    };
    let depth = if is_root {
        place.depth
    } else {
        place.depth * node.depth as f32
    };
    let transform = ctx.world(chain, place.shift, depth, !is_root);
    if layers && !is_root && layers::needs_layer(node) {
        return layers::draw_layered(ctx, pixmap, node, place, mask);
    }
    let opacity = place.opacity * node.opacity as f32;
    let (w, h) = (node.width as f32, node.height as f32);

    // Anything wholly outside the band is not drawn. A frame clips its children, so its subtree
    // goes with it; text can overflow its box downward and sideways, so only a start below the
    // band counts for it.
    let margin = node
        .stroke
        .as_ref()
        .map_or(0.0, |s| s.width as f32 * place::scale_of(transform) / 2.0)
        + 1.0;
    let (left, top, right, bottom) = place::device_rect(transform, 0.0, 0.0, w, h);
    let outside = if node.kind == NodeKind::Text {
        place::is_upright(transform) && top - margin > ctx.view.height
    } else {
        right + margin < 0.0
            || bottom + margin < 0.0
            || left - margin > ctx.view.width
            || top - margin > ctx.view.height
    };
    if outside && (node.kind == NodeKind::Frame || node.children.is_empty()) {
        return Ok(());
    }

    let shape = if outside {
        None
    } else {
        match node.kind {
            NodeKind::Frame | NodeKind::Rect => {
                rounded_rect(w, h, (node.corner_radius as f32).min(w.min(h) / 2.0))
            }
            NodeKind::Ellipse => Rect::from_xywh(0.0, 0.0, w, h).and_then(PathBuilder::from_oval),
            NodeKind::Group => None,
            NodeKind::Text => {
                let color = match &node.fill {
                    Some(fill) => fill.resolve(ctx.tokens)?,
                    None => od_core::Rgba {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                };
                text::draw_placed(pixmap, mask, node, transform, color, opacity);
                None
            }
            NodeKind::Image => {
                match load_image(ctx, node) {
                    Ok(image) => {
                        let image = grade::graded(image, node.grade.as_ref());
                        images::draw_image(pixmap, mask, node, transform, opacity, &image);
                    }
                    Err(reason) => ctx.warnings.push(Warning::ImageNotRendered {
                        id: node.id.clone(),
                        reason,
                    }),
                }
                None
            }
            NodeKind::Device3d => {
                if let Some(drawn) = ctx.devices.get(&node.id) {
                    draw_device(pixmap, mask, drawn, transform, opacity, ctx.scale);
                }
                None
            }
        }
    };

    if let Some(path) = &shape {
        if let Some(fill) = &node.fill {
            let color = fill.resolve(ctx.tokens)?;
            pixmap.fill_path(
                path,
                &solid(color, opacity),
                FillRule::Winding,
                transform,
                mask,
            );
        }
        if let Some(stroke) = &node.stroke {
            let color = stroke.paint.resolve(ctx.tokens)?;
            let style = SkStroke {
                width: stroke.width as f32,
                ..SkStroke::default()
            };
            pixmap.stroke_path(path, &solid(color, opacity), &style, transform, mask);
        }
    }

    if node.children.is_empty() {
        return Ok(());
    }
    // Frames, and groups that ask for it, clip their children to their own (rounded) bounds.
    let clip = match (node.kind, &shape) {
        (NodeKind::Frame, Some(path)) => Some(clip_mask(pixmap, mask, path, transform)?),
        (NodeKind::Group, _) if node.clip => match layers::group_clip(node) {
            Some(path) => Some(clip_mask(pixmap, mask, &path, transform)?),
            None => None,
        },
        _ => None,
    };
    let child_mask = clip.as_ref().or(mask);
    let inner = Place {
        parent: chain,
        depth,
        opacity,
        ..place
    };
    for child in &node.children {
        draw(ctx, pixmap, child, inner, child_mask, false)?;
    }
    Ok(())
}

/// Places a device drawn at the frame's scale (one of its pixels per output pixel) at the node.
fn draw_device(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    drawn: &Pixmap,
    transform: Transform,
    opacity: f32,
    scale: f32,
) {
    let plain = transform.kx == 0.0
        && transform.ky == 0.0
        && transform.sx == scale
        && transform.sy == scale;
    let (at, quality) = if plain {
        let aligned = transform.tx.fract() == 0.0 && transform.ty.fract() == 0.0;
        let quality = if aligned {
            FilterQuality::Nearest
        } else {
            FilterQuality::Bilinear
        };
        (
            Transform::from_translate(transform.tx, transform.ty),
            quality,
        )
    } else {
        (
            transform.pre_scale(1.0 / scale, 1.0 / scale),
            FilterQuality::Bilinear,
        )
    };
    let paint = PixmapPaint {
        opacity,
        quality,
        ..PixmapPaint::default()
    };
    pixmap.draw_pixmap(0, 0, drawn.as_ref(), &paint, at, mask);
}

/// The decoded image an image node refers to, or why there is none.
fn load_image(ctx: &Ctx, node: &Node) -> Result<std::sync::Arc<Pixmap>, String> {
    let reference = node.image.as_deref().ok_or("no image is set")?;
    let dir = ctx
        .project_dir
        .ok_or("the document has no project folder to read media from")?;
    let path = od_core::media::resolve(dir, reference).map_err(|e| e.to_string())?;
    images::load(&path)
}

fn clip_mask(
    pixmap: &Pixmap,
    parent: Option<&Mask>,
    path: &Path,
    transform: Transform,
) -> Result<Mask, RenderError> {
    let too_large = || RenderError::TooLarge {
        width: u64::from(pixmap.width()),
        height: u64::from(pixmap.height()),
    };
    match parent {
        Some(m) => {
            let mut mask = m.clone();
            mask.intersect_path(path, FillRule::Winding, true, transform);
            Ok(mask)
        }
        None => {
            let mut mask = Mask::new(pixmap.width(), pixmap.height()).ok_or_else(too_large)?;
            mask.fill_path(path, FillRule::Winding, true, transform);
            Ok(mask)
        }
    }
}

fn solid(color: od_core::Rgba, opacity: f32) -> SkPaint<'static> {
    let alpha = (f32::from(color.a) / 255.0 * opacity).clamp(0.0, 1.0);
    let mut paint = SkPaint::default();
    paint.set_color(
        Color::from_rgba(
            f32::from(color.r) / 255.0,
            f32::from(color.g) / 255.0,
            f32::from(color.b) / 255.0,
            alpha,
        )
        .unwrap_or(Color::TRANSPARENT),
    );
    paint.anti_alias = true;
    paint
}

fn rounded_rect(w: f32, h: f32, r: f32) -> Option<Path> {
    let rect = Rect::from_xywh(0.0, 0.0, w, h)?;
    if r <= 0.0 {
        return Some(PathBuilder::from_rect(rect));
    }
    const K: f32 = 0.552_284_8;
    let c = r * K;
    let mut p = PathBuilder::new();
    p.move_to(r, 0.0);
    p.line_to(w - r, 0.0);
    p.cubic_to(w - r + c, 0.0, w, r - c, w, r);
    p.line_to(w, h - r);
    p.cubic_to(w, h - r + c, w - r + c, h, w - r, h);
    p.line_to(r, h);
    p.cubic_to(r - c, h, 0.0, h - r + c, 0.0, h - r);
    p.line_to(0.0, r);
    p.cubic_to(0.0, r - c, r - c, 0.0, r, 0.0);
    p.close();
    p.finish()
}

/// Pictures of one size laid out left to right, top to bottom, `cols` to a row, on a dark ground
/// with a gap around each: a contact sheet. `None` when there are none or it is too large.
pub fn grid(pictures: &[Pixmap], cols: u32) -> Option<Pixmap> {
    const GAP: u32 = 6;
    let first = pictures.first()?;
    let (cw, ch) = (first.width(), first.height());
    let cols = cols.clamp(1, pictures.len() as u32);
    let rows = (pictures.len() as u32).div_ceil(cols);
    let mut sheet = Pixmap::new(cols * cw + (cols + 1) * GAP, rows * ch + (rows + 1) * GAP)?;
    sheet.fill(Color::from_rgba8(0x1b, 0x1b, 0x1f, 0xff));
    for (n, picture) in pictures.iter().enumerate() {
        let (col, row) = (n as u32 % cols, n as u32 / cols);
        sheet.draw_pixmap(
            (GAP + col * (cw + GAP)) as i32,
            (GAP + row * (ch + GAP)) as i32,
            picture.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            None,
        );
    }
    Some(sheet)
}

/// Cuts `pixmap` into `count` equal-width columns, left to right: the slices of a panorama.
pub fn slice_columns(pixmap: &Pixmap, count: u32) -> Result<Vec<Pixmap>, RenderError> {
    let width = pixmap.width();
    if count == 0 || width % count != 0 {
        return Err(RenderError::BadSlices { width, count });
    }
    let each = width / count;
    (0..count)
        .map(|i| {
            let area = tiny_skia::IntRect::from_xywh((i * each) as i32, 0, each, pixmap.height());
            area.and_then(|r| pixmap.clone_rect(r))
                .ok_or(RenderError::BadSlices { width, count })
        })
        .collect()
}

/// Encodes a PNG. With `alpha` false the image is flattened onto white and written as RGB, which
/// is what the stores require for screenshots.
/// How much of the picture is fully transparent (0 to 1). A frame without a background is
/// transparent, and an export that flattens it onto white then looks like a white screen.
pub fn transparent_share(pixmap: &Pixmap) -> f64 {
    let total = pixmap.width() as usize * pixmap.height() as usize;
    if total == 0 {
        return 0.0;
    }
    let clear = pixmap
        .data()
        .chunks_exact(4)
        .filter(|px| px[3] == 0)
        .count();
    clear as f64 / total as f64
}

/// A small checkerboard (32 x 32, two greys) as a PNG, for the canvas to show behind a frame
/// that has no background.
pub fn checkerboard_png() -> Option<Vec<u8>> {
    const CELL: u32 = 16;
    const SIZE: u32 = 32;
    let mut data = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let light = ((x / CELL) + (y / CELL)) % 2 == 0;
            let v = if light { 0x3a } else { 0x2e };
            data.extend([v, v, v, 255]);
        }
    }
    let mut pixmap = Pixmap::new(SIZE, SIZE)?;
    pixmap.data_mut().copy_from_slice(&data);
    pixmap.as_ref().encode_png().ok()
}

pub fn encode_png(pixmap: &Pixmap, alpha: bool) -> Result<Vec<u8>, RenderError> {
    if alpha {
        return pixmap
            .encode_png()
            .map_err(|e| RenderError::Encode(e.to_string()));
    }
    let mut rgb = Vec::with_capacity(pixmap.width() as usize * pixmap.height() as usize * 3);
    for px in pixmap.data().chunks_exact(4) {
        let over_white = 255 - px[3];
        rgb.extend([
            px[0].saturating_add(over_white),
            px[1].saturating_add(over_white),
            px[2].saturating_add(over_white),
        ]);
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| RenderError::Encode(e.to_string()))?;
    writer
        .write_image_data(&rgb)
        .map_err(|e| RenderError::Encode(e.to_string()))?;
    writer
        .finish()
        .map_err(|e| RenderError::Encode(e.to_string()))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{Actor, NewNode, Operation, Paint, Session, Stroke};

    fn frame_session() -> (Session, NodeId) {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut frame = NewNode::new(NodeKind::Frame);
        frame.width = 100.0;
        frame.height = 50.0;
        frame.fill = Some(Paint::parse("#ff0000").unwrap());
        let r = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: root,
                    index: None,
                    node: frame,
                }],
            )
            .unwrap();
        let id = r.created[0].clone();
        (s, id)
    }

    fn add(s: &mut Session, parent: &NodeId, node: NewNode) -> NodeId {
        let r = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: parent.clone(),
                    index: None,
                    node,
                }],
            )
            .unwrap();
        r.created[0].clone()
    }

    fn px(r: &Rendered, x: u32, y: u32) -> [u8; 4] {
        let c = r.pixmap.pixel(x, y).unwrap();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }

    fn rect(x: f64, y: f64, w: f64, h: f64, fill: &str) -> NewNode {
        let mut n = NewNode::new(NodeKind::Rect);
        n.x = x;
        n.y = y;
        n.width = w;
        n.height = h;
        n.fill = Some(Paint::parse(fill).unwrap());
        n
    }

    #[test]
    fn fills_the_frame_and_draws_children_at_their_offsets() {
        let (mut s, frame) = frame_session();
        add(&mut s, &frame, rect(10.0, 10.0, 20.0, 20.0, "#0000ff"));
        let r = render_frame(s.document(), &frame, 1.0, None).unwrap();
        assert_eq!((r.pixmap.width(), r.pixmap.height()), (100, 50));
        assert_eq!(px(&r, 5, 5), [255, 0, 0, 255]);
        assert_eq!(px(&r, 20, 20), [0, 0, 255, 255]);
        assert_eq!(px(&r, 35, 20), [255, 0, 0, 255]);
        assert!(r.warnings.is_empty());
    }

    #[test]
    fn children_are_clipped_to_the_frame_and_opacity_multiplies_down() {
        let (mut s, frame) = frame_session();
        let mut group = NewNode::new(NodeKind::Group);
        group.opacity = Some(0.5);
        group.children.push(rect(90.0, 0.0, 50.0, 50.0, "#0000ff"));
        add(&mut s, &frame, group);
        let r = render_frame(s.document(), &frame, 1.0, None).unwrap();
        let blended = px(&r, 95, 10);
        assert!(blended[0] > 100 && blended[0] < 160, "{blended:?}");
        assert!(blended[2] > 100 && blended[2] < 160, "{blended:?}");
        assert_eq!(r.pixmap.width(), 100);
    }

    #[test]
    fn scale_multiplies_the_output_size_and_geometry() {
        let (mut s, frame) = frame_session();
        add(&mut s, &frame, rect(10.0, 10.0, 20.0, 20.0, "#0000ff"));
        let r = render_frame(s.document(), &frame, 2.0, None).unwrap();
        assert_eq!((r.pixmap.width(), r.pixmap.height()), (200, 100));
        assert_eq!(px(&r, 40, 40), [0, 0, 255, 255]);
        assert_eq!(px(&r, 10, 10), [255, 0, 0, 255]);
    }

    #[test]
    fn rounded_corners_and_strokes_are_drawn() {
        let (mut s, frame) = frame_session();
        let mut card = rect(0.0, 0.0, 100.0, 50.0, "#00ff00");
        card.corner_radius = Some(25.0);
        card.stroke = Some(Stroke {
            paint: Paint::parse("#000000").unwrap(),
            width: 4.0,
        });
        add(&mut s, &frame, card);
        let r = render_frame(s.document(), &frame, 1.0, None).unwrap();
        assert_eq!(
            px(&r, 0, 0),
            [255, 0, 0, 255],
            "the corner shows the frame, not the card"
        );
        assert_eq!(px(&r, 50, 25), [0, 255, 0, 255]);
        assert_eq!(px(&r, 50, 0), [0, 0, 0, 255], "top edge is stroked");
    }

    #[test]
    fn an_image_that_cannot_be_drawn_is_reported_with_the_reason() {
        let (mut s, frame) = frame_session();
        let none = add(&mut s, &frame, NewNode::new(NodeKind::Image));
        let mut missing = NewNode::new(NodeKind::Image);
        missing.image = Some("media/gone.png".into());
        let missing = add(&mut s, &frame, missing);
        let dir = tempfile::tempdir().unwrap();
        let r = render_frame(s.document(), &frame, 1.0, Some(dir.path())).unwrap();
        let reasons: Vec<(NodeId, String)> = r
            .warnings
            .iter()
            .filter_map(|w| match w {
                Warning::ImageNotRendered { id, reason } => Some((id.clone(), reason.clone())),
                Warning::TextOverflow { .. } => None,
            })
            .collect();
        assert_eq!(reasons.len(), 2);
        assert_eq!(reasons[0].0, none);
        assert!(reasons[0].1.contains("no image"), "{reasons:?}");
        assert_eq!(reasons[1].0, missing);
        let without_folder = render_frame(s.document(), &frame, 1.0, None).unwrap();
        assert_eq!(without_folder.warnings.len(), 2);
        assert!(
            without_folder.warnings[0]
                .to_string()
                .contains("was not drawn")
        );
    }

    /// Writes a PNG into `<dir>/media/<name>`: green on the left half, blue on the right.
    fn two_tone(dir: &std::path::Path, name: &str, w: u32, h: u32) -> String {
        let mut p = Pixmap::new(w, h).unwrap();
        for (i, px) in p.data_mut().chunks_exact_mut(4).enumerate() {
            let x = i as u32 % w;
            let c = if x < w / 2 {
                [0, 255, 0, 255]
            } else {
                [0, 0, 255, 255]
            };
            px.copy_from_slice(&c);
        }
        std::fs::create_dir_all(dir.join("media")).unwrap();
        std::fs::write(dir.join("media").join(name), encode_png(&p, true).unwrap()).unwrap();
        format!("media/{name}")
    }

    fn image_frame(
        dir: &std::path::Path,
        scale: f32,
        configure: impl FnOnce(&mut NewNode),
    ) -> Rendered {
        let (mut s, frame) = frame_session();
        let mut n = NewNode::new(NodeKind::Image);
        n.image = Some(two_tone(dir, "shot.png", 20, 10));
        configure(&mut n);
        add(&mut s, &frame, n);
        render_frame(s.document(), &frame, scale, Some(dir)).unwrap()
    }

    #[test]
    fn images_fill_cover_or_contain_their_box() {
        let dir = tempfile::tempdir().unwrap();
        let at = |n: &mut NewNode, x, y, w, h, fit| {
            n.x = x;
            n.y = y;
            n.width = w;
            n.height = h;
            n.image_fit = fit;
        };
        let stretched = image_frame(dir.path(), 1.0, |n| {
            at(n, 0.0, 0.0, 100.0, 50.0, Some(od_core::ImageFit::Fill));
        });
        assert_eq!(px(&stretched, 20, 25), [0, 255, 0, 255]);
        assert_eq!(px(&stretched, 80, 25), [0, 0, 255, 255]);
        assert!(stretched.warnings.is_empty());

        // a 2:1 picture covering a square keeps its middle: half green, half blue
        let covered = image_frame(dir.path(), 1.0, |n| at(n, 10.0, 5.0, 40.0, 40.0, None));
        assert_eq!(px(&covered, 15, 25), [0, 255, 0, 255]);
        assert_eq!(px(&covered, 45, 25), [0, 0, 255, 255]);
        assert_eq!(
            px(&covered, 15, 6),
            [0, 255, 0, 255],
            "the whole box is covered"
        );

        // contained, it is 40x20 centered vertically: the bands above and below stay red
        let contained = image_frame(dir.path(), 1.0, |n| {
            at(n, 10.0, 5.0, 40.0, 40.0, Some(od_core::ImageFit::Contain));
        });
        assert_eq!(px(&contained, 15, 25), [0, 255, 0, 255]);
        assert_eq!(px(&contained, 15, 8), [255, 0, 0, 255]);
        assert_eq!(px(&contained, 15, 42), [255, 0, 0, 255]);
    }

    #[test]
    fn a_graded_image_node_is_drawn_graded() {
        let dir = tempfile::tempdir().unwrap();
        let (mut s, frame) = frame_session();
        let mut n = NewNode::new(NodeKind::Image);
        n.image = Some(two_tone(dir.path(), "shot.png", 20, 10));
        n.width = 100.0;
        n.height = 25.0;
        n.image_fit = Some(od_core::ImageFit::Fill);
        let id = add(&mut s, &frame, n);
        let plain = render_frame(s.document(), &frame, 1.0, Some(dir.path())).unwrap();
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id,
                prop: od_core::Property::Grade(Some(od_core::Grade {
                    exposure: -1.0,
                    saturation: -1.0,
                    ..od_core::Grade::default()
                })),
            }],
        )
        .unwrap();
        let graded = render_frame(s.document(), &frame, 1.0, Some(dir.path())).unwrap();
        assert_eq!(px(&plain, 20, 12), [0, 255, 0, 255]);
        let [r, g, b, a] = px(&graded, 20, 12);
        assert!(
            r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1 && a == 255,
            "{r} {g} {b}"
        );
        assert!(g.abs_diff(91) <= 2, "half the green's luma: {g}");
        assert_eq!(
            px(&graded, 20, 40),
            [255, 0, 0, 255],
            "below the image is not graded"
        );
    }

    #[test]
    fn images_follow_scale_corner_radius_and_opacity() {
        let dir = tempfile::tempdir().unwrap();
        let doubled = image_frame(dir.path(), 2.0, |n| {
            n.width = 100.0;
            n.height = 50.0;
        });
        assert_eq!(px(&doubled, 40, 50), [0, 255, 0, 255]);
        assert_eq!(px(&doubled, 160, 50), [0, 0, 255, 255]);

        let rounded = image_frame(dir.path(), 1.0, |n| {
            n.width = 100.0;
            n.height = 50.0;
            n.corner_radius = Some(25.0);
        });
        assert_eq!(
            px(&rounded, 0, 0),
            [255, 0, 0, 255],
            "the corner is cut away"
        );
        assert_eq!(px(&rounded, 80, 25), [0, 0, 255, 255]);

        let faint = image_frame(dir.path(), 1.0, |n| {
            n.width = 100.0;
            n.height = 50.0;
            n.opacity = Some(0.5);
        });
        let mixed = px(&faint, 10, 25);
        assert!(
            mixed[0] > 100 && mixed[1] > 100,
            "red shows through: {mixed:?}"
        );
    }

    #[test]
    fn a_large_image_drawn_small_is_averaged_not_aliased() {
        let dir = tempfile::tempdir().unwrap();
        let mut checker = Pixmap::new(64, 64).unwrap();
        for (i, px) in checker.data_mut().chunks_exact_mut(4).enumerate() {
            let (x, y) = (i % 64, i / 64);
            let v = if (x + y) % 2 == 0 { 255 } else { 0 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        std::fs::create_dir_all(dir.path().join("media")).unwrap();
        std::fs::write(
            dir.path().join("media/checker.png"),
            encode_png(&checker, true).unwrap(),
        )
        .unwrap();
        let r = image_frame(dir.path(), 1.0, |n| {
            n.image = Some("media/checker.png".into());
            n.width = 8.0;
            n.height = 8.0;
        });
        let v = px(&r, 4, 4)[0];
        assert!(
            (90..=165).contains(&v),
            "a 1px checker shrunk 8x is mid gray, got {v}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_media_symlink_pointing_outside_the_project_is_not_followed() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let real = two_tone(outside.path(), "secret.png", 20, 10);
        std::fs::create_dir_all(project.path().join("media")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join(real),
            project.path().join("media/link.png"),
        )
        .unwrap();
        let r = image_frame(project.path(), 1.0, |n| {
            n.image = Some("media/link.png".into());
            n.width = 100.0;
            n.height = 50.0;
        });
        assert_eq!(r.warnings.len(), 1);
        assert_eq!(px(&r, 10, 10), [255, 0, 0, 255]);
    }

    /// A white 400x200 frame with one black text node at (20, 20).
    fn text_frame(configure: impl FnOnce(&mut NewNode)) -> Rendered {
        let (mut s, frame) = frame_session();
        let set = |p: od_core::Property| Operation::Set {
            id: frame.clone(),
            prop: p,
        };
        s.apply(
            Actor::User,
            None,
            &[
                set(od_core::Property::Width(400.0)),
                set(od_core::Property::Height(200.0)),
                set(od_core::Property::Fill(Some(
                    Paint::parse("#ffffff").unwrap(),
                ))),
            ],
        )
        .unwrap();
        let mut t = NewNode::new(NodeKind::Text);
        t.x = 20.0;
        t.y = 20.0;
        t.fill = Some(Paint::parse("#000000").unwrap());
        t.font_size = Some(40.0);
        configure(&mut t);
        add(&mut s, &frame, t);
        render_frame(s.document(), &frame, 1.0, None).unwrap()
    }

    #[test]
    fn text_that_does_not_fit_its_box_is_reported_and_still_drawn() {
        let r = text_frame(|t| {
            t.width = 120.0;
            t.height = 20.0;
            t.text = Some("a long line of words that wraps over several rows".into());
        });
        let [
            Warning::TextOverflow {
                needed, available, ..
            },
        ] = r.warnings.as_slice()
        else {
            panic!("{:?}", r.warnings);
        };
        assert_eq!(*available, 20);
        assert!(*needed > 100, "several 48 px rows, got {needed}");
        assert!(ink(&r).0 > 0, "the text is still drawn");
        assert!(r.warnings[0].to_string().contains("overflows its box"));
    }

    #[test]
    fn a_word_too_wide_for_the_box_breaks_into_rows_that_overflow_and_fitting_text_is_quiet() {
        let wide = text_frame(|t| {
            t.width = 60.0;
            t.height = 100.0;
            t.text = Some("Extraordinarily".into());
        });
        assert!(
            matches!(wide.warnings.as_slice(), [Warning::TextOverflow { .. }]),
            "{:?}",
            wide.warnings
        );
        let fits = text_frame(|t| {
            t.width = 300.0;
            t.height = 100.0;
            t.text = Some("Hello".into());
        });
        assert!(fits.warnings.is_empty(), "{:?}", fits.warnings);
        let unboxed = text_frame(|t| {
            t.width = 0.0;
            t.height = 0.0;
            t.text = Some("No box at all, so nothing to overflow".into());
        });
        assert!(unboxed.warnings.is_empty(), "{:?}", unboxed.warnings);
    }

    /// Count of non-white pixels and their bounding box (x0, y0, x1, y1).
    fn ink(r: &Rendered) -> (usize, Option<(u32, u32, u32, u32)>) {
        let mut count = 0;
        let mut bounds: Option<(u32, u32, u32, u32)> = None;
        for y in 0..r.pixmap.height() {
            for x in 0..r.pixmap.width() {
                if px(r, x, y) != [255, 255, 255, 255] {
                    count += 1;
                    bounds = Some(match bounds {
                        None => (x, y, x, y),
                        Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
                    });
                }
            }
        }
        (count, bounds)
    }

    #[test]
    fn text_is_drawn_at_its_position_and_renders_the_same_every_time() {
        let draw = || {
            text_frame(|t| {
                t.text = Some("Hold".into());
            })
        };
        let (first, second) = (draw(), draw());
        assert_eq!(first.pixmap.data(), second.pixmap.data());
        let (count, bounds) = ink(&first);
        let (x0, y0, x1, y1) = bounds.expect("text leaves ink");
        assert!(count > 200, "{count} inked pixels");
        assert!(
            x0 >= 18 && y0 >= 18,
            "ink starts at the node's corner: {x0},{y0}"
        );
        assert!(
            x1 < 20 + 160 && y1 < 20 + 60,
            "four glyphs at 40px: {x1},{y1}"
        );
        assert!(first.warnings.is_empty());
    }

    #[test]
    fn weight_changes_the_glyphs_and_size_scales_them() {
        let regular = ink(&text_frame(|t| t.text = Some("Hold".into()))).0;
        let bold = ink(&text_frame(|t| {
            t.text = Some("Hold".into());
            t.font_weight = Some(700.0);
        }))
        .0;
        let large = ink(&text_frame(|t| {
            t.text = Some("Hold".into());
            t.font_size = Some(80.0);
        }))
        .0;
        assert!(bold > regular, "bold {bold} vs regular {regular}");
        assert!(large > regular * 3, "large {large} vs regular {regular}");
    }

    #[test]
    fn alignment_moves_the_ink_inside_the_box_and_a_width_wraps_lines() {
        let boxed = |align| {
            ink(&text_frame(|t| {
                t.text = Some("Hi".into());
                t.width = 300.0;
                t.text_align = align;
            }))
            .1
            .expect("ink")
        };
        let left = boxed(None);
        let centre = boxed(Some(od_core::TextAlign::Center));
        let right = boxed(Some(od_core::TextAlign::Right));
        assert!(
            left.0 < centre.0 && centre.0 < right.0,
            "{left:?} {centre:?} {right:?}"
        );
        assert!(
            right.2 <= 20 + 300,
            "right-aligned ink stays inside the box"
        );

        let one_line = ink(&text_frame(|t| t.text = Some("aaa bbb ccc ddd".into())))
            .1
            .expect("ink");
        let wrapped = ink(&text_frame(|t| {
            t.text = Some("aaa bbb ccc ddd".into());
            t.width = 120.0;
        }))
        .1
        .expect("ink");
        assert!(
            wrapped.3 > one_line.3 + 30,
            "wrapped {wrapped:?} vs one line {one_line:?}"
        );
        assert!(
            wrapped.2 <= 20 + 120 + 2,
            "no line is wider than the box: {wrapped:?}"
        );
    }

    #[test]
    fn text_respects_opacity_the_frame_clip_and_unknown_families() {
        let faint = text_frame(|t| {
            t.text = Some("Hold".into());
            t.opacity = Some(0.25);
        });
        let darkest = (0..faint.pixmap.width())
            .flat_map(|x| (0..faint.pixmap.height()).map(move |y| (x, y)))
            .map(|(x, y)| px(&faint, x, y)[0])
            .min()
            .expect("pixels");
        assert!(
            darkest > 150,
            "a quarter-opacity black glyph stays light: {darkest}"
        );

        let clipped = text_frame(|t| {
            t.text = Some("Hold".into());
            t.x = 380.0;
        });
        assert_eq!(clipped.pixmap.width(), 400);
        assert!(ink(&clipped).0 > 0, "the part inside the frame is drawn");

        let fallback = text_frame(|t| {
            t.text = Some("Hold".into());
            t.font_family = Some("No Such Family Xyz".into());
        });
        assert!(
            ink(&fallback).0 > 200,
            "a missing family falls back to the bundled font"
        );
    }

    #[test]
    fn refuses_what_it_cannot_render() {
        let (mut s, frame) = frame_session();
        let rect_id = add(&mut s, &frame, rect(0.0, 0.0, 1.0, 1.0, "#000000"));
        assert!(matches!(
            render_frame(s.document(), &rect_id, 1.0, None),
            Err(RenderError::NotAFrame { .. })
        ));
        assert!(matches!(
            render_frame(s.document(), &NodeId::parse("n_999").unwrap(), 1.0, None),
            Err(RenderError::NoSuchNode(_))
        ));
        assert!(matches!(
            render_frame(s.document(), &frame, 0.0, None),
            Err(RenderError::BadScale)
        ));
        assert!(matches!(
            render_frame(s.document(), &frame, f32::NAN, None),
            Err(RenderError::BadScale)
        ));
        s.apply(
            Actor::User,
            None,
            &[
                Operation::Set {
                    id: frame.clone(),
                    prop: od_core::Property::Width(16_000.0),
                },
                Operation::Set {
                    id: frame.clone(),
                    prop: od_core::Property::Height(16_000.0),
                },
            ],
        )
        .unwrap();
        assert!(matches!(
            render_frame(s.document(), &frame, 1.0, None),
            Err(RenderError::TooLarge { .. })
        ));
    }

    #[test]
    fn png_without_alpha_is_rgb_and_flattened_onto_white() {
        let mut pixmap = Pixmap::new(2, 1).unwrap();
        pixmap.fill(Color::TRANSPARENT);
        let bytes = encode_png(&pixmap, false).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(&bytes));
        let mut reader = decoder.read_info().unwrap();
        assert_eq!(reader.info().color_type, png::ColorType::Rgb);
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(buf, vec![255; 6]);

        let with_alpha = encode_png(&pixmap, true).unwrap();
        let reader = png::Decoder::new(std::io::Cursor::new(&with_alpha))
            .read_info()
            .unwrap();
        assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    }

    #[test]
    fn a_panorama_is_cut_into_equal_columns_that_meet_at_the_seams() {
        let (mut s, frame) = frame_session();
        add(&mut s, &frame, rect(50.0, 0.0, 50.0, 50.0, "#0000ff"));
        let r = render_frame(s.document(), &frame, 1.0, None).unwrap();
        let slices = slice_columns(&r.pixmap, 2).unwrap();
        assert_eq!(slices.len(), 2);
        assert!(slices.iter().all(|p| (p.width(), p.height()) == (50, 50)));
        let color = |p: &Pixmap| {
            let c = p.pixel(25, 25).unwrap();
            [c.red(), c.green(), c.blue()]
        };
        assert_eq!(color(&slices[0]), [255, 0, 0]);
        assert_eq!(color(&slices[1]), [0, 0, 255]);
        assert!(matches!(
            slice_columns(&r.pixmap, 3),
            Err(RenderError::BadSlices {
                width: 100,
                count: 3
            })
        ));
        assert!(slice_columns(&r.pixmap, 0).is_err());
    }

    /// Strokes, rounded corners, ellipses, nested frames, tall text across band edges and nodes
    /// that hang outside the frame.
    fn busy_frame() -> (Session, NodeId) {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut big = NewNode::new(NodeKind::Frame);
        big.width = 400.0;
        big.height = 900.0;
        big.fill = Some(Paint::parse("#101820").unwrap());
        let frame = add(&mut s, &root, big);
        let mut inner = NewNode::new(NodeKind::Frame);
        inner.x = 20.0;
        inner.y = 300.0;
        inner.width = 300.0;
        inner.height = 400.0;
        inner.corner_radius = Some(30.0);
        inner.fill = Some(Paint::parse("#223344").unwrap());
        let inner = add(&mut s, &frame, inner);
        for i in 0..300_u32 {
            let mut n = NewNode::new(match i % 3 {
                0 => NodeKind::Ellipse,
                _ => NodeKind::Rect,
            });
            n.x = f64::from(i % 17) * 29.0 - 40.0;
            n.y = f64::from(i) * 3.3 - 20.0;
            n.width = 37.0;
            n.height = 23.0;
            if i % 3 != 0 {
                n.corner_radius = Some(6.0);
            }
            n.opacity = Some(0.8);
            n.fill = Some(Paint::parse(["#f2ca83", "#33aa77", "#aa3377"][i as usize % 3]).unwrap());
            if i % 5 == 0 {
                n.stroke = Some(Stroke {
                    paint: Paint::parse("#ffffff").unwrap(),
                    width: 3.0,
                });
            }
            add(&mut s, if i % 4 == 0 { &inner } else { &frame }, n);
        }
        let mut t = NewNode::new(NodeKind::Text);
        t.x = 30.0;
        t.y = 100.0;
        t.width = 120.0;
        t.height = 20.0;
        t.font_size = Some(40.0);
        t.text = Some("a long line of words that wraps over many rows".into());
        t.fill = Some(Paint::parse("#fcf1d9").unwrap());
        add(&mut s, &frame, t);
        (s, frame)
    }

    #[test]
    fn bands_draw_what_one_pass_draws() {
        let (s, frame) = busy_frame();
        for scale in [0.3_f32, 1.0, 1.7] {
            let one = render_frame_in_bands(
                s.document(),
                &frame,
                scale,
                None,
                &mut Media::default(),
                Some(1),
                Camera::default(),
            )
            .unwrap();
            for bands in [2, 3, 8] {
                let many = render_frame_in_bands(
                    s.document(),
                    &frame,
                    scale,
                    None,
                    &mut Media::default(),
                    Some(bands),
                    Camera::default(),
                )
                .unwrap();
                assert_eq!(one.pixmap.width(), many.pixmap.width());
                assert_eq!(one.pixmap.height(), many.pixmap.height());
                // Float rounding of a shifted outline can move a few anti-aliased edge samples.
                let diffs: Vec<u8> = one
                    .pixmap
                    .data()
                    .iter()
                    .zip(many.pixmap.data())
                    .map(|(a, b)| a.abs_diff(*b))
                    .collect();
                let worst = diffs.iter().max().unwrap();
                let off = diffs.iter().filter(|d| **d > 2).count();
                assert!(
                    *worst <= 64 && off * 5000 <= diffs.len(),
                    "scale {scale}, {bands} bands: {off} values differ, by up to {worst}"
                );
            }
        }
    }

    #[test]
    fn a_busy_frame_is_split_and_a_plain_one_is_not() {
        let (s, frame) = busy_frame();
        let node = s.document().node(&frame).unwrap();
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get());
        assert_eq!(band_count(node, 900) > 1, cores > 1);
        assert_eq!(band_count(node, 40), 1, "too short to split");
        let (plain, id) = frame_session();
        assert_eq!(band_count(plain.document().node(&id).unwrap(), 50), 1);
    }

    #[test]
    fn a_frame_without_a_background_is_transparent_and_the_checkerboard_encodes() {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut frame = NewNode::new(NodeKind::Frame);
        (frame.width, frame.height) = (100.0, 100.0);
        let id = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: root,
                    index: None,
                    node: frame,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let rendered = render_frame(s.document(), &id, 1.0, None).unwrap();
        assert!(
            transparent_share(&rendered.pixmap) > 0.99,
            "no fill means fully transparent"
        );

        // with a fill the frame is opaque
        let mut filled = NewNode::new(NodeKind::Frame);
        (filled.width, filled.height) = (100.0, 100.0);
        filled.fill = Paint::parse("#204060").ok();
        let id2 = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: s.document().page(&page).unwrap().root.id.clone(),
                    index: None,
                    node: filled,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let rendered = render_frame(s.document(), &id2, 1.0, None).unwrap();
        assert!(transparent_share(&rendered.pixmap) < 0.01);

        let png = checkerboard_png().expect("the checkerboard encodes");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
    }
}
