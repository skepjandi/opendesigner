//! A review of a whole video in numbers, for animation: is it a living scene or a picture with text
//! on it (`docs/wiki/agent-integration.md#review_video`). The document tells what each scene is
//! made of and what is animated (`od_core::scene`); a few rendered samples at a small size tell
//! how much of the picture really moves. Read-only, deterministic, and it never changes the
//! project.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashSet};
use std::hash::{Hash, Hasher};

use od_core::scene::{self, SceneStats};
use od_core::video::Placed;
use od_core::{Document, Node, NodeId, NodeKind, Paint, TrackRole, Video};
use od_render::Pixmap;
use od_render::video::VideoRenderer;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{CallCtx, Output, ToolError, node_id, parse, tool};
use crate::video_export::Source;

mod checks;
mod flash;
#[cfg(test)]
mod more_tests;
mod overlap;

pub const NAMES: [&str; 1] = ["review_video"];

/// Samples across the video when the caller does not choose.
const DEFAULT_SAMPLES: usize = 12;
const MIN_SAMPLES: usize = 8;
const MAX_SAMPLES: usize = 16;
/// The longest side of a sampled picture.
const SAMPLE_SIDE: f64 = 192.0;
/// A sample compares the picture with the one this much later.
const PROBE_MS: u64 = 250;
/// A channel must differ by more than this for a pixel to count as moving.
const PIXEL_DELTA: u8 = 8;
/// Less than this share of pixels moving is a still picture.
const STILL_SHARE: f64 = 0.02;
const MAX_STILL_MS: u64 = 2_500;
/// The end card may hold still.
const END_HOLD_MS: u64 = 2_000;
const MIN_SHAPES: usize = 12;
const MIN_MOVING_LAYERS: f64 = 3.0;
const CAMERA_AFTER_MS: u64 = 15_000;
const MAX_CAPTION_SHARE: f64 = 0.25;
const MAX_IMAGE_SHARE: f64 = 0.6;
/// Scenes shorter than this are not asked for ambient life or layers.
const SETTLED_MS: u64 = 2_500;
const MAX_LOOSE_COLORS: usize = 12;
const LOOSE_COLORS_WITHOUT_TOKENS: usize = 6;
const MIN_SAME_NODES: usize = 8;

pub fn definitions() -> Vec<Value> {
    vec![tool(
        "review_video",
        "An animation director's check of a whole video, in numbers, without changing anything. \
         Call it before export_video on any animated or illustrated video and fix what it finds. \
         It reads each scene's nodes and tracks and renders a few small pictures across the video to \
         measure motion. Findings have stable codes: RV001 a scene is one image with text on it, \
         RV002 only text and rectangles move, RV003 a still stretch over 2.5 s, RV004 no looping \
         ambient motion, RV005 no camera move in a video over 15 s, RV006 too few vector shapes, \
         RV007 repeated scene compositions, RV008 captions cover too much, RV009 no sound, RV010 the \
         sound is the same in every scene, RV011 colors are not from tokens, RV012 too few layers \
         moving, RV013 flash safety (over 3 large brightness changes in a second: an error that \
         makes the video not ready), RV014 cuts off the beat (needs `bpm`), RV015 text leaves before \
         it can be read, RV016 text in a vertical video's unsafe top or bottom zone, RV017 a 2 s \
         stretch with fewer than 3 moving layers, RV018 the same scene twice, RV019 no held last \
         picture. `overlap` lists regions where two layers of a scene overlap, back to front: check \
         each with video_crop and confirm which should be on top. It returns per-scene and \
         whole-video numbers, a score from 0 to 100 and the three fixes that would help most. A \
         score under 70 is not ready.",
        json!({
            "video": {"type": "string", "description": "Id of a video from list_videos; defaults to the first."},
            "samples": {"type": "integer", "minimum": MIN_SAMPLES, "maximum": MAX_SAMPLES, "description": "How many moments to render (default 12)."},
            "bpm": {"type": "number", "minimum": 30, "maximum": 300, "description": "The tempo of the music, to check that cuts land on a beat (RV014). Videos have no tempo of their own."},
            "allow_unevaluated": {"type": "boolean", "description": "Accept a gate that could not be evaluated (the beat without a tempo, the flash scan of a video with footage). The report says which gates were unevaluated; the video is not ready unless this is true."}
        }),
        &[],
        true,
    )]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    match name {
        "review_video" => {
            let a: Args = parse(args)?;
            let report = review_at_tempo_with(
                &Source::of(ctx.ws),
                a.video.as_deref(),
                a.samples,
                a.bpm,
                a.allow_unevaluated.unwrap_or(false),
            )?;
            Ok(Output::json(&report.to_json()))
        }
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    #[serde(default)]
    video: Option<String>,
    #[serde(default)]
    samples: Option<usize>,
    #[serde(default)]
    bpm: Option<f64>,
    #[serde(default)]
    allow_unevaluated: Option<bool>,
}

/// One thing wrong, with a stable code and what to do about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub code: &'static str,
    pub name: &'static str,
    /// `error` for what must not ship (photosensitivity), `warning` for the rest.
    pub severity: &'static str,
    /// Scenes (by position, from 0) the finding is about; empty for the whole video.
    pub scenes: Vec<usize>,
    pub message: String,
    pub fix: String,
    /// Points off the score when the finding covers the whole video.
    pub weight: f64,
    pub penalty: f64,
}

/// One gate of the review and what it found: `pass`, `fail` or `unevaluated`. A gate that could
/// not look at its input says so, and is never a pass.
#[derive(Debug, Clone, PartialEq)]
pub struct GateEntry {
    pub code: &'static str,
    pub status: &'static str,
    pub reason: String,
}

impl GateEntry {
    fn pass(code: &'static str) -> GateEntry {
        GateEntry {
            code,
            status: "pass",
            reason: String::new(),
        }
    }

    fn fail(code: &'static str, reason: impl Into<String>) -> GateEntry {
        GateEntry {
            code,
            status: "fail",
            reason: reason.into(),
        }
    }

    fn unevaluated(code: &'static str, reason: impl Into<String>) -> GateEntry {
        GateEntry {
            code,
            status: "unevaluated",
            reason: reason.into(),
        }
    }
}

/// A review: findings, numbers and a score.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub video: Option<NodeId>,
    pub message: Option<String>,
    pub score: u32,
    pub findings: Vec<Finding>,
    pub top_fixes: Vec<String>,
    pub scenes: Vec<Value>,
    pub metrics: Value,
    /// Regions where two layers of a scene overlap, back to front (`overlap.rs`); information.
    pub overlap: Vec<Value>,
    /// How many regions there were before the list was cut.
    pub overlap_total: usize,
    /// What each gate found; a gate that could not look is `unevaluated`, never a pass.
    pub gates: Vec<GateEntry>,
    /// The caller accepts a gate that could not be evaluated.
    pub allow_unevaluated: bool,
}

impl Report {
    fn empty(message: &str) -> Report {
        Report {
            video: None,
            message: Some(message.to_string()),
            score: 0,
            findings: Vec::new(),
            top_fixes: Vec::new(),
            scenes: Vec::new(),
            metrics: json!({}),
            overlap: Vec::new(),
            overlap_total: 0,
            gates: Vec::new(),
            allow_unevaluated: false,
        }
    }

    /// Whether the video can ship: no error, no failed gate, no gate left unevaluated (unless
    /// the caller says an unevaluated gate is fine) and a score of 80.
    pub fn ready(&self) -> bool {
        if self.findings.iter().any(|f| f.severity == "error") {
            return false;
        }
        if self.gates.iter().any(|g| g.status == "fail") {
            return false;
        }
        if !self.allow_unevaluated && self.gates.iter().any(|g| g.status == "unevaluated") {
            return false;
        }
        self.score >= 80
    }

    fn verdict(&self) -> &'static str {
        if self.findings.iter().any(|f| f.severity == "error")
            || self.gates.iter().any(|g| g.status == "fail")
        {
            return "not ready";
        }
        match self.score {
            80..=100 if self.ready() => "ready",
            55..=100 => "needs work",
            _ => "not ready",
        }
    }

    /// Plain words: the verdict and the fixes that matter most.
    pub fn summary(&self) -> String {
        if let Some(m) = &self.message {
            return m.clone();
        }
        let mut out = format!("Score {} of 100: {}.", self.score, self.verdict());
        for (i, fix) in self.top_fixes.iter().enumerate() {
            out.push_str(&format!(" {}. {fix}", i + 1));
        }
        for g in self.gates.iter().filter(|g| g.status != "pass") {
            out.push_str(&format!(" {} is {}.", g.code, g.status));
        }
        out
    }

    pub fn to_json(&self) -> Value {
        json!({
            "video": self.video.as_ref().map(NodeId::as_str),
            "summary": self.summary(),
            "score": self.score,
            "verdict": self.verdict(),
            "ready": self.ready(),
            "gates": self.gates.iter().map(|g| json!({
                "code": g.code,
                "status": g.status,
                "reason": g.reason,
            })).collect::<Vec<_>>(),
            "top_fixes": self.top_fixes,
            "findings": self.findings.iter().map(|f| json!({
                "code": f.code,
                "name": f.name,
                "severity": f.severity,
                "scenes": f.scenes,
                "message": f.message,
                "fix": f.fix,
            })).collect::<Vec<_>>(),
            "scenes": self.scenes,
            "overlap": {
                "regions": self.overlap,
                "total": self.overlap_total,
                "note": "Regions where two layers overlap, back to front. Check each with video_crop and confirm which should be on top.",
            },
            "metrics": self.metrics,
        })
    }

    /// One finding per line, for a terminal.
    pub fn text(&self) -> String {
        let mut out = self.summary();
        for f in &self.findings {
            out.push_str(&format!(
                "\n{} {} ({}): {}",
                f.code, f.name, f.severity, f.message
            ));
        }
        if !self.overlap.is_empty() {
            out.push_str(&format!(
                "\n{} overlap regions to check with video_crop (see --json)",
                self.overlap_total
            ));
        }
        out
    }
}

struct Scene {
    placed: Placed,
    frame: NodeId,
    name: String,
    stats: SceneStats,
    layers_mean: f64,
    layers_min: usize,
}

impl Scene {
    fn len_ms(&self) -> u64 {
        self.placed.end_ms - self.placed.start_ms
    }
}

/// What rendering sample pictures found at one moment.
struct Sample {
    at_ms: u64,
    moving: f64,
}

/// Reviews `video` (the first when none is named) of the project in `src`.
pub fn review(
    src: &Source,
    video: Option<&str>,
    samples: Option<usize>,
) -> Result<Report, ToolError> {
    review_at_tempo(src, video, samples, None)
}

/// [`review`] that also checks the cuts against a music tempo of `bpm` beats a minute (RV014).
pub fn review_at_tempo(
    src: &Source,
    video: Option<&str>,
    samples: Option<usize>,
    bpm: Option<f64>,
) -> Result<Report, ToolError> {
    review_at_tempo_with(src, video, samples, bpm, false)
}

/// [`review_at_tempo`] that also says whether a gate that could not be evaluated is allowed to
/// stand: a video is not ready while anything is unevaluated unless it is.
pub fn review_at_tempo_with(
    src: &Source,
    video: Option<&str>,
    samples: Option<usize>,
    bpm: Option<f64>,
    allow_unevaluated: bool,
) -> Result<Report, ToolError> {
    if let Some(b) = bpm.filter(|b| !checks::valid_bpm(*b)) {
        return Err(ToolError(format!("bpm {b} is not a tempo from 30 to 300")));
    }
    let doc = &src.doc;
    let v = match video {
        Some(id) => doc
            .video(&node_id(id)?)
            .ok_or_else(|| ToolError(format!("no video {id}; list_videos shows them")))?,
        None => match doc.videos.first() {
            Some(v) => v,
            None => {
                return Ok(Report::empty(
                    "This project has no video, so there is nothing to review. Make one with \
                     create_video and add_clip (get_playbook explainer for an animated one), then \
                     call review_video.",
                ));
            }
        },
    };
    let scenes = scenes_of(doc, v);
    if scenes.is_empty() {
        return Ok(Report::empty(
            "This video has no design-frame scenes (it is empty or only footage), so there is \
             nothing to review. Add clips of design frames with add_clip.",
        ));
    }
    let count = samples
        .unwrap_or(DEFAULT_SAMPLES)
        .clamp(MIN_SAMPLES, MAX_SAMPLES);
    let sampled = sample(src, v, count)?;
    let flash = flash::scan(src, v)?;
    let mut report = build(doc, v, &scenes, &sampled, flash.as_ref(), bpm);
    report.allow_unevaluated = allow_unevaluated;
    Ok(report)
}

fn scenes_of(doc: &Document, v: &Video) -> Vec<Scene> {
    let mut out = Vec::new();
    for placed in v.placed() {
        let Some(frame) = v.clips.get(placed.index).and_then(|c| c.frame.clone()) else {
            continue;
        };
        let Some(stats) = scene::scene_stats(doc, v, &frame) else {
            continue;
        };
        let name = doc.node(&frame).map(|n| n.name.clone()).unwrap_or_default();
        let (mean, min) = layer_counts(v, &stats.animated, &placed);
        out.push(Scene {
            placed,
            frame,
            name,
            stats,
            layers_mean: mean,
            layers_min: min,
        });
    }
    out
}

/// How many of the scene's animated nodes are moving, at five moments inside it.
fn layer_counts(v: &Video, animated: &[NodeId], placed: &Placed) -> (f64, usize) {
    let span = placed.end_ms - placed.start_ms;
    let counts: Vec<usize> = (0..5)
        .map(|k| {
            let at = (placed.start_ms + span * (2 * k + 1) / 10) as f64;
            animated
                .iter()
                .filter(|id| {
                    v.animations
                        .iter()
                        .any(|a| &a.node == *id && a.moving_at(at))
                })
                .count()
        })
        .collect();
    let mean = counts.iter().sum::<usize>() as f64 / counts.len() as f64;
    (mean, counts.iter().copied().min().unwrap_or(0))
}

fn pixel_share(a: &Pixmap, b: &Pixmap) -> f64 {
    let total = (a.width() * a.height()) as f64;
    let moved = a
        .data()
        .chunks(4)
        .zip(b.data().chunks(4))
        .filter(|(x, y)| {
            x.iter()
                .zip(y.iter())
                .any(|(p, q)| p.abs_diff(*q) > PIXEL_DELTA)
        })
        .count();
    moved as f64 / total
}

/// Renders `count` moments across the video, each against the one `PROBE_MS` later. Footage needs
/// a decoder, so a video with footage is not sampled.
fn sample(src: &Source, v: &Video, count: usize) -> Result<Vec<Sample>, ToolError> {
    let duration = v.duration_ms();
    if duration <= PROBE_MS * 2 || crate::video_export::has_footage(&src.doc, v) {
        return Ok(Vec::new());
    }
    let scale = SAMPLE_SIDE / f64::from(v.width.max(v.height));
    let (w, h) = (
        (f64::from(v.width) * scale).round() as u32,
        (f64::from(v.height) * scale).round() as u32,
    );
    let mut renderer = VideoRenderer::new(&src.doc, v, src.dir()).with_size(w, h);
    let mut out = Vec::new();
    for i in 0..count {
        let at = duration * (2 * i as u64 + 1) / (2 * count as u64);
        let later = if at + PROBE_MS < duration {
            at + PROBE_MS
        } else {
            at - PROBE_MS
        };
        let render = |r: &mut VideoRenderer, t| r.frame_at(t).map_err(|e| ToolError(e.to_string()));
        let (a, b) = (render(&mut renderer, at)?, render(&mut renderer, later)?);
        out.push(Sample {
            at_ms: at,
            moving: pixel_share(&a, &b),
        });
    }
    Ok(out)
}

/// The findings that are errors, not warnings.
const ERROR_CODES: [&str; 2] = ["RV013", "RV020"];
/// The score a video with an error cannot rise above.
const ERROR_SCORE_CAP: u32 = 54;

struct Draft {
    code: &'static str,
    name: &'static str,
    scenes: Vec<usize>,
    message: String,
    fix: &'static str,
    weight: f64,
}

fn scenes_where(scenes: &[Scene], pred: impl Fn(&Scene) -> bool) -> Vec<usize> {
    scenes
        .iter()
        .enumerate()
        .filter(|(_, s)| pred(s))
        .map(|(i, _)| i)
        .collect()
}

fn scene_list(of: &[usize], scenes: &[Scene]) -> String {
    of.iter()
        .map(|i| format!("{} ({})", i + 1, scenes[*i].name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn scene_drafts(scenes: &[Scene]) -> Vec<Draft> {
    let mut out = Vec::new();
    let mut add = |code, name, hits: Vec<usize>, what: &str, fix, weight| {
        if !hits.is_empty() {
            let message = format!("{what}: scene {}", scene_list(&hits, scenes));
            out.push(Draft {
                code,
                name,
                scenes: hits,
                message,
                fix,
                weight,
            });
        }
    };
    add(
        "RV001",
        "image-backdrop",
        scenes_where(scenes, |s| s.stats.coverage.biggest_image > MAX_IMAGE_SHARE),
        "one image covers over 60% of the scene, so it is a picture with things on it",
        "Replace the big image with vector scenery: sky_gradient, parallax_hills, starfield and other add_block backgrounds, or path nodes and import_svg_vector.",
        28.0,
    );
    add(
        "RV002",
        "text-only-motion",
        scenes_where(scenes, |s| s.stats.text_only_motion()),
        "only text and rectangles are animated",
        "Animate the scenery and the hero: ambient loops (animate_preset float, breathe, bob), shapes drawing on, a camera move.",
        15.0,
    );
    add(
        "RV004",
        "no-ambient-loops",
        scenes_where(scenes, |s| s.len_ms() >= SETTLED_MS && s.stats.loops == 0),
        "nothing loops, so the scene freezes between its big moves",
        "Give clouds, stars, characters and props a looping track (animate_preset float, bob, breathe, pulse, or repeat loop or pingpong) with different durations.",
        12.0,
    );
    add(
        "RV006",
        "few-vector-shapes",
        scenes_where(scenes, |s| s.stats.shapes < MIN_SHAPES),
        "fewer than 12 vector shapes, too bare for an illustrated scene",
        "Add layers: a sky_gradient or cells_backdrop, parallax_hills or city_skyline, a hero (character or a prop_ block), particles in front.",
        12.0,
    );
    add(
        "RV012",
        "few-moving-layers",
        scenes_where(scenes, |s| {
            s.len_ms() >= SETTLED_MS && s.layers_mean < MIN_MOVING_LAYERS
        }),
        "fewer than 3 layers move at a time",
        "Keep three or more layers moving at any moment: backdrop drift, the hero, particles or props, each on its own timing.",
        10.0,
    );
    out
}

fn composition_hash(doc: &Document, frame: &NodeId) -> (u64, usize) {
    let mut hasher = DefaultHasher::new();
    let nodes = doc
        .node(frame)
        .map(scene::visible_nodes)
        .unwrap_or_default();
    for n in &nodes {
        n.kind.as_str().hash(&mut hasher);
        for v in [n.x, n.y, n.width, n.height] {
            (v.round() as i64).hash(&mut hasher);
        }
    }
    (hasher.finish(), nodes.len())
}

fn repeated_scenes(doc: &Document, scenes: &[Scene]) -> Option<Draft> {
    let hashes: Vec<(u64, usize)> = scenes
        .iter()
        .map(|s| composition_hash(doc, &s.frame))
        .collect();
    let hits = scenes_where(scenes, |s| {
        let i = scenes.iter().position(|o| std::ptr::eq(o, s)).unwrap_or(0);
        let (h, n) = hashes[i];
        n >= MIN_SAME_NODES && hashes.iter().filter(|(o, _)| *o == h).count() > 1
    });
    (!hits.is_empty()).then(|| Draft {
        code: "RV007",
        name: "repeated-composition",
        message: format!(
            "scenes with the same layout of shapes: {}",
            scene_list(&hits, scenes)
        ),
        scenes: hits,
        fix: "Give each scene its own composition: move the hero, change the scale, the scenery and the camera angle, so no two scenes look alike.",
        weight: 8.0,
    })
}

fn still_stretches(samples: &[Sample], v: &Video) -> Option<Draft> {
    let duration = v.duration_ms();
    let spacing = samples.windows(2).map(|w| w[1].at_ms - w[0].at_ms).next()?;
    let still = |s: &Sample| s.moving < STILL_SHARE && s.at_ms + END_HOLD_MS < duration;
    let (mut best, mut run, mut best_at) = (0u64, 0u64, 0u64);
    for s in samples {
        if still(s) {
            run += 1;
            let length = (run - 1) * spacing + PROBE_MS;
            if length > best {
                (best, best_at) = (length, s.at_ms + PROBE_MS - length);
            }
        } else {
            run = 0;
        }
    }
    (best > MAX_STILL_MS).then(|| Draft {
        code: "RV003",
        name: "still-stretch",
        scenes: Vec::new(),
        message: format!(
            "the picture holds still for about {:.1} s from {:.1} s (under 2% of pixels change)",
            best as f64 / 1000.0,
            best_at as f64 / 1000.0
        ),
        fix: "Keep something moving everywhere: ambient loops, a slow camera_drift or camera_push_in, particles, a staggered reveal. Only the last 2 s may hold.",
        weight: 15.0,
    })
}

fn camera_draft(v: &Video) -> Option<Draft> {
    (v.duration_ms() > CAMERA_AFTER_MS && !v.has_camera()).then(|| Draft {
        code: "RV005",
        name: "no-camera-move",
        scenes: Vec::new(),
        message: format!(
            "the video is {:.0} s long and the camera never moves",
            v.duration_ms() as f64 / 1000.0
        ),
        fix: "Add camera moves that carry the eye: camera_push_in on the hero, camera_pan across the scenery, camera_drift under everything (animate_preset), with layer depth values for parallax.",
        weight: 8.0,
    })
}

/// Rough share of the frame the widest caption box covers, from its size, line count and text.
fn caption_share(v: &Video) -> f64 {
    let style = &v.captions.style;
    let size = f64::from(style.size_pct) / 100.0;
    let aspect = f64::from(v.height) / f64::from(v.width.max(1));
    v.captions
        .cues
        .iter()
        .map(|c| {
            let chars = c.text.chars().count() as f64;
            let per_line = (0.9 / (0.55 * size * aspect)).max(1.0);
            let lines = (chars / per_line)
                .ceil()
                .clamp(1.0, f64::from(style.max_lines));
            let width = (chars / lines * 0.55 * size * aspect).min(0.9);
            lines * 1.25 * size * width
        })
        .fold(0.0, f64::max)
}

fn caption_draft(v: &Video) -> Option<Draft> {
    let share = caption_share(v);
    (share > MAX_CAPTION_SHARE).then(|| Draft {
        code: "RV008",
        name: "captions-cover-the-frame",
        scenes: Vec::new(),
        message: format!("captions can cover about {:.0}% of the frame", share * 100.0),
        fix: "Make the captions smaller (set_caption_style size) and shorter (two lines at most); let the picture carry the story.",
        weight: 6.0,
    })
}

fn sound_drafts(v: &Video, scenes: &[Scene]) -> Vec<Draft> {
    let mut out = Vec::new();
    if v.tracks.is_empty() && v.audio.is_none() {
        out.push(Draft {
            code: "RV009",
            name: "no-sound",
            scenes: Vec::new(),
            message: "the video has no sound".to_string(),
            fix: "Add sound: a music bed under any voice (add_sound_track, role music with duck), whooshes on cuts and a hit or chime on each reveal (make_sound).",
            weight: 8.0,
        });
        return out;
    }
    let per_scene: Vec<BTreeSet<&str>> = scenes
        .iter()
        .map(|s| {
            v.tracks
                .iter()
                .filter(|t| t.role != TrackRole::Music)
                .filter(|t| {
                    let at = u64::from(t.start_ms);
                    at + 200 >= s.placed.start_ms && at < s.placed.end_ms
                })
                .map(|t| t.file.as_str())
                .collect()
        })
        .collect();
    let files: HashSet<&str> = v
        .tracks
        .iter()
        .filter(|t| t.role != TrackRole::Music)
        .map(|t| t.file.as_str())
        .collect();
    let same = per_scene.windows(2).all(|w| w[0] == w[1]);
    if scenes.len() >= 3 && same && files.len() <= 1 {
        out.push(Draft {
            code: "RV010",
            name: "same-sound-every-scene",
            scenes: Vec::new(),
            message: "no sound effect or voice line is different from scene to scene".to_string(),
            fix: "Design the sound per scene: a whoosh on each cut, a different hit, chime or pop on each reveal (make_sound kinds), the bed lower under the voice.",
            weight: 5.0,
        });
    }
    out
}

#[derive(Default)]
struct Palette {
    tokens_used: usize,
    loose_refs: usize,
    loose: HashSet<(u8, u8, u8, u8)>,
}

impl Palette {
    fn paint(&mut self, p: &Paint) {
        match p {
            Paint::Color(c) => {
                self.loose_refs += 1;
                self.loose.insert((c.r, c.g, c.b, c.a));
            }
            Paint::Token(_) => self.tokens_used += 1,
            Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
                stops.iter().for_each(|s| self.paint(&s.color));
            }
        }
    }

    fn node(&mut self, n: &Node) {
        if let Some(p) = &n.fill {
            self.paint(p);
        }
        if let Some(s) = n.stroke.as_ref().filter(|s| s.width > 0.0) {
            self.paint(&s.paint);
        }
    }
}

fn palette_of(doc: &Document, scenes: &[Scene]) -> Palette {
    let mut palette = Palette::default();
    for s in scenes {
        if let Some(frame) = doc.node(&s.frame) {
            palette.node(frame);
            scene::visible_nodes(frame)
                .into_iter()
                .filter(|n| n.kind != NodeKind::Text)
                .for_each(|n| palette.node(n));
        }
    }
    palette
}

fn palette_draft(doc: &Document, palette: &Palette) -> Option<Draft> {
    let loose = palette.loose.len();
    let drifting = loose > MAX_LOOSE_COLORS && palette.tokens_used < palette.loose_refs;
    let untokened = doc.tokens.is_empty() && loose > LOOSE_COLORS_WITHOUT_TOKENS;
    (drifting || untokened).then(|| Draft {
        code: "RV011",
        name: "palette-not-from-tokens",
        scenes: Vec::new(),
        message: format!(
            "{loose} different hard-coded colors against {} tokens ({} paints use a token)",
            doc.tokens.len(),
            palette.tokens_used
        ),
        fix: "Define the 4 to 6 palette colors with set_token and paint with {token} references so the whole film shares one palette.",
        weight: 6.0,
    })
}

fn finish(
    drafts: Vec<Draft>,
    soft: Vec<Draft>,
    scene_count: usize,
) -> (Vec<Finding>, u32, Vec<String>) {
    let finding = |d: Draft, severity: &'static str| {
        let share = if d.scenes.is_empty() {
            1.0
        } else {
            d.scenes.len() as f64 / scene_count as f64
        };
        Finding {
            code: d.code,
            name: d.name,
            severity,
            scenes: d.scenes,
            message: d.message,
            fix: d.fix.to_string(),
            weight: d.weight,
            penalty: d.weight * share,
        }
    };
    let mut findings: Vec<Finding> = drafts
        .into_iter()
        .map(|d| {
            let severity = if ERROR_CODES.contains(&d.code) {
                "error"
            } else {
                "warning"
            };
            finding(d, severity)
        })
        .collect();
    findings.extend(soft.into_iter().map(|d| finding(d, "warning")));
    findings.sort_by(|a, b| a.code.cmp(b.code));
    let lost: f64 = findings.iter().map(|f| f.penalty).sum();
    let mut score = (100.0 - lost).round().clamp(0.0, 100.0) as u32;
    if findings.iter().any(|f| f.severity == "error") {
        score = score.min(ERROR_SCORE_CAP);
    }
    let mut ranked: Vec<&Finding> = findings.iter().collect();
    ranked.sort_by(|a, b| b.penalty.total_cmp(&a.penalty).then(a.code.cmp(b.code)));
    let fixes = ranked.iter().take(3).map(|f| f.fix.clone()).collect();
    (findings, score, fixes)
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let all: Vec<f64> = values.collect();
    if all.is_empty() {
        0.0
    } else {
        all.iter().sum::<f64>() / all.len() as f64
    }
}

fn scene_json(i: usize, s: &Scene) -> Value {
    json!({
        "scene": i + 1,
        "name": s.name,
        "frame": s.frame.as_str(),
        "start_ms": s.placed.start_ms,
        "end_ms": s.placed.end_ms,
        "vector_shapes": s.stats.shapes,
        "texts": s.stats.texts,
        "biggest_image_share": round(s.stats.coverage.biggest_image),
        "raster_share": round(s.stats.coverage.raster),
        "vector_share": round(s.stats.coverage.vector),
        "animated_nodes": s.stats.animated.len(),
        "ambient_loops": s.stats.loops,
        "moving_layers_mean": round(s.layers_mean),
        "moving_layers_min": s.layers_min,
    })
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn metrics_json(
    v: &Video,
    scenes: &[Scene],
    samples: &[Sample],
    palette: &Palette,
    doc: &Document,
    flash: Option<&flash::FlashScan>,
) -> Value {
    json!({
        "duration_ms": v.duration_ms(),
        "scenes": scenes.len(),
        "sampled_moments": samples.len(),
        "moving_layers_mean": round(mean(scenes.iter().map(|s| s.layers_mean))),
        "moving_pixel_share_mean": if samples.is_empty() { Value::Null } else { json!(round(mean(samples.iter().map(|s| s.moving)))) },
        "moving_pixel_share_min": samples.iter().map(|s| s.moving).reduce(f64::min).map(round),
        "raster_share_mean": round(mean(scenes.iter().map(|s| s.stats.coverage.raster))),
        "vector_share_mean": round(mean(scenes.iter().map(|s| s.stats.coverage.vector))),
        "vector_shapes_total": scenes.iter().map(|s| s.stats.shapes).sum::<usize>(),
        "ambient_loops_total": scenes.iter().map(|s| s.stats.loops).sum::<usize>(),
        "camera_moves": v.has_camera(),
        "sound_tracks": v.tracks.len() + usize::from(v.audio.is_some()),
        "caption_share_estimate": round(caption_share(v)),
        "tokens": doc.tokens.len(),
        "token_paints": palette.tokens_used,
        "hard_coded_paints": palette.loose_refs,
        "distinct_hard_coded_colors": palette.loose.len(),
        "flash_scanned_ms": flash.map(|f| f.scanned_ms),
        "flash_changes_per_second_max": flash.map(|f| f.worst_window().0),
    })
}

fn build(
    doc: &Document,
    v: &Video,
    scenes: &[Scene],
    samples: &[Sample],
    flash: Option<&flash::FlashScan>,
    bpm: Option<f64>,
) -> Report {
    let palette = palette_of(doc, scenes);
    let mut drafts = scene_drafts(scenes);
    drafts.extend(repeated_scenes(doc, scenes));
    drafts.extend(still_stretches(samples, v));
    drafts.extend(camera_draft(v));
    drafts.extend(caption_draft(v));
    drafts.extend(sound_drafts(v, scenes));
    drafts.extend(palette_draft(doc, &palette));
    drafts.extend(flash.and_then(flash::draft));
    drafts.extend(bpm.and_then(|b| checks::off_beat(v, scenes, b)));
    drafts.extend(checks::text_too_brief(doc, v, scenes));
    drafts.extend(checks::text_in_unsafe_zone(doc, v, scenes));
    drafts.extend(checks::thin_stretch(v, scenes));
    drafts.extend(checks::identical_scenes(doc, scenes));
    drafts.extend(checks::no_final_hold(v));
    let tracks = crate::scene_checks::track_gate::analyze(
        doc,
        v,
        crate::scene_checks::track_gate::DEFAULT_SAMPLES,
        &[],
    );
    let (dead, idle) = tracks.rv020();
    let mut soft: Vec<Draft> = Vec::new();
    if !dead.is_empty() {
        drafts.push(Draft {
            code: "RV020",
            name: "track-drives-nothing",
            scenes: Vec::new(),
            message: format!(
                "{} tracks drive nothing: {}",
                dead.len(),
                dead.join("; ")
            ),
            fix: "Give every track a node that exists, is on screen while the track plays and is visible, or remove the track.",
            weight: 15.0,
        });
    }
    if !idle.is_empty() {
        soft.push(Draft {
            code: "RV020",
            name: "track-drives-nothing",
            scenes: Vec::new(),
            message: format!(
                "{} tracks hold one value all the way: {}",
                idle.len(),
                idle.join("; ")
            ),
            fix: "Give the track keys that differ, or remove it: a track that never changes is not motion.",
            weight: 4.0,
        });
    }
    let (findings, score, top_fixes) = finish(drafts, soft, scenes.len());
    let (overlap, overlap_total) = overlap::regions(doc, v, scenes);
    let mut gates = Vec::new();
    gates.push(if samples.is_empty() {
        GateEntry::unevaluated(
            "RV003",
            "the video has no pictures to sample (it has footage, or it is too short), so its stillness was not measured",
        )
    } else {
        GateEntry::pass("RV003")
    });
    gates.push(match flash {
        Some(_) => GateEntry::pass("RV013"),
        None => GateEntry::unevaluated(
            "RV013",
            "the video could not be scanned for flashes (it has footage, or it is shorter than a second)",
        ),
    });
    gates.push(match bpm {
        Some(_) => {
            if findings.iter().any(|f| f.code == "RV014") {
                GateEntry::fail("RV014", "the cuts do not land on the beat")
            } else {
                GateEntry::pass("RV014")
            }
        }
        None => GateEntry::unevaluated(
            "RV014",
            "no tempo was given, so the cuts were not checked against the beat",
        ),
    });
    gates.push(if dead.is_empty() {
        GateEntry::pass("RV020")
    } else {
        GateEntry::fail("RV020", format!("{} tracks drive nothing", dead.len()))
    });
    Report {
        video: Some(v.id.clone()),
        message: None,
        score,
        findings,
        top_fixes,
        scenes: scenes
            .iter()
            .enumerate()
            .map(|(i, s)| scene_json(i, s))
            .collect(),
        metrics: metrics_json(v, scenes, samples, &palette, doc, flash),
        overlap,
        overlap_total,
        gates,
        allow_unevaluated: false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use od_core::{Actor, Operation, Session, SoundTrack};

    const W: u32 = 1280;
    const H: u32 = 720;

    fn apply(s: &mut Session, ops: Value) -> od_core::TxReceipt {
        let ops: Vec<Operation> = serde_json::from_value(ops).unwrap();
        s.apply(Actor::User, None, &ops).unwrap()
    }

    fn start() -> (Session, NodeId, NodeId) {
        let mut s = Session::new(Document::new("review"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        apply(
            &mut s,
            json!([
                {"op": "set_token", "name": "ink", "value": "#101830"},
                {"op": "set_token", "name": "accent", "value": "#ff7a45"},
                {"op": "create_video", "name": "V", "fps": 30, "width": W, "height": H}
            ]),
        );
        let video = s.document().videos[0].id.clone();
        (s, root, video)
    }

    fn frame(s: &mut Session, root: &NodeId, name: &str) -> NodeId {
        apply(
            s,
            json!([{"op": "create", "parent": root.as_str(), "node":
                {"kind": "frame", "name": name, "width": W, "height": H, "fill": "{ink}"}}]),
        )
        .created[0]
            .clone()
    }

    fn clip(s: &mut Session, video: &NodeId, frame: &NodeId, ms: u32) {
        apply(
            s,
            json!([{"op": "add_clip", "video": video.as_str(), "frame": frame.as_str(), "duration_ms": ms}]),
        );
    }

    fn source(s: &Session) -> Source {
        Source::of_document(s.document().clone())
    }

    /// One full-frame picture and a title that fades in: the failure this tool exists to catch.
    fn picture_and_text() -> Source {
        let (mut s, root, video) = start();
        let f = frame(&mut s, &root, "scene");
        let made = apply(
            &mut s,
            json!([
                {"op": "create", "parent": f.as_str(), "node": {"kind": "image", "name": "Backdrop",
                    "x": 0, "y": 0, "width": W, "height": H, "image": "media/backdrop.jpg"}},
                {"op": "create", "parent": f.as_str(), "node": {"kind": "text", "name": "Title",
                    "x": 100, "y": 100, "width": 800, "height": 120, "text": "Black holes",
                    "font_size": 90, "fill": "#ffffff"}}
            ]),
        );
        apply(
            &mut s,
            json!([{"op": "animate", "video": video.as_str(), "node": made.created[1].as_str(),
                "prop": "opacity", "keys": [{"at_ms": 0, "value": 0.0}, {"at_ms": 800, "value": 1.0}]}]),
        );
        clip(&mut s, &video, &f, 16_000);
        source(&s)
    }

    fn scene_shapes(s: &mut Session, f: &NodeId, video: &NodeId, scene: usize) {
        let creates: Vec<Value> = (0..26)
            .map(|i| {
                let size = 40 + (i % 5) * 12;
                let fill = if i % 2 == 0 { "{accent}" } else { "#2ee6c4" };
                json!({"op": "create", "parent": f.as_str(), "node": {
                    "kind": "ellipse", "name": format!("blob {i}"),
                    "x": (i * 53 + scene * 97) % 1100, "y": (i * 37 + scene * 61) % 600,
                    "width": size, "height": size, "fill": fill}})
            })
            .collect();
        let made = apply(s, Value::Array(creates));
        let loops: Vec<Value> = made
            .created
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let span = 2000 + (i as u64 * 370) % 2500;
                json!({"op": "animate", "video": video.as_str(), "node": id.as_str(), "prop": "y",
                    "repeat": "loop",
                    "keys": [{"at_ms": 0, "value": 100.0}, {"at_ms": span / 2, "value": 160.0},
                             {"at_ms": span, "value": 100.0}]})
            })
            .collect();
        apply(s, Value::Array(loops));
    }

    fn track(role: TrackRole, file: &str, start_ms: u32) -> SoundTrack {
        SoundTrack {
            id: NodeId::parse(&format!("t_{start_ms}_{file}")).unwrap(),
            role,
            file: format!("media/{file}.wav"),
            start_ms,
            in_ms: 0,
            out_ms: None,
            gain: 0.3,
            fade_in_ms: 0,
            fade_out_ms: 0,
            pan: 0.0,
            duck: None,
        }
    }

    /// Three scenes of layered, looping vector shapes under a slow camera zoom.
    fn living_scenes(sound: bool) -> Source {
        let (mut s, root, video) = start();
        for i in 0..3 {
            let f = frame(&mut s, &root, &format!("scene {i}"));
            scene_shapes(&mut s, &f, &video, i);
            clip(&mut s, &video, &f, 6_000);
        }
        apply(
            &mut s,
            json!([{"op": "animate", "video": video.as_str(), "node": video.as_str(),
                "prop": "camera_zoom",
                "keys": [{"at_ms": 0, "value": 1.0}, {"at_ms": 18000, "value": 1.1}]}]),
        );
        let mut src = source(&s);
        if sound {
            src.doc.videos[0].tracks = vec![
                track(TrackRole::Music, "bed", 0),
                track(TrackRole::Sfx, "a", 0),
                track(TrackRole::Sfx, "b", 6_000),
                track(TrackRole::Sfx, "c", 12_000),
            ];
        }
        src
    }

    fn codes(r: &Report) -> Vec<&'static str> {
        r.findings.iter().map(|f| f.code).collect()
    }

    #[test]
    fn a_picture_with_text_is_caught_and_scores_low() {
        let report = review(&picture_and_text(), None, None).unwrap();
        let found = codes(&report);
        for code in ["RV001", "RV002", "RV004", "RV005", "RV006", "RV009"] {
            assert!(found.contains(&code), "{code} missing from {found:?}");
        }
        assert!(report.score < 40, "{}", report.score);
        assert_eq!(report.top_fixes.len(), 3);
        assert!(
            report.summary().contains("not ready"),
            "{}",
            report.summary()
        );
        assert!(report.scenes[0]["biggest_image_share"].as_f64().unwrap() > 0.9);
    }

    #[test]
    fn layered_looping_vector_scenes_pass() {
        let report = review(&living_scenes(true), None, None).unwrap();
        let found = codes(&report);
        for code in [
            "RV001", "RV002", "RV003", "RV004", "RV005", "RV006", "RV009", "RV010",
        ] {
            assert!(!found.contains(&code), "{code} in {found:?}");
        }
        assert!(report.score >= 80, "{} {found:?}", report.score);
        let m = &report.metrics;
        assert!(m["moving_pixel_share_mean"].as_f64().unwrap() > STILL_SHARE);
        assert_eq!(m["camera_moves"], json!(true));
        assert_eq!(m["sampled_moments"], json!(DEFAULT_SAMPLES));
        assert!(m["ambient_loops_total"].as_u64().unwrap() >= 78);
    }

    #[test]
    fn the_score_separates_the_two_and_a_bare_bed_is_noticed() {
        let poor = review(&picture_and_text(), None, None).unwrap().score;
        let rich = review(&living_scenes(true), None, None).unwrap().score;
        assert!(rich > poor + 40, "{rich} vs {poor}");
        let mut bed_only = living_scenes(false);
        bed_only.doc.videos[0].tracks = vec![track(TrackRole::Music, "bed", 0)];
        let found = codes(&review(&bed_only, None, None).unwrap());
        assert!(
            found.contains(&"RV010") && !found.contains(&"RV009"),
            "{found:?}"
        );
        let silent = codes(&review(&living_scenes(false), None, None).unwrap());
        assert!(silent.contains(&"RV009"));
    }

    #[test]
    fn a_still_video_has_a_still_stretch_but_a_held_end_card_does_not() {
        let (mut s, root, video) = start();
        let f = frame(&mut s, &root, "still");
        apply(
            &mut s,
            json!([{"op": "create", "parent": f.as_str(), "node": {"kind": "ellipse", "name": "dot",
                "x": 10, "y": 10, "width": 50, "height": 50, "fill": "{accent}"}}]),
        );
        clip(&mut s, &video, &f, 12_000);
        let found = codes(&review(&source(&s), None, None).unwrap());
        assert!(found.contains(&"RV003"), "{found:?}");

        let (mut s, root, video) = start();
        let f = frame(&mut s, &root, "short");
        clip(&mut s, &video, &f, 1_500);
        let found = codes(&review(&source(&s), None, None).unwrap());
        assert!(!found.contains(&"RV003"), "{found:?}");
    }

    #[test]
    fn repeated_compositions_and_loud_captions_are_flagged() {
        let (mut s, root, video) = start();
        for i in 0..2 {
            let f = frame(&mut s, &root, &format!("same {i}"));
            scene_shapes(&mut s, &f, &video, 1);
            clip(&mut s, &video, &f, 4_000);
        }
        let mut src = source(&s);
        let cue_text = "word ".repeat(30);
        src.doc.videos[0].captions.style.size_pct = 14.0;
        src.doc.videos[0].captions.cues = vec![od_core::Cue {
            id: NodeId::parse("c_1").unwrap(),
            start_ms: 0,
            end_ms: 3_000,
            text: cue_text,
            words: Vec::new(),
        }];
        let found = codes(&review(&src, None, None).unwrap());
        assert!(
            found.contains(&"RV007") && found.contains(&"RV008"),
            "{found:?}"
        );
    }

    #[test]
    fn the_review_is_deterministic_and_reports_its_samples() {
        let src = living_scenes(true);
        let a = review(&src, None, Some(16)).unwrap();
        let b = review(&src, None, Some(16)).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.to_json(), b.to_json());
        assert_eq!(a.metrics["sampled_moments"], json!(16));
        let few = review(&src, None, Some(1)).unwrap();
        assert_eq!(few.metrics["sampled_moments"], json!(MIN_SAMPLES));
    }

    #[test]
    fn no_video_or_an_unknown_video_gets_a_clear_answer() {
        let mut s = Session::new(Document::new("old"));
        s.add_page("p").unwrap();
        let report = review(&source(&s), None, None).unwrap();
        assert!(
            report.summary().contains("no video"),
            "{}",
            report.summary()
        );
        assert!(report.findings.is_empty());
        assert_eq!(report.to_json()["video"], Value::Null);
        assert!(review(&source(&s), Some("v_999"), None).is_err());
    }

    #[test]
    fn the_tool_is_read_only_and_listed() {
        let defs = definitions();
        assert_eq!(defs[0]["name"], "review_video");
        assert_eq!(defs[0]["annotations"]["readOnlyHint"], json!(true));
        assert!(
            crate::tools::definitions()
                .iter()
                .any(|d| d["name"] == "review_video")
        );
    }

    #[test]
    fn a_gate_that_could_not_be_checked_is_named_and_keeps_the_video_from_being_ready() {
        let report = review(&living_scenes(true), None, None).unwrap();
        let beat = report
            .gates
            .iter()
            .find(|g| g.code == "RV014")
            .expect("the beat gate is named");
        assert_eq!(beat.status, "unevaluated");
        assert!(beat.reason.contains("tempo"), "{}", beat.reason);
        assert!(
            !report.ready(),
            "a video with an unevaluated gate is not ready"
        );
        assert_eq!(report.verdict(), "needs work");
        assert!(report.to_json()["gates"].as_array().unwrap().len() >= 4);

        // The caller may accept a gate it knows cannot matter.
        let accepted = review_at_tempo_with(&living_scenes(true), None, None, None, true).unwrap();
        assert!(accepted.ready());
        assert_eq!(accepted.verdict(), "ready");

        // A tempo lets the gate look, and then it passes.
        let with_beat = review_at_tempo(&living_scenes(true), None, None, Some(120.0)).unwrap();
        let beat = with_beat
            .gates
            .iter()
            .find(|g| g.code == "RV014")
            .expect("the beat gate is named");
        assert_eq!(beat.status, "pass");
    }

    #[test]
    fn a_track_that_drives_nothing_is_an_error() {
        let mut src = living_scenes(true);
        src.doc.videos[0].animations.push(od_core::Animation {
            additive: false,
            node: NodeId::parse("n_9999").unwrap(),
            prop: od_core::AnimProp::X,
            keys: vec![
                od_core::Key {
                    at_ms: 0,
                    value: 10.0,
                    ease: od_core::Ease::default(),
                },
                od_core::Key {
                    at_ms: 1000,
                    value: 20.0,
                    ease: od_core::Ease::default(),
                },
            ],
            repeat: od_core::Repeat::Once,
        });
        let report = review(&src, None, None).unwrap();
        let found = codes(&report);
        assert!(found.contains(&"RV020"), "{found:?}");
        let finding = report
            .findings
            .iter()
            .find(|f| f.code == "RV020")
            .expect("the finding is there");
        assert_eq!(finding.severity, "error");
        assert!(report.score <= ERROR_SCORE_CAP, "{}", report.score);
        assert!(!report.ready());
    }
}
