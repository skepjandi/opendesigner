//! Nodes that blur or cast a shadow are drawn on a layer of their own, the size of what they
//! paint, then blurred, shadowed and laid over the picture. Nodes with neither never come here,
//! so a frame that uses no effect pays nothing for them.

use od_core::{Node, NodeKind, Rgba, Shadow};
use tiny_skia::{Path, Pixmap, PixmapPaint, Transform};

use crate::place;
use crate::{Ctx, Place, RenderError, View, draw_node, grade, rounded_rect, text};

/// Standard deviations a blur reaches out to.
const REACH_SIGMAS: f32 = 3.0;

pub(crate) fn needs_layer(node: &Node) -> bool {
    node.blur > 0.0 || node.shadow.is_some()
}

/// The path a clipping group clips its children to: its box, rounded by its corner radius.
pub(crate) fn group_clip(node: &Node) -> Option<Path> {
    let b = node
        .layout_box()
        .filter(|b| b.width > 0.0 && b.height > 0.0)?;
    let (w, h) = (b.width as f32, b.height as f32);
    rounded_rect(w, h, (node.corner_radius as f32).min(w.min(h) / 2.0))?
        .transform(Transform::from_translate(b.x as f32, b.y as f32))
}

/// A rectangle of pixels: left, top, right, bottom.
#[derive(Clone, Copy)]
struct Area(f32, f32, f32, f32);

impl Area {
    fn union(self, o: Area) -> Area {
        Area(
            self.0.min(o.0),
            self.1.min(o.1),
            self.2.max(o.2),
            self.3.max(o.3),
        )
    }

    fn grow(self, by: f32) -> Area {
        Area(self.0 - by, self.1 - by, self.2 + by, self.3 + by)
    }

    fn within(self, o: Area) -> Area {
        Area(
            self.0.max(o.0),
            self.1.max(o.1),
            self.2.min(o.2),
            self.3.min(o.3),
        )
    }
}

/// How far a node's own blur and shadow reach past what it paints, in pixels, when a document
/// unit spans `ppu` of them.
fn reach(node: &Node, ppu: f32) -> f32 {
    let blur = node.blur as f32 * ppu * REACH_SIGMAS;
    let shadow = node.shadow.as_ref().map_or(0.0, |s| {
        s.dx.abs().max(s.dy.abs()) as f32 * ppu + s.blur as f32 * ppu * REACH_SIGMAS
    });
    blur + shadow
}

/// Pixels per document unit a node `depth` deep is drawn at (the frame's scale, magnified by the
/// camera).
fn pixels_per_unit(ctx: &Ctx, depth: f32) -> f32 {
    ctx.scale * ctx.camera.as_ref().map_or(1.0, |c| c.zoom(depth))
}

/// What `node` and everything below it can paint, in the pixels of the pixmap being drawn.
fn paint_area(ctx: &Ctx, node: &Node, place: Place) -> Option<Area> {
    if !node.visible {
        return None;
    }
    let chain = place::node_chain(place.parent, node);
    let depth = place.depth * node.depth as f32;
    let world = ctx.world(chain, place.shift, depth, true);
    let clips = node.kind == NodeKind::Frame || (node.kind == NodeKind::Group && node.clip);
    let own = own_area(node, world);
    let inner = Place {
        parent: chain,
        depth,
        ..place
    };
    let below = (!clips)
        .then(|| {
            node.children
                .iter()
                .filter_map(|c| paint_area(ctx, c, inner))
                .reduce(Area::union)
        })
        .flatten();
    let both = [own, below].into_iter().flatten().reduce(Area::union)?;
    Some(both.grow(reach(node, pixels_per_unit(ctx, depth))))
}

/// The pixels the node paints itself: its box (a group has none unless it clips), with the
/// stroke's half width, and for text the room its lines take.
fn own_area(node: &Node, world: Transform) -> Option<Area> {
    if node.kind == NodeKind::Group && !node.clip {
        return None;
    }
    let b = node.layout_box()?;
    let (mut w, mut h) = (b.width as f32, b.height as f32);
    let mut pad = node.stroke.as_ref().map_or(0.0, |s| s.width as f32 / 2.0) + 1.0;
    if node.kind == NodeKind::Text
        && let Some((tw, th)) = text::measure(node)
    {
        (w, h) = (w.max(tw), h.max(th));
        pad += node.font_size.unwrap_or(16.0) as f32;
    }
    let (l, t, r, bt) = place::device_rect(
        world,
        b.x as f32 - pad,
        b.y as f32 - pad,
        w + 2.0 * pad,
        h + 2.0 * pad,
    );
    Some(Area(l, t, r, bt))
}

/// Draws `node` on a layer, blurs it, adds its shadow and lays it over `pixmap`.
pub(crate) fn draw_layered(
    ctx: &mut Ctx,
    pixmap: &mut Pixmap,
    node: &Node,
    place: Place,
    mask: Option<&tiny_skia::Mask>,
) -> Result<(), RenderError> {
    let depth = place.depth * node.depth as f32;
    let ppu = pixels_per_unit(ctx, depth);
    let Some(area) = paint_area(ctx, node, place) else {
        return Ok(());
    };
    let view = Area(0.0, 0.0, ctx.view.width, ctx.view.height).grow(reach(node, ppu));
    let Area(l, t, r, b) = area.within(view);
    let (left, top) = (l.floor() as i32, t.floor() as i32);
    let (width, height) = (
        (r.ceil() as i32 - left).max(0) as u32,
        (b.ceil() as i32 - top).max(0) as u32,
    );
    if width == 0 || height == 0 {
        return Ok(());
    }
    let too_large = || RenderError::TooLarge {
        width: u64::from(width),
        height: u64::from(height),
    };
    if u64::from(width) * u64::from(height) > crate::MAX_PIXELS {
        return Err(too_large());
    }
    let mut layer = Pixmap::new(width, height).ok_or_else(too_large)?;
    let inner = Place {
        shift: Transform::from_translate(-(left as f32), -(top as f32)).pre_concat(place.shift),
        ..place
    };
    let outer_view = ctx.view;
    ctx.view = View {
        width: width as f32,
        height: height as f32,
    };
    let drawn = draw_node(ctx, &mut layer, node, inner, None, false, false);
    ctx.view = outer_view;
    drawn?;
    grade::blur(&mut layer, node.blur as f32 * ppu);
    if let Some(shadow) = &node.shadow {
        layer = with_shadow(layer, shadow, ctx, ppu)?;
    }
    pixmap.draw_pixmap(
        left,
        top,
        layer.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        mask,
    );
    Ok(())
}

/// `layer` with `shadow` under it: its silhouette tinted, shifted by the shadow's offset and
/// blurred. The silhouette already carries the node's opacity. The offset is rounded to whole
/// pixels, so a still shadow is the same every frame.
fn with_shadow(layer: Pixmap, shadow: &Shadow, ctx: &Ctx, ppu: f32) -> Result<Pixmap, RenderError> {
    let color = shadow.color.resolve(ctx.tokens)?;
    let (w, h) = (layer.width() as i32, layer.height() as i32);
    let mut under = Pixmap::new(layer.width(), layer.height()).ok_or(RenderError::TooLarge {
        width: u64::from(layer.width()),
        height: u64::from(layer.height()),
    })?;
    let (dx, dy) = (
        (shadow.dx as f32 * ppu).round() as i32,
        (shadow.dy as f32 * ppu).round() as i32,
    );
    let strength = f32::from(color.a) / 255.0;
    let src = layer.data();
    let dst = under.data_mut();
    for y in (dy.max(0)..(h + dy.min(0))).filter(|y| (0..h).contains(y)) {
        for x in dx.max(0)..(w + dx.min(0)) {
            let from = (((y - dy) * w + (x - dx)) * 4 + 3) as usize;
            let alpha = f32::from(src[from]) * strength;
            if alpha > 0.0 {
                let at = ((y * w + x) * 4) as usize;
                write_tint(&mut dst[at..at + 4], color, alpha);
            }
        }
    }
    grade::blur(&mut under, shadow.blur as f32 * ppu);
    under.draw_pixmap(
        0,
        0,
        layer.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    Ok(under)
}

/// One premultiplied pixel of `color` at `alpha` (0 to 255).
fn write_tint(px: &mut [u8], color: Rgba, alpha: f32) {
    let k = alpha / 255.0;
    px[0] = (f32::from(color.r) * k).round() as u8;
    px[1] = (f32::from(color.g) * k).round() as u8;
    px[2] = (f32::from(color.b) * k).round() as u8;
    px[3] = alpha.round().min(255.0) as u8;
}
