//! The preview of a video the window plays: every frame drawn small on a worker thread, kept in
//! memory as PNG, so playing never waits for a render (`docs/wiki/video-editing.md`). It draws
//! from a detached copy of the project ([`Source`]), one frame after the other so footage is
//! decoded as a stream, and draws a held picture once (see `VideoRenderer::signature`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use od_core::{NodeId, Video};
use od_media::{Ffmpeg, FootageDecoder};
use serde::Serialize;

use crate::tools::ToolError;
use crate::video_export::{Decoder, Source, has_footage};
use od_render::video::VideoRenderer;

/// How small and how much a preview is.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Longest side of a preview picture, in pixels.
    pub max_side: u32,
    /// Most pictures per second (a video with a lower rate keeps its own).
    pub max_fps: u32,
    /// Most bytes of encoded pictures kept; a held picture is counted once.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_side: 360,
            max_fps: 15,
            max_bytes: 256 << 20,
        }
    }
}

/// What a preview of one video consists of: the picture size, the rate and the number of
/// pictures, and how a moment maps to a picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub total: u64,
}

impl Plan {
    pub fn of(v: &Video, limits: &Limits) -> Plan {
        let fps = v.fps.clamp(1, limits.max_fps.max(1));
        let shrink =
            (f64::from(limits.max_side.max(16)) / f64::from(v.width.max(v.height))).min(1.0);
        Plan {
            width: ((f64::from(v.width) * shrink).round() as u32).max(1),
            height: ((f64::from(v.height) * shrink).round() as u32).max(1),
            fps,
            total: (v.duration_ms() * u64::from(fps)).div_ceil(1000),
        }
    }

    /// The moment picture `n` shows.
    pub fn time_ms(&self, n: u64) -> u64 {
        n * 1000 / u64::from(self.fps)
    }

    /// The picture that is on screen at `time_ms` (the last one from the end on).
    pub fn index_at(&self, time_ms: u64) -> u64 {
        (time_ms * u64::from(self.fps) / 1000).min(self.total.saturating_sub(1))
    }
}

/// How far a preview has got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// Pictures drawn, from the start without gaps.
    pub ready: u64,
    pub total: u64,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    /// The worker has finished (all pictures, the memory cap, an error or no clips).
    pub done: bool,
    /// Stopped early at the memory cap: only `ready` of `total` pictures exist.
    pub capped: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct Inner {
    frames: Vec<Arc<Vec<u8>>>,
    bytes: usize,
    done: bool,
    capped: bool,
    error: Option<String>,
}

struct Shared {
    cancel: AtomicBool,
    inner: Mutex<Inner>,
}

impl Shared {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The pictures of one video being drawn on another thread. Dropping it cancels the worker,
/// which stops before drawing its next picture.
pub struct Preview {
    plan: Plan,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Preview {
    /// Starts drawing `video` from the copy `src` of the project.
    pub fn start(src: Source, video: &NodeId, limits: Limits) -> Result<Preview, ToolError> {
        let v = src
            .doc
            .video(video)
            .ok_or_else(|| ToolError(format!("video {video} does not exist")))?;
        let plan = Plan::of(v, &limits);
        let shared = Arc::new(Shared {
            cancel: AtomicBool::new(false),
            inner: Mutex::new(Inner::default()),
        });
        let id = video.clone();
        let worker = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name("video-preview".into())
                .spawn(move || {
                    let result = fill(&src, &id, plan, &limits, &shared);
                    let mut inner = shared.lock();
                    if let Err(e) = result {
                        inner.error = Some(e);
                    }
                    if !shared.cancel.load(Ordering::Relaxed) {
                        inner.done = true;
                    }
                })
                .map_err(|e| ToolError(format!("cannot start the preview: {e}")))?
        };
        Ok(Preview {
            plan,
            shared,
            worker: Some(worker),
        })
    }

    pub fn plan(&self) -> Plan {
        self.plan
    }

    pub fn status(&self) -> Status {
        let inner = self.shared.lock();
        Status {
            ready: inner.frames.len() as u64,
            total: self.plan.total,
            fps: self.plan.fps,
            width: self.plan.width,
            height: self.plan.height,
            done: inner.done,
            capped: inner.capped,
            error: inner.error.clone(),
        }
    }

    /// The PNG of picture `index`, once it is drawn.
    pub fn frame(&self, index: u64) -> Option<Arc<Vec<u8>>> {
        let inner = self.shared.lock();
        inner.frames.get(usize::try_from(index).ok()?).cloned()
    }

    /// Stops the worker without waiting for it.
    pub fn cancel(&self) {
        self.shared.cancel.store(true, Ordering::Relaxed);
    }

    /// Waits for the worker to finish (for tests, and for callers off the UI thread).
    pub fn wait(&mut self) {
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.cancel();
    }
}

fn fill(
    src: &Source,
    id: &NodeId,
    plan: Plan,
    limits: &Limits,
    shared: &Shared,
) -> Result<(), String> {
    let v = src
        .doc
        .video(id)
        .ok_or_else(|| format!("video {id} does not exist"))?;
    // The decoder reads at the preview's rate, so it streams instead of seeking for each picture.
    let mut decoder = if has_footage(v) {
        let dir = src
            .dir()
            .ok_or("the project has no folder, so no footage")?;
        let ffmpeg = Ffmpeg::locate().map_err(|e| e.to_string())?;
        Some(Decoder(FootageDecoder::new(
            ffmpeg,
            dir.to_path_buf(),
            plan.fps,
        )))
    } else {
        None
    };
    let mut renderer =
        VideoRenderer::new(&src.doc, v, src.dir()).with_size(plan.width, plan.height);
    if let Some(d) = decoder.as_mut() {
        renderer = renderer.with_footage(d);
    }
    let mut held: Option<(Vec<i64>, Arc<Vec<u8>>)> = None;
    for n in 0..plan.total {
        if shared.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let time_ms = plan.time_ms(n);
        let sig = renderer.signature(time_ms);
        let (png, fresh) = match (&sig, &held) {
            (Some(s), Some((last, png))) if s == last => (png.clone(), false),
            _ => {
                let pixmap = renderer.frame_at(time_ms).map_err(|e| e.to_string())?;
                let png =
                    Arc::new(od_render::encode_png(&pixmap, false).map_err(|e| e.to_string())?);
                held = sig.map(|s| (s, png.clone()));
                (png, true)
            }
        };
        let mut inner = shared.lock();
        let add = if fresh { png.len() } else { 0 };
        if inner.bytes + add > limits.max_bytes {
            inner.capped = true;
            return Ok(());
        }
        inner.bytes += add;
        inner.frames.push(png);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use od_core::{
        Actor, Document, Motion, NewNode, NodeKind, Operation, Paint, Session, Transition,
        TransitionKind,
    };

    fn solid(s: &mut Session, root: &NodeId, color: &str) -> NodeId {
        let mut n = NewNode::new(NodeKind::Frame);
        n.width = 100.0;
        n.height = 200.0;
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

    /// A 30 fps, 1000x2000 video: a red clip of 2 s that holds still, then a blue one of 1 s
    /// that fades in over the last 500 ms of the red one.
    fn design_video() -> (Source, NodeId) {
        let mut s = Session::new(Document::new("v"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let red = solid(&mut s, &root, "#ff0000");
        let blue = solid(&mut s, &root, "#0000ff");
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "v".into(),
                    fps: 30,
                    width: 1000,
                    height: 2000,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        for (frame, ms, transition) in [
            (&red, 2000, None),
            (
                &blue,
                1000,
                Some(Transition {
                    kind: TransitionKind::Fade,
                    duration_ms: 500,
                    ease: Default::default(),
                }),
            ),
        ] {
            s.apply(
                Actor::User,
                None,
                &[Operation::AddClip {
                    video: v.clone(),
                    frame: Some(frame.clone()),
                    footage: None,
                    duration_ms: ms,
                    index: None,
                    transition,
                    motion: None::<Motion>,
                }],
            )
            .unwrap();
        }
        (
            Source {
                doc: s.document().clone(),
                dir: None,
            },
            v,
        )
    }

    #[test]
    fn the_plan_is_small_slow_and_maps_moments_to_pictures() {
        let (src, id) = design_video();
        let v = src.doc.video(&id).unwrap();
        let plan = Plan::of(v, &Limits::default());
        // the transition overlaps the first clip: 2.5 s at 15 fps
        assert_eq!((plan.width, plan.height, plan.fps), (180, 360, 15));
        assert_eq!(plan.total, 38, "2500 ms * 15 / 1000, rounded up");
        assert_eq!(plan.time_ms(15), 1000);
        assert_eq!(plan.index_at(0), 0);
        assert_eq!(plan.index_at(1000), 15);
        assert_eq!(
            plan.index_at(999_999),
            37,
            "past the end is the last picture"
        );
        let slow = Limits {
            max_fps: 60,
            ..Limits::default()
        };
        assert_eq!(Plan::of(v, &slow).fps, 30, "never faster than the video");
    }

    #[test]
    fn every_picture_is_drawn_held_ones_once_and_moving_ones_differ() {
        let (src, id) = design_video();
        let mut p = Preview::start(src, &id, Limits::default()).unwrap();
        p.wait();
        let st = p.status();
        assert!(st.done && !st.capped && st.error.is_none(), "{st:?}");
        assert_eq!(st.ready, st.total);
        // the red clip holds: its pictures are the same allocation
        let a = p.frame(1).unwrap();
        let b = p.frame(20).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "a held picture is drawn once");
        // the fade changes every picture
        // (the blue clip starts at 1500 ms and fades in until 2000 ms)
        let f1 = p.frame(p.plan().index_at(1600)).unwrap();
        let f2 = p.frame(p.plan().index_at(1800)).unwrap();
        assert_ne!(*f1, *f2, "the picture moves during the transition");
        assert_ne!(*a, *p.frame(st.total - 1).unwrap(), "red then blue");
        assert!(p.frame(st.total).is_none());
        assert!(a.starts_with(b"\x89PNG"));
    }

    #[test]
    fn cancelling_stops_the_worker_before_it_draws_everything() {
        let (src, id) = design_video();
        let mut p = Preview::start(src, &id, Limits::default()).unwrap();
        p.cancel();
        p.wait();
        let st = p.status();
        // the flag is checked before each picture, so a cancel never ends as complete
        assert!(!st.done, "{st:?}");
        assert!(st.ready < st.total, "{st:?}");
        // dropping a running preview cancels it too
        let (src, id) = design_video();
        let p = Preview::start(src, &id, Limits::default()).unwrap();
        let shared = p.shared.clone();
        drop(p);
        for _ in 0..400 {
            if Arc::strong_count(&shared) == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(Arc::strong_count(&shared), 1, "the worker ended");
        assert!(!shared.lock().done);
    }

    #[test]
    fn the_memory_cap_stops_the_preview_and_says_so() {
        let (src, id) = design_video();
        let limits = Limits {
            max_bytes: 1500,
            ..Limits::default()
        };
        let mut p = Preview::start(src, &id, limits).unwrap();
        p.wait();
        let st = p.status();
        assert!(st.done && st.capped, "{st:?}");
        assert!(st.ready > 0 && st.ready < st.total, "{st:?}");
        assert!(p.frame(st.ready).is_none());
    }

    #[test]
    fn a_missing_video_or_an_empty_one_is_handled() {
        let (src, _) = design_video();
        let missing = NodeId::parse("v_999").unwrap();
        assert!(Preview::start(src, &missing, Limits::default()).is_err());
        let mut s = Session::new(Document::new("e"));
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "e".into(),
                    fps: 10,
                    width: 100,
                    height: 100,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        let src = Source {
            doc: s.document().clone(),
            dir: None,
        };
        let mut p = Preview::start(src, &v, Limits::default()).unwrap();
        p.wait();
        let st = p.status();
        assert!(st.done && st.total == 0 && st.ready == 0);
    }

    #[test]
    fn footage_streams_into_the_preview_when_ffmpeg_is_there() {
        let Ok(ff) = Ffmpeg::locate() else {
            eprintln!("skipped: no ffmpeg on this machine");
            return;
        };
        if !ff.has_encoder("mpeg4") {
            eprintln!("skipped: {} has no mpeg4 encoder", ff.version);
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("media")).unwrap();
        let out = std::process::Command::new(&ff.path)
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi"])
            .args(["-i", "testsrc2=size=320x240:rate=10:duration=2"])
            .args(["-c:v", "mpeg4"])
            .arg(tmp.path().join("media/clip.mp4"))
            .output()
            .unwrap();
        assert!(out.status.success());
        let mut s = Session::new(Document::new("f"));
        let v = s
            .apply(
                Actor::User,
                None,
                &[Operation::CreateVideo {
                    name: "f".into(),
                    fps: 10,
                    width: 160,
                    height: 120,
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
                    file: "media/clip.mp4".into(),
                    in_ms: 0,
                }),
                duration_ms: 1500,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
        let src = Source {
            doc: s.document().clone(),
            dir: Some(tmp.path().to_path_buf()),
        };
        let mut p = Preview::start(src, &v, Limits::default()).unwrap();
        p.wait();
        let st = p.status();
        assert!(st.done && st.error.is_none(), "{st:?}");
        assert_eq!((st.ready, st.total), (15, 15));
        assert_ne!(*p.frame(0).unwrap(), *p.frame(10).unwrap(), "footage moves");
    }
}
