//! Text: shaping and layout by `cosmic-text`, glyph coverage blended into the pixmap.
//!
//! The bundled Lato (SIL OFL, `fonts/OFL.txt`) is the default and the fallback, so a document
//! renders the same on every machine. Other families are looked up in the system fonts, which are
//! loaded the first time a document asks for a family the bundle does not have.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use cosmic_text::{
    Align, Attrs, Buffer, Color as CtColor, Family, FontSystem, Metrics, Shaping, SwashCache,
    Weight, fontdb,
};
use od_core::{Node, Rgba, TextAlign};
use tiny_skia::{Mask, Pixmap, Transform};

const LATO_REGULAR: &[u8] = include_bytes!("../fonts/Lato-Regular.ttf");
const LATO_BOLD: &[u8] = include_bytes!("../fonts/Lato-Bold.ttf");
pub const DEFAULT_FAMILY: &str = "Lato";
const DEFAULT_LINE_HEIGHT: f64 = 1.2;

/// What decides a text node's layout. Colour, opacity and position do not.
#[derive(PartialEq, Eq, Hash)]
struct LayoutKey {
    text: String,
    size: u32,
    line: u32,
    family: String,
    weight: u16,
    align: u8,
    wrap_width: Option<u32>,
}

/// Laid-out text kept between renders: shaping costs far more than drawing the glyphs, and an
/// edit renders the whole frame again with nearly all of its text unchanged.
const MAX_LAYOUTS: usize = 4096;

struct Fonts {
    system: FontSystem,
    cache: SwashCache,
    layouts: HashMap<LayoutKey, Buffer>,
    system_fonts_loaded: bool,
}

impl Fonts {
    fn new() -> Fonts {
        let mut db = fontdb::Database::new();
        db.load_font_data(LATO_REGULAR.to_vec());
        db.load_font_data(LATO_BOLD.to_vec());
        for set in [
            fontdb::Database::set_sans_serif_family,
            fontdb::Database::set_serif_family,
            fontdb::Database::set_monospace_family,
            fontdb::Database::set_cursive_family,
            fontdb::Database::set_fantasy_family,
        ] {
            set(&mut db, DEFAULT_FAMILY);
        }
        Fonts {
            system: FontSystem::new_with_locale_and_db("en-US".to_string(), db),
            cache: SwashCache::new(),
            layouts: HashMap::new(),
            system_fonts_loaded: false,
        }
    }

    fn has_family(&self, family: &str) -> bool {
        self.system.db().faces().any(|face| {
            face.families
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(family))
        })
    }

    /// Loads the machine's fonts once, when a document needs a family or a script the bundle lacks.
    fn ensure_system_fonts(&mut self) {
        if !self.system_fonts_loaded {
            self.system.db_mut().load_system_fonts();
            self.layouts.clear();
            self.system_fonts_loaded = true;
        }
    }
}

static FONTS: LazyLock<Mutex<Fonts>> = LazyLock::new(|| Mutex::new(Fonts::new()));

/// How a text node is set, at one scale.
struct Style<'a> {
    text: &'a str,
    size: f32,
    line: f32,
    family: &'a str,
    weight: u16,
    align: Align,
    wrap_width: Option<f32>,
}

/// The style of `node`'s text at `scale`, or `None` when there is nothing to draw.
fn style_of(node: &Node, scale: f32) -> Option<Style<'_>> {
    let text = node.text.as_deref().filter(|t| !t.is_empty())?;
    style_with(node, text, scale)
}

/// The style of `text` set in `node`'s way at `scale`, or `None` when there is nothing to draw.
fn style_with<'a>(node: &'a Node, text: &'a str, scale: f32) -> Option<Style<'a>> {
    if text.is_empty() {
        return None;
    }
    let size = node.font_size.unwrap_or(16.0) as f32 * scale;
    let line = size * node.line_height.unwrap_or(DEFAULT_LINE_HEIGHT) as f32;
    if !(size >= 0.5 && line > 0.0) {
        return None;
    }
    Some(Style {
        text,
        size,
        line,
        family: node.font_family.as_deref().unwrap_or(DEFAULT_FAMILY),
        weight: node.font_weight.unwrap_or(400.0) as u16,
        align: match node.text_align {
            None | Some(TextAlign::Left) => Align::Left,
            Some(TextAlign::Center) => Align::Center,
            Some(TextAlign::Right) => Align::Right,
        },
        wrap_width: (node.width > 0.0).then_some(node.width as f32 * scale),
    })
}

/// Runs `then` on the laid-out text for `style`, laying it out first unless it is cached.
fn with_layout<R>(
    style: &Style<'_>,
    then: impl FnOnce(&mut Buffer, &mut FontSystem, &mut SwashCache) -> R,
) -> R {
    let mut guard = FONTS.lock().unwrap_or_else(PoisonError::into_inner);
    let fonts = &mut *guard;
    if !fonts.has_family(style.family) || style.text.chars().any(|c| c as u32 > 0x24F) {
        fonts.ensure_system_fonts();
    }
    let key = LayoutKey {
        text: style.text.to_string(),
        size: style.size.to_bits(),
        line: style.line.to_bits(),
        family: style.family.to_string(),
        weight: style.weight,
        align: style.align as u8,
        wrap_width: style.wrap_width.map(f32::to_bits),
    };
    if fonts.layouts.len() >= MAX_LAYOUTS && !fonts.layouts.contains_key(&key) {
        fonts.layouts.clear();
    }
    let Fonts {
        system,
        cache,
        layouts,
        ..
    } = fonts;
    let buffer = layouts.entry(key).or_insert_with(|| {
        let mut buffer = Buffer::new(system, Metrics::new(style.size, style.line));
        buffer.set_size(style.wrap_width, None);
        let attrs = Attrs::new()
            .family(Family::Name(style.family))
            .weight(Weight(style.weight));
        buffer.set_text(style.text, &attrs, Shaping::Advanced, Some(style.align));
        buffer
    });
    then(buffer, system, cache)
}

/// The size of `node`'s text laid out at scale 1: the widest line and the height of all lines, in
/// document units. `None` when the node has no text to draw.
pub(crate) fn measure(node: &Node) -> Option<(f32, f32)> {
    let style = style_of(node, 1.0)?;
    Some(with_layout(&style, |buffer, system, _| {
        buffer.shape_until_scroll(system, false);
        buffer
            .layout_runs()
            .fold((0.0_f32, 0.0_f32), |(w, h), run| {
                (w.max(run.line_w), h.max(run.line_top + run.line_height))
            })
    }))
}

/// One laid-out glyph (a cluster: a ligature or a letter with its marks is one) at scale 1.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedGlyph {
    /// Byte range of the cluster in [`PlacedLine::text`].
    pub start: usize,
    pub end: usize,
    /// Where the renderer draws the glyph: its left edge from the left of the text box, with the
    /// sub-pixel offset the rasteriser works in already added.
    pub x: f32,
    /// Its top from the baseline, with the sub-pixel offset the rasteriser takes off already
    /// added.
    pub y: f32,
    /// The advance the glyph takes.
    pub width: f32,
}

/// One row of laid-out text, as the renderer wraps and draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedLine {
    /// The paragraph (text between line breaks) this row belongs to.
    pub paragraph: usize,
    /// That whole paragraph; glyph ranges index into it.
    pub text: String,
    /// Top of the row, and its baseline, from the top of the text box, in document units.
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    pub width: f32,
    pub rtl: bool,
    pub glyphs: Vec<PlacedGlyph>,
}

/// The rows of `node`'s text at scale 1 with every glyph's position, from the same layout the
/// renderer draws. `None` when the node has no text to draw.
pub(crate) fn layout_lines(node: &Node) -> Option<Vec<PlacedLine>> {
    let style = style_of(node, 1.0)?;
    Some(with_layout(&style, |buffer, system, _| {
        buffer.shape_until_scroll(system, false);
        buffer
            .layout_runs()
            .map(|run| PlacedLine {
                paragraph: run.line_i,
                text: run.text.to_string(),
                top: run.line_top,
                baseline: run.line_y,
                height: run.line_height,
                width: run.line_w,
                rtl: run.rtl,
                glyphs: run.glyphs.iter().map(placed_glyph).collect(),
            })
            .collect()
    }))
}

/// One run of text laid out on its own, as the renderer draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedRun {
    pub glyphs: Vec<PlacedGlyph>,
    /// Top of the row, its baseline and its height, from the top of the text box.
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    pub width: f32,
}

/// The first row of `text` laid out on its own in the style of `node`: `aligned` keeps the
/// node's own alignment (as when a line is cut out of the text) and otherwise the text is set
/// flush left (as when a word or a character is cut out).
pub(crate) fn layout_run<'a>(node: &'a Node, text: &'a str, aligned: bool) -> Option<PlacedRun> {
    let mut style = style_with(node, text, 1.0)?;
    if !aligned {
        style.align = Align::Left;
    }
    with_layout(&style, |buffer, system, _| {
        buffer.shape_until_scroll(system, false);
        let run = buffer.layout_runs().next()?;
        Some(PlacedRun {
            glyphs: run.glyphs.iter().map(placed_glyph).collect(),
            top: run.line_top,
            baseline: run.line_y,
            height: run.line_height,
            width: run.line_w,
        })
    })
}

/// Where the renderer draws one glyph: the layout's position with the rasteriser's sub-pixel
/// offsets already in it.
fn placed_glyph(g: &cosmic_text::LayoutGlyph) -> PlacedGlyph {
    PlacedGlyph {
        start: g.start,
        end: g.end,
        x: g.x + g.font_size * g.x_offset,
        y: g.y - g.font_size * g.y_offset,
        width: g.w,
    }
}

/// Draws `node`'s text under `transform`. Where the map only scales and translates the glyphs
/// go straight onto the pixmap; where it turns, flips or stretches unevenly the text is drawn
/// upright on a scratch picture and that picture is placed.
pub(crate) fn draw_placed(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    node: &Node,
    transform: Transform,
    color: Rgba,
    opacity: f32,
) {
    if transform.kx == 0.0
        && transform.ky == 0.0
        && transform.sx > 0.0
        && transform.sx == transform.sy
    {
        return draw_text(pixmap, mask, node, transform, color, opacity);
    }
    let Some((text_w, text_h)) = measure(node) else {
        return;
    };
    let pad = (node.font_size.unwrap_or(16.0) as f32 * 0.5).ceil() + 2.0;
    let (box_w, box_h) = (
        (node.width as f32).max(text_w) + 2.0 * pad,
        (node.height as f32).max(text_h) + 2.0 * pad,
    );
    let wanted = crate::place::scale_of(transform).max(0.01);
    let s = wanted.min(MAX_SCRATCH / box_w.max(box_h));
    let (w, h) = ((box_w * s).ceil() as u32, (box_h * s).ceil() as u32);
    let Some(mut scratch) = Pixmap::new(w.max(1), h.max(1)) else {
        return;
    };
    let upright = Transform::from_row(s, 0.0, 0.0, s, pad * s, pad * s);
    draw_text(&mut scratch, None, node, upright, color, 1.0);
    let paint = tiny_skia::PixmapPaint {
        opacity,
        quality: tiny_skia::FilterQuality::Bilinear,
        ..tiny_skia::PixmapPaint::default()
    };
    let placed = transform
        .pre_scale(1.0 / s, 1.0 / s)
        .pre_translate(-pad * s, -pad * s);
    pixmap.draw_pixmap(0, 0, scratch.as_ref(), &paint, placed, mask);
}

/// The longest side, in pixels, of the scratch picture turned text is drawn on.
const MAX_SCRATCH: f32 = 4096.0;

/// Draws `node`'s text with its top-left corner at the origin of `transform`.
pub(crate) fn draw_text(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    node: &Node,
    transform: Transform,
    color: Rgba,
    opacity: f32,
) {
    let Some(style) = style_of(node, transform.sx) else {
        return;
    };
    let (origin_x, origin_y) = (transform.tx.round() as i32, transform.ty.round() as i32);
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let mask_width = mask.map_or(0, |m| m.width() as i32);
    let alpha = f32::from(color.a) / 255.0 * opacity;
    let data = pixmap.data_mut();
    with_layout(&style, |buffer, system, cache| {
        buffer.draw(
            system,
            cache,
            CtColor::rgb(color.r, color.g, color.b),
            |gx, gy, gw, gh, c| {
                let coverage = f32::from(c.a()) / 255.0 * alpha;
                if coverage <= 0.0 {
                    return;
                }
                for dy in 0..gh as i32 {
                    for dx in 0..gw as i32 {
                        let (x, y) = (origin_x + gx + dx, origin_y + gy + dy);
                        if x < 0 || y < 0 || x >= width || y >= height {
                            continue;
                        }
                        let clip = mask.map_or(1.0, |m| {
                            f32::from(m.data()[(y * mask_width + x) as usize]) / 255.0
                        });
                        blend(data, ((y * width + x) * 4) as usize, c, coverage * clip);
                    }
                }
            },
        );
    });
}

/// Source-over onto premultiplied RGBA8.
fn blend(data: &mut [u8], i: usize, c: CtColor, a: f32) {
    let Some(px) = data.get_mut(i..i + 4) else {
        return;
    };
    let keep = 1.0 - a;
    for (dst, src) in px.iter_mut().zip([c.r(), c.g(), c.b()]) {
        *dst = (f32::from(src) * a + f32::from(*dst) * keep)
            .round()
            .min(255.0) as u8;
    }
    px[3] = (255.0 * a + f32::from(px[3]) * keep).round().min(255.0) as u8;
}

/// How much room one run of text takes at `size` pixels: its width and its line height.
pub(crate) fn measure_run(text: &str, family: &str, weight: u16, size: f32) -> (f32, f32) {
    let style = run_style(text, family, weight, size);
    with_layout(&style, |buffer, system, _| {
        buffer.shape_until_scroll(system, false);
        buffer
            .layout_runs()
            .fold((0.0_f32, style.line), |(w, h), run| {
                (w.max(run.line_w), h.max(run.line_height))
            })
    })
}

/// Draws one run of text with its top-left corner at `(x, y)`.
pub(crate) fn draw_run(
    pixmap: &mut Pixmap,
    text: &str,
    (family, weight, size): (&str, u16, f32),
    (x, y): (f32, f32),
    color: Rgba,
) {
    let style = run_style(text, family, weight, size);
    let (origin_x, origin_y) = (x.round() as i32, y.round() as i32);
    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let alpha = f32::from(color.a) / 255.0;
    let data = pixmap.data_mut();
    with_layout(&style, |buffer, system, cache| {
        buffer.draw(
            system,
            cache,
            CtColor::rgb(color.r, color.g, color.b),
            |gx, gy, gw, gh, c| {
                let coverage = f32::from(c.a()) / 255.0 * alpha;
                for dy in 0..gh as i32 {
                    for dx in 0..gw as i32 {
                        let (px, py) = (origin_x + gx + dx, origin_y + gy + dy);
                        if px >= 0 && py >= 0 && px < width && py < height {
                            blend(data, ((py * width + px) * 4) as usize, c, coverage);
                        }
                    }
                }
            },
        );
    });
}

fn run_style<'a>(text: &'a str, family: &'a str, weight: u16, size: f32) -> Style<'a> {
    Style {
        text,
        size,
        line: size * DEFAULT_LINE_HEIGHT as f32,
        family,
        weight,
        align: Align::Left,
        wrap_width: None,
    }
}
