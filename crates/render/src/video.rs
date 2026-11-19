//! Pictures of a video at any moment: the clip being shown, a slow zoom over it, and the
//! transition from the clip before (`docs/wiki/video-editing.md`).
//!
//! Each clip shows a design frame, drawn by [`crate::render_frame`] once at the scale its output
//! needs and reused for every moment of the clip.

use std::path::Path as FsPath;

use od_core::{Document, NodeId, Overlay, TransitionKind, Video};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::{RenderError, Warning, render_frame};

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

struct Drawn {
    frame: NodeId,
    scale_bits: u32,
    pixmap: Pixmap,
}

/// Draws the moments of one video.
pub struct VideoRenderer<'a> {
    doc: &'a Document,
    video: &'a Video,
    project_dir: Option<&'a FsPath>,
    /// The size of the pictures drawn, the video's own unless a smaller one was asked for.
    width: u32,
    height: u32,
    drawn: Vec<Drawn>,
    warnings: Vec<Warning>,
    footage: Option<&'a mut dyn FootageSource>,
}

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
            warnings: Vec::new(),
            footage: None,
        }
    }

    /// Lets clips of footage be drawn.
    pub fn with_footage(mut self, source: &'a mut dyn FootageSource) -> VideoRenderer<'a> {
        self.footage = Some(source);
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

    /// A value that is the same for two moments exactly when the picture is: while a clip of a
    /// design frame holds still (no zoom, no transition, no fade) every frame is identical, and
    /// an export can reuse the last one instead of drawing it again. `None` when the picture
    /// changes all the time (footage).
    pub fn signature(&self, time_ms: u64) -> Option<Vec<i64>> {
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
            if clip.footage.is_some() {
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
            }
        }
        Some(sig)
    }

    /// The picture at `time_ms` from the start. Past the end it is the last picture; an empty
    /// video is black.
    pub fn frame_at(&mut self, time_ms: u64) -> Result<Pixmap, RenderError> {
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
        let time = time_ms.min(last.end_ms.saturating_sub(1));
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
        let width = w as f32;
        for (n, p) in active.iter().enumerate() {
            let kind_of = |i: usize| self.video.clips[active[i].index].transition.kind;
            let (mut dx, mut opacity) = (0.0, 1.0);
            if let Some(t) = arriving[n] {
                match kind_of(n) {
                    TransitionKind::Fade => opacity = t,
                    TransitionKind::SlideLeft => dx = (1.0 - t) * width,
                    TransitionKind::SlideRight => dx = -(1.0 - t) * width,
                    TransitionKind::Cut => {}
                }
            }
            // The clip arriving after this one pushes it out the other way.
            if let Some(t) = arriving.get(n + 1).copied().flatten() {
                match kind_of(n + 1) {
                    TransitionKind::SlideLeft => dx -= t * width,
                    TransitionKind::SlideRight => dx += t * width,
                    _ => {}
                }
            }
            self.draw_clip(&mut canvas, p.index, time, dx, opacity)?;
        }
        let overlays: Vec<Overlay> = self.video.overlays.clone();
        for o in overlays
            .iter()
            .filter(|o| o.start_ms <= time && time < o.start_ms + u64::from(o.duration_ms))
        {
            let into = (time - o.start_ms) as f32;
            let left = (u64::from(o.duration_ms) - (time - o.start_ms)) as f32;
            let fade = o.fade_ms as f32;
            let opacity = if fade > 0.0 {
                (into / fade).min(left / fade).clamp(0.0, 1.0)
            } else {
                1.0
            };
            self.draw_overlay(&mut canvas, &o.frame, opacity)?;
        }
        Ok(canvas)
    }

    /// Draws a design frame over everything, scaled to cover and centered.
    fn draw_overlay(
        &mut self,
        canvas: &mut Pixmap,
        frame: &NodeId,
        opacity: f32,
    ) -> Result<(), RenderError> {
        let (w, h) = (self.width as f32, self.height as f32);
        let node = self
            .doc
            .node(frame)
            .ok_or_else(|| RenderError::NoSuchNode(frame.to_string()))?;
        let cover = (w / node.width as f32).max(h / node.height as f32);
        let drawn = self.drawn_frame(frame, cover)?;
        let (pw, ph) = (drawn.width() as f32, drawn.height() as f32);
        let paint = PixmapPaint {
            opacity,
            quality: FilterQuality::Nearest,
            ..PixmapPaint::default()
        };
        canvas.draw_pixmap(
            ((w - pw) / 2.0).round() as i32,
            ((h - ph) / 2.0).round() as i32,
            drawn.as_ref(),
            &paint,
            Transform::identity(),
            None,
        );
        Ok(())
    }

    /// Draws clip `index` as it looks at `time`, shifted `dx` pixels, onto `canvas`.
    fn draw_clip(
        &mut self,
        canvas: &mut Pixmap,
        index: usize,
        time: u64,
        dx: f32,
        opacity: f32,
    ) -> Result<(), RenderError> {
        let placed = self.video.placed();
        let p = placed[index];
        let clip = self.video.clips[index].clone();
        let t = (time.saturating_sub(p.start_ms)) as f32 / (p.end_ms - p.start_ms) as f32;
        let zoom = lerp(clip.motion.scale_from, clip.motion.scale_to, t.min(1.0)) as f32;
        let (w, h) = (self.width as f32, self.height as f32);
        let widest = clip.motion.scale_from.max(clip.motion.scale_to) as f32;
        // The picture is made at `widest` times the size that covers the video; shrink it to this
        // moment's zoom.
        let drawn: std::borrow::Cow<'_, Pixmap> = match (&clip.frame, &clip.footage) {
            (Some(frame), _) => {
                let node = self
                    .doc
                    .node(frame)
                    .ok_or_else(|| RenderError::NoSuchNode(frame.to_string()))?;
                let cover = (w / node.width as f32).max(h / node.height as f32);
                std::borrow::Cow::Borrowed(self.drawn_frame(frame, cover * widest)?)
            }
            (None, Some(f)) => {
                let at = f.in_ms + (time.saturating_sub(p.start_ms));
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
        let k = zoom / widest;
        let (pw, ph) = (drawn.width() as f32 * k, drawn.height() as f32 * k);
        let transform = Transform::from_row(k, 0.0, 0.0, k, dx + (w - pw) / 2.0, (h - ph) / 2.0);
        let quality = if (k - 1.0).abs() < 1e-4 && dx.fract() == 0.0 {
            FilterQuality::Nearest
        } else {
            FilterQuality::Bicubic
        };
        let paint = PixmapPaint {
            opacity,
            quality,
            ..PixmapPaint::default()
        };
        canvas.draw_pixmap(0, 0, drawn.as_ref().as_ref(), &paint, transform, None);
        Ok(())
    }

    /// The design frame drawn at `scale`, from the kept pictures or freshly.
    fn drawn_frame(&mut self, frame: &NodeId, scale: f32) -> Result<&Pixmap, RenderError> {
        let bits = scale.to_bits();
        if let Some(at) = self
            .drawn
            .iter()
            .position(|d| &d.frame == frame && d.scale_bits == bits)
        {
            // most recently used last
            let d = self.drawn.remove(at);
            self.drawn.push(d);
        } else {
            let rendered =
                render_frame(self.doc, frame, scale.clamp(0.01, 16.0), self.project_dir)?;
            for warning in rendered.warnings {
                if !self.warnings.contains(&warning) {
                    self.warnings.push(warning);
                }
            }
            if self.drawn.len() >= KEPT {
                self.drawn.remove(0);
            }
            self.drawn.push(Drawn {
                frame: frame.clone(),
                scale_bits: bits,
                pixmap: rendered.pixmap,
            });
        }
        self.drawn
            .last()
            .map(|d| &d.pixmap)
            .ok_or_else(|| RenderError::NoSuchNode(frame.to_string()))
    }
}

fn lerp(from: f64, to: f64, t: f32) -> f64 {
    from + (to - from) * f64::from(t.clamp(0.0, 1.0))
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
}
