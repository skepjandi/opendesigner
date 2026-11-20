//! Colour grading and blur on pixels (`od_core::grade`), all on the CPU and deterministic.
//!
//! The order is fixed: blur, exposure, temperature, tint, contrast, saturation, vignette.
//! Colour maths runs on the stored (gamma-encoded) values, as most editors' basic controls do;
//! it is not done in linear light. Pixmaps are premultiplied, so each pixel is unpremultiplied
//! before the colour steps and premultiplied again after them. Blur is three box passes, which
//! approximates a Gaussian of the same sigma to within a few percent and costs the same for any
//! radius.

use std::sync::Arc;

use od_core::Grade;
use tiny_skia::Pixmap;

/// Blur radius in pixels for a grade's `blur` number on a picture of this size.
pub fn blur_sigma(blur: f64, width: u32, height: u32) -> f32 {
    (blur / 1000.0 * f64::from(width.min(height))) as f32
}

/// `image` graded, or `image` itself when there is nothing to do.
pub fn graded(image: Arc<Pixmap>, grade: Option<&Grade>) -> Arc<Pixmap> {
    match grade.filter(|g| !g.is_neutral()) {
        None => image,
        Some(g) => {
            let mut copy = (*image).clone();
            apply(&mut copy, g);
            Arc::new(copy)
        }
    }
}

/// Grades `pixmap` in place.
pub fn apply(pixmap: &mut Pixmap, grade: &Grade) {
    if grade.is_neutral() {
        return;
    }
    let sigma = blur_sigma(grade.blur, pixmap.width(), pixmap.height());
    if sigma > 0.0 {
        blur(pixmap, sigma);
    }
    if grade.exposure != 0.0
        || grade.temperature != 0.0
        || grade.tint != 0.0
        || grade.contrast != 0.0
        || grade.saturation != 0.0
        || grade.vignette != 0.0
    {
        colour(pixmap, grade);
    }
}

/// Per channel: exposure, temperature, tint and contrast, as a table over the 256 stored values.
fn tone_tables(g: &Grade) -> [[f32; 256]; 3] {
    let gain = (g.exposure as f32).exp2();
    let channel_gain = [
        1.0 + 0.25 * g.temperature as f32,
        1.0 - 0.15 * g.tint as f32,
        1.0 - 0.25 * g.temperature as f32,
    ];
    let slope = 1.0 + g.contrast as f32;
    let mut tables = [[0.0; 256]; 3];
    for (table, tint) in tables.iter_mut().zip(channel_gain) {
        for (v, out) in table.iter_mut().enumerate() {
            let lit = (v as f32 / 255.0 * gain * tint).clamp(0.0, 1.0);
            *out = ((lit - 0.5) * slope + 0.5).clamp(0.0, 1.0);
        }
    }
    tables
}

fn colour(pixmap: &mut Pixmap, g: &Grade) {
    let tables = tone_tables(g);
    let saturation = 1.0 + g.saturation as f32;
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let vignette = g.vignette as f32;
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    for (i, px) in pixmap.data_mut().chunks_exact_mut(4).enumerate() {
        let a = px[3];
        if a == 0 {
            continue;
        }
        let alpha = f32::from(a) / 255.0;
        let straight = |c: u8| (f32::from(c) / alpha).round().min(255.0) as usize;
        let mut rgb = [
            tables[0][straight(px[0])],
            tables[1][straight(px[1])],
            tables[2][straight(px[2])],
        ];
        if saturation != 1.0 {
            let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            for c in &mut rgb {
                *c = (luma + (*c - luma) * saturation).clamp(0.0, 1.0);
            }
        }
        let shade = if vignette > 0.0 {
            let (x, y) = ((i % w) as f32 + 0.5, (i / w) as f32 + 0.5);
            let d = (((x - cx) / cx).powi(2) + ((y - cy) / cy).powi(2)).sqrt()
                / std::f32::consts::SQRT_2;
            1.0 - vignette * smoothstep(0.4, 1.0, d)
        } else {
            1.0
        };
        for (out, c) in px.iter_mut().zip(rgb) {
            *out = (c * shade * alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

fn smoothstep(lo: f32, hi: f32, v: f32) -> f32 {
    let t = ((v - lo) / (hi - lo)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The widths of the three box passes that approximate a Gaussian of `sigma`.
fn box_sizes(sigma: f32) -> [usize; 3] {
    let ideal = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let mut lower = ideal.floor() as usize;
    if lower % 2 == 0 {
        lower = lower.saturating_sub(1);
    }
    let lower = lower.max(1);
    let upper = lower + 2;
    let m = ((12.0 * sigma * sigma - 3.0 * (lower * lower) as f32 - 12.0 * lower as f32 - 9.0)
        / (-4.0 * lower as f32 - 4.0))
        .round()
        .clamp(0.0, 3.0) as usize;
    std::array::from_fn(|i| if i < m { lower } else { upper })
}

/// Gaussian-like blur of every channel (premultiplied) by `sigma` pixels.
pub fn blur(pixmap: &mut Pixmap, sigma: f32) {
    if sigma <= 0.0 {
        return;
    }
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let mut scratch = vec![0u8; pixmap.data().len()];
    for size in box_sizes(sigma) {
        let radius = size / 2;
        box_pass(pixmap.data(), &mut scratch, w, h, radius, true);
        box_pass(&scratch, pixmap.data_mut(), w, h, radius, false);
    }
}

/// One box blur of half-width `radius` along rows (`horizontal`) or columns, edges repeated.
fn box_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, radius: usize, horizontal: bool) {
    let (lines, len, step, line_step) = if horizontal {
        (h, w, 4, w * 4)
    } else {
        (w, h, w * 4, 4)
    };
    let window = (2 * radius + 1) as u32;
    for line in 0..lines {
        let base = line * line_step;
        for ch in 0..4 {
            let at = |i: usize| base + i * step + ch;
            let mut sum = u32::from(src[at(0)]) * (radius as u32 + 1);
            for i in 1..=radius {
                sum += u32::from(src[at(i.min(len - 1))]);
            }
            for i in 0..len {
                dst[at(i)] = ((sum + window / 2) / window) as u8;
                let enter = (i + radius + 1).min(len - 1);
                let leave = i.saturating_sub(radius);
                sum = sum + u32::from(src[at(enter)]) - u32::from(src[at(leave)]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::Look;

    /// A horizontal grey ramp, 256 wide: the value at column x is x.
    fn ramp() -> Pixmap {
        let mut p = Pixmap::new(256, 8).unwrap();
        for (i, px) in p.data_mut().chunks_exact_mut(4).enumerate() {
            let v = (i % 256) as u8;
            px.copy_from_slice(&[v, v, v, 255]);
        }
        p
    }

    fn at(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * p.width() + x) * 4) as usize;
        [
            p.data()[i],
            p.data()[i + 1],
            p.data()[i + 2],
            p.data()[i + 3],
        ]
    }

    fn graded_with(g: Grade) -> Pixmap {
        let mut p = ramp();
        apply(&mut p, &g);
        p
    }

    #[test]
    fn a_neutral_grade_changes_nothing() {
        assert_eq!(graded_with(Grade::default()).data(), ramp().data());
    }

    #[test]
    fn exposure_scales_by_stops_and_clips_at_white() {
        let p = graded_with(Grade {
            exposure: 1.0,
            ..Grade::default()
        });
        assert_eq!(at(&p, 50, 0)[0], 100);
        assert_eq!(at(&p, 100, 0)[0], 200);
        assert_eq!(at(&p, 200, 0)[0], 255);
        let dark = graded_with(Grade {
            exposure: -1.0,
            ..Grade::default()
        });
        assert_eq!(at(&dark, 200, 0)[0], 100);
    }

    #[test]
    fn contrast_pivots_around_mid_grey() {
        let more = graded_with(Grade {
            contrast: 0.5,
            ..Grade::default()
        });
        let mid = at(&more, 128, 0)[0];
        assert!(mid.abs_diff(128) <= 1, "{mid}");
        assert!(at(&more, 64, 0)[0] < 64 && at(&more, 192, 0)[0] > 192);
        let flat = graded_with(Grade {
            contrast: -1.0,
            ..Grade::default()
        });
        assert!(flat.data().chunks_exact(4).all(|p| p[0].abs_diff(128) <= 1));
    }

    #[test]
    fn saturation_removes_colour_and_keeps_grey() {
        let mut p = Pixmap::new(2, 1).unwrap();
        p.data_mut()
            .copy_from_slice(&[200, 50, 50, 255, 90, 90, 90, 255]);
        apply(
            &mut p,
            &Grade {
                saturation: -1.0,
                ..Grade::default()
            },
        );
        let [r, g, b, _] = at(&p, 0, 0);
        assert!(r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1, "{r} {g} {b}");
        let luma = (0.2126 * 200.0 + 0.7152 * 50.0 + 0.0722 * 50.0_f32).round() as u8;
        assert!(r.abs_diff(luma) <= 1, "{r} vs {luma}");
        assert_eq!(at(&p, 1, 0), [90, 90, 90, 255]);
    }

    #[test]
    fn warm_pushes_red_up_and_blue_down_and_tint_moves_green() {
        let mut p = Pixmap::new(1, 1).unwrap();
        p.data_mut().copy_from_slice(&[100, 100, 100, 255]);
        let mut warm = p.clone();
        apply(
            &mut warm,
            &Grade {
                temperature: 1.0,
                ..Grade::default()
            },
        );
        assert_eq!(at(&warm, 0, 0), [125, 100, 75, 255]);
        let mut magenta = p.clone();
        apply(
            &mut magenta,
            &Grade {
                tint: 1.0,
                ..Grade::default()
            },
        );
        assert_eq!(at(&magenta, 0, 0), [100, 85, 100, 255]);
    }

    #[test]
    fn the_vignette_darkens_corners_and_leaves_the_middle() {
        let mut p = Pixmap::new(101, 101).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(200, 200, 200, 255));
        apply(
            &mut p,
            &Grade {
                vignette: 1.0,
                ..Grade::default()
            },
        );
        assert_eq!(at(&p, 50, 50), [200, 200, 200, 255]);
        assert!(at(&p, 0, 0)[0] < 5, "{:?}", at(&p, 0, 0));
        assert!(at(&p, 0, 50)[0] > at(&p, 0, 0)[0]);
    }

    #[test]
    fn blur_keeps_flat_colour_spreads_an_edge_and_is_symmetric() {
        let mut flat = Pixmap::new(20, 20).unwrap();
        flat.fill(tiny_skia::Color::from_rgba8(10, 120, 240, 255));
        let before = flat.data().to_vec();
        blur(&mut flat, 3.0);
        assert_eq!(flat.data(), &before[..]);

        let mut edge = Pixmap::new(64, 4).unwrap();
        for (i, px) in edge.data_mut().chunks_exact_mut(4).enumerate() {
            let v = if i % 64 < 32 { 0 } else { 255 };
            px.copy_from_slice(&[v, v, v, 255]);
        }
        blur(&mut edge, 4.0);
        let row: Vec<u8> = (0..64).map(|x| at(&edge, x, 1)[0]).collect();
        assert!(row.windows(2).all(|w| w[0] <= w[1]), "monotonic {row:?}");
        assert!(
            row[31].abs_diff(127) <= 30 && row[32].abs_diff(128) <= 30,
            "{row:?}"
        );
        assert_eq!(row[0], 0);
        assert_eq!(row[63], 255);
        assert!(row[31] > 0 && row[31] < 255, "{row:?}");
        // mirrored around the edge
        for k in 0..8 {
            assert!(
                u16::from(row[31 - k]) + u16::from(row[32 + k]) >= 253
                    && u16::from(row[31 - k]) + u16::from(row[32 + k]) <= 257,
                "{k}: {row:?}"
            );
        }
    }

    #[test]
    fn the_box_widths_match_a_gaussians_variance() {
        for sigma in [1.0_f32, 3.0, 8.0, 25.0] {
            let sizes = box_sizes(sigma);
            let variance: f32 = sizes.iter().map(|&s| ((s * s) as f32 - 1.0) / 12.0).sum();
            assert!(
                (variance.sqrt() - sigma).abs() < sigma * 0.1 + 0.3,
                "{sigma}: {}",
                variance.sqrt()
            );
        }
    }

    #[test]
    fn looks_are_distinct_and_mono_has_no_colour() {
        let mut p = Pixmap::new(1, 1).unwrap();
        p.data_mut().copy_from_slice(&[180, 90, 40, 255]);
        let outputs: Vec<[u8; 4]> = Look::ALL
            .iter()
            .map(|l| {
                let mut c = p.clone();
                apply(&mut c, &l.grade());
                at(&c, 0, 0)
            })
            .collect();
        for (i, a) in outputs.iter().enumerate() {
            assert!(outputs.iter().skip(i + 1).all(|b| a != b), "{outputs:?}");
        }
        let mono = outputs[2];
        assert!(mono[0].abs_diff(mono[1]) <= 1 && mono[1].abs_diff(mono[2]) <= 1);
    }

    #[test]
    fn transparent_pixels_stay_transparent_and_grading_is_repeatable() {
        let mut p = Pixmap::new(2, 1).unwrap();
        p.data_mut()
            .copy_from_slice(&[0, 0, 0, 0, 100, 50, 25, 128]);
        let g = Look::Punchy.grade();
        let mut a = p.clone();
        apply(&mut a, &g);
        let mut b = p.clone();
        apply(&mut b, &g);
        assert_eq!(a.data(), b.data());
        assert_eq!(&a.data()[..4], &[0, 0, 0, 0]);
        assert_eq!(a.data()[7], 128);
    }
}
