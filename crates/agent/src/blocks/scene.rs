//! What the scenery blocks draw with: colors mixed from the palette's four roles, gradient paints,
//! smooth paths, the clipped window a block uses when its box is smaller than the frame, and the
//! ambient loops that give every element its idle life.

use od_core::geometry::Rect;
use od_core::transform::Shadow;
use od_core::{NewNode, NodeKind, Paint, Rgba, Stop, ViewBox};

use super::kit::{Kit, Role, Target};
use crate::scene_motion::Track;

pub const WHITE: Rgba = Rgba {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
pub const BLACK: Rgba = Rgba {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};

/// The four roles of a block's palette as plain colors, for mixing the tints a scene needs.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: Rgba,
    pub fg: Rgba,
    pub accent: Rgba,
    pub muted: Rgba,
}

pub fn palette(k: &mut Kit) -> Palette {
    let mut get = |role: Role| {
        let p = k.paint(role);
        Rgba {
            a: 255,
            ..k.rgba(&p)
        }
    };
    Palette {
        bg: get(Role::Background),
        fg: get(Role::Foreground),
        accent: get(Role::Accent),
        muted: get(Role::Muted),
    }
}

/// `a` moved `t` of the way to `b`.
pub fn blend(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let m = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    Rgba {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
        a: m(a.a, b.a),
    }
}

pub fn lighten(c: Rgba, t: f64) -> Rgba {
    blend(c, WHITE, t)
}

pub fn darken(c: Rgba, t: f64) -> Rgba {
    blend(c, BLACK, t)
}

/// `c` moved toward its own gray: less saturated by `t`.
pub fn desaturate(c: Rgba, t: f64) -> Rgba {
    let luma =
        (0.299 * f64::from(c.r) + 0.587 * f64::from(c.g) + 0.114 * f64::from(c.b)).round() as u8;
    blend(
        c,
        Rgba {
            r: luma,
            g: luma,
            b: luma,
            a: c.a,
        },
        t,
    )
}

/// `c` with `a` (0 to 1) as its opacity.
pub fn alpha(c: Rgba, a: f64) -> Rgba {
    Rgba {
        a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ..c
    }
}

pub fn solid(c: Rgba) -> Paint {
    Paint::Color(c)
}

fn stops(list: &[(f64, Rgba)]) -> Vec<Stop> {
    list.iter()
        .map(|&(offset, c)| Stop {
            offset,
            color: Paint::Color(c),
        })
        .collect()
}

/// A linear gradient: `angle` 90 runs top to bottom, 0 left to right.
pub fn linear(angle: f64, list: &[(f64, Rgba)]) -> Paint {
    Paint::Linear {
        angle,
        stops: stops(list),
    }
}

/// A radial gradient around (`cx`, `cy`) as fractions of the box, reaching the last stop at `r`.
pub fn radial(cx: f64, cy: f64, r: f64, list: &[(f64, Rgba)]) -> Paint {
    Paint::Radial {
        cx,
        cy,
        r,
        stops: stops(list),
    }
}

/// A soft shadow straight down, `dy` below and `blur` wide.
pub fn soft_shadow(color: Rgba, dy: f64, blur: f64) -> Shadow {
    Shadow {
        dx: 0.0,
        dy,
        blur,
        color: Paint::Color(color),
    }
}

/// A number for path data: one decimal, no trailing zero.
pub fn num(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or(s.clone(), str::to_string)
}

fn pt(p: (f64, f64)) -> String {
    format!("{},{}", num(p.0), num(p.1))
}

/// A smooth curve through `points` (Catmull-Rom as cubic segments), open or closed. The data
/// starts with a move to the first point.
pub fn smooth(points: &[(f64, f64)], closed: bool) -> String {
    let n = points.len();
    let mut d = format!("M{}", pt(points[0]));
    let at = |i: isize| -> (f64, f64) {
        if closed {
            points[i.rem_euclid(n as isize) as usize]
        } else {
            points[i.clamp(0, n as isize - 1) as usize]
        }
    };
    let segments = if closed { n } else { n - 1 };
    for i in 0..segments as isize {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, p1.1 + (p2.1 - p0.1) / 6.0);
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, p2.1 - (p3.1 - p1.1) / 6.0);
        d.push_str(&format!("C{} {} {}", pt(c1), pt(c2), pt(p2)));
    }
    if closed {
        d.push('Z');
    }
    d
}

/// Straight segments through `points`, closed or open.
pub fn polyline(points: &[(f64, f64)], closed: bool) -> String {
    let mut d = format!("M{}", pt(points[0]));
    for p in &points[1..] {
        d.push_str(&format!("L{}", pt(*p)));
    }
    if closed {
        d.push('Z');
    }
    d
}

/// A circle as a closed subpath; several of them in one path fill as their union.
pub fn circle(cx: f64, cy: f64, r: f64) -> String {
    format!(
        "M{},{}A{r},{r} 0 1 1 {},{}A{r},{r} 0 1 1 {},{}Z",
        num(cx - r),
        num(cy),
        num(cx + r),
        num(cy),
        num(cx - r),
        num(cy),
        r = num(r)
    )
}

/// A rectangle as a closed subpath, clockwise.
pub fn rect_sub(x: f64, y: f64, w: f64, h: f64) -> String {
    format!("M{},{}h{}v{}h{}Z", num(x), num(y), num(w), num(h), num(-w))
}

/// A path node whose data is in the node's own pixels (the view box is the box).
pub fn path_node(d: String, r: Rect, fill: Option<Paint>) -> NewNode {
    let mut n = NewNode::new(NodeKind::Path);
    n.x = r.x;
    n.y = r.y;
    n.width = r.width;
    n.height = r.height;
    n.path = Some(d);
    n.view_box = Some(ViewBox {
        x: 0.0,
        y: 0.0,
        width: r.width,
        height: r.height,
    });
    n.fill = fill;
    n
}

/// Whether the block's box is the whole frame.
pub fn fills_frame(k: &Kit) -> bool {
    (k.b.x - k.frame.x).abs() < 0.5
        && (k.b.y - k.frame.y).abs() < 0.5
        && (k.b.width - k.frame.width).abs() < 0.5
        && (k.b.height - k.frame.height).abs() < 0.5
}

/// Runs `build` so that whatever it draws or scrolls beyond the box is cut at the box. The frame
/// already cuts at its own edge, so a block that fills the frame needs no window; in a smaller
/// box the drawing sits in a clipping group at the box, shifted back so `build` keeps drawing in
/// the frame's coordinates.
pub fn windowed(k: &mut Kit, build: impl FnOnce(&mut Kit)) {
    if fills_frame(k) {
        build(k);
        return;
    }
    let b = k.b;
    k.begin("window");
    k.begin("stage");
    build(k);
    k.end_with(|g| {
        g.x = -b.x;
        g.y = -b.y;
    });
    k.end_with(|g| {
        g.x = b.x;
        g.y = b.y;
        g.width = b.width;
        g.height = b.height;
        g.clip = Some(true);
    });
}

/// Puts a looping or one-shot track from `crate::scene_motion` on a part of the block.
pub fn play(k: &mut Kit, target: Target, t: &Track) {
    let keys: Vec<_> = t.keys.iter().map(|k| (k.at_ms, k.value, k.ease)).collect();
    k.track(target, t.prop, &keys, t.repeat);
}

pub fn play_part(k: &mut Kit, part: &str, t: &Track) {
    let target = k.part(part);
    play(k, target, t);
}

/// A period from `base` times `factor`, in whole seconds (a whole number of frames at any frame
/// rate, so a loop's keys keep their spacing when they snap to the video's frames).
pub fn period(base: u64, factor: f64) -> u64 {
    ((base as f64 * factor / 1_000.0).round() as u64).max(1) * 1_000
}

/// How many times slower than the nearest layer layer `i` of `n` (0 farthest) scrolls: 4, 2 or 1,
/// so a scene of layers repeats exactly every four times the nearest layer's period.
pub fn slowness(i: usize, n: usize) -> u64 {
    1 << (2 - (i * 3 / n.max(1)).min(2))
}

/// The samples of one thing that travels a track and starts over: its place `u` along it (0 the
/// start, 1 the end) over one loop of `loop_ms`, as `(time, u)`, beginning at `first` (0.1 to 0.9).
/// Between the end and the start it hides for 100 ms, so the thing must be invisible at both
/// ends; the wrap is then a jump nobody sees and every value is the same at both ends of the loop.
pub fn lap(first: f64, loop_ms: u64) -> Vec<(u64, f64)> {
    let wrap = ((1.0 - first) * loop_ms as f64 / 100.0).round() as u64 * 100;
    let hidden = wrap + 100;
    let steps = |from: u64, to: u64| ((to - from) / 250).max(2);
    let mut out = Vec::new();
    let n = steps(0, wrap);
    out.extend((0..=n).map(|j| {
        let t = wrap * j / n;
        (t, first + t as f64 / loop_ms as f64)
    }));
    out.push((hidden, 0.0));
    let m = steps(hidden, loop_ms);
    out.extend((1..=m).map(|j| {
        let t = hidden + (loop_ms - hidden) * j / m;
        (t, first * (t - hidden) as f64 / (loop_ms - hidden) as f64)
    }));
    out
}
