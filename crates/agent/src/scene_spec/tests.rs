use std::path::Path;

use serde_json::{Value, json};

use super::calibration::{Calibration, Severity, calibrate, capped};
use super::features::{
    FeatureStatus, detail_inventory_problems, effective_min_details, evaluate_features,
    targets_for, tier_floor,
};
use super::loop_policy::{
    Decision, Iteration, LoopError, LoopLimits, Policy, RunStatus, decide, loop_state,
};
use super::routing::{self, RoutingStatus};
use super::schema::{
    Action, AudioTrack, Evidence, FeatureReview, FeatureTarget, Layer, Motion, Review, SceneSpec,
    Stance, Token,
};
use super::status::{self, completed_prefix, ledger_disagreements};
use super::validate::{Issue, validate};
use super::{definitions, load, save, starter, valid_name};

const TRACKS: &[(&str, &str)] = &[("weapon", "weapon-v1.4"), ("character", "character-v1.5")];

fn route(raw: Value, explicit: Option<&str>) -> routing::Resolution {
    routing::resolve(&raw, explicit, TRACKS)
}

fn class(kind: &str, confidence: f64) -> Value {
    json!({"kind": kind, "confidence": confidence, "evidence_refs": ["brief:subject"]})
}

#[test]
fn routing_resolves_at_the_threshold_and_fails_closed_below_it() {
    let at = route(class("weapon", 0.82), None);
    assert_eq!(at.status, RoutingStatus::Resolved);
    assert_eq!(at.track.as_deref(), Some("weapon-v1.4"));
    assert!(at.conflicts.is_empty());

    let below = route(class("weapon", 0.81), None);
    assert_eq!(below.status, RoutingStatus::RequestInput);
    assert_eq!(below.track.as_deref(), Some("weapon-v1.4"));
    assert_eq!(
        below.conflicts,
        ["classification confidence 0.81 is below 0.82"]
    );
}

#[test]
fn routing_refuses_hybrid_unknown_and_malformed_records() {
    let hybrid = route(class("hybrid", 0.99), None);
    assert_eq!(hybrid.status, RoutingStatus::RequestInput);
    assert!(hybrid.conflicts[0].contains("requires input"));

    let empty_refs = route(
        json!({"kind": "unknown", "confidence": 0.9, "evidence_refs": []}),
        None,
    );
    assert_eq!(empty_refs.classification.confidence, 0.0);
    assert_eq!(empty_refs.classification.kind, "unknown");
    assert_eq!(empty_refs.status, RoutingStatus::RequestInput);

    for raw in [
        Value::Null,
        json!("weapon"),
        json!({"kind": "weapon", "confidence": 1.4, "evidence_refs": ["a"]}),
        json!({"kind": "weapon", "confidence": "high", "evidence_refs": ["a"]}),
        json!({"kind": "weapon", "confidence": 0.9, "evidence_refs": [""]}),
        json!({"kind": "weapon", "confidence": 0.9}),
    ] {
        let r = route(raw.clone(), None);
        assert_eq!(r.classification.confidence, 0.0, "{raw}");
        assert_eq!(r.status, RoutingStatus::RequestInput, "{raw}");
    }
    assert_eq!(
        route(class("banana", 0.99), None).status,
        RoutingStatus::RequestInput
    );
}

#[test]
fn an_explicit_track_cannot_contradict_a_reliable_classification() {
    let clash = route(class("weapon", 0.9), Some("character-v1.5"));
    assert_eq!(clash.status, RoutingStatus::RequestInput);
    assert!(clash.conflicts[0].contains("contradicts"));
    assert_eq!(
        route(class("weapon", 0.9), Some("weapon-v1.4")).status,
        RoutingStatus::Resolved
    );
}

fn target(id: &str, tier: &str, min: Option<f64>) -> FeatureTarget {
    FeatureTarget {
        id: id.into(),
        tier: tier.into(),
        minimum_score: min,
        ..FeatureTarget::default()
    }
}

fn scored(scores: &[(&str, f64)]) -> Vec<FeatureReview> {
    scores
        .iter()
        .map(|(id, s)| FeatureReview {
            id: (*id).into(),
            score: Some(*s),
            ..FeatureReview::default()
        })
        .collect()
}

fn abc() -> Vec<FeatureTarget> {
    vec![
        target("A", "critical", None),
        target("B", "important", Some(0.65)),
        target("C", "detail", None),
    ]
}

fn gate(scores: &[(&str, f64)]) -> super::features::FeatureGate {
    let targets = abc();
    let refs: Vec<&FeatureTarget> = targets.iter().collect();
    evaluate_features(&refs, &scored(scores))
}

#[test]
fn a_critical_feature_below_its_minimum_is_refine_code() {
    let g = gate(&[("A", 0.79)]);
    assert!(!g.passed() && !g.gating_passed);
    assert_eq!(g.action, Action::RefineCode);
    assert_eq!(g.results[0].status, FeatureStatus::Below);
    let ok = gate(&[("A", 0.80), ("B", 0.9)]);
    assert!(ok.passed());
    assert_eq!(ok.action, Action::Continue);
}

#[test]
fn a_missing_critical_feature_is_refine_spec() {
    let g = gate(&[]);
    assert_eq!(g.results[0].status, FeatureStatus::Missing);
    assert_eq!(g.action, Action::RefineSpec);
    assert!(!g.passed());
}

#[test]
fn an_important_feature_does_not_gate_but_its_average_does() {
    let g = gate(&[("A", 0.80), ("B", 0.60)]);
    assert!(g.gating_passed, "B is not gating");
    assert!(!g.passed());
    assert_eq!(g.important_average, Some(0.60));
    assert!(g.failures[0].contains("important features average"));
}

#[test]
fn a_boolean_minimum_score_is_ignored_and_the_tier_default_applies() {
    let t: FeatureTarget =
        serde_json::from_value(json!({"id": "A", "tier": "critical", "minimum_score": true}))
            .unwrap();
    assert_eq!(t.minimum_score, None);
    let refs = [&t];
    let g = evaluate_features(&refs, &scored(&[("A", 0.79)]));
    assert_eq!(g.results[0].minimum, 0.8);
    assert_eq!(g.action, Action::RefineCode);
}

#[test]
fn must_pass_makes_any_tier_gating_and_an_invisible_feature_is_below() {
    let mut t = target("D", "detail", None);
    t.must_pass = true;
    let refs = [&t];
    let g = evaluate_features(&refs, &scored(&[("D", 0.49)]));
    assert_eq!(g.action, Action::RefineCode);
    let hidden = [FeatureReview {
        id: "D".into(),
        score: Some(0.9),
        visible: Some(false),
        ..FeatureReview::default()
    }];
    assert_eq!(
        evaluate_features(&refs, &hidden).results[0].status,
        FeatureStatus::Below
    );
}

fn it(fidelity: f64, tags: &[&str], reverted: bool) -> Iteration {
    Iteration {
        fidelity,
        defect_tags: tags.iter().map(|t| (*t).into()).collect(),
        reverted,
        ..Iteration::default()
    }
}

fn halted(history: &[Iteration]) -> (Action, &'static str) {
    match decide(history, &Policy::default()).unwrap() {
        Decision::Halt { action, reason } => (action, reason),
        Decision::ContinueIterating => panic!("expected a halt"),
    }
}

#[test]
fn the_loop_keeps_going_while_nothing_says_stop() {
    assert_eq!(
        decide(&[], &Policy::default()).unwrap(),
        Decision::ContinueIterating
    );
    assert_eq!(
        decide(&[it(0.5, &["x"], false)], &Policy::default()).unwrap(),
        Decision::ContinueIterating
    );
}

#[test]
fn the_same_defect_twice_running_is_refine_spec() {
    let h = [it(0.5, &["x"], false), it(0.6, &["x"], false)];
    assert_eq!(halted(&h).0, Action::RefineSpec);
    let different = [it(0.5, &["x"], false), it(0.7, &["y"], false)];
    assert_eq!(
        decide(&different, &Policy::default()).unwrap(),
        Decision::ContinueIterating
    );
}

#[test]
fn two_reverts_in_a_row_is_refine_spec() {
    let h = [it(0.5, &[], true), it(0.6, &[], true)];
    assert_eq!(
        halted(&h),
        (Action::RefineSpec, "oscillating: two reverts in a row")
    );
    let one = [it(0.5, &[], true), it(0.7, &[], false)];
    assert_eq!(
        decide(&one, &Policy::default()).unwrap(),
        Decision::ContinueIterating
    );
}

#[test]
fn a_plateau_below_target_is_request_input() {
    let h = [it(0.70, &[], false), it(0.71, &[], false)];
    assert_eq!(halted(&h).0, Action::RequestInput);
    let improving = [it(0.70, &[], false), it(0.73, &[], false)];
    assert_eq!(
        decide(&improving, &Policy::default()).unwrap(),
        Decision::ContinueIterating
    );
}

#[test]
fn target_reached_without_defects_continues_and_with_defects_does_not() {
    assert_eq!(halted(&[it(0.86, &[], false)]).0, Action::Continue);
    assert_eq!(
        decide(&[it(0.86, &["x"], false)], &Policy::default()).unwrap(),
        Decision::ContinueIterating
    );
}

#[test]
fn a_hard_gate_failure_is_refine_code_whatever_the_score() {
    let mut last = it(0.99, &[], false);
    last.hard_gate_failures = vec!["attach.gap".into()];
    assert_eq!(halted(&[last]).0, Action::RefineCode);
}

#[test]
fn a_pending_review_routes_or_asks() {
    use super::loop_policy::PendingReview;
    let mut asks = it(0.5, &[], false);
    asks.pending_review = Some(PendingReview { route: None });
    assert_eq!(halted(&[asks]).0, Action::RequestInput);
    let mut routed = it(0.5, &[], false);
    routed.pending_review = Some(PendingReview {
        route: Some(Action::RefineSpec),
    });
    assert_eq!(halted(&[routed]).0, Action::RefineSpec);
}

#[test]
fn six_iterations_is_a_ceiling_that_cannot_be_bypassed() {
    let h: Vec<Iteration> = (0..6)
        .map(|i| it(0.5 + 0.05 * f64::from(i), &[], false))
        .collect();
    assert_eq!(
        halted(&h),
        (Action::RequestInput, "iteration ceiling reached")
    );
    assert_eq!(
        decide(&h[..5], &Policy::default()).unwrap(),
        Decision::ContinueIterating
    );
}

#[test]
fn fidelity_outside_zero_to_one_is_an_error() {
    assert_eq!(
        decide(&[it(1.2, &[], false)], &Policy::default()),
        Err(LoopError::FidelityOutOfRange(1.2))
    );
    assert!(decide(&[it(-0.1, &[], false)], &Policy::default()).is_err());
    assert!(decide(&[it(f64::NAN, &[], false)], &Policy::default()).is_err());
}

fn rev(pass: &str, action: Action) -> Review {
    Review {
        pass_id: pass.into(),
        action,
        fidelity: Some(0.5),
        ai_score: None,
        layer_scores: Default::default(),
        feature_reviews: Vec::new(),
        summary: "s".into(),
        evidence: Evidence::default(),
        defect_tags: Vec::new(),
        reverted: false,
        hard_gate_failures: Vec::new(),
    }
}

#[test]
fn three_refines_on_one_pass_stop_the_run() {
    let limits = LoopLimits::default();
    let two = vec![
        rev("shapes", Action::RefineCode),
        rev("shapes", Action::RefineCode),
    ];
    assert_eq!(
        loop_state(&two, Some("shapes"), limits).status,
        RunStatus::Active
    );
    let three = vec![
        rev("shapes", Action::RefineCode),
        rev("shapes", Action::RefineSpec),
        rev("shapes", Action::RefineCode),
    ];
    let s = loop_state(&three, Some("shapes"), limits);
    assert_eq!(s.status, RunStatus::Stopped);
    assert_eq!(
        s.stop_reason.as_deref(),
        Some("max-correction-loops-reached:shapes:3/3")
    );
}

#[test]
fn six_refines_across_passes_stop_the_run() {
    let limits = LoopLimits::default();
    let history: Vec<Review> = ["blockout", "shapes", "palette"]
        .iter()
        .flat_map(|p| {
            [
                rev(p, Action::RefineCode),
                rev(p, Action::RefineCode),
                rev(p, Action::Continue),
            ]
        })
        .collect();
    let s = loop_state(&history, Some("motion"), limits);
    assert_eq!(s.total, 6);
    assert_eq!(
        s.stop_reason.as_deref(),
        Some("max-total-correction-loops-reached:6/6")
    );
    assert_eq!(
        loop_state(&history[..7], Some("palette"), limits).status,
        RunStatus::Active,
        "five refines are still under the total"
    );
}

#[test]
fn limits_that_make_no_sense_are_refused() {
    assert_eq!(
        LoopLimits::new(3, 2),
        Err(LoopError::PerPassAboveTotal {
            per_pass: 3,
            total: 2
        })
    );
    assert_eq!(LoopLimits::new(0, 6), Err(LoopError::PerPassTooSmall(0)));
    assert_eq!(LoopLimits::new(3, 0), Err(LoopError::TotalTooSmall(0)));
    assert!(LoopLimits::new(1, 1).is_ok());
}

#[test]
fn a_recorded_stop_stops_the_run() {
    let s = loop_state(
        &[rev("blockout", Action::Stop)],
        Some("blockout"),
        LoopLimits::default(),
    );
    assert_eq!(s.stop_reason.as_deref(), Some("stop-recorded:blockout"));
}

#[test]
fn tier_floors_are_3_6_10_16_and_a_declared_target_only_raises_them() {
    assert_eq!(
        ["simple", "moderate", "complex", "ultra", "ultra-complex"].map(tier_floor),
        [Some(3), Some(6), Some(10), Some(16), Some(16)]
    );
    assert_eq!(tier_floor("huge"), None);
    assert_eq!(effective_min_details("moderate", Some(2)), Some(6));
    assert_eq!(effective_min_details("moderate", Some(9)), Some(9));
}

fn mapped_details(n: usize, bad_ref: bool) -> Vec<super::schema::Detail> {
    (0..n)
        .map(|i| super::schema::Detail {
            id: format!("d{i}"),
            kind: "highlight".into(),
            maps_to: Some(super::schema::MapsTo {
                kind: "node".into(),
                reference: if bad_ref && i == 0 {
                    "ghost".into()
                } else {
                    "gear".into()
                },
            }),
            confidence: 0.8,
            ..super::schema::Detail::default()
        })
        .collect()
}

#[test]
fn the_detail_inventory_needs_the_tier_minimum_and_every_detail_mapped() {
    let resolves = |kind: &str, r: &str| kind == "node" && r == "gear";
    assert!(
        detail_inventory_problems("moderate", None, &mapped_details(6, false), &resolves)
            .is_empty()
    );
    let few = detail_inventory_problems("moderate", None, &mapped_details(5, false), &resolves);
    assert!(few[0].contains("needs at least 6"));
    let ghost = detail_inventory_problems("moderate", None, &mapped_details(6, true), &resolves);
    assert!(ghost[0].contains("does not exist"));
    let mut prose = mapped_details(6, false);
    prose[2].maps_to = None;
    assert!(
        detail_inventory_problems("moderate", None, &prose, &resolves)[0].contains("prose only")
    );
}

#[test]
fn calibration_needs_good_and_bad_and_takes_the_midpoint() {
    let only_good = calibrate(&[0.9, 0.95], &[]);
    assert!(!only_good.corpus_acceptable);
    assert!(only_good.warnings[0].contains("BOTH good and bad"));
    let ok = calibrate(&[0.88, 0.95], &[0.71, 0.4]);
    assert!(ok.corpus_acceptable);
    assert!((ok.suggested_threshold.unwrap() - 0.795).abs() < 1e-12);
    let overlap = calibrate(&[0.7, 0.9], &[0.75]);
    assert!(!overlap.corpus_acceptable && overlap.suggested_threshold.is_none());
}

#[test]
fn an_uncalibrated_finding_is_capped_at_warn() {
    assert_eq!(
        capped(Severity::Error, Calibration::Uncalibrated),
        Severity::Warn
    );
    assert_eq!(
        capped(Severity::Info, Calibration::Uncalibrated),
        Severity::Info
    );
    assert_eq!(
        capped(Severity::Error, Calibration::Measured),
        Severity::Error
    );
    assert_eq!(
        capped(Severity::Error, Calibration::Derived),
        Severity::Error
    );
}

fn layer(id: &str, level: &str, role: &str) -> Layer {
    Layer {
        id: id.into(),
        level: level.into(),
        role: role.into(),
        ..Layer::default()
    }
}

fn good_spec() -> SceneSpec {
    let mut spec = SceneSpec {
        id: "main".into(),
        video_id: Some("v1".into()),
        ..SceneSpec::default()
    };
    spec.brief.subject = "how a gear turns".into();
    spec.routing.classification = class("explainer", 0.9);
    spec.routing.track = Some("explainer".into());
    spec.routing.status = "resolved".into();
    spec.palette.tokens = ["ink", "sky", "hero"]
        .iter()
        .zip(["#1B2430", "#CFE8F5", "#F26B3A"])
        .map(|(id, hex)| Token {
            id: (*id).into(),
            hex: hex.into(),
            ..Token::default()
        })
        .collect();
    let mut sky = layer("sky", "macro", "backdrop");
    sky.depth = Some(0.1);
    sky.bleed = true;
    sky.palette_refs = vec!["sky".into()];
    sky.expects_motion = Some("ambient".into());
    let gear = layer("gear", "macro", "hero");
    let mut chain = layer("chain", "meso", "prop");
    chain.parent = Some("gear".into());
    chain.expects_motion = Some("loop".into());
    let mut arm_l = layer("arm-l", "meso", "character-part");
    arm_l.side = Some("l".into());
    arm_l.mirror_of = Some("arm-r".into());
    let mut arm_r = layer("arm-r", "meso", "character-part");
    arm_r.side = Some("r".into());
    spec.layer_plan = vec![sky, gear, chain, arm_l, arm_r];
    spec.detail_inventory.details = mapped_details(6, false);
    spec.feature_targets = vec![
        FeatureTarget {
            pass_ids: vec!["blockout".into(), "shapes".into()],
            must_pass: true,
            ..target("hero-silhouette", "critical", Some(0.8))
        },
        FeatureTarget {
            pass_ids: vec!["palette".into()],
            ..target("palette-fidelity", "important", None)
        },
    ];
    let motion = |id: &str, target: &str, kind: &str, looping: Option<bool>| Motion {
        id: id.into(),
        target: target.into(),
        kind: kind.into(),
        looping,
        ..Motion::default()
    };
    spec.motion_plan = vec![
        motion("m1", "gear", "entrance", None),
        motion("m2", "chain", "ambient", Some(true)),
        motion("m3", "sky", "ambient", Some(true)),
    ];
    spec.audio_plan.tracks = vec![AudioTrack {
        id: "music".into(),
        kind: "music".into(),
        to_ms: 5000,
        ..AudioTrack::default()
    }];
    spec
}

fn paths(issues: &[Issue]) -> Vec<String> {
    issues
        .iter()
        .map(|i| format!("{}: {}", i.path, i.message))
        .collect()
}

fn strict_fails(spec: &SceneSpec, needle: &str) {
    let r = validate(spec);
    let all = paths(&r.errors)
        .into_iter()
        .chain(paths(&r.quality))
        .collect::<Vec<_>>();
    assert!(!r.ok(true), "expected a failure containing {needle:?}");
    assert!(
        all.iter().any(|m| m.contains(needle)),
        "no finding contains {needle:?}: {all:#?}"
    );
}

#[test]
fn the_good_spec_passes_strict_and_the_starter_does_not() {
    let r = validate(&good_spec());
    assert!(r.ok(true), "{:#?} {:#?}", r.errors, r.quality);
    let s = starter("main", "a gear", "moderate");
    let r = validate(&s);
    assert!(r.errors.is_empty(), "{:#?}", r.errors);
    assert!(!r.ok(true));
    assert!(r.ok(false), "the starter is valid, only not strict-valid");
    assert_eq!(s.detail_inventory.details.len(), 6);
    let q = paths(&r.quality).join("\n");
    for needle in ["routing", "placeholder", "audio"] {
        assert!(q.contains(needle), "{needle} not in {q}");
    }
}

#[test]
fn routing_below_threshold_fails_strict_and_a_false_resolved_is_an_error() {
    let mut s = good_spec();
    s.routing.classification = class("explainer", 0.81);
    strict_fails(&s, "routing");
    assert!(!validate(&s).errors.is_empty(), "status says resolved");
    s.routing.status = "request-input".into();
    assert!(validate(&s).errors.is_empty());
    strict_fails(&s, "0.81 is below 0.82");
}

#[test]
fn routing_needs_an_evidence_ref() {
    let mut s = good_spec();
    s.routing.classification =
        json!({"kind": "explainer", "confidence": 0.95, "evidence_refs": []});
    strict_fails(&s, "routing");
}

#[test]
fn references_must_resolve() {
    let mut s = good_spec();
    s.layer_plan[2].parent = Some("ghost".into());
    strict_fails(&s, "layer \"ghost\" does not exist");

    let mut s = good_spec();
    s.layer_plan[3].mirror_of = Some("ghost".into());
    strict_fails(&s, "mirror_of");

    let mut s = good_spec();
    s.detail_inventory.details[0]
        .maps_to
        .as_mut()
        .unwrap()
        .reference = "ghost".into();
    strict_fails(&s, "maps_to.ref");

    let mut s = good_spec();
    s.layer_plan[0].palette_refs.push("neon".into());
    strict_fails(&s, "neon");

    let mut s = good_spec();
    s.motion_plan[0].target = "ghost".into();
    strict_fails(&s, "neither a layer nor @camera");

    let mut s = good_spec();
    s.motion_plan.push(Motion {
        id: "cam".into(),
        target: "@camera".into(),
        kind: "camera".into(),
        zoom_max: Some(1.03),
        ..Motion::default()
    });
    assert!(validate(&s).ok(true));
    s.motion_plan[3].zoom_max = Some(1.04);
    strict_fails(&s, "zoom_max");
}

#[test]
fn ids_are_unique_and_the_hierarchy_is_acyclic() {
    let mut s = good_spec();
    s.layer_plan[1].id = "sky".into();
    strict_fails(&s, "used twice");
    let mut s = good_spec();
    s.layer_plan[1].parent = Some("chain".into());
    strict_fails(&s, "its own ancestor");
}

#[test]
fn sides_need_a_mirror_partner() {
    let mut s = good_spec();
    s.layer_plan[4].side = Some("l".into());
    strict_fails(&s, "no side");
    let mut s = good_spec();
    s.layer_plan[3].mirror_of = None;
    strict_fails(&s, "mirror_of");
    let mut s = good_spec();
    s.layer_plan[4].mirror_of = Some("arm-l".into());
    assert!(validate(&s).ok(true), "both sides point at each other");
}

#[test]
fn depth_hex_and_passes_are_range_checked() {
    let mut s = good_spec();
    s.layer_plan[0].depth = Some(1.51);
    strict_fails(&s, "depth");
    let mut s = good_spec();
    s.layer_plan[0].depth = Some(1.5);
    assert!(validate(&s).ok(true));
    let mut s = good_spec();
    s.palette.tokens[0].hex = "#12345".into();
    strict_fails(&s, "hex");
    let mut s = good_spec();
    s.build_passes = vec!["shapes".into(), "blockout".into()];
    strict_fails(&s, "build_passes");
    let mut s = good_spec();
    s.build_passes = vec!["blockout".into(), "motion".into(), "audio".into()];
    assert!(validate(&s).ok(true), "a subset in order is allowed");
    s.build_passes.push("glitter".into());
    strict_fails(&s, "build_passes");
}

#[test]
fn the_detail_count_follows_the_tier() {
    let mut s = good_spec();
    s.detail_inventory.details.pop();
    strict_fails(&s, "5 details inventoried, tier moderate needs at least 6");
    s.detail_inventory.details = mapped_details(10, false);
    assert!(validate(&s).ok(true));
    s.complexity.tier = "complex".into();
    s.layer_plan[2].parent = Some("gear".into());
    assert!(validate(&s).ok(true));
    s.detail_inventory.details.pop();
    strict_fails(&s, "needs at least 10");
}

#[test]
fn feature_caps_and_minimums_are_enforced() {
    let mut s = good_spec();
    for i in 0..5 {
        s.feature_targets.push(FeatureTarget {
            pass_ids: vec!["blockout".into()],
            ..target(&format!("c{i}"), "critical", Some(0.8))
        });
    }
    strict_fails(&s, "6 critical features");
    let mut s = good_spec();
    for i in 0..3 {
        s.feature_targets.push(FeatureTarget {
            pass_ids: vec!["palette".into()],
            ..target(&format!("i{i}"), "important", None)
        });
    }
    strict_fails(&s, "4 important features");
    let mut s = good_spec();
    s.feature_targets[0].minimum_score = None;
    strict_fails(&s, "needs a numeric minimum_score");
    let mut s = good_spec();
    s.feature_targets[0].minimum_score = Some(1.5);
    strict_fails(&s, "minimum_score");
}

#[test]
fn motion_rules_gait_stance_ambient_loop_and_the_three_state_loop() {
    let mut s = good_spec();
    s.motion_plan[1].looping = None;
    strict_fails(&s, "must have loop: true");
    s.motion_plan[1].looping = Some(false);
    strict_fails(&s, "must have loop: true");
    let mut s = good_spec();
    s.motion_plan.push(Motion {
        id: "walk".into(),
        target: "gear".into(),
        kind: "gait".into(),
        ..Motion::default()
    });
    strict_fails(&s, "no stance");
    s.motion_plan[3].stance = vec![Stance {
        from_ms: 0,
        to_ms: 400,
    }];
    assert!(validate(&s).ok(true));
    let m: Motion =
        serde_json::from_value(json!({"id": "x", "target": "gear", "loop": null})).unwrap();
    assert_eq!(m.looping, None);
    assert!(serde_json::from_value::<Motion>(json!({"id": "x", "loop": "yes"})).is_err());
    let mut s = good_spec();
    s.layer_plan[1].expects_motion = Some("gesture".into());
    s.motion_plan.remove(0);
    strict_fails(&s, "expects motion");
}

#[test]
fn audio_is_required_unless_the_brief_is_silent() {
    let mut s = good_spec();
    s.audio_plan.tracks.clear();
    strict_fails(&s, "no audio track");
    s.brief.silent = true;
    assert!(validate(&s).ok(true));
    let mut s = good_spec();
    s.audio_plan.cues.push(super::schema::AudioCue {
        sync_to: Some("ghost".into()),
        ..Default::default()
    });
    strict_fails(&s, "sync_to");
}

#[test]
fn unknown_fields_are_kept_and_limits_are_validated() {
    let raw =
        json!({"id": "x", "future_field": {"a": 1}, "limits": {"max_per_pass": 4, "max_total": 2}});
    let s: SceneSpec = serde_json::from_value(raw).unwrap();
    assert_eq!(s.extra["future_field"], json!({"a": 1}));
    assert_eq!(
        serde_json::to_value(&s).unwrap()["future_field"],
        json!({"a": 1})
    );
    strict_fails(&s, "cannot exceed max_total");
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("exports")).unwrap();
    for f in ["sheet.png", "frame-1.png", "frame-2.png"] {
        std::fs::write(dir.path().join("exports").join(f), b"png").unwrap();
    }
    dir
}

fn cont(pass: &str) -> Review {
    let mut r = rev(pass, Action::Continue);
    r.fidelity = Some(0.8);
    r.ai_score = Some(0.8);
    r.layer_scores = super::schema::LAYER_SCORES
        .iter()
        .map(|n| ((*n).to_string(), 0.8))
        .collect();
    r.feature_reviews = scored(&[("hero-silhouette", 0.85), ("palette-fidelity", 0.8)]);
    r.evidence = Evidence {
        contact_sheet: Some("exports/sheet.png".into()),
        frames: vec!["exports/frame-1.png".into(), "exports/frame-2.png".into()],
    };
    r
}

fn refused(spec: &mut SceneSpec, review: Review, dir: &Path) -> String {
    status::record(spec, review, dir).unwrap_err().to_string()
}

#[test]
fn passes_complete_in_order_with_evidence() {
    let dir = project();
    let mut s = good_spec();
    for (i, pass) in ["blockout", "shapes", "palette", "motion", "polish"]
        .iter()
        .enumerate()
    {
        let recorded = status::record(&mut s, cont(pass), dir.path()).unwrap();
        assert!(recorded.completes_pass);
        assert_eq!(completed_prefix(&s, dir.path()), i + 1);
    }
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["current_pass"], "audio");
    assert_eq!(st["next_call"]["tool"], "review_video");
    let audio = rev("audio", Action::Continue);
    assert!(
        status::record(&mut s, audio, dir.path())
            .unwrap()
            .completes_pass
    );
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["complete"], true);
    assert!(st["current_pass"].is_null());
}

#[test]
fn a_continue_with_a_missing_evidence_file_is_refused() {
    let dir = project();
    let mut s = good_spec();
    let mut r = cont("blockout");
    r.evidence.frames.push("exports/missing.png".into());
    assert!(refused(&mut s, r, dir.path()).contains("exports/missing.png does not exist"));
    let mut r = cont("blockout");
    r.evidence.contact_sheet = None;
    assert!(refused(&mut s, r, dir.path()).contains("contact_sheet is missing"));
    let mut r = cont("blockout");
    r.evidence.frames.clear();
    assert!(refused(&mut s, r, dir.path()).contains("no rendered frame"));
    let mut r = cont("blockout");
    r.evidence.contact_sheet = Some("../outside.png".into());
    assert!(refused(&mut s, r, dir.path()).contains("must be exports/"));
    assert!(s.review_history.is_empty());
}

#[test]
fn the_score_the_layers_and_the_features_must_reach_their_thresholds() {
    let dir = project();
    let mut s = good_spec();
    let mut r = cont("blockout");
    r.ai_score = Some(0.69);
    assert!(refused(&mut s, r, dir.path()).contains("below 0.7"));
    let mut r = cont("blockout");
    r.ai_score = Some(0.7);
    assert!(status::record(&mut s, r, dir.path()).is_ok());

    let mut r = cont("shapes");
    r.layer_scores.remove("motion_quality");
    assert!(refused(&mut s, r, dir.path()).contains("motion_quality is missing"));
    let mut r = cont("shapes");
    r.feature_reviews = scored(&[("hero-silhouette", 0.79)]);
    assert!(refused(&mut s, r, dir.path()).contains("scored 0.79, below 0.8"));
    let mut r = cont("shapes");
    r.feature_reviews.clear();
    assert!(refused(&mut s, r, dir.path()).contains("no review entry"));
}

#[test]
fn a_later_pass_cannot_be_continued_while_an_earlier_one_is_incomplete() {
    let dir = project();
    let mut s = good_spec();
    assert!(refused(&mut s, cont("shapes"), dir.path()).contains("ledger disagreement"));
    status::record(&mut s, cont("blockout"), dir.path()).unwrap();
    let e = refused(&mut s, cont("palette"), dir.path());
    assert!(
        e.contains("ledger disagreement") && e.contains("shapes"),
        "{e}"
    );
    assert!(refused(&mut s, cont("blockout"), dir.path()).contains("already complete"));
    assert!(refused(&mut s, cont("sparkle"), dir.path()).contains("not in build_passes"));
}

#[test]
fn a_hand_edited_history_that_jumps_ahead_is_reported() {
    let mut s = good_spec();
    s.review_history = vec![cont("blockout"), cont("palette")];
    let found = ledger_disagreements(&s);
    assert_eq!(found.len(), 1);
    assert!(found[0].contains("continues palette but shapes was never continued"));
    assert!(!validate(&s).errors.is_empty());
    s.review_history = vec![cont("blockout"), cont("shapes")];
    assert!(ledger_disagreements(&s).is_empty());
    let dir = project();
    s.review_history = vec![cont("blockout"), cont("palette")];
    assert_eq!(
        completed_prefix(&s, dir.path()),
        1,
        "the prefix stops at the first gap"
    );
}

#[test]
fn a_third_refine_code_stops_the_run_and_a_fourth_is_refused() {
    let dir = project();
    let mut s = good_spec();
    for _ in 0..2 {
        status::record(&mut s, rev("blockout", Action::RefineCode), dir.path()).unwrap();
    }
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["status"], "active");
    status::record(&mut s, rev("blockout", Action::RefineCode), dir.path()).unwrap();
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["status"], "stopped");
    assert_eq!(
        st["stop_reason"],
        "max-correction-loops-reached:blockout:3/3"
    );
    assert!(st["next_call"].is_null());
    let e = refused(&mut s, rev("blockout", Action::RefineCode), dir.path());
    assert!(
        e.contains("max-correction-loops-reached:blockout:3/3"),
        "{e}"
    );
    assert!(refused(&mut s, cont("blockout"), dir.path()).contains("stopped"));
    assert!(status::record(&mut s, rev("blockout", Action::RequestInput), dir.path()).is_ok());
}

#[test]
fn six_refines_across_three_passes_stop_with_the_total_reason() {
    let dir = project();
    let mut s = good_spec();
    for pass in ["blockout", "shapes", "palette"] {
        for _ in 0..2 {
            status::record(&mut s, rev(pass, Action::RefineSpec), dir.path()).unwrap();
        }
        if pass != "palette" {
            status::record(&mut s, cont(pass), dir.path()).unwrap();
        }
    }
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["stop_reason"], "max-total-correction-loops-reached:6/6");
    assert_eq!(st["loops"]["total"], 6);
}

#[test]
fn limits_chosen_at_the_start_are_honoured() {
    let dir = project();
    let mut s = good_spec();
    s.limits.max_per_pass = 1;
    s.limits.max_total = 2;
    status::record(&mut s, rev("blockout", Action::RefineCode), dir.path()).unwrap();
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(
        st["stop_reason"],
        "max-correction-loops-reached:blockout:1/1"
    );
}

#[test]
fn record_refuses_bad_fields_and_an_invalid_spec() {
    let dir = project();
    let mut s = good_spec();
    for fidelity in [Some(1.2), None] {
        let mut r = rev("blockout", Action::RefineCode);
        r.fidelity = fidelity;
        assert!(status::record(&mut s, r, dir.path()).is_err());
    }
    let mut r = rev("blockout", Action::RefineCode);
    r.summary = " ".into();
    assert!(refused(&mut s, r, dir.path()).contains("summary"));
    let mut r = rev("blockout", Action::RefineCode);
    r.feature_reviews = scored(&[("nope", 0.5)]);
    assert!(refused(&mut s, r, dir.path()).contains("not in feature_targets"));
    let mut r = rev("blockout", Action::RefineCode);
    r.layer_scores.insert("vibes".into(), 0.5);
    assert!(refused(&mut s, r, dir.path()).contains("vibes"));
    let mut bad = good_spec();
    bad.layer_plan[0].depth = Some(9.0);
    assert!(
        refused(&mut bad, rev("blockout", Action::RefineCode), dir.path())
            .contains("not valid under strict")
    );
    assert!(s.review_history.is_empty());
}

#[test]
fn the_audio_pass_needs_no_picture() {
    let dir = project();
    let mut s = good_spec();
    s.build_passes = vec!["audio".into()];
    assert!(
        status::record(&mut s, rev("audio", Action::Continue), dir.path())
            .unwrap()
            .completes_pass
    );
}

#[test]
fn status_names_the_next_call_the_gate_and_what_is_missing() {
    let dir = project();
    let mut s = good_spec();
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["status"], "active");
    assert_eq!(st["current_pass"], "blockout");
    assert_eq!(
        st["next_call"],
        json!({"tool": "scene_check", "arguments": {"video": "v1"}})
    );
    let missing = st["missing_evidence"].to_string();
    assert!(
        missing.contains("contact_sheet") && missing.contains("hero-silhouette"),
        "{missing}"
    );

    status::record(&mut s, rev("blockout", Action::RefineSpec), dir.path()).unwrap();
    assert_eq!(
        status::status(&s, dir.path(), "main")["next_call"]["tool"],
        "scene_spec_write"
    );
    status::record(&mut s, rev("blockout", Action::RefineCode), dir.path()).unwrap();
    assert_eq!(
        status::status(&s, dir.path(), "main")["next_call"]["tool"],
        "scene_check"
    );

    s.build_passes = vec!["motion".into()];
    s.review_history.clear();
    assert_eq!(
        status::status(&s, dir.path(), "main")["next_call"]["tool"],
        "verify_motion"
    );

    let starter = starter("main", "a gear", "moderate");
    let st = status::status(&starter, dir.path(), "main");
    assert_eq!(st["next_call"]["tool"], "scene_spec_validate");
    assert_eq!(st["spec"]["strict_ok"], false);
}

#[test]
fn a_request_input_waits_for_the_user() {
    let dir = project();
    let mut s = good_spec();
    status::record(&mut s, rev("blockout", Action::RequestInput), dir.path()).unwrap();
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["awaiting_input"], true);
    assert!(st["next_call"].is_null());
}

#[test]
fn status_advises_from_the_correction_loop() {
    let dir = project();
    let mut s = good_spec();
    let mut a = rev("blockout", Action::RefineCode);
    a.defect_tags = vec!["wing".into()];
    let b = a.clone();
    status::record(&mut s, a, dir.path()).unwrap();
    status::record(&mut s, b, dir.path()).unwrap();
    let st = status::status(&s, dir.path(), "main");
    assert_eq!(st["advice"]["action"], "refine-spec");
}

#[test]
fn specs_round_trip_atomically_inside_the_project() {
    let dir = project();
    let s = good_spec();
    let path = save(dir.path(), "main", &s).unwrap();
    assert_eq!(path, dir.path().join("scene_specs/main.json"));
    let back = load(dir.path(), "main").unwrap();
    assert_eq!(back.id, "main");
    assert_eq!(back.layer_plan.len(), 5);
    let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("scene_specs"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(leftovers, ["main.json"]);
    assert!(load(dir.path(), "nope").is_err());
}

#[test]
fn names_are_plain_and_the_size_is_capped() {
    for bad in [
        "",
        ".hidden",
        "a/b",
        "..",
        "a..b",
        "UPPER",
        "sp ace",
        &"a".repeat(65),
    ] {
        assert!(valid_name(bad).is_err(), "{bad:?}");
    }
    for good in ["main", "intro-2", "v1.draft_3"] {
        assert!(valid_name(good).is_ok(), "{good}");
    }
    let dir = project();
    let mut s = good_spec();
    s.extra
        .insert("blob".into(), json!("x".repeat(super::MAX_SPEC_BYTES)));
    assert!(save(dir.path(), "big", &s).is_err());
    assert!(!dir.path().join("scene_specs/big.json").exists());
}

#[cfg(unix)]
#[test]
fn a_linked_folder_or_file_is_never_followed() {
    let dir = project();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("scene_specs")).unwrap();
    assert!(save(dir.path(), "main", &good_spec()).is_err());
    assert!(load(dir.path(), "main").is_err());
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);

    let dir = project();
    std::fs::create_dir(dir.path().join("scene_specs")).unwrap();
    let target = outside.path().join("x.json");
    std::fs::write(&target, "{}").unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join("scene_specs/main.json")).unwrap();
    assert!(save(dir.path(), "main", &good_spec()).is_err());
    assert!(load(dir.path(), "main").is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "{}");
}

#[test]
fn an_evidence_link_does_not_count() {
    #[cfg(unix)]
    {
        let dir = project();
        let outside = tempfile::tempdir().unwrap();
        let real = outside.path().join("real.png");
        std::fs::write(&real, b"png").unwrap();
        std::os::unix::fs::symlink(&real, dir.path().join("exports/link.png")).unwrap();
        assert!(status::evidence_problem(dir.path(), "exports/link.png").is_some());
    }
    let dir = project();
    assert!(status::evidence_problem(dir.path(), "exports/sheet.png").is_none());
    assert!(status::evidence_problem(dir.path(), "media/sheet.png").is_some());
    assert!(status::evidence_problem(dir.path(), "exports/sub/sheet.png").is_some());
}

#[test]
fn tool_definitions_match_the_names_and_the_tracks_are_playbooks() {
    let defs = definitions();
    let names: Vec<&str> = defs.iter().filter_map(|d| d["name"].as_str()).collect();
    assert_eq!(names, super::NAMES);
    for (kind, track) in routing::TRACK_BY_KIND {
        assert!(crate::playbook::topic(track).is_some(), "{kind} -> {track}");
    }
    assert!(crate::playbook::topic("build").is_some());
    let targets = good_spec().feature_targets;
    assert_eq!(targets_for(&targets, "palette").len(), 1);
    assert_eq!(targets_for(&targets, "blockout").len(), 1);
}
