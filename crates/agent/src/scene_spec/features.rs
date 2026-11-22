//! The per-feature gate and the detail-inventory minima. The reviewer (the agent) supplies the
//! scores; this module only decides what they allow.

use super::schema::{Action, Detail, FeatureReview, FeatureTarget};

pub const CRITICAL_MIN: f64 = 0.8;
pub const IMPORTANT_MIN: f64 = 0.65;
pub const DETAIL_MIN: f64 = 0.5;
pub const IMPORTANT_AVERAGE_MIN: f64 = 0.65;
pub const MAX_CRITICAL_PER_PASS: usize = 5;
pub const MAX_IMPORTANT_PER_PASS: usize = 3;

/// The score a target must reach: its own `minimum_score`, else the default of its tier.
pub fn minimum_of(target: &FeatureTarget) -> f64 {
    target.minimum_score.unwrap_or(match target.tier.as_str() {
        "critical" => CRITICAL_MIN,
        "important" => IMPORTANT_MIN,
        _ => DETAIL_MIN,
    })
}

/// A critical or `must_pass` target blocks the pass; the rest only count toward the average.
pub fn is_gating(target: &FeatureTarget) -> bool {
    target.tier == "critical" || target.must_pass
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeatureStatus {
    Passed,
    /// Reviewed, but invisible, unscored or under its minimum: the build is wrong.
    Below,
    /// No review entry: the feature was never built, so the spec lacked it.
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureResult {
    pub id: String,
    pub status: FeatureStatus,
    pub score: Option<f64>,
    pub minimum: f64,
    pub gating: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FeatureGate {
    pub gating_passed: bool,
    /// Every reason `continue` is refused: gating failures and the important-average policy.
    pub failures: Vec<String>,
    pub important_average: Option<f64>,
    pub action: Action,
    pub results: Vec<FeatureResult>,
}

impl FeatureGate {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

fn result_of(target: &FeatureTarget, reviews: &[FeatureReview]) -> FeatureResult {
    let minimum = minimum_of(target);
    let review = reviews.iter().find(|r| r.id == target.id);
    let (status, score) = match review {
        None => (FeatureStatus::Missing, None),
        Some(r) => {
            let visible = r.visible != Some(false);
            match r.score {
                Some(s) if visible && s >= minimum => (FeatureStatus::Passed, Some(s)),
                other => (FeatureStatus::Below, other),
            }
        }
    };
    FeatureResult {
        id: target.id.clone(),
        status,
        score,
        minimum,
        gating: is_gating(target),
    }
}

/// Gates the reviewed scores against the targets of one pass. A missing gating feature means
/// `refine-spec`, a built one below its minimum `refine-code`, and anything clean `continue`
/// unless the average of the reviewed important features is under `IMPORTANT_AVERAGE_MIN`.
pub fn evaluate_features(targets: &[&FeatureTarget], reviews: &[FeatureReview]) -> FeatureGate {
    let results: Vec<FeatureResult> = targets.iter().map(|t| result_of(t, reviews)).collect();
    let mut failures = Vec::new();
    let mut missing = false;
    let mut below = false;
    for r in results.iter().filter(|r| r.gating) {
        match r.status {
            FeatureStatus::Passed => {}
            FeatureStatus::Missing => {
                missing = true;
                failures.push(format!(
                    "gating feature {} has no review entry: it was never built, fix the spec",
                    r.id
                ));
            }
            FeatureStatus::Below => {
                below = true;
                failures.push(match r.score {
                    Some(s) => format!("gating feature {} scored {s}, below {}", r.id, r.minimum),
                    None => format!(
                        "gating feature {} is not visible or has no numeric score (needs {})",
                        r.id, r.minimum
                    ),
                });
            }
        }
    }
    let gating_passed = failures.is_empty();
    let important: Vec<f64> = targets
        .iter()
        .zip(&results)
        .filter(|(t, _)| t.tier == "important")
        .filter_map(|(_, r)| r.score)
        .collect();
    let important_average =
        (!important.is_empty()).then(|| important.iter().sum::<f64>() / important.len() as f64);
    let average_low = important_average.is_some_and(|a| a < IMPORTANT_AVERAGE_MIN);
    if let Some(avg) = important_average.filter(|_| average_low) {
        failures.push(format!(
            "important features average {avg:.2}, below {IMPORTANT_AVERAGE_MIN}"
        ));
    }
    let action = if missing {
        Action::RefineSpec
    } else if below || average_low {
        Action::RefineCode
    } else {
        Action::Continue
    };
    FeatureGate {
        gating_passed,
        failures,
        important_average,
        action,
        results,
    }
}

/// The least number of details a tier must inventory, or `None` for an unknown tier.
pub fn tier_floor(tier: &str) -> Option<u32> {
    match tier {
        "simple" => Some(3),
        "moderate" => Some(6),
        "complex" => Some(10),
        "ultra" | "ultra-complex" => Some(16),
        _ => None,
    }
}

/// The declared target raised to the tier floor, never below it.
pub fn effective_min_details(tier: &str, declared: Option<u32>) -> Option<u32> {
    tier_floor(tier).map(|floor| floor.max(declared.unwrap_or(0)))
}

/// Why an inventory is too shallow: too few details, or a detail that reaches no field.
pub fn detail_inventory_problems(
    tier: &str,
    declared_min: Option<u32>,
    details: &[Detail],
    resolves: &dyn Fn(&str, &str) -> bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let min = effective_min_details(tier, declared_min).filter(|m| details.len() < *m as usize);
    if let Some(min) = min {
        out.push(format!(
            "{} details inventoried, tier {tier} needs at least {min}",
            details.len()
        ));
    }
    for d in details {
        if d.placeholder || d.kind.trim().is_empty() {
            out.push(format!("detail {} is still a starter placeholder", d.id));
            continue;
        }
        match &d.maps_to {
            Some(m) if resolves(&m.kind, &m.reference) => {}
            Some(m) => out.push(format!(
                "detail {} maps to {} {:?}, which does not exist",
                d.id, m.kind, m.reference
            )),
            None => out.push(format!("detail {} maps to nothing (prose only)", d.id)),
        }
    }
    out
}

/// Targets beyond the per-pass caps: at most 5 critical and 3 important per pass.
pub fn cap_problems(targets: &[&FeatureTarget], pass: &str) -> Vec<String> {
    let count = |tier: &str| targets.iter().filter(|t| t.tier == tier).count();
    let mut out = Vec::new();
    if count("critical") > MAX_CRITICAL_PER_PASS {
        out.push(format!(
            "pass {pass} has {} critical features, at most {MAX_CRITICAL_PER_PASS}",
            count("critical")
        ));
    }
    if count("important") > MAX_IMPORTANT_PER_PASS {
        out.push(format!(
            "pass {pass} has {} important features, at most {MAX_IMPORTANT_PER_PASS}",
            count("important")
        ));
    }
    out
}

/// The targets that apply to `pass`: those naming it, or naming no pass at all.
pub fn targets_for<'a>(targets: &'a [FeatureTarget], pass: &str) -> Vec<&'a FeatureTarget> {
    targets
        .iter()
        .filter(|t| t.pass_ids.is_empty() || t.pass_ids.iter().any(|p| p == pass))
        .collect()
}
