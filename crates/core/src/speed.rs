//! Clip speed and speed ramps (`docs/wiki/video-editing.md#speed`).
//!
//! A footage clip's `duration_ms` is its length **on the timeline**. Its **source span** (how
//! much of the file it plays) is that length times the speed: a 2000 ms clip at 2x plays 4000 ms
//! of the file, at 0.5x it plays 1000 ms. A constant `speed` is 0.25 to 4. A *ramp* instead
//! gives the rate at moments of clip time (the clip's own timeline, from 0), eased from key to
//! key, and the source time is the integral of that rate; with a ramp the constant speed must
//! stay 1. Source times are whole milliseconds, integrated one millisecond at a time with the
//! midpoint rule, so every machine reads the same moment of the file for the same frame.
//!
//! Sound follows the picture: `od_media` retimes the clip's sound with `atempo` (pitch is kept).

use serde::{Deserialize, Serialize};

use crate::ease::Ease;
use crate::error::OpError;
use crate::limits::finite_in;
use crate::video::{Clip, MAX_CLIP_MS};

pub const MIN_SPEED: f64 = 0.25;
pub const MAX_SPEED: f64 = 4.0;
pub const MAX_RAMP_KEYS: usize = 32;

/// The playback rate at a moment of clip time, reached from the key before with `ease`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpeedKey {
    /// Milliseconds from the start of the clip.
    pub at_ms: u32,
    /// 0.25 to 4: how many seconds of the file play each second.
    pub rate: f64,
    #[serde(default)]
    pub ease: Ease,
}

pub fn check_speed(speed: f64) -> Result<f64, OpError> {
    finite_in("clip speed", speed, MIN_SPEED, MAX_SPEED)
}

/// A ramp is keys in strictly increasing time, each a rate from 0.25 to 4. Returns the rates
/// rounded as stored numbers are.
pub fn check_ramp(keys: &[SpeedKey]) -> Result<Vec<SpeedKey>, OpError> {
    if keys.len() > MAX_RAMP_KEYS {
        return Err(OpError::Limit(format!(
            "at most {MAX_RAMP_KEYS} speed ramp keys per clip"
        )));
    }
    if keys.windows(2).any(|w| w[0].at_ms >= w[1].at_ms) {
        return Err(OpError::OutOfBounds {
            what: "speed ramp",
            detail: "at_ms must strictly increase from one key to the next".to_string(),
        });
    }
    keys.iter()
        .map(|k| {
            if k.at_ms > MAX_CLIP_MS {
                return Err(OpError::OutOfBounds {
                    what: "speed ramp at_ms",
                    detail: format!("{} is later than {MAX_CLIP_MS} ms", k.at_ms),
                });
            }
            Ok(SpeedKey {
                rate: finite_in("speed ramp rate", k.rate, MIN_SPEED, MAX_SPEED)?,
                ..*k
            })
        })
        .collect()
}

/// A ramp and a constant speed are two ways to say the same thing, so only one may be set.
pub fn check_clip_speed(clip: &Clip) -> Result<(), OpError> {
    check_speed(clip.speed)?;
    check_ramp(&clip.speed_ramp)?;
    let retimed = clip.speed != 1.0 || !clip.speed_ramp.is_empty();
    if !clip.speed_ramp.is_empty() && clip.speed != 1.0 {
        return Err(OpError::OutOfBounds {
            what: "speed ramp",
            detail: "a ramp replaces the constant speed; set speed to 1 first".to_string(),
        });
    }
    if retimed && clip.footage.is_none() {
        return Err(OpError::OutOfBounds {
            what: "clip speed",
            detail: "only a clip of footage can be retimed; set speed 1 and an empty ramp first"
                .to_string(),
        });
    }
    Ok(())
}

fn rate_at(keys: &[SpeedKey], t_ms: f64) -> f64 {
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return 1.0;
    };
    if t_ms <= f64::from(first.at_ms) {
        return first.rate;
    }
    if t_ms >= f64::from(last.at_ms) {
        return last.rate;
    }
    let i = keys.partition_point(|k| f64::from(k.at_ms) <= t_ms);
    let (a, b) = (keys[i - 1], keys[i]);
    let span = f64::from(b.at_ms - a.at_ms);
    let p = b.ease.apply((t_ms - f64::from(a.at_ms)) / span);
    (a.rate + (b.rate - a.rate) * p).clamp(MIN_SPEED, MAX_SPEED)
}

/// Maps a moment of a clip's timeline to the moment of its file.
#[derive(Debug, Clone)]
pub struct SpeedMap {
    speed: f64,
    /// `source[t]` is the source time at `t` ms of clip time (empty for a constant speed).
    source: Vec<f64>,
}

impl SpeedMap {
    pub fn new(clip: &Clip) -> SpeedMap {
        let source = if clip.speed_ramp.is_empty() {
            Vec::new()
        } else {
            let mut table = Vec::with_capacity(clip.duration_ms as usize + 1);
            let mut at = 0.0;
            table.push(at);
            for t in 0..clip.duration_ms {
                at += rate_at(&clip.speed_ramp, f64::from(t) + 0.5);
                table.push(at);
            }
            table
        };
        SpeedMap {
            speed: clip.speed,
            source,
        }
    }

    /// Milliseconds into the file's span (from the clip's in point) at `clip_ms` of clip time.
    pub fn source_ms(&self, clip_ms: u64) -> u64 {
        match self.source.last() {
            None => (clip_ms as f64 * self.speed).round() as u64,
            Some(end) => {
                let at = self.source.get(clip_ms as usize).unwrap_or(end);
                at.round() as u64
            }
        }
    }

    /// The rate at `clip_ms` of clip time, for the clip's sound.
    pub fn rate_at(&self, clip: &Clip, clip_ms: f64) -> f64 {
        if clip.speed_ramp.is_empty() {
            self.speed
        } else {
            rate_at(&clip.speed_ramp, clip_ms)
        }
    }
}

impl Clip {
    /// Whether the clip plays its footage at anything but the recorded speed.
    pub fn retimed(&self) -> bool {
        self.speed != 1.0 || !self.speed_ramp.is_empty()
    }

    /// How much of its file the clip plays: its length times its speed.
    pub fn source_span_ms(&self) -> u64 {
        SpeedMap::new(self).source_ms(u64::from(self.duration_ms))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeId;
    use crate::video::{Footage, Motion, Transition};

    fn clip(duration_ms: u32, speed: f64, ramp: Vec<SpeedKey>) -> Clip {
        Clip {
            id: NodeId::parse("c_1").unwrap(),
            frame: None,
            footage: Some(Footage {
                file: "media/a.mp4".into(),
                in_ms: 0,
            }),
            duration_ms,
            transition: Transition::default(),
            motion: Motion::default(),
            speed,
            speed_ramp: ramp,
            grade: None,
        }
    }

    fn key(at_ms: u32, rate: f64, ease: Ease) -> SpeedKey {
        SpeedKey { at_ms, rate, ease }
    }

    #[test]
    fn a_constant_speed_scales_clip_time_into_source_time() {
        let c = clip(2000, 2.0, vec![]);
        assert_eq!(c.source_span_ms(), 4000);
        let m = SpeedMap::new(&c);
        assert_eq!(
            (m.source_ms(0), m.source_ms(500), m.source_ms(1000)),
            (0, 1000, 2000)
        );
        assert_eq!(clip(2000, 0.5, vec![]).source_span_ms(), 1000);
        assert_eq!(clip(2000, 1.0, vec![]).source_span_ms(), 2000);
        assert!(!clip(2000, 1.0, vec![]).retimed());
    }

    #[test]
    fn a_flat_ramp_equals_a_constant_speed_and_a_two_rate_ramp_integrates() {
        let flat = clip(1000, 1.0, vec![key(0, 2.0, Ease::Linear)]);
        assert_eq!(flat.source_span_ms(), 2000);
        let m = SpeedMap::new(&flat);
        assert_eq!(m.source_ms(500), 1000);
        // 1 -> 3 linearly over 1000 ms: the average rate is 2
        let ramp = clip(
            1000,
            1.0,
            vec![key(0, 1.0, Ease::Linear), key(1000, 3.0, Ease::Linear)],
        );
        assert_eq!(ramp.source_span_ms(), 2000);
        let m = SpeedMap::new(&ramp);
        // integral of 1 + 2t over t in 0..0.5 s is 0.5 + 0.25 = 0.75 s
        assert_eq!(m.source_ms(500), 750);
    }

    #[test]
    fn source_time_never_runs_backwards_and_holds_the_last_rate_after_the_last_key() {
        let c = clip(
            3000,
            1.0,
            vec![key(500, 0.25, Ease::EaseInOut), key(1500, 4.0, Ease::Back)],
        );
        let m = SpeedMap::new(&c);
        let mut last = 0;
        for t in 0..=3000 {
            let s = m.source_ms(t);
            assert!(s >= last, "{t}");
            last = s;
        }
        assert_eq!(m.source_ms(3000) - m.source_ms(2000), 4000);
        assert_eq!(m.source_ms(9999), m.source_ms(3000));
    }

    #[test]
    fn the_same_clip_maps_to_the_same_source_times_every_time() {
        let c = clip(
            2500,
            1.0,
            vec![key(0, 0.5, Ease::Spring), key(2000, 2.5, Ease::EaseOut)],
        );
        let (a, b) = (SpeedMap::new(&c), SpeedMap::new(&c));
        assert!((0..=2500).all(|t| a.source_ms(t) == b.source_ms(t)));
    }

    #[test]
    fn speeds_and_ramps_are_bounded_and_exclusive() {
        assert!(check_speed(0.25).is_ok() && check_speed(4.0).is_ok());
        assert!(
            check_speed(0.2).is_err()
                && check_speed(4.1).is_err()
                && check_speed(f64::NAN).is_err()
        );
        assert!(check_ramp(&[key(10, 1.0, Ease::Linear), key(10, 2.0, Ease::Linear)]).is_err());
        assert!(check_ramp(&[key(0, 5.0, Ease::Linear)]).is_err());
        let many: Vec<_> = (0..=MAX_RAMP_KEYS as u32)
            .map(|i| key(i, 1.0, Ease::Linear))
            .collect();
        assert!(check_ramp(&many).is_err());
        assert!(check_clip_speed(&clip(1000, 2.0, vec![key(0, 1.0, Ease::Linear)])).is_err());
        let mut frame_clip = clip(1000, 2.0, vec![]);
        frame_clip.footage = None;
        assert!(check_clip_speed(&frame_clip).is_err());
    }
}
