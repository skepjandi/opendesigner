//! Shape morphing between path nodes (`docs/wiki/video-editing.md#path-morph`): both shapes are
//! cut into the same number of points by arc length, so a morph is a straight blend of two point
//! lists. A path node holds the lists (`Node::morph`); the `morph_progress` animation moves
//! along them, and `Document::at_time` turns the blend into path data for the renderer, so
//! nothing downstream of it knows a shape is morphing.

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::{MAX_COORD, MAX_SIZE, finite_in, round4};
use crate::path::{Path, Point, Seg};
use crate::vector::ViewBox;

/// Points per document unit of outline, as anime.js `morphTo` uses.
pub const DENSITY: f64 = 0.33;
pub const MIN_POINTS: usize = 16;
pub const MAX_POINTS: usize = 256;
/// Shapes a node can pass through, the first included.
pub const MAX_STOPS: usize = 8;
pub const MAX_CONTOURS: usize = 16;

/// What a path node morphs through. Every stop holds the same contours, each with the same
/// number of points in every stop, in the node's own box coordinates (the box the node had when
/// the morph was made: it scales with the node).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathMorph {
    /// The box the points are in.
    pub width: f64,
    pub height: f64,
    /// Per contour: whether it is a closed ring.
    pub closed: Vec<bool>,
    /// `stops[s][c]` is contour `c` of stop `s`.
    pub stops: Vec<Vec<Vec<Point>>>,
}

fn bad(detail: impl Into<String>) -> OpError {
    OpError::OutOfBounds {
        what: "path_morph",
        detail: detail.into(),
    }
}

impl PathMorph {
    /// The last value of `morph_progress`: one per stop after the first.
    pub fn max_progress(&self) -> f64 {
        self.stops.len().saturating_sub(1) as f64
    }

    /// The morph as the document stores it: shape checked, every number rounded.
    pub(crate) fn checked(self) -> Result<PathMorph, OpError> {
        if self.stops.len() < 2 || self.stops.len() > MAX_STOPS {
            return Err(bad(format!("needs 2 to {MAX_STOPS} stops")));
        }
        let contours = self.closed.len();
        if contours == 0 || contours > MAX_CONTOURS {
            return Err(bad(format!("needs 1 to {MAX_CONTOURS} contours")));
        }
        let width = finite_in("path_morph width", self.width, 0.0, MAX_SIZE)?;
        let height = finite_in("path_morph height", self.height, 0.0, MAX_SIZE)?;
        if width <= 0.0 || height <= 0.0 {
            return Err(bad("the box must have a size"));
        }
        let mut stops = Vec::with_capacity(self.stops.len());
        for stop in &self.stops {
            if stop.len() != contours {
                return Err(bad("every stop needs the same contours"));
            }
            let mut rounded = Vec::with_capacity(contours);
            for (c, points) in stop.iter().enumerate() {
                let need = if self.closed[c] { 3 } else { 2 };
                if points.len() < need || points.len() > MAX_POINTS {
                    return Err(bad(format!(
                        "a contour needs {need} to {MAX_POINTS} points"
                    )));
                }
                if points.len() != self.stops[0][c].len() {
                    return Err(bad("a contour needs the same points in every stop"));
                }
                rounded.push(
                    points
                        .iter()
                        .map(|p| {
                            Ok([
                                finite_in("path_morph point", p[0], -MAX_COORD, MAX_COORD)?,
                                finite_in("path_morph point", p[1], -MAX_COORD, MAX_COORD)?,
                            ])
                        })
                        .collect::<Result<Vec<Point>, OpError>>()?,
                );
            }
            stops.push(rounded);
        }
        Ok(PathMorph {
            width,
            height,
            closed: self.closed,
            stops,
        })
    }

    /// The contours at `progress` (0 is the first stop, 1 the second): the blend of the two
    /// stops it lies between, held to the first and last stop.
    pub fn shape_at(&self, progress: f64) -> Vec<Vec<Point>> {
        let p = if progress.is_finite() {
            progress.clamp(0.0, self.max_progress())
        } else {
            0.0
        };
        let lower = (p.floor() as usize).min(self.stops.len().saturating_sub(2));
        let f = p - lower as f64;
        self.stops[lower]
            .iter()
            .zip(&self.stops[lower + 1])
            .map(|(a, b)| {
                a.iter()
                    .zip(b)
                    .map(|(a, b)| [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f])
                    .collect()
            })
            .collect()
    }

    /// SVG path data of the shape at `progress`, in the morph's box.
    pub fn path_data_at(&self, progress: f64) -> String {
        let mut segs = Vec::new();
        for (c, points) in self.shape_at(progress).into_iter().enumerate() {
            for (i, p) in points.into_iter().enumerate() {
                segs.push(if i == 0 {
                    Seg::MoveTo(p)
                } else {
                    Seg::LineTo(p)
                });
            }
            if self.closed[c] {
                segs.push(Seg::Close);
            }
        }
        Path::from_segments(segs).to_data()
    }

    /// The view box that maps the morph's points onto the node's box unchanged.
    pub fn view_box(&self) -> ViewBox {
        ViewBox {
            x: 0.0,
            y: 0.0,
            width: self.width,
            height: self.height,
        }
    }
}

/// Refuses a change of the node's morph that would leave a `morph_progress` key past its last
/// stop (or on a node with no morph), which a saved project would then fail to load.
pub(crate) fn check_progress_keys(
    doc: &crate::model::Document,
    node: &crate::model::NodeId,
    morph: Option<&PathMorph>,
) -> Result<(), OpError> {
    let last = morph.map(PathMorph::max_progress);
    let stale = doc
        .videos
        .iter()
        .flat_map(|v| &v.animations)
        .filter(|a| &a.node == node && a.prop == crate::animation::AnimProp::MorphProgress)
        .any(|a| a.keys.iter().any(|k| last.is_none_or(|m| k.value > m)));
    if stale {
        return Err(bad(
            "a morph_progress animation on this node needs the stops it keys; remove or shorten \
             its keys first",
        ));
    }
    Ok(())
}

/// The length of an outline; a ring counts its closing edge.
pub fn outline_length(points: &[Point], closed: bool) -> f64 {
    let mut sum: f64 = points
        .windows(2)
        .map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]))
        .sum();
    if closed && let (Some(a), Some(b)) = (points.last(), points.first()) {
        sum += (b[0] - a[0]).hypot(b[1] - a[1]);
    }
    sum
}

/// How many points both outlines are cut into: [`DENSITY`] per unit of the longer one, from
/// [`MIN_POINTS`] to [`MAX_POINTS`].
pub fn point_count(length_a: f64, length_b: f64) -> usize {
    let wanted = (length_a.max(length_b) * DENSITY).ceil();
    (wanted as usize).clamp(MIN_POINTS, MAX_POINTS)
}

/// `n` points spaced evenly by arc length along the outline: for an open one the first and last
/// points are its ends; for a ring they are `n` of its length apart, the start not repeated.
pub fn resample(points: &[Point], closed: bool, n: usize) -> Vec<Point> {
    let Some(first) = points.first().copied() else {
        return Vec::new();
    };
    let mut line: Vec<Point> = points.to_vec();
    if closed {
        line.push(first);
    }
    let mut lengths = vec![0.0];
    for w in line.windows(2) {
        let last = lengths.last().copied().unwrap_or(0.0);
        lengths.push(last + (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]));
    }
    let total = lengths.last().copied().unwrap_or(0.0);
    let steps = if closed {
        n
    } else {
        n.saturating_sub(1).max(1)
    };
    (0..n)
        .map(|i| {
            if total <= 0.0 {
                return first;
            }
            let s = total * i as f64 / steps as f64;
            let at = lengths.partition_point(|l| *l < s).clamp(1, line.len() - 1);
            let (l0, l1) = (lengths[at - 1], lengths[at]);
            let u = if l1 > l0 { (s - l0) / (l1 - l0) } else { 0.0 };
            let (a, b) = (line[at - 1], line[at]);
            [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u]
        })
        .collect()
}

/// Twice the signed area of a ring: its sign is its winding.
pub fn signed_area(points: &[Point]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| {
            let (a, b) = (points[i], points[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum()
}

/// The shift `k` that makes `b[(i + k) % n]` closest to `a[i]` over all `i`: the smallest one
/// when several tie.
pub fn best_rotation(a: &[Point], b: &[Point]) -> usize {
    let n = a.len().min(b.len());
    let cost = |k: usize| -> f64 {
        (0..n)
            .map(|i| {
                let q = b[(i + k) % n];
                (a[i][0] - q[0]).powi(2) + (a[i][1] - q[1]).powi(2)
            })
            .sum()
    };
    let mut best = (0, cost(0));
    for k in 1..n {
        let c = cost(k);
        if c < best.1 - 1e-9 {
            best = (k, c);
        }
    }
    best.0
}

/// `b` turned to line up with the ring `a`: reversed when the two wind opposite ways (with
/// `fix_winding`), then started where it is nearest to `a`.
pub fn align_ring(a: &[Point], b: &[Point], fix_winding: bool) -> Vec<Point> {
    let mut b = b.to_vec();
    if fix_winding && signed_area(a) * signed_area(&b) < 0.0 {
        b.reverse();
    }
    let k = best_rotation(a, &b);
    (0..b.len()).map(|i| b[(i + k) % b.len()]).collect()
}

/// One pair of outlines cut into `n` points each and lined up.
#[derive(Debug, Clone, PartialEq)]
pub struct Matched {
    pub from: Vec<Point>,
    pub to: Vec<Point>,
    pub closed: bool,
}

/// Cuts two outlines into `n` points each. A ring morphs into a ring (windings agree and the
/// start is the nearest); a ring and an open line both become open lines, the ring's closing edge
/// included.
pub fn match_n(a: &[Point], closed_a: bool, b: &[Point], closed_b: bool, n: usize) -> Matched {
    let ring = closed_a && closed_b;
    let cut = |p: &[Point], closed: bool| {
        let as_ring = ring;
        let mut line = p.to_vec();
        if closed
            && !as_ring
            && let Some(first) = p.first()
        {
            line.push(*first);
        }
        resample(&line, as_ring, n)
    };
    let from = cut(a, closed_a);
    let mut to = cut(b, closed_b);
    if ring {
        to = align_ring(&from, &to, true);
    }
    Matched {
        from,
        to,
        closed: ring,
    }
}

/// [`match_n`] with the number of points the two lengths ask for.
pub fn match_pair(a: &[Point], closed_a: bool, b: &[Point], closed_b: bool) -> Matched {
    let n = point_count(outline_length(a, closed_a), outline_length(b, closed_b));
    match_n(a, closed_a, b, closed_b, n)
}

/// `n` copies of the middle of an outline: what a contour with no partner shrinks to or grows from.
pub fn collapsed(points: &[Point], n: usize) -> Vec<Point> {
    let count = points.len().max(1) as f64;
    let (sx, sy) = points
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
    vec![[sx / count, sy / count]; n]
}

/// The area an outline encloses, for pairing the big contours of two shapes with each other.
pub fn area(points: &[Point]) -> f64 {
    signed_area(points).abs()
}

/// A point list rounded as the document stores it.
pub fn rounded(points: &[Point]) -> Vec<Point> {
    points
        .iter()
        .map(|p| [round4(p[0]), round4(p[1])])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: Point, b: Point) -> Vec<Point> {
        vec![a, b]
    }

    fn near(a: Point, b: Point) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn open_lines_get_the_anime_point_count_and_blend_halfway() {
        let a = line([0.0, 0.0], [100.0, 0.0]);
        let b = line([0.0, 0.0], [0.0, 50.0]);
        assert_eq!(point_count(100.0, 50.0), 33);
        let m = match_pair(&a, false, &b, false);
        assert_eq!((m.from.len(), m.to.len()), (33, 33));
        assert!(near(m.from[16], [50.0, 0.0]) && near(m.to[16], [0.0, 25.0]));
        assert!(near(m.from[32], [100.0, 0.0]) && near(m.to[32], [0.0, 50.0]));
        let morph = PathMorph {
            width: 100.0,
            height: 50.0,
            closed: vec![false],
            stops: vec![vec![m.from], vec![m.to]],
        };
        let mid = morph.shape_at(0.5);
        assert!(near(mid[0][16], [25.0, 12.5]) && near(mid[0][32], [50.0, 25.0]));
    }

    #[test]
    fn short_outlines_still_get_sixteen_points_and_long_ones_stop_at_the_cap() {
        assert_eq!(point_count(10.0, 20.0), 16);
        assert_eq!(point_count(5000.0, 10.0), MAX_POINTS);
    }

    fn square() -> Vec<Point> {
        vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
    }

    fn square_from_far_corner() -> Vec<Point> {
        vec![[100.0, 100.0], [0.0, 100.0], [0.0, 0.0], [100.0, 0.0]]
    }

    #[test]
    fn a_ring_is_cut_evenly_without_repeating_its_start() {
        let ring = resample(&square(), true, 8);
        let expect: Vec<Point> = vec![
            [0.0, 0.0],
            [50.0, 0.0],
            [100.0, 0.0],
            [100.0, 50.0],
            [100.0, 100.0],
            [50.0, 100.0],
            [0.0, 100.0],
            [0.0, 50.0],
        ];
        assert_eq!(ring, expect);
    }

    #[test]
    fn the_start_of_a_ring_is_found_and_the_windings_agree() {
        let a = resample(&square(), true, 8);
        let b = resample(&square_from_far_corner(), true, 8);
        assert_eq!(best_rotation(&a, &b), 4);
        assert_eq!(align_ring(&a, &b, true), a);
        let mut opposite = resample(&square(), true, 8);
        opposite.reverse();
        let opposite = resample(&opposite, true, 8);
        assert_eq!(align_ring(&a, &opposite, true), a);
        let cost = |p: &[Point]| -> f64 {
            a.iter()
                .zip(p)
                .map(|(a, b)| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2))
                .sum()
        };
        assert!(cost(&align_ring(&a, &opposite, false)) > 0.0);
    }

    #[test]
    fn a_ring_and_a_line_both_become_open_lines() {
        let m = match_n(&square(), true, &line([0.0, 0.0], [10.0, 0.0]), false, 9);
        assert!(!m.closed);
        assert!(near(m.from[0], [0.0, 0.0]) && near(m.from[8], [0.0, 0.0]));
    }

    #[test]
    fn a_morph_is_checked_and_blends_through_three_stops() {
        let mk = |x: f64| vec![vec![[x, 0.0], [x + 10.0, 0.0]]];
        let morph = PathMorph {
            width: 10.0,
            height: 10.0,
            closed: vec![false],
            stops: vec![mk(0.0), mk(10.0), mk(40.0)],
        }
        .checked()
        .unwrap();
        assert_eq!(morph.max_progress(), 2.0);
        assert!(near(morph.shape_at(1.5)[0][0], [25.0, 0.0]));
        assert!(near(morph.shape_at(9.0)[0][0], [40.0, 0.0]));
        assert!(near(morph.shape_at(-1.0)[0][0], [0.0, 0.0]));
        let mut broken = morph.clone();
        broken.stops[1][0].pop();
        assert!(broken.checked().is_err());
        let mut lone = morph.clone();
        lone.stops.truncate(1);
        assert!(lone.checked().is_err());
        let mut far = morph;
        far.stops[0][0][0][0] = f64::NAN;
        assert!(far.checked().is_err());
    }

    #[test]
    fn the_data_of_a_shape_is_a_polyline_that_closes_a_ring() {
        let morph = PathMorph {
            width: 10.0,
            height: 10.0,
            closed: vec![true],
            stops: vec![
                vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0]]],
                vec![vec![[0.0, 0.0], [20.0, 0.0], [20.0, 20.0]]],
            ],
        };
        assert_eq!(morph.path_data_at(0.5), "M0 0 L15 0 L15 15 Z");
        assert!(Path::parse(&morph.path_data_at(0.25)).is_ok());
    }

    #[test]
    fn a_contour_with_no_partner_shrinks_to_its_middle() {
        let c = collapsed(&square(), 5);
        assert!(c.iter().all(|p| near(*p, [50.0, 50.0])) && c.len() == 5);
    }
}
