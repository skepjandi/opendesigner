//! Pictures of a video at any moment: the clip being shown, a slow zoom over it, and the
//! transition from the clip before (`docs/wiki/video-editing.md`).
//!
//! Each clip shows a design frame, drawn by [`crate::render_frame`] once at the scale its output
//! needs and reused for every moment of the clip.

use std::path::Path as FsPath;

use od_core::{Clip, Document, NodeId, TransitionKind, Video};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

use crate::{RenderError, Warning, render_frame};

/// Frames kept drawn: a transition shows two clips and a second transition may start under it.
const KEPT: usize = 3;

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
        }
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
        Ok(canvas)
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
        let (frame_w, frame_h) = self.frame_size(&clip)?;
        let cover = (w / frame_w).max(h / frame_h);
        let scale = cover * widest;
        let drawn = self.drawn_frame(&clip.frame, scale)?;
        // The pixmap is drawn at `widest` times the cover scale; shrink it to this moment's zoom.
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
        canvas.draw_pixmap(0, 0, drawn.as_ref(), &paint, transform, None);
        Ok(())
    }

    fn frame_size(&self, clip: &Clip) -> Result<(f32, f32), RenderError> {
        let node = self
            .doc
            .node(&clip.frame)
            .ok_or_else(|| RenderError::NoSuchNode(clip.frame.to_string()))?;
        Ok((node.width as f32, node.height as f32))
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
                    frame: frame.clone(),
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
                frame,
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
                frame: tall,
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
}
