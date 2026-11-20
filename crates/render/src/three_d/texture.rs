//! The screen picture as a filtered texture: sRGB bytes decoded to linear light, with a mip chain
//! built only when the picture is drawn smaller than it is, so videos do not shimmer.

use std::sync::OnceLock;

use tiny_skia::Pixmap;

use super::math::{V3, v3};

fn lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0.0; 256];
        for (i, v) in t.iter_mut().enumerate() {
            *v = srgb_decode(i as f32 / 255.0);
        }
        t
    })
}

pub fn srgb_decode(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// The encoding curve, sampled: `powf` for every pixel of a big picture is not worth it.
const ENCODE_STEPS: usize = 4096;

fn encode_lut() -> &'static [f32; ENCODE_STEPS + 1] {
    static LUT: OnceLock<[f32; ENCODE_STEPS + 1]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0.0; ENCODE_STEPS + 1];
        for (i, v) in t.iter_mut().enumerate() {
            let l = i as f32 / ENCODE_STEPS as f32;
            *v = if l <= 0.003_130_8 {
                l * 12.92
            } else {
                1.055 * l.powf(1.0 / 2.4) - 0.055
            };
        }
        t
    })
}

pub fn srgb_encode(l: f32) -> f32 {
    let l = l.clamp(0.0, 1.0);
    let x = l * ENCODE_STEPS as f32;
    let i = (x as usize).min(ENCODE_STEPS - 1);
    let f = x - i as f32;
    let t = encode_lut();
    t[i] + (t[i + 1] - t[i]) * f
}

enum Data<'a> {
    Srgb8(&'a [u8]),
    Linear(Vec<f32>),
}

struct Level<'a> {
    w: usize,
    h: usize,
    data: Data<'a>,
}

impl Level<'_> {
    #[inline]
    fn texel(&self, x: usize, y: usize) -> V3 {
        let i = y * self.w + x;
        match &self.data {
            Data::Srgb8(d) => {
                let l = lut();
                let p = &d[i * 4..i * 4 + 3];
                v3(l[p[0] as usize], l[p[1] as usize], l[p[2] as usize])
            }
            Data::Linear(d) => v3(d[i * 3], d[i * 3 + 1], d[i * 3 + 2]),
        }
    }

    fn bilinear(&self, u: f32, v: f32) -> V3 {
        let x = u * self.w as f32 - 0.5;
        let y = v * self.h as f32 - 0.5;
        let (fx, fy) = (x.floor(), y.floor());
        let (tx, ty) = (x - fx, y - fy);
        let clamp = |c: f32, n: usize| (c as i64).clamp(0, n as i64 - 1) as usize;
        let (x0, x1) = (clamp(fx, self.w), clamp(fx + 1.0, self.w));
        let (y0, y1) = (clamp(fy, self.h), clamp(fy + 1.0, self.h));
        let top = self.texel(x0, y0) * (1.0 - tx) + self.texel(x1, y0) * tx;
        let bot = self.texel(x0, y1) * (1.0 - tx) + self.texel(x1, y1) * tx;
        top * (1.0 - ty) + bot * ty
    }

    /// The next mip: 2x2 box averages in linear light, built across `threads`.
    fn half(&self, threads: usize) -> Level<'static> {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut out = vec![0.0f32; w * h * 3];
        let rows_per = h.div_ceil(threads.max(1)).max(1);
        std::thread::scope(|s| {
            for (ci, chunk) in out.chunks_mut(rows_per * w * 3).enumerate() {
                s.spawn(move || {
                    for (r, row) in chunk.chunks_mut(w * 3).enumerate() {
                        let y = ci * rows_per + r;
                        let (y0, y1) = ((2 * y).min(self.h - 1), (2 * y + 1).min(self.h - 1));
                        for (x, px) in row.chunks_mut(3).enumerate() {
                            let (x0, x1) = ((2 * x).min(self.w - 1), (2 * x + 1).min(self.w - 1));
                            let a = (self.texel(x0, y0)
                                + self.texel(x1, y0)
                                + self.texel(x0, y1)
                                + self.texel(x1, y1))
                                * 0.25;
                            px.copy_from_slice(&[a.x, a.y, a.z]);
                        }
                    }
                });
            }
        });
        Level {
            w,
            h,
            data: Data::Linear(out),
        }
    }
}

pub struct Texture<'a> {
    levels: Vec<Level<'a>>,
}

impl<'a> Texture<'a> {
    /// `mips` extra levels below the picture itself (0 when it is never minified).
    pub fn new(pix: &'a Pixmap, mips: usize, threads: usize) -> Texture<'a> {
        let base = Level {
            w: pix.width() as usize,
            h: pix.height() as usize,
            data: Data::Srgb8(pix.data()),
        };
        let mut levels = vec![base];
        for _ in 0..mips {
            let Some(last) = levels.last() else { break };
            if last.w <= 1 && last.h <= 1 {
                break;
            }
            let next = last.half(threads);
            levels.push(next);
        }
        Texture { levels }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.levels[0].w, self.levels[0].h)
    }

    /// Linear RGB (premultiplied over black) at `(u, v)` in `0..1`, v down, at mip `lod`.
    pub fn sample(&self, u: f32, v: f32, lod: f32) -> V3 {
        let last = self.levels.len() - 1;
        let lod = lod.clamp(0.0, last as f32);
        let l0 = lod.floor() as usize;
        let t = lod - l0 as f32;
        let a = self.levels[l0].bilinear(u, v);
        if t < 1e-3 || l0 >= last {
            return a;
        }
        let b = self.levels[l0 + 1].bilinear(u, v);
        a * (1.0 - t) + b * t
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn srgb_round_trips() {
        for i in 0..256 {
            let c = i as f32 / 255.0;
            assert!((srgb_encode(srgb_decode(c)) - c).abs() < 0.002, "byte {i}");
        }
    }

    #[test]
    fn a_flat_picture_stays_flat_through_the_mips() {
        let mut pix = Pixmap::new(8, 8).expect("picture");
        for px in pix.data_mut().chunks_exact_mut(4) {
            px.copy_from_slice(&[200, 100, 50, 255]);
        }
        let t = Texture::new(&pix, 3, 2);
        assert_eq!(t.size(), (8, 8));
        let c = t.sample(0.5, 0.5, 0.0);
        let expected = v3(
            srgb_decode(200.0 / 255.0),
            srgb_decode(100.0 / 255.0),
            srgb_decode(50.0 / 255.0),
        );
        assert!((c - expected).len() < 0.01, "the mipped colour moved");
        let c = t.sample(0.5, 0.5, 3.0);
        assert!(
            (c - expected).len() < 0.01,
            "the small mip changed the colour"
        );
    }
}
