//! Puts the time-valued inputs of video operations on the video's frame grid
//! (`crate::timebase`, `docs/wiki/video-editing.md#time`).
//!
//! The operation that is recorded (and so redone, replayed and shown to the agent) is the
//! snapped one. Operations the history issues itself (undo, redo, restores) are never snapped,
//! so undoing an edit puts back exactly the numbers a project was loaded with.

use crate::animation::Key;
use crate::model::NodeId;
use crate::ops::Operation;
use crate::speed::SpeedKey;
use crate::timebase::snap_within;
use crate::video::{
    ClipProperty, ClipSource, Footage, MAX_CLIP_MS, MAX_IN_MS, MAX_TRANSITION_MS, MAX_VIDEO_MS,
    MIN_CLIP_MS, OverlayProperty, Transition, TransitionKind,
};
use crate::{Document, Video};

/// `ms` on the grid inside `min..=max`. A value outside the range is left as it is, so the
/// operation's own checks refuse it with the rule it broke instead of it being quietly clamped.
fn within(ms: u64, fps: u32, min: u64, max: u64) -> u64 {
    if (min..=max).contains(&ms) {
        snap_within(ms, fps, min, max).unwrap_or(ms)
    } else {
        ms
    }
}

fn duration(ms: u32, fps: u32) -> u32 {
    within(
        u64::from(ms),
        fps,
        u64::from(MIN_CLIP_MS),
        u64::from(MAX_CLIP_MS),
    ) as u32
}

fn transition(t: Transition, clip_ms: u32, fps: u32) -> Transition {
    if t.kind == TransitionKind::Cut || t.duration_ms == 0 {
        return t;
    }
    let max = u64::from(MAX_TRANSITION_MS.min(clip_ms));
    Transition {
        duration_ms: within(u64::from(t.duration_ms), fps, 1, max.max(1)) as u32,
        ..t
    }
}

fn footage(f: &Footage, fps: u32) -> Footage {
    Footage {
        in_ms: within(f.in_ms, fps, 0, MAX_IN_MS),
        file: f.file.clone(),
    }
}

fn source(s: &ClipSource, fps: u32) -> ClipSource {
    ClipSource {
        frame: s.frame.clone(),
        footage: s.footage.as_ref().map(|f| footage(f, fps)),
    }
}

fn ramp(keys: &[SpeedKey], fps: u32) -> Vec<SpeedKey> {
    keys.iter()
        .map(|k| SpeedKey {
            at_ms: within(u64::from(k.at_ms), fps, 0, u64::from(MAX_CLIP_MS)) as u32,
            ..*k
        })
        .collect()
}

fn fade(ms: u32, duration_ms: u32, fps: u32) -> u32 {
    within(u64::from(ms), fps, 0, u64::from(duration_ms / 2)) as u32
}

/// The keys of an animation on the video's grid.
pub(crate) fn keys(keys: &[Key], fps: u32) -> Vec<Key> {
    keys.iter()
        .map(|k| Key {
            at_ms: within(k.at_ms, fps, 0, MAX_VIDEO_MS),
            ..*k
        })
        .collect()
}

fn clip_duration(video: &Video, clip: &NodeId) -> u32 {
    video
        .clip(clip)
        .map_or(MIN_CLIP_MS, |i| video.clips[i].duration_ms)
}

fn overlay_duration(video: &Video, overlay: &NodeId) -> u32 {
    video
        .overlays
        .iter()
        .find(|o| &o.id == overlay)
        .map_or(MIN_CLIP_MS, |o| o.duration_ms)
}

fn clip_prop(video: &Video, clip: &NodeId, prop: &ClipProperty) -> ClipProperty {
    let fps = video.fps;
    match prop {
        ClipProperty::DurationMs(ms) => ClipProperty::DurationMs(duration(*ms, fps)),
        ClipProperty::Source(s) => ClipProperty::Source(source(s, fps)),
        ClipProperty::Transition(t) => {
            ClipProperty::Transition(transition(*t, clip_duration(video, clip), fps))
        }
        ClipProperty::SpeedRamp(keys) => ClipProperty::SpeedRamp(ramp(keys, fps)),
        other => other.clone(),
    }
}

fn overlay_prop(video: &Video, overlay: &NodeId, prop: &OverlayProperty) -> OverlayProperty {
    let fps = video.fps;
    match prop {
        OverlayProperty::StartMs(ms) => OverlayProperty::StartMs(within(*ms, fps, 0, MAX_VIDEO_MS)),
        OverlayProperty::DurationMs(ms) => OverlayProperty::DurationMs(duration(*ms, fps)),
        OverlayProperty::FadeMs(ms) => {
            OverlayProperty::FadeMs(fade(*ms, overlay_duration(video, overlay), fps))
        }
        other => other.clone(),
    }
}

/// `op` with its times on the grid of the video it edits; anything else (or an operation on a
/// video that does not exist, which will fail on its own) comes back unchanged.
pub(crate) fn video_op(doc: &Document, op: &Operation) -> Operation {
    let mut op = op.clone();
    match &mut op {
        Operation::AddClip {
            video,
            footage: f,
            duration_ms,
            transition: t,
            ..
        } => {
            if let Some(v) = doc.video(video) {
                *duration_ms = duration(*duration_ms, v.fps);
                *f = f.as_ref().map(|f| footage(f, v.fps));
                *t = t.map(|t| transition(t, *duration_ms, v.fps));
            }
        }
        Operation::SetClip { video, clip, prop } => {
            if let Some(v) = doc.video(video) {
                *prop = clip_prop(v, clip, prop);
            }
        }
        Operation::AddOverlay {
            video,
            start_ms,
            duration_ms,
            fade_ms,
            ..
        } => {
            if let Some(v) = doc.video(video) {
                *start_ms = within(*start_ms, v.fps, 0, MAX_VIDEO_MS);
                *duration_ms = duration(*duration_ms, v.fps);
                *fade_ms = fade_ms.map(|f| fade(f, *duration_ms, v.fps));
            }
        }
        Operation::SetOverlay {
            video,
            overlay,
            prop,
        } => {
            if let Some(v) = doc.video(video) {
                *prop = overlay_prop(v, overlay, prop);
            }
        }
        _ => {}
    }
    op
}
