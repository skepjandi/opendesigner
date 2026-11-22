//! RV014 to RV019: how the video is timed, laid out and ended, read from the document alone.
//! (RV013, flash safety, draws pictures and lives in `flash.rs`.)

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use od_core::scene;
use od_core::video::Placed;
use od_core::{AnimProp, Document, Node, NodeKind, TrackRole, Video};

use super::{Draft, Scene, scene_list, scenes_where};
use crate::inspect;

/// A cut may be this far from the beat or half beat and still read as on it.
const BEAT_TOLERANCE_MS: f64 = 70.0;
/// Share of cuts off the beat that is a finding (and at least two).
const OFF_BEAT_SHARE: f64 = 0.4;
const MIN_BPM: f64 = 30.0;
const MAX_BPM: f64 = 300.0;

/// A text is shown this long at least, plus this much per short word.
const TEXT_BASE_MS: u64 = 1_000;
const TEXT_WORD_MS: f64 = 400.0;
/// A word of up to this many letters is short; a longer one counts for more.
const SHORT_WORD_CHARS: f64 = 6.0;
const TEXT_STEP_MS: u64 = 50;
const TEXT_SHOWN_OPACITY: f64 = 0.5;

/// Where a phone's interface covers a 1080 x 1920 video, scaled to the video's height.
const ZONE_REFERENCE_HEIGHT: f64 = 1920.0;
const TOP_ZONE: f64 = 150.0;
const BOTTOM_ZONE: f64 = 250.0;
/// A video at least this much taller than wide is a vertical video.
const PORTRAIT_RATIO: f64 = 1.3;
/// A text box this share inside a zone is in it.
const ZONE_SHARE: f64 = 0.3;

const DENSITY_WINDOW_MS: u64 = 2_000;
const DENSITY_STEP_MS: u64 = 250;
const MIN_LAYERS: usize = 3;
const MIN_IDENTICAL_NODES: usize = 3;
const MIN_IDENTICAL_MS: u64 = 1_000;
/// The last picture should stand still this long.
const HOLD_MS: u64 = 800;
const HOLD_STEP_MS: u64 = 100;

/// The beat grid: bpm `bpm`, starting with the first music track (or at 0).
pub fn off_beat(v: &Video, scenes: &[Scene], bpm: f64) -> Option<Draft> {
    let half_beat = 30_000.0 / bpm;
    let origin = v
        .tracks
        .iter()
        .filter(|t| t.role == TrackRole::Music)
        .map(|t| f64::from(t.start_ms))
        .fold(f64::INFINITY, f64::min);
    let origin = if origin.is_finite() { origin } else { 0.0 };
    let cuts: Vec<(usize, f64)> = v
        .placed()
        .iter()
        .filter(|p| p.index > 0)
        .map(|p| (p.index, cut_moment(v, p)))
        .collect();
    let off: Vec<(usize, f64)> = cuts
        .iter()
        .copied()
        .filter(|(_, at)| distance_to_grid(*at - origin, half_beat) > BEAT_TOLERANCE_MS)
        .collect();
    if off.len() < 2 || (off.len() as f64) < OFF_BEAT_SHARE * cuts.len() as f64 {
        return None;
    }
    let hits = scenes_where(scenes, |s| off.iter().any(|(i, _)| *i == s.placed.index));
    let worst = off
        .iter()
        .map(|(_, at)| distance_to_grid(*at - origin, half_beat))
        .fold(0.0, f64::max);
    Some(Draft {
        code: "RV014",
        name: "cuts-off-the-beat",
        message: format!(
            "{} of {} cuts are not on a beat or half beat at {bpm:.0} bpm (up to {worst:.0} ms off): scene {}",
            off.len(),
            cuts.len(),
            scene_list(&hits, scenes)
        ),
        scenes: hits,
        fix: "Move each cut onto a beat or half beat (60000 / bpm ms apart, from the start of the music): change the clip durations with set_clip, or slide the music track's start_ms.",
        weight: 8.0,
    })
}

/// When the picture really changes at the cut into `p`: its start, or the middle of a transition.
fn cut_moment(v: &Video, p: &Placed) -> f64 {
    let blended = v.clips[p.index].transition.kind != od_core::TransitionKind::Cut;
    p.start_ms as f64
        + if blended {
            f64::from(p.overlap_ms) / 2.0
        } else {
            0.0
        }
}

fn distance_to_grid(at: f64, step: f64) -> f64 {
    let r = at.rem_euclid(step);
    r.min(step - r)
}

/// Whether `bpm` is a tempo the review accepts.
pub fn valid_bpm(bpm: f64) -> bool {
    bpm.is_finite() && (MIN_BPM..=MAX_BPM).contains(&bpm)
}

/// How long a text needs on screen to be read.
pub(super) fn reading_ms(text: &str) -> u64 {
    let words: f64 = text
        .split_whitespace()
        .map(|w| {
            (w.chars().count() as f64 / SHORT_WORD_CHARS)
                .ceil()
                .max(1.0)
        })
        .sum();
    TEXT_BASE_MS + (words * TEXT_WORD_MS).round() as u64
}

/// The longest stretch inside the scene during which the text is visible enough to read.
fn shown_ms(v: &Video, n: &Node, p: &Placed) -> u64 {
    let opacity_at = |at: u64| v.animated_value(&n.id, AnimProp::Opacity, at as f64, n.opacity);
    let (mut best, mut run) = (0, 0);
    let mut at = p.start_ms;
    while at < p.end_ms {
        let level = opacity_at(at);
        if level >= TEXT_SHOWN_OPACITY {
            run += TEXT_STEP_MS;
            best = best.max(run);
        } else {
            run = 0;
        }
        at += TEXT_STEP_MS;
    }
    best.min(p.end_ms - p.start_ms)
}

fn texts_of<'a>(doc: &'a Document, s: &Scene) -> Vec<&'a Node> {
    doc.node(&s.frame)
        .map(scene::visible_nodes)
        .unwrap_or_default()
        .into_iter()
        .filter(|n| {
            n.kind == NodeKind::Text && n.text.as_deref().is_some_and(|t| !t.trim().is_empty())
        })
        .collect()
}

/// RV015: texts that leave before they can be read.
pub fn text_too_brief(doc: &Document, v: &Video, scenes: &[Scene]) -> Option<Draft> {
    let mut hits = Vec::new();
    let mut examples = Vec::new();
    for (i, s) in scenes.iter().enumerate() {
        for n in texts_of(doc, s) {
            let text = n.text.as_deref().unwrap_or_default();
            let (shown, needed) = (shown_ms(v, n, &s.placed), reading_ms(text));
            if shown < needed {
                if !hits.contains(&i) {
                    hits.push(i);
                }
                if examples.len() < 3 {
                    examples.push(format!(
                        "{:?} shows {:.1} s and needs {:.1} s (scene {})",
                        text.chars().take(32).collect::<String>(),
                        shown as f64 / 1000.0,
                        needed as f64 / 1000.0,
                        i + 1
                    ));
                }
            }
        }
    }
    (!hits.is_empty()).then(|| Draft {
        code: "RV015",
        name: "text-too-brief",
        scenes: hits,
        message: format!("text leaves before it can be read: {}", examples.join("; ")),
        fix: "Keep each text on screen for 1 s plus 0.4 s per short word (longer words count more): lengthen the scene, start the text earlier, or cut words.",
        weight: 10.0,
    })
}

fn is_portrait(v: &Video) -> bool {
    f64::from(v.height) / f64::from(v.width.max(1)) >= PORTRAIT_RATIO
}

/// RV016: in a vertical video, text under the interface bars of a phone.
pub fn text_in_unsafe_zone(doc: &Document, v: &Video, scenes: &[Scene]) -> Option<Draft> {
    if !is_portrait(v) {
        return None;
    }
    let unit = f64::from(v.height) / ZONE_REFERENCE_HEIGHT;
    let (top, bottom) = (TOP_ZONE * unit, f64::from(v.height) - BOTTOM_ZONE * unit);
    let mut hits = Vec::new();
    let mut examples = Vec::new();
    for (i, s) in scenes.iter().enumerate() {
        let at = (s.placed.start_ms + s.placed.end_ms) / 2;
        for n in texts_of(doc, s) {
            let Some(r) = inspect::node_rect(doc, v, &n.id, at) else {
                continue;
            };
            let inside = (r.bottom().min(top) - r.y.min(top)).max(0.0)
                + (r.bottom().max(bottom) - r.y.max(bottom)).max(0.0);
            if r.height > 0.0 && inside / r.height > ZONE_SHARE {
                if !hits.contains(&i) {
                    hits.push(i);
                }
                if examples.len() < 3 {
                    examples.push(format!(
                        "{:?} in scene {} at y {:.0} to {:.0}",
                        n.text
                            .as_deref()
                            .unwrap_or_default()
                            .chars()
                            .take(32)
                            .collect::<String>(),
                        i + 1,
                        r.y,
                        r.bottom()
                    ));
                }
            }
        }
    }
    (!hits.is_empty()).then(|| Draft {
        code: "RV016",
        name: "text-in-unsafe-zone",
        scenes: hits,
        message: format!(
            "text lies where the phone's interface covers the picture (top {top:.0} px, from {bottom:.0} px down): {}",
            examples.join("; ")
        ),
        fix: "Keep important text inside the safe area: at 1080x1920 not in the top 150 px or the bottom 250 px (scaled for other sizes).",
        weight: 8.0,
    })
}

/// How many distinct things move at `at`: animated nodes of the scenes on screen, and the camera.
fn moving_layers(v: &Video, scenes: &[Scene], at: u64) -> HashSet<String> {
    let mut moving = HashSet::new();
    for s in scenes
        .iter()
        .filter(|s| s.placed.start_ms <= at && at < s.placed.end_ms)
    {
        for a in v
            .animations
            .iter()
            .filter(|a| s.stats.animated.contains(&a.node) && a.moving_at(at as f64))
        {
            moving.insert(a.node.to_string());
        }
    }
    if v.animations
        .iter()
        .any(|a| a.prop.is_camera() && a.moving_at(at as f64))
    {
        moving.insert("camera".to_string());
    }
    moving
}

/// RV017: a stretch of two seconds in which fewer than three layers move.
pub fn thin_stretch(v: &Video, scenes: &[Scene]) -> Option<Draft> {
    let duration = v.duration_ms();
    let last_start = duration.checked_sub(DENSITY_WINDOW_MS + HOLD_MS)?;
    let mut thin = Vec::new();
    for start in (0..=last_start).step_by(DENSITY_STEP_MS as usize * 2) {
        let layers: HashSet<String> = (start..start + DENSITY_WINDOW_MS)
            .step_by(DENSITY_STEP_MS as usize)
            .flat_map(|at| moving_layers(v, scenes, at))
            .collect();
        if layers.len() < MIN_LAYERS {
            thin.push(start);
        }
    }
    let first = *thin.first()?;
    let hits = scenes_where(scenes, |s| {
        thin.iter()
            .any(|t| s.placed.start_ms < t + DENSITY_WINDOW_MS && *t < s.placed.end_ms)
    });
    Some(Draft {
        code: "RV017",
        name: "thin-stretch",
        message: format!(
            "fewer than {MIN_LAYERS} layers move in {} stretches of 2 s, the first from {:.1} s",
            thin.len(),
            first as f64 / 1000.0
        ),
        scenes: hits,
        fix: "Keep at least three different things moving in every two seconds: backdrop drift, the hero, particles or props, the camera, each on its own timing.",
        weight: 8.0,
    })
}

/// Everything a scene shows, to tell two scenes that are the same picture.
fn content_hash(doc: &Document, s: &Scene) -> (u64, usize) {
    let nodes = doc
        .node(&s.frame)
        .map(scene::visible_nodes)
        .unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    for n in &nodes {
        n.kind.as_str().hash(&mut hasher);
        for v in [n.x, n.y, n.width, n.height, n.rotation, n.opacity] {
            (v * 100.0).round().to_bits().hash(&mut hasher);
        }
        n.text.hash(&mut hasher);
        n.image.hash(&mut hasher);
        n.path.hash(&mut hasher);
        format!("{:?}{:?}", n.fill, n.stroke).hash(&mut hasher);
    }
    (hasher.finish(), nodes.len())
}

/// RV018: two scenes that are exactly the same picture.
pub fn identical_scenes(doc: &Document, scenes: &[Scene]) -> Option<Draft> {
    let hashes: Vec<(u64, usize)> = scenes.iter().map(|s| content_hash(doc, s)).collect();
    let hits = scenes_where(scenes, |s| {
        let me = scenes
            .iter()
            .position(|o| std::ptr::eq(o, s))
            .unwrap_or_default();
        let (hash, count) = hashes[me];
        count >= MIN_IDENTICAL_NODES
            && s.len_ms() >= MIN_IDENTICAL_MS
            && scenes
                .iter()
                .zip(&hashes)
                .enumerate()
                .any(|(i, (o, (h, _)))| i != me && *h == hash && o.len_ms() >= MIN_IDENTICAL_MS)
    });
    (!hits.is_empty()).then(|| Draft {
        code: "RV018",
        name: "identical-scene-repeated",
        message: format!("scenes that are exactly the same picture: {}", scene_list(&hits, scenes)),
        scenes: hits,
        fix: "Do not show the same scene twice: change what the second one shows (another angle, scale, hero pose, text), or cut it.",
        weight: 8.0,
    })
}

/// RV019: the last picture should hold still for [`HOLD_MS`], like a poster. Looping ambient life
/// may go on under it; a move still under way at the end, or a slow zoom, may not.
pub fn no_final_hold(v: &Video) -> Option<Draft> {
    let duration = v.duration_ms();
    if duration < HOLD_MS * 2 {
        return None;
    }
    let from = duration - HOLD_MS;
    let still_moving = (from..duration).step_by(HOLD_STEP_MS as usize).any(|at| {
        v.animations
            .iter()
            .any(|a| a.repeat.is_once() && a.moving_at(at as f64))
    });
    let zooming = v
        .clips
        .last()
        .is_some_and(|c| (c.motion.scale_to - c.motion.scale_from).abs() > f64::EPSILON);
    (still_moving || zooming).then(|| Draft {
        code: "RV019",
        name: "no-final-hold",
        scenes: Vec::new(),
        message: format!(
            "motion is still under way in the last {HOLD_MS} ms, so the video ends on a moving picture instead of a held one"
        ),
        fix: "Let every move land at least 0.8 s before the end and hold the last picture (the end card or poster); only ambient loops may go on.",
        weight: 4.0,
    })
}
