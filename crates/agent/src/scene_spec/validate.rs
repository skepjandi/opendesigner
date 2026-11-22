//! `scene_spec_validate`: errors (`E`, always) and strict-quality findings (`Q`, errors only
//! under `strict`). Strict is what a build needs before it starts; a starter fails it on purpose.

use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::features::{cap_problems, detail_inventory_problems, targets_for};
use super::loop_policy::LoopLimits;
use super::routing::{self, RoutingStatus};
use super::schema::{Layer, PASSES, SCHEMA, SceneSpec};
use super::status;

pub const MAX_DEPTH: f64 = 1.5;
pub const MAX_CAMERA_ZOOM: f64 = 1.035;
const LEVELS: [&str; 3] = ["macro", "meso", "micro"];
const TIERS: [&str; 5] = ["simple", "moderate", "complex", "ultra", "ultra-complex"];
const FEATURE_TIERS: [&str; 3] = ["critical", "important", "detail"];
const MAPS_TO_KINDS: [&str; 3] = ["node", "track", "palette"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

fn issue(path: impl Into<String>, message: impl Into<String>) -> Issue {
    Issue {
        path: path.into(),
        message: message.into(),
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub errors: Vec<Issue>,
    pub quality: Vec<Issue>,
}

impl Report {
    pub fn ok(&self, strict: bool) -> bool {
        self.errors.is_empty() && (!strict || self.quality.is_empty())
    }

    pub fn to_json(&self, strict: bool) -> Value {
        let list = |v: &[Issue]| -> Vec<Value> {
            v.iter()
                .map(|i| json!({"path": i.path, "message": i.message}))
                .collect()
        };
        json!({
            "ok": self.ok(strict),
            "strict": strict,
            "errors": list(&self.errors),
            "quality": list(&self.quality),
            "note": "quality findings block the build only under strict; scene_status always runs strict",
        })
    }
}

/// The ids a reference can point at.
pub struct Known {
    pub layers: BTreeSet<String>,
    pub palette: BTreeSet<String>,
    pub motions: BTreeSet<String>,
    pub audio_tracks: BTreeSet<String>,
}

impl Known {
    pub fn of(spec: &SceneSpec) -> Known {
        let mut layers = BTreeSet::new();
        for l in &spec.layer_plan {
            layers.insert(l.id.clone());
            if let Some(n) = &l.node_ref {
                layers.insert(n.clone());
            }
        }
        Known {
            layers,
            palette: spec.palette.tokens.iter().map(|t| t.id.clone()).collect(),
            motions: spec.motion_plan.iter().map(|m| m.id.clone()).collect(),
            audio_tracks: spec
                .audio_plan
                .tracks
                .iter()
                .map(|t| t.id.clone())
                .collect(),
        }
    }

    pub fn maps_to(&self, kind: &str, reference: &str) -> bool {
        match kind {
            "node" => self.layers.contains(reference),
            "track" => self.motions.contains(reference) || self.audio_tracks.contains(reference),
            "palette" => self.palette.contains(reference),
            _ => false,
        }
    }
}

pub fn valid_hex(hex: &str) -> bool {
    let Some(digits) = hex.strip_prefix('#') else {
        return false;
    };
    matches!(digits.len(), 3 | 6 | 8) && digits.bytes().all(|b| b.is_ascii_hexdigit())
}

fn duplicates<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut dup = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            dup.insert(id.to_string());
        }
    }
    dup.into_iter().collect()
}

pub fn validate(spec: &SceneSpec) -> Report {
    let known = Known::of(spec);
    let mut r = Report::default();
    check_header(spec, &mut r);
    check_layers(spec, &known, &mut r);
    check_palette(spec, &mut r);
    check_details(spec, &known, &mut r);
    check_features(spec, &known, &mut r);
    check_motion(spec, &known, &mut r);
    check_audio(spec, &known, &mut r);
    check_history(spec, &mut r);
    check_strict(spec, &known, &mut r);
    r
}

fn check_header(spec: &SceneSpec, r: &mut Report) {
    if spec.schema != SCHEMA {
        r.errors.push(issue(
            "schema",
            format!("must be {SCHEMA:?}, got {:?}", spec.schema),
        ));
    }
    if spec.id.trim().is_empty() {
        r.errors.push(issue("id", "must not be empty"));
    }
    if !TIERS.contains(&spec.complexity.tier.as_str()) {
        r.errors.push(issue(
            "complexity.tier",
            format!("{:?} is not one of {TIERS:?}", spec.complexity.tier),
        ));
    }
    if let Err(e) = LoopLimits::new(spec.limits.max_per_pass, spec.limits.max_total) {
        r.errors.push(issue("limits", e.to_string()));
    }
    let resolution = routing::resolve(
        &spec.routing.classification,
        spec.routing.track.as_deref(),
        routing::TRACK_BY_KIND,
    );
    if spec.routing.status == "resolved" && resolution.status != RoutingStatus::Resolved {
        r.errors.push(issue(
            "routing.status",
            format!(
                "says resolved, but the classification gives request-input: {}",
                resolution.conflicts.join("; ")
            ),
        ));
    }
    check_passes(spec, r);
}

fn check_passes(spec: &SceneSpec, r: &mut Report) {
    let positions: Option<Vec<usize>> = spec
        .build_passes
        .iter()
        .map(|p| PASSES.iter().position(|q| q == p))
        .collect();
    let ordered = positions
        .as_ref()
        .is_some_and(|p| !p.is_empty() && p.windows(2).all(|w| w[0] < w[1]));
    if !ordered {
        r.errors.push(issue(
            "build_passes",
            format!(
                "must be a non-empty subset of {PASSES:?} in that order, got {:?}",
                spec.build_passes
            ),
        ));
    }
    if let Some(after) = &spec.freeze.after_pass
        && !PASSES.contains(&after.as_str())
    {
        r.errors.push(issue(
            "freeze.after_pass",
            format!("{after:?} is not a pass"),
        ));
    }
}

fn layer_by<'a>(spec: &'a SceneSpec, id: &str) -> Option<&'a Layer> {
    spec.layer_plan.iter().find(|l| l.id == id)
}

fn check_layers(spec: &SceneSpec, known: &Known, r: &mut Report) {
    for id in duplicates(spec.layer_plan.iter().map(|l| l.id.as_str())) {
        r.errors.push(issue(
            "layer_plan",
            format!("layer id {id:?} is used twice"),
        ));
    }
    for (i, l) in spec.layer_plan.iter().enumerate() {
        let at = |f: &str| format!("layer_plan[{i}].{f}");
        if l.id.trim().is_empty() {
            r.errors.push(issue(at("id"), "must not be empty"));
        }
        if !LEVELS.contains(&l.level.as_str()) {
            r.errors.push(issue(
                at("level"),
                format!("{:?} is not one of {LEVELS:?}", l.level),
            ));
        }
        if let Some(d) = l.depth
            && !(0.0..=MAX_DEPTH).contains(&d)
        {
            r.errors.push(issue(
                at("depth"),
                format!("{d} is outside 0 to {MAX_DEPTH}"),
            ));
        }
        for (field, target) in [
            ("parent", &l.parent),
            ("mirror_of", &l.mirror_of),
            ("attach.to", &l.attach.as_ref().and_then(|a| a.to.clone())),
        ] {
            if let Some(t) = target
                && layer_by(spec, t).is_none()
            {
                r.errors
                    .push(issue(at(field), format!("layer {t:?} does not exist")));
            }
        }
        if l.attach.is_some()
            && l.parent.is_none()
            && l.attach.as_ref().and_then(|a| a.to.as_ref()).is_none()
        {
            r.errors.push(issue(
                at("attach"),
                "needs a parent or attach.to to hang from",
            ));
        }
        for p in &l.palette_refs {
            if !known.palette.contains(p) {
                r.errors.push(issue(
                    at("palette_refs"),
                    format!("token {p:?} is not in palette.tokens"),
                ));
            }
        }
        check_side(spec, l, &at("side"), r);
    }
    check_cycles(spec, r);
}

fn check_side(spec: &SceneSpec, l: &Layer, path: &str, r: &mut Report) {
    let Some(side) = l.side.as_deref() else {
        return;
    };
    let opposite = match side {
        "l" => "r",
        "r" => "l",
        other => {
            r.errors
                .push(issue(path, format!("{other:?} is not l or r")));
            return;
        }
    };
    let partner = |p: &Layer| p.side.as_deref() == Some(opposite);
    let by_my_mirror = l
        .mirror_of
        .as_deref()
        .and_then(|m| layer_by(spec, m))
        .is_some_and(partner);
    let by_their_mirror = spec
        .layer_plan
        .iter()
        .any(|p| p.mirror_of.as_deref() == Some(l.id.as_str()) && partner(p));
    if !(by_my_mirror || by_their_mirror) {
        r.errors.push(issue(
            path,
            format!(
                "layer {:?} is side {side} but no side {opposite} layer mirrors it (set mirror_of)",
                l.id
            ),
        ));
    }
}

fn check_cycles(spec: &SceneSpec, r: &mut Report) {
    for l in &spec.layer_plan {
        let mut cursor = l.parent.as_deref();
        for _ in 0..=spec.layer_plan.len() {
            let Some(id) = cursor else {
                break;
            };
            if id == l.id {
                r.errors.push(issue(
                    "layer_plan",
                    format!("layer {:?} is its own ancestor", l.id),
                ));
                break;
            }
            cursor = layer_by(spec, id).and_then(|p| p.parent.as_deref());
        }
    }
}

fn check_palette(spec: &SceneSpec, r: &mut Report) {
    for id in duplicates(spec.palette.tokens.iter().map(|t| t.id.as_str())) {
        r.errors.push(issue(
            "palette.tokens",
            format!("token id {id:?} is used twice"),
        ));
    }
    let ids: BTreeSet<&str> = spec.palette.tokens.iter().map(|t| t.id.as_str()).collect();
    for (i, t) in spec.palette.tokens.iter().enumerate() {
        if !valid_hex(&t.hex) {
            r.errors.push(issue(
                format!("palette.tokens[{i}].hex"),
                format!("{:?} is not a hex colour like #1B2430", t.hex),
            ));
        }
        if let Some(d) = &t.derived_from
            && !ids.contains(d.as_str())
        {
            r.errors.push(issue(
                format!("palette.tokens[{i}].derived_from"),
                format!("token {d:?} does not exist"),
            ));
        }
    }
    if !spec.palette.light.angle_deg.is_finite() {
        r.errors
            .push(issue("palette.light.angle_deg", "must be a finite number"));
    }
}

fn check_details(spec: &SceneSpec, known: &Known, r: &mut Report) {
    for id in duplicates(spec.detail_inventory.details.iter().map(|d| d.id.as_str())) {
        r.errors.push(issue(
            "detail_inventory.details",
            format!("detail id {id:?} is used twice"),
        ));
    }
    for (i, d) in spec.detail_inventory.details.iter().enumerate() {
        let at = |f: &str| format!("detail_inventory.details[{i}].{f}");
        if !(0.0..=1.0).contains(&d.confidence) {
            r.errors.push(issue(
                at("confidence"),
                format!("{} is outside 0 to 1", d.confidence),
            ));
        }
        if let Some(m) = &d.maps_to {
            if !MAPS_TO_KINDS.contains(&m.kind.as_str()) {
                r.errors.push(issue(
                    at("maps_to.type"),
                    format!("{:?} is not one of {MAPS_TO_KINDS:?}", m.kind),
                ));
            } else if !known.maps_to(&m.kind, &m.reference) {
                r.errors.push(issue(
                    at("maps_to.ref"),
                    format!("{} {:?} does not exist", m.kind, m.reference),
                ));
            }
        }
    }
}

fn check_features(spec: &SceneSpec, known: &Known, r: &mut Report) {
    for id in duplicates(spec.feature_targets.iter().map(|t| t.id.as_str())) {
        r.errors.push(issue(
            "feature_targets",
            format!("feature id {id:?} is used twice"),
        ));
    }
    for (i, t) in spec.feature_targets.iter().enumerate() {
        let at = |f: &str| format!("feature_targets[{i}].{f}");
        if !FEATURE_TIERS.contains(&t.tier.as_str()) {
            r.errors.push(issue(
                at("tier"),
                format!("{:?} is not one of {FEATURE_TIERS:?}", t.tier),
            ));
        }
        if let Some(p) = t.pass_ids.iter().find(|p| !PASSES.contains(&p.as_str())) {
            r.errors
                .push(issue(at("pass_ids"), format!("{p:?} is not a pass")));
        }
        if let Some(m) = t.minimum_score
            && !(0.0..=1.0).contains(&m)
        {
            r.errors
                .push(issue(at("minimum_score"), format!("{m} is outside 0 to 1")));
        }
        if let Some(n) = t.node_refs.iter().find(|n| !known.layers.contains(*n)) {
            r.errors.push(issue(
                at("node_refs"),
                format!("layer {n:?} does not exist"),
            ));
        }
    }
}

fn check_motion(spec: &SceneSpec, known: &Known, r: &mut Report) {
    for id in duplicates(spec.motion_plan.iter().map(|m| m.id.as_str())) {
        r.errors.push(issue(
            "motion_plan",
            format!("motion id {id:?} is used twice"),
        ));
    }
    for (i, m) in spec.motion_plan.iter().enumerate() {
        let at = |f: &str| format!("motion_plan[{i}].{f}");
        if m.target != "@camera" && !known.layers.contains(&m.target) {
            r.errors.push(issue(
                at("target"),
                format!("{:?} is neither a layer nor @camera", m.target),
            ));
        }
        if let (Some(a), Some(b)) = (m.start_ms, m.end_ms)
            && b < a
        {
            r.errors
                .push(issue(at("end_ms"), format!("{b} is before start_ms {a}")));
        }
        if let Some(z) = m.zoom_max
            && !(z.is_finite() && (1.0..=MAX_CAMERA_ZOOM).contains(&z))
        {
            r.errors.push(issue(
                at("zoom_max"),
                format!("{z} is outside 1 to {MAX_CAMERA_ZOOM}"),
            ));
        }
        if let Some(x) = m.camera_x_max_px
            && !(x.is_finite() && x >= 0.0)
        {
            r.errors
                .push(issue(at("camera_x_max_px"), "must be 0 or more"));
        }
        if m.stance.iter().any(|s| s.to_ms <= s.from_ms) {
            r.errors
                .push(issue(at("stance"), "each stance needs from_ms < to_ms"));
        }
    }
}

fn check_audio(spec: &SceneSpec, known: &Known, r: &mut Report) {
    for id in duplicates(spec.audio_plan.tracks.iter().map(|t| t.id.as_str())) {
        r.errors.push(issue(
            "audio_plan.tracks",
            format!("track id {id:?} is used twice"),
        ));
    }
    for (i, t) in spec.audio_plan.tracks.iter().enumerate() {
        if t.to_ms < t.from_ms {
            r.errors.push(issue(
                format!("audio_plan.tracks[{i}].to_ms"),
                "is before from_ms",
            ));
        }
        if let Some(d) = &t.duck_under
            && !known.audio_tracks.contains(d)
        {
            r.errors.push(issue(
                format!("audio_plan.tracks[{i}].duck_under"),
                format!("track {d:?} does not exist"),
            ));
        }
    }
    for (i, c) in spec.audio_plan.cues.iter().enumerate() {
        if let Some(s) = &c.sync_to
            && !known.motions.contains(s)
        {
            r.errors.push(issue(
                format!("audio_plan.cues[{i}].sync_to"),
                format!("motion {s:?} does not exist"),
            ));
        }
    }
}

fn check_history(spec: &SceneSpec, r: &mut Report) {
    for message in status::ledger_disagreements(spec) {
        r.errors.push(issue("review_history", message));
    }
}

fn depth_of(spec: &SceneSpec, layer: &Layer) -> usize {
    let mut depth = 1;
    let mut cursor = layer.parent.as_deref();
    while let Some(p) = cursor {
        depth += 1;
        if depth > spec.layer_plan.len() + 1 {
            break;
        }
        cursor = layer_by(spec, p).and_then(|l| l.parent.as_deref());
    }
    depth
}

fn check_strict(spec: &SceneSpec, known: &Known, r: &mut Report) {
    let resolution = routing::resolve(
        &spec.routing.classification,
        spec.routing.track.as_deref(),
        routing::TRACK_BY_KIND,
    );
    if resolution.status != RoutingStatus::Resolved {
        r.quality.push(issue(
            "routing",
            format!(
                "not resolved (kind {}, confidence {}): {}",
                resolution.classification.kind,
                resolution.classification.confidence,
                resolution.conflicts.join("; ")
            ),
        ));
    }
    let tier = spec.complexity.tier.as_str();
    let resolves = |kind: &str, reference: &str| known.maps_to(kind, reference);
    for p in detail_inventory_problems(
        tier,
        spec.detail_inventory.target_min_details,
        &spec.detail_inventory.details,
        &resolves,
    ) {
        r.quality.push(issue("detail_inventory", p));
    }
    check_strict_layers(spec, tier, r);
    check_strict_features(spec, r);
    check_strict_motion(spec, r);
    if spec.palette.tokens.len() > spec.palette.max_tokens {
        r.quality.push(issue(
            "palette.tokens",
            format!(
                "{} tokens, max_tokens is {}",
                spec.palette.tokens.len(),
                spec.palette.max_tokens
            ),
        ));
    }
    for t in &spec.palette.tokens {
        if t.source.as_deref() == Some("sampled") && t.sample_ref.is_none() {
            r.quality.push(issue(
                "palette.tokens",
                format!("token {:?} is sampled but has no sample_ref", t.id),
            ));
        }
    }
    if spec.audio_plan.tracks.is_empty() && !spec.brief.silent {
        r.quality.push(issue(
            "audio_plan.tracks",
            "no audio track and the brief is not silent",
        ));
    }
}

fn check_strict_layers(spec: &SceneSpec, tier: &str, r: &mut Report) {
    let level = |name: &str| spec.layer_plan.iter().filter(|l| l.level == name).count();
    if level("macro") < 1 {
        r.quality
            .push(issue("layer_plan", "needs at least one macro layer"));
    }
    if tier != "simple" && level("meso") < 1 {
        r.quality.push(issue(
            "layer_plan",
            format!("tier {tier} needs at least one meso layer"),
        ));
    }
    let deepest = spec
        .layer_plan
        .iter()
        .map(|l| depth_of(spec, l))
        .max()
        .unwrap_or(0);
    if matches!(tier, "complex" | "ultra" | "ultra-complex") && deepest < 2 {
        r.quality.push(issue(
            "layer_plan",
            format!("tier {tier} needs a hierarchy at least 2 deep"),
        ));
    }
    for l in spec.layer_plan.iter().filter(|l| l.placeholder) {
        r.quality.push(issue(
            "layer_plan",
            format!("layer {:?} is still a starter placeholder", l.id),
        ));
    }
    for l in &spec.layer_plan {
        let moves = !matches!(
            l.expects_motion.as_deref(),
            None | Some("" | "none" | "static")
        );
        let driven = spec
            .motion_plan
            .iter()
            .any(|m| m.target == l.id || l.node_ref.as_deref() == Some(m.target.as_str()));
        if moves && !driven {
            r.quality.push(issue(
                "layer_plan",
                format!(
                    "layer {:?} expects motion but motion_plan has none for it",
                    l.id
                ),
            ));
        }
    }
}

fn check_strict_features(spec: &SceneSpec, r: &mut Report) {
    let passes: BTreeSet<&str> = spec
        .build_passes
        .iter()
        .map(String::as_str)
        .chain(
            spec.feature_targets
                .iter()
                .flat_map(|t| t.pass_ids.iter().map(String::as_str)),
        )
        .collect();
    for pass in passes {
        for p in cap_problems(&targets_for(&spec.feature_targets, pass), pass) {
            r.quality.push(issue("feature_targets", p));
        }
    }
    for t in &spec.feature_targets {
        if t.placeholder {
            r.quality.push(issue(
                "feature_targets",
                format!("feature {:?} is still a starter placeholder", t.id),
            ));
        }
        if t.tier == "critical" && t.minimum_score.is_none() {
            r.quality.push(issue(
                "feature_targets",
                format!("critical feature {:?} needs a numeric minimum_score", t.id),
            ));
        }
    }
}

fn check_strict_motion(spec: &SceneSpec, r: &mut Report) {
    for m in &spec.motion_plan {
        if m.kind == "gait" && m.stance.is_empty() {
            r.quality.push(issue(
                "motion_plan",
                format!(
                    "gait {:?} has no stance, so foot slide cannot be measured",
                    m.id
                ),
            ));
        }
        if m.kind == "ambient" && m.looping != Some(true) {
            r.quality.push(issue(
                "motion_plan",
                format!("ambient motion {:?} must have loop: true", m.id),
            ));
        }
    }
}
