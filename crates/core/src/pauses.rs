//! Cutting the pauses out of a clip of footage: a clip that plays one stretch of a file becomes
//! several clips that play only the stretches with sound in them, closed up one after the other
//! (`docs/wiki/video-editing.md#pauses-and-waveforms`).
//!
//! The pauses come from the media crate's `silencedetect` (or from a transcript); what is built
//! here is only the clip operations, so the cut is one transaction: attributed, checkpointed and
//! undone in one step. Overlays, captions and sound tracks sit at absolute times and do not move.

use crate::error::OpError;
use crate::model::NodeId;
use crate::ops::Operation;
use crate::video::{
    Clip, ClipProperty, ClipSource, Ease, Footage, MAX_CLIP_MS, MIN_CLIP_MS, Motion, Video,
};

/// A stretch of a media file, in milliseconds from its start.
pub type FileRange = (u64, u64);

/// The result of planning a cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PauseCut {
    /// The stretches of the file that stay, in order.
    pub kept: Vec<FileRange>,
    /// How much shorter the video gets.
    pub removed_ms: u64,
}

/// The stretches of `window` that stay once the `pauses` inside it are taken out, except that
/// `keep_ms` of each pause is left at both of its ends so speech is not clipped. Pauses shorter
/// than `2 * keep_ms` are left whole; stretches shorter than the shortest clip are dropped.
pub fn keep_ranges(window: FileRange, pauses: &[FileRange], keep_ms: u64) -> Vec<FileRange> {
    let mut cuts: Vec<FileRange> = pauses
        .iter()
        .filter(|(s, e)| e > s && e - s > 2 * keep_ms)
        .map(|(s, e)| ((s + keep_ms).max(window.0), (e - keep_ms).min(window.1)))
        .filter(|(s, e)| e > s)
        .collect();
    cuts.sort_unstable();
    let mut kept = Vec::new();
    let mut from = window.0;
    for (s, e) in cuts {
        if s > from {
            kept.push((from, s));
        }
        from = from.max(e);
    }
    if from < window.1 {
        kept.push((from, window.1));
    }
    kept.retain(|(s, e)| e - s >= u64::from(MIN_CLIP_MS));
    kept
}

fn bad(detail: String) -> OpError {
    OpError::OutOfBounds {
        what: "pause cut",
        detail,
    }
}

impl Video {
    /// The operations that replace footage clip `clip` by the stretches of it that are not
    /// `pauses` (in the file's own time, as the media crate reports them). Apply them as one
    /// transaction.
    pub fn pause_cut_ops(
        &self,
        clip: &NodeId,
        pauses: &[FileRange],
        keep_ms: u64,
    ) -> Result<(Vec<Operation>, PauseCut), OpError> {
        let at = self
            .clip(clip)
            .ok_or_else(|| OpError::NoSuchClip(clip.to_string()))?;
        let original = &self.clips[at];
        let footage = original
            .footage
            .as_ref()
            .ok_or_else(|| bad(format!("clip {clip} shows a design frame, not footage")))?;
        let window = (
            footage.in_ms,
            footage.in_ms + u64::from(original.duration_ms),
        );
        let kept = keep_ranges(window, pauses, keep_ms);
        if kept.is_empty() {
            return Err(bad(format!(
                "nothing of clip {clip} is left after its pauses"
            )));
        }
        if kept.len() == 1 && kept[0] == window {
            return Err(bad(format!("clip {clip} has no pause to cut")));
        }
        if self.clips.len() + kept.len() - 1 > crate::video::MAX_CLIPS {
            return Err(OpError::Limit(format!(
                "cutting would make more than {} clips",
                crate::video::MAX_CLIPS
            )));
        }
        let removed_ms =
            u64::from(original.duration_ms) - kept.iter().map(|(s, e)| e - s).sum::<u64>();
        let ops = cut_ops(&self.id, original, &footage.file, at, &kept);
        Ok((ops, PauseCut { kept, removed_ms }))
    }
}

/// The motion of the piece starting `done` of `total` ms into the clip: the zoom carries on
/// across the cuts instead of starting over in each piece.
fn motion_for(m: Motion, done: u64, len: u64, total: u64) -> Motion {
    let at = |ms: u64| m.scale_from + (m.scale_to - m.scale_from) * ms as f64 / total as f64;
    Motion {
        scale_from: at(done),
        scale_to: at(done + len),
        ease: Ease::Linear,
    }
}

fn cut_ops(
    video: &NodeId,
    original: &Clip,
    file: &str,
    at: usize,
    kept: &[FileRange],
) -> Vec<Operation> {
    let zooms = (original.motion.scale_to - original.motion.scale_from).abs() > f64::EPSILON;
    let total: u64 = kept.iter().map(|(s, e)| e - s).sum();
    let footage = |from: u64| Footage {
        file: file.to_string(),
        in_ms: from,
    };
    let mut ops = Vec::new();
    let mut done = 0;
    for (n, &(from, to)) in kept.iter().enumerate() {
        let len = to - from;
        let motion = zooms.then(|| motion_for(original.motion, done, len, total));
        if n == 0 {
            let set = |prop| Operation::SetClip {
                video: video.clone(),
                clip: original.id.clone(),
                prop,
            };
            ops.push(set(ClipProperty::DurationMs(len as u32)));
            ops.push(set(ClipProperty::Source(ClipSource {
                frame: None,
                footage: Some(footage(from)),
            })));
            if let Some(m) = motion {
                ops.push(set(ClipProperty::Motion(m)));
            }
        } else {
            ops.push(Operation::AddClip {
                video: video.clone(),
                frame: None,
                footage: Some(footage(from)),
                duration_ms: len.min(u64::from(MAX_CLIP_MS)) as u32,
                index: Some(at + n),
                transition: None,
                motion,
            });
        }
        done += len;
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pauses_are_taken_out_leaving_a_margin_at_both_ends() {
        let kept = keep_ranges((0, 10_000), &[(2_000, 4_000), (7_000, 7_300)], 200);
        assert_eq!(kept, [(0, 2_200), (3_800, 10_000)]);
        let kept = keep_ranges((0, 10_000), &[(2_000, 4_000), (7_000, 8_000)], 0);
        assert_eq!(kept, [(0, 2_000), (4_000, 7_000), (8_000, 10_000)]);
    }

    #[test]
    fn pauses_are_clipped_to_the_window_merged_and_tiny_leftovers_dropped() {
        assert_eq!(
            keep_ranges((1_000, 5_000), &[(0, 1_500), (4_500, 9_000)], 0),
            [(1_500, 4_500)]
        );
        assert_eq!(
            keep_ranges((0, 5_000), &[(1_000, 3_000), (2_000, 4_000)], 0),
            [(0, 1_000), (4_000, 5_000)]
        );
        assert_eq!(
            keep_ranges((0, 5_000), &[(50, 4_950)], 0),
            Vec::<FileRange>::new()
        );
        assert_eq!(keep_ranges((0, 5_000), &[], 0), [(0, 5_000)]);
    }
}
