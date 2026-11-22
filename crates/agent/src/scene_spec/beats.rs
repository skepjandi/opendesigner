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
