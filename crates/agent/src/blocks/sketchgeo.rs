//! The geometry of a hand-drawn look, with no document and no renderer in sight: outlines that
//! wobble, bow and overshoot like a pen that is not following a ruler, and fills made of lines
//! (hachure, cross-hatch, zigzag) or dots, clipped to a shape by a scan-line walk. Everything is a
//! function of a seed, so the same seed gives the same drawing and another seed gives another.

use std::f64::consts::{PI, TAU};

use od_core::path::Path;

use super::scene::{circle, num, polyline, smooth};
use crate::scene_motion::Rng;

pub type P = (f64, f64);

/// How finely curves are cut when a path is turned into rings.
const FLATTEN_STEP: f64 = 4.0;
/// The turn, in degrees, from where a ring counts as having a corner.
pub const CORNER_DEG: f64 = 38.0;
const MAX_FILL_LINES: f64 = 110.0;
const MAX_DOTS: f64 = 260.0;

/// An outline: a closed loop or an open line, as points in frame coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Ring {
    pub pts: Vec<P>,
    pub closed: bool,
}

/// A generator keyed by a seed and a salt: one stream per pass, variant and stroke, so changing
/// one thing never shifts the numbers another thing draws.
pub fn stream(seed: u64, salt: u64) -> Rng {
    Rng::new(
        seed.wrapping_mul(0x2545_F491_4F6C_DD1D)
            .wrapping_add(salt.wrapping_mul(0x9E37_79B9)),
    )
}

fn dist(a: P, b: P) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn lerp(a: P, b: P, t: f64) -> P {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

// ---------------------------------------------------------------- shapes

pub fn rect_ring(x: f64, y: f64, w: f64, h: f64, radius: f64) -> Ring {
    let r = radius.min(w / 2.0).min(h / 2.0);
    if r < 0.5 {
        return Ring {
            pts: vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
            closed: true,
        };
    }
    let mut pts = Vec::new();
    let corners = [
        (x + w - r, y + r, -0.5),
        (x + w - r, y + h - r, 0.0),
        (x + r, y + h - r, 0.5),
        (x + r, y + r, 1.0),
    ];
    for (cx, cy, quarter) in corners {
        for i in 0..=4 {
            let a = (quarter + f64::from(i) / 8.0) * PI;
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    Ring { pts, closed: true }
}

pub fn ellipse_ring(cx: f64, cy: f64, rx: f64, ry: f64) -> Ring {
    let n = (TAU * (rx + ry) / 2.0 / 6.0).clamp(24.0, 96.0) as usize;
    let pts = (0..n)
        .map(|i| {
            let a = TAU * i as f64 / n as f64;
            (cx + rx * a.cos(), cy + ry * a.sin())
        })
        .collect();
    Ring { pts, closed: true }
}

fn tidy(points: &[P], closed: bool) -> Option<Ring> {
    let mut pts: Vec<P> = Vec::with_capacity(points.len());
    for &p in points {
        if pts.last().is_none_or(|&q| dist(p, q) > 1e-6) {
            pts.push(p);
        }
    }
    if closed
        && pts.len() > 1
        && pts
            .first()
            .zip(pts.last())
            .is_some_and(|(a, b)| dist(*a, *b) < 1e-6)
    {
        pts.pop();
    }
    let enough = if closed { 3 } else { 2 };
    (pts.len() >= enough).then_some(Ring { pts, closed })
}

pub fn polygon_ring(points: &[P], closed: bool) -> Option<Ring> {
    tidy(points, closed)
}

/// The rings of SVG path data (in frame coordinates), curves flattened.
pub fn path_rings(d: &str) -> Result<Vec<Ring>, String> {
    let path = Path::parse(d).map_err(|e| e.to_string())?;
    Ok(rings_of(&path))
}

pub fn rings_of(path: &Path) -> Vec<Ring> {
    path.flatten(FLATTEN_STEP)
        .into_iter()
        .filter_map(|c| {
            let pts: Vec<P> = c.points.iter().map(|p| (p[0], p[1])).collect();
            tidy(&pts, c.closed)
        })
        .collect()
}

pub fn translated(rings: &[Ring], dx: f64, dy: f64) -> Vec<Ring> {
    rings
        .iter()
        .map(|r| Ring {
            pts: r.pts.iter().map(|p| (p.0 + dx, p.1 + dy)).collect(),
            closed: r.closed,
        })
        .collect()
}

/// The smallest and largest corners of the rings' points.
pub fn bounds(rings: &[Ring]) -> Option<(P, P)> {
    let mut all = rings.iter().flat_map(|r| r.pts.iter().copied());
    let first = all.next()?;
    Some(all.fold((first, first), |(lo, hi), p| {
        (
            (lo.0.min(p.0), lo.1.min(p.1)),
            (hi.0.max(p.0), hi.1.max(p.1)),
        )
    }))
}

/// Whether `p` is inside the rings, counting every ring as closed and holes by parity.
pub fn point_in(rings: &[Ring], p: P) -> bool {
    let mut inside = false;
    for r in rings {
        let n = r.pts.len();
        for i in 0..n {
            let (a, b) = (r.pts[i], r.pts[(i + 1) % n]);
            if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) / (b.1 - a.1) * (b.0 - a.0) {
                inside = !inside;
            }
        }
    }
    inside
}

// ---------------------------------------------------------------- noise

/// Smooth one-dimensional noise from random heights on a lattice, in -1 to 1.
struct Noise1 {
    heights: Vec<f64>,
}

impl Noise1 {
    fn new(rng: &mut Rng, cells: usize) -> Noise1 {
        Noise1 {
            heights: (0..cells.max(2)).map(|_| rng.range(-1.0, 1.0)).collect(),
        }
    }

    fn at(&self, x: f64) -> f64 {
        let n = self.heights.len() as i64;
        let (floor, f) = (x.floor(), x - x.floor());
        let at = |i: i64| self.heights[i.rem_euclid(n) as usize];
        let t = f * f * (3.0 - 2.0 * f);
        let i = floor as i64;
        at(i) + (at(i + 1) - at(i)) * t
    }
}

// ---------------------------------------------------------------- outlines

/// How a pen behaves: how far it strays, how much a line bows, and how thick it draws.
#[derive(Debug, Clone, Copy)]
pub struct Hand {
    pub roughness: f64,
    pub bowing: f64,
    pub unit: f64,
}

/// A stretch of an outline between two corners (or a smooth loop with none).
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub pts: Vec<P>,
    pub looped: bool,
}

fn turn_at(prev: P, here: P, next: P) -> f64 {
    let (a, b) = (
        (here.0 - prev.0, here.1 - prev.1),
        (next.0 - here.0, next.1 - here.1),
    );
    (a.0 * b.1 - a.1 * b.0).atan2(a.0 * b.0 + a.1 * b.1).abs()
}

fn corner_indices(ring: &Ring, corner_deg: f64) -> Vec<usize> {
    let (n, limit) = (ring.pts.len(), corner_deg.to_radians());
    (0..n)
        .filter(|&i| {
            if !ring.closed && (i == 0 || i + 1 == n) {
                return false;
            }
            let (prev, next) = (ring.pts[(i + n - 1) % n], ring.pts[(i + 1) % n]);
            turn_at(prev, ring.pts[i], next) > limit
        })
        .collect()
}

/// Splits an outline at its corners so each side is drawn as its own stroke, like a person
/// lifting the pen at a corner; an outline with no corner is one loop.
pub fn runs(ring: &Ring, corner_deg: f64) -> Vec<Run> {
    let corners = corner_indices(ring, corner_deg);
    let n = ring.pts.len();
    if corners.is_empty() {
        return vec![Run {
            pts: ring.pts.clone(),
            looped: ring.closed,
        }];
    }
    let side = |from: usize, to: usize| -> Run {
        let count = (to + n - from) % n + usize::from(from == to) * n;
        Run {
            pts: (0..=count).map(|k| ring.pts[(from + k) % n]).collect(),
            looped: false,
        }
    };
    let mut out = Vec::new();
    if ring.closed {
        for (j, &c) in corners.iter().enumerate() {
            out.push(side(c, corners[(j + 1) % corners.len()]));
        }
    } else {
        let mut marks = vec![0];
        marks.extend(&corners);
        marks.push(n - 1);
        for w in marks.windows(2) {
            out.push(Run {
                pts: ring.pts[w[0]..=w[1]].to_vec(),
                looped: false,
            });
        }
    }
    out.retain(|r| r.pts.len() >= 2);
    out
}

/// A polyline walked by distance, extending straight past its ends (or wrapping when looped).
struct Walk {
    pts: Vec<P>,
    cum: Vec<f64>,
    looped: bool,
}

impl Walk {
    fn new(points: &[P], looped: bool) -> Walk {
        let mut pts = points.to_vec();
        if let (true, Some(first)) = (looped, points.first()) {
            pts.push(*first);
        }
        let mut cum = vec![0.0];
        for w in pts.windows(2) {
            cum.push(cum.last().copied().unwrap_or(0.0) + dist(w[0], w[1]));
        }
        Walk { pts, cum, looped }
    }

    fn total(&self) -> f64 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    /// The point at distance `s` and the unit direction of travel there.
    fn at(&self, s: f64) -> (P, P) {
        let total = self.total();
        let s = if self.looped { s.rem_euclid(total) } else { s };
        let last = self.pts.len() - 2;
        let i = self
            .cum
            .partition_point(|c| *c <= s)
            .saturating_sub(1)
            .min(last);
        let (a, b) = (self.pts[i], self.pts[i + 1]);
        let len = (self.cum[i + 1] - self.cum[i]).max(1e-9);
        let dir = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let along = s - self.cum[i];
        ((a.0 + dir.0 * along, a.1 + dir.1 * along), dir)
    }
}

/// The wobbly points of one stroke along `run`: the pen strays by smooth noise, bows to one side,
/// misses its start and end a little, and overshoots them (a loop keeps going past where it began).
pub fn wobble(run: &Run, rng: &mut Rng, hand: Hand) -> Vec<P> {
    let walk = Walk::new(&run.pts, run.looped);
    let len = walk.total();
    if len < 1e-6 {
        return run.pts.clone();
    }
    let unit = hand.unit.max(0.5);
    let amp = hand.roughness * (unit * 0.45 + len * 0.0025).min(unit * 2.5);
    let care = hand.roughness.clamp(0.25, 1.5);
    let overshoot =
        |rng: &mut Rng| (unit + len * 0.018).min(unit * 7.0) * rng.range(0.25, 1.0) * care;
    let (before, after) = if run.looped {
        (0.0, len * rng.range(0.03, 0.08) * care)
    } else {
        (overshoot(rng), overshoot(rng))
    };
    let start = if run.looped {
        rng.range(0.0, len)
    } else {
        -before
    };
    let span =
        len * f64::from(u8::from(run.looped)) + after + if run.looped { 0.0 } else { len + before };
    let n = ((span / (unit * 6.0)).round() as usize).clamp(5, 60);
    let wave = (len * 0.5).clamp(unit * 8.0, unit * 60.0);
    let cells = |w: f64| (span / w).ceil() as usize + 4;
    let (slow, fast) = (
        Noise1::new(rng, cells(wave)),
        Noise1::new(rng, cells(wave * 0.37)),
    );
    let bow =
        hand.bowing * len * 0.012 * rng.range(0.5, 1.0) * if rng.chance(0.5) { 1.0 } else { -1.0 };
    let jitter = |rng: &mut Rng| {
        let (r, a) = (amp * 1.4 * rng.unit().sqrt(), rng.range(0.0, TAU));
        (r * a.cos(), r * a.sin())
    };
    let (j0, j1) = (jitter(rng), jitter(rng));
    let drift = if rng.chance(0.5) { 1.0 } else { -1.0 } * amp * 1.6;
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let s = start + span * t;
            let ((x, y), dir) = walk.at(s);
            let beyond = if run.looped {
                ((s - start - len) / after.max(1e-9)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let away = amp * (slow.at(s / wave) + 0.35 * fast.at(s / (wave * 0.37)))
                + if run.looped {
                    0.0
                } else {
                    bow * (PI * t).sin()
                }
                + drift * beyond;
            let (w0, w1) = ((1.0 - t).powi(2), t * t);
            (
                x - dir.1 * away + j0.0 * w0 + j1.0 * w1,
                y + dir.0 * away + j0.1 * w0 + j1.1 * w1,
            )
        })
        .collect()
}

/// `n` points spread evenly along the ring (closed rings wrap), for a shape that is deformed.
pub fn resample(ring: &Ring, n: usize) -> Vec<P> {
    let walk = Walk::new(&ring.pts, ring.closed);
    let len = walk.total();
    if len < 1e-6 || n < 2 {
        return ring.pts.clone();
    }
    let steps = if ring.closed { n } else { n - 1 };
    (0..n)
        .map(|i| walk.at(len * i as f64 / steps as f64).0)
        .collect()
}

/// A closed outline drawn loosely, for a painted fill: the ring's points, each nudged along the
/// normal by smooth noise, so the fill sits a little off the line like paint does.
pub fn loose_ring(ring: &Ring, rng: &mut Rng, hand: Hand) -> Vec<P> {
    let walk = Walk::new(&ring.pts, ring.closed);
    let len = walk.total();
    if len < 1e-6 {
        return ring.pts.clone();
    }
    let unit = hand.unit.max(0.5);
    let amp = hand.roughness * (unit * 0.8 + len * 0.004).min(unit * 3.5);
    let n = ((len / (unit * 3.0)).round() as usize).clamp(8, 90);
    let wave = (len / 3.0).clamp(unit * 8.0, unit * 70.0);
    let noise = Noise1::new(rng, (len / wave).ceil() as usize + 3);
    let off = rng.range(0.0, 20.0);
    (0..n)
        .map(|i| {
            let ((x, y), dir) = walk.at(len * i as f64 / n as f64);
            let away = amp * noise.at(len * i as f64 / n as f64 / wave + off);
            (x - dir.1 * away, y + dir.0 * away)
        })
        .collect()
}

/// One full pass of a pen over the rings: a wobbly polyline per side or loop.
pub fn outline_pass(rings: &[Ring], seed: u64, pass: u64, hand: Hand) -> Vec<Vec<P>> {
    let mut out = Vec::new();
    for (ri, ring) in rings.iter().enumerate() {
        for (ci, run) in runs(ring, CORNER_DEG).iter().enumerate() {
            let mut rng = stream(seed, pass * 100_003 + ri as u64 * 1_009 + ci as u64);
            out.push(wobble(run, &mut rng, hand));
        }
    }
    out
}

// ---------------------------------------------------------------- fills

fn rotate(p: P, a: f64) -> P {
    (p.0 * a.cos() - p.1 * a.sin(), p.0 * a.sin() + p.1 * a.cos())
}

fn turned(rings: &[Ring], a: f64) -> Vec<Ring> {
    rings
        .iter()
        .map(|r| Ring {
            pts: r.pts.iter().map(|p| rotate(*p, a)).collect(),
            closed: true,
        })
        .collect()
}

/// A spacing no tighter than `max_lines` lines across the shape at `angle_deg`.
pub fn fitted_gap(rings: &[Ring], angle_deg: f64, gap: f64) -> f64 {
    let flat = turned(rings, -angle_deg.to_radians());
    let height = bounds(&flat).map_or(0.0, |(lo, hi)| hi.1 - lo.1);
    gap.max(height / MAX_FILL_LINES).max(0.5)
}

/// Parallel lines at `angle_deg`, `gap` apart, clipped to the inside of the rings by a scan-line
/// walk: each scan line is cut where it crosses an edge and the stretches between an odd and the
/// next even crossing are inside, so concave shapes and holes come out right.
pub fn hachure(rings: &[Ring], angle_deg: f64, gap: f64) -> Vec<(P, P)> {
    let a = angle_deg.to_radians();
    let flat = turned(rings, -a);
    let Some((lo, hi)) = bounds(&flat) else {
        return Vec::new();
    };
    let gap = fitted_gap(rings, angle_deg, gap);
    let mut out = Vec::new();
    let mut y = lo.1 + gap * 0.5;
    while y < hi.1 {
        let mut xs = crossings(&flat, y);
        xs.sort_by(f64::total_cmp);
        for pair in xs.chunks_exact(2) {
            out.push((rotate((pair[0], y), a), rotate((pair[1], y), a)));
        }
        y += gap;
    }
    out
}

fn crossings(rings: &[Ring], y: f64) -> Vec<f64> {
    let mut xs = Vec::new();
    for r in rings {
        let n = r.pts.len();
        for i in 0..n {
            let (p, q) = (r.pts[i], r.pts[(i + 1) % n]);
            if (p.1 <= y) != (q.1 <= y) {
                xs.push(p.0 + (y - p.1) / (q.1 - p.1) * (q.0 - p.0));
            }
        }
    }
    xs
}

/// A hachure line drawn by hand: pulled in from the edge by up to `inset` at each end and bent
/// by up to `amp` in the middle, as the three points of a quadratic curve.
pub fn roughen_lines(lines: &[(P, P)], rng: &mut Rng, amp: f64, inset: f64) -> Vec<[P; 3]> {
    lines
        .iter()
        .filter_map(|&(a, b)| {
            let len = dist(a, b);
            if len < inset * 3.0 + 1.0 {
                return None;
            }
            let dir = ((b.0 - a.0) / len, (b.1 - a.1) / len);
            let (i0, i1) = (rng.range(inset * 0.4, inset), rng.range(inset * 0.4, inset));
            let (a, b) = (
                (a.0 + dir.0 * i0, a.1 + dir.1 * i0),
                (b.0 - dir.0 * i1, b.1 - dir.1 * i1),
            );
            let bend = amp * rng.range(-1.0, 1.0);
            let mid = lerp(a, b, 0.5);
            Some([a, (mid.0 - dir.1 * bend, mid.1 + dir.0 * bend), b])
        })
        .collect()
}

/// One continuous zigzag per connected stretch of the shape: hachure lines joined end to end,
/// alternately, wherever the next line starts close to where the last one ended.
pub fn zigzag(rings: &[Ring], angle_deg: f64, gap: f64) -> Vec<Vec<P>> {
    let gap = fitted_gap(rings, angle_deg, gap);
    let mut out: Vec<Vec<P>> = Vec::new();
    let mut flip = false;
    for (a, b) in hachure(rings, angle_deg, gap) {
        let (from, to) = if flip { (b, a) } else { (a, b) };
        let joined = out.last_mut().filter(|line| {
            line.last().is_some_and(|&end| {
                dist(end, from) <= gap * 2.2 && point_in(rings, lerp(end, from, 0.4))
            })
        });
        match joined {
            Some(line) => line.extend([from, to]),
            None => out.push(vec![from, to]),
        }
        flip = !flip;
    }
    out.retain(|l| l.len() >= 2);
    out
}

/// Dots on a staggered lattice, each nudged off its place, inside the rings.
pub fn dots(rings: &[Ring], gap: f64, rng: &mut Rng) -> Vec<P> {
    let Some((lo, hi)) = bounds(rings) else {
        return Vec::new();
    };
    let area = (hi.0 - lo.0) * (hi.1 - lo.1);
    let gap = gap.max((area / MAX_DOTS).sqrt()).max(1.0);
    let mut out = Vec::new();
    let mut y = lo.1 + gap * 0.5;
    let mut row = 0;
    while y < hi.1 {
        let mut x = lo.0 + gap * if row % 2 == 0 { 0.5 } else { 1.0 };
        while x < hi.0 {
            let p = (
                x + gap * rng.range(-0.22, 0.22),
                y + gap * rng.range(-0.22, 0.22),
            );
            if point_in(rings, p) {
                out.push(p);
            }
            x += gap;
        }
        y += gap * 0.87;
        row += 1;
    }
    out
}

// ---------------------------------------------------------------- path data

/// One piece of a path: what an ink stroke or fill is made of.
#[derive(Debug, Clone, PartialEq)]
pub enum Sub {
    Open(Vec<P>),
    Closed(Vec<P>),
    /// Straight segments with sharp corners.
    Poly(Vec<P>),
    Quad([P; 3]),
    Dot(P, f64),
}

impl Sub {
    fn points(&self) -> Vec<P> {
        match self {
            Sub::Open(p) | Sub::Closed(p) | Sub::Poly(p) => p.clone(),
            Sub::Quad(p) => p.to_vec(),
            Sub::Dot(c, r) => vec![(c.0 - r, c.1 - r), (c.0 + r, c.1 + r)],
        }
    }

    /// The piece with every point held inside the box from `lo` to `hi`.
    pub fn clamped(&self, lo: P, hi: P) -> Sub {
        let keep = |p: &P| (p.0.clamp(lo.0, hi.0), p.1.clamp(lo.1, hi.1));
        let all = |v: &[P]| v.iter().map(keep).collect::<Vec<_>>();
        match self {
            Sub::Open(p) => Sub::Open(all(p)),
            Sub::Closed(p) => Sub::Closed(all(p)),
            Sub::Poly(p) => Sub::Poly(all(p)),
            Sub::Quad([a, c, b]) => Sub::Quad([keep(a), keep(c), keep(b)]),
            Sub::Dot(c, r) => Sub::Dot(keep(c), *r),
        }
    }

    fn data(&self, o: P) -> String {
        let at = |p: P| (p.0 - o.0, p.1 - o.1);
        let shifted = |pts: &[P]| pts.iter().map(|p| at(*p)).collect::<Vec<_>>();
        match self {
            Sub::Open(p) if p.len() >= 2 => smooth(&shifted(p), false),
            Sub::Closed(p) if p.len() >= 3 => smooth(&shifted(p), true),
            Sub::Poly(p) if p.len() >= 2 => polyline(&shifted(p), false),
            Sub::Quad([a, c, b]) => {
                let (a, c, b) = (at(*a), at(*c), at(*b));
                format!(
                    "M{},{}Q{},{} {},{}",
                    num(a.0),
                    num(a.1),
                    num(c.0),
                    num(c.1),
                    num(b.0),
                    num(b.1)
                )
            }
            Sub::Dot(c, r) => {
                let c = at(*c);
                circle(c.0, c.1, *r)
            }
            _ => String::new(),
        }
    }
}

/// Path data for a node: the box that holds the pieces and the data in that box's own pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    pub d: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn compose(subs: &[Sub]) -> Option<Drawn> {
    let pts: Vec<P> = subs.iter().flat_map(Sub::points).collect();
    let min_x = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let max_x = pts.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max);
    let max_y = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    if !(min_x.is_finite() && min_y.is_finite() && max_x.is_finite() && max_y.is_finite()) {
        return None;
    }
    let origin = ((min_x * 10.0).floor() / 10.0, (min_y * 10.0).floor() / 10.0);
    let d: String = subs.iter().map(|s| s.data(origin)).collect();
    (!d.is_empty()).then(|| Drawn {
        d,
        x: origin.0,
        y: origin.1,
        width: (max_x - origin.0).max(1.0).ceil(),
        height: (max_y - origin.1).max(1.0).ceil(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HAND: Hand = Hand {
        roughness: 1.0,
        bowing: 1.0,
        unit: 4.0,
    };

    fn square() -> Ring {
        rect_ring(100.0, 100.0, 300.0, 200.0, 0.0)
    }

    fn comb() -> Ring {
        Ring {
            pts: vec![
                (0.0, 0.0),
                (300.0, 0.0),
                (300.0, 200.0),
                (200.0, 200.0),
                (200.0, 60.0),
                (100.0, 60.0),
                (100.0, 200.0),
                (0.0, 200.0),
            ],
            closed: true,
        }
    }

    fn middle_inside(rings: &[Ring], lines: &[(P, P)]) {
        assert!(!lines.is_empty());
        for &(a, b) in lines {
            for t in [0.25, 0.5, 0.75] {
                assert!(point_in(rings, lerp(a, b, t)), "{a:?} to {b:?} at {t}");
            }
        }
    }

    #[test]
    fn hachure_lines_lie_inside_a_convex_shape() {
        let rings = [ellipse_ring(200.0, 150.0, 120.0, 80.0)];
        for angle in [-41.0, 0.0, 90.0, 33.0] {
            middle_inside(&rings, &hachure(&rings, angle, 12.0));
        }
    }

    #[test]
    fn hachure_lines_lie_inside_a_concave_shape_and_skip_the_notch() {
        let rings = [comb()];
        let lines = hachure(&rings, 90.0, 10.0);
        middle_inside(&rings, &lines);
        assert!(
            lines
                .iter()
                .all(|(a, b)| !(a.0 > 101.0 && a.0 < 199.0 && b.1 - a.1 > 150.0))
        );
        let notch_lines = lines
            .iter()
            .filter(|(a, _)| a.0 > 101.0 && a.0 < 199.0)
            .count();
        assert!(
            notch_lines >= 8,
            "{notch_lines}: the bar under the notch is filled"
        );
    }

    #[test]
    fn hachure_respects_holes() {
        let rings = [
            rect_ring(0.0, 0.0, 300.0, 300.0, 0.0),
            rect_ring(100.0, 100.0, 100.0, 100.0, 0.0),
        ];
        let lines = hachure(&rings, 0.0, 10.0);
        middle_inside(&rings, &lines);
        for (a, b) in &lines {
            let mid = lerp(*a, *b, 0.5);
            assert!(!(mid.0 > 100.0 && mid.0 < 200.0 && mid.1 > 100.0 && mid.1 < 200.0));
        }
        assert!(lines.len() >= 40, "{} lines", lines.len());
    }

    #[test]
    fn hachure_spacing_is_the_gap_and_capped() {
        let rings = [square()];
        let lines = hachure(&rings, 0.0, 10.0);
        assert!((lines[1].0.1 - lines[0].0.1 - 10.0).abs() < 1e-6);
        let dense = hachure(&rings, 0.0, 0.001);
        assert!(dense.len() <= 120, "{}", dense.len());
    }

    #[test]
    fn zigzag_and_dots_stay_inside() {
        let rings = [comb()];
        for line in zigzag(&rings, 20.0, 12.0) {
            for w in line.windows(2) {
                assert!(point_in(&rings, lerp(w[0], w[1], 0.5)) || dist(w[0], w[1]) < 30.0);
            }
        }
        let mut rng = stream(1, 2);
        let found = dots(&rings, 14.0, &mut rng);
        assert!(found.len() > 50 && found.len() <= 300);
        assert!(found.iter().all(|p| point_in(&rings, *p)));
    }

    #[test]
    fn the_same_seed_gives_the_same_outline_and_another_seed_another() {
        let rings = [square()];
        let a = outline_pass(&rings, 5, 0, HAND);
        assert_eq!(a, outline_pass(&rings, 5, 0, HAND));
        assert_ne!(a, outline_pass(&rings, 6, 0, HAND));
        assert_ne!(a, outline_pass(&rings, 5, 1, HAND), "a second pass differs");
    }

    #[test]
    fn a_rectangle_is_four_strokes_that_overshoot_their_corners() {
        let rings = [square()];
        let sides = outline_pass(&rings, 9, 0, HAND);
        assert_eq!(sides.len(), 4);
        let (lo, hi) = bounds(&rings).unwrap_or(((0.0, 0.0), (0.0, 0.0)));
        let reach = sides
            .iter()
            .flatten()
            .any(|p| p.0 < lo.0 - 0.5 || p.0 > hi.0 + 0.5 || p.1 < lo.1 - 0.5 || p.1 > hi.1 + 0.5);
        assert!(reach, "strokes overshoot");
        let stay = sides.iter().flatten().all(|p| {
            p.0 > lo.0 - 40.0 && p.0 < hi.0 + 40.0 && p.1 > lo.1 - 40.0 && p.1 < hi.1 + 40.0
        });
        assert!(stay, "strokes stay near the shape");
    }

    #[test]
    fn roughness_scales_the_stray_and_zero_is_nearly_straight() {
        let line = Ring {
            pts: vec![(0.0, 0.0), (400.0, 0.0)],
            closed: false,
        };
        let stray = |roughness: f64, bowing: f64| {
            let hand = Hand {
                roughness,
                bowing,
                unit: 4.0,
            };
            outline_pass(std::slice::from_ref(&line), 3, 0, hand)[0]
                .iter()
                .map(|p| p.1.abs())
                .fold(0.0, f64::max)
        };
        assert!(stray(0.0, 0.0) < 1e-6);
        assert!(stray(2.0, 0.0) > stray(0.5, 0.0));
        assert!(
            stray(0.0, 2.0) > 3.0,
            "bowing bends a line even without roughness"
        );
    }

    #[test]
    fn a_circle_is_one_loop_that_runs_past_where_it_began() {
        let rings = [ellipse_ring(200.0, 200.0, 100.0, 100.0)];
        let strokes = outline_pass(&rings, 4, 0, HAND);
        assert_eq!(strokes.len(), 1);
        let line = &strokes[0];
        let (first, last) = (line[0], line[line.len() - 1]);
        assert!(dist(first, last) > 1.0, "the ends do not meet exactly");
        assert!(
            line.iter()
                .all(|p| (dist(*p, (200.0, 200.0)) - 100.0).abs() < 25.0)
        );
    }

    #[test]
    fn compose_gives_a_box_and_data_in_its_own_pixels() {
        let drawn = compose(&[Sub::Open(vec![(50.0, 60.0), (80.0, 70.0), (120.0, 65.0)])])
            .unwrap_or_else(|| Drawn {
                d: String::new(),
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            });
        assert_eq!((drawn.x, drawn.y), (50.0, 60.0));
        assert_eq!((drawn.width, drawn.height), (70.0, 10.0));
        assert!(drawn.d.starts_with("M0,0C"));
        assert!(compose(&[]).is_none());
    }

    #[test]
    fn bad_path_data_is_an_error_and_good_data_has_rings() {
        assert!(path_rings("nonsense").is_err());
        let rings = path_rings("M0,0L100,0L100,100Z M20,20L40,20L40,40Z").unwrap_or_default();
        assert_eq!(rings.len(), 2);
        assert!(rings.iter().all(|r| r.closed));
    }
}
