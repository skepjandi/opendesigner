//! A whole video drawn frame by frame on several threads, delivered in order
//! (`docs/wiki/video-editing.md#parallel-export`).
//!
//! A frame is a pure function of the document and its time (`od_core::timebase::frame_ms`), so
//! frames can be drawn in any order and still come out the same. Each worker thread owns a
//! [`VideoRenderer`] and, for footage, its own decoder from the `new_source` factory; the
//! decoders are sequential streams, so work is handed out in small runs of consecutive frames
//! and a decoder is started (by seeking) at the start of each run. Run boundaries depend only on
//! the video, never on the number of threads, so any thread count draws the same pixels.
//!
//! The calling thread receives the pictures in order from a small reorder window and gives them
//! to the sink (the encoder); a worker waits when it is `window` pictures ahead of the sink, so
//! memory stays bounded however long the video is. While nothing moves (`signature` is equal)
//! a picture is drawn once and delivered again.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use od_core::timebase;
use od_core::{Document, Video};

use crate::video::{DrawnFrame, FootageSource, VideoRenderer, needs_footage};
use crate::{RenderError, Warning};

/// Makes one footage source per worker (none for a video without footage).
pub type SourceFactory<'a> = dyn Fn() -> Option<Box<dyn FootageSource + Send>> + Sync + 'a;

/// Where the pictures of an export go: the frame number and its RGBA pixels. It answers
/// `Ok(true)` to go on, `Ok(false)` to stop and `Err` to fail the export.
pub type Sink<'a> = dyn FnMut(u64, &[u8]) -> Result<bool, String> + 'a;

/// The most worker threads, however many cores there are.
pub const MAX_WORKERS: usize = 8;
/// Pictures a worker draws in a row for footage, so a decoder is started once per run.
const FOOTAGE_RUN: usize = 4;
/// Most bytes of pictures waiting for the sink.
const WINDOW_BYTES: usize = 256 << 20;

/// How many threads to draw with: the cores the program may use, at most [`MAX_WORKERS`].
pub fn default_workers() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .clamp(1, MAX_WORKERS)
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error(transparent)]
    Render(#[from] RenderError),
    /// What the sink said went wrong.
    #[error("{0}")]
    Sink(String),
}

/// What an export drew.
#[derive(Debug, Default)]
pub struct Drawn {
    /// Pictures delivered to the sink, held pictures included.
    pub frames: u64,
    /// Pictures actually drawn.
    pub drawn: u64,
    /// The sink asked to stop.
    pub stopped: bool,
    pub warnings: Vec<Warning>,
}

#[derive(Default)]
struct Shared {
    ready: HashMap<usize, Vec<u8>>,
    /// Pictures the sink has taken (the window starts here).
    taken: usize,
    stop: bool,
    /// The first picture (lowest index) that could not be drawn.
    failed: Option<(usize, RenderError)>,
    warnings: Vec<Warning>,
}

struct Window {
    state: Mutex<Shared>,
    changed: Condvar,
}

impl Window {
    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until picture `index` fits the window; `false` when the export is over.
    fn admit(&self, index: usize, size: usize) -> bool {
        let mut s = self.lock();
        while !s.stop && index >= s.taken + size {
            s = self.changed.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
        !s.stop
    }

    fn deposit(&self, index: usize, pixels: Vec<u8>) {
        self.lock().ready.insert(index, pixels);
        self.changed.notify_all();
    }

    fn fail(&self, index: usize, error: RenderError) {
        let mut s = self.lock();
        if s.failed.as_ref().is_none_or(|(at, _)| index < *at) {
            s.failed = Some((index, error));
        }
        drop(s);
        self.changed.notify_all();
    }

    fn halt(&self) {
        self.lock().stop = true;
        self.changed.notify_all();
    }

    /// The pixels of picture `index`, waiting for a worker to draw them.
    fn take(&self, index: usize) -> Result<Vec<u8>, RenderError> {
        let mut s = self.lock();
        loop {
            if let Some(pixels) = s.ready.remove(&index) {
                s.taken = index + 1;
                drop(s);
                self.changed.notify_all();
                return Ok(pixels);
            }
            if let Some((_, e)) = s.failed.take() {
                return Err(e);
            }
            s = self.changed.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
    }
}

/// Which frames need drawing: the first, and every one whose picture differs from the one
/// before it (see [`VideoRenderer::signature`]). The others repeat the last drawn one.
fn frames_to_draw(doc: &Document, video: &Video, dir: Option<&Path>) -> Vec<bool> {
    let planner = VideoRenderer::new(doc, video, dir);
    let mut last: Option<Vec<i64>> = None;
    (0..video.frame_count())
        .map(|n| {
            let sig = planner.signature(timebase::frame_ms(n, video.fps));
            let fresh = sig.is_none() || sig != last;
            last = sig;
            fresh
        })
        .collect()
}

struct Plan<'a> {
    doc: &'a Document,
    video: &'a Video,
    dir: Option<&'a Path>,
    /// The design frames drawn so far, shared by the workers.
    cache: Arc<Mutex<Vec<DrawnFrame>>>,
    /// Frame numbers that need drawing, in order.
    draw: Vec<u64>,
    run: usize,
    window: usize,
    next_run: AtomicUsize,
}

fn worker(plan: &Plan, new_source: &SourceFactory, shared: &Window) {
    let mut source = new_source();
    let mut renderer =
        VideoRenderer::new(plan.doc, plan.video, plan.dir).with_shared_cache(plan.cache.clone());
    if let Some(s) = source.as_deref_mut() {
        renderer = renderer.with_footage(s);
    }
    'runs: loop {
        let first = plan.next_run.fetch_add(1, Ordering::Relaxed) * plan.run;
        if first >= plan.draw.len() {
            break;
        }
        for index in first..(first + plan.run).min(plan.draw.len()) {
            if !shared.admit(index, plan.window) {
                break 'runs;
            }
            let at = timebase::frame_ms(plan.draw[index], plan.video.fps);
            match renderer.frame_at(at) {
                Ok(picture) => shared.deposit(index, picture.take()),
                Err(e) => {
                    shared.fail(index, e);
                    break 'runs;
                }
            }
        }
    }
    let mut s = shared.lock();
    for w in renderer.warnings() {
        if !s.warnings.contains(w) {
            s.warnings.push(w.clone());
        }
    }
}

/// Draws every frame of `video` with `workers` threads and gives each, in order, to `sink`
/// (frame number and RGBA pixels). The sink answers `Ok(true)` to go on, `Ok(false)` to stop
/// and `Err` to fail the export.
pub fn export_frames(
    doc: &Document,
    video: &Video,
    dir: Option<&Path>,
    new_source: &SourceFactory,
    workers: usize,
    sink: &mut Sink<'_>,
) -> Result<Drawn, ExportError> {
    let fresh = frames_to_draw(doc, video, dir);
    let draw: Vec<u64> = (0u64..)
        .zip(&fresh)
        .filter(|(_, f)| **f)
        .map(|(n, _)| n)
        .collect();
    let workers = workers.clamp(1, MAX_WORKERS).min(draw.len().max(1));
    let run = if needs_footage(doc, video) {
        FOOTAGE_RUN
    } else {
        1
    };
    let frame_bytes = video.width as usize * video.height as usize * 4;
    let window = (WINDOW_BYTES / frame_bytes.max(1)).clamp(workers + 1, workers * run * 3 + 1);
    let plan = Plan {
        doc,
        video,
        dir,
        cache: Arc::new(Mutex::new(Vec::new())),
        draw,
        run,
        window,
        next_run: AtomicUsize::new(0),
    };
    let shared = Window {
        state: Mutex::default(),
        changed: Condvar::new(),
    };
    let mut report = Drawn {
        drawn: plan.draw.len() as u64,
        ..Drawn::default()
    };
    let outcome = std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| worker(&plan, new_source, &shared));
        }
        let result = deliver(&fresh, &shared, sink, &mut report);
        shared.halt();
        result
    });
    outcome?;
    report.warnings = std::mem::take(&mut shared.lock().warnings);
    report.warnings.sort_by_key(ToString::to_string);
    Ok(report)
}

fn deliver(
    fresh: &[bool],
    shared: &Window,
    sink: &mut Sink<'_>,
    report: &mut Drawn,
) -> Result<(), ExportError> {
    let mut current: Vec<u8> = Vec::new();
    let mut index = 0;
    for (n, &new) in fresh.iter().enumerate() {
        if new {
            current = shared.take(index)?;
            index += 1;
        }
        report.frames += 1;
        match sink(n as u64, &current) {
            Ok(true) => {}
            Ok(false) => {
                report.stopped = true;
                return Ok(());
            }
            Err(e) => return Err(ExportError::Sink(e)),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{Actor, ClipProperty, NewNode, NodeId, NodeKind, Operation, Paint, Session};

    fn frame(s: &mut Session, root: &NodeId, color: &str) -> NodeId {
        let mut n = NewNode::new(NodeKind::Frame);
        n.width = 64.0;
        n.height = 48.0;
        n.fill = Paint::parse(color).ok();
        let mut inner = NewNode::new(NodeKind::Rect);
        inner.width = 20.0;
        inner.height = 20.0;
        inner.fill = Paint::parse("#ffffff").ok();
        n.children.push(inner);
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

    /// Three clips with transitions, a zoom, an overlay and a grade, 24 fps (frames that do not
    /// land on whole milliseconds) so time mapping is exercised.
    fn busy_video() -> Session {
        let mut s = Session::new(Document::new("v"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let colors = ["#ff0000", "#00ff00", "#0000ff"];
        let frames: Vec<NodeId> = colors.iter().map(|c| frame(&mut s, &root, c)).collect();
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "v".into(),
                    fps: 24,
                    width: 64,
                    height: 48,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let kinds = [
            od_core::TransitionKind::Cut,
            od_core::TransitionKind::WipeLeft,
            od_core::TransitionKind::GlitchSlice,
        ];
        for (f, kind) in frames.iter().zip(kinds) {
            let transition =
                (kind != od_core::TransitionKind::Cut).then_some(od_core::Transition {
                    kind,
                    duration_ms: 500,
                    ease: od_core::Ease::EaseInOut,
                });
            s.apply(
                Actor::User,
                None,
                &[Operation::AddClip {
                    video: v.clone(),
                    frame: Some(f.clone()),
                    footage: None,
                    duration_ms: 1000,
                    index: None,
                    transition,
                    motion: Some(od_core::Motion {
                        scale_from: 1.0,
                        scale_to: 1.3,
                        ease: od_core::Ease::Linear,
                    }),
                }],
            )
            .unwrap();
        }
        let first = s.document().video(&v).unwrap().clips[0].id.clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::SetClip {
                video: v.clone(),
                clip: first,
                prop: ClipProperty::Grade(Some(od_core::Look::Punchy.grade())),
            }],
        )
        .unwrap();
        s
    }

    fn collect(s: &Session, workers: usize) -> Vec<(u64, Vec<u8>)> {
        let doc = s.document();
        let video = &doc.videos[0];
        let mut out = Vec::new();
        let none: &SourceFactory = &|| None;
        let drawn = export_frames(doc, video, None, none, workers, &mut |n, px| {
            out.push((n, px.to_vec()));
            Ok(true)
        })
        .unwrap();
        assert_eq!(drawn.frames, video.frame_count());
        out
    }

    fn sequential(s: &Session) -> Vec<(u64, Vec<u8>)> {
        let doc = s.document();
        let video = &doc.videos[0];
        let mut renderer = VideoRenderer::new(doc, video, None);
        (0..video.frame_count())
            .map(|n| {
                let at = timebase::frame_ms(n, video.fps);
                (n, renderer.frame_at(at).unwrap().data().to_vec())
            })
            .collect()
    }

    #[test]
    fn any_thread_count_gives_exactly_the_pictures_of_a_plain_loop() {
        let s = busy_video();
        let expected = sequential(&s);
        for workers in [1, 2, 3, 8] {
            assert!(collect(&s, workers) == expected, "{workers} workers");
        }
    }

    #[test]
    fn held_pictures_are_drawn_once_and_repeated() {
        let mut s = Session::new(Document::new("v"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let f = frame(&mut s, &root, "#336699");
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "v".into(),
                    fps: 30,
                    width: 64,
                    height: 48,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v,
                frame: Some(f),
                footage: None,
                duration_ms: 2000,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
        let doc = s.document();
        let none: &SourceFactory = &|| None;
        let drawn =
            export_frames(doc, &doc.videos[0], None, none, 4, &mut |_, _| Ok(true)).unwrap();
        assert_eq!((drawn.frames, drawn.drawn), (60, 1));
    }

    #[test]
    fn the_sink_can_stop_the_export_or_fail_it() {
        let s = busy_video();
        let doc = s.document();
        let none: &SourceFactory = &|| None;
        let mut seen = 0;
        let stopped = export_frames(doc, &doc.videos[0], None, none, 4, &mut |_, _| {
            seen += 1;
            Ok(seen < 5)
        })
        .unwrap();
        assert!(stopped.stopped && stopped.frames == 5);
        let failed = export_frames(doc, &doc.videos[0], None, none, 4, &mut |n, _| {
            if n == 3 {
                Err("disk full".into())
            } else {
                Ok(true)
            }
        });
        assert!(matches!(failed, Err(ExportError::Sink(m)) if m == "disk full"));
    }

    /// Footage that is a pure function of the moment asked for.
    struct Pattern;
    impl FootageSource for Pattern {
        fn frame(&mut self, _: &str, at_ms: u64, w: u32, h: u32) -> Result<Vec<u8>, String> {
            Ok((0..w * h)
                .flat_map(|i| {
                    let v = (at_ms as u32).wrapping_mul(31).wrapping_add(i * 7);
                    [v as u8, (v >> 8) as u8, (v >> 3) as u8, 255]
                })
                .collect())
        }
    }

    #[test]
    fn footage_drawn_by_several_decoders_matches_one_decoder() {
        let mut s = Session::new(Document::new("v"));
        s.add_page("p").unwrap();
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "v".into(),
                    fps: 25,
                    width: 32,
                    height: 32,
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
                frame: None,
                footage: Some(od_core::Footage {
                    file: "media/a.mp4".into(),
                    in_ms: 500,
                }),
                duration_ms: 2000,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
        let clip = s.document().video(&v).unwrap().clips[0].id.clone();
        s.apply(
            Actor::User,
            None,
            &[Operation::SetClip {
                video: v,
                clip,
                prop: ClipProperty::Speed(1.5),
            }],
        )
        .unwrap();
        let doc = s.document();
        let video = &doc.videos[0];
        let factory: &SourceFactory = &|| Some(Box::new(Pattern));
        let run = |workers| {
            let mut all = Vec::new();
            export_frames(doc, video, None, factory, workers, &mut |_, px| {
                all.push(px.to_vec());
                Ok(true)
            })
            .unwrap();
            all
        };
        let mut reference = Pattern;
        let mut renderer = VideoRenderer::new(doc, video, None).with_footage(&mut reference);
        let expected: Vec<Vec<u8>> = (0..video.frame_count())
            .map(|n| {
                let at = timebase::frame_ms(n, video.fps);
                renderer.frame_at(at).unwrap().data().to_vec()
            })
            .collect();
        for workers in [1, 4] {
            assert!(run(workers) == expected, "{workers} workers");
        }
    }
}
