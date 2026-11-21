//! Staggering over a grid: the delay of a node is its distance from a start cell.

use super::{Origin, Rng};

/// How the distance between two cells is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// Steps along rows and columns: a diamond-shaped wave.
    Manhattan,
    /// Straight-line distance: a round wave.
    Euclid,
}

impl Metric {
    pub fn parse(s: &str) -> Option<Metric> {
        Some(match s {
            "manhattan" => Metric::Manhattan,
            "euclid" => Metric::Euclid,
            _ => return None,
        })
    }

    fn distance(self, dx: f64, dy: f64) -> f64 {
        match self {
            Metric::Manhattan => dx.abs() + dy.abs(),
            Metric::Euclid => dx.hypot(dy),
        }
    }
}

/// The delay of each of `count` nodes laid out row by row in `columns` columns. With `start`
/// (an index into the nodes) the wave spreads from that cell; otherwise from the cell `from`
/// names: `first` is the top-left, `last` the bottom-right, `center` the middle, `edges` the
/// outer ring (a cell's distance to the nearest border) and `random` is shuffled by `seed`.
/// One unit of distance is `step_ms`.
pub fn grid_delays(
    count: usize,
    columns: usize,
    step_ms: u64,
    from: Origin,
    start: Option<usize>,
    metric: Metric,
    seed: u64,
) -> Vec<u64> {
    let columns = columns.clamp(1, count.max(1));
    let rows = count.div_ceil(columns);
    let cell = |i: usize| ((i % columns) as f64, (i / columns) as f64);
    let (last_col, last_row) = ((columns - 1) as f64, (rows - 1) as f64);
    let ranks: Vec<f64> = match (start, from) {
        (Some(s), _) => {
            let (sx, sy) = cell(s);
            (0..count)
                .map(|i| metric.distance(cell(i).0 - sx, cell(i).1 - sy))
                .collect()
        }
        (None, Origin::First) => (0..count)
            .map(|i| metric.distance(cell(i).0, cell(i).1))
            .collect(),
        (None, Origin::Last) => (0..count)
            .map(|i| metric.distance(last_col - cell(i).0, last_row - cell(i).1))
            .collect(),
        (None, Origin::Center) => (0..count)
            .map(|i| metric.distance(cell(i).0 - last_col / 2.0, cell(i).1 - last_row / 2.0))
            .collect(),
        (None, Origin::Edges) => (0..count)
            .map(|i| {
                let (x, y) = cell(i);
                x.min(last_col - x).min(y).min(last_row - y)
            })
            .collect(),
        (None, Origin::Random) => {
            let mut order: Vec<f64> = (0..count).map(|i| i as f64).collect();
            Rng::new(seed).shuffle(&mut order);
            order
        }
    };
    ranks
        .into_iter()
        .map(|r| (r * step_ms as f64).round() as u64)
        .collect()
}
