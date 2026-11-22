//! `character`: a flat-vector, rigged explainer character. Every part that should move is a group
//! with a pivot at its joint (shoulder, hip, neck), the drawing is lit from the upper left with
//! soft gradients and one soft drop shadow, and the motion (idle life and an action) is written
//! as ordinary keyframes by `character_motion`.

use od_core::geometry::Rect;
use od_core::{NewNode, Paint, Rgba};

use super::BlockError;
use super::args::{Args, Param};
use super::art::{
    BLACK, Grp, Named, WHITE, blob_d, blurred, centered, dome_d, ellipse, faded, glow, line,
    linear, lit, path, pen, rect_of, rounded, shade, shadow, solid, tint, turned,
};
use super::character_motion::{Rest, Script};
use super::faces::{self, Expr, Eyes};
use super::kit::{Kit, Role, at, mix};
use super::registry::{Block, Category, Layer, Place};

const STYLES: &[&str] = &["blob", "bean", "robot", "creature"];
const FACING: &[&str] = &["auto", "front", "left", "right"];
const ACTIONS: &[&str] = &["idle", "wave", "talk", "walk", "jump", "point", "celebrate"];
const REACTIONS: &[&str] = &["none", "take", "shrug", "nod", "shake_head", "laugh"];

pub const CHARACTER_PARAMS: &[Param] = &[
    Param::choice(
        "style",
        "blob: round body with a big head; bean: tall bean body; robot: boxy machine; creature: blob with antennae.",
        "blob",
        STYLES,
    ),
    Param::text(
        "body_color",
        "The torso: #rrggbb or {token}. Empty takes the accent role.",
        "",
        40,
    ),
    Param::text(
        "accent_color",
        "Shoes, hat, backpack, robot lights: #rrggbb or {token}. Empty is a warm yellow.",
        "",
        40,
    ),
    Param::text(
        "skin_color",
        "Head and hands: #rrggbb or {token}. Empty picks one that suits the style.",
        "",
        40,
    ),
    Param::choice(
        "expression",
        "The face: eyes, lids, brows, mouth, cheeks. Eighteen of them, each a different silhouette.",
        "happy",
        faces::NAMES,
    ),
    Param::choice(
        "facing",
        "front, or turned left or right (the face shifts toward that side); auto faces a walk's direction.",
        "auto",
        FACING,
    ),
    Param::flag(
        "hat",
        "A beanie on the head (replaces the antennae).",
        false,
    ),
    Param::flag("glasses", "Round glasses over the eyes.", false),
    Param::flag("backpack", "A backpack peeking out behind the body.", false),
    Param::choice(
        "action",
        "idle (always on underneath), wave, talk, walk, jump, point at `target`, celebrate (jump with arms up).",
        "idle",
        ACTIONS,
    ),
    Param::count(
        "action_start_ms",
        "When the action begins, from the start of the block.",
        0,
        0,
        600_000,
    ),
    Param::count(
        "action_end_ms",
        "When a walk arrives or talking stops (0: a walk takes 4 s, talking never stops); point lowers its arm then (0: it stays out).",
        0,
        0,
        600_000,
    ),
    Param::data(
        "talk",
        "Time ranges [[start_ms, end_ms], ...] from the start of the block in which the mouth moves, for any action; outside them it is closed. Needs a video.",
        "null",
    ),
    Param::data(
        "from_x",
        "A walk starts with the character's center at this x in the frame (default: left of the box).",
        "null",
    ),
    Param::data(
        "to_x",
        "A walk ends with the character's center at this x in the frame (default: right of the box).",
        "null",
    ),
    Param::region(
        "target",
        "A node id or {x, y} the point action aims its arm at.",
    ),
    Param::region(
        "look_at",
        "A node id or {x, y} the character looks at: the pupils turn to it and the face turns a little toward it.",
    ),
    Param::data(
        "looks",
        "Where the eyes go over time: [{at_ms, x, y}, ...] (frame points, from the start of the block; at least 400 ms apart, up to 24). At each the eyes jump first and the face follows a beat later, so the character looks before it reacts. Starts from `look_at` or the expression's own gaze. Needs a video.",
        "null",
    ),
    Param::data(
        "expression_changes",
        "The character acts through expressions: [{at_ms, expression}, ...] from `expression` (the first at_ms is 200 or more, then 300 ms apart or more; up to 40 changes through 6 different faces). Each change squints first, swaps the face and overshoots before it settles, as ordinary keyframes. Tie each to a story beat. Needs a video.",
        "null",
    ),
    Param::choice(
        "reaction",
        "A one-off bit of acting at `reaction_at_ms`: take (a double-take with squash), shrug, nod, shake_head, laugh (body bounce for `reaction_ms`). Pair it with an expression change at the same time. take and laugh keep the body: not with walk, jump or celebrate; shrug also not with wave or point.",
        "none",
        REACTIONS,
    ),
    Param::count(
        "reaction_at_ms",
        "When the reaction starts, from the start of the block.",
        0,
        0,
        600_000,
    ),
    Param::count(
        "reaction_ms",
        "How long a laugh lasts (0: 1800 ms); the other reactions have their own length and repeat `loops` times.",
        0,
        0,
        20_000,
    ),
    Param::count(
        "loops",
        "How many times wave, jump, point and celebrate play (each one is written out).",
        1,
        1,
        12,
    ),
    Param::count(
        "seed",
        "Desynchronises the breathing, blinking and sway of several characters.",
        1,
        0,
        9999,
    ),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Blob,
    Bean,
    Robot,
    Creature,
}

/// What the face and accessories ask for.
pub struct Cfg {
    pub style: Style,
    /// Every expression that gets a face drawn: the `expression` first, then the ones it changes to.
    pub faces: Vec<Expr>,
    pub facing: f64,
    pub hat: bool,
    pub glasses: bool,
    pub backpack: bool,
    pub look: Option<(f64, f64)>,
}

/// Proportions in units where a character is about 90 tall; the box scales them.
struct Form {
    head: (f64, f64),
    head_widest: f64,
    body: (f64, f64),
    body_widest: f64,
    leg: (f64, f64),
    arm: (f64, f64),
    head_overlap: f64,
    leg_overlap: f64,
    crown: f64,
    shoulder: f64,
}

fn form(style: Style, hat: bool) -> Form {
    let mut f = match style {
        Style::Blob => Form {
            head: (54.0, 46.0),
            head_widest: 0.56,
            body: (44.0, 36.0),
            body_widest: 0.6,
            leg: (9.0, 13.0),
            arm: (7.0, 22.0),
            head_overlap: 8.0,
            leg_overlap: 5.0,
            crown: 0.0,
            shoulder: 0.25,
        },
        Style::Bean => Form {
            head: (40.0, 38.0),
            head_widest: 0.52,
            body: (42.0, 54.0),
            body_widest: 0.42,
            leg: (9.0, 16.0),
            arm: (7.0, 28.0),
            head_overlap: 8.0,
            leg_overlap: 6.0,
            crown: 0.0,
            shoulder: 0.2,
        },
        Style::Robot => Form {
            head: (50.0, 38.0),
            head_widest: 0.5,
            body: (40.0, 34.0),
            body_widest: 0.5,
            leg: (8.0, 14.0),
            arm: (7.0, 24.0),
            head_overlap: 3.0,
            leg_overlap: 3.0,
            crown: 9.0,
            shoulder: 0.22,
        },
        Style::Creature => Form {
            head: (54.0, 46.0),
            head_widest: 0.58,
            body: (36.0, 32.0),
            body_widest: 0.55,
            leg: (8.0, 12.0),
            arm: (6.0, 18.0),
            head_overlap: 8.0,
            leg_overlap: 4.0,
            crown: 14.0,
            shoulder: 0.25,
        },
    };
    if hat {
        f.crown = 17.0;
    }
    f
}

/// Where everything stands, in frame pixels.
pub struct Geo {
    pub s: f64,
    pub cx: f64,
    pub ground: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
    pub head: Rect,
    pub body: Rect,
    pub leg_w: f64,
    pub leg_len: f64,
    pub legs_top: f64,
    pub leg_dx: f64,
    pub arm_w: f64,
    pub arm_len: f64,
    pub shoulder_dx: f64,
    pub shoulder_y: f64,
    form: Form,
}

impl Geo {
    fn new(f: Form, b: Rect) -> Geo {
        let total = f.crown + f.head.1 + f.body.1 + f.leg.1 - f.head_overlap - f.leg_overlap;
        let wide = f.head.0.max(f.body.0 + 1.5 * f.arm.0);
        let s = (b.height / total).min(b.width / wide);
        let cx = b.x + b.width / 2.0;
        let ground = b.y + b.height;
        let top = ground - total * s;
        let head = rect_of(
            cx - f.head.0 * s / 2.0,
            top + f.crown * s,
            f.head.0 * s,
            f.head.1 * s,
        );
        let body_top = head.y + head.height - f.head_overlap * s;
        let body = rect_of(
            cx - f.body.0 * s / 2.0,
            body_top,
            f.body.0 * s,
            f.body.1 * s,
        );
        Geo {
            s,
            cx,
            ground,
            top,
            width: wide * s,
            height: total * s,
            head,
            body,
            leg_w: f.leg.0 * s,
            leg_len: f.leg.1 * s,
            legs_top: body_top + body.height - f.leg_overlap * s,
            leg_dx: f.body.0 * s * 0.2,
            arm_w: f.arm.0 * s,
            arm_len: f.arm.1 * s,
            shoulder_dx: f.body.0 * s / 2.0 - f.arm.0 * s * 0.15,
            shoulder_y: body_top + f.body.1 * s * f.shoulder,
            form: f,
        }
    }

    /// The shoulder (side 0 is on the screen's left) where an arm turns.
    pub fn shoulder(&self, side: usize) -> (f64, f64) {
        let sign = if side == 0 { -1.0 } else { 1.0 };
        (self.cx + sign * self.shoulder_dx, self.shoulder_y)
    }

    /// The hip where a leg turns.
    pub fn hip(&self, side: usize) -> (f64, f64) {
        let sign = if side == 0 { -1.0 } else { 1.0 };
        (
            self.cx + sign * self.leg_dx,
            self.legs_top + self.leg_w * 0.5,
        )
    }
}

/// The colors, each a paint (a token stays a token) and its resolved value for shading.
pub struct Pal {
    pub(super) body: Paint,
    pub(super) body_c: Rgba,
    pub(super) skin: Paint,
    pub(super) skin_c: Rgba,
    pub(super) accent: Paint,
    pub(super) accent_c: Rgba,
    pub(super) ink: Paint,
    legs: Paint,
    legs_c: Rgba,
}

fn given(k: &Kit, a: &Args, name: &'static str) -> Result<Option<Paint>, BlockError> {
    let s = a.text(name);
    if s.is_empty() {
        return Ok(None);
    }
    let paint = Paint::parse(s)?;
    paint.resolve(&k.doc.tokens)?;
    Ok(Some(paint))
}

impl Pal {
    fn new(k: &mut Kit, a: &Args, style: Style) -> Result<Pal, BlockError> {
        let body = match given(k, a, "body_color")? {
            Some(p) => p,
            None => k.paint(Role::Accent),
        };
        let body_c = k.rgba(&body);
        let accent = given(k, a, "accent_color")?.unwrap_or_else(|| solid("#ffbe3d"));
        let skin = given(k, a, "skin_color")?.unwrap_or_else(|| match style {
            Style::Bean => solid("#f4c7a1"),
            Style::Robot => solid("#d7dee8"),
            Style::Blob => solid(&mix(body_c, WHITE, 0.22)),
            Style::Creature => solid(&mix(body_c, WHITE, 0.34)),
        });
        let (accent_c, skin_c) = (k.rgba(&accent), k.rgba(&skin));
        let ink_c = parse(&mix(body_c, solid_rgba("#0a0f1e"), 0.82));
        let legs_c = if style == Style::Robot {
            parse(&mix(skin_c, BLACK, 0.45))
        } else {
            parse(&mix(body_c, BLACK, 0.38))
        };
        Ok(Pal {
            body,
            body_c,
            skin,
            skin_c,
            accent,
            accent_c,
            ink: Paint::Color(ink_c),
            legs: Paint::Color(legs_c),
            legs_c,
        })
    }
}

pub(super) fn solid_rgba(hex: &str) -> Rgba {
    Rgba::parse_hex(hex).unwrap_or(BLACK)
}

pub(super) fn parse(hex: &str) -> Rgba {
    solid_rgba(hex)
}

fn character_box(frame: Rect, margin: f64) -> Rect {
    let h = frame.height
        * if frame.height > frame.width * 1.1 {
            0.4
        } else {
            0.58
        };
    let w = (h * 0.72).min(frame.width - 2.0 * margin);
    let h = h.min(w / 0.72);
    at(
        frame.x + (frame.width - w) / 2.0,
        frame.y + frame.height - margin * 1.5 - h,
        w,
        h,
    )
}

pub const BLOCKS: &[Block] = &[Block {
    id: "character",
    category: Category::Characters,
    about: "A flat-vector explainer character built as a rig of named groups (body, head, eyes, \
            mouth, arm_l, arm_r, leg_l, leg_r, hat, glasses, backpack) with pivots at the joints. \
            With a video it breathes, bobs, blinks and sways for ever (desynchronised by seed) and \
            performs an action: wave, talk (mouth only moves in the ranges you give), walk from \
            from_x to to_x, jump with squash and stretch, point at a target, celebrate. Eighteen \
            expressions, a list of expression_changes (each squints, swaps and overshoots), timed \
            looks (the eyes lead the face), and reactions: take, shrug, nod, shake_head, laugh.",
    tags: &[
        "character",
        "person",
        "mascot",
        "robot",
        "creature",
        "explainer",
        "kurzgesagt",
        "avatar",
        "face",
        "walk",
        "wave",
        "talk",
    ],
    params: CHARACTER_PARAMS,
    place: Place::Custom(character_box),
    layer: Layer::Front,
    build: build_character,
}];

fn read_style(a: &Args) -> Style {
    match a.text("style") {
        "bean" => Style::Bean,
        "robot" => Style::Robot,
        "creature" => Style::Creature,
        _ => Style::Blob,
    }
}

pub(super) fn read_expr(a: &Args) -> Expr {
    Expr::parse(a.text("expression")).unwrap_or(Expr::Neutral)
}

fn read_facing(a: &Args, script: &Script) -> f64 {
    match a.text("facing") {
        "left" => -1.0,
        "right" => 1.0,
        "front" => 0.0,
        _ => script.walk_direction(),
    }
}

fn read_cfg(a: &Args, script: &Script) -> Cfg {
    let look = a
        .region("look_at")
        .map(|r| (r.x + r.width / 2.0, r.y + r.height / 2.0));
    let expr = read_expr(a);
    Cfg {
        style: read_style(a),
        faces: script.acting.faces(expr),
        facing: read_facing(a, script),
        hat: a.flag("hat"),
        glasses: a.flag("glasses"),
        backpack: a.flag("backpack"),
        look,
    }
}

fn build_character(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let script = Script::read(a, k)?;
    let cfg = read_cfg(a, &script);
    let pal = Pal::new(k, a, cfg.style)?;
    let geo = Geo::new(form(cfg.style, cfg.hat), k.b);
    k.add("shadow", ground_shadow(k, &geo));
    let (rig, rest) = draw_rig(k, &geo, &pal, &cfg);
    k.add("rig", rig.node);
    super::character_motion::animate(k, &script, &geo, &rest, &cfg)
}

fn ground_shadow(k: &Kit, g: &Geo) -> NewNode {
    let w = g.width * 0.8;
    let h = w * 0.16;
    blurred(
        ellipse(
            centered(g.cx, g.ground - h * 0.25, w, h),
            faded(BLACK, 0.26),
        ),
        h * 0.4,
    )
    .named(k, "shadow")
}

fn draw_rig(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg) -> (Grp, Rest) {
    let rig_box = rect_of(g.cx - g.width / 2.0, g.top, g.width, g.height);
    let mut rig = Grp::new(k, "rig", rig_box)
        .pivot(g.cx, g.ground)
        .shadowed(shadow(0.0, g.height * 0.018, g.height * 0.035, 0.2));
    let (rox, roy) = (rig_box.x, rig_box.y);
    for side in 0..2 {
        rig.add_group(draw_leg(k, g, pal, side));
    }
    if cfg.backpack {
        rig.add_group(draw_backpack(k, g, pal, cfg));
    }
    rig.add_group(draw_body(k, g, pal, cfg));
    let (head, mut rest) = draw_head(k, g, pal, cfg);
    rest.head_x = g.head.x - rox;
    rest.head_y = g.head.y - roy;
    rig.add_group(head);
    for (side, arm_y) in rest.arm_y.iter_mut().enumerate() {
        rig.add_group(draw_arm(k, g, pal, cfg, side));
        *arm_y = g.shoulder(side).1 - g.arm_w / 2.0 - roy;
    }
    rest.rig_x = rox;
    rest.rig_y = roy;
    (rig, rest)
}

fn draw_leg(k: &Kit, g: &Geo, pal: &Pal, side: usize) -> Grp {
    let (hx, _) = g.hip(side);
    let outward = if side == 0 { -1.0 } else { 1.0 } * g.leg_w * 0.14;
    let foot_h = g.leg_w * 0.62;
    let box_ = rect_of(hx - g.leg_w * 0.85, g.legs_top, g.leg_w * 1.7, g.leg_len);
    let name = if side == 0 { "leg_l" } else { "leg_r" };
    let (_, hy) = g.hip(side);
    let mut grp = Grp::new(k, name, box_).pivot(hx, hy);
    let limb = rounded(
        rect_of(
            hx - g.leg_w / 2.0,
            g.legs_top,
            g.leg_w,
            g.leg_len - foot_h * 0.4,
        ),
        g.leg_w / 2.0,
        lit(&pal.legs, pal.legs_c),
    );
    grp.add(limb.named(k, &format!("{name}_shape")));
    let shoe = ellipse(
        rect_of(
            hx - g.leg_w * 0.8 + outward,
            g.ground - foot_h,
            g.leg_w * 1.6,
            foot_h,
        ),
        lit(&pal.accent, pal.accent_c),
    );
    grp.add(shoe.named(k, &format!("{name}_shoe")));
    grp
}

fn draw_backpack(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg) -> Grp {
    let side = if cfg.facing > 0.0 { -1.0 } else { 1.0 };
    let w = g.body.width * 0.5;
    let h = g.body.height * 0.72;
    let x = g.cx + side * g.body.width * 0.5 - w / 2.0;
    let y = g.body.y + g.body.height * 0.14;
    let mut grp = Grp::new(k, "backpack", rect_of(x, y, w, h));
    grp.add(
        rounded(
            rect_of(x, y, w, h),
            w * 0.32,
            lit(&pal.accent, pal.accent_c),
        )
        .named(k, "backpack_shape"),
    );
    grp.add(
        rounded(
            rect_of(x + w * 0.18, y + h * 0.52, w * 0.64, h * 0.3),
            w * 0.16,
            shade(pal.accent_c, 0.2),
        )
        .named(k, "backpack_pocket"),
    );
    grp
}

fn torso(k: &Kit, g: &Geo, pal: &Pal, style: Style) -> NewNode {
    let b = g.body;
    let fill = lit(&pal.body, pal.body_c);
    match style {
        Style::Robot => rounded(b, b.width * 0.26, fill),
        _ => path(b, blob_d(b.width, b.height, g.form.body_widest), Some(fill)),
    }
    .named(k, "body_shape")
}

fn draw_body(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg) -> Grp {
    let b = g.body;
    let mut grp = Grp::new(k, "body", b).pivot(g.cx, b.y + b.height);
    grp.add(torso(k, g, pal, cfg.style));
    if cfg.style == Style::Robot {
        let panel = rect_of(
            g.cx - b.width * 0.24,
            b.y + b.height * 0.26,
            b.width * 0.48,
            b.height * 0.4,
        );
        grp.add(rounded(panel, b.width * 0.08, shade(pal.body_c, 0.3)).named(k, "chest_panel"));
        for i in 0..3 {
            let x = panel.x + panel.width * (0.2 + 0.3 * f64::from(i));
            let dot = centered(
                x + panel.width * 0.1,
                panel.y + panel.height / 2.0,
                panel.width * 0.14,
                panel.width * 0.14,
            );
            grp.add(
                ellipse(dot, lit(&pal.accent, pal.accent_c)).named(k, &format!("chest_light_{i}")),
            );
        }
    } else {
        let belly = centered(g.cx, b.y + b.height * 0.6, b.width * 0.62, b.height * 0.52);
        grp.add(ellipse(belly, glow(WHITE, 0.4)).named(k, "belly"));
    }
    let shine = rect_of(
        b.x + b.width * 0.12,
        b.y + b.height * 0.1,
        b.width * 0.26,
        b.height * 0.2,
    );
    grp.add(turned(ellipse(shine, glow(WHITE, 0.45)), -30.0).named(k, "body_shine"));
    grp
}

fn draw_arm(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg, side: usize) -> Grp {
    let (sx, sy) = g.shoulder(side);
    let hand = g.arm_w * 1.35;
    let name = if side == 0 { "arm_l" } else { "arm_r" };
    let box_ = rect_of(
        sx - hand / 2.0,
        sy - g.arm_w / 2.0,
        hand,
        g.arm_len + g.arm_w / 2.0 + hand / 2.0,
    );
    let rest = if side == 0 { 6.0 } else { -6.0 };
    let mut grp = Grp::new(k, name, box_).pivot(sx, sy).rotated(rest);
    let sleeve = rounded(
        rect_of(sx - g.arm_w / 2.0, sy - g.arm_w / 2.0, g.arm_w, g.arm_len),
        g.arm_w / 2.0,
        lit(&pal.body, pal.body_c),
    );
    grp.add(sleeve.named(k, &format!("{name}_sleeve")));
    let hand_paint = if cfg.style == Style::Robot {
        lit(&pal.accent, pal.accent_c)
    } else {
        lit(&pal.skin, pal.skin_c)
    };
    grp.add(
        ellipse(centered(sx, sy + g.arm_len, hand, hand), hand_paint)
            .named(k, &format!("{name}_hand")),
    );
    grp
}

fn draw_head(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg) -> (Grp, Rest) {
    let h = g.head;
    let mut grp = Grp::new(k, "head", h).pivot(g.cx, h.y + h.height * 0.92);
    let mut rest = Rest::default();
    if !cfg.hat {
        draw_antennae(k, g, pal, cfg, &mut grp);
    }
    grp.add(head_shape(k, g, pal, cfg));
    if cfg.style == Style::Robot {
        let plate = rect_of(
            h.x + h.width * 0.09,
            h.y + h.height * 0.16,
            h.width * 0.82,
            h.height * 0.62,
        );
        grp.add(
            rounded(
                plate,
                h.height * 0.22,
                linear(90.0, &[(0.0, solid("#27324f")), (1.0, solid("#141a2e"))]),
            )
            .named(k, "face_plate"),
        );
    } else {
        let shine = rect_of(
            h.x + h.width * 0.13,
            h.y + h.height * 0.08,
            h.width * 0.3,
            h.height * 0.2,
        );
        grp.add(turned(ellipse(shine, glow(WHITE, 0.5)), -28.0).named(k, "head_shine"));
    }
    let eyes = faces::eyes_layout(g, cfg);
    for (i, expr) in cfg.faces.iter().enumerate() {
        let prefix = if i == 0 { "" } else { expr.name() };
        let (face, rig) = faces::draw_face(k, g, pal, cfg, &eyes, *expr, prefix);
        grp.add_group(face);
        rest.faces.push(rig);
    }
    if cfg.glasses {
        grp.add_group(draw_glasses(k, g, pal, &eyes));
    }
    if cfg.hat {
        grp.add_group(draw_hat(k, g, pal));
    }
    (grp, rest)
}

fn head_shape(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg) -> NewNode {
    let h = g.head;
    let fill = lit(&pal.skin, pal.skin_c);
    match cfg.style {
        Style::Robot => rounded(h, h.height * 0.32, fill),
        _ => path(h, blob_d(h.width, h.height, g.form.head_widest), Some(fill)),
    }
    .named(k, "head_shape")
}

pub(super) fn shade_rgba(c: Rgba) -> Rgba {
    parse(&mix(c, BLACK, 0.65))
}

pub(super) fn radial_glow(c: Rgba) -> Paint {
    super::art::radial(
        0.5,
        0.5,
        0.55,
        &[(0.0, tint(c, 0.75)), (1.0, Paint::Color(c))],
    )
}

fn draw_glasses(k: &Kit, g: &Geo, pal: &Pal, e: &Eyes) -> Grp {
    let r = e.ew * 0.95;
    let box_ = rect_of(
        g.cx + e.dx - e.gap - r,
        e.cy - r,
        2.0 * e.gap + 2.0 * r,
        2.0 * r,
    );
    let mut grp = Grp::new(k, "glasses", box_);
    let frame_w = (g.head.width * 0.028).max(1.5);
    for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
        let cx = g.cx + e.dx + sign * e.gap;
        let mut ring = ellipse(centered(cx, e.cy, 2.0 * r, 2.0 * r), glow(WHITE, 0.2));
        ring.stroke = Some(pen(pal.ink.clone(), frame_w));
        grp.add(ring.named(k, &format!("lens_{i}")));
    }
    let bridge = rect_of(
        g.cx + e.dx - e.gap + r,
        e.cy - frame_w,
        2.0 * e.gap - 2.0 * r,
        frame_w * 2.0,
    );
    grp.add(rounded(bridge, frame_w, pal.ink.clone()).named(k, "bridge"));
    grp
}

fn draw_hat(k: &Kit, g: &Geo, pal: &Pal) -> Grp {
    let h = g.head;
    let w = h.width * 0.88;
    let dome_h = h.height * 0.44;
    let base = h.y + h.height * 0.26;
    let top = base - dome_h;
    let pomp = h.width * 0.075;
    let box_ = rect_of(
        g.cx - w / 2.0,
        top - pomp * 1.6,
        w,
        dome_h + pomp * 1.6 + h.height * 0.06,
    );
    let mut hat = Grp::new(k, "hat", box_).pivot(g.cx, base).rotated(-5.0);
    let dome = rect_of(g.cx - w / 2.0, top, w, dome_h);
    hat.add(
        path(
            dome,
            dome_d(w, dome_h),
            Some(lit(&pal.accent, pal.accent_c)),
        )
        .named(k, "hat_dome"),
    );
    let band = rect_of(
        g.cx - w * 0.54,
        base - h.height * 0.05,
        w * 1.08,
        h.height * 0.12,
    );
    hat.add(rounded(band, h.height * 0.06, shade(pal.accent_c, 0.22)).named(k, "hat_band"));
    hat.add(
        ellipse(
            centered(g.cx, top - pomp * 0.4, pomp * 2.0, pomp * 2.0),
            tint(pal.accent_c, 0.7),
        )
        .named(k, "hat_pompom"),
    );
    hat
}

fn draw_antennae(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg, head: &mut Grp) {
    match cfg.style {
        Style::Creature => {
            for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
                head.add_group(antenna(
                    k,
                    g,
                    pal,
                    sign,
                    if i == 0 { "antenna_l" } else { "antenna_r" },
                    1.0,
                ));
            }
        }
        Style::Robot => head.add_group(antenna(k, g, pal, 0.0, "antenna", 0.62)),
        _ => {}
    }
}

/// A stalk from the top of the head with a glowing bulb; `lean` is -1, 0 or 1 (which way it bends).
fn antenna(k: &Kit, g: &Geo, pal: &Pal, lean: f64, name: &str, reach: f64) -> Grp {
    let h = g.head;
    let rise = g.form.crown * g.s * 0.9 * reach;
    let (bx, by) = (g.cx + lean * h.width * 0.2, h.y + h.height * 0.08);
    let (tx, ty) = (bx + lean * h.width * 0.12, by - rise);
    let bulb = h.width * 0.075;
    let box_ = rect_of(
        bx.min(tx) - bulb,
        ty - bulb,
        (tx - bx).abs() + 2.0 * bulb,
        by - ty + bulb,
    );
    let mut grp = Grp::new(k, name, box_).pivot(bx, by);
    let stalk = rect_of(
        bx.min(tx) - bulb * 0.2,
        ty,
        (tx - bx).abs() + bulb * 0.4,
        by - ty,
    );
    let (sx, ex) = if tx >= bx {
        (bulb * 0.2, stalk.width - bulb * 0.2)
    } else {
        (stalk.width - bulb * 0.2, bulb * 0.2)
    };
    let d = format!(
        "M{sx:.2},{:.2} Q{:.2},{:.2} {ex:.2},0",
        stalk.height,
        sx,
        stalk.height * 0.4
    );
    let width = (h.width * 0.03).max(1.5);
    grp.add(line(stalk, d, shade(pal.skin_c, 0.35), width).named(k, &format!("{name}_stalk")));
    let glow_paint = super::art::radial(
        0.5,
        0.5,
        0.5,
        &[
            (0.0, tint(pal.accent_c, 0.6)),
            (0.6, Paint::Color(pal.accent_c)),
            (1.0, faded(pal.accent_c, 0.85)),
        ],
    );
    grp.add(
        ellipse(centered(tx, ty, bulb * 2.0, bulb * 2.0), glow_paint)
            .named(k, &format!("{name}_bulb")),
    );
    grp
}
