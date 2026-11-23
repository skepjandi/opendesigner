//! Palette quantization: every pixel of a picture in `media/` becomes the nearest colour of a
//! palette (nearest in OKLab, so the eye agrees), and a pixel that is mostly transparent becomes
//! fully transparent. The result is a new file; the original is never touched. No dithering:
//! pixel art wants flat colours (`docs/wiki/store-assets.md#palette-quantize`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Write;
use std::path::Path;

use od_core::Rgba;
use serde::{Deserialize, Serialize};

use crate::tools::ToolError;

/// Most colours a palette has.
pub const MAX_PALETTE: usize = 256;
/// Most pixels of a picture that is quantized.
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// Alpha at or above which a pixel stays, when the caller names none.
pub const DEFAULT_ALPHA_THRESHOLD: u8 = 128;

fn fail<T>(msg: impl Into<String>) -> Result<T, ToolError> {
    Err(ToolError(msg.into()))
}

/// Where the palette comes from.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum PaletteSpec {
    /// A style pack id (its palette and the tints derived from it).
    Pack(String),
    /// Token names (`brand.gold`, `{brand.gold}`) and `#rrggbb` colours.
    Colors(Vec<String>),
}

/// The colours of `spec`, without repeats, in the order given.
pub fn resolve_palette(
    spec: &PaletteSpec,
    tokens: &BTreeMap<String, String>,
) -> Result<Vec<Rgba>, ToolError> {
    let hexes: Vec<String> = match spec {
        PaletteSpec::Pack(id) => {
            let pack = crate::style_pack::find(id).ok_or_else(|| {
                ToolError(format!(
                    "{id:?} is not a style pack id; call list_style_packs, or pass a list of colours"
                ))
            })?;
            crate::style_pack::tokens(pack)
                .into_iter()
                .map(|(_, hex)| hex)
                .collect()
        }
        PaletteSpec::Colors(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                let name = item
                    .strip_prefix('{')
                    .and_then(|s| s.strip_suffix('}'))
                    .unwrap_or(item);
                if name.starts_with('#') {
                    out.push(name.to_string());
                } else if let Some(hex) = tokens.get(name) {
                    out.push(hex.clone());
                } else {
                    return fail(format!(
                        "{item:?} is neither a #rrggbb colour nor a token of this project; see list_tokens"
                    ));
                }
            }
            out
        }
    };
    let mut colors: Vec<Rgba> = Vec::new();
    for hex in &hexes {
        let c = Rgba::parse_hex(hex).map_err(|_| {
            ToolError(format!(
                "{hex:?} is not a solid colour: a palette takes #rrggbb values or tokens that hold one"
            ))
        })?;
        let c = Rgba { a: 255, ..c };
        if !colors.contains(&c) {
            colors.push(c);
        }
    }
    if colors.is_empty() || colors.len() > MAX_PALETTE {
        return fail(format!("a palette has 1 to {MAX_PALETTE} colours"));
    }
    Ok(colors)
}

fn lab(c: Rgba) -> [f64; 3] {
    od_core::color::oklab([
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
    ])
}

/// The palette index nearest to `c` in OKLab (the first of equals).
fn nearest(c: Rgba, palette_lab: &[[f64; 3]]) -> usize {
    let p = lab(c);
    let mut best = (0, f64::INFINITY);
    for (i, q) in palette_lab.iter().enumerate() {
        let d = (p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// A quantized picture: straight RGBA bytes, and what became what.
pub struct Quantized {
    pub rgba: Vec<u8>,
    /// Pixels per palette entry, in palette order.
    pub counts: Vec<u64>,
    pub transparent: u64,
    /// Distinct opaque colours the kept pixels had before.
    pub colors_before: usize,
}

/// Maps every pixel of `rgba` (straight RGBA) to the nearest of `palette`; a pixel whose alpha is
/// under `alpha_threshold` becomes transparent (`0, 0, 0, 0`), any other becomes opaque.
pub fn quantize_rgba(rgba: &[u8], palette: &[Rgba], alpha_threshold: u8) -> Quantized {
    let palette_lab: Vec<[f64; 3]> = palette.iter().copied().map(lab).collect();
    let mut cache: HashMap<u32, usize> = HashMap::new();
    let mut before: HashSet<u32> = HashSet::new();
    let mut counts = vec![0u64; palette.len()];
    let mut transparent = 0;
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.chunks_exact(4) {
        if px[3] < alpha_threshold || px[3] == 0 {
            out.extend([0, 0, 0, 0]);
            transparent += 1;
            continue;
        }
        let key = u32::from_be_bytes([0, px[0], px[1], px[2]]);
        before.insert(key);
        let index = *cache.entry(key).or_insert_with(|| {
            nearest(
                Rgba {
                    r: px[0],
                    g: px[1],
                    b: px[2],
                    a: 255,
                },
                &palette_lab,
            )
        });
        counts[index] += 1;
        let c = palette[index];
        out.extend([c.r, c.g, c.b, 255]);
    }
    Quantized {
        rgba: out,
        counts,
        transparent,
        colors_before: before.len(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ColorCount {
    pub color: String,
    pub pixels: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    /// `media/<name>-q.png`: what an image node names.
    pub file: String,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub palette_size: usize,
    pub colors_before: usize,
    pub colors_after: usize,
    pub transparent_pixels: u64,
    /// The palette colours that are used, most used first.
    pub colors: Vec<ColorCount>,
}

/// Writes `bytes` to the first free `media/<stem>-q.png`, `-q-2.png`, ... and returns its name.
fn write_free(media: &Path, stem: &str, bytes: &[u8]) -> Result<String, ToolError> {
    for n in 1..1000 {
        let name = if n == 1 {
            format!("{stem}-q.png")
        } else {
            format!("{stem}-q-{n}.png")
        };
        let opened = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(media.join(&name));
        match opened {
            Ok(mut file) => {
                file.write_all(bytes)?;
                return Ok(name);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.into()),
        }
    }
    fail("no free file name for the quantized picture")
}

/// Quantizes `media/<file>` in the project folder `dir` to `palette` and writes the result as a
/// new `media/<stem>-q.png`.
pub fn quantize_file(
    dir: &Path,
    file: &str,
    palette: &[Rgba],
    alpha_threshold: u8,
) -> Result<Report, ToolError> {
    let path = od_core::media::resolve(dir, file).map_err(|e| ToolError(e.to_string()))?;
    let (width, height, rgba) = od_render::decode_rgba(&path, MAX_PIXELS).map_err(ToolError)?;
    let q = quantize_rgba(&rgba, palette, alpha_threshold);
    let pixmap = od_render::pixmap_from_rgba(width, height, &q.rgba)
        .ok_or_else(|| ToolError("the picture is too large to write".into()))?;
    let png = od_render::encode_png(&pixmap, true)?;
    let reference = od_core::media::reference_name(file).map_err(|e| ToolError(e.to_string()))?;
    let stem = reference.rsplit_once('.').map_or(reference, |(s, _)| s);
    let name = write_free(&dir.join("media"), stem, &png)?;

    let mut colors: Vec<ColorCount> = palette
        .iter()
        .zip(&q.counts)
        .filter(|(_, n)| **n > 0)
        .map(|(c, n)| ColorCount {
            color: c.to_hex(),
            pixels: *n,
        })
        .collect();
    colors.sort_by(|a, b| b.pixels.cmp(&a.pixels).then_with(|| a.color.cmp(&b.color)));
    Ok(Report {
        file: format!("media/{name}"),
        source: format!("media/{reference}"),
        width,
        height,
        palette_size: palette.len(),
        colors_before: q.colors_before,
        colors_after: colors.len(),
        transparent_pixels: q.transparent,
        colors,
    })
}
