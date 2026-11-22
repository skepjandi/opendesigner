//! Measuring what a node's motion looks like: how far it travels, how fast, how it turns and
//! whether it comes back to where it started. Every length is a fraction of the frame height,
//! never an absolute unit, so the same scene measured at two sizes reads the same.

use od_core::{Document, NodeId, Video};
use serde::Deserialize;
use serde_json::{Value, json};

use super::geometry::{self, Placed};
use super::{Calibration, Gate, Timeline, frame_height, spread, tidy_times};

/// Moments sampled over a node's motion unless the caller asks for more.
pub const MEASURE_SAMPLES: usize = 25;
/// Ambient life must stay this small: it travels and rises less than this (frame heights).
pub const AMBIENT_TRAVEL_H: f64 = 0.02;
pub const AMBIENT_RISE_H: f64 = 0.02;
/// A node that travels less than this is in place.
pub const IN_PLACE_TRAVEL_H: f64 = 0.30;
/// Speeds that name a gait (frame heights a second).
pub const WALK_SPEED: f64 = 0.30;
pub const RUN_SPEED: f64 = 0.60;
pub const DASH_SPEED: f64 = 1.50;
/// A rise of this much is a jump; with the travel it is a leap.
pub const JUMP_RISE_H: f64 = 0.15;
pub const LEAP_TRAVEL_H: f64 = 0.50;
/// The feet that never lift more than this are planted.
pub const PLANTED_FOOT_H: f64 = 0.10;
/// A swing of this much, in place, is a gesture.
pub const GESTURE_HAND_H: f64 = 0.40;
/// How far a foot may slide while it is on the ground (frame heights).
pub const FOOT_SLIDE_H: f64 = 0.01;

/// The classes a motion can be named by, the ones that can be measured.
pub const CLASSES: [&str; 9] = [
    "idle", "in-place", "walk", "run", "dash", "jump", "leap", "planted", "gesture",
];

/// A foot-slide check.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GaitArg {
    pub node: String,
    #[serde(default)]
    pub stance: Vec<Stance>,
    #[serde(default)]
    pub character_height: Option<f64>,
    /// A node whose scroll the ground does (a floor that moves under a walking character).
    #[serde(default)]
    pub ground: Option<String>,
}

/// A stretch where the foot is on the ground.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stance {
    pub from_ms: f64,
    pub to_ms: f64,
}

/// The class a node was meant to have.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectArg {
    pub node: String,
    pub class: String,
}

/// What a node's motion measures.
#[derive(Debug, Clone, PartialEq)]
pub struct Motion {
    pub node: NodeId,
    /// How long the motion runs, in seconds.
    pub duration_s: f64,
    /// How far the node goes from where it starts (frame heights).
    pub travel: f64,
    /// How high it rises (frame heights).
    pub rise: f64,
    /// Travel a second (frame heights a second).
    pub speed: f64,
    /// How far it turns (degrees).
    pub rot_range: f64,
    /// How much its scale changes.
    pub scale_delta: f64,
    /// How far it ends from where it started, in degrees.
    pub pose_return: f64,
    /// How far it ends from where it started (frame heights).
    pub return_h: f64,
    /// How much the feet lift (frame heights).
    pub foot_range: f64,
    /// How far the hands or the body swing (frame heights).
    pub hand_range: f64,
    /// The node has feet to stand on (a child named for one).
    pub has_feet: bool,
    /// The node has hands to swing (a child named for one).
    pub has_hands: bool,
    pub labels: Vec<&'static str>,
    pub primary: &'static str,
    /// The class was named by the plan, not measured.
    pub intent_inferred: bool,
    /// The node is ambient life and must read as idle.
    pub ambient: bool,
}

/// Names a measured motion, from the table of the source this came from. A speed without a
/// distance is a guess, so the gait names need the node to have travelled as well: a node that
/// vibrates in place is not running.
pub fn classify(m: &Motion) -> (Vec<&'static str>, &'static str) {
    let mut labels = Vec::new();
    if m.travel < AMBIENT_TRAVEL_H && m.rise < AMBIENT_RISE_H {
        labels.push("idle");
    }
    if m.travel < IN_PLACE_TRAVEL_H {
        labels.push("in-place");
    }
    if m.travel >= IN_PLACE_TRAVEL_H {
        if m.speed >= DASH_SPEED {
            labels.push("dash");
        } else if m.speed >= RUN_SPEED {
            labels.push("run");
        } else if m.speed >= WALK_SPEED {
            labels.push("walk");
        }
    }
    if m.rise >= JUMP_RISE_H {
        labels.push(if m.travel >= LEAP_TRAVEL_H {
            "leap"
        } else {
            "jump"
        });
    }
    if m.has_feet && m.foot_range < PLANTED_FOOT_H {
        labels.push("planted");
    }
    if m.has_hands && m.hand_range >= GESTURE_HAND_H && m.travel < IN_PLACE_TRAVEL_H {
        labels.push("gesture");
    }
    let primary = [
        "idle", "leap", "jump", "dash", "run", "walk", "in-place", "planted", "gesture",
    ]
    .into_iter()
    .find(|c| labels.contains(c))
    .unwrap_or("in-place");
    (labels, primary)
}

/// Whether the motion of a node loops: its last pose returns to its first.
pub fn loops(m: &Motion) -> Option<bool> {
    super::track_gate::loop_rule(Some(m.pose_return), Some(m.return_h))
}

/// The tracks that move a node or its parents.
fn tracks_of<'a>(doc: &'a Document, video: &'a Video, id: &NodeId) -> Vec<&'a od_core::Animation> {
    let mut out: Vec<&od_core::Animation> = Vec::new();
    for a in &video.animations {
        if a.prop.is_camera() {
            continue;
        }
        if &a.node == id || geometry::is_ancestor_of(doc, &a.node, id) {
            out.push(a);
        }
    }
    out
}

/// Measures a node's motion over `samples` moments.
pub fn measure_node_motion(
    doc: &Document,
    video: &Video,
    id: &NodeId,
    samples: usize,
) -> Option<Motion> {
    let frame = super::frame_of(doc, video, id)?.0;
    let h = frame_height(doc, video);
    let tracks = tracks_of(doc, video, id);
    let mut first = f64::INFINITY;
    let mut last = f64::NEG_INFINITY;
    let mut repeats = false;
    for a in &tracks {
        let (Some(f), Some(l)) = (a.keys.first(), a.keys.last()) else {
            continue;
        };
        first = first.min(f.at_ms as f64);
        last = last.max(l.at_ms as f64);
        repeats |= a.repeat != od_core::Repeat::Once;
    }
    let (start, end) = if first.is_finite() && last > first {
        (first, last)
    } else {
        (0.0, super::last_ms(video))
    };
    // A repeating track wraps at its last key, so the pose before the wrap is the one that
    // shows whether the loop joins up.
    let end = if repeats { (end - 1.0).max(start) } else { end };
    let times = tidy_times(spread(start, end, samples.max(2)));
    let mut timeline = Timeline::new(doc, video);
    let mut centers: Vec<(f64, f64)> = Vec::new();
    let mut bottoms: Vec<f64> = Vec::new();
    let mut rotations: Vec<f64> = Vec::new();
    let mut scales: Vec<(f64, f64)> = Vec::new();
    let mut landmarks: Vec<(&'static str, f64, f64)> = Vec::new();
    for &t in &times {
        let at = timeline.at(t);
        let Some(p) = geometry::place(at, &frame, id) else {
            continue;
        };
        centers.push((
            p.rect.x + p.rect.width / 2.0,
            p.rect.y + p.rect.height / 2.0,
        ));
        bottoms.push(p.rect.bottom());
        rotations.push(p.rotation);
        scales.push((p.scale_x, p.scale_y));
        for (name, r) in geometry::landmarks(at, &frame, id) {
            landmarks.push((name, r.x + r.width / 2.0, r.bottom()));
        }
    }
    if centers.len() < 2 {
        return None;
    }
    let (x0, y0) = centers[0];
    let travel = centers
        .iter()
        .map(|(x, y)| ((x - x0).powi(2) + (y - y0).powi(2)).sqrt())
        .fold(0.0f64, f64::max)
        / h;
    let min_y = centers
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::INFINITY, f64::min);
    let max_y = centers
        .iter()
        .map(|(_, y)| *y)
        .fold(f64::NEG_INFINITY, f64::max);
    let rise = (max_y - min_y) / h;
    let duration_s = ((end - start) / 1000.0).max(1e-6);
    let rot_range = rotations.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - rotations.iter().copied().fold(f64::INFINITY, f64::min);
    let scale_delta = scales
        .iter()
        .map(|(sx, sy)| {
            let dx = if scales[0].0 != 0.0 {
                (sx / scales[0].0 - 1.0).abs()
            } else {
                0.0
            };
            let dy = if scales[0].1 != 0.0 {
                (sy / scales[0].1 - 1.0).abs()
            } else {
                0.0
            };
            dx.max(dy)
        })
        .fold(0.0f64, f64::max);
    let (xf, yf) = (centers[0].0, centers[0].1);
    let (xl, yl) = (centers[centers.len() - 1].0, centers[centers.len() - 1].1);
    let foot_range = landmark_range(&landmarks, "foot", bottoms.iter().copied(), h);
    let hand_range = landmark_range(&landmarks, "hand", centers.iter().map(|(_, y)| *y), h);
    let has_feet = landmarks.iter().any(|(name, _, _)| *name == "foot");
    let has_hands = landmarks.iter().any(|(name, _, _)| *name == "hand");
    let mut motion = Motion {
        node: id.clone(),
        duration_s,
        travel,
        rise,
        speed: travel / duration_s,
        rot_range,
        scale_delta,
        pose_return: (rotations[rotations.len() - 1] - rotations[0]).abs(),
        return_h: ((xl - xf).powi(2) + (yl - yf).powi(2)).sqrt() / h,
        foot_range,
        hand_range,
        has_feet,
        has_hands,
        labels: Vec::new(),
        primary: "in-place",
        intent_inferred: false,
        ambient: false,
    };
    let (labels, primary) = classify(&motion);
    motion.labels = labels;
    motion.primary = primary;
    Some(motion)
}

/// The range of a landmark (a foot, a hand) over the window, in frame heights. A node without
/// named landmarks falls back to its own edges.
fn landmark_range(
    landmarks: &[(&'static str, f64, f64)],
    kind: &str,
    fallback: impl Iterator<Item = f64>,
    h: f64,
) -> f64 {
    let named: Vec<f64> = landmarks
        .iter()
        .filter(|(name, _, _)| *name == kind)
        .map(|(_, x, _)| *x)
        .collect();
    if named.is_empty() {
        let values = fallback.collect::<Vec<f64>>();
        if values.is_empty() {
            return 0.0;
        }
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        return (max - min) / h;
    }
    let min = named.iter().copied().fold(f64::INFINITY, f64::min);
    let max = named.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (max - min) / h
}

/// Every node's motion, measured and named.
pub struct Measured {
    pub entries: Vec<Value>,
    pub gate: Gate,
}

/// Measures every node the video animates, and checks the plan's expectations.
pub fn measure_all(
    doc: &Document,
    video: &Video,
    ambient: &[String],
    expect: &[ExpectArg],
) -> Measured {
    let mut gate = Gate::new("motion", Calibration::Derived);
    let mut entries = Vec::new();
    let mut nodes: Vec<NodeId> = Vec::new();
    for a in &video.animations {
        if !a.prop.is_camera() && !nodes.contains(&a.node) {
            nodes.push(a.node.clone());
        }
    }
    for id in &nodes {
        let Some(mut m) = measure_node_motion(doc, video, id, MEASURE_SAMPLES) else {
            continue;
        };
        let own: Vec<&od_core::Animation> =
            video.animations.iter().filter(|a| &a.node == id).collect();
        m.ambient = ambient.iter().any(|a| a == id.as_str())
            || (!own.is_empty() && own.iter().all(|a| a.repeat != od_core::Repeat::Once));
        let mut warnings = Vec::new();
        if m.ambient && !(m.travel < AMBIENT_TRAVEL_H && m.rise < AMBIENT_RISE_H) {
            warnings.push(format!(
                "ambient-too-large: {} travels {:.3} and rises {:.3} of the frame height (an ambient loop must stay under {:.2})",
                id, m.travel, m.rise, AMBIENT_TRAVEL_H
            ));
        }
        for e in expect.iter().filter(|e| e.node == id.as_str()) {
            m.intent_inferred = !CLASSES.iter().any(|c| *c == e.class);
            if m.intent_inferred {
                warnings.push(format!(
                    "{}: the class {:?} names an intent, not a motion anyone can measure",
                    id, e.class
                ));
            } else if !m.labels.iter().any(|l| *l == e.class) {
                gate.failures.push(format!(
                    "{}: measured {:?}, not the planned {:?}",
                    id, m.labels, e.class
                ));
            }
        }
        let mut entry = json!({
            "node": id.as_str(),
            "duration_s": round(m.duration_s),
            "travel_h": round(m.travel),
            "rise_h": round(m.rise),
            "speed_h_per_s": round(m.speed),
            "rot_range_deg": round(m.rot_range),
            "scale_delta": round(m.scale_delta),
            "pose_return_deg": round(m.pose_return),
            "return_h": round(m.return_h),
            "foot_range_h": round(m.foot_range),
            "hand_range_h": round(m.hand_range),
            "labels": m.labels,
            "primary": m.primary,
            "ambient": m.ambient,
            "loops": loops(&m),
            "intent_inferred": m.intent_inferred,
        });
        entry["warnings"] = json!(warnings);
        entries.push(entry);
    }
    gate.data = json!({"nodes": entries.len()});
    Measured { entries, gate }
}

/// The foot-slide gate: a foot on the ground may not slide.
pub fn gait_gate(doc: &Document, video: &Video, gait: &[GaitArg]) -> Gate {
    let mut gate = Gate::new("G8", Calibration::Derived);
    let h = frame_height(doc, video);
    let mut timeline = Timeline::new(doc, video);
    for g in gait {
        let Ok(id) = od_core::NodeId::parse(&g.node) else {
            gate.failures.push(format!("{} is not a node id", g.node));
            continue;
        };
        if doc.node(&id).is_none() {
            gate.failures
                .push(format!("node {} does not exist", g.node));
            continue;
        }
        if g.stance.is_empty() {
            gate.unknowns.push(format!(
                "{}: no stance was declared, so the slide could not be measured",
                g.node
            ));
            continue;
        }
        let limit = FOOT_SLIDE_H * g.character_height.unwrap_or(h);
        let ground = g
            .ground
            .as_ref()
            .and_then(|n| od_core::NodeId::parse(n).ok())
            .and_then(|n| doc.node(&n).map(|_| n));
        for s in &g.stance {
            let times = tidy_times(spread(s.from_ms, s.to_ms, MEASURE_SAMPLES));
            let Some(contact) = times.first().copied() else {
                continue;
            };
            let start = sole(&mut timeline, video, &id, contact);
            let start_ground = ground
                .as_ref()
                .and_then(|g| sole(&mut timeline, video, g, contact));
            for t in times {
                let now = sole(&mut timeline, video, &id, t);
                let moved = match (&start, &now) {
                    (Some(a), Some(b)) => {
                        let mut d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();
                        if let (Some(ga), Some(gb)) = (&start_ground, ground.as_ref()) {
                            let gnow = sole(&mut timeline, video, gb, t);
                            if let Some(gnow) = gnow {
                                d = (d
                                    - ((ga.0 - gnow.0).powi(2) + (ga.1 - gnow.1).powi(2)).sqrt())
                                .max(0.0);
                            }
                        }
                        d
                    }
                    _ => continue,
                };
                if moved > limit * (1.0 + 1e-9) {
                    gate.failures.push(format!(
                        "{}: the foot slides {moved:.3} of the character height while it is on the ground (over {limit:.3})",
                        g.node
                    ));
                    break;
                }
            }
        }
    }
    gate
}

/// Where a node's sole is at a moment: the bottom centre of its box.
fn sole(tl: &mut Timeline, video: &Video, id: &NodeId, t: f64) -> Option<(f64, f64)> {
    let frame = super::frame_of(tl.doc, video, id)?.0;
    let at = tl.at(t);
    let p: Placed = geometry::place(at, &frame, id)?;
    Some((p.rect.x + p.rect.width / 2.0, p.rect.bottom()))
}

fn round(v: f64) -> f64 {
    (v * 10_000.0).round() / 10_000.0
}
