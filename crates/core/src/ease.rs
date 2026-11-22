//! Easing: how a value moves from its start to its end (`docs/wiki/video-editing.md#more-eases`).
//!
//! Every ease is a pure function from linear progress `t` (0 to 1) to eased progress. The named
//! ones are plain strings in files and tool arguments (`ease_out`, `steps_4`); the tunable ones
//! are a name with numbers, `cubic(0.25,0.1,0.25,1)`, `spring(450,0.3)`, `back_out(1.2)`. The
//! numbers are kept in thousandths so an `Ease` stays `Copy` and comparable, and so a file never
//! holds a float that prints differently the next time it is saved.
//!
//! A spring is the one ease that has a length of its own: it is a physical response over time.
//! Where the stretch it eases over is known (between two keyframes) it is read over that many
//! milliseconds with [`Ease::apply_in`], and the small distance it has left at the end of the
//! stretch is closed so the value always lands exactly on its key.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// What the tunable eases accept, as text a person or an agent can read.
pub const GRAMMAR: &str = "cubic(x1,y1,x2,y2) with x1 and x2 from 0 to 1, y1 and y2 from -2 to 3; \
spring(visual_ms,bounce) with visual_ms 50 to 5000 and bounce from -0.9 (slow, no overshoot) to \
0.95 (very bouncy); back_in|back_out|back_in_out(overshoot) from 0 to 5; elastic_out|elastic_in\
(amplitude,period) with amplitude 1 to 5 and period 0.05 to 2; steps(n) with n from 2 to 64; \
anticipate (pulls back first, then goes)";

/// Spring presets, by the visual duration in milliseconds and the bounce they stand for.
const SPRING_PRESETS: [(&str, u16, i16); 5] = [
    ("spring_snappy", 250, 0),
    ("spring_smooth", 400, 0),
    ("spring_playful", 450, 300),
    ("spring_bouncy", 500, 500),
    ("spring_heavy", 800, 100),
];

/// How a value moves from its start to its end over time (`t` from 0 to 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ease {
    Linear,
    /// Starts slowly, ends fast.
    EaseIn,
    /// Starts fast, settles slowly: the natural way things come to rest.
    EaseOut,
    /// Slow, fast, slow: the default for camera-like moves.
    #[default]
    EaseInOut,
    /// Overshoots the end a little and settles back: a pop or a bounce in.
    Back,
    /// Springs past the end and settles with a few small, damped swings: a physical arrival.
    Spring,
    /// The same curve as `ease_in_out`, under the name animation tools use.
    EaseInOutCubic,
    /// The same curve as `back`: overshoots the end a little, then settles.
    EaseOutBack,
    /// Snaps past the end and rings a few times before it settles, like a plucked string.
    Elastic,
    /// Falls to the end and bounces off it a few times, like a dropped ball.
    Bounce,
    /// Holds each of 2 poses (the start, then the end): a jump halfway through.
    Steps2,
    /// Holds each of 3 evenly spaced poses, jumping between them.
    Steps3,
    /// Holds each of 4 evenly spaced poses: animation "on fours".
    Steps4,
    /// Holds each of 6 evenly spaced poses.
    Steps6,
    /// Holds each of 8 evenly spaced poses.
    Steps8,
    /// Holds each of 12 evenly spaced poses.
    Steps12,
    /// Power curves: progress is `t` raised to the number in the name (2 or 4).
    Power2In,
    Power2Out,
    Power2InOut,
    Power4In,
    Power4Out,
    Power4InOut,
    /// Exponential curves: almost nothing, then everything (in), or the reverse (out).
    ExpoIn,
    ExpoOut,
    ExpoInOut,
    /// Circular curves: a quarter circle, steep at one end and flat at the other.
    CircIn,
    CircOut,
    CircInOut,
    /// Sine curves: the gentlest of the named eases.
    SineIn,
    SineOut,
    SineInOut,
    /// Pulls back a little before it goes, then arrives: the wind-up of a throw.
    Anticipate,
    /// A cubic Bezier through (0,0) and (1,1) with the two control points, the curve CSS and
    /// every design tool call `cubic-bezier`.
    Cubic(Bezier),
    /// `back` with a chosen overshoot, in thousandths (1700 is the classic 1.7).
    BackIn(i16),
    BackOut(i16),
    BackInOut(i16),
    /// A ringing arrival with a chosen amplitude and period, in thousandths.
    ElasticOut(Ring),
    ElasticIn(Ring),
    /// A damped spring defined by how long the move looks to take (it is mostly done by then)
    /// and how bouncy it is.
    Tuned(Spring),
    /// That many evenly spaced poses, any number from 2 to 64.
    StepsN(u8),
    /// Power curves with any exponent from 1 to 8, in thousandths: `power_out(3)` is `t` raised
    /// to 3 mirrored (the same as `power2_out` for 2); `power_out_in` is quick, slow, quick.
    PowerIn(i16),
    PowerOut(i16),
    PowerInOut(i16),
    PowerOutIn(i16),
    /// A dropped ball seen backwards: bounces away from the start (in), or both ends.
    BounceIn,
    BounceInOut,
}

/// The two control points of a `cubic(x1,y1,x2,y2)` ease, in thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bezier {
    pub x1: i16,
    pub y1: i16,
    pub x2: i16,
    pub y2: i16,
}

/// Amplitude and period of an elastic ease, in thousandths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ring {
    pub amplitude: i16,
    pub period: i16,
}

/// A spring by its visual duration in milliseconds and its bounce in thousandths: 0 settles
/// without overshoot, 500 overshoots by about 16 percent, negative is slow and heavy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spring {
    pub visual_ms: u16,
    pub bounce: i16,
}

impl Spring {
    fn damping_ratio(self) -> f64 {
        let bounce = f64::from(self.bounce) / 1000.0;
        if bounce < 0.0 {
            1.0 / (1.0 + bounce).max(0.05)
        } else {
            (1.0 - bounce).max(0.05)
        }
    }

    /// Radians per second, so that about 97 percent of a move without bounce is done by the
    /// visual duration.
    fn omega(self) -> f64 {
        std::f64::consts::TAU / (f64::from(self.visual_ms) / 1000.0 * 1.2)
    }

    /// The position `secs` after a rest start that is pulled to 1, with no starting speed.
    pub fn position(self, secs: f64) -> f64 {
        let zeta = self.damping_ratio();
        let omega = self.omega();
        let secs = secs.max(0.0);
        if (zeta - 1.0).abs() < 1e-6 {
            1.0 - (-omega * secs).exp() * (1.0 + omega * secs)
        } else if zeta < 1.0 {
            let damped = omega * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * omega * secs).exp();
            1.0 - decay * ((damped * secs).cos() + zeta * omega / damped * (damped * secs).sin())
        } else {
            let root = omega * (zeta * zeta - 1.0).sqrt();
            let slow = -zeta * omega + root;
            let fast = -zeta * omega - root;
            1.0 - (fast * (slow * secs).exp() - slow * (fast * secs).exp()) / (fast - slow)
        }
    }

    /// Milliseconds until the spring stays within 0.05 percent of its end.
    pub fn settle_ms(self) -> u32 {
        const STEP_MS: u32 = 5;
        const LIMIT_MS: u32 = 20_000;
        let mut last_off = 0;
        let mut ms = 0;
        while ms <= LIMIT_MS {
            if (1.0 - self.position(f64::from(ms) / 1000.0)).abs() > 0.0005 {
                last_off = ms;
            }
            ms += STEP_MS;
        }
        last_off + STEP_MS
    }
}

impl Ease {
    /// The eased progress for linear progress `t` (clamped to 0..=1; some eases pass 1 or 0).
    /// A tuned spring is read over twice its visual duration; use [`Ease::apply_in`] where the
    /// length of the stretch is known.
    pub fn apply(self, t: f64) -> f64 {
        let span_ms = match self {
            Ease::Tuned(spring) => f64::from(spring.visual_ms) * 2.0,
            _ => 0.0,
        };
        self.apply_in(t, span_ms)
    }

    /// The same, over a stretch that lasts `span_ms`: only a tuned spring cares.
    pub fn apply_in(self, t: f64, span_ms: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::EaseIn => t * t * t,
            Ease::EaseOut => 1.0 - (1.0 - t).powi(3),
            Ease::EaseInOut | Ease::EaseInOutCubic => in_out(t, |u| u * u * u),
            Ease::Back | Ease::EaseOutBack => back_out(t, 1.70158),
            Ease::Spring => {
                // a damped oscillation, nudged so it ends exactly at 1
                let wave = |t: f64| 1.0 - (-6.0 * t).exp() * (t * 4.5 * std::f64::consts::PI).cos();
                wave(t) + t * (1.0 - wave(1.0))
            }
            Ease::Elastic => elastic_out(t),
            Ease::Bounce => bounce_out(t),
            Ease::Steps2 => steps(t, 2),
            Ease::Steps3 => steps(t, 3),
            Ease::Steps4 => steps(t, 4),
            Ease::Steps6 => steps(t, 6),
            Ease::Steps8 => steps(t, 8),
            Ease::Steps12 => steps(t, 12),
            Ease::StepsN(n) => steps(t, u32::from(n)),
            Ease::PowerIn(p) => t.powf(milli(p)),
            Ease::PowerOut(p) => 1.0 - (1.0 - t).powf(milli(p)),
            Ease::PowerInOut(p) => in_out(t, |u| u.powf(milli(p))),
            Ease::PowerOutIn(p) => {
                if t < 0.5 {
                    (1.0 - (1.0 - 2.0 * t).powf(milli(p))) / 2.0
                } else {
                    0.5 + (2.0 * t - 1.0).powf(milli(p)) / 2.0
                }
            }
            Ease::BounceIn => 1.0 - bounce_out(1.0 - t),
            Ease::BounceInOut => in_out(t, |u| 1.0 - bounce_out(1.0 - u)),
            Ease::Power2In => t.powi(2),
            Ease::Power2Out => 1.0 - (1.0 - t).powi(2),
            Ease::Power2InOut => in_out(t, |u| u.powi(2)),
            Ease::Power4In => t.powi(4),
            Ease::Power4Out => 1.0 - (1.0 - t).powi(4),
            Ease::Power4InOut => in_out(t, |u| u.powi(4)),
            Ease::ExpoIn => expo_in(t),
            Ease::ExpoOut => 1.0 - expo_in(1.0 - t),
            Ease::ExpoInOut => in_out(t, expo_in),
            Ease::CircIn => circ_in(t),
            Ease::CircOut => 1.0 - circ_in(1.0 - t),
            Ease::CircInOut => in_out(t, circ_in),
            Ease::SineIn => sine_in(t),
            Ease::SineOut => 1.0 - sine_in(1.0 - t),
            Ease::SineInOut => in_out(t, sine_in),
            Ease::Anticipate => anticipate(t),
            Ease::Cubic(b) => cubic_bezier(b, t),
            Ease::BackOut(o) => back_out(t, milli(o)),
            Ease::BackIn(o) => 1.0 - back_out(1.0 - t, milli(o)),
            Ease::BackInOut(o) => in_out(t, |u| 1.0 - back_out(1.0 - u, milli(o))),
            Ease::ElasticOut(r) => elastic_ring(t, r),
            Ease::ElasticIn(r) => 1.0 - elastic_ring(1.0 - t, r),
            Ease::Tuned(spring) => {
                let secs = span_ms.max(1.0) / 1000.0;
                // what is left of the spring at the end of the stretch is spread over it, so
                // the value lands exactly on its key even when the stretch is short
                spring.position(t * secs) + t * (1.0 - spring.position(secs))
            }
        }
    }

    /// How long this ease takes to come to rest, when that is a property of the ease itself
    /// (a tuned spring); the other eases take whatever stretch they are given.
    pub fn natural_ms(self) -> Option<u32> {
        match self {
            Ease::Tuned(spring) => Some(spring.settle_ms()),
            _ => None,
        }
    }

    /// The plain names, in the order tools list them.
    pub const NAMES: [&'static str; 39] = [
        "linear",
        "ease_in",
        "ease_out",
        "ease_in_out",
        "back",
        "spring",
        "ease_in_out_cubic",
        "ease_out_back",
        "elastic",
        "bounce",
        "steps_2",
        "steps_3",
        "steps_4",
        "steps_6",
        "steps_8",
        "steps_12",
        "power2_in",
        "power2_out",
        "power2_in_out",
        "power4_in",
        "power4_out",
        "power4_in_out",
        "expo_in",
        "expo_out",
        "expo_in_out",
        "circ_in",
        "circ_out",
        "circ_in_out",
        "sine_in",
        "sine_out",
        "sine_in_out",
        "anticipate",
        "bounce_in",
        "bounce_in_out",
        "spring_snappy",
        "spring_smooth",
        "spring_playful",
        "spring_bouncy",
        "spring_heavy",
    ];

    fn plain(name: &str) -> Option<Ease> {
        Some(match name {
            "linear" => Ease::Linear,
            "ease_in" => Ease::EaseIn,
            "ease_out" => Ease::EaseOut,
            "ease_in_out" => Ease::EaseInOut,
            "back" => Ease::Back,
            "spring" => Ease::Spring,
            "ease_in_out_cubic" => Ease::EaseInOutCubic,
            "ease_out_back" => Ease::EaseOutBack,
            "elastic" => Ease::Elastic,
            "bounce" => Ease::Bounce,
            "steps_2" => Ease::Steps2,
            "steps_3" => Ease::Steps3,
            "steps_4" => Ease::Steps4,
            "steps_6" => Ease::Steps6,
            "steps_8" => Ease::Steps8,
            "steps_12" => Ease::Steps12,
            "power2_in" => Ease::Power2In,
            "power2_out" => Ease::Power2Out,
            "power2_in_out" => Ease::Power2InOut,
            "power4_in" => Ease::Power4In,
            "power4_out" => Ease::Power4Out,
            "power4_in_out" => Ease::Power4InOut,
            "expo_in" => Ease::ExpoIn,
            "expo_out" => Ease::ExpoOut,
            "expo_in_out" => Ease::ExpoInOut,
            "circ_in" => Ease::CircIn,
            "circ_out" => Ease::CircOut,
            "circ_in_out" => Ease::CircInOut,
            "sine_in" => Ease::SineIn,
            "sine_out" => Ease::SineOut,
            "sine_in_out" => Ease::SineInOut,
            "anticipate" => Ease::Anticipate,
            "bounce_in" => Ease::BounceIn,
            "bounce_in_out" => Ease::BounceInOut,
            _ => {
                let (_, visual_ms, bounce) =
                    SPRING_PRESETS.iter().find(|(preset, ..)| *preset == name)?;
                Ease::Tuned(Spring {
                    visual_ms: *visual_ms,
                    bounce: *bounce,
                })
            }
        })
    }

    fn plain_name(self) -> Option<&'static str> {
        Ease::NAMES
            .iter()
            .copied()
            .filter(|name| !name.starts_with("spring_"))
            .find(|name| Ease::plain(name) == Some(self))
    }
}

fn milli(value: i16) -> f64 {
    f64::from(value) / 1000.0
}

fn from_milli(value: f64) -> i16 {
    (value * 1000.0).round() as i16
}

fn show(value: i16) -> String {
    milli(value).to_string()
}

impl fmt::Display for Ease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(name) = self.plain_name() {
            return f.write_str(name);
        }
        match *self {
            Ease::Cubic(b) => write!(
                f,
                "cubic({},{},{},{})",
                show(b.x1),
                show(b.y1),
                show(b.x2),
                show(b.y2)
            ),
            Ease::BackIn(o) => write!(f, "back_in({})", show(o)),
            Ease::BackOut(o) => write!(f, "back_out({})", show(o)),
            Ease::BackInOut(o) => write!(f, "back_in_out({})", show(o)),
            Ease::ElasticOut(r) => {
                write!(f, "elastic_out({},{})", show(r.amplitude), show(r.period))
            }
            Ease::ElasticIn(r) => {
                write!(f, "elastic_in({},{})", show(r.amplitude), show(r.period))
            }
            Ease::Tuned(s) => write!(f, "spring({},{})", s.visual_ms, show(s.bounce)),
            Ease::StepsN(n) => write!(f, "steps({n})"),
            Ease::PowerIn(p) => write!(f, "power_in({})", show(p)),
            Ease::PowerOut(p) => write!(f, "power_out({})", show(p)),
            Ease::PowerInOut(p) => write!(f, "power_in_out({})", show(p)),
            Ease::PowerOutIn(p) => write!(f, "power_out_in({})", show(p)),
            _ => Err(fmt::Error),
        }
    }
}

/// An ease that cannot be read, with what would have been fine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EaseError(String);

impl fmt::Display for EaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EaseError {}

impl EaseError {
    fn new(what: &str) -> Self {
        EaseError(format!(
            "{what}; an ease is one of {} or {}",
            Ease::NAMES.join(", "),
            GRAMMAR
        ))
    }
}

fn number(text: &str, name: &str, low: f64, high: f64) -> Result<f64, EaseError> {
    let value: f64 = text
        .trim()
        .parse()
        .map_err(|_| EaseError::new(&format!("{name}: {text:?} is not a number")))?;
    if value.is_finite() && (low..=high).contains(&value) {
        Ok(value)
    } else {
        Err(EaseError::new(&format!(
            "{name}: {value} is outside {low} to {high}"
        )))
    }
}

fn parse_call(text: &str) -> Result<Ease, EaseError> {
    let (name, rest) = text
        .split_once('(')
        .ok_or_else(|| EaseError::new(&format!("unknown ease {text:?}")))?;
    let args: Vec<&str> = rest
        .strip_suffix(')')
        .ok_or_else(|| EaseError::new(&format!("{text:?} is missing its closing bracket")))?
        .split(',')
        .collect();
    let name = name.trim();
    let want = |n: usize| -> Result<(), EaseError> {
        if args.len() == n {
            Ok(())
        } else {
            Err(EaseError::new(&format!(
                "{name} takes {n} numbers, not {}",
                args.len()
            )))
        }
    };
    match name {
        "cubic" => {
            want(4)?;
            Ok(Ease::Cubic(Bezier {
                x1: from_milli(number(args[0], "x1", 0.0, 1.0)?),
                y1: from_milli(number(args[1], "y1", -2.0, 3.0)?),
                x2: from_milli(number(args[2], "x2", 0.0, 1.0)?),
                y2: from_milli(number(args[3], "y2", -2.0, 3.0)?),
            }))
        }
        "spring" => {
            want(2)?;
            Ok(Ease::Tuned(Spring {
                visual_ms: number(args[0], "visual_ms", 50.0, 5000.0)?.round() as u16,
                bounce: from_milli(number(args[1], "bounce", -0.9, 0.95)?),
            }))
        }
        "back_in" | "back_out" | "back_in_out" => {
            want(1)?;
            let overshoot = from_milli(number(args[0], "overshoot", 0.0, 5.0)?);
            Ok(match name {
                "back_in" => Ease::BackIn(overshoot),
                "back_out" => Ease::BackOut(overshoot),
                _ => Ease::BackInOut(overshoot),
            })
        }
        "elastic_out" | "elastic_in" => {
            want(2)?;
            let ring = Ring {
                amplitude: from_milli(number(args[0], "amplitude", 1.0, 5.0)?),
                period: from_milli(number(args[1], "period", 0.05, 2.0)?),
            };
            Ok(if name == "elastic_out" {
                Ease::ElasticOut(ring)
            } else {
                Ease::ElasticIn(ring)
            })
        }
        "power_in" | "power_out" | "power_in_out" | "power_out_in" => {
            want(1)?;
            let exponent = from_milli(number(args[0], "exponent", 1.0, 8.0)?);
            Ok(match name {
                "power_in" => Ease::PowerIn(exponent),
                "power_out" => Ease::PowerOut(exponent),
                "power_in_out" => Ease::PowerInOut(exponent),
                _ => Ease::PowerOutIn(exponent),
            })
        }
        "steps" => {
            want(1)?;
            let count = number(args[0], "n", 2.0, 64.0)?.round() as u8;
            Ok(Ease::plain(&format!("steps_{count}")).unwrap_or(Ease::StepsN(count)))
        }
        _ => Err(EaseError::new(&format!("unknown ease {name:?}"))),
    }
}

impl FromStr for Ease {
    type Err = EaseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let text = text.trim();
        match Ease::plain(text) {
            Some(ease) => Ok(ease),
            None => parse_call(text),
        }
    }
}

impl Serialize for Ease {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Ease {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(de::Error::custom)
    }
}

/// `count` poses spread from 0 to 1 and held for an equal share of the time each.
fn steps(t: f64, count: u32) -> f64 {
    let n = f64::from(count);
    ((t * n).floor().min(n - 1.0)) / (n - 1.0)
}

/// Builds the in-out curve from an ease-in curve: the in curve mirrored around the middle.
fn in_out(t: f64, ease_in: impl Fn(f64) -> f64) -> f64 {
    if t < 0.5 {
        ease_in(2.0 * t) / 2.0
    } else {
        1.0 - ease_in(2.0 - 2.0 * t) / 2.0
    }
}

fn expo_in(t: f64) -> f64 {
    if t <= 0.0 {
        0.0
    } else {
        2f64.powf(10.0 * t - 10.0)
    }
}

fn circ_in(t: f64) -> f64 {
    1.0 - (1.0 - t * t).max(0.0).sqrt()
}

fn sine_in(t: f64) -> f64 {
    1.0 - (t * std::f64::consts::FRAC_PI_2).cos()
}

fn back_out(t: f64, overshoot: f64) -> f64 {
    1.0 + (overshoot + 1.0) * (t - 1.0).powi(3) + overshoot * (t - 1.0).powi(2)
}

fn elastic_out(t: f64) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let c4 = std::f64::consts::TAU / 3.0;
    2f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * c4).sin() + 1.0
}

/// Rings around 1 with the given amplitude (at least 1) and period, starting exactly at 0.
fn elastic_ring(t: f64, ring: Ring) -> f64 {
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let amplitude = milli(ring.amplitude).max(1.0);
    let period = milli(ring.period).max(0.05);
    let shift = period / std::f64::consts::TAU * (1.0 / amplitude).asin();
    amplitude * 2f64.powf(-10.0 * t) * ((t - shift) * std::f64::consts::TAU / period).sin() + 1.0
}

fn bounce_out(t: f64) -> f64 {
    const N1: f64 = 7.5625;
    const D1: f64 = 2.75;
    if t < 1.0 / D1 {
        N1 * t * t
    } else if t < 2.0 / D1 {
        let t = t - 1.5 / D1;
        N1 * t * t + 0.75
    } else if t < 2.5 / D1 {
        let t = t - 2.25 / D1;
        N1 * t * t + 0.9375
    } else {
        let t = t - 2.625 / D1;
        N1 * t * t + 0.984_375
    }
}

/// Eases from 0 to 1 along a cubic Bezier: finds the curve parameter whose x is `t` (Newton's
/// method, with bisection where the slope is too flat to trust), then returns its y.
fn cubic_bezier(b: Bezier, t: f64) -> f64 {
    let (x1, y1, x2, y2) = (milli(b.x1), milli(b.y1), milli(b.x2), milli(b.y2));
    if t <= 0.0 || t >= 1.0 {
        return t;
    }
    let coefficient = |p1: f64, p2: f64| {
        let c = 3.0 * p1;
        let b = 3.0 * (p2 - p1) - c;
        (1.0 - c - b, b, c)
    };
    let (ax, bx, cx) = coefficient(x1, x2);
    let (ay, by, cy) = coefficient(y1, y2);
    let x_at = |s: f64| ((ax * s + bx) * s + cx) * s;
    let slope_at = |s: f64| (3.0 * ax * s + 2.0 * bx) * s + cx;

    let mut s = t;
    for _ in 0..8 {
        let error = x_at(s) - t;
        if error.abs() < 1e-7 {
            return ((ay * s + by) * s + cy) * s;
        }
        let slope = slope_at(s);
        if slope.abs() < 1e-6 {
            break;
        }
        s -= error / slope;
    }
    let (mut low, mut high) = (0.0, 1.0);
    s = t;
    for _ in 0..40 {
        let error = x_at(s) - t;
        if error.abs() < 1e-7 {
            break;
        }
        if error > 0.0 {
            high = s;
        } else {
            low = s;
        }
        s = (low + high) / 2.0;
    }
    ((ay * s + by) * s + cy) * s
}

/// Pulls back first (a wind-up), then goes; ends exactly at 1.
fn anticipate(t: f64) -> f64 {
    if t >= 1.0 {
        return 1.0;
    }
    let back_in = |p: f64| {
        1.0 - cubic_bezier(
            Bezier {
                x1: 330,
                y1: 1530,
                x2: 690,
                y2: 990,
            },
            1.0 - p,
        )
    };
    let p = t * 2.0;
    if p < 1.0 {
        0.5 * back_in(p)
    } else {
        0.5 * (2.0 - 2f64.powf(-10.0 * (p - 1.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_ease_runs_from_zero_to_one_and_only_back_overshoots() {
        for e in [
            Ease::Linear,
            Ease::EaseIn,
            Ease::EaseOut,
            Ease::EaseInOut,
            Ease::Back,
        ] {
            assert!(e.apply(0.0).abs() < 1e-9, "{e:?}");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9, "{e:?}");
            assert_eq!(e.apply(-3.0), e.apply(0.0));
            assert_eq!(e.apply(9.0), e.apply(1.0));
        }
        assert!(Ease::EaseIn.apply(0.5) < 0.5 && Ease::EaseOut.apply(0.5) > 0.5);
        assert!((Ease::EaseInOut.apply(0.5) - 0.5).abs() < 1e-9);
        assert!((Ease::Linear.apply(0.3) - 0.3).abs() < 1e-9);
        let peak = (1..100)
            .map(|i| Ease::Back.apply(f64::from(i) / 100.0))
            .fold(0.0, f64::max);
        assert!(peak > 1.05, "back overshoots: {peak}");
        for e in [Ease::EaseIn, Ease::EaseOut, Ease::EaseInOut] {
            assert!(
                (1..100).all(|i| e.apply(f64::from(i) / 100.0) <= 1.0),
                "{e:?}"
            );
        }
    }

    const MONOTONE: [Ease; 18] = [
        Ease::Power2In,
        Ease::Power2Out,
        Ease::Power2InOut,
        Ease::Power4In,
        Ease::Power4Out,
        Ease::Power4InOut,
        Ease::ExpoIn,
        Ease::ExpoOut,
        Ease::ExpoInOut,
        Ease::CircIn,
        Ease::CircOut,
        Ease::CircInOut,
        Ease::SineIn,
        Ease::SineOut,
        Ease::SineInOut,
        Ease::Steps2,
        Ease::Steps4,
        Ease::Steps12,
    ];

    #[test]
    fn named_eases_start_at_zero_end_at_one_and_never_go_backwards() {
        for e in MONOTONE {
            assert!(e.apply(0.0).abs() < 1e-9, "{e:?}");
            assert!((e.apply(1.0) - 1.0).abs() < 1e-9, "{e:?}");
            let mut last = 0.0;
            for i in 0..=1000 {
                let v = e.apply(f64::from(i) / 1000.0);
                assert!(v >= last - 1e-12 && v <= 1.0 + 1e-12, "{e:?} at {i}: {v}");
                last = v;
            }
        }
    }

    #[test]
    fn named_eases_have_their_known_midpoints() {
        let mid = |e: Ease| e.apply(0.5);
        assert!((mid(Ease::Power2In) - 0.25).abs() < 1e-9);
        assert!((mid(Ease::Power2Out) - 0.75).abs() < 1e-9);
        assert!((mid(Ease::Power4In) - 0.0625).abs() < 1e-9);
        assert!((mid(Ease::Power4Out) - 0.9375).abs() < 1e-9);
        assert!((mid(Ease::ExpoIn) - 2f64.powi(-5)).abs() < 1e-9);
        assert!((mid(Ease::CircIn) - (1.0 - 0.75f64.sqrt())).abs() < 1e-9);
        assert!((mid(Ease::SineIn) - (1.0 - 0.5f64.sqrt())).abs() < 1e-9);
        for e in [
            Ease::Power2InOut,
            Ease::Power4InOut,
            Ease::ExpoInOut,
            Ease::CircInOut,
            Ease::SineInOut,
        ] {
            assert!((mid(e) - 0.5).abs() < 1e-9, "{e:?}");
        }
        for (a, b) in [
            (Ease::ExpoIn, Ease::ExpoOut),
            (Ease::CircIn, Ease::CircOut),
            (Ease::SineIn, Ease::SineOut),
            (Ease::Power4In, Ease::Power4Out),
        ] {
            assert!((a.apply(0.3) - (1.0 - b.apply(0.7))).abs() < 1e-9, "{a:?}");
        }
    }

    #[test]
    fn steps_hold_each_pose_for_an_equal_share_of_the_time() {
        assert_eq!(Ease::Steps4.apply(0.0), 0.0);
        assert_eq!(Ease::Steps4.apply(0.24), 0.0);
        assert!((Ease::Steps4.apply(0.25) - 1.0 / 3.0).abs() < 1e-9);
        assert!((Ease::Steps4.apply(0.6) - 2.0 / 3.0).abs() < 1e-9);
        assert_eq!(Ease::Steps4.apply(0.75), 1.0);
        assert_eq!(Ease::Steps4.apply(1.0), 1.0);
        assert_eq!(Ease::Steps2.apply(0.49), 0.0);
        assert_eq!(Ease::Steps2.apply(0.5), 1.0);
    }

    #[test]
    fn named_eases_use_their_snake_case_names() {
        for (name, ease) in [
            ("steps_4", Ease::Steps4),
            ("power2_in_out", Ease::Power2InOut),
            ("power4_out", Ease::Power4Out),
            ("expo_in", Ease::ExpoIn),
            ("circ_out", Ease::CircOut),
            ("sine_in_out", Ease::SineInOut),
            ("ease_in_out_cubic", Ease::EaseInOutCubic),
            ("anticipate", Ease::Anticipate),
        ] {
            let json = serde_json::json!(name);
            assert_eq!(serde_json::from_value::<Ease>(json.clone()).unwrap(), ease);
            assert_eq!(serde_json::to_value(ease).unwrap(), json);
        }
    }

    #[test]
    fn every_listed_name_reads_back_as_itself() {
        for name in Ease::NAMES {
            let ease: Ease = name.parse().unwrap();
            if !name.starts_with("spring_") {
                assert_eq!(ease.to_string(), name);
            }
        }
    }

    fn tuned(visual_ms: u16, bounce: i16) -> Ease {
        Ease::Tuned(Spring { visual_ms, bounce })
    }

    #[test]
    fn tunable_eases_keep_their_numbers_through_a_file() {
        for text in [
            "cubic(0.25,0.1,0.25,1)",
            "spring(450,0.3)",
            "spring(600,-0.4)",
            "back_out(1.2)",
            "back_in(0.5)",
            "back_in_out(2)",
            "elastic_out(1,0.3)",
            "elastic_in(1.5,0.4)",
            "steps(5)",
        ] {
            let ease: Ease = text.parse().unwrap();
            assert_eq!(ease.to_string(), text);
            let json = serde_json::to_value(ease).unwrap();
            assert_eq!(json, serde_json::json!(text));
            assert_eq!(serde_json::from_value::<Ease>(json).unwrap(), ease);
        }
        assert_eq!(
            " cubic( 0.25 , 0.1,0.25,1 ) "
                .parse::<Ease>()
                .unwrap()
                .to_string(),
            "cubic(0.25,0.1,0.25,1)"
        );
        assert_eq!("steps(4)".parse::<Ease>().unwrap(), Ease::Steps4);
        assert_eq!("spring_playful".parse::<Ease>().unwrap(), tuned(450, 300));
    }

    #[test]
    fn ease_text_that_cannot_be_read_says_what_is_allowed() {
        for bad in [
            "cubic(2,0,0,1)",
            "cubic(0,0,1)",
            "spring(10,0)",
            "spring(300,1.5)",
            "back_out(9)",
            "steps(1)",
            "steps(65)",
            "bogus",
            "spring(300,0",
            "cubic(a,0,0,1)",
        ] {
            let error = bad.parse::<Ease>().unwrap_err().to_string();
            assert!(
                error.contains("steps_4") && error.contains("cubic("),
                "{bad}: {error}"
            );
        }
        assert!(serde_json::from_value::<Ease>(serde_json::json!("wobbly")).is_err());
    }

    #[test]
    fn a_cubic_bezier_matches_the_css_curves() {
        let linear: Ease = "cubic(0,0,1,1)".parse().unwrap();
        for i in 0..=20 {
            let t = f64::from(i) / 20.0;
            assert!((linear.apply(t) - t).abs() < 1e-6);
        }
        // the CSS keyword `ease` is cubic-bezier(0.25, 0.1, 0.25, 1); its y at x = 0.5 is
        // 0.80 (a quick start that settles slowly)
        let css_ease: Ease = "cubic(0.25,0.1,0.25,1)".parse().unwrap();
        assert!(
            (css_ease.apply(0.5) - 0.8024).abs() < 1e-3,
            "{}",
            css_ease.apply(0.5)
        );
        let overshoot: Ease = "cubic(0.33,1.53,0.69,0.99)".parse().unwrap();
        assert!((0..=100).any(|i| overshoot.apply(f64::from(i) / 100.0) > 1.05));
        assert!((overshoot.apply(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_spring_is_mostly_done_at_its_visual_duration() {
        // no bounce: 1 - e^-w t (1 + w t) with w t = 2 pi / 1.2 is 0.9668 at the visual duration
        let spring = Spring {
            visual_ms: 500,
            bounce: 0,
        };
        assert!((spring.position(0.5) - 0.9668).abs() < 1e-3);
        let ease = Ease::Tuned(spring);
        assert!((ease.apply_in(0.5, 1000.0) - 0.9669).abs() < 1e-3);
        assert!(ease.apply_in(0.0, 1000.0).abs() < 1e-9);
        assert!((ease.apply_in(1.0, 1000.0) - 1.0).abs() < 1e-9);
        let rising = (0..=200).map(|i| ease.apply_in(f64::from(i) / 200.0, 1000.0));
        assert!(
            rising
                .clone()
                .zip(rising.skip(1))
                .all(|(a, b)| b >= a - 1e-9)
        );
    }

    #[test]
    fn a_bouncy_spring_overshoots_by_the_damping_it_has() {
        // zeta = 0.5: the first peak is 1 + e^(-pi zeta / sqrt(1 - zeta^2)) = 1.163
        let spring = Spring {
            visual_ms: 500,
            bounce: 500,
        };
        let peak = (0..2000)
            .map(|i| spring.position(f64::from(i) / 1000.0))
            .fold(0.0, f64::max);
        assert!((peak - 1.163).abs() < 0.005, "{peak}");
        let heavy = Spring {
            visual_ms: 500,
            bounce: -500,
        };
        assert!((0..4000).all(|i| heavy.position(f64::from(i) / 1000.0) <= 1.0 + 1e-9));
    }

    #[test]
    fn a_short_stretch_still_lands_on_the_key() {
        let ease = tuned(800, 500);
        for span in [60.0, 200.0, 5000.0] {
            assert!((ease.apply_in(1.0, span) - 1.0).abs() < 1e-9, "{span}");
        }
        let settle = Spring {
            visual_ms: 500,
            bounce: 300,
        }
        .settle_ms();
        assert!((600..4000).contains(&settle), "{settle}");
        assert_eq!(tuned(500, 300).natural_ms(), Some(settle));
        assert_eq!(Ease::Spring.natural_ms(), None);
    }

    #[test]
    fn a_critically_damped_spring_has_no_seam() {
        // bounce 0 is zeta 1, the branch between the two formulas: both sides must agree
        let near = |bounce| {
            Spring {
                visual_ms: 400,
                bounce,
            }
            .position(0.3)
        };
        assert!((near(0) - near(1)).abs() < 1e-3);
        assert!((near(0) - near(-1)).abs() < 1e-3);
    }

    #[test]
    fn anticipate_pulls_back_then_arrives() {
        let dip = (1..50)
            .map(|i| Ease::Anticipate.apply(f64::from(i) / 100.0))
            .fold(0.0, f64::min);
        assert!(dip < -0.03, "{dip}");
        assert!((Ease::Anticipate.apply(0.5) - 0.5).abs() < 1e-9);
        assert_eq!(Ease::Anticipate.apply(1.0), 1.0);
        assert!(Ease::Anticipate.apply(0.0).abs() < 1e-9);
    }

    #[test]
    fn back_and_elastic_can_be_tuned() {
        let mild: Ease = "back_out(0.5)".parse().unwrap();
        let strong: Ease = "back_out(3)".parse().unwrap();
        let peak = |e: Ease| {
            (1..100)
                .map(|i| e.apply(f64::from(i) / 100.0))
                .fold(0.0, f64::max)
        };
        assert!(peak(strong) > peak(mild) + 0.1);
        assert!((peak(Ease::BackOut(1701)) - peak(Ease::Back)).abs() < 1e-3);
        let back_in: Ease = "back_in(1.7)".parse().unwrap();
        assert!(back_in.apply(0.2) < 0.0, "pulls back first");
        let ring: Ease = "elastic_out(1,0.3)".parse().unwrap();
        assert!(ring.apply(0.0).abs() < 1e-9 && (ring.apply(1.0) - 1.0).abs() < 1e-9);
        assert!(peak(ring) > 1.1);
        let ring_in: Ease = "elastic_in(1,0.3)".parse().unwrap();
        assert!((0..=100).any(|i| ring_in.apply(f64::from(i) / 100.0) < -0.1));
    }

    #[test]
    fn any_number_of_steps_holds_that_many_poses() {
        let five = Ease::StepsN(5);
        assert_eq!(five.apply(0.19), 0.0);
        assert!((five.apply(0.2) - 0.25).abs() < 1e-9);
        assert_eq!(five.apply(0.8), 1.0);
    }
}

#[cfg(test)]
mod power_and_bounce_tests {
    use super::*;

    fn ease(text: &str) -> Ease {
        text.parse().unwrap()
    }

    #[test]
    fn a_power_ease_with_any_exponent_matches_the_named_ones() {
        for i in 0..=20 {
            let t = f64::from(i) / 20.0;
            assert!((ease("power_out(2)").apply(t) - Ease::Power2Out.apply(t)).abs() < 1e-9);
            assert!((ease("power_in(4)").apply(t) - Ease::Power4In.apply(t)).abs() < 1e-9);
            assert!((ease("power_in_out(2)").apply(t) - Ease::Power2InOut.apply(t)).abs() < 1e-9);
        }
        assert!((ease("power_in_out(3)").apply(0.25) - 0.0625).abs() < 1e-9);
        assert!((ease("power_in_out(3)").apply(0.75) - 0.9375).abs() < 1e-9);
    }

    #[test]
    fn out_in_is_quick_slow_quick() {
        let out_in = ease("power_out_in(2)");
        assert!((out_in.apply(0.25) - 0.375).abs() < 1e-9);
        assert!((out_in.apply(0.5) - 0.5).abs() < 1e-9);
        assert!((out_in.apply(0.75) - 0.625).abs() < 1e-9);
        assert!(out_in.apply(0.0).abs() < 1e-9 && (out_in.apply(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn bounce_in_mirrors_bounce() {
        assert!((Ease::BounceIn.apply(0.5) - 0.234_375).abs() < 1e-9);
        assert!((Ease::Bounce.apply(0.5) - 0.765_625).abs() < 1e-9);
        assert!((Ease::BounceInOut.apply(0.5) - 0.5).abs() < 1e-9);
        assert!(Ease::BounceIn.apply(0.0).abs() < 1e-9);
        assert!((Ease::BounceInOut.apply(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn these_eases_keep_their_names_and_numbers() {
        for text in [
            "power_out(3)",
            "power_in(1.5)",
            "power_in_out(2.5)",
            "power_out_in(2)",
            "bounce_in",
            "bounce_in_out",
        ] {
            assert_eq!(ease(text).to_string(), text);
        }
        assert!("power_in(9)".parse::<Ease>().is_err());
        assert!("power_in(0.5)".parse::<Ease>().is_err());
    }
}
