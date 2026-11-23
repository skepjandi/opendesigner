//! The transitions that cut a mask out of the arriving clip instead of moving or fading it
//! (`docs/wiki/video-editing.md#transitions`): wipes, an iris, a clock wipe and a glitch of slices. Each is a
//! pure function of the transition's progress, the picture's size and a seed, so a frame is the
//! same every time it is drawn.

use od_core::TransitionKind;
use tiny_skia::Pixmap;

/// A mask over the arriving clip at an eased `progress` from 0 (nothing shows) to 1 (all shows).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reveal {
    pub kind: TransitionKind,
    pub progress: f32,
    /// Chooses the slices of a glitch; the clip's position in the video, so it never changes.
    pub seed: u64,
}

impl Reveal {
    /// Whether `kind` is drawn by masking the arriving clip.
    pub fn of(kind: TransitionKind, progress: f32, seed: u64) -> Option<Reveal> {
        matches!(
            kind,
            TransitionKind::WipeLeft
                | TransitionKind::WipeRight
                | TransitionKind::WipeUp
                | TransitionKind::WipeDown
                | TransitionKind::Iris
                | TransitionKind::GlitchSlice
                | TransitionKind::ClockWipe
        )
        .then_some(Reveal {
            kind,
            progress: progress.clamp(0.0, 1.0),
            seed,
        })
    }
}

/// Hides what the reveal has not uncovered yet (the layer is premultiplied).
pub fn reveal(layer: &mut Pixmap, r: Reveal) {
    let (w, h) = (layer.width() as usize, layer.height() as usize);
    match r.kind {
        TransitionKind::WipeLeft => {
            keep_columns(layer, ((w as f32) * (1.0 - r.progress)).round() as usize, w)
        }
        TransitionKind::WipeRight => {
            keep_columns(layer, 0, ((w as f32) * r.progress).round() as usize)
        }
        TransitionKind::WipeUp => {
            keep_rows(layer, ((h as f32) * (1.0 - r.progress)).round() as usize, h)
        }
        TransitionKind::WipeDown => keep_rows(layer, 0, ((h as f32) * r.progress).round() as usize),
        TransitionKind::Iris => iris(layer, r.progress),
        TransitionKind::GlitchSlice => glitch(layer, r),
        TransitionKind::ClockWipe => clock_wipe(layer, r.progress),
        _ => {}
    }
}

fn keep_columns(layer: &mut Pixmap, from: usize, to: usize) {
    let w = layer.width() as usize;
    for row in layer.data_mut().chunks_exact_mut(w * 4) {
        for (x, px) in row.chunks_exact_mut(4).enumerate() {
            if x < from || x >= to {
                px.fill(0);
            }
        }
    }
}

fn keep_rows(layer: &mut Pixmap, from: usize, to: usize) {
    let w = layer.width() as usize;
    for (y, row) in layer.data_mut().chunks_exact_mut(w * 4).enumerate() {
        if y < from || y >= to {
            row.fill(0);
        }
    }
}

/// A circle from the middle that grows until it covers the corners; its edge is one pixel soft.
fn iris(layer: &mut Pixmap, progress: f32) {
    let (w, h) = (layer.width() as usize, layer.height() as usize);
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let radius = progress * (cx * cx + cy * cy).sqrt();
    for (i, px) in layer.data_mut().chunks_exact_mut(4).enumerate() {
        let (x, y) = ((i % w) as f32 + 0.5 - cx, (i / w) as f32 + 0.5 - cy);
        let cover = (radius - (x * x + y * y).sqrt() + 0.5).clamp(0.0, 1.0);
        if cover < 1.0 {
            for c in px {
                *c = (f32::from(*c) * cover).round() as u8;
            }
        }
    }
}

/// A radius from twelve o'clock sweeping clockwise: a pixel shows once the sweep has passed its
/// angle. The edge is about one pixel soft; 0 shows nothing and 1 everything.
fn clock_wipe(layer: &mut Pixmap, progress: f32) {
    let (w, h) = (layer.width() as usize, layer.height() as usize);
    if progress >= 1.0 {
        return;
    }
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    let limit = progress * std::f32::consts::TAU;
    for (i, px) in layer.data_mut().chunks_exact_mut(4).enumerate() {
        let (x, y) = ((i % w) as f32 + 0.5 - cx, (i / w) as f32 + 0.5 - cy);
        let mut angle = x.atan2(-y);
        if angle < 0.0 {
            angle += std::f32::consts::TAU;
        }
        let distance = (x * x + y * y).sqrt().max(1.0);
        let cover = if progress <= 0.0 {
            0.0
        } else {
            ((limit - angle) * distance + 0.5).clamp(0.0, 1.0)
        };
        if cover < 1.0 {
            for c in px {
                *c = (f32::from(*c) * cover).round() as u8;
            }
        }
    }
}

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// A number from 0 up to but not including 1, chosen by the seed, the slice and the step.
fn unit(seed: u64, slice: u64, step: u64) -> f32 {
    let bits = mix(seed ^ mix(slice) ^ mix(step.wrapping_add(0x51)));
    (bits >> 40) as f32 / (1u64 << 24) as f32
}

/// Horizontal slices that appear in a seeded order and slide sideways by seeded amounts that
/// grow towards the middle of the transition and settle to zero at its end.
fn glitch(layer: &mut Pixmap, r: Reveal) {
    let (w, h) = (layer.width() as usize, layer.height() as usize);
    let band = (h / 24).max(4);
    let sway = (std::f32::consts::PI * r.progress).sin() * 0.2 * w as f32;
    let step = (r.progress * 12.0) as u64;
    let source = layer.data().to_vec();
    let out = layer.data_mut();
    for (n, rows) in (0..h).step_by(band).enumerate() {
        let shown = unit(r.seed, n as u64, 0) < r.progress;
        let shift = ((unit(r.seed, n as u64, step + 1) * 2.0 - 1.0) * sway).round() as isize;
        for y in rows..(rows + band).min(h) {
            for x in 0..w {
                let from = x as isize - shift;
                let at = (y * w + x) * 4;
                if shown && (0..w as isize).contains(&from) {
                    let src = (y * w + from as usize) * 4;
                    out[at..at + 4].copy_from_slice(&source[src..src + 4]);
                } else {
                    out[at..at + 4].fill(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32) -> Pixmap {
        let mut p = Pixmap::new(w, h).unwrap();
        p.fill(tiny_skia::Color::from_rgba8(200, 100, 50, 255));
        p
    }

    fn visible(p: &Pixmap, x: u32, y: u32) -> bool {
        p.data()[((y * p.width() + x) * 4 + 3) as usize] == 255
    }

    fn masked(kind: TransitionKind, progress: f32) -> Pixmap {
        let mut p = solid(40, 20);
        reveal(&mut p, Reveal::of(kind, progress, 7).unwrap());
        p
    }

    #[test]
    fn a_wipe_uncovers_from_its_edge_by_the_progress() {
        let left = masked(TransitionKind::WipeLeft, 0.25);
        assert!(!visible(&left, 29, 5) && visible(&left, 30, 5) && visible(&left, 39, 5));
        let right = masked(TransitionKind::WipeRight, 0.25);
        assert!(visible(&right, 9, 5) && !visible(&right, 10, 5));
        let up = masked(TransitionKind::WipeUp, 0.5);
        assert!(!visible(&up, 3, 9) && visible(&up, 3, 10));
        let down = masked(TransitionKind::WipeDown, 0.5);
        assert!(visible(&down, 3, 9) && !visible(&down, 3, 10));
        for kind in [TransitionKind::WipeLeft, TransitionKind::WipeDown] {
            let none = masked(kind, 0.0);
            assert!(none.data().iter().all(|&b| b == 0), "{kind:?}");
            let all = masked(kind, 1.0);
            assert!(
                all.data().chunks_exact(4).all(|p| p == [200, 100, 50, 255]),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn an_iris_opens_from_the_middle_to_the_corners() {
        let small = masked(TransitionKind::Iris, 0.3);
        assert!(visible(&small, 20, 10));
        assert!(!visible(&small, 0, 0) && !visible(&small, 39, 19));
        let full = masked(TransitionKind::Iris, 1.0);
        assert!(visible(&full, 0, 0) && visible(&full, 39, 19));
        let shown = |p: &Pixmap| p.data().chunks_exact(4).filter(|c| c[3] > 0).count();
        assert!(
            shown(&masked(TransitionKind::Iris, 0.2)) < shown(&masked(TransitionKind::Iris, 0.6))
        );
    }

    #[test]
    fn a_glitch_is_seeded_repeatable_and_complete_at_the_end() {
        let a = masked(TransitionKind::GlitchSlice, 0.5);
        let b = masked(TransitionKind::GlitchSlice, 0.5);
        assert_eq!(a.data(), b.data());
        let mut other = solid(40, 20);
        reveal(
            &mut other,
            Reveal::of(TransitionKind::GlitchSlice, 0.5, 8).unwrap(),
        );
        assert_ne!(a.data(), other.data(), "another seed makes another glitch");
        assert!(
            masked(TransitionKind::GlitchSlice, 0.0)
                .data()
                .iter()
                .all(|&b| b == 0)
        );
        let end = masked(TransitionKind::GlitchSlice, 0.999);
        let shown = end.data().chunks_exact(4).filter(|c| c[3] == 255).count();
        assert!(shown >= 40 * 20 * 9 / 10, "{shown}");
        let mid = a.data().chunks_exact(4).filter(|c| c[3] == 255).count();
        assert!(mid > 0 && mid < 40 * 20, "{mid}");
    }

    #[test]
    fn a_clock_wipe_sweeps_clockwise_from_twelve_and_ends_complete() {
        let quarter = masked(TransitionKind::ClockWipe, 0.25);
        assert!(visible(&quarter, 30, 3), "top right shows first");
        assert!(!visible(&quarter, 30, 17), "bottom right waits");
        assert!(!visible(&quarter, 10, 17) && !visible(&quarter, 10, 3));
        let half = masked(TransitionKind::ClockWipe, 0.5);
        assert!(visible(&half, 30, 17) && visible(&half, 30, 3));
        assert!(!visible(&half, 10, 17) && !visible(&half, 10, 3));
        let three = masked(TransitionKind::ClockWipe, 0.75);
        assert!(visible(&three, 10, 17) && !visible(&three, 10, 3));
        assert!(
            masked(TransitionKind::ClockWipe, 0.0)
                .data()
                .iter()
                .all(|&b| b == 0)
        );
        let all = masked(TransitionKind::ClockWipe, 1.0);
        assert!(all.data().chunks_exact(4).all(|p| p == [200, 100, 50, 255]));
        let shown = |p: &Pixmap| p.data().chunks_exact(4).filter(|c| c[3] > 0).count();
        assert!(
            shown(&masked(TransitionKind::ClockWipe, 0.3))
                < shown(&masked(TransitionKind::ClockWipe, 0.6))
        );
    }
    #[test]
    fn only_masking_kinds_make_a_reveal() {
        assert!(Reveal::of(TransitionKind::Fade, 0.5, 0).is_none());
        assert!(Reveal::of(TransitionKind::Iris, 2.0, 0).unwrap().progress == 1.0);
    }
}
