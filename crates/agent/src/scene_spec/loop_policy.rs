//! The bounded correction loop: when to stop refining and what to do instead, and the counters
//! that make the stop non-negotiable. Pure functions over the review history, never over memory.

use std::collections::BTreeMap;

use thiserror::Error;

use super::schema::{Action, Review};

pub const DEFAULT_MAX_PER_PASS: u32 = 3;
pub const DEFAULT_MAX_TOTAL: u32 = 6;
pub const DEFAULT_TARGET_FIDELITY: f64 = 0.85;
pub const DEFAULT_MAX_ITER: usize = 6;
pub const DEFAULT_MIN_DELTA: f64 = 0.02;

#[derive(Debug, Error, PartialEq)]
pub enum LoopError {
    #[error("max_per_pass must be at least 1 (got {0})")]
    PerPassTooSmall(u32),
    #[error("max_total must be at least 1 (got {0})")]
    TotalTooSmall(u32),
    #[error("max_per_pass ({per_pass}) cannot exceed max_total ({total})")]
    PerPassAboveTotal { per_pass: u32, total: u32 },
    #[error("fidelity {0} is outside 0 to 1")]
    FidelityOutOfRange(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopLimits {
    pub max_per_pass: u32,
    pub max_total: u32,
}

impl Default for LoopLimits {
    fn default() -> Self {
        LoopLimits {
            max_per_pass: DEFAULT_MAX_PER_PASS,
            max_total: DEFAULT_MAX_TOTAL,
        }
    }
}

impl LoopLimits {
    pub fn new(max_per_pass: u32, max_total: u32) -> Result<LoopLimits, LoopError> {
        if max_per_pass < 1 {
            return Err(LoopError::PerPassTooSmall(max_per_pass));
        }
        if max_total < 1 {
            return Err(LoopError::TotalTooSmall(max_total));
        }
        if max_per_pass > max_total {
            return Err(LoopError::PerPassAboveTotal {
                per_pass: max_per_pass,
                total: max_total,
            });
        }
        Ok(LoopLimits {
            max_per_pass,
            max_total,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Active,
    Stopped,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Active => "active",
            RunStatus::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopState {
    pub per_pass: BTreeMap<String, u32>,
    pub total: u32,
    pub status: RunStatus,
    pub stop_reason: Option<String>,
}

/// The loop counters from the reviews. A refine action counts against its pass and the total; the
/// per-pass limit of the current pass is checked first, then the total. A recorded `stop` also
/// stops the run.
pub fn loop_state(history: &[Review], current_pass: Option<&str>, limits: LoopLimits) -> LoopState {
    let mut per_pass: BTreeMap<String, u32> = BTreeMap::new();
    let mut total = 0;
    for review in history.iter().filter(|r| r.action.is_refine()) {
        *per_pass.entry(review.pass_id.clone()).or_insert(0) += 1;
        total += 1;
    }
    let stop_reason = stop_reason(history, current_pass, limits, &per_pass, total);
    LoopState {
        per_pass,
        total,
        status: if stop_reason.is_some() {
            RunStatus::Stopped
        } else {
            RunStatus::Active
        },
        stop_reason,
    }
}

fn stop_reason(
    history: &[Review],
    current_pass: Option<&str>,
    limits: LoopLimits,
    per_pass: &BTreeMap<String, u32>,
    total: u32,
) -> Option<String> {
    if let Some(stop) = history.iter().find(|r| r.action == Action::Stop) {
        return Some(format!("stop-recorded:{}", stop.pass_id));
    }
    let pass = current_pass?;
    let n = per_pass.get(pass).copied().unwrap_or(0);
    if n >= limits.max_per_pass {
        return Some(format!(
            "max-correction-loops-reached:{pass}:{n}/{}",
            limits.max_per_pass
        ));
    }
    if total >= limits.max_total {
        return Some(format!(
            "max-total-correction-loops-reached:{total}/{}",
            limits.max_total
        ));
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    pub target_fidelity: f64,
    pub max_iter: usize,
    pub min_delta: f64,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            target_fidelity: DEFAULT_TARGET_FIDELITY,
            max_iter: DEFAULT_MAX_ITER,
            min_delta: DEFAULT_MIN_DELTA,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Iteration {
    pub fidelity: f64,
    pub defect_tags: Vec<String>,
    pub reverted: bool,
    pub hard_gate_failures: Vec<String>,
    pub pending_review: Option<PendingReview>,
}

/// A review the numbers could not settle; `route` is what the reviewer said to do about it.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingReview {
    pub route: Option<Action>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    ContinueIterating,
    Halt {
        action: Action,
        reason: &'static str,
    },
}

fn halt(action: Action, reason: &'static str) -> Result<Decision, LoopError> {
    Ok(Decision::Halt { action, reason })
}

/// Whether to keep iterating; the first matching rule wins:
/// 1 empty history, 2 a hard gate failed, 3 a pending review, 4 target reached clean,
/// 5 the same defect twice running, 6 two reverts running, 7 a plateau below target, 8 the
/// iteration ceiling, 9 otherwise keep going.
pub fn decide(history: &[Iteration], policy: &Policy) -> Result<Decision, LoopError> {
    for it in history {
        if !(0.0..=1.0).contains(&it.fidelity) {
            return Err(LoopError::FidelityOutOfRange(it.fidelity));
        }
    }
    let Some(last) = history.last() else {
        return Ok(Decision::ContinueIterating);
    };
    if !last.hard_gate_failures.is_empty() {
        return halt(Action::RefineCode, "a hard gate failed");
    }
    if let Some(pending) = &last.pending_review {
        return halt(
            match pending.route {
                Some(Action::RefineCode) => Action::RefineCode,
                Some(Action::RefineSpec) => Action::RefineSpec,
                _ => Action::RequestInput,
            },
            "a review is pending",
        );
    }
    if last.fidelity >= policy.target_fidelity && last.defect_tags.is_empty() {
        return halt(Action::Continue, "target fidelity reached with no defects");
    }
    if let [.., prev, last] = history {
        if last
            .defect_tags
            .iter()
            .any(|t| prev.defect_tags.contains(t))
        {
            return halt(
                Action::RefineSpec,
                "the same defect survived 2 consecutive fixes",
            );
        }
        if prev.reverted && last.reverted {
            return halt(Action::RefineSpec, "oscillating: two reverts in a row");
        }
        if last.fidelity - prev.fidelity < policy.min_delta
            && last.fidelity < policy.target_fidelity
        {
            return halt(Action::RequestInput, "plateau: fidelity stopped improving");
        }
    }
    if history.len() >= policy.max_iter {
        return halt(Action::RequestInput, "iteration ceiling reached");
    }
    Ok(Decision::ContinueIterating)
}
