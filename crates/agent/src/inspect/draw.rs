//! Pixel work for the inspection tools: number labels, a coordinate grid, crops and the picture
//! of what changed between two pictures. Frames are opaque, so the premultiplied bytes of a
//! [`Pixmap`] are plain RGB with full alpha and can be blended directly.

use od_render::Pixmap;

/// Glyphs of a 3 x 5 pixel font, one row per entry, the leftmost pixel in bit 2.
const GLYPHS: [(char, [u8; 5]); 12] = [
    ('0', [7, 5, 5, 5, 7]),
    ('1', [2, 6, 2, 2, 7]),
    ('2', [7, 1, 7, 4, 7]),
    ('3', [7, 1, 7, 1, 7]),
    ('4', [5, 5, 7, 1, 1]),
    ('5', [7, 4, 7, 1, 7]),
    ('6', [7, 4, 7, 5, 7]),
    ('7', [7, 1, 1, 2, 2]),
    ('8', [7, 5, 7, 5, 7]),
    ('9', [7, 5, 7, 1, 7]),
    ('.', [0, 0, 0, 0, 2]),
    ('-', [0, 0, 7, 0, 0]),
];

const GLYPH_W: i64 = 3;
const GLYPH_H: i64 = 5;
const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
/// Changed pixels are painted in this color.
const CHANGED: [u8; 3] = [255, 0, 170];

/// A rectangle of whole pixels: left, top, width, height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

fn blend(pix: &mut Pixmap, x: i64, y: i64, rgb: [u8; 3], alpha: f32) {
    let (w, h) = (i64::from(pix.width()), i64::from(pix.height()));
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    let at = ((y * w + x) * 4) as usize;
    let data = pix.data_mut();
    for (c, v) in rgb.iter().enumerate() {
        let old = f32::from(data[at + c]);
        data[at + c] = (old + (f32::from(*v) - old) * alpha).round() as u8;
    }
    data[at + 3] = 255;
}

/// Blends `rgb` at `alpha` over the rectangle; the parts outside the picture are skipped.
pub fn fill_rect(
    pix: &mut Pixmap,
    (x, y): (i64, i64),
    (w, h): (i64, i64),
    rgb: [u8; 3],
    alpha: f32,
) {
    for py in y.max(0)..(y + h).min(i64::from(pix.height())) {
        for px in x.max(0)..(x + w).min(i64::from(pix.width())) {
            blend(pix, px, py, rgb, alpha);
        }
    }
}

fn glyph(c: char) -> Option<[u8; 5]> {
    GLYPHS.iter().find(|(g, _)| *g == c).map(|(_, rows)| *rows)
}

/// How wide `text` is at `scale`: one blank column between glyphs.
fn text_width(text: &str, scale: i64) -> i64 {
    let n = text.chars().count() as i64;
    (n * (GLYPH_W + 1) - 1).max(0) * scale
}

/// Writes `text` (digits, `.` and `-`) with its top-left corner at `(x, y)`, white on a dark box,
/// so it reads over any picture.
pub fn label(pix: &mut Pixmap, text: &str, (x, y): (i64, i64), scale: i64) {
    let pad = scale.max(1);
    fill_rect(
        pix,
        (x - pad, y - pad),
        (text_width(text, scale) + 2 * pad, GLYPH_H * scale + 2 * pad),
        BLACK,
        0.72,
    );
    for (n, c) in text.chars().enumerate() {
        let Some(rows) = glyph(c) else { continue };
        let left = x + n as i64 * (GLYPH_W + 1) * scale;
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..GLYPH_W {
                if bits >> (GLYPH_W - 1 - col) & 1 == 1 {
                    let at = (left + col * scale, y + row as i64 * scale);
                    fill_rect(pix, at, (scale, scale), WHITE, 1.0);
                }
            }
        }
    }
}

/// A label size that stays readable: 2 for small pictures, growing with the picture.
pub fn label_scale(pix: &Pixmap) -> i64 {
    (i64::from(pix.width().max(pix.height())) / 400).clamp(2, 5)
}

/// Draws lines every `spacing` source pixels over a picture that shows a source `source_w` x
/// `source_h` pixels wide, and numbers each line (in source pixels) along the top and left edges.
pub fn coordinate_grid(pix: &mut Pixmap, (source_w, source_h): (u32, u32), spacing: u32) {
    let scale_x = f64::from(pix.width()) / f64::from(source_w.max(1));
    let scale_y = f64::from(pix.height()) / f64::from(source_h.max(1));
    let text = label_scale(pix);
    let (w, h) = (i64::from(pix.width()), i64::from(pix.height()));
    for line in (0..=source_w).step_by(spacing.max(1) as usize) {
        let x = ((f64::from(line) * scale_x).round() as i64).min(w - 1);
        fill_rect(pix, (x, 0), (1, h), WHITE, 0.55);
        fill_rect(pix, (x + 1, 0), (1, h), BLACK, 0.3);
        label(pix, &line.to_string(), (x + 3 * text, 3 * text), text);
    }
    for line in (0..=source_h).step_by(spacing.max(1) as usize) {
        let y = ((f64::from(line) * scale_y).round() as i64).min(h - 1);
        fill_rect(pix, (0, y), (w, 1), WHITE, 0.55);
        fill_rect(pix, (0, y + 1), (w, 1), BLACK, 0.3);
        if line > 0 {
            label(pix, &line.to_string(), (3 * text, y + 3 * text), text);
        }
    }
}

/// The rectangle of `pix` cut out, or `None` when it is empty or leaves the picture.
pub fn crop(pix: &Pixmap, r: PixelRect) -> Option<Pixmap> {
    if r.width == 0
        || r.height == 0
        || r.x.checked_add(r.width)? > pix.width()
        || r.y.checked_add(r.height)? > pix.height()
    {
        return None;
    }
    let mut out = Pixmap::new(r.width, r.height)?;
    let stride = pix.width() as usize * 4;
    let row = r.width as usize * 4;
    for y in 0..r.height as usize {
        let from = (r.y as usize + y) * stride + r.x as usize * 4;
        out.data_mut()[y * row..(y + 1) * row].copy_from_slice(&pix.data()[from..from + row]);
    }
    Some(out)
}

/// Every pixel made `factor` times as large, without smoothing, so single pixels stay visible.
pub fn enlarge(pix: &Pixmap, factor: u32) -> Option<Pixmap> {
    if factor <= 1 {
        return Some(pix.clone());
    }
    let (w, h) = (pix.width() as usize, pix.height() as usize);
    let f = factor as usize;
    let mut out = Pixmap::new(pix.width() * factor, pix.height() * factor)?;
    let out_stride = w * f * 4;
    for y in 0..h * f {
        for x in 0..w * f {
            let from = ((y / f) * w + x / f) * 4;
            let to = y * out_stride + x * 4;
            out.data_mut()[to..to + 4].copy_from_slice(&pix.data()[from..from + 4]);
        }
    }
    Some(out)
}

/// What differs between two pictures of one size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Changes {
    pub changed: usize,
    pub total: usize,
    pub bounds: Option<PixelRect>,
}

impl Changes {
    pub fn share(&self) -> f64 {
        self.changed as f64 / self.total.max(1) as f64
    }
}

/// `a` darkened and grayed, with every pixel that differs from `b` by more than `threshold` in
/// any channel painted in a loud color; and what changed. `None` when the sizes differ.
pub fn highlight(a: &Pixmap, b: &Pixmap, threshold: u8) -> Option<(Pixmap, Changes)> {
    if (a.width(), a.height()) != (b.width(), b.height()) {
        return None;
    }
    let mut out = a.clone();
    let width = a.width() as usize;
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    let mut changed = 0;
    for (i, (pa, pb)) in a.data().chunks(4).zip(b.data().chunks(4)).enumerate() {
        let differs = pa[..3]
            .iter()
            .zip(&pb[..3])
            .any(|(p, q)| p.abs_diff(*q) > threshold);
        let target = &mut out.data_mut()[i * 4..i * 4 + 4];
        if differs {
            changed += 1;
            let (x, y) = (i % width, i / width);
            (left, top) = (left.min(x), top.min(y));
            (right, bottom) = (right.max(x), bottom.max(y));
            target[..3].copy_from_slice(&CHANGED);
        } else {
            let gray = (u16::from(pa[0]) + u16::from(pa[1]) + u16::from(pa[2])) / 3;
            target[..3].fill((gray * 2 / 5 + 20) as u8);
        }
        target[3] = 255;
    }
    let bounds = (changed > 0).then(|| PixelRect {
        x: left as u32,
        y: top as u32,
        width: (right - left + 1) as u32,
        height: (bottom - top + 1) as u32,
    });
    let total = width * a.height() as usize;
    Some((
        out,
        Changes {
            changed,
            total,
            bounds,
        },
    ))
}
