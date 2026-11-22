//! The review ledger: which passes are complete, whether the loop has stopped, what evidence is
//! missing and the exact next tool call. Everything is derived from `review_history`; chat memory
//! is never authority.

use std::path::Path;

use serde_json::{Value, json};
use thiserror::Error;

use super::features::{evaluate_features, targets_for};
use super::loop_policy::{
    Decision, Iteration, LoopLimits, LoopState, Policy, RunStatus, decide, loop_state,
};
use super::schema::{Action, Evidence, LAYER_SCORES, Review, SceneSpec, is_visual_pass};
use super::validate::validate;

pub const VISUAL_ACCEPTANCE_THRESHOLD: f64 = 0.7;
pub const MAX_REVIEWS: usize = 200;
const MAX_SUMMARY_CHARS: usize = 4_000;

#[derive(Debug, Error)]
pub enum RecordError {
    #[error("{0}")]
    Refused(String),
}

fn refuse<T>(message: impl Into<String>) -> Result<T, RecordError> {
    Err(RecordError::Refused(message.into()))
}

/// Why an evidence file does not count: it must be `exports/<name>.png` inside the project and a
/// plain file, never a link.
pub fn evidence_problem(dir: &Path, relative: &str) -> Option<String> {
    let Some(name) = relative.strip_prefix("exports/") else {
        return Some(format!("{relative:?} must be exports/<name>.png"));
    };
    if crate::export::export_name(name).is_err() {
        return Some(format!("{relative:?} must be exports/<name>.png"));
    }
    match std::fs::symlink_metadata(dir.join(relative)) {
        Ok(m) if m.is_file() => None,
        Ok(_) => Some(format!("{relative} is not a plain file")),
        Err(_) => Some(format!("{relative} does not exist in the project")),
    }
}

fn evidence_problems(dir: &Path, evidence: &Evidence) -> Vec<String> {
    let mut out = Vec::new();
    match &evidence.contact_sheet {
        Some(sheet) => out.extend(evidence_problem(dir, sheet)),
        None => out.push("evidence.contact_sheet is missing".into()),
    }
    if evidence.frames.is_empty() {
        out.push("evidence.frames has no rendered frame".into());
    }
    out.extend(
        evidence
            .frames
            .iter()
            .filter_map(|f| evidence_problem(dir, f)),
    );
    out
}

/// What stops `review` from completing its pass; empty means it completes it. A pass is complete
/// when it has a `continue`, and for a visual pass also: evidence files on disk, an AI score of
/// at least 0.7, every layer score, and a clean feature gate.
pub fn completion_problems(spec: &SceneSpec, review: &Review, dir: &Path) -> Vec<String> {
    if review.action != Action::Continue {
        return vec![format!(
            "action is {}, not continue",
            review.action.as_str()
        )];
    }
    if !is_visual_pass(&review.pass_id) {
        return Vec::new();
    }
    let mut out = evidence_problems(dir, &review.evidence);
    match review.ai_score {
        Some(s) if s >= VISUAL_ACCEPTANCE_THRESHOLD => {}
        Some(s) => out.push(format!(
            "ai_score {s} is below {VISUAL_ACCEPTANCE_THRESHOLD}"
        )),
        None => out.push("ai_score is missing".into()),
    }
    for name in LAYER_SCORES {
        if !review.layer_scores.contains_key(name) {
            out.push(format!("layer score {name} is missing"));
        }
    }
    let targets = targets_for(&spec.feature_targets, &review.pass_id);
    out.extend(evaluate_features(&targets, &review.feature_reviews).failures);
    out
}

fn completes(spec: &SceneSpec, review: &Review, dir: &Path) -> bool {
    completion_problems(spec, review, dir).is_empty()
}

/// How many passes, from the first, are complete. Passes complete strictly in order, so the count
/// stops at the first gap.
pub fn completed_prefix(spec: &SceneSpec, dir: &Path) -> usize {
    spec.build_passes
        .iter()
        .take_while(|pass| {
            spec.review_history
                .iter()
                .any(|r| &r.pass_id == *pass && completes(spec, r, dir))
        })
        .count()
}

/// A `continue` that jumps ahead: for an unknown pass, or for a pass whose predecessor has no
/// earlier `continue`.
pub fn ledger_disagreements(spec: &SceneSpec) -> Vec<String> {
    let mut out = Vec::new();
    for (i, review) in spec.review_history.iter().enumerate() {
        let Some(index) = spec.build_passes.iter().position(|p| p == &review.pass_id) else {
            out.push(format!(
                "review {i} names pass {:?}, which is not in build_passes",
                review.pass_id
            ));
            continue;
        };
        if review.action != Action::Continue {
            continue;
        }
        for earlier in &spec.build_passes[..index] {
            let done = spec.review_history[..i]
                .iter()
                .any(|r| &r.pass_id == earlier && r.action == Action::Continue);
            if !done {
                out.push(format!(
                    "ledger disagreement: review {i} continues {} but {earlier} was never continued",
                    review.pass_id
                ));
            }
        }
    }
    out
}

fn limits_of(spec: &SceneSpec) -> LoopLimits {
    LoopLimits::new(spec.limits.max_per_pass, spec.limits.max_total).unwrap_or_default()
}

fn iteration_of(review: &Review) -> Iteration {
    Iteration {
        fidelity: review.fidelity.unwrap_or(f64::NAN),
        defect_tags: review.defect_tags.clone(),
        reverted: review.reverted,
        hard_gate_failures: review.hard_gate_failures.clone(),
        pending_review: None,
    }
}

fn advice(spec: &SceneSpec, pass: &str) -> Value {
    let history: Vec<Iteration> = spec
        .review_history
        .iter()
        .filter(|r| r.pass_id == pass)
        .map(iteration_of)
        .collect();
    match decide(&history, &Policy::default()) {
        Ok(Decision::ContinueIterating) => json!({"decision": "continue-iterating"}),
        Ok(Decision::Halt { action, reason }) => {
            json!({"decision": "stop-iterating", "action": action.as_str(), "reason": reason})
        }
        Err(e) => json!({"decision": "unknown", "error": e.to_string()}),
    }
}

fn call(tool: &str, arguments: Value) -> Value {
    json!({"tool": tool, "arguments": arguments})
}

fn gate_tool(pass: &str) -> &'static str {
    match pass {
        "motion" => "verify_motion",
        "audio" => "review_video",
        _ => "scene_check",
    }
}

fn requirements(spec: &SceneSpec, pass: &str) -> Vec<String> {
    let mut out = vec![format!(
        "the numeric gate ({}) passes with no hard failure",
        gate_tool(pass)
    )];
    if is_visual_pass(pass) {
        let targets = targets_for(&spec.feature_targets, pass);
        let gating: Vec<&str> = targets
            .iter()
            .filter(|t| super::features::is_gating(t))
            .map(|t| t.id.as_str())
            .collect();
        out.extend([
            "evidence.contact_sheet: one exports/<name>.png contact sheet of the pass".to_string(),
            "evidence.frames: the exports/<name>.png frames you judged".to_string(),
            format!("ai_score of at least {VISUAL_ACCEPTANCE_THRESHOLD}"),
            format!("layer_scores for {}", LAYER_SCORES.join(", ")),
            format!(
                "feature_reviews for the gating features: {}",
                gating.join(", ")
            ),
        ]);
    }
    out.push("record_review with exactly one action".into());
    out
}

struct Next {
    call: Option<Value>,
    instruction: String,
    awaiting_input: bool,
}

fn next_step(
    spec: &SceneSpec,
    name: &str,
    state: &LoopState,
    pass: Option<&str>,
    spec_ok: bool,
) -> Next {
    let video = spec
        .video_id
        .clone()
        .unwrap_or_else(|| "<video id from list_videos>".into());
    let go = |call: Option<Value>, instruction: &str| Next {
        call,
        instruction: instruction.into(),
        awaiting_input: false,
    };
    if state.status == RunStatus::Stopped {
        return go(
            None,
            "The run is stopped. Do not build or record more. Tell the user what is done, what is not and why it stopped.",
        );
    }
    if !spec_ok {
        return go(
            Some(call(
                "scene_spec_validate",
                json!({"name": name, "strict": true}),
            )),
            "The spec is not valid under strict. Fix what the validation lists with scene_spec_write, then validate again. Nothing is built from an invalid spec.",
        );
    }
    if spec.video_id.is_none() {
        return go(
            Some(call(
                "scene_spec_write",
                json!({"name": name, "spec": "<the SceneSpec with video_id>"}),
            )),
            "The spec has no video_id. Set it with scene_spec_write, then build the passes for that video.",
        );
    }
    let Some(pass) = pass else {
        return go(
            Some(call("review_video", json!({"video": video}))),
            "Every pass is complete. Run the final review_video, fix any RV001 to RV004, RV009, RV013 finding, then export.",
        );
    };
    let last = spec.review_history.iter().rfind(|r| r.pass_id == pass);
    match last.map(|r| r.action) {
        Some(Action::RequestInput) => Next {
            call: None,
            instruction: "A request-input is open. Ask the user what the review needs, then write the answer into the spec with scene_spec_write.".into(),
            awaiting_input: true,
        },
        Some(Action::RefineSpec) => go(
            Some(call(
                "scene_spec_write",
                json!({"name": name, "spec": "<the corrected SceneSpec>"}),
            )),
            "refine-spec: change the spec first (missing or wrong layer, palette, motion), validate it, then rebuild the pass from it.",
        ),
        Some(Action::RefineCode) => go(
            Some(call(gate_tool(pass), json!({"video": video}))),
            "refine-code: edit the existing nodes and tracks to match the spec (do not rebuild from scratch), render, then run the gate.",
        ),
        _ => go(
            Some(call(gate_tool(pass), json!({"video": video}))),
            "Build the pass from the spec, render it, then run this numeric gate. Only when it has no hard failure, look at one contact sheet and record the review.",
        ),
    }
}

/// `scene_status`: the whole state of a build in one JSON value.
pub fn status(spec: &SceneSpec, dir: &Path, name: &str) -> Value {
    let report = validate(spec);
    let spec_ok = report.ok(true);
    let prefix = completed_prefix(spec, dir);
    let pass = spec.build_passes.get(prefix).map(String::as_str);
    let limits = limits_of(spec);
    let state = loop_state(&spec.review_history, pass, limits);
    let next = next_step(spec, name, &state, pass, spec_ok);
    let missing = match pass {
        None => Vec::new(),
        Some(p) => match spec.review_history.iter().rfind(|r| r.pass_id == p) {
            Some(last) if last.action == Action::Continue => completion_problems(spec, last, dir),
            _ => requirements(spec, p),
        },
    };
    let first_issues: Vec<Value> = report
        .errors
        .iter()
        .chain(&report.quality)
        .take(8)
        .map(|i| json!({"path": i.path, "message": i.message}))
        .collect();
    json!({
        "name": name,
        "status": state.status.as_str(),
        "stop_reason": state.stop_reason,
        "complete": pass.is_none(),
        "awaiting_input": next.awaiting_input,
        "current_pass": pass,
        "passes_completed": &spec.build_passes[..prefix],
        "passes_remaining": &spec.build_passes[prefix..],
        "loops": {
            "per_pass": state.per_pass,
            "total": state.total,
            "max_per_pass": limits.max_per_pass,
            "max_total": limits.max_total,
        },
        "spec": {
            "strict_ok": spec_ok,
            "errors": report.errors.len(),
            "quality": report.quality.len(),
            "first_issues": first_issues,
        },
        "ledger_disagreements": ledger_disagreements(spec),
        "advice": pass.map(|p| advice(spec, p)),
        "missing_evidence": missing,
        "next_call": next.call,
        "next_instruction": next.instruction,
        "next_beat": super::beats::next_unbuilt(spec),
        "uncalibrated": &spec.calibration.uncalibrated,
        "note": "Loop limits are safety limits, not targets: stop earlier on success, a repeated defect, oscillation or a plateau.",
    })
}

fn unit(label: &str, value: f64) -> Result<(), RecordError> {
    if (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        refuse(format!("{label} {value} is outside 0 to 1"))
    }
}

fn check_fields(spec: &SceneSpec, review: &Review, dir: &Path) -> Result<(), RecordError> {
    match review.fidelity {
        Some(f) => unit("fidelity", f)?,
        None => return refuse("fidelity is required: a number from 0 to 1"),
    }
    if let Some(s) = review.ai_score {
        unit("ai_score", s)?;
    }
    for (name, score) in &review.layer_scores {
        if !LAYER_SCORES.contains(&name.as_str()) {
            return refuse(format!(
                "layer score {name:?} is not one of {LAYER_SCORES:?}"
            ));
        }
        unit(name, *score)?;
    }
    for f in &review.feature_reviews {
        if !spec.feature_targets.iter().any(|t| t.id == f.id) {
            return refuse(format!(
                "feature review {:?} is not in feature_targets",
                f.id
            ));
        }
        if let Some(s) = f.score {
            unit(&f.id, s)?;
        }
    }
    let summary = review.summary.trim();
    if summary.is_empty() || summary.chars().count() > MAX_SUMMARY_CHARS {
        return refuse(format!(
            "summary must say what changed and what the gate does not measure (1 to {MAX_SUMMARY_CHARS} characters)"
        ));
    }
    let named = review
        .evidence
        .contact_sheet
        .iter()
        .chain(&review.evidence.frames);
    for file in named {
        if let Some(p) = evidence_problem(dir, file) {
            return refuse(p);
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Recorded {
    pub index: usize,
    pub completes_pass: bool,
}

/// Appends `review` to the history after every refusal rule: the spec is strictly valid, the pass
/// is the current one, the loop has not stopped, the fields are in range and any `continue` really
/// completes its pass. Passes lock in order: a `continue` for a later pass is a ledger
/// disagreement.
pub fn record(spec: &mut SceneSpec, review: Review, dir: &Path) -> Result<Recorded, RecordError> {
    let report = validate(spec);
    if !report.ok(true) {
        let first = report.errors.iter().chain(&report.quality).next();
        return refuse(format!(
            "the spec is not valid under strict ({} errors, {} quality findings{}); fix it before recording a review",
            report.errors.len(),
            report.quality.len(),
            first
                .map(|i| format!(": {} {}", i.path, i.message))
                .unwrap_or_default()
        ));
    }
    let Some(index) = spec.build_passes.iter().position(|p| p == &review.pass_id) else {
        return refuse(format!("{:?} is not in build_passes", review.pass_id));
    };
    let prefix = completed_prefix(spec, dir);
    if index > prefix {
        return refuse(format!(
            "ledger disagreement: {} is locked, {} is not complete yet",
            review.pass_id, spec.build_passes[prefix]
        ));
    }
    if index < prefix {
        return refuse(format!("{} is already complete", review.pass_id));
    }
    let state = loop_state(&spec.review_history, Some(&review.pass_id), limits_of(spec));
    if let (RunStatus::Stopped, Some(reason)) = (&state.status, &state.stop_reason) {
        let recorded_stop = reason.starts_with("stop-recorded");
        let ends_the_run = matches!(review.action, Action::Stop | Action::RequestInput);
        if recorded_stop || !ends_the_run {
            return refuse(format!("the run is stopped: {reason}"));
        }
    }
    if spec.review_history.len() >= MAX_REVIEWS {
        return refuse(format!("review_history is full ({MAX_REVIEWS} entries)"));
    }
    check_fields(spec, &review, dir)?;
    let problems = completion_problems(spec, &review, dir);
    let completes_pass = problems.is_empty();
    if review.action == Action::Continue && !completes_pass {
        return refuse(format!(
            "continue refused for {}: {}",
            review.pass_id,
            problems.join("; ")
        ));
    }
    spec.review_history.push(review);
    Ok(Recorded {
        index: spec.review_history.len() - 1,
        completes_pass,
    })
}
