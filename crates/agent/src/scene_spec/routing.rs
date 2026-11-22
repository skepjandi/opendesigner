//! Routing by classification, fail-closed: a confident, known kind picks the playbook; anything
//! else asks the user. A guess is never promoted to a decision.

use serde_json::Value;

pub const CONFIDENCE_THRESHOLD: f64 = 0.82;
const MALFORMED_REF: &str = "routing:malformed-classification";

/// The kinds OpenDesigner routes, each to the `get_playbook` topic that builds it.
pub const TRACK_BY_KIND: &[(&str, &str)] = &[
    ("explainer", "explainer"),
    ("character", "characters"),
    ("scenery", "scenery"),
    ("sketch", "sketch"),
    ("device", "3d"),
    ("logo", "logo"),
    ("motion-graphics", "motion"),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Classification {
    pub kind: String,
    pub confidence: f64,
    pub evidence_refs: Vec<String>,
    pub provider: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingStatus {
    Resolved,
    RequestInput,
}

impl RoutingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RoutingStatus::Resolved => "resolved",
            RoutingStatus::RequestInput => "request-input",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub classification: Classification,
    pub status: RoutingStatus,
    /// Named whenever the kind is known, but only to be used when `status` is `Resolved`.
    pub track: Option<String>,
    pub conflicts: Vec<String>,
}

fn malformed() -> Classification {
    Classification {
        kind: "unknown".into(),
        confidence: 0.0,
        evidence_refs: vec![MALFORMED_REF.into()],
        provider: None,
        version: None,
    }
}

fn optional_text(raw: &Value, key: &str) -> Result<Option<String>, ()> {
    match raw.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(()),
    }
}

/// The record as written, or the synthetic unknown one when any part of it is malformed.
pub fn parse_classification(raw: &Value) -> Classification {
    parse_strict(raw).unwrap_or_else(|()| malformed())
}

fn parse_strict(raw: &Value) -> Result<Classification, ()> {
    let kind = raw.get("kind").and_then(Value::as_str).ok_or(())?;
    let confidence = raw.get("confidence").and_then(Value::as_f64).ok_or(())?;
    if kind.is_empty() || !(0.0..=1.0).contains(&confidence) {
        return Err(());
    }
    let refs = raw
        .get("evidence_refs")
        .and_then(Value::as_array)
        .ok_or(())?;
    let evidence_refs: Vec<String> = refs
        .iter()
        .map(|r| {
            r.as_str()
                .filter(|s| !s.trim().is_empty())
                .map(str::to_string)
        })
        .collect::<Option<_>>()
        .ok_or(())?;
    if evidence_refs.is_empty() {
        return Err(());
    }
    Ok(Classification {
        kind: kind.to_string(),
        confidence,
        evidence_refs,
        provider: optional_text(raw, "provider")?,
        version: optional_text(raw, "version")?,
    })
}

/// Resolves a raw classification against the kind-to-track table `tracks`. Resolved only when
/// the kind has a track, confidence reaches `CONFIDENCE_THRESHOLD`, nothing conflicts and an
/// explicit track, if named, is the kind's own.
pub fn resolve(raw: &Value, explicit_track: Option<&str>, tracks: &[(&str, &str)]) -> Resolution {
    let classification = parse_classification(raw);
    let track = tracks
        .iter()
        .find(|(k, _)| *k == classification.kind)
        .map(|(_, t)| (*t).to_string());
    let mut conflicts = Vec::new();
    if matches!(classification.kind.as_str(), "hybrid" | "unknown") {
        conflicts.push(format!(
            "classification kind {} requires input",
            classification.kind
        ));
    } else if track.is_none() {
        conflicts.push(format!(
            "classification kind {} has no track",
            classification.kind
        ));
    }
    if classification.confidence < CONFIDENCE_THRESHOLD {
        conflicts.push(format!(
            "classification confidence {} is below {CONFIDENCE_THRESHOLD}",
            classification.confidence
        ));
    }
    let reliable = track.is_some() && classification.confidence >= CONFIDENCE_THRESHOLD;
    let clash = match (reliable, explicit_track, &track) {
        (true, Some(explicit), Some(own)) if explicit != own => Some((explicit, own.as_str())),
        _ => None,
    };
    if let Some((explicit, own)) = clash {
        conflicts.push(format!(
            "explicit track {explicit} contradicts classification kind {} (track {own})",
            classification.kind
        ));
    }
    let status = if reliable && conflicts.is_empty() {
        RoutingStatus::Resolved
    } else {
        RoutingStatus::RequestInput
    };
    Resolution {
        classification,
        status,
        track,
        conflicts,
    }
}
