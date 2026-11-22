//! Stagger: how far along a row, a grid or a scatter of nodes each one is from where the wave
//! starts, as times (a delay for each) or as values (a scale that falls off from the middle). The
//! distribution can be eased, reversed, jittered with a seeded generator and measured between the
//! real node centres (`docs/wiki/video-editing.md#stagger`).

use od_core::Ease;

/// A seeded generator, bit for bit the one anime.js uses (Mulberry32), so a seed gives the same
/// jitter and the same shuffle every run and in every tool that shares it.
#[derive(Debug, Clone)]
pub struct Mulberry32(u32);

impl Mulberry32 {
    pub fn new(seed: u32) -> Mulberry32 {
        Mulberry32(seed)
    }

    /// A number from 0 up to, not including, 1.
    pub fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    /// A number from `min` to `max`, both included, with `decimals` decimal places.
    pub fn range(&mut self, min: f64, max: f64, decimals: i32) -> f64 {
        let m = 10f64.powi(decimals);
        ((self.next_f64() * (max - min + 1.0 / m) + min) * m).floor() / m
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        let mut m = items.len();
        while m > 0 {
            m -= 1;
            let i = self.range(0.0, m as f64, 0) as usize;
            items.swap(m, i);
        }
    }
}

/// How the distance between two cells or two nodes is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// Straight-line distance: a round wave.
    Euclid,
    /// Steps along rows and columns: a diamond-shaped wave.
    Manhattan,
    /// The larger of the two steps: a square wave.
    Chebyshev,
}

impl Metric {
    pub fn parse(s: &str) -> Option<Metric> {
        Some(match s {
            "euclid" => Metric::Euclid,
            "manhattan" => Metric::Manhattan,
            "chebyshev" => Metric::Chebyshev,
            _ => return None,
        })
    }

    fn distance(self, dx: f64, dy: f64) -> f64 {
        match self {
            Metric::Euclid => dx.hypot(dy),
            Metric::Manhattan => dx.abs() + dy.abs(),
            Metric::Chebyshev => dx.abs().max(dy.abs()),
        }
    }
}

/// Where a stagger starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Origin {
    First,
    Last,
    Center,
    /// The first and last go together, then inward (in a grid or a scatter, the outer ring first).
    Edges,
    /// Shuffled by the seed.
    Random,
    /// The node at this index.
    Index(usize),
    /// A point as fractions of the way across the grid or the nodes' bounding box (0 to 1).
    Point(f64, f64),
}

impl Origin {
    /// The origins that are a word; `Index` and `Point` are built from numbers.
    pub fn parse(s: &str) -> Option<Origin> {
        Some(match s {
            "first" => Origin::First,
            "last" => Origin::Last,
            "center" => Origin::Center,
            "edges" => Origin::Edges,
            "random" => Origin::Random,
            _ => return None,
        })
    }
}

/// The step of a stagger: a time between neighbours, or a range of values spread over the
/// distribution (the first end at the origin, the second at the farthest node).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    Each(f64),
    Range(f64, f64),
}

/// Which way a grid or a scatter is read: only the distance along `x` or `y` counts, signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

impl Axis {
    pub fn parse(s: &str) -> Option<Axis> {
        match s {
            "x" => Some(Axis::X),
            "y" => Some(Axis::Y),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaggerSpec {
    pub step: Step,
    pub from: Origin,
    /// Columns and rows, the nodes laid out row by row.
    pub grid: Option<(usize, usize)>,
    pub metric: Metric,
    pub axis: Option<Axis>,
    /// Shapes the distribution: `out_*` eases make the wave front-loaded.
    pub ease: Option<Ease>,
    pub reversed: bool,
    /// Added to every value.
    pub start: f64,
    /// Random change added to each value, from the first magnitude (at the origin) to the second
    /// (at the farthest node).
    pub jitter: Option<(f64, f64)>,
    pub seed: Option<u64>,
    /// The centre of each node, in order: distances are measured between them, in units of the
    /// smallest nonzero one.
    pub positions: Option<Vec<(f64, f64)>>,
}

impl Default for StaggerSpec {
    fn default() -> StaggerSpec {
        StaggerSpec {
            step: Step::Each(0.0),
            from: Origin::First,
            grid: None,
            metric: Metric::Euclid,
            axis: None,
            ease: None,
            reversed: false,
            start: 0.0,
            jitter: None,
            seed: None,
            positions: None,
        }
    }
}

/// The value of each of `n` nodes: `start + step * distance` (a range spreads its two ends over
/// the distances instead), with the distribution eased, reversed and jittered as asked. Negative
/// values can come out of `axis` reading or jitter; [`stagger_offsets_ms`] shifts those.
pub fn stagger_values(n: usize, spec: &StaggerSpec) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let mut rng = Mulberry32::new(spec.seed.unwrap_or(0) as u32);
    let samples: Vec<f64> = if spec.jitter.is_some() {
        (0..n).map(|_| rng.range(-1.0, 1.0, 4)).collect()
    } else {
        Vec::new()
    };
    let mut values = distances(n, spec, &mut rng);
    let least = values.iter().copied().fold(f64::INFINITY, f64::min);
    if least < 0.0 {
        values.iter_mut().for_each(|v| *v -= least);
    }
    let farthest = values.iter().copied().fold(0.0, f64::max);
    if farthest > 0.0 {
        for v in &mut values {
            if let Some(ease) = spec.ease {
                *v = ease.apply(*v / farthest) * farthest;
            }
            if spec.reversed {
                *v = (farthest - *v).abs();
            }
        }
    }
    let (base, spacing) = match spec.step {
        Step::Each(ms) => (spec.start, ms),
        Step::Range(a, b) if farthest > 0.0 => (a + spec.start, (b - a) / farthest),
        Step::Range(a, _) => (a + spec.start, 0.0),
    };
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let mut out = base + spacing * v;
            if let Some((first, last)) = spec.jitter {
                let progress = if farthest > 0.0 { v / farthest } else { 0.0 };
                out += samples[i] * (first + (last - first) * progress);
            }
            out
        })
        .collect()
}

/// Whole-millisecond delays: the values, shifted up by the most negative one so the earliest node
/// starts at the beginning of the stagger. A value stagger (`Range`) is not shifted: its values
/// are values, not times.
pub fn stagger_offsets_ms(n: usize, spec: &StaggerSpec) -> Vec<u64> {
    let values = stagger_values(n, spec);
    let shift = match spec.step {
        Step::Each(_) => values.iter().copied().fold(0.0, f64::min).abs(),
        Step::Range(..) => 0.0,
    };
    values
        .into_iter()
        .map(|v| (v + shift).round().max(0.0) as u64)
        .collect()
}

fn distances(n: usize, spec: &StaggerSpec, rng: &mut Mulberry32) -> Vec<f64> {
    if spec.from == Origin::Random {
        let mut order: Vec<f64> = (0..n).map(|i| i as f64).collect();
        rng.shuffle(&mut order);
        return order;
    }
    match (&spec.positions, spec.grid) {
        (Some(points), _) if points.len() == n => scattered(points, spec),
        (_, Some((columns, rows))) => gridded(n, columns.max(1), rows.max(1), spec),
        _ => in_a_row(n, spec),
    }
}

fn in_a_row(n: usize, spec: &StaggerSpec) -> Vec<f64> {
    let last = (n - 1) as f64;
    let from = match spec.from {
        Origin::Last => last,
        Origin::Center => last / 2.0,
        Origin::Index(k) => k.min(n - 1) as f64,
        Origin::Point(fx, _) => fx.clamp(0.0, 1.0) * last,
        _ => 0.0,
    };
    (0..n)
        .map(|i| {
            let i = i as f64;
            match (spec.from, spec.axis) {
                (Origin::Edges, _) => i.min(last - i),
                (_, Some(_)) => i - from,
                _ => (i - from).abs(),
            }
        })
        .collect()
}

fn gridded(n: usize, columns: usize, rows: usize, spec: &StaggerSpec) -> Vec<f64> {
    let cell = |i: usize| ((i % columns) as f64, (i / columns) as f64);
    let (last_col, last_row) = ((columns - 1) as f64, (rows - 1) as f64);
    let (fx, fy) = match spec.from {
        Origin::Last => (last_col, last_row),
        Origin::Center => (last_col / 2.0, last_row / 2.0),
        Origin::Index(k) => cell(k.min(n - 1)),
        Origin::Point(px, py) => (px.clamp(0.0, 1.0) * last_col, py.clamp(0.0, 1.0) * last_row),
        _ => (0.0, 0.0),
    };
    (0..n)
        .map(|i| {
            let (x, y) = cell(i);
            if spec.from == Origin::Edges {
                return x.min(last_col - x).min(y).min(last_row - y).max(0.0);
            }
            match spec.axis {
                Some(Axis::X) => x - fx,
                Some(Axis::Y) => y - fy,
                None => spec.metric.distance(x - fx, y - fy),
            }
        })
        .collect()
}

fn scattered(points: &[(f64, f64)], spec: &StaggerSpec) -> Vec<f64> {
    let n = points.len();
    let bounds = points.iter().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(x0, y0, x1, y1), p| (x0.min(p.0), y0.min(p.1), x1.max(p.0), y1.max(p.1)),
    );
    let (x0, y0, x1, y1) = bounds;
    let (ox, oy) = match spec.from {
        Origin::Last => points[n - 1],
        Origin::Center => ((x0 + x1) / 2.0, (y0 + y1) / 2.0),
        Origin::Index(k) => points[k.min(n - 1)],
        Origin::Point(px, py) => (
            x0 + px.clamp(0.0, 1.0) * (x1 - x0),
            y0 + py.clamp(0.0, 1.0) * (y1 - y0),
        ),
        _ => points[0],
    };
    let mut values: Vec<f64> = points
        .iter()
        .map(|&(x, y)| match (spec.from, spec.axis) {
            (Origin::Edges, _) => (x - x0).min(x1 - x).min(y - y0).min(y1 - y),
            (_, Some(Axis::X)) => x - ox,
            (_, Some(Axis::Y)) => y - oy,
            _ => spec.metric.distance(x - ox, y - oy),
        })
        .collect();
    let nearest = values
        .iter()
        .map(|v| v.abs())
        .filter(|v| *v > 1e-9)
        .fold(f64::INFINITY, f64::min);
    if nearest.is_finite() {
        values.iter_mut().for_each(|v| *v /= nearest);
    }
    values
}
