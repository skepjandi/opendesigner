//! Lower thirds and overlays: name plates, buttons, callouts, highlights, spotlights, a cursor and
//! a toast. They draw their own plates, so they read over any footage.

use od_core::animation::AnimProp;
use od_core::geometry::Rect;
use od_core::{Ease, TextAlign};

use super::BlockError;
use super::args::{Args, Param};
use super::kit::{
    Kit, Role, Style, Target, Ty, align_of, at, center, ellipse, frame_line, picture, rect, ring,
};
use super::registry::{Block, Category, Layer, Place};

fn lower_band(frame: Rect, margin: f64) -> Rect {
    at(
        margin,
        frame.height * 0.6,
        frame.width * 0.7,
        frame.height * 0.4 - margin,
    )
}

fn top_left_strip(frame: Rect, margin: f64) -> Rect {
    at(margin, margin, frame.width * 0.6, frame.height * 0.2)
}

fn top_band(frame: Rect, margin: f64) -> Rect {
    at(
        margin,
        margin,
        frame.width - 2.0 * margin,
        frame.height * 0.25,
    )
}

/// A rectangle `w` x `h` that is the target grown by `pad` and kept inside the frame.
fn around(target: Rect, pad: f64, frame: Rect) -> Rect {
    let x = (target.x - pad).max(0.0);
    let y = (target.y - pad).max(0.0);
    let right = (target.x + target.width + pad).min(frame.width);
    let bottom = (target.y + target.height + pad).min(frame.height);
    at(x, y, right - x, bottom - y)
}

/// The target region, or a default box in the middle of the frame.
fn target_or_default(k: &Kit, given: Option<Rect>) -> Rect {
    given.unwrap_or_else(|| {
        let (w, h) = (k.s(36.0).min(k.frame.width * 0.6), k.s(16.0));
        at((k.frame.width - w) / 2.0, (k.frame.height - h) / 2.0, w, h)
    })
}

const LOWER_THIRD: &[Param] = &[
    Param::text("name", "The name or main line.", "Alex Rivera", 50),
    Param::text(
        "title",
        "The role or secondary line (empty for none).",
        "Head of Design",
        70,
    ),
    Param::choice(
        "style",
        "bar: a dark plate with an accent bar; card: an accent plate.",
        "bar",
        &["bar", "card"],
    ),
];

fn lower_third(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let card = a.text("style") == "card";
    let pad = k.s(2.2);
    let bar = if card { 0.0 } else { k.s(0.9) };
    let max_w = k.b.width.min(k.frame.width * 0.62) - 2.0 * pad - bar;
    let plate_fill = k.paint(if card { Role::Accent } else { Role::Background });
    let ink = if card {
        k.ink_on(&plate_fill)
    } else {
        k.paint(Role::Foreground)
    };
    let second = if card {
        ink.clone()
    } else {
        k.paint(Role::Muted)
    };
    let size = k.s(4.0);
    let name_ty = Ty::of(size, 700.0, ink);
    let (name_ty, name_h) = k.fit(a.text("name"), &name_ty, max_w, size * 1.6, 1)?;
    let title = a.text("title");
    let size2 = k.s(2.8);
    let title_ty = Ty::of(size2, 400.0, second);
    let (title_ty, title_h) = if title.is_empty() {
        (title_ty, 0.0)
    } else {
        k.fit(title, &title_ty, max_w, size2 * 1.6, 1)?
    };
    let gap = if title.is_empty() { 0.0 } else { k.s(0.6) };
    let text_w = k
        .width_of(a.text("name"), &name_ty)
        .max(if title.is_empty() {
            0.0
        } else {
            k.width_of(title, &title_ty)
        })
        .min(max_w);
    let (pw, ph) = (text_w + 2.0 * pad + bar, name_h + gap + title_h + 2.0 * pad);
    let (x, y) = (k.b.x, k.b.y + k.b.height - ph);
    k.begin("content");
    k.add(
        "content/plate",
        rect(at(x, y, pw, ph), plate_fill)
            .radius(k.s(0.9))
            .alpha(if card { 1.0 } else { 0.9 }),
    );
    if !card {
        let accent = k.paint(Role::Accent);
        k.add(
            "content/bar",
            rect(at(x, y, bar, ph), accent).radius(k.s(0.45)),
        );
    }
    k.text(
        "content/name",
        a.text("name"),
        &name_ty,
        x + pad + bar,
        y + pad,
        text_w,
    );
    if !title.is_empty() {
        k.text(
            "content/title",
            title,
            &title_ty,
            x + pad + bar,
            y + pad + name_h + gap,
            text_w,
        );
    }
    k.end();
    let dist = pw + x;
    let t = k.part("content");
    k.keys(
        t.clone(),
        AnimProp::X,
        &[(0, -dist, Ease::Linear), (650, 0.0, Ease::EaseOut)],
    );
    k.keys(
        t,
        AnimProp::Opacity,
        &[(0, 0.0, Ease::Linear), (200, 1.0, Ease::EaseOut)],
    );
    k.fade("content/name", 300, 350);
    if !title.is_empty() {
        k.fade("content/title", 420, 350);
    }
    Ok(())
}

const NAME_TAG: &[Param] = &[
    Param::text("name", "The person's name.", "Alex Rivera", 40),
    Param::text("role", "Their role (empty for none).", "Host", 40),
];

fn name_tag(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let size = k.s(3.0);
    let d = size * 2.6;
    let pad = k.s(1.2);
    let max_w = k.b.width.min(k.frame.width * 0.5) - d - 3.0 * pad - pad;
    let fg = k.paint(Role::Foreground);
    let (name_ty, name_h) = k.fit(
        a.text("name"),
        &Ty::of(size, 700.0, fg),
        max_w,
        size * 1.6,
        1,
    )?;
    let role = a.text("role");
    let muted = k.paint(Role::Muted);
    let (role_ty, role_h) = if role.is_empty() {
        (Ty::of(size, 400.0, muted), 0.0)
    } else {
        k.fit(role, &Ty::of(k.s(2.4), 400.0, muted), max_w, size * 1.6, 1)?
    };
    let text_w = k
        .width_of(a.text("name"), &name_ty)
        .max(if role.is_empty() {
            0.0
        } else {
            k.width_of(role, &role_ty)
        })
        .min(max_w);
    let text_h = name_h + role_h;
    let h = d.max(text_h) + 2.0 * pad;
    let w = pad + d + pad + text_w + 2.0 * pad;
    let (x, y) = (k.b.x, k.b.y);
    let bg = k.paint(Role::Background);
    let accent = k.paint(Role::Accent);
    let ink = k.ink_on(&accent);
    k.add("plate", rect(at(x, y, w, h), bg).radius(h / 2.0).alpha(0.9));
    k.add(
        "avatar",
        ellipse(at(x + pad, y + (h - d) / 2.0, d, d), accent),
    );
    let initial: String = a
        .text("name")
        .chars()
        .take(1)
        .flat_map(char::to_uppercase)
        .collect();
    let initial_ty = Ty::of(size * 1.2, 700.0, ink).align(TextAlign::Center);
    let ih = k.measure(&initial, &initial_ty, d).1;
    k.text(
        "initial",
        &initial,
        &initial_ty,
        x + pad,
        y + (h - ih) / 2.0,
        d,
    );
    let tx = x + pad + d + pad;
    let ty0 = y + (h - text_h) / 2.0;
    k.text("name", a.text("name"), &name_ty, tx, ty0, text_w);
    if !role.is_empty() {
        k.text("role", role, &role_ty, tx, ty0 + name_h, text_w);
    }
    let dy = k.s(1.5);
    k.reveal(Target::Root, 0, 500, dy);
    Ok(())
}

const CTA: &[Param] = &[
    Param::text("label", "The button text.", "Get started", 28),
    Param::choice(
        "style",
        "filled or outline.",
        "filled",
        &["filled", "outline"],
    ),
    Param::choice(
        "align",
        "Where it sits in the box.",
        "center",
        &["left", "center", "right"],
    ),
];

fn cta_button(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let size = k.s(3.6);
    let accent = k.paint(Role::Accent);
    let filled = a.text("style") == "filled";
    let ink = if filled {
        k.ink_on(&accent)
    } else {
        accent.clone()
    };
    let ty = Ty::of(size, 700.0, ink);
    let w = k.width_of(a.text("label"), &ty);
    let (bw, bh) = (w + size * 3.0, (size * 1.2).ceil() + size * 1.4);
    if bw > k.b.width {
        return Err(BlockError::Fit(
            "the button label is wider than its box".into(),
        ));
    }
    let x = match align_of(a.text("align")) {
        TextAlign::Center => k.b.x + (k.b.width - bw) / 2.0,
        TextAlign::Right => k.b.x + k.b.width - bw,
        TextAlign::Left => k.b.x,
    };
    let y = k.b.y + k.b.height - bh;
    k.begin("button");
    let plate = if filled {
        rect(at(x, y, bw, bh), accent)
    } else {
        frame_line(at(x, y, bw, bh), accent, (size * 0.12).max(2.0))
    };
    k.add("button/plate", plate.radius(bh / 2.0));
    k.text(
        "button/label",
        a.text("label"),
        &ty,
        x + size * 1.5,
        y + size * 0.7,
        w,
    );
    k.end();
    let (t, dy) = (k.part("button"), k.s(3.0));
    k.keys(
        t.clone(),
        AnimProp::Y,
        &[(0, dy, Ease::Linear), (650, 0.0, Ease::Back)],
    );
    k.keys(
        t,
        AnimProp::Opacity,
        &[(0, 0.0, Ease::Linear), (250, 1.0, Ease::EaseOut)],
    );
    Ok(())
}

const TOOLTIP: &[Param] = &[
    Param::text("title", "A bold first line (empty for none).", "Tip", 40),
    Param::text(
        "text",
        "What to point out.",
        "This is where the magic happens.",
        120,
    ),
    Param::region(
        "target",
        "What it points at: a node id or {x, y, width, height}.",
    ),
    Param::choice(
        "side",
        "Where the card sits relative to the target.",
        "auto",
        &["auto", "above", "below", "left", "right"],
    ),
];

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Above,
    Below,
    Left,
    Right,
}

fn pick_side(want: &str, t: Rect, card: (f64, f64), reach: f64, frame: Rect, margin: f64) -> Side {
    match want {
        "above" => return Side::Above,
        "below" => return Side::Below,
        "left" => return Side::Left,
        "right" => return Side::Right,
        _ => {}
    }
    let fits = |side| match side {
        Side::Below => t.y + t.height + reach + card.1 + margin <= frame.height,
        Side::Above => t.y - reach - card.1 - margin >= 0.0,
        Side::Right => t.x + t.width + reach + card.0 + margin <= frame.width,
        Side::Left => t.x - reach - card.0 - margin >= 0.0,
    };
    [Side::Below, Side::Above, Side::Right, Side::Left]
        .into_iter()
        .find(|s| fits(*s))
        .unwrap_or(Side::Below)
}

fn tooltip_callout(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let target = target_or_default(k, a.region("target"));
    let pad = k.s(2.0);
    let card_w = k.s(46.0).min(k.frame.width - 2.0 * k.margin);
    let inner = card_w - 2.0 * pad;
    let fg = k.paint(Role::Foreground);
    let muted = k.paint(Role::Muted);
    let size = k.s(3.0);
    let title = a.text("title");
    let (t_ty, t_h) = if title.is_empty() {
        (Ty::of(size, 700.0, fg.clone()), 0.0)
    } else {
        k.fit(title, &Ty::of(size, 700.0, fg), inner, size * 1.6, 1)?
    };
    let body_size = k.s(2.6);
    let (b_ty, b_h) = k.fit(
        a.text("text"),
        &Ty::of(body_size, 400.0, muted).line(1.35),
        inner,
        body_size * 6.0,
        4,
    )?;
    let gap = if title.is_empty() { 0.0 } else { k.s(0.6) };
    let card_h = t_h + gap + b_h + 2.0 * pad;
    let arrow = k.s(5.0);
    let g = k.s(1.2);
    let side = pick_side(
        a.text("side"),
        target,
        (card_w, card_h),
        arrow + 2.0 * g,
        k.frame,
        k.margin,
    );
    let (tcx, tcy) = center(target);
    let (cx, cy) = match side {
        Side::Below => (
            tcx - card_w / 2.0,
            target.y + target.height + 2.0 * g + arrow,
        ),
        Side::Above => (tcx - card_w / 2.0, target.y - 2.0 * g - arrow - card_h),
        Side::Right => (
            target.x + target.width + 2.0 * g + arrow,
            tcy - card_h / 2.0,
        ),
        Side::Left => (target.x - 2.0 * g - arrow - card_w, tcy - card_h / 2.0),
    };
    let (cx, cy) = (
        cx.clamp(k.margin, (k.frame.width - k.margin - card_w).max(k.margin)),
        cy.clamp(k.margin, (k.frame.height - k.margin - card_h).max(k.margin)),
    );
    let (ax, ay, icon) = match side {
        Side::Below => (
            (tcx - arrow / 2.0).clamp(cx + pad, cx + card_w - pad - arrow),
            cy - g - arrow,
            "arrow-up",
        ),
        Side::Above => (
            (tcx - arrow / 2.0).clamp(cx + pad, cx + card_w - pad - arrow),
            cy + card_h + g,
            "arrow-down",
        ),
        Side::Right => (
            cx - g - arrow,
            (tcy - arrow / 2.0).clamp(cy + pad, cy + card_h - pad - arrow),
            "arrow-left",
        ),
        Side::Left => (
            cx + card_w + g,
            (tcy - arrow / 2.0).clamp(cy + pad, cy + card_h - pad - arrow),
            "arrow-right",
        ),
    };
    if ax < 0.0 || ay < 0.0 || ax + arrow > k.frame.width || ay + arrow > k.frame.height {
        return Err(BlockError::Fit(
            "there is no room for the callout beside that target: choose another side or target"
                .into(),
        ));
    }
    let bg = k.paint(Role::Background);
    let accent = k.paint(Role::Accent);
    k.begin("card");
    k.add(
        "card/plate",
        rect(at(cx, cy, card_w, card_h), bg)
            .radius(k.s(1.4))
            .outline(accent, 2.0),
    );
    if !title.is_empty() {
        k.text("card/title", title, &t_ty, cx + pad, cy + pad, inner);
    }
    k.text(
        "card/text",
        a.text("text"),
        &b_ty,
        cx + pad,
        cy + pad + t_h + gap,
        inner,
    );
    k.end();
    let file = k.icon(icon, Role::Accent, arrow);
    k.add("arrow", picture(at(ax, ay, arrow, arrow), file));
    let dy = match side {
        Side::Below => k.s(2.0),
        Side::Above => -k.s(2.0),
        _ => 0.0,
    };
    k.reveal_group("card", 0, 500, dy);
    k.fade("arrow", 250, 350);
    Ok(())
}

const HIGHLIGHT: &[Param] = &[
    Param::region(
        "target",
        "What to outline: a node id or {x, y, width, height} (default: the middle of the frame).",
    ),
    Param::number(
        "padding",
        "Space around the target, as a percent of the frame's shorter side.",
        1.2,
        0.0,
        10.0,
    ),
    Param::number(
        "stroke",
        "Outline width, as a percent of the shorter side.",
        0.45,
        0.1,
        2.0,
    ),
    Param::number(
        "fill",
        "How strongly the inside is tinted (0 for none).",
        0.12,
        0.0,
        0.5,
    ),
    Param::number(
        "roundness",
        "Corner radius, as a percent of the shorter side.",
        1.0,
        0.0,
        8.0,
    ),
];

fn highlight_box(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let target = target_or_default(k, a.region("target"));
    let hole = around(target, k.s(a.num("padding")), k.frame);
    let accent = k.paint(Role::Accent);
    let (radius, stroke) = (k.s(a.num("roundness")), k.s(a.num("stroke")).max(2.0));
    if a.num("fill") > 0.0 {
        k.add(
            "fill",
            rect(hole, accent.clone())
                .radius(radius)
                .alpha(a.num("fill")),
        );
        k.fade("fill", 400, 500);
    }
    k.add("outline", frame_line(hole, accent, stroke).radius(radius));
    let grow = k.s(2.5);
    let t = k.part("outline");
    let ease = Ease::Back;
    for (prop, to, from) in [
        (AnimProp::X, hole.x, hole.x - grow),
        (AnimProp::Y, hole.y, hole.y - grow),
        (AnimProp::Width, hole.width, hole.width + 2.0 * grow),
        (AnimProp::Height, hole.height, hole.height + 2.0 * grow),
    ] {
        k.keys(t.clone(), prop, &[(0, from, Ease::Linear), (550, to, ease)]);
    }
    k.fade("outline", 0, 250);
    Ok(())
}

const SPOTLIGHT: &[Param] = &[
    Param::region(
        "target",
        "What stays bright: a node id or {x, y, width, height} (default: the middle of the frame).",
    ),
    Param::number("dim", "How dark the rest becomes.", 0.72, 0.2, 0.95),
    Param::number(
        "padding",
        "Space around the target, as a percent of the shorter side.",
        1.5,
        0.0,
        10.0,
    ),
    Param::flag("outline", "An accent outline around the bright area.", true),
];

fn spotlight_dim(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let target = target_or_default(k, a.region("target"));
    let hole = around(target, k.s(a.num("padding")), k.frame);
    let f = k.frame;
    let bg = k.paint(Role::Background);
    let dim = a.num("dim");
    let bands = [
        ("top", at(0.0, 0.0, f.width, hole.y)),
        (
            "bottom",
            at(
                0.0,
                hole.y + hole.height,
                f.width,
                f.height - hole.y - hole.height,
            ),
        ),
        ("left", at(0.0, hole.y, hole.x, hole.height)),
        (
            "right",
            at(
                hole.x + hole.width,
                hole.y,
                f.width - hole.x - hole.width,
                hole.height,
            ),
        ),
    ];
    for (name, r) in bands {
        if r.width >= 1.0 && r.height >= 1.0 {
            k.add(name, rect(r, bg.clone()).alpha(dim));
        }
    }
    if a.flag("outline") {
        let accent = k.paint(Role::Accent);
        k.add(
            "outline",
            frame_line(hole, accent, k.s(0.35).max(2.0)).radius(k.s(0.6)),
        );
    }
    k.fade_root(0, 500);
    Ok(())
}

const CURSOR: &[Param] = &[
    Param::region(
        "to",
        "Where the click lands: a node id or {x, y} (default: the middle of the frame).",
    ),
    Param::region(
        "from",
        "Where the cursor starts (default: toward the lower right).",
    ),
    Param::count(
        "move_ms",
        "How long the cursor takes to arrive.",
        900,
        200,
        4000,
    ),
    Param::flag("ripple", "Rings spreading from the click.", true),
];

fn cursor_click(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let f = k.frame;
    let to = a
        .region("to")
        .map_or((f.width / 2.0, f.height / 2.0), center);
    let from = a
        .region("from")
        .map_or((f.width * 0.82, f.height * 0.86), center);
    let size = k.s(5.5);
    let tip = size * 4.0 / 24.0;
    let file = k.icon("mouse-pointer-2", Role::Foreground, size);
    let start = 300;
    let travel = a.num("move_ms") as u64;
    if a.flag("ripple") {
        let accent = k.paint(Role::Accent);
        let r1 = k.s(9.0);
        for (i, delay) in [(1, 0u64), (2, 160)] {
            let d = r1 * if i == 1 { 1.0 } else { 0.62 };
            let part = format!("ripple-{i}");
            k.add(
                &part,
                ring(
                    at(to.0 - d / 2.0, to.1 - d / 2.0, d, d),
                    accent.clone(),
                    k.s(0.4).max(2.0),
                )
                .alpha(0.0),
            );
            let t = k.part(&part);
            let (s0, s1) = (
                start + travel + 60 + delay,
                start + travel + 60 + delay + 700,
            );
            let small = d * 0.2;
            for (prop, v0, v1) in [
                (AnimProp::X, to.0 - small / 2.0, to.0 - d / 2.0),
                (AnimProp::Y, to.1 - small / 2.0, to.1 - d / 2.0),
                (AnimProp::Width, small, d),
                (AnimProp::Height, small, d),
            ] {
                k.keys(
                    t.clone(),
                    prop,
                    &[(s0, v0, Ease::Linear), (s1, v1, Ease::EaseOut)],
                );
            }
            k.keys(
                t,
                AnimProp::Opacity,
                &[
                    (s0, 0.0, Ease::Linear),
                    (s0 + 80, 0.9, Ease::EaseOut),
                    (s1, 0.0, Ease::EaseIn),
                ],
            );
        }
    }
    k.add(
        "cursor",
        picture(at(to.0 - tip, to.1 - tip, size, size), file),
    );
    let t = k.part("cursor");
    k.keys(
        t.clone(),
        AnimProp::X,
        &[
            (start, from.0 - tip, Ease::Linear),
            (start + travel, to.0 - tip, Ease::EaseInOut),
        ],
    );
    k.keys(
        t.clone(),
        AnimProp::Y,
        &[
            (start, from.1 - tip, Ease::Linear),
            (start + travel, to.1 - tip, Ease::EaseInOut),
        ],
    );
    k.keys(
        t,
        AnimProp::Opacity,
        &[(0, 0.0, Ease::Linear), (start, 1.0, Ease::EaseOut)],
    );
    Ok(())
}

const TOAST: &[Param] = &[
    Param::text("title", "The notification's title.", "Export finished", 40),
    Param::text(
        "message",
        "The notification's message.",
        "Your video is ready to share.",
        100,
    ),
    Param::icon(
        "icon",
        "A Lucide icon name (empty for a plain accent dot).",
        "bell",
    ),
];

fn notification_toast(k: &mut Kit, a: &Args) -> Result<(), BlockError> {
    let pad = k.s(2.0);
    let w = k.b.width.min(k.s(70.0));
    let d = k.s(7.0);
    let inner = w - 3.0 * pad - d;
    let fg = k.paint(Role::Foreground);
    let muted = k.paint(Role::Muted);
    let size = k.s(3.2);
    let (t_ty, t_h) = k.fit(
        a.text("title"),
        &Ty::of(size, 700.0, fg),
        inner,
        size * 1.6,
        1,
    )?;
    let msize = k.s(2.6);
    let (m_ty, m_h) = k.fit(
        a.text("message"),
        &Ty::of(msize, 400.0, muted).line(1.3),
        inner,
        msize * 3.2,
        2,
    )?;
    let text_h = t_h + k.s(0.4) + m_h;
    let h = text_h.max(d) + 2.0 * pad;
    let x = k.b.x + (k.b.width - w) / 2.0;
    let y = k.b.y;
    let bg = k.paint(Role::Background);
    let accent = k.paint(Role::Accent);
    let line = k.paint(Role::Muted);
    k.begin("card");
    k.add(
        "card/plate",
        rect(at(x, y, w, h), bg).radius(k.s(1.6)).outline(line, 1.0),
    );
    let (bx, by) = (x + pad, y + (h - d) / 2.0);
    k.add(
        "card/badge",
        ellipse(at(bx, by, d, d), accent.clone()).alpha(0.22),
    );
    let icon = a.text("icon");
    if icon.is_empty() {
        let dot = d * 0.34;
        k.add(
            "card/dot",
            ellipse(
                at(bx + (d - dot) / 2.0, by + (d - dot) / 2.0, dot, dot),
                accent,
            ),
        );
    } else {
        let size = d * 0.56;
        let file = k.icon(icon, Role::Accent, size);
        k.add(
            "card/icon",
            picture(
                at(bx + (d - size) / 2.0, by + (d - size) / 2.0, size, size),
                file,
            ),
        );
    }
    let tx = x + 2.0 * pad + d;
    let ty0 = y + (h - text_h) / 2.0;
    k.text("card/title", a.text("title"), &t_ty, tx, ty0, inner);
    k.text(
        "card/message",
        a.text("message"),
        &m_ty,
        tx,
        ty0 + t_h + k.s(0.4),
        inner,
    );
    k.end();
    let t = k.part("card");
    let from = -(h + y);
    k.keys(
        t.clone(),
        AnimProp::Y,
        &[(0, from, Ease::Linear), (600, 0.0, Ease::Back)],
    );
    k.keys(
        t,
        AnimProp::Opacity,
        &[(0, 0.0, Ease::Linear), (200, 1.0, Ease::EaseOut)],
    );
    Ok(())
}

pub const BLOCKS: &[Block] = &[
    Block {
        id: "lower_third",
        category: Category::Overlays,
        about: "A name plate for the lower left: a name and role on a dark plate with an accent bar, or on an accent card. Slides in from the left.",
        tags: &[
            "lower third",
            "name",
            "title",
            "speaker",
            "interview",
            "plate",
            "caption",
        ],
        params: LOWER_THIRD,
        place: Place::Custom(lower_band),
        layer: Layer::Front,
        build: lower_third,
    },
    Block {
        id: "name_tag",
        category: Category::Overlays,
        about: "A compact pill with an initial, a name and a role, for the top corner of a talking head or a call.",
        tags: &["name", "tag", "speaker", "avatar", "pill", "chip", "person"],
        params: NAME_TAG,
        place: Place::Custom(top_left_strip),
        layer: Layer::Front,
        build: name_tag,
    },
    Block {
        id: "cta_button",
        category: Category::Overlays,
        about: "A call-to-action button sized to its label, filled or outlined, that pops up with a spring.",
        tags: &[
            "cta",
            "button",
            "call to action",
            "download",
            "sign up",
            "link",
        ],
        params: CTA,
        place: Place::Safe,
        layer: Layer::Front,
        build: cta_button,
    },
    Block {
        id: "tooltip_callout",
        category: Category::Overlays,
        about: "A callout card with an arrow pointing at a node or rectangle, placed on the side with room.",
        tags: &[
            "tooltip",
            "callout",
            "annotation",
            "arrow",
            "pointer",
            "explain",
            "point",
        ],
        params: TOOLTIP,
        place: Place::Frame,
        layer: Layer::Front,
        build: tooltip_callout,
    },
    Block {
        id: "highlight_box",
        category: Category::Overlays,
        about: "An accent outline that snaps around a region with a small overshoot, with an optional soft tint inside.",
        tags: &[
            "highlight",
            "outline",
            "box",
            "focus",
            "emphasis",
            "region",
            "rectangle",
        ],
        params: HIGHLIGHT,
        place: Place::Frame,
        layer: Layer::Front,
        build: highlight_box,
    },
    Block {
        id: "spotlight_dim",
        category: Category::Overlays,
        about: "Dims everything except a region so the eye goes there, with an accent outline around the bright area.",
        tags: &[
            "spotlight",
            "dim",
            "focus",
            "darken",
            "vignette",
            "attention",
            "region",
        ],
        params: SPOTLIGHT,
        place: Place::Frame,
        layer: Layer::Front,
        build: spotlight_dim,
    },
    Block {
        id: "cursor_click",
        category: Category::Overlays,
        about: "A mouse cursor that glides to a point and clicks, with rings spreading from the click. Shows where to press in a walkthrough.",
        tags: &[
            "cursor",
            "click",
            "mouse",
            "pointer",
            "tap",
            "walkthrough",
            "demo",
        ],
        params: CURSOR,
        place: Place::Frame,
        layer: Layer::Front,
        build: cursor_click,
    },
    Block {
        id: "notification_toast",
        category: Category::Overlays,
        about: "A notification card with an icon, a title and a message that drops in from the top of the frame.",
        tags: &[
            "notification",
            "toast",
            "alert",
            "banner",
            "message",
            "push",
            "popup",
        ],
        params: TOAST,
        place: Place::Custom(top_band),
        layer: Layer::Front,
        build: notification_toast,
    },
];
