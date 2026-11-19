//! Images: decoding files from the project's `media/` folder and drawing them into a node's box.
//!
//! Decoded pixmaps are cached by path, size and modification time, because the editor renders the
//! same frame after every edit.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::SystemTime;

use image::{ImageReader, Limits};
use od_core::{ImageFit, Node};
use tiny_skia::{
    FillRule, FilterQuality, Mask, Paint as SkPaint, Pattern, Pixmap, Rect, SpreadMode, Transform,
};

/// Largest image accepted, in pixels per side and in total.
const MAX_SIDE: u32 = 16_384;
const MAX_PIXELS: u64 = 100 * 1024 * 1024;
/// Decoded pixels kept between renders.
const CACHE_BYTES: usize = 256 * 1024 * 1024;

#[derive(PartialEq, Eq, Clone, Copy)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Default)]
struct Cache {
    entries: HashMap<PathBuf, (Stamp, Arc<Pixmap>)>,
    order: VecDeque<PathBuf>,
    bytes: usize,
}

static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Mutex::default);

impl Cache {
    fn remove(&mut self, path: &Path) {
        if let Some((_, old)) = self.entries.remove(path) {
            self.bytes -= old.data().len();
            self.order.retain(|p| p != path);
        }
    }

    fn insert(&mut self, path: PathBuf, stamp: Stamp, image: Arc<Pixmap>) {
        self.remove(&path);
        self.bytes += image.data().len();
        self.order.push_back(path.clone());
        self.entries.insert(path, (stamp, image));
        while self.bytes > CACHE_BYTES && self.order.len() > 1 {
            if let Some(oldest) = self.order.front().cloned() {
                self.remove(&oldest);
            }
        }
    }
}

/// Decodes `path` (cached) into a premultiplied pixmap.
pub(crate) fn load(path: &Path) -> Result<Arc<Pixmap>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let stamp = Stamp {
        len: meta.len(),
        modified: meta.modified().ok(),
    };
    let cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((cached, image)) = cache.entries.get(path) {
        if *cached == stamp {
            return Ok(Arc::clone(image));
        }
    }
    drop(cache);
    let image = Arc::new(decode(path)?);
    let mut cache = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    cache.insert(path.to_path_buf(), stamp, Arc::clone(&image));
    Ok(image)
}

fn decode(path: &Path) -> Result<Pixmap, String> {
    let mut reader = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(CACHE_BYTES as u64 * 2);
    reader.limits(limits);
    let rgba = reader.decode().map_err(|e| e.to_string())?.into_rgba8();
    let (w, h) = rgba.dimensions();
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(format!("{w}x{h} is too many pixels"));
    }
    let mut pixmap = Pixmap::new(w, h).ok_or_else(|| "the image is empty".to_string())?;
    for (dst, src) in pixmap.data_mut().chunks_exact_mut(4).zip(rgba.pixels()) {
        let [r, g, b, a] = src.0;
        let premultiply = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        dst.copy_from_slice(&[premultiply(r), premultiply(g), premultiply(b), a]);
    }
    Ok(pixmap)
}

/// A copy at half the size (box filter): enough to keep a large screenshot from aliasing when it
/// is drawn small.
fn halve(src: &Pixmap) -> Option<Pixmap> {
    let (w, h) = ((src.width() / 2).max(1), (src.height() / 2).max(1));
    let mut out = Pixmap::new(w, h)?;
    let (sw, sh) = (src.width() as usize, src.height() as usize);
    let data = src.data();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut sum = [0u32; 4];
            let mut n = 0;
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let (sx, sy) = ((2 * x + dx).min(sw - 1), (2 * y + dy).min(sh - 1));
                let i = (sy * sw + sx) * 4;
                for (s, v) in sum.iter_mut().zip(&data[i..i + 4]) {
                    *s += u32::from(*v);
                }
                n += 1;
            }
            let o = (y * w as usize + x) * 4;
            for (c, s) in sum.iter().enumerate() {
                out.data_mut()[o + c] = (s / n) as u8;
            }
        }
    }
    Some(out)
}

/// Draws `image` into the node's `w` x `h` box (node space) according to the node's fit.
pub(crate) fn draw_image(
    pixmap: &mut Pixmap,
    mask: Option<&Mask>,
    node: &Node,
    transform: Transform,
    opacity: f32,
    image: &Arc<Pixmap>,
) {
    let (w, h) = (node.width as f32, node.height as f32);
    let (iw, ih) = (image.width() as f32, image.height() as f32);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    // where the image lands in node space
    let (sx, sy) = match node.image_fit.unwrap_or(ImageFit::Cover) {
        ImageFit::Cover => {
            let s = (w / iw).max(h / ih);
            (s, s)
        }
        ImageFit::Contain => {
            let s = (w / iw).min(h / ih);
            (s, s)
        }
        ImageFit::Fill => (w / iw, h / ih),
    };
    let (dw, dh) = (iw * sx, ih * sy);
    let (ox, oy) = ((w - dw) / 2.0, (h - dh) / 2.0);
    let (ox, oy) = if node.image_fit == Some(ImageFit::Fill) {
        (0.0, 0.0)
    } else {
        (ox, oy)
    };

    // shrink first when the picture is drawn much smaller than it is stored
    let device_scale = sx.max(sy) * transform.sx.abs();
    let mut source = Arc::clone(image);
    let mut shrink = 1.0;
    while device_scale * shrink <= 0.5 && source.width() > 1 && source.height() > 1 {
        match halve(&source) {
            Some(smaller) => source = Arc::new(smaller),
            None => break,
        }
        shrink *= 2.0;
    }
    let pattern_transform = Transform::from_row(sx * shrink, 0.0, 0.0, sy * shrink, ox, oy);
    let quality = if device_scale * shrink >= 0.99 && device_scale * shrink <= 1.01 {
        FilterQuality::Nearest
    } else {
        FilterQuality::Bicubic
    };
    let source_ref: &Pixmap = &source;
    let paint = SkPaint {
        shader: Pattern::new(
            source_ref.as_ref(),
            SpreadMode::Pad,
            quality,
            opacity,
            pattern_transform,
        ),
        anti_alias: true,
        ..SkPaint::default()
    };
    let path = if node.image_fit == Some(ImageFit::Contain) {
        Rect::from_xywh(ox, oy, dw, dh).map(tiny_skia::PathBuilder::from_rect)
    } else {
        crate::rounded_rect(w, h, (node.corner_radius as f32).min(w.min(h) / 2.0))
    };
    if let Some(path) = path {
        pixmap.fill_path(&path, &paint, FillRule::Winding, transform, mask);
    }
}

/// The size of an image file in pixels, read from its header.
pub fn dimensions(path: &Path) -> Result<(u32, u32), String> {
    ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())
}
