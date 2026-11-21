//! The video time grid: frames and milliseconds (`docs/wiki/video-editing.md#time`).
//!
//! Time is stored as whole milliseconds, but a video only ever shows frames, so a start or a
//! length that falls between two frames means nothing. Every time-valued input of a video
//! operation is *snapped* to the video's frame grid when the operation is applied, and the
//! operation as recorded (and as the agent reads it back) carries the snapped value.
//!
//! The grid is exact integer math, so nothing drifts however long the video is:
//! frame `n` starts at `round(n * 1000 / fps)` ms, and the frame at or just before a time
//! `ms` is `floor(ms * fps / 1000)`. The renderer asks for frame `n` at [`frame_ms`], so a
//! clip that starts on a grid time shows its first picture on exactly the frame that starts
//! there (flooring would show the previous clip's last picture for one frame at 24 fps).

/// The time frame `frame` starts, in whole milliseconds, rounded to the nearest.
pub fn frame_ms(frame: u64, fps: u32) -> u64 {
    let fps = u64::from(fps.max(1));
    (frame * 1000 + fps / 2) / fps
}

/// The frame showing at `ms`: the last one that starts at or before it.
pub fn frame_at(ms: u64, fps: u32) -> u64 {
    ms * u64::from(fps.max(1)) / 1000
}

/// The frame that is showing at `ms`: the last one whose [`frame_ms`] is at or before it, so
/// `frame_containing(frame_ms(n, fps), fps) == n` for every frame.
pub fn frame_containing(ms: u64, fps: u32) -> u64 {
    let fps = u64::from(fps.max(1));
    let shifted = (ms + 1) * fps;
    (shifted - fps / 2 - 1) / 1000
}

/// The frame boundary nearest `ms` (ties go later).
pub fn snap(ms: u64, fps: u32) -> u64 {
    let fps = u64::from(fps.max(1));
    frame_ms((ms * fps + 500) / 1000, fps as u32)
}

/// The nearest boundary inside `min..=max`, or `None` when no whole frame fits in it.
pub fn snap_within(ms: u64, fps: u32, min: u64, max: u64) -> Option<u64> {
    let fps = u64::from(fps.max(1));
    let first = (min * fps).div_ceil(1000);
    let last = max * fps / 1000;
    let mut frame = (ms * fps + 500) / 1000;
    frame = frame.clamp(first, last.max(first));
    // rounding a boundary to whole milliseconds may land just outside a tight range
    let at = frame_ms(frame, fps as u32);
    (first <= last && (min..=max).contains(&at)).then_some(at)
}

/// [`snap_within`], keeping `ms` itself clamped into the range when no boundary fits
/// (a range narrower than one frame), so the result never leaves `min..=max`.
pub fn snap_or_clamp(ms: u64, fps: u32, min: u64, max: u64) -> u64 {
    snap_within(ms, fps, min, max).unwrap_or_else(|| ms.clamp(min, max.max(min)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATES: [u32; 6] = [1, 24, 25, 29, 30, 60];
    const TEN_MINUTES_MS: u64 = 10 * 60 * 1000;

    #[test]
    fn frames_do_not_drift_over_ten_minutes() {
        for fps in RATES {
            let frames = TEN_MINUTES_MS * u64::from(fps) / 1000;
            let mut last = 0;
            for n in 0..=frames {
                let ms = frame_ms(n, fps);
                let exact = n as f64 * 1000.0 / f64::from(fps);
                assert!((ms as f64 - exact).abs() <= 0.5, "{fps} fps frame {n}");
                assert!(ms >= last, "{fps} fps frame {n} goes backwards");
                last = ms;
            }
            assert_eq!(frame_ms(frames, fps), TEN_MINUTES_MS, "{fps} fps");
        }
    }

    #[test]
    fn frame_containing_inverts_frame_ms() {
        for fps in RATES {
            for n in 0..u64::from(fps) * 5 {
                let start = frame_ms(n, fps);
                assert_eq!(frame_containing(start, fps), n, "{fps} fps frame {n}");
                let next = frame_ms(n + 1, fps);
                assert_eq!(frame_containing(next - 1, fps), n, "{fps} fps frame {n}");
            }
        }
    }

    #[test]
    fn frame_gaps_are_the_floor_or_the_ceiling_of_the_period() {
        for fps in RATES {
            let (lo, hi) = (1000 / u64::from(fps), 1000_u64.div_ceil(u64::from(fps)));
            for n in 0..u64::from(fps) * 5 {
                let gap = frame_ms(n + 1, fps) - frame_ms(n, fps);
                assert!((lo..=hi).contains(&gap), "{fps} fps frame {n}: {gap}");
            }
        }
    }

    #[test]
    fn the_frame_at_a_time_is_the_last_one_started() {
        for fps in RATES {
            for ms in (0..20_000).step_by(7) {
                let n = frame_at(ms, fps);
                assert!(n * 1000 <= ms * u64::from(fps));
                assert!((n + 1) * 1000 > ms * u64::from(fps));
            }
        }
    }

    #[test]
    fn snapping_lands_on_a_frame_and_is_idempotent() {
        for fps in RATES {
            for ms in (0..70_000).step_by(3) {
                let s = snap(ms, fps);
                assert_eq!(snap(s, fps), s, "{fps} fps {ms} ms");
                let half = 500 / u64::from(fps) + 1;
                assert!(s.abs_diff(ms) <= half, "{fps} fps {ms} -> {s}");
            }
        }
        assert_eq!(snap(1000, 24), 1000);
        assert_eq!(snap(1010, 24), 1000);
        assert_eq!(snap(1030, 24), 1042);
        assert_eq!(snap(3000, 30), 3000);
    }

    #[test]
    fn snapping_never_leaves_its_bounds() {
        for fps in RATES {
            for (min, max) in [(100, 60_000), (1, 5_000), (100, 130), (0, 0), (80, 90)] {
                for ms in (0..80_000).step_by(11) {
                    let s = snap_or_clamp(ms, fps, min, max);
                    assert!(
                        (min..=max).contains(&s),
                        "{fps} fps {ms} in {min}..={max}: {s}"
                    );
                    assert_eq!(snap_or_clamp(s, fps, min, max), s, "idempotent");
                    if let Some(grid) = snap_within(ms, fps, min, max) {
                        assert_eq!(snap(grid, fps), grid, "on the grid");
                    }
                }
            }
        }
    }

    #[test]
    fn a_range_with_no_whole_frame_has_no_boundary() {
        assert_eq!(snap_within(50, 24, 1, 30), None);
        assert_eq!(snap_or_clamp(50, 24, 1, 30), 30);
        assert_eq!(snap_within(100, 24, 100, 130), Some(125));
    }

    #[test]
    fn the_minimum_clip_snaps_up_not_below() {
        assert_eq!(snap_within(100, 24, 100, 60_000), Some(125));
        assert_eq!(snap_within(100, 60, 100, 60_000), Some(100));
        assert_eq!(snap_within(100, 1, 100, 60_000), Some(1000));
    }
}
