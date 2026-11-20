//! The preview of a video the window plays: every picture drawn on a worker thread and kept in
//! memory, so playing never waits for a render (`docs/wiki/video-editing.md`). It draws from a
//! detached copy of the project ([`Source`]), one picture after the other so footage is decoded
//! as a stream.
//!
//! Three things keep it fast. Pictures are kept by what they were made from
//! ([`crate::preview_key`]), so after an edit only the pictures that changed are drawn and the
//! rest come from the preview being replaced ([`Reuse`]). A picture that holds still is drawn
//! once. And the size and rate are chosen from how fast this machine draws
//! ([`crate::preview_tier`]): a quick machine gets a sharper, smoother preview, within the
//! memory cap.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use od_core::{NodeId, Video};
use od_media::{Ffmpeg, FootageDecoder};
use serde::Serialize;

use crate::preview_frames::Frame;
use crate::preview_key::Keys;
use crate::preview_tier::{self, Cost, Speed, Tier};
use crate::tools::ToolError;
use crate::video_export::{Decoder, Source, has_footage};
use od_render::video::VideoRenderer;

/// The most a preview may be, and how much memory it may take.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Longest side of a picture, in pixels, at the sharpest tier.
    pub max_side: u32,
    /// Most pictures per second at the smoothest tier (a video with a lower rate keeps its own).
    pub max_fps: u32,
    /// Most bytes of pictures kept; a picture shown more than once is counted once.
    pub max_bytes: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_side: 960,
            max_fps: preview_tier::MAX_FPS,
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
    pub fn of(v: &Video, tier: Tier) -> Plan {
        Plan {
            width: tier.width,
            height: tier.height,
            fps: tier.fps,
            total: (v.duration_ms() * u64::from(tier.fps)).div_ceil(1000),
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
    /// Pictures ready, from the start without gaps.
    pub ready: u64,
    /// Pictures the preview will have; 0 until the worker has chosen the size and rate.
    pub total: u64,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    /// Of the ready pictures: drawn now, and taken from the preview this one replaced.
    pub drawn: u64,
    pub reused: u64,
    /// The worker has finished (all pictures, the memory cap, an error or no clips).
    pub done: bool,
    /// Stopped early at the memory cap: only `ready` of `total` pictures exist.
    pub capped: bool,
    pub error: Option<String>,
}

/// What a new preview may take from the one it replaces: its pictures by content key, and how
/// fast this machine drew them.
#[derive(Default)]
pub struct Reuse {
    frames: HashMap<u64, Arc<Frame>>,
    speed: Option<Speed>,
    /// The size and rate the last preview used: the next one keeps them while they stay
    /// affordable, so the kept pictures are still the right size.
    tier: Option<Tier>,
}

impl Reuse {
    /// Nothing to reuse and a known machine speed (the worker does not time one itself): for
    /// tests and callers that already measured.
    pub fn at_speed(ms_per_megapixel: f64) -> Reuse {
        Reuse {
            frames: HashMap::new(),
            speed: Some(Speed(ms_per_megapixel)),
            tier: None,
        }
    }

    pub fn pictures(&self) -> usize {
        self.frames.len()
    }
}

#[derive(Default)]
struct Inner {
    plan: Option<Plan>,
    frames: Vec<Arc<Frame>>,
    keys: Vec<u64>,
    bytes: usize,
    drawn: u64,
    reused: u64,
    speed: Option<Speed>,
    tier: Option<Tier>,
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
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl Preview {
    /// Starts drawing `video` from the copy `src` of the project, taking what it can from
    /// `reuse`.
    pub fn start(
        src: Source,
        video: &NodeId,
        limits: Limits,
        reuse: Reuse,
    ) -> Result<Preview, ToolError> {
        if src.doc.video(video).is_none() {
            return Err(ToolError(format!("video {video} does not exist")));
        }
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
                    let result = fill(&src, &id, &limits, reuse, &shared);
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
            shared,
            worker: Some(worker),
        })
    }

    /// The size, rate and length, once the worker has chosen them.
    pub fn plan(&self) -> Option<Plan> {
        self.shared.lock().plan
    }

    pub fn status(&self) -> Status {
        let inner = self.shared.lock();
        let plan = inner.plan.unwrap_or(Plan {
            width: 0,
            height: 0,
            fps: 0,
            total: 0,
        });
        Status {
            ready: inner.frames.len() as u64,
            total: plan.total,
            fps: plan.fps,
            width: plan.width,
            height: plan.height,
            drawn: inner.drawn,
            reused: inner.reused,
            done: inner.done,
            capped: inner.capped,
            error: inner.error.clone(),
        }
    }

    /// Picture `index`, once it is drawn.
    pub fn frame(&self, index: u64) -> Option<Arc<Frame>> {
        let inner = self.shared.lock();
        inner.frames.get(usize::try_from(index).ok()?).cloned()
    }

    /// What the preview that replaces this one may reuse.
    pub fn reusable(&self) -> Reuse {
        let inner = self.shared.lock();
        Reuse {
            frames: inner
                .keys
                .iter()
                .copied()
                .zip(inner.frames.iter().cloned())
                .collect(),
            speed: inner.speed,
            tier: inner.tier,
        }
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

/// How many different pictures `video` needs at `fps`: a held picture is one, a moving or
/// footage moment each its own.
fn unique_pictures(renderer: &VideoRenderer, video: &Video, fps: u32) -> u64 {
    let total = (video.duration_ms() * u64::from(fps)).div_ceil(1000);
    let mut unique = 0;
    let mut last: Option<Vec<i64>> = None;
    for n in 0..total {
        let sig = renderer.signature(n * 1000 / u64::from(fps));
        if sig.is_none() || sig != last {
            unique += 1;
        }
        last = sig;
    }
    unique
}

/// Times three pictures at the floor tier: the machine's speed in milliseconds per megapixel.
fn calibrate(
    renderer: &mut VideoRenderer,
    video: &Video,
    floor: Tier,
    footage: bool,
) -> Result<Speed, String> {
    let plan = Plan::of(video, floor);
    let moments: [u64; 3] = if footage {
        [0, 1, 2].map(|n| plan.time_ms(n.min(plan.total.saturating_sub(1))))
    } else {
        [0, plan.total / 3, 2 * plan.total / 3].map(|n| plan.time_ms(n))
    };
    let mut times = Vec::new();
    for ms in moments {
        let started = Instant::now();
        renderer.frame_at(ms).map_err(|e| e.to_string())?;
        times.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    // the first picture of footage also starts the decoder; the others show the steady rate
    let steady = if footage { &times[1..] } else { &times[..] };
    let slowest = steady.iter().copied().fold(0.0, f64::max);
    Ok(Speed(slowest / floor.megapixels().max(f64::EPSILON)))
}

fn fill(
    src: &Source,
    id: &NodeId,
    limits: &Limits,
    reuse: Reuse,
    shared: &Shared,
) -> Result<(), String> {
    let v = src
        .doc
        .video(id)
        .ok_or_else(|| format!("video {id} does not exist"))?;
    let footage = has_footage(&src.doc, v);
    let tiers = preview_tier::candidates(v.width, v.height, v.fps, limits.max_side, limits.max_fps);
    let floor = *tiers.first().ok_or("the video has no size")?;
    // The decoder reads at the preview's rate, so it streams instead of seeking for each picture.
    let make_decoder = |fps: u32| -> Result<Option<Decoder>, String> {
        if !footage {
            return Ok(None);
        }
        let dir = src
            .dir()
            .ok_or("the project has no folder, so no footage")?;
        let ffmpeg = Ffmpeg::locate().map_err(|e| e.to_string())?;
        Ok(Some(Decoder(FootageDecoder::new(
            ffmpeg,
            dir.to_path_buf(),
            fps,
        ))))
    };
    let mut speed = reuse.speed;
    if speed.is_none() && !v.clips.is_empty() {
        let mut decoder = make_decoder(floor.fps)?;
        let mut renderer =
            VideoRenderer::new(&src.doc, v, src.dir()).with_size(floor.width, floor.height);
        if let Some(d) = decoder.as_mut() {
            renderer = renderer.with_footage(d);
        }
        speed = Some(calibrate(&mut renderer, v, floor, footage)?);
    }
    let sizing = VideoRenderer::new(&src.doc, v, src.dir());
    let costs: Vec<Cost> = tiers
        .iter()
        .map(|&tier| Cost {
            tier,
            unique_pictures: unique_pictures(&sizing, v, tier.fps),
        })
        .collect();
    let tier = preview_tier::choose(&costs, speed, v.duration_ms(), limits.max_bytes, reuse.tier)
        .unwrap_or(floor);
    let plan = Plan::of(v, tier);
    {
        let mut inner = shared.lock();
        inner.plan = Some(plan);
        inner.tier = Some(tier);
    }

    let mut decoder = make_decoder(plan.fps)?;
    let mut renderer =
        VideoRenderer::new(&src.doc, v, src.dir()).with_size(plan.width, plan.height);
    if let Some(d) = decoder.as_mut() {
        renderer = renderer.with_footage(d);
    }
    let keys = Keys::new(src, v, (plan.width, plan.height, plan.fps));
    let mut known = reuse.frames;
    let mut counted: HashSet<u64> = HashSet::new();
    let (mut spent_ms, mut drawn_mpix) = (0.0, 0.0);
    let finish = |shared: &Shared, spent_ms: f64, drawn_mpix: f64| {
        if drawn_mpix > 0.0 {
            let measured = Speed(spent_ms / drawn_mpix);
            let mut inner = shared.lock();
            inner.speed = Some(preview_tier::steady(speed, measured));
        } else {
            shared.lock().speed = speed;
        }
    };
    for n in 0..plan.total {
        if shared.cancel.load(Ordering::Relaxed) {
            finish(shared, spent_ms, drawn_mpix);
            return Ok(());
        }
        let time_ms = plan.time_ms(n);
        let key = keys.key(time_ms, renderer.signature(time_ms).as_deref());
        let (picture, fresh) = match known.get(&key) {
            Some(p) => (p.clone(), false),
            None => {
                let started = Instant::now();
                let pixmap = renderer.frame_at(time_ms).map_err(|e| e.to_string())?;
                spent_ms += started.elapsed().as_secs_f64() * 1000.0;
                drawn_mpix += tier.megapixels();
                let picture = Arc::new(Frame::encode(&pixmap));
                known.insert(key, picture.clone());
                (picture, true)
            }
        };
        let mut inner = shared.lock();
        let add = if counted.insert(key) {
            picture.len()
        } else {
            0
        };
        if inner.bytes + add > limits.max_bytes {
            inner.capped = true;
            drop(inner);
            finish(shared, spent_ms, drawn_mpix);
            return Ok(());
        }
        inner.bytes += add;
        inner.frames.push(picture);
        inner.keys.push(key);
        if fresh {
            inner.drawn += 1;
        } else {
            inner.reused += 1;
        }
    }
    finish(shared, spent_ms, drawn_mpix);
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

    fn floor_tier() -> Reuse {
        Reuse::at_speed(1e9)
    }

    fn start(src: Source, id: &NodeId, limits: Limits, reuse: Reuse) -> Preview {
        Preview::start(src, id, limits, reuse).unwrap()
    }

    fn pixels(p: &Preview, index: u64) -> Vec<u8> {
        let mut out = Vec::new();
        p.frame(index).unwrap().decode_into(&mut out);
        out
    }

    fn all_pixels(p: &Preview) -> Vec<Vec<u8>> {
        (0..p.status().ready).map(|i| pixels(p, i)).collect()
    }

    #[test]
    fn the_plan_maps_moments_to_pictures() {
        let (src, id) = design_video();
        let v = src.doc.video(&id).unwrap();
        let tier = preview_tier::candidates(v.width, v.height, v.fps, 960, 60)[0];
        let plan = Plan::of(v, tier);
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
    }

    #[test]
    fn every_picture_is_drawn_held_ones_once_and_moving_ones_differ() {
        let (src, id) = design_video();
        let mut p = start(src, &id, Limits::default(), floor_tier());
        p.wait();
        let st = p.status();
        assert!(st.done && !st.capped && st.error.is_none(), "{st:?}");
        assert_eq!((st.width, st.height, st.fps), (180, 360, 15));
        assert_eq!(st.ready, st.total);
        // the red clip holds: its pictures are the same allocation
        let a = p.frame(1).unwrap();
        let b = p.frame(20).unwrap();
        assert!(Arc::ptr_eq(&a, &b), "a held picture is drawn once");
        assert!(st.drawn < st.total, "{st:?}");
        // the fade changes every picture
        // (the blue clip starts at 1500 ms and fades in until 2000 ms)
        let f1 = pixels(&p, p.plan().unwrap().index_at(1600));
        let f2 = pixels(&p, p.plan().unwrap().index_at(1800));
        assert_ne!(f1, f2, "the picture moves during the transition");
        assert_ne!(pixels(&p, 1), pixels(&p, st.total - 1), "red then blue");
        assert!(p.frame(st.total).is_none());
        assert_eq!(a.width * a.height * 4, pixels(&p, 1).len() as u32);
        assert_eq!(&pixels(&p, 1)[..4], &[255, 0, 0, 255], "premultiplied red");
    }

    #[test]
    fn a_fast_machine_gets_a_sharper_smoother_preview_and_a_slow_one_the_floor() {
        let (src, id) = design_video();
        let mut fast = start(src, &id, Limits::default(), Reuse::at_speed(0.001));
        fast.wait();
        let st = fast.status();
        assert_eq!((st.width, st.height, st.fps), (480, 960, 30), "{st:?}");
        assert_eq!(st.total, 75);
        assert!(st.done && st.error.is_none());
        let (src, id) = design_video();
        let capped = Limits {
            max_side: 360,
            max_fps: 15,
            ..Limits::default()
        };
        let mut limited = start(src, &id, capped, Reuse::at_speed(0.001));
        limited.wait();
        assert_eq!(
            (limited.status().width, limited.status().fps),
            (180, 15),
            "the limits are a ceiling"
        );
    }

    #[test]
    fn an_unknown_speed_is_measured_by_the_worker_and_handed_on() {
        let (src, id) = design_video();
        let mut p = start(src, &id, Limits::default(), Reuse::default());
        p.wait();
        let st = p.status();
        assert!(st.done && st.error.is_none() && st.total > 0, "{st:?}");
        assert!(
            p.reusable().speed.is_some(),
            "the next preview need not measure"
        );
    }

    #[test]
    fn after_an_edit_only_the_pictures_that_changed_are_drawn() {
        let (src, id) = design_video();
        let mut first = start(src, &id, Limits::default(), floor_tier());
        first.wait();
        let total = first.status().total;

        // nothing changed: everything comes from the old preview
        let (same, _) = design_video();
        let mut again = start(same, &id, Limits::default(), first.reusable());
        again.wait();
        let st = again.status();
        assert_eq!((st.drawn, st.reused, st.ready), (0, total, total), "{st:?}");
        assert_eq!(all_pixels(&again), all_pixels(&first));

        // the blue clip becomes green: the red clip's pictures are kept, the rest are drawn
        let (mut edited, _) = design_video();
        let blue = edited.doc.video(&id).unwrap().clips[1]
            .frame
            .clone()
            .unwrap();
        let loc = edited.doc.locate(&blue).unwrap();
        edited.doc.node_at_mut(&loc).unwrap().fill = Paint::parse("#00ff00").ok();
        let mut fresh = start(
            Source {
                doc: edited.doc.clone(),
                dir: None,
            },
            &id,
            Limits::default(),
            floor_tier(),
        );
        fresh.wait();
        let mut incremental = start(edited, &id, Limits::default(), first.reusable());
        incremental.wait();
        let st = incremental.status();
        assert!(st.reused > 0 && st.drawn > 0 && st.drawn < total, "{st:?}");
        assert_eq!(
            all_pixels(&incremental),
            all_pixels(&fresh),
            "reused pictures equal a full draw"
        );
    }

    #[test]
    fn a_shifted_clip_never_reuses_the_picture_of_its_old_place() {
        let (src, id) = design_video();
        let mut first = start(src, &id, Limits::default(), floor_tier());
        first.wait();
        // the red clip gets longer: everything after it moves, and so does the fade
        let (mut longer, _) = design_video();
        longer.doc.videos[0].clips[0].duration_ms = 2600;
        let source = |doc: &Document| Source {
            doc: doc.clone(),
            dir: None,
        };
        let mut fresh = start(source(&longer.doc), &id, Limits::default(), floor_tier());
        fresh.wait();
        let mut incremental = start(longer, &id, Limits::default(), first.reusable());
        incremental.wait();
        assert_eq!(all_pixels(&incremental), all_pixels(&fresh));
    }

    #[test]
    fn cancelling_stops_the_worker_before_it_draws_everything() {
        let (src, id) = design_video();
        let mut p = start(src, &id, Limits::default(), floor_tier());
        p.cancel();
        p.wait();
        let st = p.status();
        // the flag is checked before each picture, so a cancel never ends as complete
        assert!(!st.done, "{st:?}");
        assert!(st.ready < st.total, "{st:?}");
        // dropping a running preview cancels it too
        let (src, id) = design_video();
        let p = start(src, &id, Limits::default(), floor_tier());
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
            max_bytes: 6000,
            ..Limits::default()
        };
        let mut p = start(src, &id, limits, floor_tier());
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
        assert!(Preview::start(src, &missing, Limits::default(), Reuse::default()).is_err());
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
        let mut p = start(src, &v, Limits::default(), Reuse::default());
        p.wait();
        let st = p.status();
        assert!(st.done && st.total == 0 && st.ready == 0, "{st:?}");
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
        let mut p = start(src, &v, Limits::default(), floor_tier());
        p.wait();
        let st = p.status();
        assert!(st.done && st.error.is_none(), "{st:?}");
        assert_eq!((st.ready, st.total), (15, 15));
        assert_ne!(pixels(&p, 0), pixels(&p, 10), "footage moves");
    }
}
