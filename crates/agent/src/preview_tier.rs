//! How sharp and how smooth a preview is drawn, chosen from how fast this machine draws.
//!
//! A preview is only worth having if the pictures arrive about as fast as they are played and fit
//! in memory. A tier (the longest side and the rate) is affordable when the pictures it needs
//! take less time to draw than a good share of the video takes to play, and their raw size fits
//! the memory cap. The best affordable tier wins. The choice is pure: the numbers come in as
//! arguments, and [`steady`] keeps the speed estimate from jittering so that an edit does not
//! change the tier and throw the kept pictures away.

/// The longest sides tried, smallest first. The first is the floor every video gets.
pub const SIDES: [u32; 4] = [360, 540, 720, 960];
/// The floor of the rate; a video that is faster also tries its own rate, up to the ceiling.
pub const BASE_FPS: u32 = 15;
pub const MAX_FPS: u32 = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tier {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
}

impl Tier {
    pub fn megapixels(&self) -> f64 {
        f64::from(self.width) * f64::from(self.height) / 1_000_000.0
    }

    /// Bytes of one raw RGBA picture.
    pub fn raw_bytes(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * 4
    }
}

/// The tiers a video of `width` x `height` at `fps` can be previewed in, worst first. A tier is
/// never bigger or faster than the video itself.
pub fn candidates(width: u32, height: u32, fps: u32, max_side: u32, max_fps: u32) -> Vec<Tier> {
    let longest = f64::from(width.max(height).max(1));
    let fits = |side: u32| {
        let shrink = (f64::from(side) / longest).min(1.0);
        (
            ((f64::from(width) * shrink).round() as u32).max(1),
            ((f64::from(height) * shrink).round() as u32).max(1),
        )
    };
    let fastest = fps.min(max_fps.max(1));
    let mut rates = vec![fps.clamp(1, BASE_FPS).min(fastest)];
    if fastest > BASE_FPS {
        rates.push(fastest);
    }
    let mut out: Vec<Tier> = Vec::new();
    for side in SIDES
        .into_iter()
        .filter(|&s| s == SIDES[0] || s <= max_side)
    {
        let (w, h) = fits(side);
        for &fps in &rates {
            let tier = Tier {
                width: w,
                height: h,
                fps,
            };
            if !out.contains(&tier) {
                out.push(tier);
            }
        }
    }
    out
}

/// What one candidate costs: how many different pictures it needs (held ones are drawn once).
#[derive(Debug, Clone, Copy)]
pub struct Cost {
    pub tier: Tier,
    pub unique_pictures: u64,
}

/// How a machine is doing: milliseconds to draw one megapixel of picture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Speed(pub f64);

/// The share of the video's own length the drawing may take: the pictures must come in faster
/// than they are played, with room to spare for the machine doing other things.
const DRAW_SHARE: f64 = 0.6;
/// A video this short may take at least this long to draw whatever its length.
const MIN_BUDGET_MS: f64 = 1500.0;

/// The tier to draw. `costs` is worst first, as [`candidates`] returns them; `preferred` is the
/// tier of the preview this one replaces, which wins while it stays affordable: the picture size
/// must not change under an edit, or the kept pictures would be useless. The floor tier is the
/// answer when nothing is affordable or the speed is not known yet.
pub fn choose(
    costs: &[Cost],
    speed: Option<Speed>,
    duration_ms: u64,
    max_bytes: usize,
    preferred: Option<Tier>,
) -> Option<Tier> {
    let floor = costs.first()?.tier;
    let Some(Speed(ms_per_mpix)) = speed else {
        return Some(floor);
    };
    let budget = (duration_ms as f64 * DRAW_SHARE).max(MIN_BUDGET_MS);
    let affordable = |c: &Cost| {
        let draw_ms = c.unique_pictures as f64 * ms_per_mpix * c.tier.megapixels();
        let bytes = c.unique_pictures.saturating_mul(c.tier.raw_bytes());
        draw_ms <= budget && bytes <= max_bytes as u64
    };
    if let Some(kept) = costs
        .iter()
        .find(|c| Some(c.tier) == preferred)
        .filter(|c| affordable(c))
    {
        return Some(kept.tier);
    }
    Some(
        costs
            .iter()
            .rev()
            .find(|c| affordable(c))
            .map_or(floor, |c| c.tier),
    )
}

/// The speed to plan with: the earlier estimate unless the new measurement differs from it by
/// more than a quarter, so noise does not move the tier.
pub fn steady(earlier: Option<Speed>, measured: Speed) -> Speed {
    match earlier {
        Some(Speed(old)) if (measured.0 - old).abs() <= old * 0.25 => Speed(old),
        _ => measured,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn costs(unique: impl Fn(Tier) -> u64) -> Vec<Cost> {
        candidates(1080, 1920, 30, 960, MAX_FPS)
            .into_iter()
            .map(|tier| Cost {
                tier,
                unique_pictures: unique(tier),
            })
            .collect()
    }

    #[test]
    fn candidates_grow_but_never_past_the_video() {
        let c = candidates(1080, 1920, 30, 960, MAX_FPS);
        assert_eq!(
            c.first().unwrap(),
            &Tier {
                width: 203,
                height: 360,
                fps: 15
            }
        );
        assert_eq!(
            c.last().unwrap(),
            &Tier {
                width: 540,
                height: 960,
                fps: 30
            }
        );
        assert_eq!(c.len(), 8);
        assert!(
            candidates(1080, 1920, 30, 360, 15).len() == 1,
            "the ceiling holds"
        );
        let small = candidates(100, 200, 10, 960, MAX_FPS);
        assert_eq!(
            small,
            vec![Tier {
                width: 100,
                height: 200,
                fps: 10
            }]
        );
        assert!(
            candidates(1920, 1080, 120, 960, MAX_FPS)
                .iter()
                .all(|t| t.fps <= MAX_FPS)
        );
    }

    #[test]
    fn the_tier_of_the_preview_before_wins_while_it_is_affordable() {
        let c = costs(|t| u64::from(t.fps) * 3);
        let kept = c[2].tier;
        assert_eq!(
            choose(&c, Some(Speed(1.0)), 3000, 256 << 20, Some(kept)),
            Some(kept)
        );
        // a machine that got much slower cannot keep it
        assert_ne!(
            choose(&c, Some(Speed(1e6)), 3000, 256 << 20, Some(kept)),
            Some(kept)
        );
    }

    #[test]
    fn an_unknown_speed_takes_the_floor_and_a_fast_machine_the_ceiling() {
        let c = costs(|t| u64::from(t.fps) * 3);
        assert_eq!(choose(&c, None, 3000, 256 << 20, None), Some(c[0].tier));
        let fast = choose(&c, Some(Speed(1.0)), 3000, 256 << 20, None);
        assert_eq!(fast, Some(c.last().unwrap().tier));
    }

    #[test]
    fn a_slow_machine_or_a_small_cap_pulls_the_tier_down() {
        let c = costs(|t| u64::from(t.fps) * 10);
        let slow = choose(&c, Some(Speed(400.0)), 10_000, 256 << 20, None).unwrap();
        let quick = choose(&c, Some(Speed(20.0)), 10_000, 256 << 20, None).unwrap();
        assert!(slow.megapixels() < quick.megapixels(), "{slow:?} {quick:?}");
        let capped = choose(&c, Some(Speed(1.0)), 10_000, 20 << 20, None).unwrap();
        assert!(capped.megapixels() < c.last().unwrap().tier.megapixels());
        assert_eq!(
            choose(&c, Some(Speed(1e9)), 10_000, 256 << 20, None),
            Some(c[0].tier),
            "nothing affordable falls back to the floor"
        );
    }

    #[test]
    fn a_speed_estimate_moves_only_when_the_measurement_does() {
        assert_eq!(steady(None, Speed(10.0)), Speed(10.0));
        assert_eq!(steady(Some(Speed(10.0)), Speed(11.5)), Speed(10.0));
        assert_eq!(steady(Some(Speed(10.0)), Speed(20.0)), Speed(20.0));
        assert_eq!(choose(&[], Some(Speed(1.0)), 1000, 1, None), None);
    }
}
