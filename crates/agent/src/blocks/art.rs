//! The flat-vector drawing kit behind characters, bubbles and props: gradient paints lit from one
//! direction, soft shapes, groups that know their own origin (so a part can turn about a joint),
//! a seeded generator for desynchronised life, and looping key tracks.

use od_core::animation::{AnimProp, Repeat};
use od_core::geometry::Rect;
use od_core::{
    Ease, LineCap, LineJoin, NewNode, NodeKind, Paint, Rgba, Shadow, Stop, Stroke, ViewBox,
};

use super::BlockError;
use super::kit::{Kit, Target, mix};

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

/// The light comes from the upper left in every drawing: gradients run along this angle.
pub const LIGHT_ANGLE: f64 = 55.0;

/// At most this many keys a block writes into one track (the document allows 500).
const MAX_TRACK_KEYS: usize = 480;

pub type Key = (u64, f64, Ease);

pub fn solid(hex: &str) -> Paint {
    Paint::Color(Rgba::parse_hex(hex).unwrap_or(BLACK))
}

/// `c` mixed toward black, as a paint.
pub fn shade(c: Rgba, t: f64) -> Paint {
    solid(&mix(c, BLACK, t))
}

/// `c` mixed toward white, as a paint.
pub fn tint(c: Rgba, t: f64) -> Paint {
    solid(&mix(c, WHITE, t))
}

/// `c` at `a` (0 to 1) opacity.
pub fn faded(c: Rgba, a: f64) -> Paint {
    Paint::Color(Rgba {
        a: (a.clamp(0.0, 1.0) * 255.0).round() as u8,
        ..c
    })
}

fn stops(list: &[(f64, Paint)]) -> Vec<Stop> {
    list.iter()
        .map(|(offset, color)| Stop {
            offset: *offset,
            color: color.clone(),
        })
        .collect()
}

pub fn linear(angle: f64, list: &[(f64, Paint)]) -> Paint {
    Paint::Linear {
        angle,
        stops: stops(list),
    }
}

pub fn radial(cx: f64, cy: f64, r: f64, list: &[(f64, Paint)]) -> Paint {
    Paint::Radial {
        cx,
        cy,
        r,
        stops: stops(list),
    }
}

/// A fill lit from the upper left: a lighter edge, the base color (kept as given, so a token
/// stays a token) and a darker edge.
pub fn lit(base: &Paint, c: Rgba) -> Paint {
    linear(
        LIGHT_ANGLE,
        &[
            (0.0, tint(c, 0.28)),
            (0.55, base.clone()),
            (1.0, shade(c, 0.24)),
        ],
    )
}

/// A soft round spot of `c` that fades to nothing: highlights, glows and cheeks.
pub fn glow(c: Rgba, a: f64) -> Paint {
    radial(0.5, 0.5, 0.5, &[(0.0, faded(c, a)), (1.0, faded(c, 0.0))])
}

pub fn shadow(dx: f64, dy: f64, blur: f64, a: f64) -> Shadow {
    Shadow {
        dx,
        dy,
        blur,
        color: faded(BLACK, a),
    }
}

fn shape(kind: NodeKind, r: Rect) -> NewNode {
    let mut n = NewNode::new(kind);
    n.x = r.x;
    n.y = r.y;
    n.width = r.width;
    n.height = r.height;
    n
}

pub fn ellipse(r: Rect, fill: Paint) -> NewNode {
    let mut n = shape(NodeKind::Ellipse, r);
    n.fill = Some(fill);
    n
}

pub fn rounded(r: Rect, radius: f64, fill: Paint) -> NewNode {
    let mut n = shape(NodeKind::Rect, r);
    n.corner_radius = Some(radius.min(r.width.min(r.height) / 2.0));
    n.fill = Some(fill);
    n
}

/// A path in its own box: `d` is written in the box's pixels, `0,0` at its top left.
pub fn path(r: Rect, d: String, fill: Option<Paint>) -> NewNode {
    let mut n = shape(NodeKind::Path, r);
    n.path = Some(d);
    n.view_box = Some(ViewBox {
        x: 0.0,
        y: 0.0,
        width: r.width.max(0.01),
        height: r.height.max(0.01),
    });
    n.fill = fill;
    n
}

/// A stroke with round ends and joins.
pub fn pen(paint: Paint, width: f64) -> Stroke {
    let mut s = Stroke::new(paint, width);
    s.cap = LineCap::Round;
    s.join = LineJoin::Round;
    s
}

/// An open curve drawn as a round-ended line.
pub fn line(r: Rect, d: String, paint: Paint, width: f64) -> NewNode {
    let mut n = path(r, d, None);
    n.stroke = Some(pen(paint, width));
    n
}

pub fn turned(mut n: NewNode, degrees: f64) -> NewNode {
    n.rotation = Some(degrees);
    n
}

pub fn blurred(mut n: NewNode, radius: f64) -> NewNode {
    n.blur = Some(radius);
    n
}

/// A closed egg: `w` x `h`, widest `widest` of the way down (0.5 is an ellipse).
pub fn blob_d(w: f64, h: f64, widest: f64) -> String {
    const K: f64 = 0.5523;
    let (cx, hy) = (w / 2.0, h * widest);
    let (kx, ku, kd) = (K * cx, K * hy, K * (h - hy));
    format!(
        "M{cx:.2},0 C{:.2},0 {w:.2},{:.2} {w:.2},{hy:.2} C{w:.2},{:.2} {:.2},{h:.2} {cx:.2},{h:.2} \
         C{:.2},{h:.2} 0,{:.2} 0,{hy:.2} C0,{:.2} {:.2},0 {cx:.2},0 Z",
        cx + kx,
        hy - ku,
        hy + kd,
        cx + kx,
        cx - kx,
        hy + kd,
        hy - ku,
        cx - kx,
    )
}

/// A dome: flat at the bottom, round on top, filling `w` x `h`.
pub fn dome_d(w: f64, h: f64) -> String {
    format!(
        "M0,{h:.2} C0,{:.2} {w:.2},{:.2} {w:.2},{h:.2} Z",
        -h * 0.33,
        -h * 0.33
    )
}

/// The rectangle of `w` x `h` centered on `(cx, cy)`.
pub fn centered(cx: f64, cy: f64, w: f64, h: f64) -> Rect {
    Rect {
        x: cx - w / 2.0,
        y: cy - h / 2.0,
        width: w,
        height: h,
    }
}

pub fn rect_of(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

pub trait Named {
    /// The node under the block's part name `part`.
    fn named(self, k: &Kit, part: &str) -> Self;
}

impl Named for NewNode {
    fn named(mut self, k: &Kit, part: &str) -> Self {
        self.name = Some(k.full(part));
        self
    }
}

/// A group with a box of its own, so it can turn and scale about a joint inside that box. Parts
/// are added in frame coordinates; the group stores them relative to its origin.
pub struct Grp {
    pub node: NewNode,
    ox: f64,
    oy: f64,
}

impl Grp {
    pub fn new(k: &Kit, part: &str, r: Rect) -> Grp {
        let mut node = shape(NodeKind::Group, r);
        node.name = Some(k.full(part));
        Grp {
            node,
            ox: r.x,
            oy: r.y,
        }
    }

    /// Adds a node written in frame coordinates.
    pub fn add(&mut self, mut n: NewNode) {
        n.x -= self.ox;
        n.y -= self.oy;
        self.node.children.push(n);
    }

    pub fn add_group(&mut self, g: Grp) {
        self.add(g.node);
    }

    /// Turns and scales about the frame point `(ax, ay)`.
    pub fn pivot(mut self, ax: f64, ay: f64) -> Grp {
        let (w, h) = (self.node.width.max(0.01), self.node.height.max(0.01));
        self.node.pivot_x = Some((ax - self.ox) / w);
        self.node.pivot_y = Some((ay - self.oy) / h);
        self
    }

    pub fn rotated(mut self, degrees: f64) -> Grp {
        self.node.rotation = Some(degrees);
        self
    }

    pub fn shadowed(mut self, s: Shadow) -> Grp {
        self.node.shadow = Some(s);
        self
    }

    pub fn opacity(mut self, a: f64) -> Grp {
        self.node.opacity = Some(a);
        self
    }
}

/// A small deterministic generator: the same seed always gives the same life.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64, salt: u64) -> Rng {
        let s = seed.wrapping_add(1).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ salt.wrapping_mul(0xD1B5_4A32_D192_ED03);
        Rng(s | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `lo..hi`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.next() >> 11) as f64 / (1u64 << 53) as f64)
    }
}

pub fn ms(v: f64) -> u64 {
    v.max(0.0).round() as u64
}

/// Keys closer than this share a frame at 30 fps, where the video snaps them.
const MIN_GAP_MS: u64 = 34;

/// Keys made to increase by at least a frame: a key too close to its predecessor moves after it.
pub fn ordered(keys: Vec<Key>) -> Vec<Key> {
    let mut out: Vec<Key> = Vec::with_capacity(keys.len());
    for (t, v, e) in keys {
        let t = out.last().map_or(t, |p| t.max(p.0 + MIN_GAP_MS));
        out.push((t, v, e));
    }
    out
}

/// One cycle of a looping track written for `loops` repetitions: a count of 0 repeats for as
/// long as the video plays, a count above 0 writes the repetitions out and then holds still.
pub fn cycles(keys: &[Key], mode: Repeat, loops: u32) -> Result<(Vec<Key>, Repeat), BlockError> {
    if loops == 0 || keys.len() < 2 {
        return Ok((ordered(keys.to_vec()), mode));
    }
    let span = keys.last().map_or(0, |k| k.0) - keys.first().map_or(0, |k| k.0);
    let one: Vec<Key> = match mode {
        Repeat::Pingpong => {
            let t0 = keys.first().map_or(0, |k| k.0);
            let mut both = keys.to_vec();
            both.extend(
                keys.iter()
                    .rev()
                    .skip(1)
                    .map(|(t, v, e)| (t0 + 2 * span - (t - t0), *v, *e)),
            );
            both
        }
        _ => keys.to_vec(),
    };
    let period = one.last().map_or(0, |k| k.0) - one.first().map_or(0, |k| k.0);
    let mut out = one.clone();
    for i in 1..u64::from(loops) {
        out.extend(one.iter().skip(1).map(|(t, v, e)| (t + period * i, *v, *e)));
    }
    if out.len() > MAX_TRACK_KEYS {
        return Err(BlockError::Param(format!(
            "{loops} repeats need {} keys in one track (at most {MAX_TRACK_KEYS}): use fewer, or 0 to repeat for ever",
            out.len()
        )));
    }
    Ok((ordered(out), Repeat::Once))
}

/// A pair of keys easing back and forth between `a` and `b` for ever: one half-cycle takes `span`.
pub fn sway(a: f64, b: f64, span: f64) -> (Vec<Key>, Repeat) {
    (
        vec![(0, a, Ease::EaseInOut), (ms(span), b, Ease::EaseInOut)],
        Repeat::Pingpong,
    )
}

/// The tracks a block asks for, one per part and property, a later one replacing an earlier one
/// (an action replaces the idle motion it takes over). Written to the kit in a fixed order.
#[derive(Default)]
pub struct Tracks {
    list: Vec<(String, AnimProp, Vec<Key>, Repeat)>,
}

impl Tracks {
    pub fn set(&mut self, part: &str, prop: AnimProp, keys: Vec<Key>, repeat: Repeat) {
        let keys = ordered(keys);
        match self
            .list
            .iter_mut()
            .find(|(p, pr, ..)| p == part && *pr == prop)
        {
            Some(slot) => *slot = (part.to_string(), prop, keys, repeat),
            None => self.list.push((part.to_string(), prop, keys, repeat)),
        }
    }

    pub fn looping(&mut self, part: &str, prop: AnimProp, track: (Vec<Key>, Repeat)) {
        self.set(part, prop, track.0, track.1);
    }

    pub fn emit(self, k: &mut Kit) -> Result<(), BlockError> {
        for (part, prop, keys, repeat) in self.list {
            if keys.len() > MAX_TRACK_KEYS {
                return Err(BlockError::Param(format!(
                    "{part} {} needs {} keys (at most {MAX_TRACK_KEYS}): shorten the action",
                    prop.as_str(),
                    keys.len()
                )));
            }
            let target = if part.is_empty() {
                Target::Root
            } else {
                Target::Part(k.full(&part))
            };
            k.track(target, prop, &keys, repeat);
        }
        Ok(())
    }
}
