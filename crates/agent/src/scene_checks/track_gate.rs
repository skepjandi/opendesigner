//! G1, G2, G3 and the loop rule for the tracks of a video, and the reachability checks behind
//! finding RV020 (a track that drives nothing).
//!
//! Nothing about a track is believed until it has been evaluated at sampled moments and compared
//! with what the renderer draws: a track that exists is not a track that plays.

use od_core::{AnimProp, Animation, Document, Node, NodeId, Repeat, Rgba, Video};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{
    Calibration, Gate, GateStatus, Timeline, combine, frame_height, frame_of, last_ms, spread,
    tidy_times,
};

/// Moments sampled per track unless the caller chooses; fewer than the floor is a failure.
pub const DEFAULT_SAMPLES: usize = 7;
pub const MIN_SAMPLES: usize = 5;
pub const MAX_SAMPLES: usize = 64;
/// A track and what is drawn may differ by this relative to the value (2^-23, the float32 step).
pub const BINDING_EPSILON: f64 = 1.0 / 8_388_608.0;
/// The renderer's pipeline stores every animated number rounded to 4 decimals
/// (`od_core::limits::round4`); half a step is the most that rounding can move a value.
pub const PIPELINE_QUANTUM: f64 = 0.5e-4;
/// A loop ends within this of where it starts: degrees of rotation.
pub const LOOP_ROTATION_DEG: f64 = 0.5;
/// ... a position, as a fraction of the frame height.
pub const LOOP_TRANSLATION_H: f64 = 0.01;
/// ... a scale, an opacity or another 0-to-1 number (uncalibrated).
pub const LOOP_UNIT: f64 = 0.005;
/// Floating-point slack on a comparison against a limit that a value can sit exactly on.
const SLACK: f64 = 1e-9;

/// A loop declared by the plan: `loop: null` (or absent) is not known.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeclaredLoop {
    pub node: String,
    pub prop: AnimProp,
    #[serde(default, rename = "loop")]
    pub looping: Option<bool>,
}

/// What kind of number a property is, which sets how a loop's seam is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropClass {
    /// Degrees, compared modulo a turn.
    Rotation,
    /// Document units, compared as a fraction of the frame height.
    Length,
    /// A scale, an opacity, a 0-to-1 amount.
    Unit,
    Color,
}

pub fn prop_class(prop: AnimProp) -> PropClass {
    use AnimProp as P;
    match prop {
        P::Rotation
        | P::RotateX
        | P::RotateY
        | P::RotateZ
        | P::CameraRotation
        | P::LightAngle
        | P::Fov => PropClass::Rotation,
        P::X
        | P::Y
        | P::Width
        | P::Height
        | P::CornerRadius
        | P::StrokeWidth
        | P::Blur
        | P::ShadowDx
        | P::ShadowDy
        | P::ShadowBlur
        | P::CameraX
        | P::CameraY
        | P::GradeBlur => PropClass::Length,
        P::FillColor => PropClass::Color,
        _ => PropClass::Unit,
    }
}

/// How far a loop's last value is from its first, against what is allowed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seam {
    pub delta: f64,
    pub tolerance: f64,
    pub unit: &'static str,
    pub ok: bool,
    pub calibration: Calibration,
}

pub fn seam(prop: AnimProp, first: f64, last: f64, frame_h: f64) -> Seam {
    let (delta, tolerance, unit, calibration) = match prop_class(prop) {
        PropClass::Rotation => {
            let turn = (last - first).rem_euclid(360.0);
            (
                turn.min(360.0 - turn),
                LOOP_ROTATION_DEG,
                "deg",
                Calibration::Derived,
            )
        }
        PropClass::Length => (
            (last - first).abs() / frame_h.max(f64::MIN_POSITIVE),
            LOOP_TRANSLATION_H,
            "frame heights",
            Calibration::Derived,
        ),
        PropClass::Unit => (
            (last - first).abs(),
            LOOP_UNIT,
            "",
            Calibration::Uncalibrated,
        ),
        PropClass::Color => {
            let (a, b) = (Rgba::unpack(first), Rgba::unpack(last));
            let worst = [
                a.r.abs_diff(b.r),
                a.g.abs_diff(b.g),
                a.b.abs_diff(b.b),
                a.a.abs_diff(b.a),
            ]
            .into_iter()
            .max()
            .unwrap_or(0);
            (
                f64::from(worst) / 255.0,
                LOOP_UNIT,
                "of the channel range",
                Calibration::Uncalibrated,
            )
        }
    };
    Seam {
        delta,
        tolerance,
        unit,
        ok: delta <= tolerance * (1.0 + SLACK),
        calibration,
    }
}

/// Whether a clip loops: its last pose returns to its first (`pose_return_deg` the largest
/// rotation that differs, `return_h` how far the node ends from where it started in frame
/// heights). `None` when the pose return was not measured: "measured, does not loop" is not
/// "nobody looked", and an unknown loop is played once.
pub fn loop_rule(pose_return_deg: Option<f64>, return_h: Option<f64>) -> Option<bool> {
    let (pose, back) = (pose_return_deg?, return_h?);
    Some(pose <= LOOP_ROTATION_DEG * (1.0 + SLACK) && back <= LOOP_TRANSLATION_H * (1.0 + SLACK))
}

/// The rule this one replaced, kept so a test can pin that the two disagree: a clip that
/// neither travels nor rises can repeat seamlessly.
pub fn legacy_loop_rule(travel_h: f64, rise_h: f64) -> bool {
    travel_h < 0.02 && rise_h < 0.02
}

/// The most a track and the pipeline may differ at a value of magnitude `magnitude`.
pub fn binding_tolerance(magnitude: f64, quantum: f64) -> f64 {
    BINDING_EPSILON * magnitude.abs().max(1.0) + quantum
}

/// G1 on the numbers: enough samples, and none further from the pipeline than the tolerance.
pub fn judge_binding(
    samples: usize,
    max_delta: f64,
    magnitude: f64,
    quantum: f64,
) -> Result<(), String> {
    if samples < MIN_SAMPLES {
        return Err(format!(
            "under-sampled: {samples} of the {MIN_SAMPLES} samples every track needs"
        ));
    }
    let tolerance = binding_tolerance(magnitude, quantum);
    if max_delta.is_nan() || max_delta > tolerance {
        return Err(format!(
            "the pipeline draws a value {max_delta:e} from the track's (at most {tolerance:e})"
        ));
    }
    Ok(())
}

/// One track's verdict.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackReport {
    pub node: NodeId,
    pub prop: AnimProp,
    pub samples: usize,
    pub max_delta: f64,
    pub g1: Gate,
    pub g2: Gate,
    pub g3: Gate,
    /// Why the track drives nothing (RV020 errors).
    pub drives_nothing: Vec<String>,
    /// Every keyframe holds one value (an RV020 warning).
    pub all_equal: bool,
    pub seam: Option<(&'static str, Seam)>,
}

impl TrackReport {
    fn new(a: &Animation) -> TrackReport {
        TrackReport {
            node: a.node.clone(),
            prop: a.prop,
            samples: 0,
            max_delta: 0.0,
            g1: Gate::passing("G1"),
            g2: Gate::passing("G2"),
            g3: Gate::passing("G3"),
            drives_nothing: Vec::new(),
            all_equal: false,
            seam: None,
        }
    }

    pub fn id(&self) -> String {
        format!("{}:{}", self.node, self.prop.as_str())
    }

    pub fn status(&self) -> GateStatus {
        combine([self.g1.status(), self.g2.status(), self.g3.status()])
    }

    fn all_gates(&self) -> [&Gate; 3] {
        [&self.g1, &self.g2, &self.g3]
    }

    pub fn to_json(&self) -> Value {
        let status = self.status();
        let reasons: Vec<String> = self
            .all_gates()
            .iter()
            .flat_map(|g| g.failures.iter().chain(g.unknowns.iter()))
            .cloned()
            .collect();
        let warnings: Vec<String> = self
            .all_gates()
            .iter()
            .flat_map(|g| g.warnings.iter())
            .cloned()
            .collect();
        let mut out = json!({
            "id": self.id(),
            "node": self.node.as_str(),
            "prop": self.prop.as_str(),
            "samples": self.samples,
            "max_delta": self.max_delta,
            "status": status.label(),
            "reasons": reasons,
            "warnings": warnings,
        });
        if let Some((source, s)) = &self.seam {
            out["loop"] = json!({
                "source": source,
                "delta": s.delta,
                "tolerance": s.tolerance,
                "unit": s.unit,
                "ok": s.ok,
                "calibration": s.calibration.label(),
            });
        }
        out
    }
}

/// Every track of a video, judged.
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub tracks: Vec<TrackReport>,
}

impl Analysis {
    /// G1, G2 and G3 over all the tracks. A video with no tracks has nothing to prove, which is
    /// a gap in the evidence for G1 and G2, not a pass.
    pub fn gates(&self) -> [Gate; 3] {
        let merged = |code: &str, pick: fn(&TrackReport) -> &Gate, empty_is_gap: bool| {
            let mut gate = Gate::passing(code);
            if self.tracks.is_empty() && empty_is_gap {
                gate.unknowns
                    .push("the video has no animation tracks to check".into());
            }
            for t in &self.tracks {
                let g = pick(t);
                let tag = |m: &String| format!("{}: {m}", t.id());
                gate.failures.extend(g.failures.iter().map(tag));
                gate.unknowns.extend(g.unknowns.iter().map(tag));
                gate.warnings.extend(g.warnings.iter().map(tag));
            }
            gate.data = json!({"tracks": self.tracks.len()});
            gate
        };
        [
            merged("G1", |t| &t.g1, true),
            merged("G2", |t| &t.g2, true),
            merged("G3", |t| &t.g3, false),
        ]
    }

    /// RV020: the tracks that drive nothing (errors) and the ones whose keys never change
    /// (warnings).
    pub fn rv020(&self) -> (Vec<String>, Vec<String>) {
        let hard = self
            .tracks
            .iter()
            .filter(|t| !t.drives_nothing.is_empty())
            .map(|t| format!("{}: {}", t.id(), t.drives_nothing.join("; ")))
            .collect();
        let soft = self
            .tracks
            .iter()
            .filter(|t| t.all_equal && t.drives_nothing.is_empty())
            .map(|t| format!("{}: every keyframe holds the same value", t.id()))
            .collect();
        (hard, soft)
    }
}

/// Judges every animation track of `video` with `samples` moments each (at least 5).
pub fn analyze(
    doc: &Document,
    video: &Video,
    samples: usize,
    declared: &[DeclaredLoop],
) -> Analysis {
    let samples = samples.clamp(MIN_SAMPLES, MAX_SAMPLES);
    let mut timeline = Timeline::new(doc, video);
    let tracks = video
        .animations
        .iter()
        .map(|a| check_track(&mut timeline, a, samples, declared))
        .collect();
    Analysis { tracks }
}

fn check_track(
    tl: &mut Timeline,
    a: &Animation,
    samples: usize,
    declared: &[DeclaredLoop],
) -> TrackReport {
    let mut report = TrackReport::new(a);
    let (Some(first), Some(last)) = (a.keys.first(), a.keys.last()) else {
        report.drives_nothing.push("the track has no keys".into());
        report.g1.failures.push("the track has no keys".into());
        return report;
    };
    reach(tl, a, &mut report);
    bind(tl, a, samples, &mut report);
    loop_seam(tl, a, first.value, last.value, declared, &mut report);
    report
}

fn nothing(report: &mut TrackReport, why: String) {
    report.g1.failures.push(why.clone());
    report.drives_nothing.push(why);
}

/// The target exists, the property fits it, it is seen while the track plays.
fn reach(tl: &mut Timeline, a: &Animation, report: &mut TrackReport) {
    let (doc, video) = (tl.doc, tl.video);
    let (Some(first), Some(last)) = (a.keys.first(), a.keys.last()) else {
        return;
    };
    let duration = video.duration_ms();
    if a.keys.iter().all(|k| k.value == first.value) {
        report.all_equal = true;
        report
            .g1
            .warnings
            .push("every keyframe holds the same value, so the track moves nothing".into());
    }
    if first.at_ms >= duration {
        nothing(
            report,
            format!(
                "the track starts at {} ms, at or after the end of the video ({duration} ms)",
                first.at_ms
            ),
        );
    } else if last.at_ms > duration && a.repeat == Repeat::Once {
        report.g1.warnings.push(format!(
            "keys after {duration} ms, the end of the video, never play"
        ));
    }
    if a.prop.is_camera() {
        if a.node != video.id {
            nothing(
                report,
                format!(
                    "a camera property moves the video's camera: its node must be the video {}",
                    video.id
                ),
            );
        }
        return;
    }
    let Some(node) = doc.node(&a.node) else {
        nothing(report, format!("node {} does not exist", a.node));
        return;
    };
    if a.prop.read(node).is_none() {
        nothing(
            report,
            format!(
                "{} does not apply to a {} node (or the node lacks what it needs, such as a stroke)",
                a.prop.as_str(),
                node.kind.as_str()
            ),
        );
        return;
    }
    if first.at_ms < duration
        && let Err(why) = visible_while_running(tl, a, node)
    {
        nothing(report, why);
    }
}

/// The ancestors of `id` from the page root down to it.
fn chain<'a>(doc: &'a Document, id: &NodeId) -> Vec<&'a Node> {
    let Some(loc) = doc.locate(id) else {
        return Vec::new();
    };
    let Some(page) = doc.pages.get(loc.page) else {
        return Vec::new();
    };
    let mut out = vec![&page.root];
    let mut node = &page.root;
    for &i in &loc.path {
        let Some(child) = node.children.get(i) else {
            break;
        };
        out.push(child);
        node = child;
    }
    out
}

fn visible_while_running(tl: &mut Timeline, a: &Animation, node: &Node) -> Result<(), String> {
    let (doc, video) = (tl.doc, tl.video);
    let duration = video.duration_ms();
    let Some(first) = a.keys.first() else {
        return Ok(());
    };
    let Some((_, windows)) = frame_of(doc, video, &node.id) else {
        return Err(
            "the node is in no scene the video shows (no clip or overlay uses its frame)".into(),
        );
    };
    // A track holds its last value, so it reaches the node from its first key onwards: a scene
    // that starts later still shows what the track left.
    let on: Vec<(u64, u64)> = windows
        .iter()
        .map(|(s, e)| ((*s).max(first.at_ms), (*e).min(duration)))
        .filter(|(s, e)| s < e)
        .collect();
    if on.is_empty() {
        return Err(format!(
            "its scene is not on screen from the first key of the track ({} ms) to the end of the video",
            first.at_ms
        ));
    }
    if let Some(hidden) = chain(doc, &node.id).iter().find(|n| !n.visible) {
        return Err(format!(
            "{} is hidden (visible is off), so the track is never seen",
            hidden.id
        ));
    }
    let seen = on.iter().any(|(s, e)| {
        spread(*s as f64, (*e - 1) as f64, MIN_SAMPLES)
            .into_iter()
            .any(|t| {
                let at = tl.at(t);
                chain(at, &node.id)
                    .iter()
                    .map(|n| n.opacity)
                    .product::<f64>()
                    > 0.0
            })
    });
    if seen {
        Ok(())
    } else {
        Err("the node or an ancestor has opacity 0 whenever the track plays".into())
    }
}

/// What the track's property should show at `t_ms`: the track's value, or with an additive layer
/// over the node's own value the sum the pipeline draws.
fn expected_value(tl: &Timeline, a: &Animation, t_ms: f64) -> Option<f64> {
    a.value_at(t_ms)?;
    let base = if a.prop.is_camera() {
        let c = od_core::transform::Camera::default();
        match a.prop {
            AnimProp::CameraX => c.x,
            AnimProp::CameraY => c.y,
            AnimProp::CameraZoom => c.zoom,
            _ => c.rotation,
        }
    } else {
        a.prop.read(tl.doc.node(&a.node)?)?
    };
    Some(tl.video.animated_value(&a.node, a.prop, t_ms, base))
}

/// What the renderer's pipeline holds for the track's property at `t_ms`, and the most its
/// rounding can have moved it.
fn pipeline_value(tl: &mut Timeline, a: &Animation, t_ms: f64) -> Option<(f64, f64)> {
    if a.prop.is_camera() {
        let c = tl.video.camera_at(t_ms);
        let v = match a.prop {
            AnimProp::CameraX => c.x,
            AnimProp::CameraY => c.y,
            AnimProp::CameraZoom => c.zoom,
            _ => c.rotation,
        };
        return Some((v, 0.0));
    }
    let node = tl.at(t_ms).node(&a.node)?;
    Some((a.prop.read(node)?, PIPELINE_QUANTUM))
}

fn sample_times(a: &Animation, video: &Video, n: usize) -> Vec<f64> {
    let end = last_ms(video);
    let mut times = spread(0.0, end, n);
    if let (Some(first), Some(last)) = (a.keys.first(), a.keys.last()) {
        times.extend(spread(
            (first.at_ms as f64).min(end),
            (last.at_ms as f64).min(end),
            n,
        ));
    }
    tidy_times(times)
}

/// G1 (the track and the pipeline agree) and G2 (what is drawn is finite and legal), at the
/// sampled moments.
fn bind(tl: &mut Timeline, a: &Animation, samples: usize, report: &mut TrackReport) {
    let times = sample_times(a, tl.video, samples);
    report.samples = times.len();
    let mut max_delta = 0.0f64;
    let mut seen_unreachable = false;
    for &t in &times {
        let Some(tv) = expected_value(tl, a, t) else {
            continue;
        };
        let Some((pv, quantum)) = pipeline_value(tl, a, t) else {
            seen_unreachable = true;
            continue;
        };
        if !tv.is_finite() || !pv.is_finite() {
            report.g2.failures.push(format!(
                "{} is not a finite number at {t} ms (track {tv}, drawn {pv})",
                a.prop.as_str()
            ));
            continue;
        }
        let delta = (tv - pv).abs();
        max_delta = max_delta.max(delta);
        let tolerance = binding_tolerance(tv, quantum);
        if delta > tolerance * (1.0 + SLACK) {
            let why = format!(
                "at {t} ms the picture draws {pv} where the track says {tv} (over the tolerance {tolerance:e})"
            );
            report.g1.failures.push(why.clone());
            report.drives_nothing.push(why);
        }
        legal(tl, a, t, report);
    }
    report.max_delta = max_delta;
    let sampled = report.samples;
    if sampled < MIN_SAMPLES {
        report.g1.failures.push(format!(
            "under-sampled: {sampled} of the {MIN_SAMPLES} samples every track needs"
        ));
    }
    if seen_unreachable {
        nothing(
            report,
            "the renderer's pipeline holds no value for the track's property".into(),
        );
    }
}

fn legal(tl: &mut Timeline, a: &Animation, t: f64, report: &mut TrackReport) {
    if a.prop.is_camera() {
        return;
    }
    let Some(node) = tl.at(t).node(&a.node) else {
        return;
    };
    let mut why = Vec::new();
    if matches!(
        a.prop,
        AnimProp::Scale | AnimProp::ScaleX | AnimProp::ScaleY
    ) && node.scale_x * node.scale_y == 0.0
    {
        why.push(format!(
            "the scale product is 0 at {t} ms, so the node collapses"
        ));
    }
    let trim = matches!(a.prop, AnimProp::TrimStart | AnimProp::TrimEnd)
        && node.stroke.as_ref().is_some_and(|s| {
            !(0.0..=1.0).contains(&s.trim_start)
                || !(0.0..=1.0).contains(&s.trim_end)
                || s.trim_start > s.trim_end
        });
    if trim {
        why.push(format!(
            "the trim is not 0 <= trim_start <= trim_end <= 1 at {t} ms"
        ));
    }
    if a.prop == AnimProp::StrokeWidth && node.stroke.as_ref().is_some_and(|s| s.width < 0.0) {
        why.push(format!("the stroke width is negative at {t} ms"));
    }
    if matches!(a.prop, AnimProp::Width | AnimProp::Height)
        && (node.width < 0.0 || node.height < 0.0)
    {
        why.push(format!("the size is negative at {t} ms"));
    }
    for w in why {
        if !report.g2.failures.contains(&w) {
            report.g2.failures.push(w);
        }
    }
}

/// G3: a track that repeats, or is declared a loop, ends where it starts.
fn loop_seam(
    tl: &Timeline,
    a: &Animation,
    first: f64,
    last: f64,
    declared: &[DeclaredLoop],
    report: &mut TrackReport,
) {
    let declared = declared
        .iter()
        .find(|d| d.node == a.node.as_str() && d.prop == a.prop);
    let source = match (a.repeat, declared.and_then(|d| d.looping)) {
        (Repeat::Loop, _) => "repeat: loop",
        (Repeat::Pingpong, _) => "repeat: pingpong",
        (Repeat::Once, Some(true)) => "declared loop",
        (Repeat::Once, None) if declared.is_some() => {
            report.g3.unknowns.push(
                "loop is not known (null): treated as a one-shot track, so its seam was not checked"
                    .into(),
            );
            return;
        }
        _ => return,
    };
    if a.repeat == Repeat::Once {
        report.g3.failures.push(
            "declared as a loop but its repeat is `once`: it holds its last value instead of repeating".into(),
        );
    }
    let end = if a.repeat == Repeat::Pingpong {
        first
    } else {
        last
    };
    let s = seam(a.prop, first, end, frame_height(tl.doc, tl.video));
    let (id, unit) = (a.prop.as_str(), s.unit);
    report.g3.check(s.ok, s.calibration, || {
        format!(
            "{id} ends {:.4} {unit} from where it starts, over the limit {} {unit}, so the loop jumps when it wraps",
            s.delta, s.tolerance
        )
    });
    report.seam = Some((source, s));
}
