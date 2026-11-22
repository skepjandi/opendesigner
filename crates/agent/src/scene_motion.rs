//! Named moves for any node and for the video camera, written as ordinary keyframes
//! (`docs/wiki/video-editing.md#scene-motion`). The functions here are pure: they take the pose a
//! node (or the camera) has where the move starts and return the tracks that perform the move.
//! `crate::motion` turns the tracks into `animate` operations for the `animate_preset` tool, and
//! the scenery blocks use the same builders for their ambient loops, so a block's float and a
//! `float` preset are the same motion.
//!
//! Ambient moves (`float`, `bob`, `breathe`, `pulse`, `spin`, `wobble`, `swing`, `orbit`,
//! `camera_drift`) loop by default and end on the value they start from, so the cycle has no seam.
//! `seed` shifts where in the cycle the move begins, so a row of nodes given different seeds does
//! not move in lockstep. Entrances and impacts play once.

use std::f64::consts::TAU;

use od_core::Ease;
use od_core::animation::{AnimProp, Key, MAX_KEYS, Repeat};
use od_core::transform::{MAX_CAMERA_PAN, MAX_CAMERA_ZOOM, MIN_CAMERA_ZOOM};

mod burst;
mod stagger;
#[cfg(test)]
mod tests;

pub use stagger::{
    Axis, Metric, Mulberry32, Origin, StaggerSpec, Step, stagger_offsets_ms, stagger_values,
};

/// A small deterministic generator (SplitMix64): the same seed always gives the same numbers, so
/// every move and every block built from a seed is byte-identical from run to run.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number from 0 up to, not including, 1.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// A whole number from `lo` to `hi`, both included.
    pub fn count(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi.saturating_sub(lo) + 1)
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.next_u64() as usize % (i + 1);
            items.swap(i, j);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Up,
    Down,
}

impl Side {
    pub fn parse(s: &str) -> Option<Side> {
        Some(match s {
            "left" => Side::Left,
            "right" => Side::Right,
            "up" => Side::Up,
            "down" => Side::Down,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Up => "up",
            Side::Down => "down",
        }
    }

    fn horizontal(self) -> bool {
        matches!(self, Side::Left | Side::Right)
    }

    /// `-1` toward the top or left, `1` toward the bottom or right.
    fn sign(self) -> f64 {
        match self {
            Side::Left | Side::Up => -1.0,
            Side::Right | Side::Down => 1.0,
        }
    }
}

/// A move that works on a node's own numbers, or on the camera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Float,
    Bob,
    Breathe,
    Pulse,
    Spin,
    Wobble,
    Shake,
    Swing,
    PopIn,
    BounceIn,
    ElasticIn,
    SlideIn(Side),
    FadeUp,
    ZoomIn,
    DrawOn,
    Orbit,
    CameraPushIn,
    CameraPullBack,
    CameraPan,
    CameraDrift,
    CameraShake,
    CameraWhip,
    CameraBurst,
}

impl Named {
    /// Every name the tool offers, in the order it lists them.
    pub const NAMES: [&'static str; 26] = [
        "float",
        "bob",
        "breathe",
        "pulse",
        "spin",
        "wobble",
        "shake",
        "swing",
        "pop_in",
        "bounce_in",
        "elastic_in",
        "slide_in_left",
        "slide_in_right",
        "slide_in_up",
        "slide_in_down",
        "fade_up",
        "zoom_in",
        "draw_on",
        "orbit",
        "camera_push_in",
        "camera_pull_back",
        "camera_pan",
        "camera_drift",
        "camera_shake",
        "camera_whip",
        "camera_burst",
    ];

    pub fn parse(name: &str) -> Option<Named> {
        Some(match name {
            "float" => Named::Float,
            "bob" => Named::Bob,
            "breathe" => Named::Breathe,
            "pulse" => Named::Pulse,
            "spin" => Named::Spin,
            "wobble" => Named::Wobble,
            "shake" => Named::Shake,
            "swing" => Named::Swing,
            "pop_in" => Named::PopIn,
            "bounce_in" => Named::BounceIn,
            "elastic_in" => Named::ElasticIn,
            "slide_in_left" => Named::SlideIn(Side::Left),
            "slide_in_right" => Named::SlideIn(Side::Right),
            "slide_in_up" => Named::SlideIn(Side::Up),
            "slide_in_down" => Named::SlideIn(Side::Down),
            "fade_up" => Named::FadeUp,
            "zoom_in" => Named::ZoomIn,
            "draw_on" => Named::DrawOn,
            "orbit" => Named::Orbit,
            "camera_push_in" => Named::CameraPushIn,
            "camera_pull_back" => Named::CameraPullBack,
            "camera_pan" => Named::CameraPan,
            "camera_drift" => Named::CameraDrift,
            "camera_shake" => Named::CameraShake,
            "camera_whip" => Named::CameraWhip,
            "camera_burst" => Named::CameraBurst,
            _ => return None,
        })
    }

    pub fn is_camera(self) -> bool {
        matches!(
            self,
            Named::CameraPushIn
                | Named::CameraPullBack
                | Named::CameraPan
                | Named::CameraDrift
                | Named::CameraShake
                | Named::CameraWhip
                | Named::CameraBurst
        )
    }

    /// Whether the move cycles by default.
    pub fn ambient(self) -> bool {
        matches!(
            self,
            Named::Float
                | Named::Bob
                | Named::Breathe
                | Named::Pulse
                | Named::Spin
                | Named::Wobble
                | Named::Swing
                | Named::Orbit
                | Named::CameraDrift
        )
    }

    /// Whether one ease runs over the whole move (the entrances), so a spring ease needs the
    /// move to last as long as the spring takes to rest.
    pub fn spans_ease(self) -> bool {
        matches!(
            self,
            Named::PopIn
                | Named::BounceIn
                | Named::ElasticIn
                | Named::SlideIn(_)
                | Named::FadeUp
                | Named::ZoomIn
                | Named::DrawOn
        )
    }

    /// How long the move takes (a cycle, for an ambient one) when the caller gives none.
    pub fn default_duration(self) -> u64 {
        match self {
            Named::Float => 4_000,
            Named::Bob => 1_800,
            Named::Breathe => 3_200,
            Named::Pulse => 1_200,
            Named::Spin => 4_000,
            Named::Wobble => 1_600,
            Named::Shake => 500,
            Named::Swing => 2_400,
            Named::PopIn => 450,
            Named::BounceIn => 800,
            Named::ElasticIn => 900,
            Named::SlideIn(_) => 550,
            Named::FadeUp | Named::ZoomIn => 500,
            Named::DrawOn => 900,
            Named::Orbit => 8_000,
            Named::CameraPushIn | Named::CameraPullBack => 3_000,
            Named::CameraPan => 2_000,
            Named::CameraDrift => 12_000,
            Named::CameraShake => 600,
            Named::CameraWhip => 450,
            Named::CameraBurst => 600,
        }
    }

    /// The ease the move uses when the caller names none.
    pub fn default_ease(self) -> Ease {
        match self {
            Named::PopIn => Ease::EaseOutBack,
            Named::BounceIn => Ease::Bounce,
            Named::ElasticIn => Ease::Elastic,
            Named::SlideIn(_) | Named::FadeUp | Named::ZoomIn => Ease::EaseOut,
            Named::Spin | Named::Orbit => Ease::Linear,
            Named::CameraWhip => Ease::EaseInOutCubic,
            Named::CameraBurst => Ease::Power4Out,
            _ => Ease::EaseInOut,
        }
    }
}

/// What a node is where a move starts (its animated values at that moment), and the size of the
/// stage it moves on, which scales moves that cross the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub opacity: f64,
    pub stage_w: f64,
    pub stage_h: f64,
}

impl Pose {
    /// The node's longer side, or the stage's tenth when it has no size of its own.
    fn extent(&self) -> f64 {
        let own = self.width.max(self.height);
        if own > 0.0 {
            own
        } else {
            self.stage_w.min(self.stage_h) * 0.1
        }
    }

    fn center(&self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// The camera where a move starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
    pub rotation: f64,
    pub stage_w: f64,
    pub stage_h: f64,
}

/// How a move is asked for.
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub start_ms: u64,
    pub duration_ms: u64,
    pub ease: Option<Ease>,
    /// How far the move goes: 1 is as designed.
    pub intensity: f64,
    /// `None` is the move's own habit: ambient moves loop, the rest play once.
    pub repeat: Option<Repeat>,
    pub seed: u64,
    /// Which way a pan or whip goes (`camera_pan` defaults to right).
    pub side: Option<Side>,
    /// How far a camera pan goes, in document units; `None` is an eighth of the stage.
    pub amount: Option<f64>,
    /// The closest two keys may be, so keys survive snapping to the video's frames.
    pub gap_ms: u64,
}

impl Spec {
    pub fn new(start_ms: u64, duration_ms: u64) -> Spec {
        Spec {
            start_ms,
            duration_ms,
            ease: None,
            intensity: 1.0,
            repeat: None,
            seed: 0,
            side: None,
            amount: None,
            gap_ms: 34,
        }
    }

    pub fn end_ms(&self) -> u64 {
        self.start_ms + self.duration_ms
    }

    fn phase(&self) -> f64 {
        Rng::new(self.seed).unit()
    }

    fn looped(&self, named: Named) -> Repeat {
        self.repeat.unwrap_or(if named.ambient() {
            Repeat::Loop
        } else {
            Repeat::Once
        })
    }

    fn at(&self, fraction: f64) -> u64 {
        self.start_ms + (self.duration_ms as f64 * fraction).round() as u64
    }
}

/// One property's keys and what it does after the last.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub prop: AnimProp,
    pub keys: Vec<Key>,
    pub repeat: Repeat,
}

fn key(at_ms: u64, value: f64, ease: Ease) -> Key {
    Key { at_ms, value, ease }
}

/// Keys at least `gap_ms` apart: a key too close to the one before is dropped (the last key of
/// the move is always kept, replacing the one before it), and at most [`MAX_KEYS`].
fn thinned(keys: Vec<Key>, gap_ms: u64) -> Vec<Key> {
    let count = keys.len();
    let mut out: Vec<Key> = Vec::with_capacity(count);
    for (i, k) in keys.into_iter().enumerate() {
        let close = out.last().is_some_and(|prev| k.at_ms < prev.at_ms + gap_ms);
        if !close {
            out.push(k);
        } else if i + 1 == count {
            if out.len() > 1 {
                out.pop();
            }
            out.push(k);
        }
    }
    out.truncate(MAX_KEYS);
    out
}

fn track(spec: &Spec, prop: AnimProp, keys: Vec<Key>, repeat: Repeat) -> Track {
    Track {
        prop,
        keys: thinned(keys, spec.gap_ms),
        repeat,
    }
}

fn once(spec: &Spec, prop: AnimProp, keys: Vec<Key>) -> Track {
    track(spec, prop, keys, Repeat::Once)
}

/// `want` steps over the move, fewer when they would come closer than a frame.
fn steps(spec: &Spec, want: u64) -> u64 {
    want.min(spec.duration_ms / (spec.gap_ms + 1)).max(1)
}

fn segments(spec: &Spec, lo: u64, hi: u64) -> u64 {
    steps(spec, (spec.duration_ms / 80).clamp(lo, hi)).max(2)
}

/// A value computed along the move: `f(u)` for `u` from 0 to 1, in `n` straight steps.
fn sampled(spec: &Spec, n: u64, f: impl Fn(f64) -> f64) -> Vec<Key> {
    (0..=n)
        .map(|i| {
            let u = i as f64 / n as f64;
            key(spec.at(u), f(u), Ease::Linear)
        })
        .collect()
}

/// A cycle of `f` (periodic in `u`) that ends exactly where it starts.
fn cycle(spec: &Spec, named: Named, prop: AnimProp, f: impl Fn(f64) -> f64) -> Track {
    let n = segments(spec, 12, 40);
    let mut keys = sampled(spec, n, f);
    if let Some(first) = keys.first().map(|k| k.value) {
        if let Some(last) = keys.last_mut() {
            last.value = first;
        }
    }
    track(spec, prop, keys, spec.looped(named))
}

fn sine(spec: &Spec, named: Named, prop: AnimProp, base: f64, amp: f64, shift: f64) -> Track {
    let phase = spec.phase() + shift;
    cycle(spec, named, prop, |u| {
        base + amp * (TAU * (u + phase)).sin()
    })
}

/// A swing that dies away: `amp` at the start, `base` at the end.
fn decaying(spec: &Spec, prop: AnimProp, base: f64, amp: f64, shift: f64) -> Track {
    let n = segments(spec, 12, 60);
    let waves = (spec.duration_ms as f64 / 90.0).round().clamp(3.0, 12.0);
    let phase = Rng::new(spec.seed).unit() * 0.5 + shift;
    let mut keys = sampled(spec, n, |u| {
        base + amp * (-4.0 * u).exp() * (TAU * (waves * u + phase)).sin()
    });
    if let Some(last) = keys.last_mut() {
        last.value = base;
    }
    track(spec, prop, keys, spec.repeat.unwrap_or(Repeat::Once))
}

fn grow(spec: &Spec, prop: AnimProp, from: f64, to: f64, ease: Ease) -> Track {
    once(
        spec,
        prop,
        vec![
            key(spec.start_ms, from, Ease::Linear),
            key(spec.end_ms(), to, ease),
        ],
    )
}

fn fade_in(spec: &Spec, pose: &Pose, share: f64) -> Track {
    once(
        spec,
        AnimProp::Opacity,
        vec![
            key(spec.start_ms, 0.0, Ease::Linear),
            key(
                spec.at(share)
                    .max(spec.start_ms + spec.gap_ms + 1)
                    .min(spec.end_ms()),
                pose.opacity,
                Ease::EaseOut,
            ),
        ],
    )
}

fn scale_both(spec: &Spec, pose: &Pose, from: f64, ease: Ease) -> Vec<Track> {
    vec![
        grow(
            spec,
            AnimProp::ScaleX,
            from * pose.scale_x,
            pose.scale_x,
            ease,
        ),
        grow(
            spec,
            AnimProp::ScaleY,
            from * pose.scale_y,
            pose.scale_y,
            ease,
        ),
    ]
}

/// The closest two keys of a block's loop may be: a whole number of frames at any video rate
/// from 10 frames a second up.
pub const LOOP_GAP_MS: u64 = 100;

fn loop_spec(period_ms: u64) -> Spec {
    Spec {
        gap_ms: LOOP_GAP_MS,
        ..Spec::new(0, period_ms)
    }
}

/// A looping sine of `prop` about `base`, `amp` either way, one cycle in `period_ms`, starting
/// `phase` of a cycle along. It ends on the value it starts from.
pub fn loop_sine(prop: AnimProp, base: f64, amp: f64, period_ms: u64, phase: f64) -> Track {
    cycle(&loop_spec(period_ms), Named::Float, prop, |u| {
        base + amp * (TAU * (u + phase)).sin()
    })
}

/// A looping track of `f(u)` for `u` from 0 to 1 over `period_ms`, in `want` straight pieces.
/// `f` must give the same value at both ends for the loop to have no seam.
pub fn loop_samples(prop: AnimProp, period_ms: u64, want: u64, f: impl Fn(f64) -> f64) -> Track {
    let spec = loop_spec(period_ms);
    let n = steps(&spec, want);
    let keys = sampled(&spec, n, f);
    track(&spec, prop, keys, Repeat::Loop)
}

/// A looping track through `(time_ms, value)` points joined by straight lines. The first and last
/// values should be the same, or the jump between them hidden by another track.
pub fn loop_keys(prop: AnimProp, points: &[(u64, f64)]) -> Track {
    let spec = loop_spec(points.last().map_or(LOOP_GAP_MS, |p| p.0));
    let keys = points
        .iter()
        .map(|&(at, value)| key(at, value, Ease::Linear))
        .collect();
    track(&spec, prop, keys, Repeat::Loop)
}

/// A straight run from `from` to `to` that starts again from `from`: the track of a layer that
/// scrolls one tile, whose copy then stands exactly where the layer began.
pub fn loop_ramp(prop: AnimProp, from: f64, to: f64, period_ms: u64) -> Track {
    let spec = loop_spec(period_ms);
    track(
        &spec,
        prop,
        vec![key(0, from, Ease::Linear), key(period_ms, to, Ease::Linear)],
        Repeat::Loop,
    )
}

/// The tracks that perform `named` on a node. Camera moves go through [`camera_tracks`].
pub fn node_tracks(named: Named, spec: &Spec, pose: &Pose) -> Vec<Track> {
    use AnimProp as P;
    let a = spec.intensity;
    let ext = pose.extent();
    let ease = spec.ease.unwrap_or(named.default_ease());
    let mut t = match named {
        Named::Float => vec![
            sine(spec, named, P::Y, pose.y, 0.03 * ext * a, 0.0),
            sine(spec, named, P::X, pose.x, 0.012 * ext * a, 0.25),
        ],
        Named::Bob => vec![
            sine(spec, named, P::Y, pose.y, 0.06 * ext * a, 0.0),
            sine(spec, named, P::Rotation, pose.rotation, 2.5 * a, 0.25),
        ],
        Named::Breathe => vec![
            sine(
                spec,
                named,
                P::ScaleY,
                pose.scale_y,
                0.035 * a * pose.scale_y,
                0.0,
            ),
            sine(
                spec,
                named,
                P::ScaleX,
                pose.scale_x,
                -0.0175 * a * pose.scale_x,
                0.0,
            ),
        ],
        Named::Pulse => {
            let phase = spec.phase();
            let beat = move |base: f64, u: f64| {
                base * (1.0 + 0.1 * a * (0.5 - 0.5 * (TAU * (u + phase)).cos()))
            };
            vec![
                cycle(spec, named, P::ScaleX, |u| beat(pose.scale_x, u)),
                cycle(spec, named, P::ScaleY, |u| beat(pose.scale_y, u)),
            ]
        }
        Named::Spin => spin(spec, named, pose, ease),
        Named::Wobble => {
            let phase = spec.phase();
            vec![
                cycle(spec, named, P::Rotation, |u| {
                    pose.rotation + 5.0 * a * (TAU * (2.0 * u + phase)).sin()
                }),
                cycle(spec, named, P::ScaleX, |u| {
                    pose.scale_x * (1.0 + 0.02 * a * (TAU * (2.0 * u + phase + 0.25)).sin())
                }),
                cycle(spec, named, P::ScaleY, |u| {
                    pose.scale_y * (1.0 - 0.02 * a * (TAU * (2.0 * u + phase + 0.25)).sin())
                }),
            ]
        }
        Named::Swing => vec![sine(spec, named, P::Rotation, pose.rotation, 14.0 * a, 0.0)],
        Named::Shake => vec![
            decaying(spec, P::X, pose.x, (0.03 * ext * a).max(3.0), 0.0),
            decaying(spec, P::Rotation, pose.rotation, 1.5 * a, 0.3),
        ],
        Named::PopIn => {
            let mut t = scale_both(spec, pose, 0.0, ease);
            t.push(fade_in(spec, pose, 0.25));
            t
        }
        Named::ElasticIn => {
            let mut t = scale_both(spec, pose, 0.0, ease);
            t.push(fade_in(spec, pose, 0.2));
            t
        }
        Named::BounceIn => vec![
            grow(spec, P::Y, pose.y - 0.45 * pose.stage_h * a, pose.y, ease),
            fade_in(spec, pose, 0.12),
        ],
        Named::SlideIn(side) => slide_in(spec, pose, side, ease),
        Named::FadeUp => vec![
            grow(spec, P::Y, pose.y + (0.2 * ext * a).max(12.0), pose.y, ease),
            fade_in(spec, pose, 0.8),
        ],
        Named::ZoomIn => {
            let mut t = scale_both(spec, pose, (1.0 - 0.4 * a).max(0.05), ease);
            t.push(fade_in(spec, pose, 0.6));
            t
        }
        Named::DrawOn => vec![grow(spec, P::TrimEnd, 0.0, 1.0, ease)],
        Named::Orbit => orbit(spec, pose, None, None),
        _ => Vec::new(),
    };
    t.retain(|track| track.keys.len() >= 2);
    t
}

fn spin(spec: &Spec, named: Named, pose: &Pose, ease: Ease) -> Vec<Track> {
    let turns = spec.intensity.round().max(1.0);
    let from = pose.rotation + spec.phase() * 360.0;
    let keys = vec![
        key(spec.start_ms, from, Ease::Linear),
        key(spec.end_ms(), from + 360.0 * turns, ease),
    ];
    vec![track(spec, AnimProp::Rotation, keys, spec.looped(named))]
}

fn slide_in(spec: &Spec, pose: &Pose, side: Side, ease: Ease) -> Vec<Track> {
    let (prop, home, span) = if side.horizontal() {
        (AnimProp::X, pose.x, pose.stage_w)
    } else {
        (AnimProp::Y, pose.y, pose.stage_h)
    };
    let dist = (pose.extent() * 1.2).max(span * 0.2) * spec.intensity;
    let from = home - side.sign() * dist;
    let keys = if spec.ease.is_some() {
        vec![
            key(spec.start_ms, from, Ease::Linear),
            key(spec.end_ms(), home, ease),
        ]
    } else {
        vec![
            key(spec.start_ms, from, Ease::Linear),
            key(
                spec.at(0.7),
                home + side.sign() * dist * 0.06,
                Ease::EaseOut,
            ),
            key(spec.end_ms(), home, Ease::EaseInOut),
        ]
    };
    vec![once(spec, prop, keys), fade_in(spec, pose, 0.35)]
}

/// A circle around `center` (default: a point one radius to the node's left, so the node starts
/// at the right of the circle). `radius` defaults to one and a half node sizes. The node's middle
/// travels the circle clockwise on screen, sampled at about 33 ms.
pub fn orbit(
    spec: &Spec,
    pose: &Pose,
    center: Option<(f64, f64)>,
    radius: Option<f64>,
) -> Vec<Track> {
    let (cx, cy) = pose.center();
    let (hub, r, theta0) = match center {
        Some((hx, hy)) => {
            let r = (cx - hx).hypot(cy - hy);
            ((hx, hy), r, (cy - hy).atan2(cx - hx))
        }
        None => {
            let r = radius.unwrap_or(1.5 * pose.extent() * spec.intensity);
            ((cx - r, cy), r, 0.0)
        }
    };
    let n = steps(spec, (spec.duration_ms / 33).clamp(24, 480));
    let point = |u: f64| {
        let a = theta0 + TAU * u;
        (hub.0 + r * a.cos(), hub.1 + r * a.sin())
    };
    let axis = |prop: AnimProp, pick: fn((f64, f64)) -> f64, half: f64| {
        let mut keys = sampled(spec, n, |u| pick(point(u)) - half);
        if let Some(first) = keys.first().map(|k| k.value) {
            if let Some(last) = keys.last_mut() {
                last.value = first;
            }
        }
        track(spec, prop, keys, spec.repeat.unwrap_or(Repeat::Loop))
    };
    vec![
        axis(AnimProp::X, |p| p.0, pose.width / 2.0),
        axis(AnimProp::Y, |p| p.1, pose.height / 2.0),
    ]
}

/// The node's middle riding `points` (parent coordinates) over the move, `ease` shaping how far
/// along it is at each moment. With `orient` the node also turns to face along the path (its
/// rotation is `pose.rotation` plus the path's heading, 0 degrees being toward the right).
pub fn follow(spec: &Spec, pose: &Pose, points: &[(f64, f64)], orient: bool) -> Vec<Track> {
    if points.len() < 2 {
        return Vec::new();
    }
    let ease = spec.ease.unwrap_or(Ease::EaseInOut);
    let straight = points.len() == 2
        && !matches!(
            ease,
            Ease::Back | Ease::EaseOutBack | Ease::Spring | Ease::Elastic | Ease::Bounce
        );
    let n = if straight {
        1
    } else {
        steps(spec, (spec.duration_ms / 33).clamp(8, 480))
    };
    let lengths = cumulative(points);
    let total = lengths.last().copied().unwrap_or(0.0);
    let (hw, hh) = (pose.width / 2.0, pose.height / 2.0);
    let progress = |i: u64| {
        let u = i as f64 / n as f64;
        if straight { u } else { ease.apply(u) }
    };
    let at = |p: f64| point_at(points, &lengths, p * total);
    let key_ease = if straight { ease } else { Ease::Linear };
    let build = |value: &dyn Fn(u64) -> f64| -> Vec<Key> {
        (0..=n)
            .map(|i| {
                key(
                    spec.at(i as f64 / n as f64),
                    value(i),
                    if i == 0 { Ease::Linear } else { key_ease },
                )
            })
            .collect()
    };
    let mut tracks = vec![
        once(spec, AnimProp::X, build(&|i| at(progress(i)).0 - hw)),
        once(spec, AnimProp::Y, build(&|i| at(progress(i)).1 - hh)),
    ];
    if orient {
        let headings = headings(points, &lengths, total, n, &progress);
        tracks.push(once(
            spec,
            AnimProp::Rotation,
            build(&|i| pose.rotation + headings[i as usize]),
        ));
    }
    tracks
}

fn headings(
    points: &[(f64, f64)],
    lengths: &[f64],
    total: f64,
    n: u64,
    progress: &dyn Fn(u64) -> f64,
) -> Vec<f64> {
    let eps = (total * 0.002).max(1e-6);
    let mut previous = 0.0;
    (0..=n)
        .map(|i| {
            let s = progress(i) * total;
            let (a, b) = (
                point_at(points, lengths, (s - eps).max(0.0)),
                point_at(points, lengths, (s + eps).min(total)),
            );
            let mut h = (b.1 - a.1).atan2(b.0 - a.0).to_degrees();
            while h - previous > 180.0 {
                h -= 360.0;
            }
            while h - previous < -180.0 {
                h += 360.0;
            }
            previous = h;
            h
        })
        .collect()
}

fn cumulative(points: &[(f64, f64)]) -> Vec<f64> {
    let mut sum = 0.0;
    let mut out = vec![0.0];
    for w in points.windows(2) {
        sum += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        out.push(sum);
    }
    out
}

/// The point `s` along a polyline whose running lengths are `lengths`.
pub fn point_at(points: &[(f64, f64)], lengths: &[f64], s: f64) -> (f64, f64) {
    let i = lengths
        .partition_point(|&l| l < s)
        .clamp(1, points.len().saturating_sub(1).max(1));
    let (a, b) = (points[i - 1], points[i.min(points.len() - 1)]);
    let span = lengths[i.min(lengths.len() - 1)] - lengths[i - 1];
    let t = if span > 0.0 {
        ((s - lengths[i - 1]) / span).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

/// `n` points evenly spaced along a polyline.
pub fn resample(points: &[(f64, f64)], n: usize) -> Vec<(f64, f64)> {
    let lengths = cumulative(points);
    let total = lengths.last().copied().unwrap_or(0.0);
    (0..n)
        .map(|i| point_at(points, &lengths, total * i as f64 / (n.max(2) - 1) as f64))
        .collect()
}

/// The tracks that perform a camera move. The camera props belong to the video, so the caller
/// writes them with the video's own id as the node.
pub fn camera_tracks(named: Named, spec: &Spec, cam: &CameraPose) -> Vec<Track> {
    use AnimProp as P;
    let a = spec.intensity;
    let ease = spec.ease.unwrap_or(named.default_ease());
    let side = spec.side.unwrap_or(Side::Right);
    let mut tracks = match named {
        Named::CameraPushIn => vec![camera_zoom(
            spec,
            cam.zoom,
            cam.zoom * (1.0 + 0.2 * a),
            ease,
        )],
        Named::CameraPullBack => {
            let (from, to) = if cam.zoom > 1.001 {
                (cam.zoom, (cam.zoom / (1.0 + 0.2 * a)).max(1.0))
            } else {
                (cam.zoom * (1.0 + 0.2 * a), cam.zoom)
            };
            vec![camera_zoom(spec, from, to, ease)]
        }
        Named::CameraPan => {
            let span = if side.horizontal() {
                cam.stage_w
            } else {
                cam.stage_h
            };
            let by = spec.amount.unwrap_or(span / 8.0 * a) * side.sign();
            let (prop, from) = if side.horizontal() {
                (P::CameraX, cam.x)
            } else {
                (P::CameraY, cam.y)
            };
            vec![grow(spec, prop, from, from + by, ease)]
        }
        Named::CameraDrift => vec![
            sine(spec, named, P::CameraX, cam.x, 0.012 * cam.stage_w * a, 0.0),
            sine(spec, named, P::CameraY, cam.y, 0.008 * cam.stage_h * a, 0.3),
            sine(spec, named, P::CameraRotation, cam.rotation, 0.3 * a, 0.6),
        ],
        Named::CameraShake => vec![
            decaying(spec, P::CameraX, cam.x, 0.012 * cam.stage_w * a, 0.0),
            decaying(spec, P::CameraY, cam.y, 0.012 * cam.stage_h * a, 0.25),
            decaying(spec, P::CameraRotation, cam.rotation, 0.6 * a, 0.5),
        ],
        Named::CameraWhip => whip(spec, cam, side, ease),
        Named::CameraBurst => burst::burst(spec, cam, side),
        _ => Vec::new(),
    };
    tracks.retain(|t| t.keys.len() >= 2);
    for t in &mut tracks {
        for k in &mut t.keys {
            k.value = clamp_camera(t.prop, k.value);
        }
    }
    tracks
}

fn camera_zoom(spec: &Spec, from: f64, to: f64, ease: Ease) -> Track {
    grow(spec, AnimProp::CameraZoom, from, to, ease)
}

fn whip(spec: &Spec, cam: &CameraPose, side: Side, ease: Ease) -> Vec<Track> {
    let by = spec.amount.unwrap_or(
        spec.intensity
            * if side.horizontal() {
                cam.stage_w
            } else {
                cam.stage_h
            }
            * 0.5,
    ) * side.sign();
    let (prop, from) = if side.horizontal() {
        (AnimProp::CameraX, cam.x)
    } else {
        (AnimProp::CameraY, cam.y)
    };
    let tilt = 2.0 * spec.intensity * side.sign();
    vec![
        grow(spec, prop, from, from + by, ease),
        once(
            spec,
            AnimProp::CameraRotation,
            vec![
                key(spec.start_ms, cam.rotation, Ease::Linear),
                key(spec.at(0.5), cam.rotation + tilt, Ease::EaseOut),
                key(spec.end_ms(), cam.rotation, Ease::EaseInOut),
            ],
        ),
    ]
}

fn clamp_camera(prop: AnimProp, v: f64) -> f64 {
    match prop {
        AnimProp::CameraZoom => v.clamp(MIN_CAMERA_ZOOM, MAX_CAMERA_ZOOM),
        AnimProp::CameraX | AnimProp::CameraY => v.clamp(-MAX_CAMERA_PAN, MAX_CAMERA_PAN),
        _ => v,
    }
}
