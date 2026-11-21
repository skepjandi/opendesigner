//! Procedural assets: SVG written from a seed and the agent's colors. No data ships for any of
//! them. The same generator, size, colors, variant and seed always give the same text, so a
//! design can be remade and an asset can be named by its recipe.

use std::f64::consts::PI;
use std::fmt::Write;

use super::raster::is_hex_color;
use super::{Result, msg};

/// The most shapes a pattern may draw: the SVG stays far below the size the rasterizer takes.
const MAX_CELLS: u64 = 20_000;

/// What the agent asks for.
#[derive(Debug, Clone)]
pub struct Params {
    pub width: u32,
    pub height: u32,
    /// Hex colors, in the order the generator documents.
    pub colors: Vec<String>,
    pub seed: u64,
    /// A named variant (`dots`, `star-6`, `radial`, ...).
    pub variant: Option<String>,
    /// A size in pixels: a pattern's cell, the grain's speck.
    pub scale: Option<u32>,
}

/// SplitMix64: tiny, fast, the same on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `[0, 1)`.
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f()
    }
}

/// A number as short SVG text.
fn n(x: f64) -> String {
    let s = format!("{x:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.into()
    }
}

fn head(p: &Params) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\">",
        w = p.width,
        h = p.height
    )
}

/// The colors given, or `defaults` for what is missing.
fn colors(p: &Params, defaults: &[&str]) -> Vec<String> {
    if p.colors.is_empty() {
        defaults.iter().map(|c| c.to_string()).collect()
    } else {
        p.colors.clone()
    }
}

/// A fill for one color, or a gradient for several: `(defs, fill)`.
fn paint(cols: &[String]) -> (String, String) {
    if cols.len() < 2 {
        return (String::new(), cols.first().cloned().unwrap_or_default());
    }
    let mut defs =
        String::from("<defs><linearGradient id=\"p\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\">");
    for (i, c) in cols.iter().enumerate() {
        let _ = write!(
            defs,
            "<stop offset=\"{}\" stop-color=\"{c}\"/>",
            n(i as f64 / (cols.len() - 1) as f64)
        );
    }
    defs.push_str("</linearGradient></defs>");
    (defs, "url(#p)".into())
}

/// The number after a variant's name: `star-6` is 6.
fn count(variant: &str, name: &str, default: u32) -> u32 {
    variant
        .strip_prefix(name)
        .and_then(|r| r.strip_prefix('-'))
        .and_then(|r| r.parse().ok())
        .unwrap_or(default)
}

/// `(red, green, blue)` of a hex color, each 0 to 1.
fn rgb(hex: &str) -> (f64, f64, f64) {
    let h = hex.trim_start_matches('#');
    let full: String = if h.len() <= 4 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_string()
    };
    let ch = |i: usize| {
        full.get(i..i + 2)
            .and_then(|s| u8::from_str_radix(s, 16).ok())
            .map_or(0.0, |v| f64::from(v) / 255.0)
    };
    (ch(0), ch(2), ch(4))
}

/// Writes the SVG for a generator.
pub fn svg(generator: &str, p: &Params) -> Result<String> {
    if p.width == 0 || p.height == 0 {
        return msg("width and height must be at least 1");
    }
    if p.colors.len() > 8 {
        return msg("at most 8 colors");
    }
    if let Some(bad) = p.colors.iter().find(|c| !is_hex_color(c)) {
        return msg(format!(
            "{bad:?} is not a hex color like #1a2b3c; take the values from list_tokens"
        ));
    }
    let mut p = p.clone();
    p.colors = p.colors.iter().map(|c| c.to_ascii_lowercase()).collect();
    let mut rng = Rng(p.seed);
    match generator {
        "blob" => Ok(blob(&p, &mut rng)),
        "gradient" => Ok(gradient(&p, &mut rng)),
        "aurora" => Ok(aurora(&p, &mut rng)),
        "grain" => Ok(grain(&p)),
        "pattern" => pattern(&p, &mut rng),
        "shape" => shape(&p),
        other => msg(format!(
            "no generator {other:?}; use blob, gradient, aurora, grain, pattern or shape"
        )),
    }
}

/// A closed smooth curve through `points`, as cubic Béziers (Catmull-Rom, tension 1/6).
fn smooth_closed(points: &[(f64, f64)]) -> String {
    let len = points.len();
    let at = |i: usize| points[i % len];
    let mut d = format!("M{} {}", n(points[0].0), n(points[0].1));
    for i in 0..len {
        let (p0, p1, p2, p3) = (at(i + len - 1), at(i), at(i + 1), at(i + 2));
        let _ = write!(
            d,
            "C{} {} {} {} {} {}",
            n(p1.0 + (p2.0 - p0.0) / 6.0),
            n(p1.1 + (p2.1 - p0.1) / 6.0),
            n(p2.0 - (p3.0 - p1.0) / 6.0),
            n(p2.1 - (p3.1 - p1.1) / 6.0),
            n(p2.0),
            n(p2.1)
        );
    }
    d.push('Z');
    d
}

fn blob(p: &Params, rng: &mut Rng) -> String {
    let cols = colors(p, &["#6366f1"]);
    let points = p
        .variant
        .as_deref()
        .map_or(0, |v| count(v, "blob", 0))
        .clamp(0, 16);
    let points = if points >= 3 {
        points
    } else {
        5 + (rng.next() % 4) as u32
    };
    let (cx, cy) = (f64::from(p.width) / 2.0, f64::from(p.height) / 2.0);
    let r = f64::from(p.width.min(p.height)) / 2.0 * 0.94;
    let pts: Vec<(f64, f64)> = (0..points)
        .map(|i| {
            let a = 2.0 * PI * (f64::from(i) + rng.range(-0.18, 0.18)) / f64::from(points);
            let rr = r * rng.range(0.66, 1.0);
            (cx + rr * a.cos(), cy + rr * a.sin())
        })
        .collect();
    let (defs, fill) = paint(&cols);
    format!(
        "{}{defs}<path d=\"{}\" fill=\"{fill}\"/></svg>",
        head(p),
        smooth_closed(&pts)
    )
}

fn gradient(p: &Params, rng: &mut Rng) -> String {
    let cols = colors(p, &["#6366f1", "#ec4899"]);
    let cols = if cols.len() < 2 {
        vec![cols[0].clone(), cols[0].clone()]
    } else {
        cols
    };
    let stops: String = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            format!(
                "<stop offset=\"{}\" stop-color=\"{c}\"/>",
                n(i as f64 / (cols.len() - 1) as f64)
            )
        })
        .collect();
    let def = if p.variant.as_deref() == Some("radial") {
        format!(
            "<radialGradient id=\"g\" cx=\"0.5\" cy=\"0.5\" r=\"0.75\">{stops}</radialGradient>"
        )
    } else {
        let a = rng.range(0.0, 2.0 * PI);
        let (dx, dy) = (a.cos() / 2.0, a.sin() / 2.0);
        format!(
            "<linearGradient id=\"g\" x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\">{stops}</linearGradient>",
            n(0.5 - dx),
            n(0.5 - dy),
            n(0.5 + dx),
            n(0.5 + dy)
        )
    };
    format!(
        "{}<defs>{def}</defs><rect width=\"{}\" height=\"{}\" fill=\"url(#g)\"/></svg>",
        head(p),
        p.width,
        p.height
    )
}

fn aurora(p: &Params, rng: &mut Rng) -> String {
    let mut cols = colors(p, &["#0b1020", "#7c3aed", "#06b6d4", "#f472b6"]);
    if cols.len() < 2 {
        cols.extend(["#7c3aed".into(), "#06b6d4".into()]);
    }
    let (w, h) = (f64::from(p.width), f64::from(p.height));
    let big = w.max(h);
    let blur = (w.min(h) * 0.13).max(1.0);
    let mut clouds = String::new();
    for c in &cols[1..] {
        for _ in 0..2 {
            let _ = write!(
                clouds,
                "<ellipse cx=\"{}\" cy=\"{}\" rx=\"{}\" ry=\"{}\" fill=\"{c}\"/>",
                n(rng.range(0.05, 0.95) * w),
                n(rng.range(0.05, 0.95) * h),
                n(rng.range(0.18, 0.38) * big),
                n(rng.range(0.12, 0.28) * big),
            );
        }
    }
    format!(
        "{}<defs><filter id=\"b\" filterUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{}\" height=\"{}\"><feGaussianBlur stdDeviation=\"{}\"/></filter></defs><rect width=\"{}\" height=\"{}\" fill=\"{}\"/><g filter=\"url(#b)\" opacity=\"0.85\">{clouds}</g></svg>",
        head(p),
        p.width,
        p.height,
        n(blur),
        p.width,
        p.height,
        cols[0]
    )
}

fn grain(p: &Params) -> String {
    let overlay = p.variant.as_deref() == Some("overlay");
    let cols = colors(p, &["#f5f5f5", "#000000"]);
    let speck = if overlay { cols.last() } else { cols.get(1) };
    let (r, g, b) = rgb(speck.map_or("#000000", String::as_str));
    let freq = 0.9 / f64::from(p.scale.unwrap_or(1).clamp(1, 16));
    let ground = if overlay {
        String::new()
    } else {
        format!(
            "<rect width=\"{}\" height=\"{}\" fill=\"{}\"/>",
            p.width, p.height, cols[0]
        )
    };
    format!(
        "{}<defs><filter id=\"n\" filterUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" width=\"{w}\" height=\"{h}\"><feTurbulence type=\"fractalNoise\" baseFrequency=\"{}\" numOctaves=\"3\" seed=\"{}\" stitchTiles=\"stitch\"/><feColorMatrix type=\"matrix\" values=\"0 0 0 0 {} 0 0 0 0 {} 0 0 0 0 {} 0 0 0 0.9 -0.35\"/></filter></defs>{ground}<rect width=\"{w}\" height=\"{h}\" filter=\"url(#n)\"/></svg>",
        head(p),
        n(freq),
        p.seed % 1000,
        n(r),
        n(g),
        n(b),
        w = p.width,
        h = p.height
    )
}

type Segment = ((f64, f64), (f64, f64));

/// Smooth value noise over a lattice of random heights.
struct Lattice {
    cols: usize,
    values: Vec<f64>,
    spacing: f64,
}

impl Lattice {
    fn new(rng: &mut Rng, w: f64, h: f64, spacing: f64) -> Lattice {
        let cols = (w / spacing).ceil() as usize + 2;
        let rows = (h / spacing).ceil() as usize + 2;
        Lattice {
            cols,
            values: (0..cols * rows).map(|_| rng.f()).collect(),
            spacing,
        }
    }

    fn at(&self, x: f64, y: f64) -> f64 {
        let (gx, gy) = (x / self.spacing, y / self.spacing);
        let (ix, iy) = (gx.floor() as usize, gy.floor() as usize);
        let smooth = |t: f64| t * t * (3.0 - 2.0 * t);
        let (fx, fy) = (smooth(gx - gx.floor()), smooth(gy - gy.floor()));
        let v = |dx: usize, dy: usize| self.values[(iy + dy) * self.cols + ix + dx];
        let top = v(0, 0) + (v(1, 0) - v(0, 0)) * fx;
        let bottom = v(0, 1) + (v(1, 1) - v(0, 1)) * fx;
        top + (bottom - top) * fy
    }
}

fn pattern(p: &Params, rng: &mut Rng) -> Result<String> {
    let cols = colors(p, &["#f8fafc", "#334155"]);
    let (bg, ink) = (
        cols[0].clone(),
        cols.get(1).cloned().unwrap_or_else(|| "#334155".into()),
    );
    let (w, h) = (f64::from(p.width), f64::from(p.height));
    let cell = f64::from(p.scale.unwrap_or(0));
    let cell = if cell >= 2.0 {
        cell
    } else {
        (w.min(h) / 16.0).max(8.0)
    };
    let cells = ((w / cell).ceil() + 1.0) as u64 * ((h / cell).ceil() + 1.0) as u64;
    if cells > MAX_CELLS {
        return msg(format!(
            "a cell of {} px makes {cells} cells in {}x{}; raise scale (at most {MAX_CELLS} cells)",
            n(cell),
            p.width,
            p.height
        ));
    }
    let variant = p.variant.as_deref().unwrap_or("dots");
    let sw = (cell * 0.12).max(0.5);
    let mut body = String::new();
    let stroke_path = |d: &str| {
        format!(
            "<path d=\"{d}\" fill=\"none\" stroke=\"{ink}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>",
            n(sw)
        )
    };
    match variant {
        "dots" => {
            let mut y = cell / 2.0;
            while y < h + cell / 2.0 {
                let mut x = cell / 2.0;
                while x < w + cell / 2.0 {
                    let _ = write!(
                        body,
                        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{ink}\"/>",
                        n(x),
                        n(y),
                        n(cell * 0.12)
                    );
                    x += cell;
                }
                y += cell;
            }
        }
        "lines" => {
            let mut d = String::new();
            let mut x = -h;
            while x < w {
                let _ = write!(d, "M{} {}L{} 0", n(x), n(h), n(x + h));
                x += cell;
            }
            body = stroke_path(&d);
        }
        "grid" => {
            let mut d = String::new();
            let mut x = 0.0;
            while x <= w {
                let _ = write!(d, "M{} 0V{}", n(x), n(h));
                x += cell;
            }
            let mut y = 0.0;
            while y <= h {
                let _ = write!(d, "M0 {}H{}", n(y), n(w));
                y += cell;
            }
            body = stroke_path(&d);
        }
        "checker" => {
            let mut d = String::new();
            let (mut j, mut y) = (0u32, 0.0);
            while y < h {
                let (mut i, mut x) = (0u32, 0.0);
                while x < w {
                    if (i + j) % 2 == 0 {
                        let _ = write!(d, "M{} {}h{c}v{c}h-{c}z", n(x), n(y), c = n(cell));
                    }
                    i += 1;
                    x += cell;
                }
                j += 1;
                y += cell;
            }
            body = format!("<path d=\"{d}\" fill=\"{ink}\"/>");
        }
        "waves" | "zigzag" => {
            let mut d = String::new();
            let amp = cell * 0.25;
            let step = if variant == "waves" {
                cell / 4.0
            } else {
                cell / 2.0
            };
            let mut y = cell / 2.0;
            while y < h + cell {
                let mut x = 0.0;
                let mut k = 0u32;
                while x <= w + step {
                    let dy = if variant == "waves" {
                        amp * (x / cell * PI).sin()
                    } else if k % 2 == 0 {
                        -amp
                    } else {
                        amp
                    };
                    let _ = write!(
                        d,
                        "{}{} {}",
                        if x == 0.0 { "M" } else { "L" },
                        n(x),
                        n(y + dy)
                    );
                    x += step;
                    k += 1;
                }
                y += cell;
            }
            body = stroke_path(&d);
        }
        "topo" => {
            let lat = Lattice::new(rng, w, h, cell * 4.0);
            let step = (cell / 3.0).max((w * h / MAX_CELLS as f64).sqrt()).max(2.0);
            let (nx, ny) = ((w / step).ceil() as usize, (h / step).ceil() as usize);
            let field: Vec<f64> = (0..=ny)
                .flat_map(|j| (0..=nx).map(move |i| (i, j)))
                .map(|(i, j)| lat.at(i as f64 * step, j as f64 * step))
                .collect();
            let v = |i: usize, j: usize| field[j * (nx + 1) + i];
            let mut d = String::new();
            for level in 1..=8 {
                let l = f64::from(level) / 9.0;
                for j in 0..ny {
                    for i in 0..nx {
                        let (tl, tr, br, bl) = (v(i, j), v(i + 1, j), v(i + 1, j + 1), v(i, j + 1));
                        let case = u8::from(tl >= l)
                            | u8::from(tr >= l) << 1
                            | u8::from(br >= l) << 2
                            | u8::from(bl >= l) << 3;
                        let (x0, y0) = (i as f64 * step, j as f64 * step);
                        let t = |a: f64, b: f64| (l - a) / (b - a);
                        let top = (x0 + step * t(tl, tr), y0);
                        let right = (x0 + step, y0 + step * t(tr, br));
                        let bottom = (x0 + step * t(bl, br), y0 + step);
                        let left = (x0, y0 + step * t(tl, bl));
                        let segs: &[Segment] = match case {
                            1 | 14 => &[(left, top)],
                            2 | 13 => &[(top, right)],
                            3 | 12 => &[(left, right)],
                            4 | 11 => &[(right, bottom)],
                            5 => &[(left, top), (right, bottom)],
                            6 | 9 => &[(top, bottom)],
                            7 | 8 => &[(left, bottom)],
                            10 => &[(top, right), (left, bottom)],
                            _ => &[],
                        };
                        for (a, b) in segs {
                            let _ = write!(d, "M{} {}L{} {}", n(a.0), n(a.1), n(b.0), n(b.1));
                        }
                    }
                }
            }
            body = stroke_path(&d);
        }
        other => {
            return msg(format!(
                "no pattern {other:?}; use dots, lines, waves, grid, checker, zigzag or topo"
            ));
        }
    }
    Ok(format!(
        "{}<rect width=\"{}\" height=\"{}\" fill=\"{bg}\"/>{body}</svg>",
        head(p),
        p.width,
        p.height
    ))
}

fn shape(p: &Params) -> Result<String> {
    let cols = colors(p, &["#6366f1"]);
    let (defs, fill) = paint(&cols);
    let (w, h) = (f64::from(p.width), f64::from(p.height));
    let (cx, cy) = (w / 2.0, h / 2.0);
    let r = w.min(h) / 2.0 * 0.92;
    let variant = p.variant.as_deref().unwrap_or("circle");
    let star = |points: u32, inner: f64| -> String {
        (0..points * 2)
            .map(|i| {
                let a = PI * f64::from(i) / f64::from(points) - PI / 2.0;
                let rr = if i % 2 == 0 { r } else { r * inner };
                format!("{},{}", n(cx + rr * a.cos()), n(cy + rr * a.sin()))
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let poly = |sides: u32| -> String {
        (0..sides)
            .map(|i| {
                let a = 2.0 * PI * f64::from(i) / f64::from(sides) - PI / 2.0;
                format!("{},{}", n(cx + r * a.cos()), n(cy + r * a.sin()))
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    let element = match variant {
        "circle" => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"{fill}\"/>",
            n(cx),
            n(cy),
            n(r)
        ),
        "rounded-rect" => format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"{fill}\"/>",
            n(w * 0.04),
            n(h * 0.04),
            n(w * 0.92),
            n(h * 0.92),
            n(w.min(h) * 0.2)
        ),
        "triangle" => format!("<polygon points=\"{}\" fill=\"{fill}\"/>", poly(3)),
        "ring" => format!(
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"none\" stroke=\"{fill}\" stroke-width=\"{}\"/>",
            n(cx),
            n(cy),
            n(r * 0.86),
            n(r * 0.28)
        ),
        "burst" => format!("<polygon points=\"{}\" fill=\"{fill}\"/>", star(14, 0.72)),
        v if v.starts_with("star") => {
            let k = count(v, "star", 5).clamp(3, 24);
            format!("<polygon points=\"{}\" fill=\"{fill}\"/>", star(k, 0.45))
        }
        v if v.starts_with("polygon") => {
            let k = count(v, "polygon", 6).clamp(3, 24);
            format!("<polygon points=\"{}\" fill=\"{fill}\"/>", poly(k))
        }
        other => {
            return msg(format!(
                "no shape {other:?}; use circle, rounded-rect, triangle, star, polygon, ring or burst"
            ));
        }
    };
    Ok(format!("{}{defs}{element}</svg>", head(p)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::raster;

    fn params(seed: u64, variant: Option<&str>, colors: &[&str]) -> Params {
        Params {
            width: 200,
            height: 120,
            colors: colors.iter().map(|c| c.to_string()).collect(),
            seed,
            variant: variant.map(str::to_string),
            scale: None,
        }
    }

    #[test]
    fn every_generator_and_variant_is_valid_svg_that_draws() {
        let cases: &[(&str, &[&str])] = &[
            ("blob", &[""]),
            ("blob", &["blob-9"]),
            ("gradient", &["linear", "radial"]),
            ("aurora", &[""]),
            ("grain", &[""]),
            (
                "pattern",
                &[
                    "dots", "lines", "waves", "grid", "checker", "zigzag", "topo",
                ],
            ),
            (
                "shape",
                &[
                    "circle",
                    "rounded-rect",
                    "triangle",
                    "star",
                    "star-8",
                    "polygon-5",
                    "ring",
                    "burst",
                ],
            ),
        ];
        for (generator, variants) in cases {
            for v in *variants {
                let variant = if v.is_empty() { None } else { Some(*v) };
                let text = svg(generator, &params(7, variant, &["#112233", "#ffcc00"])).unwrap();
                let (png, w, h) = raster::rasterize(&text, None, None)
                    .unwrap_or_else(|e| panic!("{generator} {v}: {e}"));
                assert_eq!((w, h), (512, 307));
                assert!(png.starts_with(b"\x89PNG"), "{generator} {v}");
            }
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_svg_and_another_seed_another() {
        for g in ["blob", "gradient", "aurora", "grain", "pattern", "shape"] {
            let variant = (g == "pattern").then_some("topo");
            let a = svg(g, &params(1, variant, &["#102030", "#aabbcc"])).unwrap();
            let b = svg(g, &params(1, variant, &["#102030", "#aabbcc"])).unwrap();
            let c = svg(g, &params(2, variant, &["#102030", "#aabbcc"])).unwrap();
            assert_eq!(a, b, "{g} is not deterministic");
            if g != "shape" {
                assert_ne!(a, c, "{g} ignores the seed");
            }
        }
    }

    #[test]
    fn bad_input_is_refused_with_a_reason() {
        assert!(
            svg("blob", &params(1, None, &["red"]))
                .unwrap_err()
                .to_string()
                .contains("hex")
        );
        assert!(svg("nope", &params(1, None, &[])).is_err());
        assert!(svg("pattern", &params(1, Some("nope"), &[])).is_err());
        let mut big = params(1, Some("dots"), &[]);
        big.width = 4000;
        big.height = 4000;
        big.scale = Some(4);
        assert!(
            svg("pattern", &big)
                .unwrap_err()
                .to_string()
                .contains("scale")
        );
        assert!(svg("blob", &params(1, None, &["#111"; 9])).is_err());
    }
}
