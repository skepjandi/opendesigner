//! Trimming an outline to a stretch of its length: what draws a shape on. The path is cut into
//! short straight pieces (finer than a pixel at the drawing scale) and the pieces between the two
//! fractions are kept.

use od_core::path::{Contour, Path as CorePath, Point, Seg};
use tiny_skia::{Path, PathBuilder, PathSegment};

/// The part of `path` from `start` to `end`, both fractions of its total length with
/// `start < end`, flattened into pieces at most `step` long. `None` when nothing is left.
pub(crate) fn trim(path: &Path, start: f64, end: f64, step: f64) -> Option<Path> {
    let contours = to_core(path).flatten(step);
    let total: f64 = contours.iter().map(length).sum();
    if total <= 0.0 {
        return None;
    }
    let (from, to) = (start * total, end * total);
    let mut builder = PathBuilder::new();
    let mut at = 0.0;
    for contour in &contours {
        let len = length(contour);
        if len > 0.0 && at + len > from && at < to {
            add_piece(
                &mut builder,
                contour,
                (from - at).max(0.0),
                (to - at).min(len),
                len,
            );
        }
        at += len;
    }
    builder.finish()
}

fn to_core(path: &Path) -> CorePath {
    let p = |q: tiny_skia::Point| [f64::from(q.x), f64::from(q.y)];
    let segs = path
        .segments()
        .map(|s| match s {
            PathSegment::MoveTo(a) => Seg::MoveTo(p(a)),
            PathSegment::LineTo(a) => Seg::LineTo(p(a)),
            PathSegment::QuadTo(a, b) => Seg::QuadTo(p(a), p(b)),
            PathSegment::CubicTo(a, b, c) => Seg::CubicTo(p(a), p(b), p(c)),
            PathSegment::Close => Seg::Close,
        })
        .collect();
    CorePath::from_segments(segs)
}

/// The edges of a contour as (from, to), the closing edge included for a closed one.
fn edges(c: &Contour) -> Vec<(Point, Point)> {
    let mut edges: Vec<(Point, Point)> = c.points.windows(2).map(|w| (w[0], w[1])).collect();
    if let (true, Some(last), Some(first)) = (c.closed, c.points.last(), c.points.first()) {
        edges.push((*last, *first));
    }
    edges
}

fn length(c: &Contour) -> f64 {
    edges(c)
        .iter()
        .map(|(a, b)| (a[0] - b[0]).hypot(a[1] - b[1]))
        .sum()
}

/// Adds the stretch `from..to` (distances along `contour`, `len` long) as one open piece, or the
/// whole contour closed when the stretch covers all of it.
fn add_piece(builder: &mut PathBuilder, contour: &Contour, from: f64, to: f64, len: f64) {
    let whole = from <= 0.0 && to >= len;
    let mut walked = 0.0;
    let mut started = false;
    for (a, b) in edges(contour) {
        let seg = (a[0] - b[0]).hypot(a[1] - b[1]);
        let (lo, hi) = (walked, walked + seg);
        walked = hi;
        if seg <= 0.0 || hi <= from || lo >= to {
            continue;
        }
        let at = |d: f64| {
            let t = ((d - lo) / seg).clamp(0.0, 1.0);
            [
                (a[0] + (b[0] - a[0]) * t) as f32,
                (a[1] + (b[1] - a[1]) * t) as f32,
            ]
        };
        let first = at(from.max(lo));
        if !started {
            builder.move_to(first[0], first[1]);
            started = true;
        }
        let last = at(to.min(hi));
        builder.line_to(last[0], last[1]);
    }
    if started && whole && contour.closed {
        builder.close();
    }
}
