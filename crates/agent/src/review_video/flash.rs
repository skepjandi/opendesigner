//! RV013, flash safety: a picture that changes brightness hard and fast can trigger seizures in
//! people with photosensitive epilepsy (the rule of thumb is no more than three flashes in any
//! second; here no more than three large changes). The whole video is drawn small at 15 pictures
//! a second, which is fine enough to see a change as brief as a tenth of a second.

use od_core::Video;
use od_render::Pixmap;
use od_render::video::VideoRenderer;

use super::{Draft, Source};
use crate::tools::ToolError;

/// Pictures a second: a flash of 100 ms spans at least one.
const SCAN_FPS: u64 = 15;
/// The longest side of a scanned picture.
const SCAN_SIDE: f64 = 48.0;
/// Most pictures drawn; a longer video is scanned from its start up to this.
const MAX_SCANNED: u64 = 1_800;
/// A change of mean relative luminance this large is a flash (WCAG: 10% of the range).
const LARGE_CHANGE: f64 = 0.1;
/// A change between two pictures this small does not decide which way the brightness goes.
const NOISE: f64 = 0.004;
/// The picture must not be brighter than this on its darker side (WCAG: 0.8).
const BRIGHT_LIMIT: f64 = 0.8;
const WINDOW_MS: u64 = 1_000;
const MAX_CHANGES_PER_WINDOW: usize = 3;

/// What scanning the video found.
#[derive(Debug, Clone, PartialEq)]
pub struct FlashScan {
    /// When each large change of brightness ended, in order.
    pub changes: Vec<u64>,
    /// How much of the video was scanned.
    pub scanned_ms: u64,
}

impl FlashScan {
    /// The most large changes in any second, and where that second starts.
    pub fn worst_window(&self) -> (usize, u64) {
        let mut worst = (0, 0);
        for (i, start) in self.changes.iter().enumerate() {
            let count = self.changes[i..]
                .iter()
                .take_while(|t| **t - start <= WINDOW_MS)
                .count();
            if count > worst.0 {
                worst = (count, *start);
            }
        }
        worst
    }
}

fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Mean relative luminance of the picture, 0 (black) to 1 (white).
fn brightness(p: &Pixmap) -> f64 {
    let pixels = p.data().chunks_exact(4);
    let n = pixels.len().max(1) as f64;
    pixels
        .map(|px| 0.2126 * linear(px[0]) + 0.7152 * linear(px[1]) + 0.0722 * linear(px[2]))
        .sum::<f64>()
        / n
}

/// The moments where a run of brightening or darkening ended having moved the brightness by a
/// large step, from `(time, brightness)` pairs. A fade is one change, not one per picture.
pub fn large_changes(levels: &[(u64, f64)]) -> Vec<u64> {
    let mut out = Vec::new();
    let Some(&(_, first)) = levels.first() else {
        return out;
    };
    let (mut anchor, mut last, mut rising) = (first, first, None::<bool>);
    let mut close = |from: f64, to: f64, at: u64| {
        if (to - from).abs() >= LARGE_CHANGE && from.min(to) < BRIGHT_LIMIT {
            out.push(at);
        }
    };
    let mut last_at = levels[0].0;
    for &(at, level) in &levels[1..] {
        let step = level - last;
        if step.abs() > NOISE {
            let now_rising = step > 0.0;
            if rising.is_some_and(|r| r != now_rising) {
                close(anchor, last, last_at);
                anchor = last;
            }
            rising = Some(now_rising);
            (last, last_at) = (level, at);
        }
    }
    close(anchor, last, last_at);
    out
}

/// Draws the video small, 15 times a second, and finds its large changes of brightness. `None`
/// for footage, which needs a decoder, and for a video too short to hold a second.
pub fn scan(src: &Source, v: &Video) -> Result<Option<FlashScan>, ToolError> {
    let duration = v.duration_ms();
    if duration < WINDOW_MS || crate::video_export::has_footage(&src.doc, v) {
        return Ok(None);
    }
    let shrink = SCAN_SIDE / f64::from(v.width.max(v.height));
    let (w, h) = (
        (f64::from(v.width) * shrink).round().max(1.0) as u32,
        (f64::from(v.height) * shrink).round().max(1.0) as u32,
    );
    let mut renderer = VideoRenderer::new(&src.doc, v, src.dir()).with_size(w, h);
    let count = (duration * SCAN_FPS / 1000).min(MAX_SCANNED);
    let mut levels = Vec::with_capacity(count as usize);
    for k in 0..count {
        let at = k * 1000 / SCAN_FPS;
        let picture = renderer
            .frame_at(at)
            .map_err(|e| ToolError(e.to_string()))?;
        levels.push((at, brightness(&picture)));
    }
    Ok(Some(FlashScan {
        changes: large_changes(&levels),
        scanned_ms: count * 1000 / SCAN_FPS,
    }))
}

pub(super) fn draft(scan: &FlashScan) -> Option<Draft> {
    let (count, at) = scan.worst_window();
    (count > MAX_CHANGES_PER_WINDOW).then(|| Draft {
        code: "RV013",
        name: "flash-safety",
        scenes: Vec::new(),
        message: format!(
            "{count} large brightness changes within one second from {:.1} s: this can trigger seizures in people with photosensitive epilepsy",
            at as f64 / 1000.0
        ),
        fix: "Slow the flicker: no more than three large light/dark changes in any second. Lengthen strobing cuts and flashes, fade instead of flashing, lower the contrast of the flashing area, avoid full-frame white flashes.",
        weight: 40.0,
    })
}
