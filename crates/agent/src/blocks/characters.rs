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
use super::kit::{Kit, Role, at, mix};
use super::registry::{Block, Category, Layer, Place};

const STYLES: &[&str] = &["blob", "bean", "robot", "creature"];
const EXPRESSIONS: &[&str] = &["neutral", "happy", "surprised", "sad", "angry", "thinking"];
const FACING: &[&str] = &["auto", "front", "left", "right"];
const ACTIONS: &[&str] = &["idle", "wave", "talk", "walk", "jump", "point", "celebrate"];

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
        "The face: eyes, brows, mouth and cheeks.",
        "happy",
        EXPRESSIONS,
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
    Param::region("look_at", "A node id or {x, y} the pupils look toward."),
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Expr {
    Neutral,
    Happy,
    Surprised,
    Sad,
    Angry,
    Thinking,
}

/// What the face and accessories ask for.
pub struct Cfg {
    pub style: Style,
    pub expr: Expr,
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
    body: Paint,
    body_c: Rgba,
    skin: Paint,
    skin_c: Rgba,
    accent: Paint,
    accent_c: Rgba,
    ink: Paint,
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

fn solid_rgba(hex: &str) -> Rgba {
    Rgba::parse_hex(hex).unwrap_or(BLACK)
}

fn parse(hex: &str) -> Rgba {
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
            from_x to to_x, jump with squash and stretch, point at a target, celebrate.",
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

fn read_expr(a: &Args) -> Expr {
    match a.text("expression") {
        "happy" => Expr::Happy,
        "surprised" => Expr::Surprised,
        "sad" => Expr::Sad,
        "angry" => Expr::Angry,
        "thinking" => Expr::Thinking,
        _ => Expr::Neutral,
    }
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
    Cfg {
        style: read_style(a),
        expr: read_expr(a),
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
    rest.head_y = g.head.y - roy;
    rig.add_group(head);
    for side in 0..2 {
        rig.add_group(draw_arm(k, g, pal, cfg, side));
        rest.arm_y[side] = g.shoulder(side).1 - g.arm_w / 2.0 - roy;
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

/// The eyes' layout, shared by the eyes, brows, glasses and cheeks.
struct Eyes {
    cy: f64,
    gap: f64,
    ew: f64,
    eh: f64,
    dx: f64,
}

fn eyes_layout(g: &Geo, cfg: &Cfg) -> Eyes {
    let h = g.head;
    let robot = cfg.style == Style::Robot;
    let mut ew = h.width * if robot { 0.16 } else { 0.19 };
    if cfg.expr == Expr::Surprised {
        ew *= 1.2;
    }
    let eh =
        ew * if robot { 1.05 } else { 1.18 } * if cfg.expr == Expr::Angry { 0.86 } else { 1.0 };
    Eyes {
        cy: h.y + h.height * 0.47,
        gap: h.width * 0.2,
        ew,
        eh,
        dx: cfg.facing * h.width * 0.13,
    }
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
    let eyes = eyes_layout(g, cfg);
    if cfg.style != Style::Robot {
        draw_cheeks(k, g, pal, cfg, &eyes, &mut grp);
    }
    grp.add_group(draw_eyes(k, g, pal, cfg, &eyes, &mut rest));
    grp.add_group(draw_brows(k, g, pal, cfg, &eyes));
    grp.add_group(draw_mouth(k, g, pal, cfg));
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

fn draw_cheeks(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg, e: &Eyes, head: &mut Grp) {
    let alpha = match cfg.expr {
        Expr::Sad => 0.25,
        Expr::Happy => 0.7,
        _ => 0.5,
    };
    let blush = parse(&mix(pal.skin_c, solid_rgba("#ff4d6d"), 0.6));
    let (w, h) = (g.head.width * 0.15, g.head.width * 0.09);
    for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
        let cx = g.cx + e.dx + sign * g.head.width * 0.3;
        let cy = e.cy + g.head.height * 0.18;
        head.add(
            ellipse(centered(cx, cy, w, h), glow(blush, alpha)).named(k, &format!("cheek_{i}")),
        );
    }
}

/// How far (as a share of the room the pupil has) it sits off center for each expression.
fn base_gaze(expr: Expr) -> (f64, f64) {
    match expr {
        Expr::Happy => (0.0, 0.1),
        Expr::Sad => (0.0, 0.6),
        Expr::Angry => (0.0, 0.2),
        Expr::Thinking => (0.7, -0.7),
        _ => (0.0, 0.0),
    }
}

fn draw_eyes(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg, e: &Eyes, rest: &mut Rest) -> Grp {
    let box_ = rect_of(
        g.cx + e.dx - e.gap - e.ew / 2.0,
        e.cy - e.eh / 2.0,
        2.0 * e.gap + e.ew,
        e.eh,
    );
    let mut eyes = Grp::new(k, "eyes", box_);
    for (side, sign) in [(0usize, -1.0), (1usize, 1.0)] {
        let cx = g.cx + e.dx + sign * e.gap;
        let far = (cfg.facing * sign).is_sign_negative() && cfg.facing != 0.0;
        let (eye, pupil_rest) = draw_eye(k, g, pal, cfg, e, side, cx, far);
        rest.pupil[side] = pupil_rest;
        eyes.add_group(eye);
    }
    eyes
}

#[allow(clippy::too_many_arguments)]
fn draw_eye(
    k: &Kit,
    g: &Geo,
    pal: &Pal,
    cfg: &Cfg,
    e: &Eyes,
    side: usize,
    cx: f64,
    far: bool,
) -> (Grp, (f64, f64)) {
    let tag = if side == 0 { "l" } else { "r" };
    let robot = cfg.style == Style::Robot;
    let box_ = centered(cx, e.cy, e.ew, e.eh);
    let mut eye = Grp::new(k, &format!("eye_{tag}"), box_);
    if far {
        eye.node.scale_x = Some(0.82);
    }
    let sclera = if robot {
        radial_glow(pal.accent_c)
    } else {
        linear(90.0, &[(0.0, solid("#ffffff")), (1.0, solid("#dce4ef"))])
    };
    eye.add(ellipse(box_, sclera).named(k, &format!("sclera_{tag}")));
    let pscale = if cfg.expr == Expr::Surprised {
        0.62
    } else {
        0.8
    } * if robot { 0.7 } else { 1.0 };
    let pw = e.ew * 0.7 * pscale;
    let ph = pw * 1.1;
    let (room_x, room_y) = ((e.ew - pw) / 2.0 * 0.85, (e.eh - ph) / 2.0 * 0.85);
    let (gx, gy) = gaze(g, cfg, e, cx);
    let (px, py) = (cx + gx * room_x, e.cy + gy * room_y);
    let pupil_box = centered(px, py, pw, ph);
    let mut pupil = Grp::new(k, &format!("pupil_{tag}"), pupil_box);
    let ink = if robot {
        Paint::Color(shade_rgba(pal.accent_c))
    } else {
        pal.ink.clone()
    };
    pupil.add(ellipse(pupil_box, ink).named(k, &format!("pupil_shape_{tag}")));
    let glint = pw * 0.36;
    pupil.add(
        ellipse(
            centered(px + pw * 0.16, py - ph * 0.2, glint, glint),
            solid("#ffffff"),
        )
        .named(k, &format!("glint_{tag}")),
    );
    let (ox, oy) = (pupil_box.x - box_.x, pupil_box.y - box_.y);
    eye.add_group(pupil);
    (eye, (ox, oy))
}

fn shade_rgba(c: Rgba) -> Rgba {
    parse(&mix(c, BLACK, 0.65))
}

fn radial_glow(c: Rgba) -> Paint {
    super::art::radial(
        0.5,
        0.5,
        0.55,
        &[(0.0, tint(c, 0.75)), (1.0, Paint::Color(c))],
    )
}

/// The pupil's offset as shares of its room (-1 to 1 on each axis).
fn gaze(g: &Geo, cfg: &Cfg, e: &Eyes, eye_x: f64) -> (f64, f64) {
    let Some((tx, ty)) = cfg.look else {
        let (bx, by) = base_gaze(cfg.expr);
        return (bx + cfg.facing * 0.5, by);
    };
    let (dx, dy) = (tx - eye_x, ty - e.cy);
    let len = dx.hypot(dy).max(1.0);
    let reach = (len / (g.head.width * 1.5)).min(1.0);
    (dx / len * reach, dy / len * reach)
}

fn draw_brows(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg, e: &Eyes) -> Grp {
    let (lift, tilt) = match cfg.expr {
        Expr::Surprised => (e.ew * 0.55, 0.0),
        Expr::Sad => (e.ew * 0.25, -14.0),
        Expr::Angry => (-e.ew * 0.05, 20.0),
        Expr::Thinking => (e.ew * 0.3, -8.0),
        Expr::Happy => (e.ew * 0.25, -5.0),
        Expr::Neutral => (e.ew * 0.2, 0.0),
    };
    let bl = e.ew * 1.15;
    let bt = (e.ew * 0.2).max(1.5);
    let y = e.cy - e.eh / 2.0 - e.ew * 0.32 - lift;
    let box_ = rect_of(
        g.cx + e.dx - e.gap - bl / 2.0,
        y - bt,
        2.0 * e.gap + bl,
        bt * 2.0 + e.ew * 0.6,
    );
    let mut brows = Grp::new(k, "brows", box_);
    let hidden = cfg.style == Style::Robot || cfg.expr == Expr::Neutral;
    for (i, sign) in [-1.0, 1.0].into_iter().enumerate() {
        let cx = g.cx + e.dx + sign * e.gap;
        let raise = if cfg.expr == Expr::Thinking && i == 1 {
            e.ew * 0.35
        } else {
            0.0
        };
        let brow = rounded(centered(cx, y - raise, bl, bt), bt / 2.0, pal.ink.clone());
        brows.add(turned(brow, tilt * -sign).named(k, &format!("brow_{i}")));
    }
    if hidden { brows.opacity(0.0) } else { brows }
}

/// The mouth's rectangle and which of its two shapes shows.
fn draw_mouth(k: &Kit, g: &Geo, pal: &Pal, cfg: &Cfg) -> Grp {
    let h = g.head;
    let (mw, mh) = (h.width * 0.26, h.width * 0.17);
    let shift = if cfg.expr == Expr::Thinking {
        h.width * 0.07
    } else {
        0.0
    };
    let (mx, my) = (
        g.cx + cfg.facing * h.width * 0.13 + shift,
        h.y + h.height * 0.74,
    );
    let box_ = centered(mx, my, mw, mh);
    let ink = if cfg.style == Style::Robot {
        lit(&pal.accent, pal.accent_c)
    } else {
        pal.ink.clone()
    };
    let mut mouth = Grp::new(k, "mouth", box_);
    let open_visible = matches!(cfg.expr, Expr::Happy | Expr::Surprised);
    if !open_visible {
        mouth.add(lip(k, cfg, box_, ink.clone(), h.width * 0.03));
    }
    let mut open = Grp::new(k, "mouth_open", box_).pivot(mx, box_.y + mh * 0.1);
    for part in open_shape(k, cfg, box_, ink, pal) {
        open.add(part);
    }
    if !open_visible {
        open = open.opacity(0.0);
        open.node.scale_y = Some(0.3);
    }
    mouth.add_group(open);
    mouth
}

fn lip(k: &Kit, cfg: &Cfg, b: Rect, ink: Paint, width: f64) -> NewNode {
    let (w, h) = (b.width, b.height);
    let d = match cfg.expr {
        Expr::Sad => format!(
            "M0,{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.85,
            w / 2.0,
            h * 0.05,
            h * 0.85
        ),
        Expr::Angry => format!(
            "M0,{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.7,
            w / 2.0,
            h * 0.3,
            h * 0.7
        ),
        Expr::Thinking => format!(
            "M0,{:.2} Q{:.2},{:.2} {:.2},{:.2} T{w:.2},{:.2}",
            h * 0.55,
            w * 0.25,
            h * 0.15,
            w / 2.0,
            h * 0.55,
            h * 0.55
        ),
        _ => format!(
            "M0,{:.2} Q{:.2},{:.2} {w:.2},{:.2}",
            h * 0.3,
            w / 2.0,
            h * 1.0,
            h * 0.3
        ),
    };
    line(b, d, ink, width.max(1.5)).named(k, "lip")
}

fn open_shape(k: &Kit, cfg: &Cfg, b: Rect, ink: Paint, pal: &Pal) -> Vec<NewNode> {
    let (w, h) = (b.width, b.height);
    if cfg.expr == Expr::Surprised {
        let o = centered(b.x + w / 2.0, b.y + h / 2.0, w * 0.42, h * 0.8);
        return vec![ellipse(o, ink).named(k, "mouth_o")];
    }
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
    let tongue = mix(pal.skin_c, solid_rgba("#ff4d6d"), 0.75);
    let tongue_box = centered(b.x + w / 2.0, b.y + h * 0.74, w * 0.5, h * 0.3);
    vec![
        path(b, d, Some(ink)).named(k, "mouth_bowl"),
        ellipse(tongue_box, solid(&tongue)).named(k, "tongue"),
    ]
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
