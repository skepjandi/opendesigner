//! The faces of a `character`: eighteen expressions, each its own drawing of eyes, lids, brows,
//! mouth, cheeks and tears so that the silhouette of the face differs, not only a few degrees of
//! a brow. A character that changes expression draws one face per expression and the motion
//! swaps them (`character_acting`); the first face keeps the plain part names, the others are
//! prefixed with their expression (`sad_eye_l`).

use od_core::geometry::Rect;
use od_core::{NewNode, Paint};

use super::art::{
    Grp, Named, centered, ellipse, glow, line, linear, lit, path, pen, rect_of, rounded, solid,
    turned,
};
use super::characters::{Cfg, Geo, Pal, Style, parse, radial_glow, shade_rgba, solid_rgba};
use super::kit::{Kit, mix};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Expr {
    Neutral,
    Happy,
    Excited,
    Surprised,
    Shocked,
    Sad,
    Crying,
    Angry,
    Annoyed,
    Thinking,
    Confused,
    Sleepy,
    Bored,
    Smug,
    Worried,
    Scared,
    Love,
    Determined,
}

const ALL: [(Expr, &str); 18] = [
    (Expr::Neutral, "neutral"),
    (Expr::Happy, "happy"),
    (Expr::Excited, "excited"),
    (Expr::Surprised, "surprised"),
    (Expr::Shocked, "shocked"),
    (Expr::Sad, "sad"),
    (Expr::Crying, "crying"),
    (Expr::Angry, "angry"),
    (Expr::Annoyed, "annoyed"),
    (Expr::Thinking, "thinking"),
    (Expr::Confused, "confused"),
    (Expr::Sleepy, "sleepy"),
    (Expr::Bored, "bored"),
    (Expr::Smug, "smug"),
    (Expr::Worried, "worried"),
    (Expr::Scared, "scared"),
    (Expr::Love, "love"),
    (Expr::Determined, "determined"),
];

pub const NAMES: &[&str] = &[
    "neutral",
    "happy",
    "excited",
    "surprised",
    "shocked",
    "sad",
    "crying",
    "angry",
    "annoyed",
    "thinking",
    "confused",
    "sleepy",
    "bored",
    "smug",
    "worried",
    "scared",
    "love",
    "determined",
];

impl Expr {
    pub fn parse(name: &str) -> Option<Expr> {
        ALL.iter().find(|(_, n)| *n == name).map(|(e, _)| *e)
    }

    pub fn name(self) -> &'static str {
        ALL.iter()
            .find(|(e, _)| *e == self)
            .map_or("neutral", |(_, n)| n)
    }
}

/// The part name of `base` on the face drawn with `prefix` (the first face has none).
pub fn part(prefix: &str, base: &str) -> String {
    if prefix.is_empty() {
        base.to_string()
    } else {
        format!("{prefix}_{base}")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Iris {
    Dot,
    Spark,
    Heart,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mouth {
    Smile,
    Frown,
    Cranky,
    Flat,
    Tiny,
    Smirk,
    Wavy,
    Worry,
    Pout,
    Zigzag,
    Cat,
    Grin,
    Cheer,
    OpenO,
    Gasp,
    Wail,
    Yawn,
    Teeth,
}

impl Mouth {
    /// Whether the mouth is an open shape that shows without talking.
    fn open(self) -> bool {
        matches!(
            self,
            Mouth::Grin
                | Mouth::Cheer
                | Mouth::OpenO
                | Mouth::Gasp
                | Mouth::Wail
                | Mouth::Yawn
                | Mouth::Teeth
        )
    }

    /// Width and height as shares of the head's width, and how far it sits lower than usual.
    fn size(self) -> (f64, f64, f64) {
        match self {
            Mouth::Cheer => (0.36, 0.23, 0.0),
            Mouth::Gasp => (0.19, 0.27, 0.035),
            Mouth::Wail => (0.3, 0.16, 0.01),
            Mouth::Yawn => (0.17, 0.2, 0.02),
            Mouth::Tiny => (0.16, 0.08, 0.01),
            Mouth::Teeth => (0.3, 0.13, 0.0),
            Mouth::Zigzag => (0.3, 0.12, 0.01),
            Mouth::Worry => (0.22, 0.1, 0.01),
            Mouth::Cat => (0.22, 0.11, 0.0),
            _ => (0.26, 0.17, 0.0),
        }
    }
}

/// What one expression asks of each part of the face.
#[derive(Clone, Copy)]
struct Look {
    /// Each eye's width and height as a multiple of the usual.
    eye: [(f64, f64); 2],
    pupil: f64,
    iris: Iris,
    /// How much of the eye's height the upper lid covers at the inner and the outer corner.
    lid: (f64, f64),
    /// Each brow's lift (in eye widths) and tilt (degrees, positive: the inner end lower).
    brow: Option<[(f64, f64); 2]>,
    mouth: Mouth,
    cheeks: f64,
    gaze: (f64, f64),
    tears: bool,
}

const BASE: Look = Look {
    eye: [(1.0, 1.0); 2],
    pupil: 1.0,
    iris: Iris::Dot,
    lid: (0.0, 0.0),
    brow: None,
    mouth: Mouth::Smile,
    cheeks: 0.5,
    gaze: (0.0, 0.0),
    tears: false,
};

fn look(expr: Expr) -> Look {
    let same = |lift: f64, tilt: f64| Some([(lift, tilt); 2]);
    match expr {
        Expr::Neutral => BASE,
        Expr::Happy => Look {
            brow: same(0.25, -5.0),
            mouth: Mouth::Grin,
            cheeks: 0.7,
            gaze: (0.0, 0.1),
            ..BASE
        },
        Expr::Excited => Look {
            eye: [(1.2, 1.3); 2],
            pupil: 1.3,
            iris: Iris::Spark,
            brow: same(0.7, -8.0),
            mouth: Mouth::Cheer,
            cheeks: 0.85,
            gaze: (0.0, -0.1),
            ..BASE
        },
        Expr::Surprised => Look {
            eye: [(1.2, 1.2); 2],
            pupil: 0.775,
            brow: same(0.55, 0.0),
            mouth: Mouth::OpenO,
            ..BASE
        },
        Expr::Shocked => Look {
            eye: [(1.45, 1.55); 2],
            pupil: 0.5,
            brow: same(0.95, 0.0),
            mouth: Mouth::Gasp,
            cheeks: 0.1,
            ..BASE
        },
        Expr::Sad => Look {
            lid: (0.0, 0.2),
            brow: same(0.25, -14.0),
            mouth: Mouth::Frown,
            cheeks: 0.25,
            gaze: (0.0, 0.6),
            ..BASE
        },
        Expr::Crying => Look {
            eye: [(1.0, 0.95); 2],
            pupil: 1.1,
            lid: (0.0, 0.3),
            brow: same(0.1, -22.0),
            mouth: Mouth::Wail,
            cheeks: 0.6,
            gaze: (0.0, 0.5),
            tears: true,
            ..BASE
        },
        Expr::Angry => Look {
            eye: [(1.0, 0.86); 2],
            lid: (0.4, 0.06),
            brow: same(-0.05, 20.0),
            mouth: Mouth::Cranky,
            gaze: (0.0, 0.2),
            ..BASE
        },
        Expr::Annoyed => Look {
            pupil: 0.95,
            lid: (0.5, 0.32),
            brow: Some([(0.0, 6.0), (0.12, 3.0)]),
            mouth: Mouth::Flat,
            cheeks: 0.1,
            gaze: (0.55, 0.0),
            ..BASE
        },
        Expr::Thinking => Look {
            brow: Some([(0.3, -8.0), (0.65, -8.0)]),
            mouth: Mouth::Pout,
            gaze: (0.7, -0.7),
            ..BASE
        },
        Expr::Confused => Look {
            eye: [(1.0, 1.0), (1.1, 1.25)],
            pupil: 0.7,
            brow: Some([(0.05, 12.0), (0.75, -12.0)]),
            mouth: Mouth::Wavy,
            cheeks: 0.35,
            gaze: (0.4, -0.3),
            ..BASE
        },
        Expr::Sleepy => Look {
            pupil: 0.9,
            lid: (0.6, 0.6),
            brow: same(0.05, -6.0),
            mouth: Mouth::Yawn,
            cheeks: 0.6,
            gaze: (0.0, 0.4),
            ..BASE
        },
        Expr::Bored => Look {
            pupil: 0.9,
            lid: (0.55, 0.55),
            brow: same(0.3, 0.0),
            mouth: Mouth::Tiny,
            cheeks: 0.15,
            gaze: (0.3, -0.6),
            ..BASE
        },
        Expr::Smug => Look {
            lid: (0.42, 0.42),
            brow: Some([(0.1, 8.0), (0.8, -10.0)]),
            mouth: Mouth::Smirk,
            cheeks: 0.45,
            gaze: (0.7, 0.1),
            ..BASE
        },
        Expr::Worried => Look {
            eye: [(1.05, 1.15); 2],
            pupil: 0.75,
            brow: same(0.55, -22.0),
            mouth: Mouth::Worry,
            cheeks: 0.2,
            gaze: (0.0, 0.25),
            ..BASE
        },
        Expr::Scared => Look {
            eye: [(1.35, 1.45); 2],
            pupil: 0.45,
            brow: same(0.85, -16.0),
            mouth: Mouth::Zigzag,
            cheeks: 0.0,
            ..BASE
        },
        Expr::Love => Look {
            eye: [(1.15, 1.15); 2],
            iris: Iris::Heart,
            brow: same(0.4, -6.0),
            mouth: Mouth::Cat,
            cheeks: 1.0,
            gaze: (0.0, -0.05),
            ..BASE
        },
        Expr::Determined => Look {
            eye: [(1.0, 0.95); 2],
            pupil: 0.9,
            lid: (0.3, 0.2),
            brow: same(-0.1, 24.0),
            mouth: Mouth::Teeth,
            cheeks: 0.2,
            gaze: (0.25, -0.1),
            ..BASE
        },
    }
}

/// Whether this expression's mouth is an open shape that shows without talking.
pub fn open_visible(expr: Expr) -> bool {
    look(expr).mouth.open()
}

/// The eyes' layout, shared by every face, the glasses and the cheeks.
pub struct Eyes {
    pub cy: f64,
    pub gap: f64,
    pub ew: f64,
    pub eh: f64,
    pub dx: f64,
}

pub fn eyes_layout(g: &Geo, cfg: &Cfg) -> Eyes {
    let h = g.head;
    let robot = cfg.style == Style::Robot;
    let ew = h.width * if robot { 0.16 } else { 0.19 };
    Eyes {
        cy: h.y + h.height * 0.47,
        gap: h.width * 0.2,
        ew,
        eh: ew * if robot { 1.05 } else { 1.18 },
        dx: cfg.facing * h.width * 0.13,
    }
}

/// Where one pupil can go: its position inside its eye for a gaze of `(0, 0)` and how far it
/// can travel, both relative to the eye's own corner.
pub struct PupilRig {
    pub centre: (f64, f64),
    pub room: (f64, f64),
    /// Where the pupil is drawn, which is where it rests.
    pub start: (f64, f64),
    eye: (f64, f64),
    reach: f64,
}

impl PupilRig {
    /// The pupil's position for a gaze given as shares of its room (-1 to 1 on each axis).
    pub fn at(&self, share: (f64, f64)) -> (f64, f64) {
        (
            self.centre.0 + share.0 * self.room.0,
            self.centre.1 + share.1 * self.room.1,
        )
    }

    /// The shares that turn the pupil toward the frame point `target`.
    pub fn toward(&self, target: (f64, f64)) -> (f64, f64) {
        share_toward(self.eye, self.reach, target)
    }
}

fn share_toward(from: (f64, f64), reach: f64, target: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (target.0 - from.0, target.1 - from.1);
    let len = dx.hypot(dy).max(1.0);
    let amount = (len / reach).min(1.0);
    (dx / len * amount, dy / len * amount)
}

/// What the motion needs to know about one drawn face.
pub struct FaceRig {
    pub prefix: String,
    pub expr: Expr,
    pub pupils: [PupilRig; 2],
    /// How far the whole face sits off the head's middle because the character looks at something.
    pub turn: (f64, f64),
    pub tears: bool,
    /// Where a tear starts, relative to the face.
    pub tears_y: [f64; 2],
    pub heart: bool,
    pub trembles: bool,
}

struct Ctx<'a> {
    k: &'a Kit<'a>,
    g: &'a Geo,
    pal: &'a Pal,
    cfg: &'a Cfg,
    e: &'a Eyes,
    look: Look,
    prefix: &'a str,
}

impl Ctx<'_> {
    fn name(&self, base: &str) -> String {
        part(self.prefix, base)
    }

    fn robot(&self) -> bool {
        self.cfg.style == Style::Robot
    }

    fn ink(&self) -> Paint {
        if self.robot() {
            lit(&self.pal.accent, self.pal.accent_c)
        } else {
            self.pal.ink.clone()
        }
    }
}

/// One face: cheeks, eyes with their lids, brows, mouth and tears, in a group that is the
/// head's size so a face can be swapped for another or turned as one piece.
pub fn draw_face(
    k: &Kit,
    g: &Geo,
    pal: &Pal,
    cfg: &Cfg,
    e: &Eyes,
    expr: Expr,
    prefix: &str,
) -> (Grp, FaceRig) {
    let c = Ctx {
        k,
        g,
        pal,
        cfg,
        e,
        look: look(expr),
        prefix,
    };
    let mut face = Grp::new(k, &c.name("face"), g.head).pivot(g.cx, e.cy);
    let turn = face_turn(&c);
    face.node.x += turn.0;
    face.node.y += turn.1;
    if !c.robot() {
        draw_cheeks(&c, &mut face);
    }
    let (eyes, pupils) = draw_eyes(&c);
    face.add_group(eyes);
    if let Some(brows) = draw_brows(&c) {
        face.add_group(brows);
    }
    face.add_group(draw_mouth(&c));
    if c.look.tears {
        for side in 0..2 {
            face.add_group(draw_tear(&c, side));
        }
    }
    let rig = FaceRig {
        prefix: prefix.to_string(),
        expr,
        pupils,
        turn,
        tears: c.look.tears,
        tears_y: [e.cy + e.eh * 0.42 - g.head.y; 2],
        heart: c.look.iris == Iris::Heart,
        trembles: expr == Expr::Scared,
    };
    (
        if prefix.is_empty() {
            face
        } else {
            face.opacity(0.0)
        },
        rig,
    )
}

/// A character that looks at something turns its face a little toward it.
fn face_turn(c: &Ctx) -> (f64, f64) {
    c.cfg
        .look
        .map_or((0.0, 0.0), |target| turn_toward(c.g, target))
}

/// How far the face sits off the head's middle when the character looks at the frame point `target`.
pub fn turn_toward(g: &Geo, target: (f64, f64)) -> (f64, f64) {
    let eyes_y = g.head.y + g.head.height * 0.47;
    let (gx, gy) = share_toward((g.cx, eyes_y), g.head.width * 1.5, target);
    (gx * g.head.width * 0.04, gy * g.head.height * 0.025)
}

fn draw_cheeks(c: &Ctx, face: &mut Grp) {
    let alpha = c.look.cheeks;
    if alpha <= 0.0 {
        return;
    }
    let blush = parse(&mix(c.pal.skin_c, solid_rgba("#ff4d6d"), 0.6));
    let (w, h) = (c.g.head.width * 0.15, c.g.head.width * 0.09);
    for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
        let cx = c.g.cx + c.e.dx + sign * c.g.head.width * 0.3;
        let cy = c.e.cy + c.g.head.height * 0.18;
        face.add(
            ellipse(centered(cx, cy, w, h), glow(blush, alpha))
                .named(c.k, &c.name(&format!("cheek_{i}"))),
        );
    }
}

fn draw_eyes(c: &Ctx) -> (Grp, [PupilRig; 2]) {
    let e = c.e;
    let box_ = rect_of(
        c.g.cx + e.dx - e.gap - e.ew / 2.0,
        e.cy - e.eh / 2.0,
        2.0 * e.gap + e.ew,
        e.eh,
    );
    let mut eyes = Grp::new(c.k, &c.name("eyes"), box_);
    let far = |sign: f64| (c.cfg.facing * sign).is_sign_negative() && c.cfg.facing != 0.0;
    let (left, left_rig) = draw_eye(c, 0, -1.0, far(-1.0));
    let (right, right_rig) = draw_eye(c, 1, 1.0, far(1.0));
    eyes.add_group(left);
    eyes.add_group(right);
    (eyes, [left_rig, right_rig])
}

fn draw_eye(c: &Ctx, side: usize, sign: f64, far: bool) -> (Grp, PupilRig) {
    let (e, k) = (c.e, c.k);
    let tag = if side == 0 { "l" } else { "r" };
    let cx = c.g.cx + e.dx + sign * e.gap;
    let (mw, mh) = c.look.eye[side];
    let (w, h) = (e.ew * mw, e.eh * mh);
    let box_ = centered(cx, e.cy, w, h);
    let mut eye = Grp::new(k, &c.name(&format!("eye_{tag}")), box_);
    if far {
        eye.node.scale_x = Some(0.82);
    }
    if c.look.iris != Iris::Heart {
        let sclera = if c.robot() {
            radial_glow(c.pal.accent_c)
        } else {
            linear(90.0, &[(0.0, solid("#ffffff")), (1.0, solid("#dce4ef"))])
        };
        eye.add(ellipse(box_, sclera).named(k, &c.name(&format!("sclera_{tag}"))));
    }
    let (pupil, rig) = draw_pupil(c, tag, cx, box_);
    eye.add_group(pupil);
    add_lid(c, &mut eye, side, box_);
    (eye, rig)
}

/// The pupil group, placed where the expression looks, and the room it has to move in.
fn draw_pupil(c: &Ctx, tag: &str, cx: f64, eye: Rect) -> (Grp, PupilRig) {
    let heart = c.look.iris == Iris::Heart;
    let robot_shrink = if c.robot() { 0.7 } else { 1.0 };
    let pw = if heart {
        eye.width * 0.95
    } else {
        eye.width * 0.7 * 0.8 * c.look.pupil * robot_shrink
    };
    let ph = if heart { pw * 0.9 } else { pw * 1.1 };
    let slack = if heart { 0.25 } else { 0.85 };
    let room = (
        (eye.width - pw) / 2.0 * slack,
        (eye.height - ph) / 2.0 * slack,
    );
    let centre = ((eye.width - pw) / 2.0, (eye.height - ph) / 2.0);
    let mut rig = PupilRig {
        centre,
        room,
        start: centre,
        eye: (cx, c.e.cy),
        reach: c.g.head.width * 1.5,
    };
    let (ox, oy) = rig.at(gaze(c, &rig));
    rig.start = (ox, oy);
    let pupil_box = rect_of(eye.x + ox, eye.y + oy, pw, ph);
    let mut pupil = Grp::new(c.k, &c.name(&format!("pupil_{tag}")), pupil_box);
    pupil_marks(c, tag, &mut pupil, pupil_box);
    (pupil, rig)
}

fn gaze(c: &Ctx, rig: &PupilRig) -> (f64, f64) {
    match c.cfg.look {
        Some(target) => rig.toward(target),
        None => (c.look.gaze.0 + c.cfg.facing * 0.5, c.look.gaze.1),
    }
}

fn pupil_marks(c: &Ctx, tag: &str, pupil: &mut Grp, b: Rect) {
    let k = c.k;
    match c.look.iris {
        Iris::Heart => {
            let d = heart_d(b.width, b.height);
            let pink = solid(&mix(c.pal.skin_c, solid_rgba("#ff2e63"), 0.92));
            pupil.add(path(b, d, Some(pink)).named(k, &c.name(&format!("pupil_shape_{tag}"))));
            let glint = b.width * 0.18;
            pupil.add(
                ellipse(
                    centered(
                        b.x + b.width * 0.3,
                        b.y + b.height * 0.3,
                        glint,
                        glint * 0.8,
                    ),
                    solid("#ffffff"),
                )
                .named(k, &c.name(&format!("glint_{tag}"))),
            );
        }
        _ => {
            let ink = if c.robot() {
                Paint::Color(shade_rgba(c.pal.accent_c))
            } else {
                c.pal.ink.clone()
            };
            pupil.add(ellipse(b, ink).named(k, &c.name(&format!("pupil_shape_{tag}"))));
            ordinary_glints(c, tag, pupil, b);
        }
    }
}

fn ordinary_glints(c: &Ctx, tag: &str, pupil: &mut Grp, b: Rect) {
    let wet = if c.look.tears { 1.45 } else { 1.0 };
    let glint = b.width * 0.36 * wet;
    let at = (
        b.x + b.width / 2.0 + b.width * 0.16,
        b.y + b.height / 2.0 - b.height * 0.2,
    );
    let name = c.name(&format!("glint_{tag}"));
    if c.look.iris == Iris::Spark {
        let star = centered(at.0, at.1, glint * 1.5, glint * 1.5);
        pupil.add(
            path(
                star,
                star_d(star.width, star.height),
                Some(solid("#ffffff")),
            )
            .named(c.k, &name),
        );
        let dot = b.width * 0.16;
        pupil.add(
            ellipse(
                centered(b.x + b.width * 0.3, b.y + b.height * 0.74, dot, dot),
                solid("#ffffff"),
            )
            .named(c.k, &c.name(&format!("glint2_{tag}"))),
        );
        return;
    }
    pupil.add(ellipse(centered(at.0, at.1, glint, glint), solid("#ffffff")).named(c.k, &name));
}

/// A four-pointed sparkle with curved sides, filling `w` x `h`.
pub fn star_d(w: f64, h: f64) -> String {
    let (cx, cy) = (w / 2.0, h / 2.0);
    format!(
        "M{cx:.2},0 Q{cx:.2},{cy:.2} {w:.2},{cy:.2} Q{cx:.2},{cy:.2} {cx:.2},{h:.2} \
         Q{cx:.2},{cy:.2} 0,{cy:.2} Q{cx:.2},{cy:.2} {cx:.2},0 Z"
    )
}

/// A heart filling `w` x `h`.
pub fn heart_d(w: f64, h: f64) -> String {
    format!(
        "M{:.2},{:.2} C{:.2},{:.2} 0,{:.2} {:.2},{:.2} C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} \
         C{:.2},{:.2} {:.2},{:.2} {:.2},{:.2} C{w:.2},{:.2} {:.2},{:.2} {:.2},{:.2} Z",
        w * 0.5,
        h * 0.95,
        w * 0.08,
        h * 0.66,
        h * 0.32,
        w * 0.22,
        h * 0.1,
        w * 0.36,
        h * -0.02,
        w * 0.5,
        h * 0.1,
        w * 0.5,
        h * 0.28,
        w * 0.5,
        h * 0.1,
        w * 0.64,
        h * -0.02,
        w * 0.78,
        h * 0.1,
        h * 0.32,
        w * 0.92,
        h * 0.66,
        w * 0.5,
        h * 0.95,
    )
}

/// The upper lid: a wedge of skin over the top of the eye, painted with the head's own gradient
/// (a path as large as the head) so no seam shows, and a dark line along its edge.
fn add_lid(c: &Ctx, eye: &mut Grp, side: usize, b: Rect) {
    let (inner, outer) = c.look.lid;
    if inner <= 0.0 && outer <= 0.0 {
        return;
    }
    let toward_inner = if side == 0 { 1.0 } else { -1.0 };
    let (cx, top) = (b.x + b.width / 2.0, b.y);
    let (x_out, x_in) = (
        cx - toward_inner * b.width * 0.5,
        cx + toward_inner * b.width * 0.5,
    );
    let (y_out, y_in) = (top + outer * b.height, top + inner * b.height);
    let pad = b.width * 0.18;
    let (xo, xi) = (x_out - toward_inner * pad, x_in + toward_inner * pad);
    let slope = (y_in - y_out) / (x_in - x_out);
    let (yo, yi) = (y_out + slope * (xo - x_out), y_in + slope * (xi - x_in));
    let lift = b.height * 0.3;
    let skin_box = skin_canvas(c);
    let local = |x: f64, y: f64| format!("{:.2},{:.2}", x - skin_box.x, y - skin_box.y);
    let d = format!(
        "M{} L{} L{} L{} Z",
        local(xo, top - lift),
        local(xi, top - lift),
        local(xi, yi),
        local(xo, yo)
    );
    eye.add(path(skin_box, d, Some(skin_paint(c))).named(c.k, &c.name(&format!("lid_{side}"))));
    let width = (c.g.head.width * 0.022).max(1.5);
    let edge = rect_of(
        x_out.min(x_in),
        y_out.min(y_in) - width,
        b.width,
        (y_out - y_in).abs() + 2.0 * width,
    );
    let (ya, yb) = if x_out < x_in {
        (y_out - edge.y, y_in - edge.y)
    } else {
        (y_in - edge.y, y_out - edge.y)
    };
    let d = format!("M0,{ya:.2} L{:.2},{yb:.2}", edge.width);
    eye.add(
        line(edge, d, c.pal.ink.clone(), width).named(c.k, &c.name(&format!("lid_edge_{side}"))),
    );
}

/// The rectangle a skin-colored shape uses so its gradient matches the head's (or, on a robot,
/// the face plate's).
fn skin_canvas(c: &Ctx) -> Rect {
    let h = c.g.head;
    if c.robot() {
        rect_of(
            h.x + h.width * 0.09,
            h.y + h.height * 0.16,
            h.width * 0.82,
            h.height * 0.62,
        )
    } else {
        h
    }
}

fn skin_paint(c: &Ctx) -> Paint {
    if c.robot() {
        linear(90.0, &[(0.0, solid("#27324f")), (1.0, solid("#141a2e"))])
    } else {
        lit(&c.pal.skin, c.pal.skin_c)
    }
}

fn draw_brows(c: &Ctx) -> Option<Grp> {
    let looks = c.look.brow?;
    if c.robot() {
        return None;
    }
    let e = c.e;
    let bl = e.ew * 1.15;
    let bt = (e.ew * 0.2).max(1.5);
    let top = e.cy - e.eh / 2.0 - e.ew * 1.2;
    let box_ = rect_of(
        c.g.cx + e.dx - e.gap - bl / 2.0,
        top,
        2.0 * e.gap + bl,
        e.eh + e.ew * 1.4,
    );
    let mut brows = Grp::new(c.k, &c.name("brows"), box_);
    for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
        let (lift, tilt) = looks[i];
        let eye_top = e.cy - e.eh * c.look.eye[i].1 / 2.0;
        let y = (eye_top - e.ew * 0.32 - e.ew * lift).max(c.g.head.y + c.g.head.height * 0.07 + bt);
        let cx = c.g.cx + e.dx + sign * e.gap;
        let brow = rounded(centered(cx, y, bl, bt), bt / 2.0, c.pal.ink.clone());
        brows.add(turned(brow, tilt * -sign).named(c.k, &c.name(&format!("brow_{i}"))));
    }
    Some(brows)
}

fn draw_mouth(c: &Ctx) -> Grp {
    let h = c.g.head;
    let mouth = c.look.mouth;
    let (wk, hk, dy) = mouth.size();
    let (mw, mh) = (h.width * wk, h.width * hk);
    let shift = if mouth == Mouth::Pout {
        h.width * 0.07
    } else {
        0.0
    };
    let low = h.y + h.height * 0.74 + h.width * dy;
    let plate_bottom = h.y + h.height * 0.76 - mh / 2.0;
    let (mx, my) = (
        c.g.cx + c.cfg.facing * h.width * 0.13 + shift,
        if c.robot() && mouth.open() {
            low.min(plate_bottom)
        } else {
            low
        },
    );
    let box_ = centered(mx, my, mw, mh);
    let ink = c.ink();
    let mut group = Grp::new(c.k, &c.name("mouth"), box_);
    if !mouth.open() {
        group.add(lip(c, box_, ink.clone(), h.width * 0.03));
    }
    let mut open = Grp::new(c.k, &c.name("mouth_open"), box_).pivot(mx, box_.y + mh * 0.1);
    for part in open_shape(c, box_, ink) {
        open.add(part);
    }
    if !mouth.open() {
        open = open.opacity(0.0);
        open.node.scale_y = Some(0.3);
    }
    group.add_group(open);
    group
}

fn lip(c: &Ctx, b: Rect, ink: Paint, width: f64) -> NewNode {
    let (w, h) = (b.width, b.height);
    let d = match c.look.mouth {
        Mouth::Frown => format!(
            "M0,{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.85,
            w / 2.0,
            h * 0.05,
            h * 0.85
        ),
        Mouth::Cranky => format!(
            "M0,{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.7,
            w / 2.0,
            h * 0.3,
            h * 0.7
        ),
        Mouth::Flat | Mouth::Tiny => format!("M0,{:.2} L{w:.2},{:.2}", h * 0.5, h * 0.5),
        Mouth::Smirk => format!(
            "M0,{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.62,
            w * 0.55,
            h * 0.8,
            h * 0.05
        ),
        Mouth::Wavy => format!(
            "M0,{:.2} Q{:.2},{:.2} {:.2},{:.2} T{:.2},{:.2} T{:.2},{:.2} T{w:.2},{:.2}",
            h * 0.5,
            w * 0.125,
            h * 0.0,
            w * 0.25,
            h * 0.5,
            w * 0.5,
            h * 0.5,
            w * 0.75,
            h * 0.5,
            h * 0.5
        ),
        Mouth::Worry => format!(
            "M0,{:.2} Q{:.2},{:.2} {:.2},{:.2} T{:.2},{:.2} T{w:.2},{:.2}",
            h * 0.85,
            w * 0.17,
            h * 0.2,
            w * 0.35,
            h * 0.6,
            w * 0.7,
            h * 0.6,
            h * 0.85
        ),
        Mouth::Pout => format!(
            "M0,{:.2} Q{:.2},{:.2} {:.2},{:.2} T{w:.2},{:.2}",
            h * 0.55,
            w * 0.25,
            h * 0.15,
            w / 2.0,
            h * 0.55,
            h * 0.55
        ),
        Mouth::Zigzag => format!(
            "M0,{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{:.2},{:.2} L{w:.2},{:.2}",
            h * 0.5,
            w * 0.17,
            h * 0.05,
            w * 0.34,
            h * 0.95,
            w * 0.5,
            h * 0.05,
            w * 0.67,
            h * 0.95,
            w * 0.84,
            h * 0.05,
            h * 0.5
        ),
        Mouth::Cat => format!(
            "M0,{:.2} Q{:.2},{:.2} {:.2},{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.2,
            w * 0.25,
            h * 1.0,
            w * 0.5,
            h * 0.2,
            w * 0.75,
            h * 1.0,
            h * 0.2
        ),
        _ => format!(
            "M0,{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.3,
            w / 2.0,
            h * 1.0,
            h * 0.3
        ),
    };
    line(b, d, ink, width.max(1.5)).named(c.k, &c.name("lip"))
}

fn tongue_paint(c: &Ctx) -> Paint {
    solid(&mix(c.pal.skin_c, solid_rgba("#ff4d6d"), 0.75))
}

fn open_shape(c: &Ctx, b: Rect, ink: Paint) -> Vec<NewNode> {
    let (w, h) = (b.width, b.height);
    let k = c.k;
    let tongue = |share_w: f64, share_h: f64, at: f64| {
        ellipse(
            centered(b.x + w / 2.0, b.y + h * at, w * share_w, h * share_h),
            tongue_paint(c),
        )
        .named(k, &c.name("tongue"))
    };
    match c.look.mouth {
        Mouth::OpenO => vec![
            ellipse(
                centered(b.x + w / 2.0, b.y + h / 2.0, w * 0.42, h * 0.8),
                ink,
            )
            .named(k, &c.name("mouth_o")),
        ],
        Mouth::Gasp => vec![
            ellipse(b, ink).named(k, &c.name("mouth_o")),
            tongue(0.6, 0.28, 0.78),
        ],
        Mouth::Yawn => vec![
            ellipse(b, ink).named(k, &c.name("mouth_o")),
            tongue(0.6, 0.3, 0.74),
        ],
        Mouth::Wail => {
            let d = format!(
                "M0,{:.2} C0,{:.2} {w:.2},{:.2} {w:.2},{:.2} Z",
                h * 0.95,
                h * -0.15,
                h * -0.15,
                h * 0.95
            );
            vec![
                path(b, d, Some(ink)).named(k, &c.name("mouth_bowl")),
                tongue(0.5, 0.3, 0.8),
            ]
        }
        Mouth::Teeth => teeth(c, b, ink),
        Mouth::Cheer => {
            let mut parts = vec![bowl(c, b, ink)];
            let strip = rect_of(b.x + w * 0.1, b.y + h * 0.1, w * 0.8, h * 0.28);
            parts.push(rounded(strip, h * 0.08, solid("#f7f9fc")).named(k, &c.name("teeth")));
            parts.push(tongue(0.5, 0.3, 0.76));
            parts
        }
        _ => vec![bowl(c, b, ink), tongue(0.5, 0.3, 0.74)],
    }
}

fn bowl(c: &Ctx, b: Rect, ink: Paint) -> NewNode {
    let (w, h) = (b.width, b.height);
    let d = format!(
        "M0,{:.2} L{w:.2},{:.2} C{:.2},{:.2} {:.2},{:.2} 0,{:.2} Z",
        h * 0.1,
        h * 0.1,
        w * 1.0,
        h * 1.15,
        0.0,
        h * 1.15,
        h * 0.1,
    );
    path(b, d, Some(ink)).named(c.k, &c.name("mouth_bowl"))
}

/// A clenched grin: a row of teeth inside an outline.
fn teeth(c: &Ctx, b: Rect, ink: Paint) -> Vec<NewNode> {
    let width = (c.g.head.width * 0.022).max(1.5);
    let mut plate = rounded(b, b.height * 0.32, solid("#f7f9fc"));
    plate.stroke = Some(pen(ink.clone(), width));
    let mut parts = vec![plate.named(c.k, &c.name("teeth"))];
    for i in 1..4 {
        let x = b.x + b.width * f64::from(i) / 4.0;
        let seam = rect_of(x - width / 2.0, b.y, width, b.height);
        parts.push(rect_of_ink(c, seam, &ink, i));
    }
    parts
}

fn rect_of_ink(c: &Ctx, r: Rect, ink: &Paint, i: i32) -> NewNode {
    rounded(r, r.width / 2.0, ink.clone()).named(c.k, &c.name(&format!("tooth_gap_{i}")))
}

/// A tear on its way down the cheek.
fn draw_tear(c: &Ctx, side: usize) -> Grp {
    let sign = if side == 0 { -1.0 } else { 1.0 };
    let (w, h) = (c.g.head.width * 0.07, c.g.head.width * 0.2);
    let cx = c.g.cx + c.e.dx + sign * (c.e.gap + c.e.ew * 0.38);
    let top = c.e.cy + c.e.eh * 0.42;
    let box_ = rect_of(cx - w / 2.0, top, w, h);
    let name = if side == 0 { "tear_l" } else { "tear_r" };
    let mut tear = Grp::new(c.k, &c.name(name), box_);
    let d = format!(
        "M{:.2},0 C{:.2},{:.2} {w:.2},{:.2} {w:.2},{:.2} C{w:.2},{:.2} {:.2},{h:.2} {:.2},{h:.2} \
         C{:.2},{h:.2} 0,{:.2} 0,{:.2} C0,{:.2} {:.2},{:.2} {:.2},0 Z",
        w / 2.0,
        w * 0.95,
        h * 0.4,
        h * 0.62,
        h * 0.74,
        h * 0.92,
        w * 0.78,
        w / 2.0,
        w * 0.22,
        h * 0.92,
        h * 0.74,
        h * 0.62,
        w * 0.05,
        h * 0.4,
        w / 2.0
    );
    let water = Paint::Color(parse("#8fd3ff"));
    tear.add(path(box_, d, Some(water)).named(c.k, &c.name(&format!("{name}_drop"))));
    let shine = centered(cx - w * 0.15, top + h * 0.66, w * 0.28, h * 0.2);
    tear.add(ellipse(shine, solid("#ffffff")).named(c.k, &c.name(&format!("{name}_shine"))));
    tear
}
