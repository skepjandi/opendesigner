//! Smooth, seeded noise for organic motion: a camera that breathes, a shake, a drift, a flicker
//! (`docs/wiki/video-editing.md#noise-motion`).
//!
//! Value noise on a lattice, smoothed between lattice points, summed over octaves. It is a pure
//! function of the seed and the position using only integer hashing, `+`, `-`, `*` and `/`, so the
//! same keys are written on every machine and a frame drawn out of order agrees with the rest.
//! The motion is baked into ordinary keyframes by the tool; nothing is computed at render time.

/// 32-bit integer hash (Wang/Jenkins style mix): the lattice value for `(seed, i)`.
fn hash(seed: u32, i: i64) -> u32 {
    let mut h = seed ^ (i as u32).wrapping_mul(0x9E37_79B9) ^ ((i >> 32) as u32);
    h ^= h >> 16;
    h = h.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

/// The lattice value at integer `i`, from -1 to 1. With `period`, lattice points repeat every
/// `period` steps, which makes the noise loop seamlessly.
fn lattice(seed: u32, i: i64, period: Option<i64>) -> f64 {
    let i = period.map_or(i, |p| i.rem_euclid(p));
    f64::from(hash(seed, i)) / f64::from(u32::MAX) * 2.0 - 1.0
}

fn smooth(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// One octave: value noise at `x` (in lattice steps), from -1 to 1.
fn value(seed: u32, x: f64, period: Option<i64>) -> f64 {
    let cell = x.floor();
    let t = smooth(x - cell);
    let i = cell as i64;
    let (a, b) = (lattice(seed, i, period), lattice(seed, i + 1, period));
    a + (b - a) * t
}

/// How the noise is shaped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Noise {
    pub seed: u32,
    /// Lattice points per second: about how many times a second the value changes direction.
    pub frequency: f64,
    /// 1 to 4 layers, each twice as fast and half as strong: more detail, more jitter.
    pub octaves: u32,
    /// A seamless loop of this many seconds (the frequency is bent to fit a whole number of
    /// lattice points), or none.
    pub loop_seconds: Option<f64>,
}

pub const MAX_OCTAVES: u32 = 4;

impl Noise {
    /// The frequency in use: `frequency`, or the nearest that fits a whole number of lattice
    /// points into the loop.
    pub fn effective_frequency(&self) -> f64 {
        match self.loop_seconds {
            Some(s) if s > 0.0 => (self.frequency * s).round().max(2.0) / s,
            _ => self.frequency,
        }
    }

    /// The noise at `seconds`, from -1 to 1.
    pub fn at(&self, seconds: f64) -> f64 {
        let base = self.effective_frequency();
        let period = self
            .loop_seconds
            .filter(|s| *s > 0.0)
            .map(|s| (base * s).round() as i64);
        let (mut sum, mut strength, mut total) = (0.0, 1.0, 0.0);
        for octave in 0..self.octaves.clamp(1, MAX_OCTAVES) {
            let scale = f64::from(1u32 << octave);
            let octave_period = period.map(|p| p * (1i64 << octave));
            sum += strength
                * value(
                    self.seed.wrapping_add(octave.wrapping_mul(0x27D4_EB2F)),
                    seconds * base * scale,
                    octave_period,
                );
            total += strength;
            strength *= 0.5;
        }
        (sum / total).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: u32) -> Noise {
        Noise {
            seed,
            frequency: 2.0,
            octaves: 1,
            loop_seconds: None,
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_curve_and_another_seed_another_one() {
        let (a, b) = (noise(1), noise(1));
        let c = noise(2);
        let times: Vec<f64> = (0..40).map(|n| f64::from(n) * 0.137).collect();
        assert!(times.iter().all(|&t| a.at(t) == b.at(t)));
        assert!(times.iter().any(|&t| (a.at(t) - c.at(t)).abs() > 0.1));
    }

    #[test]
    fn values_stay_in_range_and_use_most_of_it() {
        let n = Noise {
            octaves: 3,
            ..noise(7)
        };
        let values: Vec<f64> = (0..2000).map(|k| n.at(f64::from(k) * 0.01)).collect();
        assert!(values.iter().all(|v| (-1.0..=1.0).contains(v)));
        let (min, max) = values
            .iter()
            .fold((1.0f64, -1.0f64), |(lo, hi), &v| (lo.min(v), hi.max(v)));
        assert!(max - min > 1.0, "range {min}..{max}");
    }

    #[test]
    fn the_curve_is_smooth_with_no_jump_between_close_times() {
        let n = noise(3);
        let worst = (0..4000)
            .map(|k| (n.at(f64::from(k + 1) * 0.001) - n.at(f64::from(k) * 0.001)).abs())
            .fold(0.0, f64::max);
        assert!(worst < 0.02, "largest step {worst}");
    }

    #[test]
    fn a_loop_ends_exactly_where_it_began_for_any_octave_count() {
        for octaves in 1..=MAX_OCTAVES {
            let n = Noise {
                seed: 11,
                frequency: 1.7,
                octaves,
                loop_seconds: Some(3.0),
            };
            assert!((n.at(0.0) - n.at(3.0)).abs() < 1e-9, "octaves {octaves}");
            assert!((n.at(0.4) - n.at(3.4)).abs() < 1e-9, "octaves {octaves}");
        }
        let n = Noise {
            loop_seconds: Some(3.0),
            frequency: 1.7,
            ..noise(11)
        };
        assert!((n.effective_frequency() * 3.0 - 5.0).abs() < 1e-9);
    }
}
