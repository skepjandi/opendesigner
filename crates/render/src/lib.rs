//! CPU rasterizer for OpenDesigner documents: frames, groups, rectangles and ellipses to pixels,
//! and PNG export. Text and images are not drawn yet; they are reported as warnings so an export
//! never silently drops content (`docs/wiki/roadmap.md`, Phase 2).

use std::collections::BTreeMap;

use od_core::{Document, Node, NodeId, NodeKind, OpError, limits};
use thiserror::Error;
use tiny_skia::{
    Color, FillRule, Mask, Paint as SkPaint, Path, PathBuilder, Pixmap, Rect, Stroke as SkStroke,
    Transform,
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
}

/// Content the renderer could not draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning {
    TextNotRendered(NodeId),
    ImageNotRendered(NodeId),
}

pub struct Rendered {
    pub pixmap: Pixmap,
    pub warnings: Vec<Warning>,
}

struct Ctx<'a> {
    tokens: &'a BTreeMap<String, String>,
    warnings: Vec<Warning>,
}

/// Renders `frame` at `scale` (1.0 = one pixel per document unit).
pub fn render_frame(doc: &Document, frame: &NodeId, scale: f32) -> Result<Rendered, RenderError> {
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
    let (w, h) = (width as u32, height as u32);
    let mut pixmap = Pixmap::new(w, h).ok_or(RenderError::TooLarge { width, height })?;
    let mut ctx = Ctx {
        tokens: &doc.tokens,
        warnings: Vec::new(),
    };
    let base = Transform::from_scale(scale, scale);
    draw(&mut ctx, &mut pixmap, node, base, 1.0, None, true)?;
    Ok(Rendered {
        pixmap,
        warnings: ctx.warnings,
    })
}

fn draw(
    ctx: &mut Ctx,
    pixmap: &mut Pixmap,
    node: &Node,
    parent: Transform,
    parent_opacity: f32,
    mask: Option<&Mask>,
    is_root: bool,
) -> Result<(), RenderError> {
    if !node.visible {
        return Ok(());
    }
    // The exported frame sits at the origin whatever its position on the page.
    let (x, y) = if is_root {
        (0.0, 0.0)
    } else {
        (node.x as f32, node.y as f32)
    };
    let transform = parent.pre_translate(x, y);
    let opacity = parent_opacity * node.opacity as f32;
    let (w, h) = (node.width as f32, node.height as f32);

    let shape = match node.kind {
        NodeKind::Frame | NodeKind::Rect => {
            rounded_rect(w, h, (node.corner_radius as f32).min(w.min(h) / 2.0))
        }
        NodeKind::Ellipse => Rect::from_xywh(0.0, 0.0, w, h).and_then(PathBuilder::from_oval),
        NodeKind::Group => None,
        NodeKind::Text => {
            ctx.warnings.push(Warning::TextNotRendered(node.id.clone()));
            None
        }
        NodeKind::Image => {
            ctx.warnings
                .push(Warning::ImageNotRendered(node.id.clone()));
            None
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
    // Frames clip their children to their own (rounded) bounds.
    let clip = match (node.kind, &shape) {
        (NodeKind::Frame, Some(path)) => Some(clip_mask(pixmap, mask, path, transform)?),
        _ => None,
    };
    let child_mask = clip.as_ref().or(mask);
    for child in &node.children {
        draw(ctx, pixmap, child, transform, opacity, child_mask, false)?;
    }
    Ok(())
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

/// Encodes a PNG. With `alpha` false the image is flattened onto white and written as RGB, which
/// is what the stores require for screenshots.
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
        let r = render_frame(s.document(), &frame, 1.0).unwrap();
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
        let r = render_frame(s.document(), &frame, 1.0).unwrap();
        let blended = px(&r, 95, 10);
        assert!(blended[0] > 100 && blended[0] < 160, "{blended:?}");
        assert!(blended[2] > 100 && blended[2] < 160, "{blended:?}");
        assert_eq!(r.pixmap.width(), 100);
    }

    #[test]
    fn scale_multiplies_the_output_size_and_geometry() {
        let (mut s, frame) = frame_session();
        add(&mut s, &frame, rect(10.0, 10.0, 20.0, 20.0, "#0000ff"));
        let r = render_frame(s.document(), &frame, 2.0).unwrap();
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
        let r = render_frame(s.document(), &frame, 1.0).unwrap();
        assert_eq!(
            px(&r, 0, 0),
            [255, 0, 0, 255],
            "the corner shows the frame, not the card"
        );
        assert_eq!(px(&r, 50, 25), [0, 255, 0, 255]);
        assert_eq!(px(&r, 50, 0), [0, 0, 0, 255], "top edge is stroked");
    }

    #[test]
    fn text_and_images_are_reported_not_dropped_silently() {
        let (mut s, frame) = frame_session();
        let t = add(&mut s, &frame, NewNode::new(NodeKind::Text));
        let i = add(&mut s, &frame, NewNode::new(NodeKind::Image));
        let r = render_frame(s.document(), &frame, 1.0).unwrap();
        assert_eq!(
            r.warnings,
            vec![Warning::TextNotRendered(t), Warning::ImageNotRendered(i)]
        );
    }

    #[test]
    fn refuses_what_it_cannot_render() {
        let (mut s, frame) = frame_session();
        let rect_id = add(&mut s, &frame, rect(0.0, 0.0, 1.0, 1.0, "#000000"));
        assert!(matches!(
            render_frame(s.document(), &rect_id, 1.0),
            Err(RenderError::NotAFrame { .. })
        ));
        assert!(matches!(
            render_frame(s.document(), &NodeId::parse("n_999").unwrap(), 1.0),
            Err(RenderError::NoSuchNode(_))
        ));
        assert!(matches!(
            render_frame(s.document(), &frame, 0.0),
            Err(RenderError::BadScale)
        ));
        assert!(matches!(
            render_frame(s.document(), &frame, f32::NAN),
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
            render_frame(s.document(), &frame, 1.0),
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
}
