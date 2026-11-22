//! Numeric gates over a video's scene graph and tracks, the "scripts enforce" half of the review
//! loop (`docs/wiki/agent-integration.md#verify_motion`). Every gate answers `pass`, `fail` or
//! `unevaluated` with reasons; the agent judges the one contact sheet. A gate that could not
//! look at its input says so (`unevaluated`) and is never a pass. Every threshold is a fraction
//! of the frame height, and carries how far it can be trusted: a threshold that has not been
//! calibrated can only warn.

use std::collections::HashMap;

use od_core::{Document, Node, NodeId, Video};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::tools::{CallCtx, Output, ToolError, node_id, parse, tool};
use crate::video_export::Source;

pub mod geometry;
pub mod motion_measure;
pub mod structure;
pub mod track_gate;

#[cfg(test)]
mod tests;

pub const NAMES: [&str; 3] = ["verify_motion", "scene_check", "hit_test"];

/// Where a gate stands. `Unevaluated` carries why nothing was measured; it is never a pass.
#[derive(Debug, Clone, PartialEq)]
pub enum GateStatus {
    Pass,
    Fail,
    Unevaluated(String),
}

impl GateStatus {
    pub fn label(&self) -> &'static str {
        match self {
            GateStatus::Pass => "pass",
            GateStatus::Fail => "fail",
            GateStatus::Unevaluated(_) => "unevaluated",
        }
    }

    pub fn is_pass(&self) -> bool {
        *self == GateStatus::Pass
    }
}

/// The worst of several statuses: a failure beats a gap in the evidence, which beats a pass.
/// Nothing checked at all is a gap, not a pass.
pub fn combine(statuses: impl IntoIterator<Item = GateStatus>) -> GateStatus {
    let (mut failed, mut gaps, mut any) = (false, Vec::new(), false);
    for s in statuses {
        any = true;
        match s {
            GateStatus::Pass => {}
            GateStatus::Fail => failed = true,
            GateStatus::Unevaluated(why) => gaps.push(why),
        }
    }
    if failed {
        GateStatus::Fail
    } else if !any {
        GateStatus::Unevaluated("nothing was checked".into())
    } else if gaps.is_empty() {
        GateStatus::Pass
    } else {
        GateStatus::Unevaluated(gaps.join("; "))
    }
}

/// How a threshold was obtained. `Uncalibrated` ones cannot fail a gate, only warn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Calibration {
    /// Measured on a labelled corpus of OpenDesigner videos (none exists yet).
    Measured,
    /// Taken from a source that measured it, or exact geometry that needs no calibration.
    Derived,
    Uncalibrated,
}

impl Calibration {
    pub fn label(self) -> &'static str {
        match self {
            Calibration::Measured => "measured",
            Calibration::Derived => "derived",
            Calibration::Uncalibrated => "uncalibrated",
        }
    }
}

/// One gate's verdict with the reasons behind it.
#[derive(Debug, Clone, PartialEq)]
pub struct Gate {
    pub code: String,
    pub calibration: Calibration,
    pub failures: Vec<String>,
    pub unknowns: Vec<String>,
    pub warnings: Vec<String>,
    pub data: Value,
}

impl Gate {
    pub fn new(code: &str, calibration: Calibration) -> Gate {
        Gate {
            code: code.to_string(),
            calibration,
            failures: Vec::new(),
            unknowns: Vec::new(),
            warnings: Vec::new(),
            data: Value::Null,
        }
    }

    pub fn passing(code: &str) -> Gate {
        Gate::new(code, Calibration::Derived)
    }

    pub fn failing(code: &str, reason: impl Into<String>) -> Gate {
        let mut g = Gate::new(code, Calibration::Derived);
        g.failures.push(reason.into());
        g
    }

    pub fn unevaluated(code: &str, reason: impl Into<String>) -> Gate {
        let mut g = Gate::new(code, Calibration::Derived);
        g.unknowns.push(reason.into());
        g
    }

    pub fn status(&self) -> GateStatus {
        if !self.failures.is_empty() {
            GateStatus::Fail
        } else if !self.unknowns.is_empty() {
            GateStatus::Unevaluated(self.unknowns.join("; "))
        } else {
            GateStatus::Pass
        }
    }

    /// Records a failure of a check calibrated as `calibration`: an uncalibrated check can only
    /// warn, and says so.
    pub fn check(&mut self, ok: bool, calibration: Calibration, message: impl FnOnce() -> String) {
        if ok {
            return;
        }
        if calibration == Calibration::Uncalibrated {
            self.warnings.push(format!(
                "{} (uncalibrated threshold: warning only)",
                message()
            ));
        } else {
            self.failures.push(message());
        }
    }

    pub fn reason(&self) -> String {
        match self.status() {
            GateStatus::Pass => String::new(),
            GateStatus::Fail => self.failures.join("; "),
            GateStatus::Unevaluated(why) => why,
        }
    }

    pub fn to_json(&self) -> Value {
        let status = self.status();
        let mut out = json!({
            "code": self.code,
            "status": status.label(),
            "reason": self.reason(),
            "calibration": self.calibration.label(),
            "warnings": self.warnings,
        });
        if !self.data.is_null() {
            out["data"] = self.data.clone();
        }
        out
    }
}

/// The video named, or the first.
pub fn pick_video<'a>(doc: &'a Document, id: Option<&str>) -> Result<&'a Video, ToolError> {
    match id {
        Some(id) => doc
            .video(&node_id(id)?)
            .ok_or_else(|| ToolError(format!("no video {id}; list_videos shows them"))),
        None => doc
            .videos
            .first()
            .ok_or_else(|| ToolError("this project has no video; create_video makes one".into())),
    }
}

/// The height every length threshold is a fraction of: the first scene's frame, in document
/// units (so a document drawn at twice the size reads the same), else the video's own height.
pub fn frame_height(doc: &Document, video: &Video) -> f64 {
    video
        .clips
        .iter()
        .filter_map(|c| c.frame.as_ref())
        .chain(video.overlays.iter().map(|o| &o.frame))
        .filter_map(|f| doc.node(f))
        .map(|f| f.height)
        .find(|h| *h > 0.0)
        .unwrap_or_else(|| f64::from(video.height.max(1)))
}

/// The frames the video shows and when, as `(frame, start_ms, end_ms)`.
pub fn shown_frames(video: &Video) -> Vec<(NodeId, u64, u64)> {
    let mut out: Vec<(NodeId, u64, u64)> = video
        .placed()
        .iter()
        .filter_map(|p| {
            let frame = video.clips.get(p.index)?.frame.clone()?;
            Some((frame, p.start_ms, p.end_ms))
        })
        .collect();
    out.extend(video.overlays.iter().map(|o| {
        (
            o.frame.clone(),
            o.start_ms,
            o.start_ms + u64::from(o.duration_ms),
        )
    }));
    out
}

/// The frame the video shows that holds `id`, with the times it is on screen.
pub fn frame_of(doc: &Document, video: &Video, id: &NodeId) -> Option<(NodeId, Vec<(u64, u64)>)> {
    let loc = doc.locate(id)?;
    let shown = shown_frames(video);
    let frame = shown.iter().find_map(|(f, _, _)| {
        let at = doc.locate(f)?;
        (at.page == loc.page && loc.path.starts_with(&at.path)).then(|| f.clone())
    })?;
    let windows = shown
        .iter()
        .filter(|(f, _, _)| *f == frame)
        .map(|(_, s, e)| (*s, *e))
        .collect();
    Some((frame, windows))
}

/// Documents at moments of a video, made once each however many gates ask.
pub struct Timeline<'a> {
    pub doc: &'a Document,
    pub video: &'a Video,
    cache: HashMap<u64, Option<Document>>,
}

impl<'a> Timeline<'a> {
    pub fn new(doc: &'a Document, video: &'a Video) -> Timeline<'a> {
        Timeline {
            doc,
            video,
            cache: HashMap::new(),
        }
    }

    /// The document as the renderer draws it at `t_ms` (`Document::at_time`).
    pub fn at(&mut self, t_ms: f64) -> &Document {
        let (doc, video) = (self.doc, self.video);
        self.cache
            .entry(t_ms.to_bits())
            .or_insert_with(|| doc.at_time(video, t_ms))
            .as_ref()
            .unwrap_or(doc)
    }
}

/// Sorted, de-duplicated times.
pub fn tidy_times(mut times: Vec<f64>) -> Vec<f64> {
    times.retain(|t| t.is_finite());
    times.sort_by(f64::total_cmp);
    times.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    times
}

/// `n` times spread evenly over `from..=to`.
pub fn spread(from: f64, to: f64, n: usize) -> Vec<f64> {
    let steps = n.saturating_sub(1).max(1) as f64;
    (0..n.max(1))
        .map(|i| from + (to - from) * i as f64 / steps)
        .collect()
}

/// The last moment of the video that is drawn (`duration - 1` ms, never below 0).
pub fn last_ms(video: &Video) -> f64 {
    video.duration_ms().saturating_sub(1) as f64
}

/// Every node below `node` (itself first), in document order.
pub(crate) fn subtree<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(node);
    for c in &node.children {
        subtree(c, out);
    }
}

pub fn definitions() -> Vec<Value> {
    vec![
        tool(
            "verify_motion",
            "Proves the tracks of a video play, in numbers, without changing anything. For every \
             animation track (and the camera's) it samples at least 5 moments (default 7, always \
             the start, the middle and the end) and compares the value the track says with the \
             value the renderer's pipeline draws (G1), checks the node exists, the property fits \
             its kind, the node is visible and its scene on screen while the track runs (finding \
             RV020 track-drives-nothing), that every value is finite and legal (G2), and that \
             looping tracks end where they start (G3: rotation 0.5 degrees, position 0.01 of \
             the frame height, scale and opacity 0.005). Each track reports id, node, prop, \
             samples, max_delta, status (pass, fail or unevaluated, never a silent pass) and \
             reasons. With classify it also measures each moving node in fractions of the frame \
             height (travel, rise, speed, rotation, scale, pose return) and names the motion \
             (idle, in-place, walk, run, dash, jump, leap); looping ambient motion must read as \
             idle. `gait` checks foot slide against the 0.01 limit. Call it after the motion \
             pass and fix every fail before review_video.",
            json!({
                "video": {"type": "string", "description": "Id of a video from list_videos; defaults to the first."},
                "samples": {"type": "integer", "minimum": 5, "maximum": 64, "description": "Moments sampled per track (default 7, at least 5)."},
                "classify": {"type": "boolean", "description": "Also measure and classify the motion of every animated node."},
                "declared_loops": {"type": "array", "description": "Loop declarations from the plan: {node, prop, loop: true|false|null}. A null loop is unknown: the track is treated as one-shot and the gate is unevaluated.",
                    "items": {"type": "object", "properties": {"node": {"type": "string"}, "prop": {"type": "string"}, "loop": {"type": ["boolean", "null"]}}, "required": ["node", "prop"], "additionalProperties": false}},
                "gait": {"type": "array", "description": "Foot-slide checks: {node (the foot), stance: [{from_ms, to_ms}], character_height (default the frame height), ground (a scrolling node whose motion is subtracted)}. A gait with no stance is unevaluated.",
                    "items": {"type": "object", "properties": {
                        "node": {"type": "string"},
                        "stance": {"type": "array", "items": {"type": "object", "properties": {"from_ms": {"type": "number"}, "to_ms": {"type": "number"}}, "required": ["from_ms", "to_ms"], "additionalProperties": false}},
                        "character_height": {"type": "number", "exclusiveMinimum": 0},
                        "ground": {"type": "string"}
                    }, "required": ["node"], "additionalProperties": false}},
                "expect": {"type": "array", "description": "Intended motion classes: {node, class}; the measured labels must include it. A class that names intent (strike, guard) is reported as inferred.",
                    "items": {"type": "object", "properties": {"node": {"type": "string"}, "class": {"type": "string"}}, "required": ["node", "class"], "additionalProperties": false}},
                "ambient": {"type": "array", "items": {"type": "string"}, "description": "Node ids that are ambient life and must read as idle (nodes whose every track loops are ambient already)."}
            }),
            &[],
            true,
        ),
        tool(
            "scene_check",
            "Geometry gates over a video's document, without rendering the whole video and without \
             changing anything. Run at the union of the keyframe times of each chain plus 12 even \
             moments. Each gate answers pass, fail or unevaluated with reasons: bleed (a layer \
             flagged bleed:true must cover the frame under every camera pan, zoom and turn, \
             scaled by its depth), attach (a child's box within gap_tolerance of its parent's), \
             chirality (left and right parts are a reflection, never a rotation, and the left \
             one is on the viewer's right when facing front), part_coverage (every layer_plan \
             entry is a node; none is fused; unowned nodes and unmapped details warn), \
             motion_roster (every layer that expects motion has a track), and freeze (nodes \
             frozen after the shapes pass have not changed). `freeze_snapshot` returns the \
             hashes to store; `footprint` counts the pixels a node really changes in the picture.",
            json!({
                "video": {"type": "string", "description": "Id of a video; defaults to the first."},
                "layer_plan": {"type": "array", "description": "The plan's layers: {id, node_ref (a node id or name), role, bleed, parent, attach: {gap_tolerance_px, contact}, side: l|r, side_source: reference|convention, facing: front|side, mirror_of, expects_motion, ignore}.",
                    "items": {"type": "object"}},
                "details": {"type": "array", "description": "The detail inventory: {id, maps_to: {type: node|track, ref}}; one that maps to nothing real warns.",
                    "items": {"type": "object"}},
                "camera": {"type": "object", "description": "Declared camera extremes, checked as extra poses: {x_max_px, y_max_px, zoom_min, zoom_max, rotation_max_deg}.",
                    "properties": {"x_max_px": {"type": "number"}, "y_max_px": {"type": "number"}, "zoom_min": {"type": "number"}, "zoom_max": {"type": "number"}, "rotation_max_deg": {"type": "number"}}, "additionalProperties": false},
                "freeze": {"type": "object", "description": "A snapshot from freeze_snapshot: {hashes, attributes}. Later passes may change nothing in it but nodes listed in added_by.",
                    "properties": {"hashes": {"type": "object"}, "attributes": {"type": "object"}}, "required": ["hashes"], "additionalProperties": false},
                "added_by": {"type": "object", "description": "Nodes a later pass added, by pass name: {node_id: pass}."},
                "freeze_snapshot": {"type": "boolean", "description": "Return the freeze hashes of the video's scenes instead of comparing."},
                "footprint": {"type": "object", "description": "Pixels node really changes at t_ms (the picture with and without it).",
                    "properties": {"node": {"type": "string"}, "t_ms": {"type": "number"}}, "required": ["node"], "additionalProperties": false}
            }),
            &[],
            true,
        ),
        tool(
            "hit_test",
            "The node ids under a point at a moment of a video, topmost first (the discriminating \
             test for 'what is covering this?'). The point is in the frame's own units (0,0 is \
             its top-left corner), as the design shows at that moment; a moving camera is not \
             applied.",
            json!({
                "video": {"type": "string", "description": "Id of a video; defaults to the first."},
                "x": {"type": "number"},
                "y": {"type": "number"},
                "t": {"type": "number", "minimum": 0, "description": "Moment in ms from the start of the video (default 0)."}
            }),
            &["x", "y"],
            true,
        ),
    ]
}

pub fn call(ctx: &mut CallCtx, name: &str, args: Value) -> Result<Output, ToolError> {
    let src = Source::of(ctx.ws);
    match name {
        "verify_motion" => Ok(Output::json(&verify_motion(&src, parse(args)?)?)),
        "scene_check" => Ok(Output::json(&scene_check(&src, parse(args)?)?)),
        "hit_test" => Ok(Output::json(&hit_test_tool(&src, parse(args)?)?)),
        other => Err(ToolError(format!("unknown tool {other:?}"))),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyArgs {
    #[serde(default)]
    pub video: Option<String>,
    #[serde(default)]
    pub samples: Option<usize>,
    #[serde(default)]
    pub classify: bool,
    #[serde(default)]
    pub declared_loops: Vec<track_gate::DeclaredLoop>,
    #[serde(default)]
    pub gait: Vec<motion_measure::GaitArg>,
    #[serde(default)]
    pub expect: Vec<motion_measure::ExpectArg>,
    #[serde(default)]
    pub ambient: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct CheckArgs {
    #[serde(default)]
    pub video: Option<String>,
    #[serde(default)]
    pub layer_plan: Option<Vec<structure::LayerEntry>>,
    #[serde(default)]
    pub details: Option<Vec<structure::Detail>>,
    #[serde(default)]
    pub camera: Option<geometry::CameraExtremes>,
    #[serde(default)]
    pub freeze: Option<structure::Freeze>,
    #[serde(default)]
    pub added_by: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    pub freeze_snapshot: bool,
    #[serde(default)]
    pub footprint: Option<geometry::FootprintArgs>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HitArgs {
    #[serde(default)]
    video: Option<String>,
    x: f64,
    y: f64,
    #[serde(default)]
    t: Option<f64>,
}

/// The `verify_motion` report.
pub fn verify_motion(src: &Source, a: VerifyArgs) -> Result<Value, ToolError> {
    let doc = &src.doc;
    let video = pick_video(doc, a.video.as_deref())?;
    let samples = a
        .samples
        .unwrap_or(track_gate::DEFAULT_SAMPLES)
        .clamp(track_gate::MIN_SAMPLES, track_gate::MAX_SAMPLES);
    let analysis = track_gate::analyze(doc, video, samples, &a.declared_loops);
    let mut gates: Vec<Gate> = analysis.gates().to_vec();
    let mut out = json!({
        "video": video.id.as_str(),
        "samples": samples,
        "tracks": analysis.tracks.iter().map(track_gate::TrackReport::to_json).collect::<Vec<_>>(),
    });
    if a.classify || !a.expect.is_empty() {
        let measured = motion_measure::measure_all(doc, video, &a.ambient, &a.expect);
        gates.push(measured.gate);
        out["motion"] = Value::Array(measured.entries);
    }
    if !a.gait.is_empty() {
        let gait = motion_measure::gait_gate(doc, video, &a.gait);
        out["gait"] = gait.data.clone();
        gates.push(gait);
    }
    let status = combine(gates.iter().map(Gate::status));
    out["status"] = json!(status.label());
    out["ready"] = json!(status.is_pass());
    out["gates"] = gates.iter().map(Gate::to_json).collect();
    let (hard, soft) = analysis.rv020();
    out["findings"] = json!({"RV020": {"errors": hard, "warnings": soft}});
    Ok(out)
}

/// The `scene_check` report.
pub fn scene_check(src: &Source, a: CheckArgs) -> Result<Value, ToolError> {
    let doc = &src.doc;
    let video = pick_video(doc, a.video.as_deref())?;
    let mut out = json!({"video": video.id.as_str()});
    if a.freeze_snapshot {
        out["freeze"] = structure::snapshot(doc, video, &a.added_by).to_json();
        return Ok(out);
    }
    if let Some(f) = &a.footprint {
        out["footprint"] = geometry::footprint(src, video, f)?;
    }
    let plan = a.layer_plan.as_deref();
    let mut gates = vec![
        geometry::bleed_gate(doc, video, plan, a.camera.as_ref()),
        geometry::attach_gate(doc, video, plan),
        structure::chirality_gate(doc, video, plan),
        structure::coverage_gate(doc, video, plan, a.details.as_deref()),
        structure::motion_roster_gate(doc, video, plan),
    ];
    gates.push(match &a.freeze {
        Some(f) => structure::freeze_gate(doc, video, f, &a.added_by),
        None => Gate::unevaluated("freeze", "no freeze snapshot was given to compare with"),
    });
    let status = combine(gates.iter().map(Gate::status));
    out["status"] = json!(status.label());
    out["ready"] = json!(status.is_pass());
    out["gates"] = gates.iter().map(Gate::to_json).collect();
    Ok(out)
}

fn hit_test_tool(src: &Source, a: HitArgs) -> Result<Value, ToolError> {
    let video = pick_video(&src.doc, a.video.as_deref())?;
    geometry::hit_list(&src.doc, video, a.x, a.y, a.t.unwrap_or(0.0))
}
