//! Pictures of a video at any moment: the clip being shown, a slow zoom over it, the transition
//! from the clip before, the overlays, keyframed nodes (`od_core::animation`) and motion blur
//! (`docs/wiki/video-editing.md`).
//!
//! Each clip shows a design frame, drawn by [`crate::render_frame_with`] once at the scale its
//! output needs and reused for every moment of the clip. A frame that holds animated nodes or a
//! screen playing a recording is "live": it is drawn again whenever what it shows changes.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::Path as FsPath;
use std::sync::{Arc, Mutex, PoisonError};

use od_core::speed::SpeedMap;
use od_core::{Document, Entrance, Node, NodeId, NodeKind, Overlay, TransitionKind, Video};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::grade;
use crate::transitions::{self, Reveal};
use crate::{Media, RenderError, Warning, render_frame_camera};

/// Frames kept drawn: a transition shows two clips and a second transition may start under it.
const KEPT: usize = 3;

/// Where footage frames come from. The editor draws design frames itself but leaves decoding
/// video to the program that can (`od_media`); the renderer asks for one picture at a time.
pub trait FootageSource {
    /// The RGBA pixels (`width * height * 4` bytes) of the file `file` (`media/<name>`) at
    /// `at_ms` into it, scaled to cover `width` x `height` and cropped in the middle. Past the end
    /// it gives the last picture.
    fn frame(&mut self, file: &str, at_ms: u64, width: u32, height: u32)
    -> Result<Vec<u8>, String>;
}

pub struct DrawnFrame {
    frame: NodeId,
    scale_bits: u32,
    /// For a live frame: the animated values and recording times it was drawn with.
    state: Vec<u64>,
    pixmap: Pixmap,
}

/// What changes a frame's picture over time.
#[derive(Default)]
struct Liveness {
    /// Indices into `video.animations` of nodes inside the frame.
    animations: Vec<usize>,
    /// Indices of the camera's animations, which move a frame shown by a clip but not an overlay.
    camera: Vec<usize>,
    /// The frame holds a device whose screen plays a recording.
    plays: bool,
}

impl Liveness {
    /// The animations that change the picture: the frame's nodes', and the camera's when the
    /// frame is drawn through it.
    fn moving_indices(&self, through_camera: bool) -> impl Iterator<Item = usize> + '_ {
        let camera: &[usize] = if through_camera { &self.camera } else { &[] };
        self.animations.iter().chain(camera).copied()
    }
}

/// One moment being drawn: the time that places clips and overlays, the time animations are
/// read at (they differ only by fractions of a millisecond inside a motion-blur shutter), and
/// the document as it looks then.
struct Moment<'d> {
    time: u64,
    anim_ms: f64,
    /// The time recordings are read at: the frame's own moment, not the blur sample's, so a
    /// recording is decoded once per frame.
    footage_time: u64,
    doc: Option<&'d Document>,
}

/// Draws the moments of one video.
pub struct VideoRenderer<'a> {
    doc: &'a Document,
    video: &'a Video,
    project_dir: Option<&'a FsPath>,
    /// The size of the pictures drawn, the video's own unless a smaller one was asked for.
    width: u32,
    height: u32,
    drawn: Vec<DrawnFrame>,
    /// A cache shared with other renderers (the parallel export), so a design frame is drawn
    /// once for the whole export and not once per thread.
    shared: Option<Arc<Mutex<Vec<DrawnFrame>>>>,
    warnings: Vec<Warning>,
    footage: Option<&'a mut dyn FootageSource>,
    live: HashMap<NodeId, Liveness>,
    /// Where each clip's footage time comes from (`od_core::speed`), by clip index.
    speeds: Vec<SpeedMap>,
}

/// How far a blur dissolve blurs at most, in thousandths of the shorter side (a grade's blur
/// unit).
const BLUR_DISSOLVE: f64 = 40.0;

impl<'a> VideoRenderer<'a> {
    pub fn new(
        doc: &'a Document,
        video: &'a Video,
        project_dir: Option<&'a FsPath>,
    ) -> VideoRenderer<'a> {
        VideoRenderer {
            doc,
            video,
            project_dir,
            width: video.width,
            height: video.height,
            drawn: Vec::new(),
            shared: None,
            warnings: Vec::new(),
            footage: None,
            live: live_frames(doc, video),
            speeds: video.clips.iter().map(SpeedMap::new).collect(),
        }
    }

    /// Lets clips of footage, and screens that play recordings, be drawn.
    pub fn with_footage(mut self, source: &'a mut dyn FootageSource) -> VideoRenderer<'a> {
        self.footage = Some(source);
        self
    }

    /// Shares the cache of drawn design frames with other renderers, so a frame that several
    /// threads show at once is drawn once (the parallel export does this).
    pub fn with_shared_cache(mut self, cache: Arc<Mutex<Vec<DrawnFrame>>>) -> VideoRenderer<'a> {
        self.shared = Some(cache);
        self
    }

    /// Like [`VideoRenderer::new`] but drawing pictures of `width` x `height`, for previews.
    pub fn with_size(mut self, width: u32, height: u32) -> VideoRenderer<'a> {
        (self.width, self.height) = (width.max(1), height.max(1));
        self
    }

    /// What could not be drawn so far (a missing image, text that overflows its box).
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// How long the motion-blur shutter stays open before each frame: half a frame (a 180
    /// degree shutter), or nothing when motion blur is off.
    fn shutter_ms(&self) -> f64 {
        if self.video.motion_blur >= 2 {
            500.0 / f64::from(self.video.fps.max(1))
        } else {
            0.0
        }
    }

    /// A value that is the same for two moments exactly when the picture is: while a clip of a
    /// design frame holds still (no zoom, no transition, no fade, nothing animated moving)
    /// every frame is identical, and an export can reuse the last one instead of drawing it
    /// again. `None` when the picture changes all the time (footage, a screen playing).
    pub fn signature(&self, time_ms: u64) -> Option<Vec<i64>> {
        let mut sig = self.signature_at(time_ms, true)?;
        let shutter = self.shutter_ms();
        if shutter > 0.0 {
            // a blurred picture also shows the moments just before it
            let before = time_ms.saturating_sub(shutter.ceil() as u64);
            sig.push(-1);
            sig.extend(self.signature_at(before, true)?);
        }
        Some(sig)
    }

    /// The signature of the single moment `time_ms`; footage makes it `None` only when
    /// `footage_moves`.
    fn signature_at(&self, time_ms: u64, footage_moves: bool) -> Option<Vec<i64>> {
        let placed = self.video.placed();
        let Some(last) = placed.last() else {
            return Some(Vec::new());
        };
        let time = time_ms.min(last.end_ms.saturating_sub(1));
        let mut sig = Vec::new();
        for (n, p) in placed
            .iter()
            .enumerate()
            .filter(|(_, p)| p.start_ms <= time && time < p.end_ms)
        {
            let clip = &self.video.clips[p.index];
            if clip.footage.is_some() && footage_moves {
                return None;
            }
            sig.push(n as i64);
            let into = time - p.start_ms;
            if into < u64::from(p.overlap_ms) {
                sig.push(into as i64);
            }
            // while a zoom runs, every moment differs
            if (clip.motion.scale_to - clip.motion.scale_from).abs() > f64::EPSILON {
                sig.push(time as i64);
            }
            // a clip arriving later changes how this one is pushed or faded
            if placed
                .get(p.index + 1)
                .is_some_and(|next| next.start_ms <= time && time < p.end_ms)
            {
                sig.push(time as i64);
            }
            if let Some(frame) = &clip.frame {
                self.live_signature(frame, time, footage_moves, true, &mut sig)?;
            }
        }
        for (n, o) in self.video.overlays.iter().enumerate() {
            let end = o.start_ms + u64::from(o.duration_ms);
            if o.start_ms <= time && time < end {
                sig.push(1_000 + n as i64);
                let fading = o.fade_ms > 0
                    && (time - o.start_ms < u64::from(o.fade_ms)
                        || end - time <= u64::from(o.fade_ms));
                if fading {
                    sig.push(time as i64);
                }
                self.live_signature(&o.frame, time, footage_moves, false, &mut sig)?;
            }
        }
        if let Some((cue, word)) = crate::captions::signature(self.video, time) {
            sig.extend([-2, cue as i64, word.map_or(-1, |w| w as i64)]);
        }
        Some(sig)
    }

    /// Adds what a live frame shows at `time` to a signature: the moment itself while one of its
    /// animations moves. `None` while a screen in it plays (and `footage_moves`).
    fn live_signature(
        &self,
        frame: &NodeId,
        time: u64,
        footage_moves: bool,
        through_camera: bool,
        sig: &mut Vec<i64>,
    ) -> Option<()> {
        let Some(live) = self.live.get(frame) else {
            return Some(());
        };
        if live.plays && footage_moves {
            return None;
        }
        let moving = live
            .moving_indices(through_camera)
            .any(|i| self.video.animations[i].moving_at(time as f64));
        if moving {
            sig.push(time as i64);
        }
        Some(())
    }

    /// Whether anything but recordings moves at `time_ms` or within the shutter before it.
    fn in_motion(&self, time_ms: u64, shutter: f64) -> bool {
        let before = time_ms.saturating_sub(shutter.ceil() as u64);
        self.moving_at(time_ms) || self.moving_at(before)
    }

    /// Whether a transition, zoom, overlay fade or animation is under way at `time_ms`.
    fn moving_at(&self, time_ms: u64) -> bool {
        let placed = self.video.placed();
        let Some(last) = placed.last() else {
            return false;
        };
        let time = time_ms.min(last.end_ms.saturating_sub(1));
        let animated = |frame: &NodeId, through_camera: bool| {
            self.live.get(frame).is_some_and(|l| {
                l.moving_indices(through_camera)
                    .any(|i| self.video.animations[i].moving_at(time as f64))
            })
        };
        let clips = placed
            .iter()
            .filter(|p| p.start_ms <= time && time < p.end_ms)
            .any(|p| {
                let clip = &self.video.clips[p.index];
                time - p.start_ms < u64::from(p.overlap_ms)
                    || (clip.motion.scale_to - clip.motion.scale_from).abs() > f64::EPSILON
                    || clip.frame.as_ref().is_some_and(|f| animated(f, true))
            });
        let overlays = self.video.overlays.iter().any(|o| {
            let end = o.start_ms + u64::from(o.duration_ms);
            o.start_ms <= time
                && time < end
                && ((o.fade_ms > 0
                    && (time - o.start_ms < u64::from(o.fade_ms)
                        || end - time <= u64::from(o.fade_ms)))
                    || animated(&o.frame, false))
        });
        clips || overlays
    }

    /// The earliest moment a blurred picture at `time_ms` may show: never before a cut, which a
    /// real shutter does not cross.
    fn shutter_floor(&self, time_ms: u64) -> f64 {
        self.video
            .placed()
            .iter()
            .filter(|p| p.start_ms <= time_ms && p.overlap_ms == 0)
            .map(|p| p.start_ms)
            .max()
            .unwrap_or(0) as f64
    }

    /// The picture at `time_ms` from the start. Past the end it is the last picture; an empty
    /// video is black. With motion blur on, a moment in which something moves is the average of
    /// several pictures across the shutter before it.
    pub fn frame_at(&mut self, time_ms: u64) -> Result<Pixmap, RenderError> {
        let shutter = self.shutter_ms();
        if shutter <= 0.0 || !self.in_motion(time_ms, shutter) {
            return self.picture(time_ms as f64, time_ms);
        }
        let samples = self.video.motion_blur;
        let floor = self.shutter_floor(time_ms);
        let mut sum: Vec<u32> = Vec::new();
        for i in 0..samples {
            let back = shutter * f64::from(samples - 1 - i) / f64::from(samples - 1);
            let at = (time_ms as f64 - back).max(floor);
            let picture = self.picture(at, time_ms)?;
            if sum.is_empty() {
                sum = vec![0; picture.data().len()];
            }
            for (s, v) in sum.iter_mut().zip(picture.data()) {
                *s += u32::from(*v);
            }
        }
        let (w, h) = (self.width, self.height);
        let mut out = Pixmap::new(w, h).ok_or(RenderError::TooLarge {
            width: u64::from(w),
            height: u64::from(h),
        })?;
        let half = samples / 2;
        for (o, s) in out.data_mut().iter_mut().zip(&sum) {
            *o = ((s + half) / samples) as u8;
        }
        Ok(out)
    }

    /// One unblurred picture: clips and overlays placed at `anim_ms` (rounded down), animated
    /// nodes as they are at `anim_ms`, recordings at `footage_time`.
    fn picture(&mut self, anim_ms: f64, footage_time: u64) -> Result<Pixmap, RenderError> {
        let (w, h) = (self.width, self.height);
        let mut canvas = Pixmap::new(w, h).ok_or(RenderError::TooLarge {
            width: u64::from(w),
            height: u64::from(h),
        })?;
        canvas.fill(tiny_skia::Color::BLACK);
        let placed = self.video.placed();
        let Some(last) = placed.last() else {
            return Ok(canvas);
        };
        let end = last.end_ms.saturating_sub(1);
        let anim_ms = anim_ms.min(end as f64);
        let time = (anim_ms.floor() as u64).min(end);
        let animated = self.doc.at_time(self.video, anim_ms);
        let moment = Moment {
            time,
            anim_ms,
            footage_time: footage_time.min(end),
            doc: animated.as_ref(),
        };
        let active: Vec<_> = placed
            .iter()
            .filter(|p| p.start_ms <= time && time < p.end_ms)
            .collect();
        // How far into its arrival each active clip is (None once it has fully arrived).
        let arriving: Vec<Option<f32>> = active
            .iter()
            .enumerate()
            .map(|(n, p)| {
                let into = time - p.start_ms;
                (n > 0 && into < u64::from(p.overlap_ms)).then(|| into as f32 / p.overlap_ms as f32)
            })
            .collect();
        let (width, height) = (w as f32, h as f32);
        for (n, p) in active.iter().enumerate() {
            let tr = |i: usize| self.video.clips[active[i].index].transition;
            let mut pose = Pose::STILL;
            if let Some(t) = arriving[n] {
                let tr = tr(n);
                let e = tr.ease.apply(f64::from(t)) as f32;
                match tr.kind {
                    TransitionKind::Fade => pose.opacity = e,
                    TransitionKind::SlideLeft => pose.dx = (1.0 - e) * width,
                    TransitionKind::SlideRight => pose.dx = -(1.0 - e) * width,
                    TransitionKind::SlideUp => pose.dy = (1.0 - e) * height,
                    TransitionKind::SlideDown => pose.dy = -(1.0 - e) * height,
                    TransitionKind::Dip => pose.opacity = (2.0 * e - 1.0).clamp(0.0, 1.0),
                    TransitionKind::ZoomIn => {
                        pose.opacity = e;
                        pose.scale = 1.2 - 0.2 * e;
                    }
                    TransitionKind::BlurDissolve => {
                        pose.opacity = e;
                        pose.blur = grade::blur_sigma(BLUR_DISSOLVE, w, h) * (1.0 - e);
                    }
                    TransitionKind::Flash => pose.opacity = f32::from(e >= 0.5),
                    TransitionKind::Cut => {}
                    masked => pose.reveal = Reveal::of(masked, e, p.index as u64),
                }
            }
            // The clip arriving after this one pushes it out, or fades it to black.
            if let Some(t) = arriving.get(n + 1).copied().flatten() {
                let next = tr(n + 1);
                let e = next.ease.apply(f64::from(t)) as f32;
                match next.kind {
                    TransitionKind::SlideLeft => pose.dx -= e * width,
                    TransitionKind::SlideRight => pose.dx += e * width,
                    TransitionKind::SlideUp => pose.dy -= e * height,
                    TransitionKind::SlideDown => pose.dy += e * height,
                    TransitionKind::Dip => pose.opacity *= (1.0 - 2.0 * e).clamp(0.0, 1.0),
                    TransitionKind::BlurDissolve => {
                        pose.blur = grade::blur_sigma(BLUR_DISSOLVE, w, h) * e;
                    }
                    TransitionKind::Flash => pose.opacity *= f32::from(e < 0.5),
                    _ => {}
                }
            }
            pose.opacity = pose.opacity.clamp(0.0, 1.0);
            self.draw_clip(&mut canvas, p.index, &moment, pose)?;
        }
        for (n, t) in arriving.iter().enumerate() {
            let tr = self.video.clips[active[n].index].transition;
            if let (Some(t), TransitionKind::Flash) = (t, tr.kind) {
                let e = tr.ease.apply(f64::from(*t)) as f32;
                flash_white(&mut canvas, 1.0 - (2.0 * e - 1.0).abs());
            }
        }
        let overlays: Vec<Overlay> = self.video.overlays.clone();
        for o in overlays
            .iter()
            .filter(|o| o.start_ms <= time && time < o.start_ms + u64::from(o.duration_ms))
        {
            let into = (time - o.start_ms) as f64;
            let left = (u64::from(o.duration_ms) - (time - o.start_ms)) as f64;
            let fade = f64::from(o.fade_ms);
            let progress = |elapsed: f64| {
                if fade > 0.0 && elapsed < fade {
                    o.ease.apply(elapsed / fade) as f32
                } else {
                    1.0
                }
            };
            let mut pose = Pose::STILL;
            pose.then(entrance_pose(o.enter, progress(into), width, height, true));
            pose.then(entrance_pose(o.exit, progress(left), width, height, false));
            pose.opacity = pose.opacity.clamp(0.0, 1.0);
            let played = moment.footage_time.saturating_sub(o.start_ms);
            self.draw_overlay(&mut canvas, &o.frame, &moment, played, pose)?;
        }
        crate::captions::draw(&mut canvas, self.video, time)?;
        Ok(canvas)
    }

    /// Draws a design frame over everything, scaled to cover and centered, then posed.
    fn draw_overlay(
        &mut self,
        canvas: &mut Pixmap,
        frame: &NodeId,
        moment: &Moment,
        played_ms: u64,
        pose: Pose,
    ) -> Result<(), RenderError> {
        let (w, h) = (self.width as f32, self.height as f32);
        let node = self
            .doc
            .node(frame)
            .ok_or_else(|| RenderError::NoSuchNode(frame.to_string()))?;
        let cover = (w / node.width as f32).max(h / node.height as f32);
        let drawn = self.drawn_frame(frame, cover, moment, played_ms, false)?;
        let (pw, ph) = (drawn.width() as f32, drawn.height() as f32);
        let moved = pose.dx != 0.0 || pose.dy != 0.0 || (pose.scale - 1.0).abs() > 1e-4;
        let paint = PixmapPaint {
            opacity: pose.opacity,
            quality: if moved {
                FilterQuality::Bicubic
            } else {
                FilterQuality::Nearest
            },
            ..PixmapPaint::default()
        };
        let s = pose.scale;
        let transform = Transform::from_row(
            s,
            0.0,
            0.0,
            s,
            ((w - pw * s) / 2.0 + pose.dx).round(),
            ((h - ph * s) / 2.0 + pose.dy).round(),
        );
        canvas.draw_pixmap(0, 0, drawn.as_ref().as_ref(), &paint, transform, None);
        Ok(())
    }

    /// Draws clip `index` as it looks at the moment, posed, onto `canvas`.
    fn draw_clip(
        &mut self,
        canvas: &mut Pixmap,
        index: usize,
        moment: &Moment,
        pose: Pose,
    ) -> Result<(), RenderError> {
        let placed = self.video.placed();
        let p = placed[index];
        let clip = self.video.clips[index].clone();
        let time = moment.time;
        let t = (time.saturating_sub(p.start_ms)) as f32 / (p.end_ms - p.start_ms) as f32;
        let zoom = lerp(
            clip.motion.scale_from,
            clip.motion.scale_to,
            clip.motion.ease.apply(f64::from(t.min(1.0))) as f32,
        ) as f32;
        let (w, h) = (self.width as f32, self.height as f32);
        let widest = clip.motion.scale_from.max(clip.motion.scale_to) as f32;
        let played = moment.footage_time.saturating_sub(p.start_ms);
        // The picture is made at `widest` times the size that covers the video; shrink it to this
        // moment's zoom.
        let drawn: Cow<'_, Pixmap> = match (&clip.frame, &clip.footage) {
            (Some(frame), _) => {
                let node = self
                    .doc
                    .node(frame)
                    .ok_or_else(|| RenderError::NoSuchNode(frame.to_string()))?;
                let cover = (w / node.width as f32).max(h / node.height as f32);
                self.drawn_frame(frame, cover * widest, moment, played, true)?
            }
            (None, Some(f)) => {
                let at = f.in_ms + self.speeds[index].source_ms(played);
                let (pw, ph) = (
                    (w * widest).round().max(2.0) as u32,
                    (h * widest).round().max(2.0) as u32,
                );
                let source = self.footage.as_deref_mut().ok_or_else(|| {
                    RenderError::Footage("this renderer has no footage source".into())
                })?;
                let rgba = source
                    .frame(&f.file, at, pw, ph)
                    .map_err(RenderError::Footage)?;
                let mut pixmap = Pixmap::new(pw, ph).ok_or(RenderError::TooLarge {
                    width: u64::from(pw),
                    height: u64::from(ph),
                })?;
                if rgba.len() != pixmap.data().len() {
                    return Err(RenderError::Footage(format!(
                        "{} gave {} bytes for a {pw}x{ph} picture",
                        f.file,
                        rgba.len()
                    )));
                }
                pixmap.data_mut().copy_from_slice(&rgba);
                std::borrow::Cow::Owned(pixmap)
            }
            (None, None) => return Ok(()),
        };
        let drawn = match clip.grade.as_ref().filter(|g| !g.is_neutral()) {
            Some(g) => {
                let mut graded = drawn.into_owned();
                grade::apply(&mut graded, g);
                std::borrow::Cow::Owned(graded)
            }
            None => drawn,
        };
        let k = zoom / widest * pose.scale;
        let (pw, ph) = (drawn.width() as f32 * k, drawn.height() as f32 * k);
        let transform = Transform::from_row(
            k,
            0.0,
            0.0,
            k,
            pose.dx + (w - pw) / 2.0,
            pose.dy + (h - ph) / 2.0,
        );
        let quality = if (k - 1.0).abs() < 1e-4 && pose.dx.fract() == 0.0 && pose.dy.fract() == 0.0
        {
            FilterQuality::Nearest
        } else {
            FilterQuality::Bicubic
        };
        let paint = PixmapPaint {
            opacity: pose.opacity,
            quality,
            ..PixmapPaint::default()
        };
        if pose.reveal.is_none() && pose.blur <= 0.0 {
            canvas.draw_pixmap(0, 0, drawn.as_ref().as_ref(), &paint, transform, None);
            return Ok(());
        }
        let mut layer =
            Pixmap::new(canvas.width(), canvas.height()).ok_or(RenderError::TooLarge {
                width: u64::from(canvas.width()),
                height: u64::from(canvas.height()),
            })?;
        let plain = PixmapPaint {
            quality,
            ..PixmapPaint::default()
        };
        layer.draw_pixmap(0, 0, drawn.as_ref().as_ref(), &plain, transform, None);
        if pose.blur > 0.0 {
            grade::blur(&mut layer, pose.blur);
        }
        if let Some(r) = pose.reveal {
            transitions::reveal(&mut layer, r);
        }
        let over = PixmapPaint {
            opacity: pose.opacity,
            ..PixmapPaint::default()
        };
        canvas.draw_pixmap(0, 0, layer.as_ref(), &over, Transform::identity(), None);
        Ok(())
    }

    /// What a live frame shows at the moment, as numbers: its animated values and, when a
    /// screen in it plays, the recording's time. Empty for a frame that never changes.
    fn state_of(
        &self,
        frame: &NodeId,
        moment: &Moment,
        played_ms: u64,
        through_camera: bool,
    ) -> Vec<u64> {
        let Some(live) = self.live.get(frame) else {
            return Vec::new();
        };
        let mut state: Vec<u64> = live
            .moving_indices(through_camera)
            .filter_map(|i| self.video.animations[i].value_at(moment.anim_ms))
            .map(f64::to_bits)
            .collect();
        if live.plays {
            state.push(u64::MAX);
            state.push(played_ms);
        }
        state
    }

    /// The design frame drawn at `scale` as it looks at the moment, from the kept pictures or
    /// freshly.
    fn drawn_frame(
        &mut self,
        frame: &NodeId,
        scale: f32,
        moment: &Moment,
        played_ms: u64,
        through_camera: bool,
    ) -> Result<Cow<'_, Pixmap>, RenderError> {
        let bits = scale.to_bits();
        let state = self.state_of(frame, moment, played_ms, through_camera);
        if let Some(shared) = self.shared.clone() {
            {
                let cache = shared.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some(d) = cache
                    .iter()
                    .find(|d| &d.frame == frame && d.scale_bits == bits && d.state == state)
                {
                    return Ok(Cow::Owned(d.pixmap.clone()));
                }
            }
            let pixmap = self.render_drawn(frame, scale, moment, played_ms, through_camera)?;
            let mut cache = shared.lock().unwrap_or_else(PoisonError::into_inner);
            if cache.len() >= KEPT {
                cache.remove(0);
            }
            cache.push(DrawnFrame {
                frame: frame.clone(),
                scale_bits: bits,
                state,
                pixmap: pixmap.clone(),
            });
            return Ok(Cow::Owned(pixmap));
        }
        if let Some(at) = self
            .drawn
            .iter()
            .position(|d| &d.frame == frame && d.scale_bits == bits && d.state == state)
        {
            // most recently used last
            let d = self.drawn.remove(at);
            self.drawn.push(d);
        } else {
            let pixmap = self.render_drawn(frame, scale, moment, played_ms, through_camera)?;
            if self.drawn.len() >= KEPT {
                self.drawn.remove(0);
            }
            self.drawn.push(DrawnFrame {
                frame: frame.clone(),
                scale_bits: bits,
                state,
                pixmap,
            });
        }
        self.drawn
            .last()
            .map(|d| Cow::Borrowed(&d.pixmap))
            .ok_or_else(|| RenderError::NoSuchNode(frame.to_string()))
    }

    /// Draws the design frame at `scale` as it looks at the moment.
    fn render_drawn(
        &mut self,
        frame: &NodeId,
        scale: f32,
        moment: &Moment,
        played_ms: u64,
        through_camera: bool,
    ) -> Result<Pixmap, RenderError> {
        let camera = if through_camera {
            self.video.camera_at(moment.anim_ms)
        } else {
            od_core::Camera::default()
        };
        let doc = match moment.doc {
            Some(animated) if self.live.contains_key(frame) => animated,
            _ => self.doc,
        };
        let mut media = Media {
            footage: self
                .footage
                .as_deref_mut()
                .map(|f| f as &mut dyn FootageSource),
            played_ms,
        };
        let rendered = render_frame_camera(
            doc,
            frame,
            scale.clamp(0.01, 16.0),
            self.project_dir,
            &mut media,
            camera,
        )?;
        for warning in rendered.warnings {
            if !self.warnings.contains(&warning) {
                self.warnings.push(warning);
            }
        }
        Ok(rendered.pixmap)
    }
}

/// The frames a video shows whose picture changes over time, and what changes them.
fn live_frames(doc: &Document, video: &Video) -> HashMap<NodeId, Liveness> {
    let in_clips: HashSet<&NodeId> = video
        .clips
        .iter()
        .filter_map(|c| c.frame.as_ref())
        .collect();
    let shown: HashSet<&NodeId> = in_clips
        .iter()
        .copied()
        .chain(video.overlays.iter().map(|o| &o.frame))
        .collect();
    let mut out = HashMap::new();
    for frame in shown {
        let Some(node) = doc.node(frame) else {
            continue;
        };
        let mut ids = Vec::new();
        node.ids(&mut ids);
        let ids: HashSet<NodeId> = ids.into_iter().collect();
        let live = Liveness {
            animations: video
                .animations
                .iter()
                .enumerate()
                .filter(|(_, a)| ids.contains(&a.node))
                .map(|(i, _)| i)
                .collect(),
            camera: video
                .animations
                .iter()
                .enumerate()
                .filter(|(_, a)| a.prop.is_camera() && in_clips.contains(frame))
                .map(|(i, _)| i)
                .collect(),
            plays: plays_recording(node),
        };
        if live.plays || !live.animations.is_empty() || !live.camera.is_empty() {
            out.insert(frame.clone(), live);
        }
    }
    out
}

/// Whether a visible device under `node` (or `node` itself) shows a recording on its screen.
pub fn plays_recording(node: &Node) -> bool {
    (node.kind == NodeKind::Device3d
        && node.visible
        && node.device3d.as_ref().is_some_and(|d| d.footage.is_some()))
        || node.children.iter().any(plays_recording)
}

/// Whether drawing `video` needs recordings decoded: footage clips, or screens that play one in
/// a frame the video shows.
pub fn needs_footage(doc: &Document, video: &Video) -> bool {
    video.clips.iter().any(|c| c.footage.is_some())
        || video
            .clips
            .iter()
            .filter_map(|c| c.frame.as_ref())
            .chain(video.overlays.iter().map(|o| &o.frame))
            .filter_map(|f| doc.node(f))
            .any(plays_recording)
}

fn lerp(from: f64, to: f64, t: f32) -> f64 {
    from + (to - from) * f64::from(t)
}

/// Where and how big a picture is drawn, and how opaque.
#[derive(Clone, Copy)]
struct Pose {
    dx: f32,
    dy: f32,
    scale: f32,
    opacity: f32,
    /// Blur of the picture, as a sigma in pixels (blur dissolve).
    blur: f32,
    /// A mask over the picture (wipes, iris, glitch).
    reveal: Option<Reveal>,
}

/// Washes the picture towards white by `amount` (0 leaves it, 1 is all white).
fn flash_white(canvas: &mut Pixmap, amount: f32) {
    let mut paint = tiny_skia::Paint::default();
    paint.set_color(
        tiny_skia::Color::from_rgba(1.0, 1.0, 1.0, amount.clamp(0.0, 1.0))
            .unwrap_or(tiny_skia::Color::WHITE),
    );
    if let Some(rect) =
        tiny_skia::Rect::from_xywh(0.0, 0.0, canvas.width() as f32, canvas.height() as f32)
    {
        canvas.fill_rect(rect, &paint, Transform::identity(), None);
    }
}

impl Pose {
    const STILL: Pose = Pose {
        dx: 0.0,
        dy: 0.0,
        scale: 1.0,
        opacity: 1.0,
        blur: 0.0,
        reveal: None,
    };

    /// Combines another pose with this one (offsets add, scales and opacities multiply).
    fn then(&mut self, other: Pose) {
        self.dx += other.dx;
        self.dy += other.dy;
        self.scale *= other.scale;
        self.opacity *= other.opacity;
    }
}

/// The pose of an overlay at progress `p` of its entrance (`entering`) or exit: 0 is hidden, 1 is
/// fully shown. A slide moves a tenth of the picture; pop and zoom change the size by a fifth.
fn entrance_pose(kind: Entrance, p: f32, w: f32, h: f32, entering: bool) -> Pose {
    let mut pose = Pose::STILL;
    let away = 1.0 - p;
    // leaving, a slide goes the way its name says; arriving, it comes from the opposite side
    let sign = if entering { 1.0 } else { -1.0 };
    match kind {
        Entrance::None => {}
        Entrance::Fade => pose.opacity = p,
        Entrance::SlideUp => {
            pose.opacity = p;
            pose.dy = sign * away * 0.1 * h;
        }
        Entrance::SlideDown => {
            pose.opacity = p;
            pose.dy = -sign * away * 0.1 * h;
        }
        Entrance::SlideLeft => {
            pose.opacity = p;
            pose.dx = sign * away * 0.1 * w;
        }
        Entrance::SlideRight => {
            pose.opacity = p;
            pose.dx = -sign * away * 0.1 * w;
        }
        Entrance::Pop => {
            pose.opacity = p;
            pose.scale = 0.8 + 0.2 * p;
        }
        Entrance::Zoom => {
            pose.opacity = p;
            pose.scale = if entering {
                1.25 - 0.25 * p
            } else {
                1.0 + 0.25 * away
            };
        }
    }
    pose
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{
        Actor, ClipProperty, Motion, NewNode, NodeKind, Operation, Paint, Session, Transition,
    };

    fn solid(s: &mut Session, root: &NodeId, color: &str, w: f64, h: f64) -> NodeId {
        let mut n = NewNode::new(NodeKind::Frame);
        n.width = w;
        n.height = h;
        n.fill = Paint::parse(color).ok();
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root.clone(),
                index: None,
                node: n,
            }],
        )
        .unwrap()
        .created[0]
            .clone()
    }

    /// Two 100x200 frames, red and blue, in a 100x200 video of clips of 2 s each.
    fn two_clips() -> (Session, NodeId, NodeId, NodeId) {
        let mut s = Session::new(Document::new("v"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let red = solid(&mut s, &root, "#ff0000", 100.0, 200.0);
        let blue = solid(&mut s, &root, "#0000ff", 100.0, 200.0);
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "v".into(),
                    fps: 10,
                    width: 100,
                    height: 200,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        for frame in [&red, &blue] {
            s.apply(
                Actor::User,
                None,
                &[Operation::AddClip {
                    video: v.clone(),
                    frame: Some(frame.clone()),
                    footage: None,
                    duration_ms: 2000,
                    index: None,
                    transition: None,
                    motion: None,
                }],
            )
            .unwrap();
        }
        (s, v, red, blue)
    }

    fn set_transition(s: &mut Session, v: &NodeId, clip: usize, t: Transition) {
        let id = s.document().video(v).unwrap().clips[clip].id.clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::SetClip {
                video: v.clone(),
                clip: id,
                prop: ClipProperty::Transition(t),
            }],
        )
        .unwrap();
    }

    fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 3] {
        let d = p.pixel(x, y).unwrap();
        [d.red(), d.green(), d.blue()]
    }

    fn at(s: &Session, v: &NodeId, ms: u64) -> Pixmap {
        let doc = s.document();
        VideoRenderer::new(doc, doc.video(v).unwrap(), None)
            .frame_at(ms)
            .unwrap()
    }

    #[test]
    fn a_cut_shows_one_clip_then_the_next_and_the_end_holds_the_last_picture() {
        let (s, v, ..) = two_clips();
        let size = |p: &Pixmap| (p.width(), p.height());
        assert_eq!(size(&at(&s, &v, 0)), (100, 200));
        assert_eq!(px(&at(&s, &v, 0), 50, 100), [255, 0, 0]);
        assert_eq!(px(&at(&s, &v, 1999), 50, 100), [255, 0, 0]);
        assert_eq!(px(&at(&s, &v, 2000), 50, 100), [0, 0, 255]);
        assert_eq!(px(&at(&s, &v, 3999), 50, 100), [0, 0, 255]);
        assert_eq!(
            px(&at(&s, &v, 99_999), 50, 100),
            [0, 0, 255],
            "past the end"
        );
    }

    #[test]
    fn a_fade_blends_the_two_clips_across_the_overlap() {
        let (mut s, v, ..) = two_clips();
        set_transition(
            &mut s,
            &v,
            1,
            Transition {
                kind: TransitionKind::Fade,
                duration_ms: 1000,
                ease: od_core::Ease::default(),
            },
        );
        // clip 2 starts at 1000 ms and has fully arrived at 2000 ms
        assert_eq!(px(&at(&s, &v, 999), 5, 5), [255, 0, 0]);
        let mid = px(&at(&s, &v, 1500), 5, 5);
        assert!(
            mid[0].abs_diff(128) <= 3 && mid[2].abs_diff(127) <= 3,
            "{mid:?}"
        );
        assert_eq!(px(&at(&s, &v, 2000), 5, 5), [0, 0, 255]);
    }

    #[test]
    fn a_slide_pushes_the_old_clip_out_as_the_new_one_comes_in() {
        let (mut s, v, ..) = two_clips();
        set_transition(
            &mut s,
            &v,
            1,
            Transition {
                kind: TransitionKind::SlideLeft,
                duration_ms: 1000,
                ease: od_core::Ease::default(),
            },
        );
        let half = at(&s, &v, 1500);
        assert_eq!(px(&half, 10, 100), [255, 0, 0], "old clip on the left");
        assert_eq!(px(&half, 90, 100), [0, 0, 255], "new clip on the right");
        let start = at(&s, &v, 1000);
        assert_eq!(px(&start, 10, 100), [255, 0, 0]);
        assert_eq!(px(&start, 99, 100), [255, 0, 0], "nothing has moved yet");
    }

    #[test]
    fn a_zoom_grows_the_picture_over_the_clip() {
        let mut s = Session::new(Document::new("v"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let frame = solid(&mut s, &root, "#000000", 200.0, 200.0);
        let mut dot = NewNode::new(NodeKind::Rect);
        (dot.x, dot.y, dot.width, dot.height) = (90.0, 90.0, 20.0, 20.0);
        dot.fill = Paint::parse("#ffffff").ok();
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: frame.clone(),
                index: None,
                node: dot,
            }],
        )
        .unwrap();
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "z".into(),
                    fps: 10,
                    width: 200,
                    height: 200,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame: Some(frame),
                footage: None,
                duration_ms: 2000,
                index: None,
                transition: None,
                motion: Some(Motion {
                    scale_from: 1.0,
                    scale_to: 2.0,
                    ease: od_core::Ease::default(),
                }),
            }],
        )
        .unwrap();
        let white = |p: &Pixmap| {
            (0..200)
                .flat_map(|y| (0..200).map(move |x| (x, y)))
                .filter(|&(x, y)| px(p, x, y)[0] > 128)
                .count()
        };
        let (first, last) = (white(&at(&s, &v, 0)), white(&at(&s, &v, 1999)));
        assert!((first as i64 - 400).abs() < 40, "a 20x20 dot, got {first}");
        assert!(
            last > first * 3,
            "twice the zoom is about four times the area: {first} to {last}"
        );
    }

    #[test]
    fn a_frame_of_another_shape_covers_the_output_and_is_cropped_in_the_middle() {
        let mut s = Session::new(Document::new("v"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let tall = solid(&mut s, &root, "#00ff00", 100.0, 400.0);
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "wide".into(),
                    fps: 10,
                    width: 200,
                    height: 100,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame: Some(tall),
                footage: None,
                duration_ms: 1000,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
        let p = at(&s, &v, 0);
        assert_eq!((p.width(), p.height()), (200, 100));
        for (x, y) in [(0, 0), (199, 0), (0, 99), (199, 99), (100, 50)] {
            assert_eq!(px(&p, x, y), [0, 255, 0], "{x},{y}");
        }
    }

    #[test]
    fn an_empty_video_is_black() {
        let (mut s, v, ..) = two_clips();
        let ids: Vec<_> = s
            .document()
            .video(&v)
            .unwrap()
            .clips
            .iter()
            .map(|c| c.id.clone())
            .collect();
        for id in ids {
            s.apply(
                Actor::User,
                None,
                &[Operation::RemoveClip {
                    video: v.clone(),
                    clip: id,
                }],
            )
            .unwrap();
        }
        assert_eq!(px(&at(&s, &v, 0), 3, 3), [0, 0, 0]);
    }

    /// Footage that is one solid color per second of the file, to see which moment was asked for.
    struct Fake(Vec<(String, u64)>);
    impl FootageSource for Fake {
        fn frame(&mut self, file: &str, at_ms: u64, w: u32, h: u32) -> Result<Vec<u8>, String> {
            self.0.push((file.to_string(), at_ms));
            let shade = ((at_ms / 1000) * 40).min(255) as u8;
            Ok((0..w * h)
                .flat_map(|_| [shade, shade, shade, 255])
                .collect())
        }
    }

    fn footage_video() -> (Session, NodeId, NodeId) {
        let (mut s, v, red, _) = two_clips();
        let ids: Vec<_> = s
            .document()
            .video(&v)
            .unwrap()
            .clips
            .iter()
            .map(|c| c.id.clone())
            .collect();
        for id in ids {
            s.apply(
                Actor::User,
                None,
                &[Operation::RemoveClip {
                    video: v.clone(),
                    clip: id,
                }],
            )
            .unwrap();
        }
        s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame: None,
                footage: Some(od_core::Footage {
                    file: "media/talk.mp4".into(),
                    in_ms: 3000,
                }),
                duration_ms: 4000,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
        (s, v, red)
    }

    #[test]
    fn footage_plays_from_its_in_point_and_needs_a_source() {
        let (s, v, _) = footage_video();
        let doc = s.document();
        let video = doc.video(&v).unwrap();
        let mut fake = Fake(Vec::new());
        let mut r = VideoRenderer::new(doc, video, None).with_footage(&mut fake);
        // 1 s into the clip is 4 s into the file: shade 160
        assert_eq!(px(&r.frame_at(1000).unwrap(), 5, 5), [160, 160, 160]);
        assert_eq!(px(&r.frame_at(0).unwrap(), 5, 5), [120, 120, 120]);
        drop(r);
        assert_eq!(
            fake.0,
            [
                ("media/talk.mp4".to_string(), 4000),
                ("media/talk.mp4".to_string(), 3000)
            ]
        );
        let err = VideoRenderer::new(doc, video, None)
            .frame_at(0)
            .unwrap_err();
        assert!(err.to_string().contains("no footage source"), "{err}");
    }

    #[test]
    fn an_overlay_fades_in_and_out_over_the_footage_and_is_absent_outside_its_time() {
        let (mut s, v, red) = footage_video();
        // red, 100x200, drawn over the 100x200 video from 1 s to 3 s with 500 ms fades
        s.apply(
            Actor::User,
            None,
            &[Operation::AddOverlay {
                video: v.clone(),
                frame: red,
                start_ms: 1000,
                duration_ms: 2000,
                fade_ms: Some(500),
                enter: None,
                exit: None,
                ease: None,
            }],
        )
        .unwrap();
        let doc = s.document();
        let video = doc.video(&v).unwrap();
        let mut fake = Fake(Vec::new());
        let mut r = VideoRenderer::new(doc, video, None).with_footage(&mut fake);
        let under = 160; // the footage's shade 4.25 s into the file
        assert_eq!(
            px(&r.frame_at(500).unwrap(), 5, 5),
            [120, 120, 120],
            "before the overlay"
        );
        let half = px(&r.frame_at(1250).unwrap(), 5, 5);
        assert!(
            half[0] > 160 && half[0] < 255 && half[2] < 160,
            "half faded in over {under}: {half:?}"
        );
        assert_eq!(
            px(&r.frame_at(2000).unwrap(), 5, 5),
            [255, 0, 0],
            "fully shown"
        );
        assert_eq!(
            px(&r.frame_at(3500).unwrap(), 5, 5),
            [240, 240, 240],
            "after the overlay"
        );
    }

    #[test]
    fn the_signature_is_equal_while_the_picture_holds_and_differs_while_it_moves() {
        let (mut s, v, ..) = two_clips();
        set_transition(
            &mut s,
            &v,
            1,
            Transition {
                kind: TransitionKind::Fade,
                duration_ms: 1000,
                ease: od_core::Ease::default(),
            },
        );
        let doc = s.document();
        let r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
        assert_eq!(r.signature(100), r.signature(900), "clip one holds still");
        assert_ne!(r.signature(1100), r.signature(1200), "during the fade");
        assert_eq!(r.signature(2100), r.signature(3000), "clip two holds still");
        assert_ne!(r.signature(900), r.signature(2100), "different clips");
        // and a held moment really is the same picture
        let mut r2 = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
        assert_eq!(
            r2.frame_at(100).unwrap().data(),
            r2.frame_at(900).unwrap().data()
        );

        let (s, v, _) = footage_video();
        let doc = s.document();
        let r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
        assert_eq!(r.signature(500), None, "footage changes every frame");
    }

    fn with_overlay(enter: od_core::Entrance, ease: od_core::Ease) -> (Session, NodeId) {
        let (mut s, v, red) = footage_video();
        // a 40x40 red square drawn near the middle of an otherwise transparent 100x200 frame
        let mut card = NewNode::new(NodeKind::Frame);
        (card.width, card.height) = (100.0, 200.0);
        let card = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: s.document().pages[0].root.id.clone(),
                    index: None,
                    node: card,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let mut dot = NewNode::new(NodeKind::Rect);
        (dot.x, dot.y, dot.width, dot.height) = (30.0, 80.0, 40.0, 40.0);
        dot.fill = Paint::parse("#ff0000").ok();
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: card.clone(),
                index: None,
                node: dot,
            }],
        )
        .unwrap();
        let _ = red;
        s.apply(
            Actor::User,
            None,
            &[Operation::AddOverlay {
                video: v.clone(),
                frame: card,
                start_ms: 0,
                duration_ms: 3000,
                fade_ms: Some(1000),
                enter: Some(enter),
                exit: None,
                ease: Some(ease),
            }],
        )
        .unwrap();
        (s, v)
    }

    /// The rows of red (255,0,0)-dominant pixels: where the square is, top and bottom.
    fn red_rows(p: &Pixmap) -> Option<(u32, u32)> {
        let rows: Vec<u32> = (0..p.height())
            .filter(|&y| {
                (0..p.width()).any(|x| {
                    let c = px(p, x, y);
                    c[0] > 200 && c[1] < 60 && c[2] < 60
                })
            })
            .collect();
        Some((*rows.first()?, *rows.last()?))
    }

    #[test]
    fn an_overlay_that_slides_up_starts_lower_and_ends_in_place_and_pop_grows() {
        let mut fake = Fake(Vec::new());
        let (s, v) = with_overlay(od_core::Entrance::SlideUp, od_core::Ease::Linear);
        let doc = s.document();
        let mut r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None).with_footage(&mut fake);
        let settled = red_rows(&r.frame_at(1500).unwrap()).unwrap();
        assert_eq!(settled, (80, 119), "the square's own place");
        let early = red_rows(&r.frame_at(900).unwrap()).unwrap();
        assert!(
            early.0 > settled.0,
            "still below its place at 0.9 of the entrance: {early:?}"
        );
        drop(r);

        let mut fake = Fake(Vec::new());
        let (s, v) = with_overlay(od_core::Entrance::Pop, od_core::Ease::Linear);
        let doc = s.document();
        let mut r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None).with_footage(&mut fake);
        let small = red_rows(&r.frame_at(800).unwrap()).unwrap();
        let full = red_rows(&r.frame_at(1500).unwrap()).unwrap();
        assert!(
            small.1 - small.0 < full.1 - full.0,
            "grows to full size: {small:?} then {full:?}"
        );
    }

    #[test]
    fn a_slide_up_transition_brings_the_new_clip_from_below_and_a_dip_passes_through_black() {
        let (mut s, v, ..) = two_clips();
        set_transition(
            &mut s,
            &v,
            1,
            Transition {
                kind: TransitionKind::SlideUp,
                duration_ms: 1000,
                ease: od_core::Ease::Linear,
            },
        );
        let half = at(&s, &v, 1500);
        assert_eq!(
            px(&half, 50, 10),
            [255, 0, 0],
            "the old clip is pushed up and out"
        );
        assert_eq!(
            px(&half, 50, 190),
            [0, 0, 255],
            "the new clip comes up from below"
        );

        set_transition(
            &mut s,
            &v,
            1,
            Transition {
                kind: TransitionKind::Dip,
                duration_ms: 1000,
                ease: od_core::Ease::Linear,
            },
        );
        assert_eq!(
            px(&at(&s, &v, 1500), 50, 100),
            [0, 0, 0],
            "black in the middle"
        );
        let early = px(&at(&s, &v, 1250), 50, 100);
        assert!(
            early[0] > 100 && early[2] == 0,
            "the old clip fading out: {early:?}"
        );
        let late = px(&at(&s, &v, 1750), 50, 100);
        assert!(
            late[2] > 100 && late[0] == 0,
            "the new clip fading in: {late:?}"
        );
    }

    /// A 100x100 black frame with a white 20x20 square at x 0 whose x is keyed from 0 to 80
    /// over the first second of a 2 s clip at 10 fps.
    fn sliding_square(blur: u32) -> (Session, NodeId) {
        let mut s = Session::new(Document::new("v"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let frame = solid(&mut s, &root, "#000000", 100.0, 100.0);
        let mut sq = NewNode::new(NodeKind::Rect);
        (sq.y, sq.width, sq.height) = (40.0, 20.0, 20.0);
        sq.fill = Paint::parse("#ffffff").ok();
        let square = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: frame.clone(),
                    index: None,
                    node: sq,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "a".into(),
                    fps: 10,
                    width: 100,
                    height: 100,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let key = |at_ms, value| od_core::Key {
            at_ms,
            value,
            ease: od_core::Ease::Linear,
        };
        s.apply(
            Actor::User,
            None,
            &[
                Operation::AddClip {
                    video: v.clone(),
                    frame: Some(frame),
                    footage: None,
                    duration_ms: 2000,
                    index: None,
                    transition: None,
                    motion: None,
                },
                Operation::Animate {
                    video: v.clone(),
                    node: square,
                    prop: od_core::AnimProp::X,
                    keys: vec![key(0, 0.0), key(1000, 80.0)],
                    repeat: od_core::Repeat::Once,
                },
                Operation::SetVideo {
                    id: v.clone(),
                    prop: od_core::VideoProperty::MotionBlur(blur),
                },
            ],
        )
        .unwrap();
        (s, v)
    }

    /// Pixels on the square's row that are neither black nor white: the smear of motion blur.
    fn smeared(p: &Pixmap) -> usize {
        (0..100)
            .filter(|&x| {
                let r = px(p, x, 50)[0];
                (20..=235).contains(&r)
            })
            .count()
    }

    #[test]
    fn a_keyframed_node_moves_with_the_video_and_holds_after_its_last_key() {
        let (s, v) = sliding_square(0);
        assert_eq!(px(&at(&s, &v, 0), 10, 50), [255, 255, 255]);
        assert_eq!(px(&at(&s, &v, 0), 90, 50), [0, 0, 0]);
        assert_eq!(px(&at(&s, &v, 500), 50, 50), [255, 255, 255], "halfway");
        assert_eq!(
            px(&at(&s, &v, 1500), 90, 50),
            [255, 255, 255],
            "held at the end"
        );
        let doc = s.document();
        let r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
        assert_ne!(r.signature(400), r.signature(500), "moving moments differ");
        assert_eq!(
            r.signature(1200),
            r.signature(1800),
            "after the last key it holds still"
        );
    }

    #[test]
    fn motion_blur_smears_only_what_moves() {
        let (sharp, v1) = sliding_square(0);
        let (blurred, v2) = sliding_square(8);
        assert_eq!(smeared(&at(&sharp, &v1, 500)), 0);
        let smear = smeared(&at(&blurred, &v2, 500));
        // 80 px a second, half a 10 fps frame of shutter: about 4 px of smear on each edge
        assert!((4..=12).contains(&smear), "smear of {smear} px");
        assert_eq!(
            at(&blurred, &v2, 1500).data(),
            at(&sharp, &v1, 1500).data(),
            "a still moment is not blurred"
        );
    }

    fn transition_of(kind: TransitionKind, ms: u32) -> Transition {
        Transition {
            kind,
            duration_ms: ms,
            ease: od_core::Ease::Linear,
        }
    }

    fn near(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tolerance)
    }

    const RED: [u8; 3] = [255, 0, 0];
    const BLUE: [u8; 3] = [0, 0, 255];

    #[test]
    fn wipes_uncover_the_new_clip_from_their_edge() {
        for (kind, new_at, old_at) in [
            (TransitionKind::WipeLeft, (75, 100), (25, 100)),
            (TransitionKind::WipeRight, (25, 100), (75, 100)),
            (TransitionKind::WipeUp, (50, 150), (50, 50)),
            (TransitionKind::WipeDown, (50, 50), (50, 150)),
        ] {
            let (mut s, v, ..) = two_clips();
            set_transition(&mut s, &v, 1, transition_of(kind, 1000));
            let p = at(&s, &v, 1500);
            assert_eq!(px(&p, new_at.0, new_at.1), BLUE, "{kind:?} new");
            assert_eq!(px(&p, old_at.0, old_at.1), RED, "{kind:?} old");
            assert_eq!(px(&at(&s, &v, 999), 50, 100), RED);
            assert_eq!(px(&at(&s, &v, 2000), 50, 100), BLUE);
        }
    }

    #[test]
    fn an_iris_opens_from_the_middle_of_the_picture() {
        let (mut s, v, ..) = two_clips();
        set_transition(&mut s, &v, 1, transition_of(TransitionKind::Iris, 1000));
        let p = at(&s, &v, 1300);
        assert_eq!(px(&p, 50, 100), BLUE);
        assert_eq!(px(&p, 1, 1), RED);
        assert_eq!(px(&at(&s, &v, 1900), 1, 1), RED, "corners go last");
    }

    #[test]
    fn a_flash_passes_through_white_between_the_clips() {
        let (mut s, v, ..) = two_clips();
        set_transition(&mut s, &v, 1, transition_of(TransitionKind::Flash, 1000));
        assert!(near(px(&at(&s, &v, 1250), 50, 100), [255, 128, 128], 2));
        assert!(near(px(&at(&s, &v, 1500), 50, 100), [255, 255, 255], 1));
        assert!(near(px(&at(&s, &v, 1750), 50, 100), [128, 128, 255], 2));
        assert_eq!(px(&at(&s, &v, 999), 50, 100), RED);
    }

    #[test]
    fn a_blur_dissolve_mixes_and_blurs_both_clips_and_a_glitch_repeats_exactly() {
        let (mut s, v, ..) = two_clips();
        set_transition(
            &mut s,
            &v,
            1,
            transition_of(TransitionKind::BlurDissolve, 1000),
        );
        let mid = at(&s, &v, 1500);
        assert!(
            near(px(&mid, 50, 100), [128, 0, 128], 3),
            "{:?}",
            px(&mid, 50, 100)
        );
        set_transition(&mut s, &v, 1, transition_of(TransitionKind::Fade, 1000));
        assert_eq!(
            mid.data(),
            at(&s, &v, 1500).data(),
            "blurring flat colour changes nothing, so it dissolves like a fade"
        );
        assert_eq!(mid.data(), at(&s, &v, 1500).data());

        set_transition(
            &mut s,
            &v,
            1,
            transition_of(TransitionKind::GlitchSlice, 1000),
        );
        let glitch = at(&s, &v, 1500);
        assert_eq!(glitch.data(), at(&s, &v, 1500).data());
        let blue = glitch
            .data()
            .chunks_exact(4)
            .filter(|c| c[2] == 255 && c[0] == 0)
            .count();
        assert!(
            blue > 0 && blue < 100 * 200,
            "part of the new clip shows: {blue}"
        );
    }

    fn set_clip_prop(s: &mut Session, v: &NodeId, prop: ClipProperty) {
        let id = s.document().video(v).unwrap().clips[0].id.clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::SetClip {
                video: v.clone(),
                clip: id,
                prop,
            }],
        )
        .unwrap();
    }

    #[test]
    fn footage_speed_reads_the_file_faster_or_slower_than_the_clip_plays() {
        let (mut s, v, _) = footage_video();
        set_clip_prop(&mut s, &v, ClipProperty::Speed(2.0));
        let doc = s.document();
        let video = doc.video(&v).unwrap();
        let mut fake = Fake(Vec::new());
        let mut r = VideoRenderer::new(doc, video, None).with_footage(&mut fake);
        // 1 s into the clip at 2x is 2 s into the file after the 3 s in point: shade 200
        assert_eq!(px(&r.frame_at(1000).unwrap(), 5, 5), [200, 200, 200]);
        drop(r);
        assert_eq!(fake.0, [("media/talk.mp4".to_string(), 5000)]);
        set_clip_prop(&mut s, &v, ClipProperty::Speed(0.5));
        let doc = s.document();
        let mut fake = Fake(Vec::new());
        let mut r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None).with_footage(&mut fake);
        r.frame_at(2000).unwrap();
        drop(r);
        assert_eq!(fake.0, [("media/talk.mp4".to_string(), 4000)]);
    }

    #[test]
    fn a_speed_ramp_reads_the_integral_of_its_rate() {
        let (mut s, v, _) = footage_video();
        let key = |at_ms, rate| od_core::SpeedKey {
            at_ms,
            rate,
            ease: od_core::Ease::Linear,
        };
        set_clip_prop(
            &mut s,
            &v,
            ClipProperty::SpeedRamp(vec![key(0, 1.0), key(2000, 3.0)]),
        );
        let doc = s.document();
        let mut fake = Fake(Vec::new());
        let mut r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None).with_footage(&mut fake);
        for ms in [0, 1000, 2000, 3000] {
            r.frame_at(ms).unwrap();
        }
        drop(r);
        // the rate is 1 + t/1000: 1.5 s of file after 1 s, 4 s after 2 s, then 3x
        let asked: Vec<u64> = fake.0.iter().map(|(_, t)| *t).collect();
        assert_eq!(asked, [3000, 4500, 7000, 10000]);
    }

    #[test]
    fn a_clip_grade_changes_the_picture_and_a_neutral_one_does_not() {
        let (mut s, v, ..) = two_clips();
        let plain = at(&s, &v, 500);
        set_clip_prop(
            &mut s,
            &v,
            ClipProperty::Grade(Some(od_core::Look::Mono.grade())),
        );
        let [r, g, b] = px(&at(&s, &v, 500), 50, 100);
        assert!(r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1, "{r} {g} {b}");
        assert_eq!(px(&at(&s, &v, 2500), 50, 100), BLUE, "only the graded clip");
        set_clip_prop(&mut s, &v, ClipProperty::Grade(None));
        assert_eq!(at(&s, &v, 500).data(), plain.data());
    }
}
