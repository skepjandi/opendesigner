//! The soft shadow: the device's silhouette, shrunk to a small grid, blurred twice (a wide soft
//! one and a tight contact one), pushed away from the light and drawn under the device. It fades
//! to nothing before the viewport's edge.

use super::math::smoothstep;

fn box_blur(src: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut a = src.to_vec();
    let mut b = vec![0.0f32; w * h];
    let norm = 1.0 / (2 * radius + 1) as f32;
    for _ in 0..3 {
        // Horizontal: a -> b, zero outside.
        for y in 0..h {
            let row = &a[y * w..(y + 1) * w];
            let out = &mut b[y * w..(y + 1) * w];
            let mut acc: f32 = row[..radius.min(w - 1) + 1].iter().sum();
            for x in 0..w {
                out[x] = acc * norm;
                if x + radius + 1 < w {
                    acc += row[x + radius + 1];
                }
                if x >= radius {
                    acc -= row[x - radius];
                }
            }
        }
        // Vertical: b -> a.
        for x in 0..w {
            let mut acc: f32 = (0..=radius.min(h - 1)).map(|y| b[y * w + x]).sum();
            for y in 0..h {
                a[y * w + x] = acc * norm;
                if y + radius + 1 < h {
                    acc += b[(y + radius + 1) * w + x];
                }
                if y >= radius {
                    acc -= b[(y - radius) * w + x];
                }
            }
        }
    }
    a
}

fn bilinear(g: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x, y) = (x - 0.5, y - 0.5);
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (x - fx, y - fy);
    let at = |cx: f32, cy: f32| {
        if cx < 0.0 || cy < 0.0 || cx >= w as f32 || cy >= h as f32 {
            0.0
        } else {
            g[cy as usize * w + cx as usize]
        }
    };
    let top = at(fx, fy) * (1.0 - tx) + at(fx + 1.0, fy) * tx;
    let bot = at(fx, fy + 1.0) * (1.0 - tx) + at(fx + 1.0, fy + 1.0) * tx;
    top * (1.0 - ty) + bot * ty
}

/// Darkens the transparent parts of `data` (premultiplied RGBA, `w` x `h`) with the shadow of the
/// opaque parts at `strength` (0..1).
pub fn apply(data: &mut [u8], w: usize, h: usize, strength: f32, light_angle: f64, threads: usize) {
    if strength <= 0.0 || w < 2 || h < 2 {
        return;
    }
    let k = w.max(h).div_ceil(256).max(1);
    let (sw, sh) = (w.div_ceil(k), h.div_ceil(k));
    let mut lo = vec![0.0f32; sw * sh];
    for (y, row) in lo.chunks_mut(sw).enumerate() {
        for (x, v) in row.iter_mut().enumerate() {
            let mut acc = 0u32;
            let mut n = 0u32;
            for yy in (y * k)..((y + 1) * k).min(h) {
                for xx in (x * k)..((x + 1) * k).min(w) {
                    acc += u32::from(data[(yy * w + xx) * 4 + 3]);
                    n += 1;
                }
            }
            *v = acc as f32 / (255.0 * n.max(1) as f32);
        }
    }
    let m = w.min(h) as f32;
    let a = light_angle.to_radians() as f32;
    let dir = (-a.sin(), a.cos());
    let layer = |sigma: f32, shift: f32| {
        let s = sigma / k as f32;
        let radius = (((4.0 * s * s + 1.0).sqrt() - 1.0) * 0.5).round().max(1.0) as usize;
        (box_blur(&lo, sw, sh, radius), shift)
    };
    let (wide, wide_shift) = layer(0.05 * m, 0.05 * m);
    let (tight, tight_shift) = layer(0.014 * m, 0.016 * m);
    let fade_w = (0.05 * m).max(2.0);
    let rows_per = h.div_ceil(threads.max(1)).max(1);
    std::thread::scope(|s| {
        for (ci, chunk) in data.chunks_mut(rows_per * w * 4).enumerate() {
            let (wide, tight) = (&wide, &tight);
            s.spawn(move || {
                for (r, row) in chunk.chunks_mut(w * 4).enumerate() {
                    let y = ci * rows_per + r;
                    let fy = (y as f32 + 0.5).min(h as f32 - y as f32 - 0.5);
                    for (x, px) in row.chunks_exact_mut(4).enumerate() {
                        if px[3] == 255 {
                            continue;
                        }
                        let fx = (x as f32 + 0.5).min(w as f32 - x as f32 - 0.5);
                        let fade = smoothstep(0.0, fade_w, fx.min(fy));
                        if fade <= 0.0 {
                            continue;
                        }
                        let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                        let sample = |g: &[f32], shift: f32| {
                            bilinear(
                                g,
                                sw,
                                sh,
                                (cx - dir.0 * shift) / k as f32,
                                (cy - dir.1 * shift) / k as f32,
                            )
                        };
                        let s = 0.55 * sample(wide, wide_shift) + 0.45 * sample(tight, tight_shift);
                        let s = (s * strength * fade).clamp(0.0, 1.0);
                        let a = f32::from(px[3]);
                        px[3] = (a + s * (255.0 - a)).round().min(255.0) as u8;
                    }
                }
            });
        }
    });
}
