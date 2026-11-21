//! Geometry for painted and flowing looks, as pure functions of a seed: a smooth vector field to
//! follow, strokes that follow it, tapered ribbons around a stroke, and the repeated midpoint
//! deformation that gives a watercolor patch its irregular edge.

use std::f64::consts::PI;

use super::sketchgeo::P;
use crate::scene_motion::Rng;

/// Smooth two-dimensional noise from random heights on a lattice, in -1 to 1.
struct Noise2 {
    cols: usize,
    heights: Vec<f64>,
}

impl Noise2 {
    fn new(rng: &mut Rng, cols: usize, rows: usize) -> Noise2 {
        let (cols, rows) = (cols.max(2), rows.max(2));
        Noise2 {
            cols,
            heights: (0..cols * rows).map(|_| rng.range(-1.0, 1.0)).collect(),
        }
    }

    fn at(&self, x: f64, y: f64) -> f64 {
        let rows = (self.heights.len() / self.cols) as i64;
        let (fx, fy) = (x - x.floor(), y - y.floor());
        let (ix, iy) = (x.floor() as i64, y.floor() as i64);
        let h = |dx: i64, dy: i64| {
            let (c, r) = (
                (ix + dx).rem_euclid(self.cols as i64),
                (iy + dy).rem_euclid(rows),
            );
            self.heights[(r * self.cols as i64 + c) as usize]
        };
        let ease = |t: f64| t * t * (3.0 - 2.0 * t);
        let (tx, ty) = (ease(fx), ease(fy));
        let top = h(0, 0) + (h(1, 0) - h(0, 0)) * tx;
        let bottom = h(0, 1) + (h(1, 1) - h(0, 1)) * tx;
        top + (bottom - top) * ty
    }
}

/// A direction at every point of the plane: a base heading bent by two octaves of noise. Strokes
/// that follow it curve together the way grass, hair and wind do.
pub struct Field {
    low: Noise2,
    high: Noise2,
    base: f64,
    swirl: f64,
    cell: f64,
}

impl Field {
    /// `base_deg` is the heading where the noise is zero, `swirl` how far (in radians) the noise
    /// bends it, `cell` the distance over which the field changes.
    pub fn new(rng: &mut Rng, base_deg: f64, swirl: f64, cell: f64) -> Field {
        Field {
            low: Noise2::new(rng, 24, 24),
            high: Noise2::new(rng, 24, 24),
            base: base_deg.to_radians(),
            swirl,
            cell: cell.max(1.0),
        }
    }

    pub fn heading(&self, p: P) -> f64 {
        let (x, y) = (p.0 / self.cell, p.1 / self.cell);
        self.base
            + self.swirl * (self.low.at(x, y) + 0.4 * self.high.at(x * 2.7 + 9.0, y * 2.7 + 4.0))
    }
}

/// A stroke from `start`, `steps` steps of `step` long, following the field turned by `tilt`
/// radians and curling by `curl` more over its length. It stops early where `keep` says the
/// point is out of bounds.
pub fn follow(
    field: &Field,
    start: P,
    steps: usize,
    (step, tilt, curl): (f64, f64, f64),
    keep: impl Fn(P) -> bool,
) -> Vec<P> {
    let mut line = vec![start];
    let mut at = start;
    for i in 0..steps {
        let t = (i + 1) as f64 / steps as f64;
        let a = field.heading(at) + tilt + curl * t * t;
        at = (at.0 + a.cos() * step, at.1 + a.sin() * step);
        if !keep(at) {
            break;
        }
        line.push(at);
    }
    line
}

/// How a ribbon's width runs from its start (0) to its end (1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Taper {
    /// Full width at the root, a point at the tip: a blade of grass.
    Blade,
    /// A point at both ends and full width in the middle: a wind line or a water streak.
    Lens,
    /// Narrow almost everywhere: a hair.
    Hair,
}

impl Taper {
    fn width_at(self, t: f64) -> f64 {
        match self {
            Taper::Blade => (1.0 - t).powf(0.8) * (t * 10.0 + 0.55).min(1.0),
            Taper::Lens => (PI * t).sin().powf(0.75),
            Taper::Hair => (1.0 - t).powf(1.5) * (t * 12.0 + 0.4).min(1.0),
        }
    }
}

/// The closed outline of a tapered ribbon around `line`: down one side and back up the other.
pub fn ribbon(line: &[P], width: f64, taper: Taper) -> Vec<P> {
    let n = line.len();
    if n < 2 {
        return Vec::new();
    }
    let normal = |i: usize| {
        let (a, b) = (line[i.saturating_sub(1)], line[(i + 1).min(n - 1)]);
        let len = (b.0 - a.0).hypot(b.1 - a.1).max(1e-9);
        (-(b.1 - a.1) / len, (b.0 - a.0) / len)
    };
    let side = |sign: f64, i: usize| {
        let (nx, ny) = normal(i);
        let w = width / 2.0 * taper.width_at(i as f64 / (n - 1) as f64) * sign;
        (line[i].0 + nx * w, line[i].1 + ny * w)
    };
    let mut out: Vec<P> = (0..n).map(|i| side(1.0, i)).collect();
    out.extend((0..n - 1).rev().map(|i| side(-1.0, i)));
    out
}

fn gauss(rng: &mut Rng) -> f64 {
    (rng.unit() + rng.unit() + rng.unit() + rng.unit() - 2.0) * 1.732
}

/// Splits every edge of the closed polygon at its middle and pushes the new point off by a
/// random amount of the edge's length, `depth` times over. Each round roughens the edge at a
/// finer grain and with less force (`spread` shrinks by 0.62 a round), so the shape stays
/// itself but is no longer regular.
pub fn deform(poly: &[P], depth: usize, spread: f64, rng: &mut Rng) -> Vec<P> {
    let mut points = poly.to_vec();
    let mut spread = spread;
    for _ in 0..depth {
        let n = points.len();
        let mut next = Vec::with_capacity(n * 2);
        for i in 0..n {
            let (a, b) = (points[i], points[(i + 1) % n]);
            let len = (b.0 - a.0).hypot(b.1 - a.1);
            let dir = ((b.0 - a.0) / len.max(1e-9), (b.1 - a.1) / len.max(1e-9));
            let (side, along) = (gauss(rng) * spread * len, gauss(rng) * spread * len * 0.3);
            next.push(a);
            next.push((
                (a.0 + b.0) / 2.0 - dir.1 * side + dir.0 * along,
                (a.1 + b.1) / 2.0 + dir.0 * side + dir.1 * along,
            ));
        }
        points = next;
        spread *= 0.62;
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::sketchgeo::stream;

    #[test]
    fn a_field_is_a_function_of_its_seed() {
        let make = |seed| Field::new(&mut stream(seed, 1), -90.0, 1.0, 120.0);
        let (a, b, c) = (make(1), make(1), make(2));
        let at = [(10.0, 20.0), (300.0, 80.0), (999.0, 640.0)];
        assert!(at.iter().all(|p| a.heading(*p) == b.heading(*p)));
        assert!(at.iter().any(|p| a.heading(*p) != c.heading(*p)));
    }

    #[test]
    fn strokes_follow_the_heading_and_stop_at_the_bounds() {
        let flat = Field::new(&mut stream(3, 1), 0.0, 0.0, 100.0);
        let line = follow(&flat, (0.0, 50.0), 20, (10.0, 0.0, 0.0), |_| true);
        assert_eq!(line.len(), 21);
        assert!((line[20].0 - 200.0).abs() < 1e-6 && (line[20].1 - 50.0).abs() < 1e-6);
        let cut = follow(&flat, (0.0, 50.0), 20, (10.0, 0.0, 0.0), |p| p.0 < 55.0);
        assert_eq!(cut.len(), 6);
        let curled = follow(&flat, (0.0, 50.0), 20, (10.0, 0.0, 1.5), |_| true);
        assert!(curled[20].1 > 55.0, "a curl turns the stroke");
    }

    #[test]
    fn a_blade_is_widest_at_the_root_and_a_lens_in_the_middle() {
        let line: Vec<P> = (0..11).map(|i| (f64::from(i) * 10.0, 0.0)).collect();
        let span = |poly: &[P], x: f64| {
            let ys: Vec<f64> = poly
                .iter()
                .filter(|p| (p.0 - x).abs() < 0.1)
                .map(|p| p.1)
                .collect();
            ys.iter().copied().fold(f64::MIN, f64::max)
                - ys.iter().copied().fold(f64::MAX, f64::min)
        };
        let blade = ribbon(&line, 8.0, Taper::Blade);
        assert!(span(&blade, 0.0) > span(&blade, 60.0) && span(&blade, 60.0) > span(&blade, 90.0));
        let lens = ribbon(&line, 8.0, Taper::Lens);
        assert!(span(&lens, 50.0) > span(&lens, 10.0) && span(&lens, 50.0) > span(&lens, 90.0));
        assert!(ribbon(&line[..1], 8.0, Taper::Blade).is_empty());
    }

    #[test]
    fn deformation_doubles_the_points_and_is_seeded() {
        let square = [(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
        let a = deform(&square, 3, 0.1, &mut stream(1, 1));
        assert_eq!(a.len(), 32);
        assert_eq!(a, deform(&square, 3, 0.1, &mut stream(1, 1)));
        assert_ne!(a, deform(&square, 3, 0.1, &mut stream(2, 1)));
        assert!(
            a.iter()
                .all(|p| p.0 > -60.0 && p.0 < 160.0 && p.1 > -60.0 && p.1 < 160.0)
        );
    }
}
