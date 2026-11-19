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

/// Draws `node`'s text with its top-left corner at the origin of `transform`.
pub(crate) fn draw_text(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    node: &Node,
    transform: Transform,
    color: Rgba,
    opacity: f32,
) {
    let Some(text) = node.text.as_deref().filter(|t| !t.is_empty()) else {
        return;
    };
    let scale = transform.sx;
    let size = node.font_size.unwrap_or(16.0) as f32 * scale;
    let line = size * node.line_height.unwrap_or(DEFAULT_LINE_HEIGHT) as f32;
    if !(size >= 0.5 && line > 0.0) {
        return;
    }
    let family = node.font_family.as_deref().unwrap_or(DEFAULT_FAMILY);
    let weight = node.font_weight.unwrap_or(400.0) as u16;
    let align = match node.text_align {
        None | Some(TextAlign::Left) => Align::Left,
        Some(TextAlign::Center) => Align::Center,
        Some(TextAlign::Right) => Align::Right,
    };
    let wrap_width = (node.width > 0.0).then_some(node.width as f32 * scale);
    let (origin_x, origin_y) = (transform.tx.round() as i32, transform.ty.round() as i32);

    let mut guard = FONTS.lock().unwrap_or_else(PoisonError::into_inner);
    let fonts = &mut *guard;
    if !fonts.has_family(family) || text.chars().any(|c| c as u32 > 0x24F) {
        fonts.ensure_system_fonts();
    }
    let key = LayoutKey {
        text: text.to_string(),
        size: size.to_bits(),
        line: line.to_bits(),
        family: family.to_string(),
        weight,
        align: align as u8,
        wrap_width: wrap_width.map(f32::to_bits),
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
        let mut buffer = Buffer::new(system, Metrics::new(size, line));
        buffer.set_size(wrap_width, None);
        let attrs = Attrs::new()
            .family(Family::Name(family))
            .weight(Weight(weight));
        buffer.set_text(text, &attrs, Shaping::Advanced, Some(align));
        buffer
    });

    let (width, height) = (pixmap.width() as i32, pixmap.height() as i32);
    let mask_width = mask.map_or(0, |m| m.width() as i32);
    let alpha = f32::from(color.a) / 255.0 * opacity;
    let data = pixmap.data_mut();
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
