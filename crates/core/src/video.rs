//! Videos: a timeline of clips, each showing a design frame for a while, with a transition into it
//! and a slow zoom (`docs/wiki/video-editing.md`).
//!
//! A video is data like everything else. It changes only through operations (so it has history,
//! attribution and checkpoints), and `render` and the exporter turn it into pictures and a file.
//! Time is whole milliseconds, never floating-point seconds, so edits do not drift.

use serde::{Deserialize, Serialize};

use crate::ease::Ease;
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

/// How an overlay arrives and leaves. Every kind but `None` also fades.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Entrance {
    /// Appears and disappears at once.
    None,
    #[default]
    Fade,
    /// Rises into place from slightly below (on exit, rises away).
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    /// Grows from 80% to full size (with `ease: back`, overshooting).
    Pop,
    /// Settles from 125% to full size.
    Zoom,
}

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
    /// The new clip slides in from below, pushing the old one up and out.
    SlideUp,
    /// The new clip slides in from above, pushing the old one down and out.
    SlideDown,
    /// The old clip fades to black, then the new one fades up from it.
    Dip,
    /// The new clip settles from 120% to full size while it fades in.
    ZoomIn,
    /// The new clip is uncovered by an edge sweeping from the right to the left.
    WipeLeft,
    /// The new clip is uncovered by an edge sweeping from the left to the right.
    WipeRight,
    /// The new clip is uncovered by an edge sweeping from the bottom to the top.
    WipeUp,
    /// The new clip is uncovered by an edge sweeping from the top to the bottom.
    WipeDown,
    /// The new clip shows through a circle that opens from the middle.
    Iris,
    /// The old clip washes out to white, then the new one comes up from it.
    Flash,
    /// Both clips blur while the new one fades in over the old.
    BlurDissolve,
    /// The picture breaks into horizontal slices that slide sideways by seeded, repeatable
    /// amounts while the new clip takes over.
    GlitchSlice,
    /// The old clip turns away like a card, narrowing to an edge, and the new one turns into
    /// view from the edge (flat: no perspective).
    Flip,
    /// The new clip is uncovered by a radius sweeping clockwise from twelve o'clock.
    ClockWipe,
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
    #[serde(default)]
    pub ease: Ease,
}

/// A slow zoom over the clip: the picture starts at `scale_from` times the frame and ends at
/// `scale_to`, centred. Both are 1 to 3; 1 and 1 is no motion.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    pub scale_from: f64,
    pub scale_to: f64,
    #[serde(default)]
    pub ease: Ease,
}

impl Default for Motion {
    fn default() -> Motion {
        Motion {
            scale_from: 1.0,
            scale_to: 1.0,
            ease: Ease::default(),
        }
    }
}

/// A stretch of a video file the user imported: it plays from `in_ms` into the file for as long
/// as the clip lasts, with the file's own sound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Footage {
    /// `media/<name>.mp4|mov|m4v`
    pub file: String,
    #[serde(default)]
    pub in_ms: u64,
}

/// The sound under a video's picture: one audio file from the project's `media/` folder, delayed
/// to where it starts, at a volume, with optional fades in and out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioTrack {
    /// `media/<name>.mp3|wav|m4a|aac|ogg`
    pub file: String,
    /// When it starts, counted from the beginning of the video.
    #[serde(default)]
    pub start_ms: u32,
    /// The volume: 1 is as recorded, 0 is silent, 2 is twice as loud.
    #[serde(default = "one")]
    pub gain: f32,
    /// How long it takes to reach its volume, and how long it takes to leave.
    #[serde(default)]
    pub fade_in_ms: u32,
    #[serde(default)]
    pub fade_out_ms: u32,
}

fn one() -> f32 {
    1.0
}

/// What a clip shows: a design frame, or footage (exactly one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ClipSource {
    #[serde(default)]
    pub frame: Option<NodeId>,
    #[serde(default)]
    pub footage: Option<Footage>,
}

pub const MAX_OVERLAYS: usize = 200;
pub const MAX_IN_MS: u64 = 24 * 60 * 60 * 1000;
/// How late a sound track may start, and how long its fades may be.
pub const MAX_AUDIO_MS: u32 = 600_000;
/// The loudest a sound track may be played.
pub const MAX_AUDIO_GAIN: f32 = 2.0;

/// A design frame drawn over the whole video for a while, fading in and out: a title, a lower
/// third, a call-out. A frame with no fill is transparent, so only what is drawn on it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    pub id: NodeId,
    pub frame: NodeId,
    /// From the start of the video.
    pub start_ms: u64,
    pub duration_ms: u32,
    /// How long the entrance and the exit each take, at most half the duration; 0 is a cut.
    #[serde(default)]
    pub fade_ms: u32,
    #[serde(default)]
    pub enter: Entrance,
    #[serde(default)]
    pub exit: Entrance,
    #[serde(default)]
    pub ease: Ease,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub id: NodeId,
    /// The design frame this clip shows (none for footage).
    #[serde(default)]
    pub frame: Option<NodeId>,
    /// The footage this clip plays (none for a design frame).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footage: Option<Footage>,
    pub duration_ms: u32,
    #[serde(default)]
    pub transition: Transition,
    #[serde(default)]
    pub motion: Motion,
    /// How fast footage plays, 0.25 to 4 (1 is as recorded). `duration_ms` is the length on the
    /// timeline, so the clip plays `duration_ms * speed` of its file (`crate::speed`).
    #[serde(default = "one_f64", skip_serializing_if = "is_one_f64")]
    pub speed: f64,
    /// Instead of a constant speed: the rate at moments of clip time, eased between keys.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub speed_ramp: Vec<crate::speed::SpeedKey>,
    /// Colour grading and effects over the clip's picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grade: Option<crate::grade::Grade>,
}

fn one_f64() -> f64 {
    1.0
}

fn is_one_f64(v: &f64) -> bool {
    *v == 1.0
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlays: Vec<Overlay>,
    /// The sound under the picture (none for a silent video).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioTrack>,
    /// More sound tracks (music, voice, effects) mixed with `audio`; see `crate::audio`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<crate::audio::SoundTrack>,
    /// Timed captions burned into the picture; see `crate::captions`.
    #[serde(default, skip_serializing_if = "crate::captions::Captions::is_empty")]
    pub captions: crate::captions::Captions,
    /// Numbers of nodes keyframed over the video's time, sorted by node and property.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub animations: Vec<crate::animation::Animation>,
    /// Pictures blended into each frame while something moves (0 is off, 2 to 16 on): the
    /// smear a real camera's shutter gives fast motion.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub motion_blur: u32,
    /// Each drawing is held for this many frames (1 draws every frame, 2 is animation "on
    /// twos", 3 "on threes"); see `Video::held_ms`.
    #[serde(default = "one_u32", skip_serializing_if = "is_one_u32")]
    pub hold_frames: u32,
}

pub const MAX_HOLD_FRAMES: u32 = 6;

fn one_u32() -> u32 {
    1
}

fn is_one_u32(v: &u32) -> bool {
    *v == 1
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
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

    /// The moment whose picture is shown at `ms`: with `hold_frames` above 1 every frame of a
    /// group of that many shows the picture of the group's first frame ("on twos"), otherwise
    /// `ms` itself. Idempotent, so a frame's own start time maps to its group's start time.
    pub fn held_ms(&self, ms: u64) -> u64 {
        if self.hold_frames <= 1 {
            return ms;
        }
        let frame = crate::timebase::frame_containing(ms, self.fps);
        let first = frame - frame % u64::from(self.hold_frames);
        crate::timebase::frame_ms(first, self.fps)
    }

    pub fn clip(&self, id: &NodeId) -> Option<usize> {
        self.clips.iter().position(|c| &c.id == id)
    }

    /// Field and structure checks that need no document.
    fn check_fields(&self) -> Result<(), OpError> {
        text_within("video name", &self.name, limits::MAX_NAME_LEN)?;
        check_fps(self.fps)?;
        check_size(self.width, self.height)?;
        if !(1..=MAX_HOLD_FRAMES).contains(&self.hold_frames) {
            return Err(OpError::OutOfBounds {
                what: "hold_frames",
                detail: format!(
                    "{} is not a number of frames from 1 to {MAX_HOLD_FRAMES}",
                    self.hold_frames
                ),
            });
        }
        if self.clips.len() > MAX_CLIPS {
            return Err(OpError::Limit(format!(
                "at most {MAX_CLIPS} clips per video"
            )));
        }
        if self.overlays.len() > MAX_OVERLAYS {
            return Err(OpError::Limit(format!("at most {MAX_OVERLAYS} overlays")));
        }
        for o in &self.overlays {
            check_overlay(o)?;
        }
        if let Some(a) = &self.audio {
            check_audio(a)?;
        }
        crate::audio::check_tracks(&self.tracks)?;
        crate::captions::check_captions(&self.captions)?;
        for clip in &self.clips {
            check_source(&ClipSource {
                frame: clip.frame.clone(),
                footage: clip.footage.clone(),
            })?;
            check_duration(clip.duration_ms)?;
            check_transition(&clip.transition, clip.duration_ms)?;
            check_motion(&clip.motion)?;
            crate::speed::check_clip_speed(clip)?;
            if let Some(g) = &clip.grade {
                g.checked()?;
            }
        }
        crate::animation::check_animations(self)?;
        if self.duration_ms() > MAX_VIDEO_MS {
            return Err(OpError::Limit(format!(
                "a video is at most {} minutes long",
                MAX_VIDEO_MS / 60_000
            )));
        }
        Ok(())
    }
}

/// A clip shows a frame or footage, not both and not neither; footage is a plain media reference.
pub fn check_source(s: &ClipSource) -> Result<(), OpError> {
    match (&s.frame, &s.footage) {
        (Some(_), None) => Ok(()),
        (None, Some(f)) => {
            crate::media::footage_name(&f.file)?;
            finite_in("in_ms", f.in_ms as f64, 0.0, MAX_IN_MS as f64).map(|_| ())
        }
        _ => Err(OpError::OutOfBounds {
            what: "clip source",
            detail: "give either a frame or footage, not both and not neither".to_string(),
        }),
    }
}

pub fn check_overlay(o: &Overlay) -> Result<(), OpError> {
    check_duration(o.duration_ms)?;
    if o.start_ms + u64::from(o.duration_ms) > MAX_VIDEO_MS {
        return Err(OpError::OutOfBounds {
            what: "overlay",
            detail: format!("must end within {} minutes", MAX_VIDEO_MS / 60_000),
        });
    }
    if u64::from(o.fade_ms) * 2 > u64::from(o.duration_ms) {
        return Err(OpError::OutOfBounds {
            what: "overlay fade_ms",
            detail: format!(
                "{} ms in and out do not fit a {} ms overlay (at most half each)",
                o.fade_ms, o.duration_ms
            ),
        });
    }
    Ok(())
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

/// A sound track names a file in `media/`, starts inside the video and is played at a volume
/// between silence and twice the recording.
pub fn check_audio(a: &AudioTrack) -> Result<(), OpError> {
    crate::media::audio_name(&a.file)?;
    if a.start_ms > MAX_AUDIO_MS {
        return Err(OpError::OutOfBounds {
            what: "audio start_ms",
            detail: format!("{} is later than {} ms", a.start_ms, MAX_AUDIO_MS),
        });
    }
    for (what, ms) in [
        ("audio fade_in_ms", a.fade_in_ms),
        ("audio fade_out_ms", a.fade_out_ms),
    ] {
        if ms > MAX_AUDIO_MS {
            return Err(OpError::OutOfBounds {
                what,
                detail: format!("{ms} is longer than {MAX_AUDIO_MS} ms"),
            });
        }
    }
    if !a.gain.is_finite() || !(0.0..=MAX_AUDIO_GAIN).contains(&a.gain) {
        return Err(OpError::OutOfBounds {
            what: "audio gain",
            detail: format!("{} is not a number from 0 to {MAX_AUDIO_GAIN}", a.gain),
        });
    }
    Ok(())
}

/// One settable property of a video. Serialized as `{"name": "fps", "value": 30}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum VideoProperty {
    Name(String),
    Fps(u32),
    Size {
        width: u32,
        height: u32,
    },
    /// The sound under the picture (`null` for a silent video).
    Audio(Option<AudioTrack>),
    /// The sound tracks besides `audio` (`crate::audio`).
    Tracks(Vec<crate::audio::SoundTrack>),
    /// The captions: style and cues (`crate::captions`).
    Captions(crate::captions::Captions),
    /// Pictures blended per frame for motion blur: 0 (off) or 2 to 16.
    MotionBlur(u32),
    /// Frames each drawing is held for: 1 (every frame is drawn) to 6; 2 is "on twos".
    HoldFrames(u32),
}

/// One settable property of a clip. Serialized as `{"name": "duration_ms", "value": 3000}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum ClipProperty {
    DurationMs(u32),
    Source(ClipSource),
    Transition(Transition),
    Motion(Motion),
    /// Footage speed, 0.25 to 4.
    Speed(f64),
    /// The speed ramp keys (empty for none).
    SpeedRamp(Vec<crate::speed::SpeedKey>),
    /// The colour grade (`null` for none).
    Grade(Option<crate::grade::Grade>),
}

/// One settable property of an overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum OverlayProperty {
    Frame(NodeId),
    StartMs(u64),
    DurationMs(u32),
    FadeMs(u32),
    Enter(Entrance),
    Exit(Entrance),
    Ease(Ease),
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
    pub fn video_using(&self, ids: &[NodeId]) -> Option<(&Video, NodeId, Option<NodeId>)> {
        self.videos.iter().find_map(|v| {
            v.clips
                .iter()
                .find(|c| c.frame.as_ref().is_some_and(|f| ids.contains(f)))
                .map(|c| (v, c.id.clone(), c.frame.clone()))
                .or_else(|| {
                    v.overlays
                        .iter()
                        .find(|o| ids.contains(&o.frame))
                        .map(|o| (v, o.id.clone(), Some(o.frame.clone())))
                })
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
                if let Some(f) = &clip.frame {
                    self.check_frame(f)?;
                }
            }
            for o in &video.overlays {
                if !seen.insert(o.id.clone()) {
                    return Err(OpError::DuplicateId(o.id.to_string()));
                }
                self.check_counter_of(&o.id)?;
                self.check_frame(&o.frame)?;
            }
            self.validate_animations(video)?;
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
    let snapped;
    let op = if allow_internal {
        op
    } else {
        snapped = crate::snap::video_op(doc, op);
        &snapped
    };
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
                overlays: Vec::new(),
                audio: None,
                tracks: Vec::new(),
                captions: Default::default(),
                animations: Vec::new(),
                motion_blur: 0,
                hold_frames: 1,
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
                if let Some(f) = &clip.frame {
                    doc.check_frame(f)?;
                }
            }
            for o in &video.overlays {
                doc.check_frame(&o.frame)?;
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
                VideoProperty::Audio(audio) => {
                    if let Some(a) = audio {
                        check_audio(a)?;
                    }
                    let old = std::mem::replace(&mut video.audio, audio.clone());
                    (VideoProperty::Audio(old), prop.clone())
                }
                VideoProperty::Tracks(tracks) => {
                    let old = std::mem::replace(&mut video.tracks, tracks.clone());
                    (VideoProperty::Tracks(old), prop.clone())
                }
                VideoProperty::Captions(captions) => {
                    let old = std::mem::replace(&mut video.captions, captions.clone());
                    (VideoProperty::Captions(old), prop.clone())
                }
                VideoProperty::MotionBlur(samples) => {
                    let old = std::mem::replace(&mut video.motion_blur, *samples);
                    (VideoProperty::MotionBlur(old), prop.clone())
                }
                VideoProperty::HoldFrames(frames) => {
                    let old = std::mem::replace(&mut video.hold_frames, *frames);
                    (VideoProperty::HoldFrames(old), prop.clone())
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
            footage,
            duration_ms,
            index,
            transition,
            motion,
        } => {
            check_source(&ClipSource {
                frame: frame.clone(),
                footage: footage.clone(),
            })?;
            if let Some(f) = frame {
                doc.check_frame(f)?;
            }
            let at = doc.video_index(video)?;
            let clip = Clip {
                id: doc.alloc_id('c'),
                frame: frame.clone(),
                footage: footage.clone(),
                duration_ms: *duration_ms,
                transition: transition.unwrap_or_default(),
                motion: motion.unwrap_or_default(),
                speed: 1.0,
                speed_ramp: Vec::new(),
                grade: None,
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
            if let Some(f) = &clip.frame {
                doc.check_frame(f)?;
            }
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
            if let ClipProperty::Source(source) = prop {
                check_source(source)?;
                if let Some(f) = &source.frame {
                    doc.check_frame(f)?;
                }
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
                ClipProperty::Source(s) => {
                    let old = ClipSource {
                        frame: c.frame.take(),
                        footage: c.footage.take(),
                    };
                    c.frame = s.frame.clone();
                    c.footage = s.footage.clone();
                    ClipProperty::Source(old)
                }
                ClipProperty::Transition(t) => {
                    ClipProperty::Transition(std::mem::replace(&mut c.transition, *t))
                }
                ClipProperty::Motion(m) => {
                    ClipProperty::Motion(std::mem::replace(&mut c.motion, *m))
                }
                ClipProperty::Speed(v) => {
                    let v = crate::speed::check_speed(*v)?;
                    ClipProperty::Speed(std::mem::replace(&mut c.speed, v))
                }
                ClipProperty::SpeedRamp(keys) => {
                    let keys = crate::speed::check_ramp(keys)?;
                    ClipProperty::SpeedRamp(std::mem::replace(&mut c.speed_ramp, keys))
                }
                ClipProperty::Grade(g) => {
                    let g = g.as_ref().map(crate::grade::Grade::checked).transpose()?;
                    let g = g.filter(|g| !g.is_neutral());
                    ClipProperty::Grade(std::mem::replace(&mut c.grade, g))
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
        O::AddOverlay {
            video,
            frame,
            start_ms,
            duration_ms,
            fade_ms,
            enter,
            exit,
            ease,
        } => {
            doc.check_frame(frame)?;
            let at = doc.video_index(video)?;
            let overlay = Overlay {
                id: doc.alloc_id('o'),
                frame: frame.clone(),
                start_ms: *start_ms,
                duration_ms: *duration_ms,
                fade_ms: fade_ms.unwrap_or(0),
                enter: enter.unwrap_or_default(),
                exit: exit.unwrap_or_default(),
                ease: ease.unwrap_or(Ease::EaseOut),
            };
            let target = &mut doc.videos[at];
            target.overlays.push(overlay.clone());
            if let Err(e) = target.check_fields() {
                target.overlays.pop();
                return Err(e);
            }
            let touched = video_touched(video, Some(&overlay.id));
            Ok(done(
                O::RestoreOverlay {
                    video: video.clone(),
                    index: doc.videos[at].overlays.len() - 1,
                    overlay: overlay.clone(),
                },
                O::RemoveOverlay {
                    video: video.clone(),
                    overlay: overlay.id,
                },
                touched,
            ))
        }
        O::RestoreOverlay {
            video,
            index,
            overlay,
        } => {
            if !allow_internal {
                return Err(OpError::InternalOnly("restore_overlay"));
            }
            doc.check_frame(&overlay.frame)?;
            let at = doc.video_index(video)?;
            let target = &mut doc.videos[at];
            if *index > target.overlays.len() {
                return Err(OpError::BadIndex {
                    parent: video.to_string(),
                    index: *index,
                    len: target.overlays.len(),
                });
            }
            if target.overlays.iter().any(|o| o.id == overlay.id) {
                return Err(OpError::DuplicateId(overlay.id.to_string()));
            }
            target.overlays.insert(*index, overlay.clone());
            if let Err(e) = target.check_fields() {
                target.overlays.remove(*index);
                return Err(e);
            }
            Ok(done(
                op.clone(),
                O::RemoveOverlay {
                    video: video.clone(),
                    overlay: overlay.id.clone(),
                },
                video_touched(video, Some(&overlay.id)),
            ))
        }
        O::RemoveOverlay { video, overlay } => {
            let at = doc.video_index(video)?;
            let target = &mut doc.videos[at];
            let index = target
                .overlays
                .iter()
                .position(|o| &o.id == overlay)
                .ok_or_else(|| OpError::NoSuchClip(overlay.to_string()))?;
            let removed = target.overlays.remove(index);
            Ok(done(
                op.clone(),
                O::RestoreOverlay {
                    video: video.clone(),
                    index,
                    overlay: removed,
                },
                video_touched(video, Some(overlay)),
            ))
        }
        O::SetOverlay {
            video,
            overlay,
            prop,
        } => {
            let vat = doc.video_index(video)?;
            if let OverlayProperty::Frame(f) = prop {
                doc.check_frame(f)?;
            }
            let target = &mut doc.videos[vat];
            let oat = target
                .overlays
                .iter()
                .position(|o| &o.id == overlay)
                .ok_or_else(|| OpError::NoSuchClip(overlay.to_string()))?;
            let before = target.overlays[oat].clone();
            let o = &mut target.overlays[oat];
            let old = match prop {
                OverlayProperty::Frame(f) => {
                    OverlayProperty::Frame(std::mem::replace(&mut o.frame, f.clone()))
                }
                OverlayProperty::StartMs(ms) => {
                    OverlayProperty::StartMs(std::mem::replace(&mut o.start_ms, *ms))
                }
                OverlayProperty::DurationMs(ms) => {
                    OverlayProperty::DurationMs(std::mem::replace(&mut o.duration_ms, *ms))
                }
                OverlayProperty::FadeMs(ms) => {
                    OverlayProperty::FadeMs(std::mem::replace(&mut o.fade_ms, *ms))
                }
                OverlayProperty::Enter(e) => {
                    OverlayProperty::Enter(std::mem::replace(&mut o.enter, *e))
                }
                OverlayProperty::Exit(e) => {
                    OverlayProperty::Exit(std::mem::replace(&mut o.exit, *e))
                }
                OverlayProperty::Ease(e) => {
                    OverlayProperty::Ease(std::mem::replace(&mut o.ease, *e))
                }
            };
            if let Err(e) = target.check_fields() {
                target.overlays[oat] = before;
                return Err(e);
            }
            Ok(done(
                op.clone(),
                O::SetOverlay {
                    video: video.clone(),
                    overlay: overlay.clone(),
                    prop: old,
                },
                video_touched(video, Some(overlay)),
            ))
        }
        _ => Err(OpError::Internal("not a video operation".to_string())),
    }
}
