//! The life of a `character`: idle motion that is always on (breathing, head bob, blinking, arm
//! sway, drifting pupils, antennae) and one action on top (wave, talk, walk, jump, point,
//! celebrate). Everything becomes ordinary keyframes; an action replaces the idle track of the
//! same part and property, so no property has two tracks.

use od_core::Ease;
use od_core::animation::{AnimProp, Repeat};
use serde_json::Value;

use super::BlockError;
use super::args::Args;
use super::art::{Key, Rng, Tracks, ms, sway};
use super::characters::{Cfg, Expr, Geo};
use super::kit::Kit;

const MAX_WALK_MS: u64 = 40_000;
const MIN_WALK_MS: u64 = 800;
const STRIDE_MS: f64 = 700.0;
const MAX_TALK_MS: u64 = 600_000;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Idle,
    Wave,
    Talk,
    Walk,
    Jump,
    Point,
    Celebrate,
}

/// Where parts rest, as the values their position tracks must start from (a part's `x` and `y`
/// are relative to its parent).
#[derive(Default)]
pub struct Rest {
    pub rig_x: f64,
    pub rig_y: f64,
    pub head_y: f64,
    pub arm_y: [f64; 2],
    pub pupil: [(f64, f64); 2],
}

/// What the caller asked the character to do.
pub struct Script {
    pub action: Action,
    pub start: u64,
    pub end: u64,
    pub from_x: f64,
    pub to_x: f64,
    pub talk: Vec<(u64, u64)>,
    pub target: Option<(f64, f64)>,
    pub loops: u32,
    pub seed: u64,
    center: f64,
}

fn x_of(v: &Value, name: &str) -> Result<Option<f64>, BlockError> {
    match v {
        Value::Null => Ok(None),
        n => n
            .as_f64()
            .filter(|x| x.is_finite() && x.abs() <= 100_000.0)
            .map(Some)
            .ok_or_else(|| {
                BlockError::Param(format!("character.{name} must be a number (frame x)"))
            }),
    }
}

fn time_ranges(v: &Value) -> Result<Vec<(u64, u64)>, BlockError> {
    let bad = |why: &str| BlockError::Param(format!("character.talk: {why}"));
    let Value::Array(list) = v else {
        return if v.is_null() {
            Ok(Vec::new())
        } else {
            Err(bad("give a list of [start_ms, end_ms] ranges"))
        };
    };
    let mut out: Vec<(u64, u64)> = Vec::new();
    for item in list {
        let pair = item.as_array().filter(|p| p.len() == 2);
        let (Some(a), Some(b)) = (
            pair.and_then(|p| p[0].as_u64()),
            pair.and_then(|p| p[1].as_u64()),
        ) else {
            return Err(bad(
                "each range is [start_ms, end_ms] in whole milliseconds",
            ));
        };
        if b <= a || b > MAX_TALK_MS {
            return Err(bad("a range must end after it starts, inside 10 minutes"));
        }
        if out.last().is_some_and(|l| a <= l.1) {
            return Err(bad("ranges must be in time order and not overlap"));
        }
        out.push((a, b));
    }
    Ok(out)
}

fn action_of(name: &str) -> Action {
    match name {
        "wave" => Action::Wave,
        "talk" => Action::Talk,
        "walk" => Action::Walk,
        "jump" => Action::Jump,
        "point" => Action::Point,
        "celebrate" => Action::Celebrate,
        _ => Action::Idle,
    }
}

impl Script {
    pub fn read(a: &Args, k: &Kit) -> Result<Script, BlockError> {
        let action = action_of(a.text("action"));
        let (start, end) = (
            a.num("action_start_ms") as u64,
            a.num("action_end_ms") as u64,
        );
        let center = k.b.x + k.b.width / 2.0;
        let (from, to) = (
            x_of(a.data("from_x"), "from_x")?,
            x_of(a.data("to_x"), "to_x")?,
        );
        if action != Action::Walk && (from.is_some() || to.is_some()) {
            return Err(BlockError::Param(
                "character.from_x and to_x only apply to action walk".into(),
            ));
        }
        let target = a
            .region("target")
            .map(|r| (r.x + r.width / 2.0, r.y + r.height / 2.0));
        if action == Action::Point && target.is_none() {
            return Err(BlockError::Param(
                "character: action point needs a target (a node id or {x, y})".into(),
            ));
        }
        let mut talk = time_ranges(a.data("talk"))?;
        if action == Action::Talk && talk.is_empty() && end > start {
            talk.push((start, end));
        }
        let mut script = Script {
            action,
            start,
            end,
            from_x: from.unwrap_or(center - k.frame.width * 0.25),
            to_x: to.unwrap_or(center + k.frame.width * 0.25),
            talk,
            target,
            loops: a.num("loops") as u32,
            seed: a.num("seed") as u64,
            center,
        };
        if action == Action::Walk {
            if script.end == 0 {
                script.end = start + 4000;
            }
            let span = script.end.saturating_sub(start);
            if !(MIN_WALK_MS..=MAX_WALK_MS).contains(&span) {
                return Err(BlockError::Param(format!(
                    "character: a walk takes {MIN_WALK_MS} to {MAX_WALK_MS} ms (action_end_ms - action_start_ms)"
                )));
            }
        }
        Ok(script)
    }

    /// -1 walking left, 1 walking right, 0 otherwise.
    pub fn walk_direction(&self) -> f64 {
        if self.action == Action::Walk && (self.to_x - self.from_x).abs() > 1.0 {
            (self.to_x - self.from_x).signum()
        } else {
            0.0
        }
    }
}

pub fn animate(k: &mut Kit, s: &Script, g: &Geo, rest: &Rest, cfg: &Cfg) -> Result<(), BlockError> {
    let mut t = Tracks::default();
    idle(&mut t, s, g, rest, cfg);
    match s.action {
        Action::Wave => wave(&mut t, s),
        Action::Walk => walk(&mut t, s, g, rest),
        Action::Jump => jump(&mut t, s, g, rest, 1500, false),
        Action::Celebrate => jump(&mut t, s, g, rest, 1900, true),
        Action::Point => point(&mut t, s, g)?,
        Action::Talk | Action::Idle => {}
    }
    mouth(&mut t, s, cfg);
    t.emit(k)
}

fn idle(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest, cfg: &Cfg) {
    let mut rng = Rng::new(s.seed, 1);
    let breath = rng.range(1500.0, 2000.0);
    let amp = 0.028;
    let lift = g.body.height * amp;
    t.looping("body", AnimProp::ScaleY, sway(1.0, 1.0 + amp, breath));
    t.looping("body", AnimProp::ScaleX, sway(1.0, 0.992, breath));
    t.looping(
        "head",
        AnimProp::Y,
        sway(rest.head_y, rest.head_y - lift * 1.05, breath),
    );
    let tilt = rng.range(2200.0, 3200.0);
    t.looping("head", AnimProp::Rotation, sway(-1.6, 1.6, tilt));
    for (side, name) in ["arm_l", "arm_r"].into_iter().enumerate() {
        let sign = if side == 0 { 1.0 } else { -1.0 };
        let y = rest.arm_y[side];
        t.looping(name, AnimProp::Y, sway(y, y - lift * 0.78, breath));
        t.looping(
            name,
            AnimProp::Rotation,
            sway(sign * 3.0, sign * 10.0, rng.range(1900.0, 3100.0)),
        );
    }
    blink(t, &mut rng);
    drift(t, s, g, rest, cfg, &mut rng);
    antennae(t, cfg, &mut rng);
}

/// Both eyes close and open once per cycle, 2 to 4 seconds apart.
fn blink(t: &mut Tracks, rng: &mut Rng) {
    let p = rng.range(2000.0, 4000.0);
    let close = ms(p - 150.0);
    let keys: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (close.saturating_sub(70), 1.0, Ease::Linear),
        (close, 0.08, Ease::EaseIn),
        (close + 90, 1.0, Ease::EaseOut),
        (ms(p), 1.0, Ease::Linear),
    ];
    for name in ["eye_l", "eye_r"] {
        t.set(name, AnimProp::ScaleY, keys.clone(), Repeat::Loop);
    }
}

/// The pupils wander a little when nothing is being looked at.
fn drift(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest, cfg: &Cfg, rng: &mut Rng) {
    if cfg.look.is_some() || s.target.is_some() {
        return;
    }
    let span = rng.range(2600.0, 4200.0);
    let reach = g.s * 0.45;
    for (side, name) in ["pupil_l", "pupil_r"].into_iter().enumerate() {
        let (x, y) = rest.pupil[side];
        t.looping(name, AnimProp::X, sway(x - reach, x + reach, span));
        t.looping(name, AnimProp::Y, sway(y, y + reach * 0.3, span * 1.3));
    }
}

fn antennae(t: &mut Tracks, cfg: &Cfg, rng: &mut Rng) {
    use super::characters::Style;
    match cfg.style {
        Style::Creature if !cfg.hat => {
            for (name, sign) in [("antenna_l", -1.0), ("antenna_r", 1.0)] {
                let span = rng.range(1700.0, 2600.0);
                t.looping(
                    name,
                    AnimProp::Rotation,
                    sway(sign * 4.0, sign * 11.0, span),
                );
            }
        }
        Style::Robot if !cfg.hat => {
            t.looping(
                "antenna_bulb",
                AnimProp::Opacity,
                sway(0.55, 1.0, rng.range(600.0, 900.0)),
            );
        }
        _ => {}
    }
}

/// `cycle` (times from its own start, the first key at the rest value) played `loops` times from
/// `start`, preceded by a hold at `rest_value` until it starts.
fn play(start: u64, rest_value: f64, cycle: &[Key], period: u64, loops: u32) -> Vec<Key> {
    let mut keys = Vec::new();
    if start > 0 {
        keys.push((0, rest_value, Ease::Linear));
    }
    for i in 0..u64::from(loops) {
        for (j, (dt, v, e)) in cycle.iter().enumerate() {
            if i > 0 && j == 0 {
                continue;
            }
            keys.push((start + i * period + dt, *v, *e));
        }
    }
    keys
}

fn wave(t: &mut Tracks, s: &Script) {
    const REST: f64 = -6.0;
    let cycle: Vec<Key> = vec![
        (0, REST, Ease::Linear),
        (520, -150.0, Ease::Elastic),
        (800, -126.0, Ease::EaseInOut),
        (1060, -162.0, Ease::EaseInOut),
        (1320, -126.0, Ease::EaseInOut),
        (1580, -162.0, Ease::EaseInOut),
        (1840, -136.0, Ease::EaseInOut),
        (2340, REST, Ease::EaseOutBack),
        (2800, REST, Ease::Linear),
    ];
    t.set(
        "arm_r",
        AnimProp::Rotation,
        play(s.start, REST, &cycle, 2800, s.loops),
        Repeat::Once,
    );
}

fn walk(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest) {
    let (start, end) = (s.start, s.end);
    let half = STRIDE_MS / 2.0;
    let steps = ((end - start) as f64 / half).floor() as u64;
    let bob = g.s * 1.4;
    let at = |i: u64| start + ms(i as f64 * half);
    let swing = |rest_v: f64, amp: f64, phase: u64| -> Vec<Key> {
        let mut keys: Vec<Key> = Vec::new();
        if start > 0 {
            keys.push((0, rest_v, Ease::Linear));
        }
        keys.push((start, rest_v, Ease::EaseInOut));
        for i in 0..=steps {
            let sign = if (i + phase) % 2 == 0 { 1.0 } else { -1.0 };
            if at(i) + 120 < end {
                keys.push((at(i), rest_v + sign * amp, Ease::EaseInOut));
            }
        }
        keys.push((end, rest_v, Ease::EaseInOut));
        keys
    };
    t.set(
        "leg_l",
        AnimProp::Rotation,
        swing(0.0, 22.0, 1),
        Repeat::Once,
    );
    t.set(
        "leg_r",
        AnimProp::Rotation,
        swing(0.0, 22.0, 0),
        Repeat::Once,
    );
    t.set(
        "arm_l",
        AnimProp::Rotation,
        swing(6.0, 14.0, 0),
        Repeat::Once,
    );
    t.set(
        "arm_r",
        AnimProp::Rotation,
        swing(-6.0, 14.0, 1),
        Repeat::Once,
    );
    t.set("rig", AnimProp::Rotation, swing(0.0, 1.6, 0), Repeat::Once);
    let mut y: Vec<Key> = Vec::new();
    if start > 0 {
        y.push((0, rest.rig_y, Ease::Linear));
    }
    for i in 0..steps {
        y.push((at(i), rest.rig_y, Ease::EaseOut));
        y.push((at(i) + ms(half / 2.0), rest.rig_y - bob, Ease::EaseIn));
    }
    y.push((end, rest.rig_y, Ease::EaseOut));
    t.set("rig", AnimProp::Y, y, Repeat::Once);
    let from = s.from_x - s.center;
    let to = s.to_x - s.center;
    let mut x: Vec<Key> = vec![(0, from, Ease::Linear)];
    x.push((start, from, Ease::Linear));
    x.push((end, to, Ease::Linear));
    t.set("", AnimProp::X, x, Repeat::Once);
}

/// A hop with anticipation, stretch, squash on landing and an elastic settle; `arms` throws both
/// arms up for the celebration.
fn jump(t: &mut Tracks, s: &Script, g: &Geo, rest: &Rest, period: u64, arms: bool) {
    let h = g.height * 0.3;
    let y0 = rest.rig_y;
    let y: Vec<Key> = vec![
        (0, y0, Ease::Linear),
        (200, y0, Ease::EaseOut),
        (530, y0 - h, Ease::EaseIn),
        (860, y0, Ease::Linear),
        (period, y0, Ease::Linear),
    ];
    let sy: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (200, 0.84, Ease::EaseOut),
        (330, 1.14, Ease::EaseInOut),
        (530, 1.0, Ease::EaseIn),
        (800, 1.07, Ease::EaseOut),
        (880, 0.8, Ease::Linear),
        (1350, 1.0, Ease::Elastic),
        (period, 1.0, Ease::Linear),
    ];
    let sx: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (200, 1.1, Ease::EaseOut),
        (330, 0.92, Ease::EaseInOut),
        (530, 1.0, Ease::EaseIn),
        (800, 0.96, Ease::EaseOut),
        (880, 1.14, Ease::Linear),
        (1350, 1.0, Ease::Elastic),
        (period, 1.0, Ease::Linear),
    ];
    let shadow: Vec<Key> = vec![
        (0, 1.0, Ease::Linear),
        (200, 1.0, Ease::EaseOut),
        (530, 0.62, Ease::EaseIn),
        (860, 1.0, Ease::Linear),
        (period, 1.0, Ease::Linear),
    ];
    let loops = s.loops;
    t.set(
        "rig",
        AnimProp::Y,
        play(s.start, y0, &y, period, loops),
        Repeat::Once,
    );
    t.set(
        "rig",
        AnimProp::ScaleY,
        play(s.start, 1.0, &sy, period, loops),
        Repeat::Once,
    );
    t.set(
        "rig",
        AnimProp::ScaleX,
        play(s.start, 1.0, &sx, period, loops),
        Repeat::Once,
    );
    t.set(
        "shadow",
        AnimProp::Scale,
        play(s.start, 1.0, &shadow, period, loops),
        Repeat::Once,
    );
    if arms {
        throw_arms(t, s, period);
    } else {
        flail_arms(t, s, period);
    }
}

fn arm_cycle(rest: f64, up: f64, period: u64, flutter: &[(u64, f64)]) -> Vec<Key> {
    let mut keys: Vec<Key> = vec![(0, rest, Ease::Linear), (150, up, Ease::EaseOutBack)];
    keys.extend(flutter.iter().map(|(t, v)| (*t, *v, Ease::EaseInOut)));
    keys.push((period - 400, rest, Ease::EaseInOut));
    keys.push((period, rest, Ease::Linear));
    keys
}

fn throw_arms(t: &mut Tracks, s: &Script, period: u64) {
    for (name, rest, up) in [("arm_l", 6.0, 160.0), ("arm_r", -6.0, -160.0)] {
        let sign = if up > 0.0 { 1.0 } else { -1.0 };
        let flutter = [
            (550, up - sign * 16.0),
            (750, up + sign * 6.0),
            (950, up - sign * 16.0),
            (1150, up + sign * 6.0),
        ];
        let cycle = arm_cycle(rest, up, period, &flutter);
        t.set(
            name,
            AnimProp::Rotation,
            play(s.start, rest, &cycle, period, s.loops),
            Repeat::Once,
        );
    }
}

fn flail_arms(t: &mut Tracks, s: &Script, period: u64) {
    for (name, rest, out) in [("arm_l", 6.0, 42.0), ("arm_r", -6.0, -42.0)] {
        let cycle: Vec<Key> = vec![
            (0, rest, Ease::Linear),
            (200, rest * 0.5, Ease::EaseOut),
            (530, out, Ease::EaseOut),
            (900, out * 0.6, Ease::EaseIn),
            (1350, rest, Ease::Elastic),
            (period, rest, Ease::Linear),
        ];
        t.set(
            name,
            AnimProp::Rotation,
            play(s.start, rest, &cycle, period, s.loops),
            Repeat::Once,
        );
    }
}

fn point(t: &mut Tracks, s: &Script, g: &Geo) -> Result<(), BlockError> {
    let Some((tx, ty)) = s.target else {
        return Err(BlockError::Internal("point without a target".into()));
    };
    let side = usize::from(tx >= g.cx);
    let (sx, sy) = g.shoulder(side);
    let mut angle = (-(tx - sx)).atan2(ty - sy).to_degrees();
    let (name, rest) = if side == 0 {
        ("arm_l", 6.0)
    } else {
        ("arm_r", -6.0)
    };
    if side == 0 && angle < -30.0 {
        angle += 360.0;
    }
    if side == 1 && angle > 30.0 {
        angle -= 360.0;
    }
    let mut cycle: Vec<Key> = vec![(0, rest, Ease::Linear), (450, angle, Ease::EaseOutBack)];
    if s.end > s.start + 450 {
        let hold = s.end - s.start;
        cycle.push((hold, angle, Ease::Linear));
        cycle.push((hold + 500, rest, Ease::EaseOutBack));
    }
    let period = cycle.last().map_or(0, |k| k.0);
    t.set(
        name,
        AnimProp::Rotation,
        play(s.start, rest, &cycle, period, 1),
        Repeat::Once,
    );
    Ok(())
}

/// One syllable's worth of mouth: closed, open by `amount`, closed again.
fn syllables(rng: &mut Rng, from: u64, to: u64) -> Vec<(u64, f64)> {
    let mut out = vec![(from, 0.0)];
    let mut t = from as f64;
    while t + 260.0 < to as f64 {
        let (open, close, gap) = (
            rng.range(70.0, 110.0),
            rng.range(90.0, 170.0),
            rng.range(20.0, 90.0),
        );
        out.push((ms(t + open), rng.range(0.55, 1.0)));
        out.push((ms(t + open + close), 0.0));
        t += open + close + gap;
    }
    out.push((to, 0.0));
    out
}

fn mouth(t: &mut Tracks, s: &Script, cfg: &Cfg) {
    let forever = s.action == Action::Talk && s.talk.is_empty();
    if s.talk.is_empty() && !forever {
        return;
    }
    let mut rng = Rng::new(s.seed, 7);
    let mut beats: Vec<(u64, f64)> = Vec::new();
    if forever {
        beats = syllables(&mut rng, s.start, s.start + 2400);
    } else {
        for (a, b) in &s.talk {
            if beats.is_empty() && *a > 0 {
                beats.push((0, 0.0));
            }
            beats.extend(syllables(&mut rng, *a, *b));
        }
    }
    let visible = matches!(cfg.expr, Expr::Happy | Expr::Surprised);
    let closed_scale = if visible { 0.45 } else { 0.3 };
    let scale: Vec<Key> = beats
        .iter()
        .map(|(at, amount)| {
            (
                *at,
                closed_scale + (1.0 - closed_scale) * amount,
                Ease::EaseInOut,
            )
        })
        .collect();
    let opacity: Vec<Key> = beats
        .iter()
        .map(|(at, amount)| {
            (
                *at,
                if visible {
                    1.0
                } else {
                    f64::from(*amount > 0.0)
                },
                Ease::Linear,
            )
        })
        .collect();
    let mode = if forever { Repeat::Loop } else { Repeat::Once };
    t.set("mouth_open", AnimProp::ScaleY, scale, mode);
    if !visible {
        t.set("mouth_open", AnimProp::Opacity, opacity, mode);
    }
}
