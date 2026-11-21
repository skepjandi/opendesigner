//! `camera_burst`: a camera kick that is fastest on its first frame and dies away, then holds.

use super::{AnimProp, CameraPose, Ease, Side, Spec, Track, key, once};

/// Frames over which the kick decays.
const BURST_FRAMES: u64 = 9;

/// A pan along `side` that covers `amount` (an eighth to a sixth of the stage by `intensity`)
/// with `power4_out`, so the camera's speed peaks at the first frame and fades over about nine
/// frames; the camera then holds at its new place until the move's end.
pub(super) fn burst(spec: &Spec, cam: &CameraPose, side: Side) -> Vec<Track> {
    let span = if side.horizontal() {
        cam.stage_w
    } else {
        cam.stage_h
    };
    let by = spec.amount.unwrap_or(span / 6.0 * spec.intensity) * side.sign();
    let (prop, from) = if side.horizontal() {
        (AnimProp::CameraX, cam.x)
    } else {
        (AnimProp::CameraY, cam.y)
    };
    let frame_ms = spec.gap_ms.saturating_sub(2).max(1);
    let settled = (spec.start_ms + BURST_FRAMES * frame_ms).min(spec.end_ms());
    vec![once(
        spec,
        prop,
        vec![
            key(spec.start_ms, from, Ease::Linear),
            key(settled, from + by, Ease::Power4Out),
            key(spec.end_ms(), from + by, Ease::Linear),
        ],
    )]
}
