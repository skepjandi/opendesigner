//! The beat map: named moments in time, so a sound cue or a build step can say "on the reveal"
//! instead of a number (`docs/wiki/scene-spec.md#beat-map`).

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::schema::{Beat, SceneSpec};

/// The longest a video may go without a beat.
pub const MAX_GAP_MS: u64 = 8_000;

/// The name a beat answers to: its `id`, or `beat-<n>` (from 1) when it has none.
pub fn beat_name(beat: &Beat, index: usize) -> String {
    if beat.id.trim().is_empty() {
        format!("beat-{}", index + 1)
    } else {
        beat.id.clone()
    }
}

/// Every named beat and the millisecond it lands on.
pub fn beat_times(spec: &SceneSpec) -> BTreeMap<String, u64> {
    spec.beat_map
        .iter()
        .enumerate()
        .map(|(i, b)| (beat_name(b, i), b.at_ms))
        .collect()
}

/// One ready-to-call `make_sound` per beat that names a cue, so each moment on screen gets its own
/// sound: the kind is normalised to one `make_sound` knows (`swoosh` becomes `swoosh_up`), the seed
/// is the beat's number and the pitch walks up the scale across the video, so two beats of the same
/// kind never share a file. The cue is only a starting point: when the beat's change is a number
/// counting, a slide or an object landing, pick the kind that matches it.
pub fn sound_plan(spec: &SceneSpec) -> Vec<Value> {
    const STEPS: [f64; 5] = [1.0, 1.122, 1.26, 1.335, 1.498];
    spec.beat_map
        .iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let cue = b.audio_cue.as_deref()?.trim();
            if cue.is_empty() {
                return None;
            }
            let kind = match cue {
                "swoosh" => "swoosh_up",
                other => other,
            };
            let name = beat_name(b, i);
            let mut call = json!({
                "kind": kind,
                "seed": i as u64 + 1,
                "name": format!("{name}-{kind}"),
            });
            if matches!(kind, "chime" | "pop" | "tick" | "blip" | "stinger") {
                let base = match kind {
                    "chime" => 880.0,
                    "stinger" => 392.0,
                    "tick" => 1_400.0,
                    _ => 700.0,
                };
                call["pitch_hz"] = json!((base * STEPS[i % STEPS.len()]).round());
            }
            Some(json!({
                "beat": name,
                "at_ms": b.at_ms,
                "shows": b.transformation,
                "make_sound": call,
            }))
        })
        .collect()
}

/// The first beat that is not on the timeline yet, with what to build for it.
pub fn next_unbuilt(spec: &SceneSpec) -> Option<Value> {
    let (i, b) = spec.beat_map.iter().enumerate().find(|(_, b)| !b.built)?;
    Some(json!({
        "beat": beat_name(b, i),
        "at_ms": b.at_ms,
        "takeaway": b.takeaway,
        "focal_node": b.focal_node,
        "transformation": b.transformation,
        "copy": b.copy,
        "audio_cue": b.audio_cue,
        "exit_ms": b.exit_ms,
        "remaining": spec.beat_map.iter().filter(|b| !b.built).count(),
    }))
}
