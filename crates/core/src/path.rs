//! Vector paths: SVG path data (`d`) parsed into absolute segments, validated and capped
//! (`docs/wiki/document-format.md#paths`). A path is data: it is never executed, only measured and
//! drawn. Every command (`M L H V C S Q T A Z`, absolute and relative) becomes one of five
//! absolute segments, so the renderer and the hit test deal with a closed set.

use crate::error::OpError;
use crate::limits::MAX_COORD;

/// Most segments in one path, after expanding arcs into curves.
pub const MAX_SEGMENTS: usize = 4_000;
/// Longest path data string.
pub const MAX_BYTES: usize = 64 * 1024;

pub type Point = [f64; 2];

/// One absolute drawing step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),
    CubicTo(Point, Point, Point),
    Close,
}

/// A path's own coordinate bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A polyline made from flattening curves.
#[derive(Debug, Clone, PartialEq)]
pub struct Contour {
    pub points: Vec<Point>,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    segs: Vec<Seg>,
}

fn bad(detail: impl Into<String>) -> OpError {
    OpError::OutOfBounds {
        what: "path",
        detail: detail.into(),
    }
}

impl Path {
    pub fn from_segments(segs: Vec<Seg>) -> Path {
        Path { segs }
    }

    pub fn segments(&self) -> &[Seg] {
        &self.segs
    }

    /// Parses and validates SVG path data: at most [`MAX_BYTES`] and [`MAX_SEGMENTS`], finite
    /// numbers, every coordinate within the document's coordinate range, starting with a move.
    pub fn parse(d: &str) -> Result<Path, OpError> {
        if d.len() > MAX_BYTES {
            return Err(bad(format!("longer than {MAX_BYTES} bytes")));
        }
        let mut parser = Parser::new(d);
        parser.run()?;
        if parser.segs.is_empty() {
            return Err(bad("is empty"));
        }
        Ok(Path { segs: parser.segs })
    }

    /// The tight bounds of the drawn outline (curve extremes, not control points), or `None` for a
    /// path with no points.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut acc = Extent::default();
        let mut at = [0.0, 0.0];
        let mut start = at;
        for seg in &self.segs {
            match *seg {
                Seg::MoveTo(p) => {
                    acc.add(p);
                    at = p;
                    start = p;
                }
                Seg::LineTo(p) => {
                    acc.add(p);
                    at = p;
                }
                Seg::QuadTo(c, p) => {
                    acc.add(p);
                    for t in quad_extrema(at, c, p) {
                        acc.add(quad_at(at, c, p, t));
                    }
                    at = p;
                }
                Seg::CubicTo(c1, c2, p) => {
                    acc.add(p);
                    for t in cubic_extrema(at, c1, c2, p) {
                        acc.add(cubic_at(at, c1, c2, p, t));
                    }
                    at = p;
                }
                Seg::Close => at = start,
            }
        }
        acc.bounds()
    }

    /// The path as absolute SVG path data (`M L Q C Z`), numbers rounded to three decimals.
    pub fn to_data(&self) -> String {
        let mut out = String::new();
        for seg in &self.segs {
            if !out.is_empty() {
                out.push(' ');
            }
            match *seg {
                Seg::MoveTo(p) => write_command(&mut out, 'M', &[p]),
                Seg::LineTo(p) => write_command(&mut out, 'L', &[p]),
                Seg::QuadTo(c, p) => write_command(&mut out, 'Q', &[c, p]),
                Seg::CubicTo(a, b, p) => write_command(&mut out, 'C', &[a, b, p]),
                Seg::Close => out.push('Z'),
            }
        }
        out
    }

    /// The path with every point mapped by `x * sx + tx`, `y * sy + ty`.
    pub fn scaled(&self, sx: f64, sy: f64, tx: f64, ty: f64) -> Path {
        let m = |p: Point| [p[0] * sx + tx, p[1] * sy + ty];
        let segs = self
            .segs
            .iter()
            .map(|s| match *s {
                Seg::MoveTo(p) => Seg::MoveTo(m(p)),
                Seg::LineTo(p) => Seg::LineTo(m(p)),
                Seg::QuadTo(c, p) => Seg::QuadTo(m(c), m(p)),
                Seg::CubicTo(a, b, p) => Seg::CubicTo(m(a), m(b), m(p)),
                Seg::Close => Seg::Close,
            })
            .collect();
        Path { segs }
    }

    /// The path as polylines, each curve cut into pieces at most `max_step` long (along its
    /// control polygon, at least 2 and at most 256 pieces).
    pub fn flatten(&self, max_step: f64) -> Vec<Contour> {
        let step = max_step.max(1e-6);
        let mut out: Vec<Contour> = Vec::new();
        let mut at = [0.0, 0.0];
        let mut start = at;
        for seg in &self.segs {
            match *seg {
                Seg::MoveTo(p) => {
                    out.push(Contour {
                        points: vec![p],
                        closed: false,
                    });
                    at = p;
                    start = p;
                }
                Seg::LineTo(p) => {
                    push_point(&mut out, at, p);
                    at = p;
                }
                Seg::QuadTo(c, p) => {
                    let n = pieces(dist(at, c) + dist(c, p), step);
                    for i in 1..=n {
                        push_point(&mut out, at, quad_at(at, c, p, i as f64 / n as f64));
                    }
                    at = p;
                }
                Seg::CubicTo(c1, c2, p) => {
                    let n = pieces(dist(at, c1) + dist(c1, c2) + dist(c2, p), step);
                    for i in 1..=n {
                        push_point(&mut out, at, cubic_at(at, c1, c2, p, i as f64 / n as f64));
                    }
                    at = p;
                }
                Seg::Close => {
                    if let Some(last) = out.last_mut() {
                        last.closed = true;
                    }
                    at = start;
                }
            }
        }
        out
    }
}

fn write_command(out: &mut String, letter: char, points: &[Point]) {
    out.push(letter);
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&number(p[0]));
        out.push(' ');
        out.push_str(&number(p[1]));
    }
}

fn number(v: f64) -> String {
    let text = format!("{v:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" || text.is_empty() {
        "0".to_string()
    } else {
        text.to_string()
    }
}

/// Adds `p` to the contour being drawn, opening a new one at `from` after a close.
fn push_point(out: &mut Vec<Contour>, from: Point, p: Point) {
    match out.last_mut() {
        Some(c) if !c.closed => c.points.push(p),
        _ => out.push(Contour {
            points: vec![from, p],
            closed: false,
        }),
    }
}

fn pieces(length: f64, step: f64) -> usize {
    ((length / step).ceil() as usize).clamp(2, 256)
}

fn dist(a: Point, b: Point) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[derive(Default)]
struct Extent {
    min: Option<Point>,
    max: Option<Point>,
}

impl Extent {
    fn add(&mut self, p: Point) {
        let lo = self.min.get_or_insert(p);
        let hi = self.max.get_or_insert(p);
        for i in 0..2 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }

    fn bounds(&self) -> Option<Bounds> {
        let (lo, hi) = (self.min?, self.max?);
        Some(Bounds {
            x: lo[0],
            y: lo[1],
            width: hi[0] - lo[0],
            height: hi[1] - lo[1],
        })
    }
}

fn quad_at(p0: Point, c: Point, p1: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let f = |i: usize| u * u * p0[i] + 2.0 * u * t * c[i] + t * t * p1[i];
    [f(0), f(1)]
}

fn cubic_at(p0: Point, c1: Point, c2: Point, p1: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let f = |i: usize| {
        u * u * u * p0[i] + 3.0 * u * u * t * c1[i] + 3.0 * u * t * t * c2[i] + t * t * t * p1[i]
    };
    [f(0), f(1)]
}

/// Parameters in (0, 1) where either coordinate of a quadratic stops growing.
fn quad_extrema(p0: Point, c: Point, p1: Point) -> Vec<f64> {
    (0..2)
        .filter_map(|i| {
            let den = p0[i] - 2.0 * c[i] + p1[i];
            (den.abs() > 1e-12).then(|| (p0[i] - c[i]) / den)
        })
        .filter(|t| *t > 0.0 && *t < 1.0)
        .collect()
}

/// Parameters in (0, 1) where either coordinate of a cubic stops growing.
fn cubic_extrema(p0: Point, c1: Point, c2: Point, p1: Point) -> Vec<f64> {
    let mut found = Vec::new();
    for i in 0..2 {
        let a = -p0[i] + 3.0 * c1[i] - 3.0 * c2[i] + p1[i];
        let b = 2.0 * (p0[i] - 2.0 * c1[i] + c2[i]);
        let c = c1[i] - p0[i];
        if a.abs() < 1e-12 {
            if b.abs() > 1e-12 {
                found.push(-c / b);
            }
            continue;
        }
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let root = disc.sqrt();
            found.push((-b + root) / (2.0 * a));
            found.push((-b - root) / (2.0 * a));
        }
    }
    found.retain(|t| *t > 0.0 && *t < 1.0);
    found
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    segs: Vec<Seg>,
    current: Point,
    start: Point,
    /// The control point a following `S` or `T` mirrors, with the kind of curve it ended.
    reflect: Option<(Point, bool)>,
    /// A close was the last step: a drawing command then starts a new subpath at `start`.
    closed: bool,
}

impl<'a> Parser<'a> {
    fn new(d: &'a str) -> Parser<'a> {
        Parser {
            bytes: d.as_bytes(),
            at: 0,
            segs: Vec::new(),
            current: [0.0, 0.0],
            start: [0.0, 0.0],
            reflect: None,
            closed: false,
        }
    }

    fn run(&mut self) -> Result<(), OpError> {
        self.skip_space();
        let mut first = true;
        while self.at < self.bytes.len() {
            let command = self.bytes[self.at];
            if !command.is_ascii_alphabetic() {
                return Err(bad(format!(
                    "expected a command letter at byte {}",
                    self.at
                )));
            }
            if first && !matches!(command, b'M' | b'm') {
                return Err(bad("must start with M or m"));
            }
            first = false;
            self.at += 1;
            self.command(command)?;
            self.skip_space();
        }
        Ok(())
    }

    fn command(&mut self, command: u8) -> Result<(), OpError> {
        if matches!(command, b'Z' | b'z') {
            self.close();
            return Ok(());
        }
        let Some(arity) = arity(command) else {
            return Err(bad(format!("unknown command {:?}", command as char)));
        };
        let mut repeat = command;
        loop {
            let mut args = [0.0; 7];
            for (i, slot) in args.iter_mut().enumerate().take(arity) {
                *slot = self.argument(repeat, i)?;
            }
            self.step(repeat, &args)?;
            if repeat == b'M' {
                repeat = b'L';
            } else if repeat == b'm' {
                repeat = b'l';
            }
            self.skip_space();
            if !self.at_number() {
                return Ok(());
            }
        }
    }

    /// Argument `i` of `command`: an arc's flags are single digits, everything else a number.
    fn argument(&mut self, command: u8, i: usize) -> Result<f64, OpError> {
        if matches!(command, b'A' | b'a') && (i == 3 || i == 4) {
            return self.flag();
        }
        self.number()
    }

    fn skip_space(&mut self) {
        while self
            .bytes
            .get(self.at)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b'\x0c'))
        {
            self.at += 1;
        }
    }

    fn skip_separator(&mut self) {
        self.skip_space();
        if self.bytes.get(self.at) == Some(&b',') {
            self.at += 1;
            self.skip_space();
        }
    }

    fn at_number(&self) -> bool {
        match self.bytes.get(self.at) {
            Some(b) => b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b','),
            None => false,
        }
    }

    fn flag(&mut self) -> Result<f64, OpError> {
        self.skip_separator();
        let v = match self.bytes.get(self.at) {
            Some(b'0') => 0.0,
            Some(b'1') => 1.0,
            _ => {
                return Err(bad(format!(
                    "expected an arc flag 0 or 1 at byte {}",
                    self.at
                )));
            }
        };
        self.at += 1;
        Ok(v)
    }

    fn number(&mut self) -> Result<f64, OpError> {
        self.skip_separator();
        let begin = self.at;
        let digits = |p: &mut Parser<'a>| {
            let from = p.at;
            while p.bytes.get(p.at).is_some_and(u8::is_ascii_digit) {
                p.at += 1;
            }
            p.at - from
        };
        if matches!(self.bytes.get(self.at), Some(b'+' | b'-')) {
            self.at += 1;
        }
        let mut count = digits(self);
        if self.bytes.get(self.at) == Some(&b'.') {
            self.at += 1;
            count += digits(self);
        }
        if count == 0 {
            return Err(bad(format!("expected a number at byte {begin}")));
        }
        if matches!(self.bytes.get(self.at), Some(b'e' | b'E')) {
            let mark = self.at;
            self.at += 1;
            if matches!(self.bytes.get(self.at), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if digits(self) == 0 {
                self.at = mark;
            }
        }
        let text = std::str::from_utf8(&self.bytes[begin..self.at])
            .map_err(|_| bad("is not valid text"))?;
        let v: f64 = text
            .parse()
            .map_err(|_| bad(format!("{text:?} is not a number")))?;
        if !v.is_finite() || v.abs() > MAX_COORD {
            return Err(bad(format!("{text} is not within +-{MAX_COORD}")));
        }
        Ok(v)
    }

    fn push(&mut self, seg: Seg) -> Result<(), OpError> {
        if self.segs.len() >= MAX_SEGMENTS {
            return Err(bad(format!("has more than {MAX_SEGMENTS} segments")));
        }
        let points: &[Point] = match &seg {
            Seg::MoveTo(p) | Seg::LineTo(p) => std::slice::from_ref(p),
            Seg::QuadTo(c, p) => &[*c, *p],
            Seg::CubicTo(a, b, p) => &[*a, *b, *p],
            Seg::Close => &[],
        };
        if points
            .iter()
            .any(|p| p.iter().any(|v| !v.is_finite() || v.abs() > MAX_COORD))
        {
            return Err(bad(format!("leaves the +-{MAX_COORD} coordinate range")));
        }
        self.segs.push(seg);
        Ok(())
    }

    /// Starts a drawing step: after a close it begins a new subpath where the last one began.
    fn begin(&mut self) -> Result<(), OpError> {
        if self.closed {
            self.closed = false;
            self.push(Seg::MoveTo(self.start))?;
        }
        Ok(())
    }

    fn close(&mut self) {
        if matches!(self.segs.last(), Some(Seg::Close)) {
            return;
        }
        self.segs.push(Seg::Close);
        self.current = self.start;
        self.reflect = None;
        self.closed = true;
    }

    fn step(&mut self, command: u8, a: &[f64; 7]) -> Result<(), OpError> {
        let relative = command.is_ascii_lowercase();
        let origin = if relative { self.current } else { [0.0, 0.0] };
        let abs = |x: f64, y: f64| [origin[0] + x, origin[1] + y];
        let previous = self.reflect.take();
        match command.to_ascii_uppercase() {
            b'M' => {
                self.closed = false;
                let p = abs(a[0], a[1]);
                self.push(Seg::MoveTo(p))?;
                self.current = p;
                self.start = p;
            }
            b'L' => {
                self.begin()?;
                self.line(abs(a[0], a[1]))?;
            }
            b'H' => {
                self.begin()?;
                self.line([origin[0] + a[0], self.current[1]])?;
            }
            b'V' => {
                self.begin()?;
                self.line([self.current[0], origin[1] + a[0]])?;
            }
            b'C' => {
                self.begin()?;
                self.cubic(abs(a[0], a[1]), abs(a[2], a[3]), abs(a[4], a[5]))?;
            }
            b'S' => {
                self.begin()?;
                let c1 = reflection(previous, true, self.current);
                self.cubic(c1, abs(a[0], a[1]), abs(a[2], a[3]))?;
            }
            b'Q' => {
                self.begin()?;
                self.quad(abs(a[0], a[1]), abs(a[2], a[3]))?;
            }
            b'T' => {
                self.begin()?;
                let c = reflection(previous, false, self.current);
                self.quad(c, abs(a[0], a[1]))?;
            }
            _ => {
                self.begin()?;
                let to = abs(a[5], a[6]);
                self.arc(a[0], a[1], a[2], a[3] != 0.0, a[4] != 0.0, to)?;
            }
        }
        Ok(())
    }

    fn line(&mut self, to: Point) -> Result<(), OpError> {
        self.push(Seg::LineTo(to))?;
        self.current = to;
        Ok(())
    }

    fn cubic(&mut self, c1: Point, c2: Point, to: Point) -> Result<(), OpError> {
        self.push(Seg::CubicTo(c1, c2, to))?;
        self.current = to;
        self.reflect = Some((c2, true));
        Ok(())
    }

    fn quad(&mut self, c: Point, to: Point) -> Result<(), OpError> {
        self.push(Seg::QuadTo(c, to))?;
        self.current = to;
        self.reflect = Some((c, false));
        Ok(())
    }

    fn arc(
        &mut self,
        rx: f64,
        ry: f64,
        rotation: f64,
        large: bool,
        sweep: bool,
        to: Point,
    ) -> Result<(), OpError> {
        let from = self.current;
        if from == to {
            return Ok(());
        }
        if rx == 0.0 || ry == 0.0 {
            return self.line(to);
        }
        for [c1, c2, p] in arc_cubics(from, rx.abs(), ry.abs(), rotation, large, sweep, to) {
            self.push(Seg::CubicTo(c1, c2, p))?;
        }
        self.current = to;
        Ok(())
    }
}

fn arity(command: u8) -> Option<usize> {
    Some(match command.to_ascii_uppercase() {
        b'M' | b'L' | b'T' => 2,
        b'H' | b'V' => 1,
        b'C' => 6,
        b'S' | b'Q' => 4,
        b'A' => 7,
        _ => return None,
    })
}

/// The control point a smooth curve inherits: the previous curve's last control point mirrored
/// through the current point, when the previous step was a curve of the same kind.
fn reflection(previous: Option<(Point, bool)>, cubic: bool, current: Point) -> Point {
    match previous {
        Some((c, was_cubic)) if was_cubic == cubic => {
            [2.0 * current[0] - c[0], 2.0 * current[1] - c[1]]
        }
        _ => current,
    }
}

/// An elliptical arc as cubic curves, at most a quarter turn each (SVG 1.1 appendix F.6).
fn arc_cubics(
    from: Point,
    mut rx: f64,
    mut ry: f64,
    rotation_deg: f64,
    large: bool,
    sweep: bool,
    to: Point,
) -> Vec<[Point; 3]> {
    let phi = rotation_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let (dx, dy) = ((from[0] - to[0]) / 2.0, (from[1] - to[1]) / 2.0);
    let (x1, y1) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    let lambda = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if lambda > 1.0 {
        let s = lambda.sqrt();
        rx *= s;
        ry *= s;
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coefficient = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        coefficient = -coefficient;
    }
    let (cxp, cyp) = (coefficient * rx * y1 / ry, -coefficient * ry * x1 / rx);
    let center = [
        cos * cxp - sin * cyp + (from[0] + to[0]) / 2.0,
        sin * cxp + cos * cyp + (from[1] + to[1]) / 2.0,
    ];
    let theta1 = ((y1 - cyp) / ry).atan2((x1 - cxp) / rx);
    let theta2 = ((-y1 - cyp) / ry).atan2((-x1 - cxp) / rx);
    let mut delta = theta2 - theta1;
    if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    } else if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    }
    let count = (delta.abs() / std::f64::consts::FRAC_PI_2 - 1e-9)
        .ceil()
        .max(1.0) as usize;
    let each = delta / count as f64;
    let k = 4.0 / 3.0 * (each / 4.0).tan();
    let map = |ux: f64, uy: f64| -> Point {
        let (x, y) = (ux * rx, uy * ry);
        [cos * x - sin * y + center[0], sin * x + cos * y + center[1]]
    };
    (0..count)
        .map(|i| {
            let a = theta1 + each * i as f64;
            let b = a + each;
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = b.sin_cos();
            let end = if i + 1 == count { to } else { map(cb, sb) };
            [
                map(ca - k * sa, sa + k * ca),
                map(cb + k * sb, sb - k * cb),
                end,
            ]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(d: &str) -> Vec<Seg> {
        Path::parse(d).unwrap().segments().to_vec()
    }

    #[test]
    fn absolute_and_relative_lines_become_absolute_segments() {
        assert_eq!(
            parse("M10 10 L20 10 l0 10 H5 h-5 V40 v-10 z"),
            vec![
                Seg::MoveTo([10.0, 10.0]),
                Seg::LineTo([20.0, 10.0]),
                Seg::LineTo([20.0, 20.0]),
                Seg::LineTo([5.0, 20.0]),
                Seg::LineTo([0.0, 20.0]),
                Seg::LineTo([0.0, 40.0]),
                Seg::LineTo([0.0, 30.0]),
                Seg::Close,
            ]
        );
    }

    #[test]
    fn extra_pairs_after_a_move_are_lines() {
        assert_eq!(
            parse("m1 1 2 2 3 3"),
            vec![
                Seg::MoveTo([1.0, 1.0]),
                Seg::LineTo([3.0, 3.0]),
                Seg::LineTo([6.0, 6.0]),
            ]
        );
    }

    #[test]
    fn curves_and_smooth_curves_reflect_the_previous_control_point() {
        let segs = parse("M0 0 C0 10 10 10 10 0 S20 -10 20 0 Q25 10 30 0 T40 0");
        assert_eq!(
            segs[2],
            Seg::CubicTo([10.0, -10.0], [20.0, -10.0], [20.0, 0.0])
        );
        assert_eq!(segs[3], Seg::QuadTo([25.0, 10.0], [30.0, 0.0]));
        assert_eq!(segs[4], Seg::QuadTo([35.0, -10.0], [40.0, 0.0]));
    }

    #[test]
    fn a_smooth_curve_after_a_line_starts_at_the_current_point() {
        let segs = parse("M0 0 L10 0 S20 10 30 0");
        assert_eq!(
            segs[2],
            Seg::CubicTo([10.0, 0.0], [20.0, 10.0], [30.0, 0.0])
        );
    }

    #[test]
    fn numbers_may_be_packed_with_signs_dots_and_exponents() {
        assert_eq!(
            parse("M.5.5L1e1-2.5e0"),
            vec![Seg::MoveTo([0.5, 0.5]), Seg::LineTo([10.0, -2.5])]
        );
        assert_eq!(
            parse("M0,0,5,5"),
            vec![Seg::MoveTo([0.0, 0.0]), Seg::LineTo([5.0, 5.0])]
        );
    }

    #[test]
    fn a_half_circle_arc_reaches_its_end_and_bounds_match_the_circle() {
        let path = Path::parse("M0 0 A10 10 0 0 1 20 0").unwrap();
        let Some(Seg::CubicTo(_, _, end)) = path.segments().last() else {
            panic!("arc is not cubic");
        };
        assert_eq!(*end, [20.0, 0.0]);
        let b = path.bounds().unwrap();
        assert!((b.y + 10.0).abs() < 1e-6, "top {}", b.y);
        assert!((b.width - 20.0).abs() < 1e-6);
        assert!((b.height - 10.0).abs() < 1e-6);
    }

    #[test]
    fn arc_flags_may_be_packed_and_the_sweep_flag_picks_the_side() {
        let up = Path::parse("M0 0a10 10 0 01 20 0")
            .unwrap()
            .bounds()
            .unwrap();
        let down = Path::parse("M0 0a10 10 0 00 20 0")
            .unwrap()
            .bounds()
            .unwrap();
        assert!(up.y < -9.0 && down.y > -1.0, "{up:?} {down:?}");
    }

    #[test]
    fn an_arc_too_small_for_its_endpoints_is_scaled_up_and_a_zero_radius_is_a_line() {
        let b = Path::parse("M0 0 A1 1 0 0 1 20 0")
            .unwrap()
            .bounds()
            .unwrap();
        assert!((b.width - 20.0).abs() < 1e-6);
        assert_eq!(parse("M0 0 A0 5 0 0 1 20 0")[1], Seg::LineTo([20.0, 0.0]));
    }

    #[test]
    fn a_large_arc_covers_more_than_half_the_circle() {
        let b = Path::parse("M0 0 A10 10 0 1 1 10 10")
            .unwrap()
            .bounds()
            .unwrap();
        assert!(b.width > 14.0 && b.height > 14.0, "{b:?}");
    }

    #[test]
    fn bounds_are_tight_around_curves() {
        let b = Path::parse("M0 0 C0 20 20 20 20 0")
            .unwrap()
            .bounds()
            .unwrap();
        assert!((b.height - 15.0).abs() < 1e-9, "{b:?}");
        assert_eq!((b.x, b.width), (0.0, 20.0));
    }

    #[test]
    fn a_drawing_command_after_a_close_starts_where_the_subpath_began() {
        assert_eq!(
            parse("M5 5 L10 5 Z L9 9"),
            vec![
                Seg::MoveTo([5.0, 5.0]),
                Seg::LineTo([10.0, 5.0]),
                Seg::Close,
                Seg::MoveTo([5.0, 5.0]),
                Seg::LineTo([9.0, 9.0]),
            ]
        );
    }

    #[test]
    fn malformed_data_is_refused() {
        for d in [
            "",
            "   ",
            "L10 10",
            "M10",
            "M10 10 L",
            "M10 10 X5",
            "M10 10 20",
            "M 1e999 0",
            "M0 0 A5 5 0 2 0 9 9",
            "M0 0 Z 5 5",
            "M0 0 L nan 0",
            "M1000001 0",
            "M0 0 l1000000 0 l1000000 0",
            "M0 0 #",
        ] {
            assert!(Path::parse(d).is_err(), "{d:?} should be refused");
        }
    }

    #[test]
    fn size_and_segment_caps_hold() {
        let long = format!("M0 0{}", " l1 1".repeat(MAX_SEGMENTS));
        assert!(Path::parse(&long).is_err());
        let ok = format!("M0 0{}", " l1 1".repeat(MAX_SEGMENTS - 1));
        assert_eq!(Path::parse(&ok).unwrap().segments().len(), MAX_SEGMENTS);
        let huge = format!("M0 0 {}", "1".repeat(MAX_BYTES));
        assert!(Path::parse(&huge).is_err());
    }

    #[test]
    fn flattening_keeps_endpoints_and_closes_contours() {
        let path = Path::parse("M0 0 L10 0 L10 10 Z M20 20 C20 30 30 30 30 20").unwrap();
        let contours = path.flatten(1.0);
        assert_eq!(contours.len(), 2);
        assert!(contours[0].closed && !contours[1].closed);
        assert_eq!(contours[1].points.first(), Some(&[20.0, 20.0]));
        assert_eq!(contours[1].points.last(), Some(&[30.0, 20.0]));
        assert!(contours[1].points.len() > 5);
    }

    #[test]
    fn data_written_back_parses_to_the_same_path() {
        let path = Path::parse("M0 0 L10.5 0 Q12 5 10 10 C8 12 2 12 0 10 Z M-3 -3 l1 1").unwrap();
        let text = path.to_data();
        assert_eq!(
            text,
            "M0 0 L10.5 0 Q12 5 10 10 C8 12 2 12 0 10 Z M-3 -3 L-2 -2"
        );
        assert_eq!(
            Path::parse(&text).unwrap(),
            Path::parse(&text).unwrap().clone()
        );
        assert_eq!(Path::parse(&text).unwrap().segments(), path.segments());
    }

    #[test]
    fn scaling_maps_every_point() {
        let b = Path::parse("M0 0 L10 20")
            .unwrap()
            .scaled(2.0, 0.5, 1.0, 1.0)
            .bounds()
            .unwrap();
        assert_eq!((b.x, b.y, b.width, b.height), (1.0, 1.0, 20.0, 10.0));
    }
}
