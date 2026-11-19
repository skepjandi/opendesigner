//! Videos: a timeline of clips, each showing a design frame for a while, with a transition into it
//! and a slow zoom (`docs/wiki/video-editing.md`).
//!
//! A video is data like everything else. It changes only through operations (so it has history,
//! attribution and checkpoints), and `render` and the exporter turn it into pictures and a file.
//! Time is whole milliseconds, never floating-point seconds, so edits do not drift.

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::{self, finite_in, text_within};
use crate::model::{Document, NodeId, NodeKind};

pub const MAX_VIDEOS: usize = 100;
pub const MAX_CLIPS: usize = 500;
pub const MIN_CLIP_MS: u32 = 100;
pub const MAX_CLIP_MS: u32 = 60_000;
pub const MAX_TRANSITION_MS: u32 = 5_000;
pub const MAX_VIDEO_MS: u64 = 10 * 60 * 1000;
pub const MAX_FPS: u32 = 60;
/// Output sides are even (the common video codecs need it) and at most this many pixels.
pub const MAX_VIDEO_SIDE: u32 = 4_096;
pub const MIN_VIDEO_SIDE: u32 = 16;
pub const MAX_ZOOM: f64 = 3.0;

/// How one clip arrives from the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// The picture changes at once.
    #[default]
    Cut,
    /// The new clip fades in over the old one.
    Fade,
    /// The new clip slides in from the right, pushing the old one out to the left.
    SlideLeft,
    /// The new clip slides in from the left, pushing the old one out to the right.
    SlideRight,
}

/// The transition into a clip. It overlaps the end of the previous clip, so a 500 ms fade makes
/// the video 500 ms shorter than the sum of its clips. The first clip has nothing to arrive from,
/// so its transition is kept but not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    #[serde(default)]
    pub kind: TransitionKind,
    /// Zero for a cut; otherwise 1 to 5000 and no longer than the clip.
    #[serde(default)]
    pub duration_ms: u32,
}

/// A slow zoom over the clip: the picture starts at `scale_from` times the frame and ends at
/// `scale_to`, centred. Both are 1 to 3; 1 and 1 is no motion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub scale_from: f64,
    pub scale_to: f64,
}

impl Default for Motion {
    fn default() -> Motion {
        Motion {
            scale_from: 1.0,
            scale_to: 1.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub id: NodeId,
    /// The design frame this clip shows.
    pub frame: NodeId,
    pub duration_ms: u32,
    #[serde(default)]
    pub transition: Transition,
    #[serde(default)]
    pub motion: Motion,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Video {
    pub id: NodeId,
    pub name: String,
    pub fps: u32,
    /// The output size in pixels. Each frame is scaled to cover it and cropped in the middle.
    pub width: u32,
    pub height: u32,
    pub clips: Vec<Clip>,
}

/// Where a clip sits on the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    /// How long the transition into this clip really lasts (clamped to both neighbours).
    pub overlap_ms: u32,
}

impl Video {
    /// Start, end and effective transition of every clip.
    pub fn placed(&self) -> Vec<Placed> {
        let mut out: Vec<Placed> = Vec::with_capacity(self.clips.len());
        for (index, clip) in self.clips.iter().enumerate() {
            let (start_ms, overlap_ms) = match out.last() {
                None => (0, 0),
                Some(prev) => {
                    let prev_len = prev.end_ms - prev.start_ms;
                    let overlap = if clip.transition.kind == TransitionKind::Cut {
                        0
                    } else {
                        u64::from(clip.transition.duration_ms)
                            .min(prev_len)
                            .min(u64::from(clip.duration_ms))
                    };
                    (prev.end_ms - overlap, overlap as u32)
                }
            };
            out.push(Placed {
                index,
                start_ms,
                end_ms: start_ms + u64::from(clip.duration_ms),
                overlap_ms,
            });
        }
        out
    }

    pub fn duration_ms(&self) -> u64 {
        self.placed().last().map_or(0, |p| p.end_ms)
    }

    /// Number of frames in the video at its frame rate (rounded up, at least one when it has clips).
    pub fn frame_count(&self) -> u64 {
        (self.duration_ms() * u64::from(self.fps)).div_ceil(1000)
    }

    pub fn clip(&self, id: &NodeId) -> Option<usize> {
        self.clips.iter().position(|c| &c.id == id)
    }

    /// Field and structure checks that need no document.
    fn check_fields(&self) -> Result<(), OpError> {
        text_within("video name", &self.name, limits::MAX_NAME_LEN)?;
        check_fps(self.fps)?;
        check_size(self.width, self.height)?;
        if self.clips.len() > MAX_CLIPS {
            return Err(OpError::Limit(format!(
                "at most {MAX_CLIPS} clips per video"
            )));
        }
        for clip in &self.clips {
            check_duration(clip.duration_ms)?;
            check_transition(&clip.transition, clip.duration_ms)?;
            check_motion(&clip.motion)?;
        }
        if self.duration_ms() > MAX_VIDEO_MS {
            return Err(OpError::Limit(format!(
                "a video is at most {} minutes long",
                MAX_VIDEO_MS / 60_000
            )));
        }
        Ok(())
    }
}

pub fn check_fps(fps: u32) -> Result<(), OpError> {
    if (1..=MAX_FPS).contains(&fps) {
        Ok(())
    } else {
        Err(OpError::OutOfBounds {
            what: "fps",
            detail: format!("{fps} is not a whole number from 1 to {MAX_FPS}"),
        })
    }
}

pub fn check_size(width: u32, height: u32) -> Result<(), OpError> {
    for (what, v) in [("video width", width), ("video height", height)] {
        if !(MIN_VIDEO_SIDE..=MAX_VIDEO_SIDE).contains(&v) || v % 2 != 0 {
            return Err(OpError::OutOfBounds {
                what,
                detail: format!(
                    "{v} is not an even number from {MIN_VIDEO_SIDE} to {MAX_VIDEO_SIDE}"
                ),
            });
        }
    }
    Ok(())
}

pub fn check_duration(ms: u32) -> Result<(), OpError> {
    if (MIN_CLIP_MS..=MAX_CLIP_MS).contains(&ms) {
        Ok(())
    } else {
        Err(OpError::OutOfBounds {
            what: "clip duration_ms",
            detail: format!("{ms} is not from {MIN_CLIP_MS} to {MAX_CLIP_MS} milliseconds"),
        })
    }
}

pub fn check_transition(t: &Transition, clip_ms: u32) -> Result<(), OpError> {
    let ok = match t.kind {
        TransitionKind::Cut => t.duration_ms == 0,
        _ => (1..=MAX_TRANSITION_MS.min(clip_ms)).contains(&t.duration_ms),
    };
    if ok {
        Ok(())
    } else {
        Err(OpError::OutOfBounds {
            what: "transition",
            detail: format!(
                "a cut has no duration; any other transition lasts 1 to {} ms and no longer than its clip ({clip_ms} ms), not {} ms",
                MAX_TRANSITION_MS.min(clip_ms),
                t.duration_ms
            ),
        })
    }
}

pub fn check_motion(m: &Motion) -> Result<(), OpError> {
    finite_in("scale_from", m.scale_from, 1.0, MAX_ZOOM)?;
    finite_in("scale_to", m.scale_to, 1.0, MAX_ZOOM)?;
    Ok(())
}

/// One settable property of a video. Serialized as `{"name": "fps", "value": 30}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum VideoProperty {
    Name(String),
    Fps(u32),
    Size { width: u32, height: u32 },
}

/// One settable property of a clip. Serialized as `{"name": "duration_ms", "value": 3000}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum ClipProperty {
    DurationMs(u32),
    Frame(NodeId),
    Transition(Transition),
    Motion(Motion),
}

impl Document {
    pub fn video(&self, id: &NodeId) -> Option<&Video> {
        self.videos.iter().find(|v| &v.id == id)
    }

    fn video_index(&self, id: &NodeId) -> Result<usize, OpError> {
        self.videos
            .iter()
            .position(|v| &v.id == id)
            .ok_or_else(|| OpError::NoSuchVideo(id.to_string()))
    }

    /// The video and clip that show `frame` or one of the frames inside `ids`, if any.
    pub fn video_using(&self, ids: &[NodeId]) -> Option<(&Video, &Clip)> {
        self.videos.iter().find_map(|v| {
            v.clips
                .iter()
                .find(|c| ids.contains(&c.frame))
                .map(|c| (v, c))
        })
    }

    fn check_frame(&self, frame: &NodeId) -> Result<(), OpError> {
        match self.node(frame) {
            None => Err(OpError::NoSuchNode(frame.to_string())),
            Some(n) if n.kind != NodeKind::Frame => Err(OpError::NotApplicable {
                property: "clip frame",
                kind: n.kind.as_str(),
            }),
            Some(_) => Ok(()),
        }
    }

    /// Every video holds its limits and shows only frames that exist.
    pub(crate) fn validate_videos(&self) -> Result<(), OpError> {
        if self.videos.len() > MAX_VIDEOS {
            return Err(OpError::Limit(format!("at most {MAX_VIDEOS} videos")));
        }
        let mut seen = std::collections::HashSet::new();
        for video in &self.videos {
            if !seen.insert(video.id.clone()) {
                return Err(OpError::DuplicateId(video.id.to_string()));
            }
            video.check_fields()?;
            self.check_counter_of(&video.id)?;
            for clip in &video.clips {
                if !seen.insert(clip.id.clone()) {
                    return Err(OpError::DuplicateId(clip.id.to_string()));
                }
                self.check_counter_of(&clip.id)?;
                self.check_frame(&clip.frame)?;
            }
        }
        Ok(())
    }
}

fn video_touched(video: &NodeId, clip: Option<&NodeId>) -> Vec<NodeId> {
    let mut t = vec![video.clone()];
    t.extend(clip.cloned());
    t
}

fn done(
    forward: super::Operation,
    inverse: super::Operation,
    touched: Vec<NodeId>,
) -> super::AppliedOp {
    super::AppliedOp {
        forward,
        inverse,
        touched,
    }
}

/// Applies one video operation. `allow_internal` lets the history issue the `Restore*` ones.
pub(crate) fn apply(
    doc: &mut Document,
    op: &super::Operation,
    allow_internal: bool,
) -> Result<super::AppliedOp, OpError> {
    use super::Operation as O;
    match op {
        O::CreateVideo {
            name,
            fps,
            width,
            height,
        } => {
            if doc.videos.len() >= MAX_VIDEOS {
                return Err(OpError::Limit(format!("at most {MAX_VIDEOS} videos")));
            }
            let video = Video {
                id: doc.alloc_id('v'),
                name: name.clone(),
                fps: *fps,
                width: *width,
                height: *height,
                clips: Vec::new(),
            };
            video.check_fields()?;
            let id = video.id.clone();
            doc.videos.push(video.clone());
            Ok(done(
                O::RestoreVideo { video },
                O::DeleteVideo { id: id.clone() },
                video_touched(&id, None),
            ))
        }
        O::RestoreVideo { video } => {
            if !allow_internal {
                return Err(OpError::InternalOnly("restore_video"));
            }
            if doc.videos.len() >= MAX_VIDEOS {
                return Err(OpError::Limit(format!("at most {MAX_VIDEOS} videos")));
            }
            video.check_fields()?;
            if doc.videos.iter().any(|v| v.id == video.id) {
                return Err(OpError::DuplicateId(video.id.to_string()));
            }
            for clip in &video.clips {
                doc.check_frame(&clip.frame)?;
            }
            doc.videos.push(video.clone());
            Ok(done(
                op.clone(),
                O::DeleteVideo {
                    id: video.id.clone(),
                },
                video_touched(&video.id, None),
            ))
        }
        O::DeleteVideo { id } => {
            let at = doc.video_index(id)?;
            let video = doc.videos.remove(at);
            let at_clip = video.clips.first().map(|c| c.id.clone());
            Ok(done(
                op.clone(),
                O::RestoreVideo { video },
                video_touched(id, at_clip.as_ref()),
            ))
        }
        O::SetVideo { id, prop } => {
            let at = doc.video_index(id)?;
            let video = &mut doc.videos[at];
            let before = video.clone();
            let (old, new) = match prop {
                VideoProperty::Name(name) => {
                    let old = std::mem::replace(&mut video.name, name.clone());
                    (VideoProperty::Name(old), prop.clone())
                }
                VideoProperty::Fps(fps) => {
                    check_fps(*fps)?;
                    let old = std::mem::replace(&mut video.fps, *fps);
                    (VideoProperty::Fps(old), prop.clone())
                }
                VideoProperty::Size { width, height } => {
                    check_size(*width, *height)?;
                    let old = VideoProperty::Size {
                        width: video.width,
                        height: video.height,
                    };
                    (video.width, video.height) = (*width, *height);
                    (old, prop.clone())
                }
            };
            if let Err(e) = video.check_fields() {
                doc.videos[at] = before;
                return Err(e);
            }
            Ok(done(
                O::SetVideo {
                    id: id.clone(),
                    prop: new,
                },
                O::SetVideo {
                    id: id.clone(),
                    prop: old,
                },
                video_touched(id, None),
            ))
        }
        O::AddClip {
            video,
            frame,
            duration_ms,
            index,
            transition,
            motion,
        } => {
            doc.check_frame(frame)?;
            let at = doc.video_index(video)?;
            let clip = Clip {
                id: doc.alloc_id('c'),
                frame: frame.clone(),
                duration_ms: *duration_ms,
                transition: transition.unwrap_or_default(),
                motion: motion.unwrap_or_default(),
            };
            let target = &mut doc.videos[at];
            let index = index.unwrap_or(target.clips.len());
            if index > target.clips.len() {
                return Err(OpError::BadIndex {
                    parent: video.to_string(),
                    index,
                    len: target.clips.len(),
                });
            }
            target.clips.insert(index, clip.clone());
            if let Err(e) = target.check_fields() {
                target.clips.remove(index);
                return Err(e);
            }
            let touched = video_touched(video, Some(&clip.id));
            Ok(done(
                O::RestoreClip {
                    video: video.clone(),
                    index,
                    clip: clip.clone(),
                },
                O::RemoveClip {
                    video: video.clone(),
                    clip: clip.id,
                },
                touched,
            ))
        }
        O::RestoreClip { video, index, clip } => {
            if !allow_internal {
                return Err(OpError::InternalOnly("restore_clip"));
            }
            doc.check_frame(&clip.frame)?;
            let at = doc.video_index(video)?;
            let target = &mut doc.videos[at];
            if *index > target.clips.len() {
                return Err(OpError::BadIndex {
                    parent: video.to_string(),
                    index: *index,
                    len: target.clips.len(),
                });
            }
            if target.clips.iter().any(|c| c.id == clip.id) {
                return Err(OpError::DuplicateId(clip.id.to_string()));
            }
            target.clips.insert(*index, clip.clone());
            if let Err(e) = target.check_fields() {
                target.clips.remove(*index);
                return Err(e);
            }
            Ok(done(
                op.clone(),
                O::RemoveClip {
                    video: video.clone(),
                    clip: clip.id.clone(),
                },
                video_touched(video, Some(&clip.id)),
            ))
        }
        O::RemoveClip { video, clip } => {
            let at = doc.video_index(video)?;
            let target = &mut doc.videos[at];
            let index = target
                .clip(clip)
                .ok_or_else(|| OpError::NoSuchClip(clip.to_string()))?;
            let removed = target.clips.remove(index);
            Ok(done(
                op.clone(),
                O::RestoreClip {
                    video: video.clone(),
                    index,
                    clip: removed,
                },
                video_touched(video, Some(clip)),
            ))
        }
        O::MoveClip { video, clip, index } => {
            let at = doc.video_index(video)?;
            let target = &mut doc.videos[at];
            let from = target
                .clip(clip)
                .ok_or_else(|| OpError::NoSuchClip(clip.to_string()))?;
            if *index >= target.clips.len() {
                return Err(OpError::BadIndex {
                    parent: video.to_string(),
                    index: *index,
                    len: target.clips.len(),
                });
            }
            let moved = target.clips.remove(from);
            target.clips.insert(*index, moved);
            Ok(done(
                op.clone(),
                O::MoveClip {
                    video: video.clone(),
                    clip: clip.clone(),
                    index: from,
                },
                video_touched(video, Some(clip)),
            ))
        }
        O::SetClip { video, clip, prop } => {
            let vat = doc.video_index(video)?;
            if let ClipProperty::Frame(frame) = prop {
                doc.check_frame(frame)?;
            }
            let target = &mut doc.videos[vat];
            let cat = target
                .clip(clip)
                .ok_or_else(|| OpError::NoSuchClip(clip.to_string()))?;
            let before = target.clips[cat].clone();
            let c = &mut target.clips[cat];
            let old = match prop {
                ClipProperty::DurationMs(ms) => {
                    ClipProperty::DurationMs(std::mem::replace(&mut c.duration_ms, *ms))
                }
                ClipProperty::Frame(f) => {
                    ClipProperty::Frame(std::mem::replace(&mut c.frame, f.clone()))
                }
                ClipProperty::Transition(t) => {
                    ClipProperty::Transition(std::mem::replace(&mut c.transition, *t))
                }
                ClipProperty::Motion(m) => {
                    ClipProperty::Motion(std::mem::replace(&mut c.motion, *m))
                }
            };
            if let Err(e) = target.check_fields() {
                target.clips[cat] = before;
                return Err(e);
            }
            Ok(done(
                op.clone(),
                O::SetClip {
                    video: video.clone(),
                    clip: clip.clone(),
                    prop: old,
                },
                video_touched(video, Some(clip)),
            ))
        }
        _ => Err(OpError::Internal("not a video operation".to_string())),
    }
}
