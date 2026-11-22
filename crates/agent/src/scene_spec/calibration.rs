//! "Never hard-gate an unmeasured signal": every threshold says how it was obtained, and one
//! that was only guessed can warn but never block.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Calibration {
    Measured,
    Derived,
    Uncalibrated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warn,
    Error,
}

/// The severity a finding may have: an uncalibrated threshold is capped at `Warn`.
pub fn capped(severity: Severity, calibration: Calibration) -> Severity {
    match calibration {
        Calibration::Uncalibrated => severity.min(Severity::Warn),
        Calibration::Measured | Calibration::Derived => severity,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorpusReport {
    pub corpus_acceptable: bool,
    pub suggested_threshold: Option<f64>,
    pub warnings: Vec<String>,
}

/// A threshold from labelled scores: only when every good score beats every bad one, and then
/// the midpoint between the worst good and the best bad.
pub fn calibrate(good: &[f64], bad: &[f64]) -> CorpusReport {
    if good.is_empty() || bad.is_empty() {
        return CorpusReport {
            corpus_acceptable: false,
            suggested_threshold: None,
            warnings: vec!["the corpus needs BOTH good and bad pairs".into()],
        };
    }
    let min_good = good.iter().copied().fold(f64::INFINITY, f64::min);
    let max_bad = bad.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if min_good > max_bad {
        CorpusReport {
            corpus_acceptable: true,
            suggested_threshold: Some((min_good + max_bad) / 2.0),
            warnings: Vec::new(),
        }
    } else {
        CorpusReport {
            corpus_acceptable: false,
            suggested_threshold: None,
            warnings: vec![format!(
                "good and bad scores overlap: the lowest good {min_good} is not above the highest bad {max_bad}"
            )],
        }
    }
}
