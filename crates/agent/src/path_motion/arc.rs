//! Arcs: a node that travels from one point to another along a curve instead of a straight
//! line. The curve is a quadratic Bezier whose control point sits off the straight line, sampled
//! into ordinary x and y keys about every 33 ms on the video's frame grid.

use od_core::Ease;
use od_core::animation::{AnimProp, Key, Repeat};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::scene_motion::Track;
use crate::tools::ToolError;

/// How far apart the sampled keys are.
pub const SAMPLE_MS: u64 = 33;
const MAX_SAMPLES: u64 = 480;
const EDGE: f64 = 1e-9;

pub type Pt = (f64, f64);

/// Which side of the straight line the arc bulges to, as the node travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bulge {
    /// Up (toward the top of the screen), or to the right on a vertical move: the same side
    /// whichever way the node travels.
    #[default]
    Auto,
    /// The path sweeps clockwise on screen (it bulges to the left of the way it goes).
    Cw,
    /// The path sweeps counter-clockwise on screen (it bulges to the right of the way it goes).
    Ccw,
}

/// The `arc` argument: how the straight run is bent.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArcSpec {
    /// How far off the straight line the arc bulges, as a share of the distance (0 straight).
    #[serde(default = "half")]
    pub strength: f64,
    /// Where along the run the bulge is: 0 at the start, 1 at the end.
    #[serde(default = "half")]
    pub peak: f64,
    #[serde(default)]
    pub direction: Bulge,
}

fn half() -> f64 {
    0.5
}

impl Default for ArcSpec {
    fn default() -> ArcSpec {
        ArcSpec {
            strength: 0.5,
            peak: 0.5,
            direction: Bulge::Auto,
        }
    }
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "description": "Bend the straight run into an arc: a quadratic Bezier whose control point is the start plus the run times `peak` plus the perpendicular times `strength` times the distance. Sampled into x/y keys every ~33 ms, so the node curves instead of sliding straight.",
        "properties": {
            "strength": {"type": "number", "minimum": 0, "maximum": 1, "description": "How far the path bulges, as a share of the distance (default 0.5; 0 is straight)."},
            "peak": {"type": "number", "minimum": 0, "maximum": 1, "description": "Where along the run the bulge is: 0 at the start, 1 at the end (default 0.5)."},
            "direction": {"type": "string", "enum": ["auto", "cw", "ccw"], "description": "Which side it bulges to: auto (up, or right on a vertical move, whichever way it goes), cw (the path sweeps clockwise on screen), ccw."}
        }
    })
}

/// The curve itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    pub from: Pt,
    pub control: Pt,
    pub to: Pt,
}

fn unit_side(delta: Pt, len: f64, direction: Bulge) -> Pt {
    let cw = (delta.1 / len, -delta.0 / len);
    let ccw = (-cw.0, -cw.1);
    match direction {
        Bulge::Cw => cw,
        Bulge::Ccw => ccw,
        Bulge::Auto => {
            if cw.1 < -EDGE || (cw.1.abs() <= EDGE && cw.0 > 0.0) {
                cw
            } else {
                ccw
            }
        }
    }
}

/// The arc from `from` to `to`.
pub fn curve(from: Pt, to: Pt, arc: &ArcSpec) -> Result<Curve, ToolError> {
    for (name, v) in [("strength", arc.strength), ("peak", arc.peak)] {
        if !(0.0..=1.0).contains(&v) {
            return Err(ToolError(format!("arc {name} {v} is not from 0 to 1")));
        }
    }
    let delta = (to.0 - from.0, to.1 - from.1);
    let len = delta.0.hypot(delta.1);
    if len.is_nan() || len <= EDGE {
        return Err(ToolError(
            "an arc needs somewhere to go: the start and the end are the same point".into(),
        ));
    }
    let side = unit_side(delta, len, arc.direction);
    let bulge = arc.strength * len;
    Ok(Curve {
        from,
        control: (
            from.0 + delta.0 * arc.peak + side.0 * bulge,
            from.1 + delta.1 * arc.peak + side.1 * bulge,
        ),
        to,
    })
}

impl Curve {
    pub fn at(&self, u: f64) -> Pt {
        let (a, b, c) = ((1.0 - u).powi(2), 2.0 * (1.0 - u) * u, u * u);
        (
            a * self.from.0 + b * self.control.0 + c * self.to.0,
            a * self.from.1 + b * self.control.1 + c * self.to.1,
        )
    }

    /// The heading of the curve at `u`, in degrees, 0 being toward the right.
    pub fn heading(&self, u: f64) -> f64 {
        let t = (
            2.0 * (1.0 - u) * (self.control.0 - self.from.0)
                + 2.0 * u * (self.to.0 - self.control.0),
            2.0 * (1.0 - u) * (self.control.1 - self.from.1)
                + 2.0 * u * (self.to.1 - self.control.1),
        );
        t.1.atan2(t.0).to_degrees()
    }
}

/// How many samples a move of `duration_ms` is cut into.
fn sample_count(duration_ms: u64) -> u64 {
    duration_ms.div_ceil(SAMPLE_MS).clamp(2, MAX_SAMPLES)
}

/// The times of the samples, on the video's frame grid, each with how far through the move it
/// is. The first and last are the move's ends.
pub fn sample_times(start_ms: u64, duration_ms: u64, fps: u32) -> Vec<(u64, f64)> {
    let n = sample_count(duration_ms);
    let mut out: Vec<(u64, f64)> = Vec::with_capacity(n as usize + 1);
    for i in 0..=n {
        let raw = start_ms + (duration_ms as f64 * i as f64 / n as f64).round() as u64;
        let at = crate::sequence::snap(raw, fps);
        let fraction = if duration_ms == 0 {
            1.0
        } else {
            ((at.saturating_sub(start_ms)) as f64 / duration_ms as f64).clamp(0.0, 1.0)
        };
        let count = out.len();
        match out.last_mut() {
            Some(last) if last.0 == at && count > 1 => *last = (at, fraction),
            Some(last) if last.0 == at => {}
            _ => out.push((at, fraction)),
        }
    }
    out
}

fn key(at_ms: u64, value: f64) -> Key {
    Key {
        at_ms,
        value,
        ease: Ease::Linear,
    }
}

fn track(prop: AnimProp, keys: Vec<Key>) -> Track {
    Track {
        prop,
        keys,
        repeat: Repeat::Once,
    }
}

/// The x and y tracks of a node riding `curve`, and with `orient` a rotation that follows its
/// heading (added to `base_rotation`, kept continuous so it never jumps by a turn). `progress`
/// is the eased share of the curve done at each sample.
pub fn tracks(
    curve: &Curve,
    samples: &[(u64, f64)],
    progress: &[f64],
    orient: Option<f64>,
) -> Vec<Track> {
    let mut xs = Vec::with_capacity(samples.len());
    let mut ys = Vec::with_capacity(samples.len());
    let mut turns = Vec::with_capacity(samples.len());
    let mut previous: Option<f64> = None;
    for ((at, _), u) in samples.iter().zip(progress) {
        let (x, y) = curve.at(*u);
        xs.push(key(*at, x));
        ys.push(key(*at, y));
        if let Some(base) = orient {
            let mut h = curve.heading(*u);
            if let Some(p) = previous {
                while h - p > 180.0 {
                    h -= 360.0;
                }
                while h - p < -180.0 {
                    h += 360.0;
                }
            }
            previous = Some(h);
            turns.push(key(*at, base + h));
        }
    }
    let mut out = vec![track(AnimProp::X, xs), track(AnimProp::Y, ys)];
    if orient.is_some() {
        out.push(track(AnimProp::Rotation, turns));
    }
    out
}

/// A track's value at `time_ms`, with its eases and no clamping.
fn value_in(keys: &[Key], time_ms: f64) -> f64 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return 0.0;
    };
    if time_ms <= first.at_ms as f64 {
        return first.value;
    }
    if time_ms >= last.at_ms as f64 {
        return last.value;
    }
    let i = keys.partition_point(|k| (k.at_ms as f64) <= time_ms);
    let (a, b) = (keys[i - 1], keys[i]);
    let span = (b.at_ms - a.at_ms) as f64;
    let p = b.ease.apply_in((time_ms - a.at_ms as f64) / span, span);
    a.value + (b.value - a.value) * p
}

/// Bends the straight run a move's x and y tracks make into an arc. The run's progress at each
/// moment is read from whichever axis travels farther, so the move keeps its own ease and timing.
/// A move that travels in one axis only (a slide in) is bent sideways from `rest`, the x and y
/// the node has where the move starts, and gains the track for the other axis. A move that does
/// not travel at all cannot be bent.
pub fn bend(
    mut tracks: Vec<Track>,
    arc: &ArcSpec,
    fps: u32,
    rest: Pt,
) -> Result<Vec<Track>, ToolError> {
    let find = |tracks: &[Track], prop: AnimProp| {
        tracks
            .iter()
            .position(|t| t.prop == prop && t.repeat == Repeat::Once && t.keys.len() >= 2)
    };
    let (found_x, found_y) = (find(&tracks, AnimProp::X), find(&tracks, AnimProp::Y));
    if found_x.is_none() && found_y.is_none() {
        return Err(ToolError(
            "`arc` bends a move that travels in x or y; this move does not (animate_arc takes \
             any two points)"
                .into(),
        ));
    }
    let axis = |tracks: &mut Vec<Track>, found: Option<usize>, prop: AnimProp| {
        found.unwrap_or_else(|| {
            tracks.push(Track {
                prop,
                keys: Vec::new(),
                repeat: Repeat::Once,
            });
            tracks.len() - 1
        })
    };
    let xi = axis(&mut tracks, found_x, AnimProp::X);
    let yi = axis(&mut tracks, found_y, AnimProp::Y);
    let ends = |t: &Track, rest: f64| {
        let a = t.keys.first().map_or(rest, |k| k.value);
        let b = t.keys.last().map_or(rest, |k| k.value);
        (a, b)
    };
    let ((x0, x1), (y0, y1)) = (ends(&tracks[xi], rest.0), ends(&tracks[yi], rest.1));
    let curve = curve((x0, y0), (x1, y1), arc)?;
    let (start, end) = [xi, yi]
        .iter()
        .filter_map(|i| tracks[*i].keys.first().zip(tracks[*i].keys.last()))
        .fold((u64::MAX, 0), |(lo, hi), (a, b)| {
            (lo.min(a.at_ms), hi.max(b.at_ms))
        });
    let lead = if (x1 - x0).abs() >= (y1 - y0).abs() {
        xi
    } else {
        yi
    };
    let (v0, v1) = ends(&tracks[lead], 0.0);
    let samples = sample_times(start, end.saturating_sub(start), fps);
    let progress: Vec<f64> = samples
        .iter()
        .map(|(at, _)| (value_in(&tracks[lead].keys, *at as f64) - v0) / (v1 - v0))
        .collect();
    let bent = self::tracks(&curve, &samples, &progress, None);
    tracks[xi].keys.clone_from(&bent[0].keys);
    tracks[yi].keys.clone_from(&bent[1].keys);
    Ok(tracks)
}
