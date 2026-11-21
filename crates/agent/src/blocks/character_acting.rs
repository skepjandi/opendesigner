//! The acting a `character` does on top of its idle life: expression changes that anticipate
//! (a squint, the swap, an overshoot) instead of snapping, timed looks where the eyes lead and
//! the face follows, and one-off reactions (take, shrug, nod, shake_head, laugh). All of it is
//! ordinary keyframes on parts nothing else animates, or on parts an action would take over.

use od_core::Ease;
use od_core::animation::{AnimProp, Repeat};
use serde_json::Value;

use super::BlockError;
use super::args::Args;
use super::art::{Key, Tracks};
use super::character_motion::{Action, Rest, Script, play};
use super::characters::Geo;
use super::faces::{self, Expr, part};

pub const MAX_CHANGES: usize = 40;
pub const MAX_FACES: usize = 6;
const FIRST_CHANGE_MS: u64 = 200;
const MIN_CHANGE_GAP_MS: u64 = 500;
const MAX_LOOKS: usize = 24;
const MIN_LOOK_GAP_MS: u64 = 400;
const MAX_TIME_MS: u64 = 600_000;
const LAUGH_MS: u64 = 1800;
const LAUGH_PERIOD_MS: u64 = 330;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reaction {
    None,
    Take,
    Shrug,
    Nod,
    ShakeHead,
    Laugh,
}

/// When the eyes go (ms from the block's start) and the frame point they go to.
pub type GazeKey = (u64, (f64, f64));

/// What the caller asked the character to act, beyond its action.
pub struct Acting {
    pub changes: Vec<(u64, Expr)>,
    pub looks: Vec<GazeKey>,
    pub reaction: Reaction,
    pub reaction_at: u64,
    pub reaction_ms: u64,
}

fn reaction_of(name: &str) -> Reaction {
    match name {
        "take" => Reaction::Take,
        "shrug" => Reaction::Shrug,
        "nod" => Reaction::Nod,
        "shake_head" => Reaction::ShakeHead,
        "laugh" => Reaction::Laugh,
        _ => Reaction::None,
    }
}

fn changes_of(v: &Value, first: Expr) -> Result<Vec<(u64, Expr)>, BlockError> {
    let bad = |why: String| BlockError::Param(format!("character.expression_changes: {why}"));
    let list = match v {
        Value::Null => return Ok(Vec::new()),
        Value::Array(list) => list,
        _ => return Err(bad("give a list of {at_ms, expression}".into())),
    };
    if list.len() > MAX_CHANGES {
        return Err(bad(format!("at most {MAX_CHANGES} changes")));
    }
    let mut out: Vec<(u64, Expr)> = Vec::new();
    let mut current = first;
    for item in list {
        let obj = item
            .as_object()
            .filter(|o| o.keys().all(|k| k == "at_ms" || k == "expression"))
            .ok_or_else(|| bad("each change is {at_ms, expression}".into()))?;
        let at = obj
            .get("at_ms")
            .and_then(Value::as_u64)
            .ok_or_else(|| bad("at_ms is a whole number of milliseconds".into()))?;
        let expr = obj
            .get("expression")
            .and_then(Value::as_str)
            .and_then(Expr::parse)
            .ok_or_else(|| bad(format!("expression is one of {}", faces::NAMES.join(", "))))?;
        let earliest = out
            .last()
            .map_or(FIRST_CHANGE_MS, |l| l.0 + MIN_CHANGE_GAP_MS);
        if at < earliest || at > MAX_TIME_MS {
            return Err(bad(format!(
                "at_ms {at} must be at least {earliest} (the first change at {FIRST_CHANGE_MS} or later, then {MIN_CHANGE_GAP_MS} ms apart or more, in time order) and within 10 minutes"
            )));
        }
        if expr == current {
            return Err(bad(format!(
                "at {at} the face is already {}: a change must be to a different expression",
                expr.name()
            )));
        }
        out.push((at, expr));
        current = expr;
    }
    if distinct(first, &out) > MAX_FACES {
        return Err(bad(format!("at most {MAX_FACES} different expressions")));
    }
    Ok(out)
}

fn distinct(first: Expr, changes: &[(u64, Expr)]) -> usize {
    let mut seen = vec![first];
    for (_, e) in changes {
        if !seen.contains(e) {
            seen.push(*e);
        }
    }
    seen.len()
}

fn looks_of(v: &Value) -> Result<Vec<GazeKey>, BlockError> {
    let bad = |why: String| BlockError::Param(format!("character.looks: {why}"));
    let list = match v {
        Value::Null => return Ok(Vec::new()),
        Value::Array(list) => list,
        _ => return Err(bad("give a list of {at_ms, x, y}".into())),
    };
    if list.len() > MAX_LOOKS {
        return Err(bad(format!("at most {MAX_LOOKS} looks")));
    }
    let mut out: Vec<GazeKey> = Vec::new();
    for item in list {
        let obj = item
            .as_object()
            .filter(|o| o.keys().all(|k| matches!(k.as_str(), "at_ms" | "x" | "y")))
            .ok_or_else(|| bad("each look is {at_ms, x, y}".into()))?;
        let num = |k: &str| {
            obj.get(k)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite() && n.abs() <= 100_000.0)
        };
        let (Some(at), Some(x), Some(y)) =
            (obj.get("at_ms").and_then(Value::as_u64), num("x"), num("y"))
        else {
            return Err(bad(
                "at_ms is whole milliseconds and x, y are frame coordinates".into(),
            ));
        };
        if at > MAX_TIME_MS || out.last().is_some_and(|l| at < l.0 + MIN_LOOK_GAP_MS) {
            return Err(bad(format!(
                "looks go in time order, {MIN_LOOK_GAP_MS} ms apart or more, within 10 minutes"
            )));
        }
        out.push((at, (x, y)));
    }
    Ok(out)
}

impl Acting {
    pub fn read(a: &Args, first: Expr, action: Action) -> Result<Acting, BlockError> {
        let reaction = reaction_of(a.text("reaction"));
        check_reaction(reaction, action)?;
        Ok(Acting {
            changes: changes_of(a.data("expression_changes"), first)?,
            looks: looks_of(a.data("looks"))?,
            reaction,
            reaction_at: a.num("reaction_at_ms") as u64,
            reaction_ms: match a.num("reaction_ms") as u64 {
                0 => LAUGH_MS,
                n => n,
            },
        })
    }

    /// The expressions that get a face: the first, then each one it changes to.
    pub fn faces(&self, first: Expr) -> Vec<Expr> {
        let mut out = vec![first];
        for (_, e) in &self.changes {
            if !out.contains(e) {
                out.push(*e);
            }
        }
        out
    }
}

/// A reaction that moves a part an action already owns is refused, with the reason.
fn check_reaction(reaction: Reaction, action: Action) -> Result<(), BlockError> {
    let body = matches!(action, Action::Walk | Action::Jump | Action::Celebrate);
    let arms = body || matches!(action, Action::Wave | Action::Point);
    let (clash, what) = match reaction {
        Reaction::Take | Reaction::Laugh => (body, "the body"),
        Reaction::Shrug => (arms, "the arms"),
        _ => (false, ""),
    };
    if clash {
        return Err(BlockError::Param(format!(
            "character: reaction {reaction:?} moves {what}, which this action already moves: pick another action or reaction"
        )));
    }
    Ok(())
}

pub fn animate(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest) {
    swaps(t, s, rest);
    looks(t, s, g, rest);
    react(t, s, g, rest);
}

/// The opacity and scale of each face over the video: the face that leaves squints, the one that
/// arrives starts small, overshoots and settles.
fn swaps(t: &mut Tracks, s: &Script, rest: &Rest) {
    if s.acting.changes.is_empty() {
        return;
    }
    let Some(first) = rest.faces.first() else {
        return;
    };
    let mut timeline = vec![(0u64, first.expr)];
    timeline.extend(s.acting.changes.iter().copied());
    for face in &rest.faces {
        let name = part(&face.prefix, "face");
        let shown = |on: bool| if on { 1.0 } else { 0.0 };
        let mut opacity: Vec<Key> = vec![(0, shown(timeline[0].1 == face.expr), Ease::Linear)];
        let mut sx: Vec<Key> = vec![(0, 1.0, Ease::Linear)];
        let mut sy = sx.clone();
        for pair in timeline.windows(2) {
            let (from, (at, to)) = (pair[0].1, pair[1]);
            if from == face.expr {
                leaving(at, &mut opacity, &mut sx, &mut sy);
            }
            if to == face.expr {
                arriving(at, &mut opacity, &mut sx, &mut sy);
            }
        }
        t.set(&name, AnimProp::Opacity, opacity, Repeat::Once);
        t.set(&name, AnimProp::ScaleX, sx, Repeat::Once);
        t.set(&name, AnimProp::ScaleY, sy, Repeat::Once);
    }
}

fn leaving(at: u64, opacity: &mut Vec<Key>, sx: &mut Vec<Key>, sy: &mut Vec<Key>) {
    opacity.push((at - 30, 1.0, Ease::Linear));
    opacity.push((at + 70, 0.0, Ease::EaseOut));
    sx.push((at - 150, 1.0, Ease::Linear));
    sx.push((at - 30, 1.05, Ease::EaseIn));
    sx.push((at + 120, 1.0, Ease::Linear));
    sy.push((at - 150, 1.0, Ease::Linear));
    sy.push((at - 30, 0.86, Ease::EaseIn));
    sy.push((at + 120, 1.0, Ease::Linear));
}

fn arriving(at: u64, opacity: &mut Vec<Key>, sx: &mut Vec<Key>, sy: &mut Vec<Key>) {
    opacity.push((at - 30, 0.0, Ease::Linear));
    opacity.push((at + 70, 1.0, Ease::EaseOut));
    sx.push((at - 30, 1.06, Ease::Linear));
    sx.push((at + 110, 0.93, Ease::EaseOut));
    sx.push((at + 300, 1.0, Ease::EaseInOut));
    sy.push((at - 30, 0.82, Ease::Linear));
    sy.push((at + 110, 1.14, Ease::EaseOut));
    sy.push((at + 300, 1.0, Ease::EaseInOut));
}

/// The pupils jump to each look at once and the face turns after them.
fn looks(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest) {
    if s.acting.looks.is_empty() {
        return;
    }
    for face in &rest.faces {
        for (side, tag) in ["l", "r"].into_iter().enumerate() {
            let rig = &face.pupils[side];
            let name = part(&face.prefix, &format!("pupil_{tag}"));
            let (mut xs, mut ys) = (
                vec![(0, rig.start.0, Ease::Linear)],
                vec![(0, rig.start.1, Ease::Linear)],
            );
            let mut here = rig.start;
            for (at, target) in &s.acting.looks {
                let there = rig.at(rig.toward(*target));
                eyes_move(*at, here, there, &mut xs, &mut ys);
                here = there;
            }
            t.set(&name, AnimProp::X, xs, Repeat::Once);
            t.set(&name, AnimProp::Y, ys, Repeat::Once);
        }
        let name = part(&face.prefix, "face");
        let (mut xs, mut ys) = (
            vec![(0, face.turn.0, Ease::Linear)],
            vec![(0, face.turn.1, Ease::Linear)],
        );
        let mut here = face.turn;
        for (at, target) in &s.acting.looks {
            let there = faces::turn_toward(g, *target);
            xs.push((at + 80, here.0, Ease::Linear));
            xs.push((at + 340, there.0, Ease::EaseOutBack));
            ys.push((at + 80, here.1, Ease::Linear));
            ys.push((at + 340, there.1, Ease::EaseOutBack));
            here = there;
        }
        t.set(&name, AnimProp::X, xs, Repeat::Once);
        t.set(&name, AnimProp::Y, ys, Repeat::Once);
    }
}

fn eyes_move(at: u64, here: (f64, f64), there: (f64, f64), xs: &mut Vec<Key>, ys: &mut Vec<Key>) {
    if at > 0 {
        xs.push((at, here.0, Ease::Linear));
        ys.push((at, here.1, Ease::Linear));
    }
    xs.push((at + 110, there.0, Ease::EaseOut));
    ys.push((at + 110, there.1, Ease::EaseOut));
}

fn react(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest) {
    let at = s.acting.reaction_at;
    match s.acting.reaction {
        Reaction::None => {}
        Reaction::Take => take(t, s, g, rest, at),
        Reaction::Shrug => shrug(t, s, g, rest, at),
        Reaction::Nod => nod(t, s, at),
        Reaction::ShakeHead => shake_head(t, s, g, rest, at),
        Reaction::Laugh => laugh(t, s, g, rest, at),
    }
}

fn cycle(t: &mut Tracks, part: &str, prop: AnimProp, at: u64, base: f64, keys: &[Key], loops: u32) {
    let period = keys.last().map_or(0, |k| k.0);
    t.set(
        part,
        prop,
        play(at, base, keys, period, loops),
        Repeat::Once,
    );
}

/// A double-take: a small startle, a look away, then the head snaps back with a squash, a stretch
/// and a settle.
fn take(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest, at: u64) {
    let away = g.s * 3.0;
    let sy: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (110, 1.05, Ease::EaseOut),
        (420, 1.0, Ease::EaseInOut),
        (780, 1.0, Ease::Linear),
        (870, 0.9, Ease::EaseIn),
        (1030, 1.16, Ease::EaseOut),
        (1190, 0.96, Ease::EaseInOut),
        (1400, 1.0, Ease::EaseOutBack),
        (1600, 1.0, Ease::Linear),
    ];
    let sx: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (110, 0.97, Ease::EaseOut),
        (420, 1.0, Ease::EaseInOut),
        (780, 1.0, Ease::Linear),
        (870, 1.07, Ease::EaseIn),
        (1030, 0.9, Ease::EaseOut),
        (1190, 1.03, Ease::EaseInOut),
        (1400, 1.0, Ease::EaseOutBack),
        (1600, 1.0, Ease::Linear),
    ];
    let head: Vec<Key> = vec![
        (0, rest.head_x, Ease::Linear),
        (300, rest.head_x - away, Ease::EaseOut),
        (780, rest.head_x - away, Ease::Linear),
        (1030, rest.head_x + away * 0.4, Ease::EaseOutBack),
        (1400, rest.head_x, Ease::EaseInOut),
        (1600, rest.head_x, Ease::Linear),
    ];
    cycle(t, "rig", AnimProp::ScaleY, at, 1.0, &sy, s.loops);
    cycle(t, "rig", AnimProp::ScaleX, at, 1.0, &sx, s.loops);
    cycle(t, "head", AnimProp::X, at, rest.head_x, &head, s.loops);
}

/// Shoulders up, palms out, the head sinking between them.
fn shrug(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest, at: u64) {
    for (side, (name, base, out)) in [("arm_l", 6.0, 38.0), ("arm_r", -6.0, -38.0)]
        .into_iter()
        .enumerate()
    {
        let y = rest.arm_y[side];
        let up = y - g.s * 2.4;
        let turn: Vec<Key> = vec![
            (0, base, Ease::Linear),
            (220, out, Ease::EaseOutBack),
            (900, out, Ease::Linear),
            (1250, base, Ease::EaseInOut),
            (1400, base, Ease::Linear),
        ];
        let lift: Vec<Key> = vec![
            (0, y, Ease::Linear),
            (200, up, Ease::EaseOut),
            (900, up, Ease::Linear),
            (1250, y, Ease::EaseInOut),
            (1400, y, Ease::Linear),
        ];
        cycle(t, name, AnimProp::Rotation, at, base, &turn, s.loops);
        cycle(t, name, AnimProp::Y, at, y, &lift, s.loops);
    }
    let sink: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (200, 0.93, Ease::EaseOut),
        (900, 0.93, Ease::Linear),
        (1250, 1.0, Ease::EaseInOut),
        (1400, 1.0, Ease::Linear),
    ];
    let widen: Vec<Key> = sink.iter().map(|&(a, v, e)| (a, 2.0 - v, e)).collect();
    cycle(t, "head", AnimProp::ScaleY, at, 1.0, &sink, s.loops);
    cycle(t, "head", AnimProp::ScaleX, at, 1.0, &widen, s.loops);
}

/// Two dips of the head from the neck.
fn nod(t: &mut Tracks, s: &Script, at: u64) {
    let dips: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (130, 0.9, Ease::EaseInOut),
        (300, 1.02, Ease::EaseOut),
        (430, 0.91, Ease::EaseInOut),
        (600, 1.0, Ease::EaseOut),
        (1000, 1.0, Ease::Linear),
    ];
    let widen: Vec<Key> = dips.iter().map(|&(a, v, e)| (a, 2.0 - v, e)).collect();
    cycle(t, "head", AnimProp::ScaleY, at, 1.0, &dips, s.loops);
    cycle(t, "head", AnimProp::ScaleX, at, 1.0, &widen, s.loops);
}

/// Three quick swings of the head that die away.
fn shake_head(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest, at: u64) {
    let (x, a) = (rest.head_x, g.s * 2.6);
    let swings: Vec<Key> = vec![
        (0, x, Ease::Linear),
        (110, x - a, Ease::EaseInOut),
        (330, x + a, Ease::EaseInOut),
        (550, x - a * 0.8, Ease::EaseInOut),
        (720, x + a * 0.4, Ease::EaseInOut),
        (900, x, Ease::EaseOut),
        (1100, x, Ease::Linear),
    ];
    cycle(t, "head", AnimProp::X, at, x, &swings, s.loops);
}

/// The body hops with each "ha" for `reaction_ms`.
fn laugh(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest, at: u64) {
    let hop = g.height * 0.022;
    let y0 = rest.rig_y;
    let beats = u32::try_from((s.acting.reaction_ms / LAUGH_PERIOD_MS).max(1)).unwrap_or(1);
    let y: Vec<Key> = vec![
        (0, y0, Ease::Linear),
        (110, y0 - hop, Ease::EaseOut),
        (220, y0, Ease::EaseIn),
        (LAUGH_PERIOD_MS, y0, Ease::Linear),
    ];
    let sy: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (110, 1.03, Ease::EaseOut),
        (220, 0.965, Ease::EaseIn),
        (LAUGH_PERIOD_MS, 1.0, Ease::EaseOut),
    ];
    let sx: Vec<Key> = sy.iter().map(|&(a, v, e)| (a, 2.0 - v, e)).collect();
    cycle(t, "rig", AnimProp::Y, at, y0, &y, beats);
    cycle(t, "rig", AnimProp::ScaleY, at, 1.0, &sy, beats);
    cycle(t, "rig", AnimProp::ScaleX, at, 1.0, &sx, beats);
}
