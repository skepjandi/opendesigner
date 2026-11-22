//! Cutting a picture into depth layers: pure pixel work, no files and no document.
//!
//! A picture and a grey plane (a subject mask, or a depth map that is brighter the nearer it is)
//! become `bands` bands, 0 the farthest. Each band becomes one RGBA layer that also carries a
//! little of what lies behind the nearer bands, grown out of its own edge, so that moving the
//! nearer layers in a camera move does not uncover a hole.

use std::collections::VecDeque;

/// A plane of one byte per pixel, the size of the picture.
pub type Plane = Vec<u8>;

/// The grey plane of `data` (straight RGBA, `from` pixels wide and high) at `w` by `h`, nearest
/// neighbour. A picture with any transparent pixel is read by its alpha (a cut-out), else by its
/// luminance (a mask or a depth map).
pub fn grey_plane(data: &[u8], from: (u32, u32), to: (u32, u32), invert: bool) -> Plane {
    let (fw, fh) = (from.0 as usize, from.1 as usize);
    let by_alpha = data.chunks_exact(4).any(|p| p[3] < 255);
    let value = |x: usize, y: usize| -> u8 {
        let p = &data[(y * fw + x) * 4..(y * fw + x) * 4 + 4];
        let v = if by_alpha {
            p[3]
        } else {
            ((u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000) as u8
        };
        if invert { 255 - v } else { v }
    };
    let (w, h) = (to.0 as usize, to.1 as usize);
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let sy = (y * fh / h.max(1)).min(fh - 1);
        for x in 0..w {
            out.push(value((x * fw / w.max(1)).min(fw - 1), sy));
        }
    }
    out
}

/// Two bands from a subject mask: 1 where the mask is at least `threshold`, else 0.
pub fn bands_from_mask(mask: &[u8], threshold: u8) -> Plane {
    mask.iter().map(|&v| u8::from(v >= threshold)).collect()
}

/// `count` bands of equal population from a depth plane, 0 the farthest. Pixels of equal depth
/// share a band, so a flat plane is one band and the others stay empty.
pub fn bands_from_depth(depth: &[u8], count: u8) -> Plane {
    let count = usize::from(count.max(1));
    let total = depth.len().max(1);
    let mut histogram = [0usize; 256];
    for &v in depth {
        histogram[usize::from(v)] += 1;
    }
    let mut band_of = [0u8; 256];
    let mut below = 0usize;
    for (value, &n) in histogram.iter().enumerate() {
        let middle = below * 2 + n;
        band_of[value] = ((middle * count) / (total * 2)).min(count - 1) as u8;
        below += n;
    }
    depth.iter().map(|&v| band_of[usize::from(v)]).collect()
}

/// How many pixels each band holds.
pub fn band_sizes(bands: &[u8], count: u8) -> Vec<usize> {
    let mut sizes = vec![0; usize::from(count)];
    for &b in bands {
        if let Some(n) = sizes.get_mut(usize::from(b)) {
            *n += 1;
        }
    }
    sizes
}

/// How far a layer's own colours are grown behind the nearer bands, and how soft its outer edge is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reach {
    /// Pixels grown under the nearer bands (the lowest layer is always grown to fill the picture).
    pub grow: u32,
    /// Pixels the edge towards farther bands is softened over.
    pub feather: u32,
}

/// Layer `k` of `bands` as straight RGBA: the band's own pixels, the colours grown out of its edge
/// under the nearer bands, and nothing where a farther band shows.
pub fn layer(rgba: &[u8], size: (u32, u32), bands: &[u8], k: u8, reach: Reach) -> Vec<u8> {
    let (w, h) = (size.0 as usize, size.1 as usize);
    let n = w * h;
    let limit = |band: u8| -> u32 {
        if band > k {
            if k == 0 { u32::MAX } else { reach.grow }
        } else {
            reach.feather
        }
    };
    let mut source = vec![usize::MAX; n];
    let mut distance = vec![u32::MAX; n];
    let mut queue = VecDeque::new();
    for i in 0..n {
        if bands[i] == k {
            source[i] = i;
            distance[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let (x, y) = (i % w, i / w);
        let next = distance[i].saturating_add(1);
        let around = [
            (x > 0).then(|| i - 1),
            (x + 1 < w).then(|| i + 1),
            (y > 0).then(|| i - w),
            (y + 1 < h).then(|| i + w),
        ];
        for j in around.into_iter().flatten() {
            if distance[j] == u32::MAX && next <= limit(bands[j]) {
                distance[j] = next;
                source[j] = source[i];
                queue.push_back(j);
            }
        }
    }
    let mut alpha: Vec<u8> = (0..n)
        .map(|i| match (bands[i] >= k, source[i] != usize::MAX) {
            (true, true) => 255,
            _ => 0,
        })
        .collect();
    soften(&mut alpha, w, h, reach.feather);
    let mut out = vec![0u8; n * 4];
    for i in 0..n {
        if source[i] == usize::MAX {
            continue;
        }
        let s = source[i] * 4;
        out[i * 4..i * 4 + 3].copy_from_slice(&rgba[s..s + 3]);
        out[i * 4 + 3] = alpha[i];
    }
    out
}

/// A box blur of `radius` pixels over the alpha, both directions.
fn soften(alpha: &mut [u8], w: usize, h: usize, radius: u32) {
    if radius == 0 {
        return;
    }
    let r = radius as usize;
    let pass = |data: &mut [u8], len: usize, stride: usize, lines: usize, step: usize| {
        let mut line = vec![0u8; len];
        for l in 0..lines {
            let base = l * step;
            for (i, slot) in line.iter_mut().enumerate() {
                let (lo, hi) = (i.saturating_sub(r), (i + r).min(len - 1));
                let sum: u32 = (lo..=hi).map(|j| u32::from(data[base + j * stride])).sum();
                *slot = (sum / (hi - lo + 1) as u32) as u8;
            }
            for (i, v) in line.iter().enumerate() {
                data[base + i * stride] = *v;
            }
        }
    };
    pass(alpha, w, 1, h, w);
    pass(alpha, h, w, w, 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(colours: &[[u8; 3]]) -> Vec<u8> {
        colours
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect()
    }

    #[test]
    fn depth_splits_into_bands_of_equal_population() {
        let depth: Vec<u8> = (0..100).map(|i| (i * 2) as u8).collect();
        let bands = bands_from_depth(&depth, 4);
        assert_eq!(band_sizes(&bands, 4), vec![25, 25, 25, 25]);
        assert!(bands.windows(2).all(|p| p[0] <= p[1]), "farther first");
    }

    #[test]
    fn equal_depths_share_a_band_and_a_flat_plane_is_one_band() {
        let flat = vec![90u8; 40];
        let bands = bands_from_depth(&flat, 3);
        assert_eq!(band_sizes(&bands, 3).iter().filter(|&&n| n > 0).count(), 1);
    }

    #[test]
    fn a_cutout_is_read_by_alpha_and_a_mask_by_luminance() {
        let cut = vec![255, 0, 0, 0, 255, 0, 0, 255];
        assert_eq!(grey_plane(&cut, (2, 1), (2, 1), false), vec![0, 255]);
        let mask = row(&[[0, 0, 0], [255, 255, 255]]);
        assert_eq!(grey_plane(&mask, (2, 1), (2, 1), false), vec![0, 255]);
        assert_eq!(grey_plane(&mask, (2, 1), (2, 1), true), vec![255, 0]);
        assert_eq!(
            grey_plane(&mask, (2, 1), (4, 2), false),
            vec![0, 0, 255, 255, 0, 0, 255, 255]
        );
        assert_eq!(bands_from_mask(&[0, 127, 128, 255], 128), vec![0, 0, 1, 1]);
    }

    #[test]
    fn a_farther_layer_grows_its_colours_under_the_nearer_band() {
        let rgba = row(&[[10, 0, 0], [20, 0, 0], [200, 0, 0], [210, 0, 0]]);
        let bands = vec![0, 0, 1, 1];
        let reach = Reach {
            grow: 8,
            feather: 0,
        };
        let back = layer(&rgba, (4, 1), &bands, 0, reach);
        assert_eq!(
            &back[8..12],
            &[20, 0, 0, 255],
            "grown from the edge, opaque"
        );
        assert_eq!(
            &back[12..16],
            &[20, 0, 0, 255],
            "the lowest layer fills the picture"
        );
        let front = layer(&rgba, (4, 1), &bands, 1, reach);
        assert_eq!(front[3], 0);
        assert_eq!(front[7], 0, "nothing where a farther band shows");
        assert_eq!(&front[8..12], &[200, 0, 0, 255]);
    }

    #[test]
    fn a_middle_layer_grows_only_as_far_as_asked() {
        let rgba = row(&[[1, 0, 0], [2, 0, 0], [3, 0, 0], [4, 0, 0], [5, 0, 0]]);
        let bands = vec![0, 1, 2, 2, 2];
        let near = layer(
            &rgba,
            (5, 1),
            &bands,
            1,
            Reach {
                grow: 1,
                feather: 0,
            },
        );
        assert_eq!(near[2 * 4 + 3], 255);
        assert_eq!(near[3 * 4 + 3], 0, "one pixel of growth, no more");
    }

    #[test]
    fn the_edge_towards_a_farther_band_is_softened() {
        let rgba = row(&[[9, 0, 0]; 6]);
        let bands = vec![0, 0, 0, 1, 1, 1];
        let hard = layer(
            &rgba,
            (6, 1),
            &bands,
            1,
            Reach {
                grow: 0,
                feather: 0,
            },
        );
        let soft = layer(
            &rgba,
            (6, 1),
            &bands,
            1,
            Reach {
                grow: 0,
                feather: 1,
            },
        );
        assert_eq!(hard[2 * 4 + 3], 0);
        assert!(soft[2 * 4 + 3] > 0 && soft[2 * 4 + 3] < 255);
        assert_eq!(soft[5 * 4 + 3], 255, "the inside stays solid");
    }
}
