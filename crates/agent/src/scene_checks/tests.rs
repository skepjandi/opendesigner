//! Tests for the scene gates: a fixture that passes and a mutation that fails it, for each
//! gate, with the numbers from the source this came from.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, Document, NodeId, Operation, Session};
use serde_json::{Value, json};

use super::geometry::CameraExtremes;
use super::motion_measure::{ExpectArg, GaitArg, Motion, Stance};
use super::structure::{Attach, Detail, LayerEntry, MapsTo};
use super::track_gate::{self, DeclaredLoop};
use super::*;
use crate::video_export::Source;

/// A project with a page and a video.
struct Doc {
    session: Session,
    root: NodeId,
    video: NodeId,
    width: u32,
    height: u32,
}

fn apply(s: &mut Session, ops: Value) -> od_core::TxReceipt {
    let ops: Vec<Operation> = serde_json::from_value(ops).unwrap();
    s.apply(Actor::User, None, &ops).unwrap()
}

fn start(width: u32, height: u32) -> Doc {
    let mut session = Session::new(Document::new("gates"));
    let page = session.add_page("p").unwrap();
    let root = session.document().page(&page).unwrap().root.id.clone();
    apply(
        &mut session,
        json!([{"op": "create_video", "name": "V", "fps": 30, "width": width, "height": height}]),
    );
    let video = session.document().videos[0].id.clone();
    Doc {
        session,
        root,
        video,
        width,
        height,
    }
}

impl Doc {
    fn frame(&mut self, name: &str) -> NodeId {
        let (w, h) = (self.width, self.height);
        apply(
            &mut self.session,
            json!([{"op": "create", "parent": self.root.as_str(), "node":
                {"kind": "frame", "name": name, "width": w, "height": h, "fill": "#101830"}}]),
        )
        .created[0]
            .clone()
    }

    fn node(&mut self, parent: &NodeId, node: Value) -> NodeId {
        apply(
            &mut self.session,
            json!([{"op": "create", "parent": parent.as_str(), "node": node}]),
        )
        .created[0]
            .clone()
    }

    fn rect(&mut self, parent: &NodeId, name: &str, at: (f64, f64, f64, f64)) -> NodeId {
        self.node(
            parent,
            json!({"kind": "rect", "name": name, "x": at.0, "y": at.1,
                "width": at.2, "height": at.3, "fill": "#ff7a45"}),
        )
    }

    fn path(&mut self, parent: &NodeId, name: &str, at: (f64, f64, f64, f64)) -> NodeId {
        self.node(
            parent,
            json!({"kind": "path", "name": name, "x": at.0, "y": at.1,
                "width": at.2, "height": at.3, "fill": "#ff7a45",
                "path": "M0 0 L10 0 L10 10 L0 10 Z"}),
        )
    }

    fn clip(&mut self, frame: &NodeId, ms: u32) {
        apply(
            &mut self.session,
            json!([{"op": "add_clip", "video": self.video.as_str(),
                "frame": frame.as_str(), "duration_ms": ms}]),
        );
    }

    fn animate(&mut self, node: &NodeId, prop: &str, keys: Value, repeat: Option<&str>) {
        let mut op = json!({"op": "animate", "video": self.video.as_str(),
            "node": node.as_str(), "prop": prop, "keys": keys});
        if let Some(r) = repeat {
            op["repeat"] = json!(r);
        }
        apply(&mut self.session, json!([op]));
    }

    fn source(&self) -> Source {
        Source::of_document(self.session.document().clone())
    }

    fn doc(&self) -> Document {
        self.session.document().clone()
    }
}

fn entry(id: &str, node_ref: &str) -> LayerEntry {
    LayerEntry {
        id: id.into(),
        node_ref: Some(node_ref.into()),
        ..LayerEntry::default()
    }
}

fn gate_status(gate: &Gate) -> &'static str {
    gate.status().label()
}

/// The report of `verify_motion` for a project.
fn verify(src: &Source, args: VerifyArgs) -> Value {
    verify_motion(src, args).unwrap()
}

/// The arguments of a plain `verify_motion` call.
fn plain() -> VerifyArgs {
    VerifyArgs {
        video: None,
        samples: None,
        classify: false,
        declared_loops: Vec::new(),
        gait: Vec::new(),
        expect: Vec::new(),
        ambient: Vec::new(),
    }
}

/// A scene with a moving block and a looping cloud.
fn moving_scene() -> (Doc, NodeId, NodeId) {
    let mut d = start(1280, 720);
    let f = d.frame("scene");
    let block = d.rect(&f, "block", (100.0, 100.0, 200.0, 200.0));
    let cloud = d.rect(&f, "cloud", (0.0, 0.0, 300.0, 120.0));
    d.animate(
        &block,
        "x",
        json!([{"at_ms": 0, "value": 100.0}, {"at_ms": 1000, "value": 400.0}]),
        None,
    );
    d.animate(
        &cloud,
        "y",
        json!([{"at_ms": 0, "value": 0.0}, {"at_ms": 1000, "value": 40.0},
               {"at_ms": 2000, "value": 0.0}]),
        Some("loop"),
    );
    d.clip(&f, 6000);
    (d, f, block)
}

#[test]
fn a_track_that_moves_agrees_with_the_pipeline() {
    let (d, _, block) = moving_scene();
    let out = verify(&d.source(), plain());
    assert_eq!(out["status"], json!("pass"), "{out}");
    let tracks = out["tracks"].as_array().unwrap();
    assert_eq!(tracks.len(), 2);
    for t in tracks {
        assert_eq!(t["status"], json!("pass"), "{t}");
        assert!(t["samples"].as_u64().unwrap() >= track_gate::MIN_SAMPLES as u64);
        let delta = t["max_delta"].as_f64().unwrap();
        assert!(
            delta <= track_gate::BINDING_EPSILON + track_gate::PIPELINE_QUANTUM,
            "{t}"
        );
    }
    assert_eq!(tracks[0]["node"], json!(block.as_str()));
    assert_eq!(tracks[0]["prop"], json!("x"));
}

#[test]
fn the_binding_numbers_follow_the_source() {
    let eps = track_gate::BINDING_EPSILON;
    assert!(track_gate::judge_binding(5, eps, 1.0, 0.0).is_ok());
    assert!(track_gate::judge_binding(5, eps * 2.0, 1.0, 0.0).is_err());
    assert!(track_gate::judge_binding(3, 1e-12, 1.0, 0.0).is_err());
    assert!(track_gate::judge_binding(5, 1e-12, 1.0, 0.0).is_ok());
}

#[test]
fn a_track_that_drives_nothing_is_caught() {
    let (d, _, _) = moving_scene();
    let ghost = NodeId::parse("n_9999").unwrap();
    let mut src = d.source();
    src.doc.videos[0].animations.push(od_core::Animation {
        node: ghost,
        prop: od_core::AnimProp::X,
        keys: vec![
            od_core::Key {
                at_ms: 0,
                value: 10.0,
                ease: od_core::Ease::default(),
            },
            od_core::Key {
                at_ms: 1000,
                value: 20.0,
                ease: od_core::Ease::default(),
            },
        ],
        repeat: od_core::Repeat::Once,
    });
    let out = verify(&src, plain());
    assert_eq!(out["status"], json!("fail"), "{out}");
    let errors = out["findings"]["RV020"]["errors"].as_array().unwrap();
    assert!(!errors.is_empty(), "{out}");
    assert!(
        errors[0].as_str().unwrap().contains("does not exist"),
        "{out}"
    );
}

#[test]
fn a_track_whose_keys_hold_one_value_only_warns() {
    let (mut d, f, _) = moving_scene();
    let still = d.rect(&f, "still", (400.0, 400.0, 100.0, 100.0));
    d.animate(
        &still,
        "opacity",
        json!([{"at_ms": 0, "value": 1.0}, {"at_ms": 1000, "value": 1.0}]),
        None,
    );
    let out = verify(&d.source(), plain());
    assert_eq!(out["status"], json!("pass"), "{out}");
    let warnings = out["findings"]["RV020"]["warnings"].as_array().unwrap();
    assert!(
        warnings
            .iter()
            .any(|w| w.as_str().unwrap().contains("same value")),
        "{out}"
    );
}

#[test]
fn a_track_that_starts_after_the_video_fails() {
    let (mut d, f, _) = moving_scene();
    let late = d.rect(&f, "late", (0.0, 500.0, 100.0, 100.0));
    d.animate(
        &late,
        "x",
        json!([{"at_ms": 9000, "value": 10.0}, {"at_ms": 9500, "value": 20.0}]),
        None,
    );
    let out = verify(&d.source(), plain());
    assert_eq!(out["status"], json!("fail"), "{out}");
    assert!(
        out["findings"]["RV020"]["errors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_str().unwrap().contains("after the end")),
        "{out}"
    );
}

#[test]
fn the_loop_rule_follows_the_numbers() {
    // A rotation that comes back within half a degree loops; half a degree more does not.
    let loop_ok = track_gate::seam(od_core::AnimProp::Rotation, 0.0, 0.4, 720.0);
    assert!(loop_ok.ok, "{loop_ok:?}");
    let loop_no = track_gate::seam(od_core::AnimProp::Rotation, 0.0, 0.6, 720.0);
    assert!(!loop_no.ok, "{loop_no:?}");
    // A position that comes back within a hundredth of the frame height loops.
    let back = track_gate::seam(od_core::AnimProp::X, 0.0, 0.01 * 720.0, 720.0);
    assert!(back.ok, "{back:?}");
    let away = track_gate::seam(od_core::AnimProp::X, 0.0, 0.0101 * 720.0, 720.0);
    assert!(!away.ok, "{away:?}");
}

#[test]
fn the_legacy_loop_rule_disagrees_and_the_new_one_agrees() {
    // The old rule: a clip that neither travels nor rises can repeat seamlessly.
    assert!(!track_gate::legacy_loop_rule(0.121, 0.01));
    assert!(track_gate::legacy_loop_rule(0.013, 0.0));
    // The new rule: the last pose returns to the first.
    assert_eq!(track_gate::loop_rule(Some(0.2), Some(0.001)), Some(true));
    assert_eq!(track_gate::loop_rule(Some(0.2), Some(0.121)), Some(false));
    assert_eq!(track_gate::loop_rule(None, Some(0.0)), None);
    assert_eq!(track_gate::loop_rule(Some(0.0), None), None);
}

#[test]
fn a_looping_track_that_does_not_come_back_fails() {
    let (mut d, f, _) = moving_scene();
    let wheel = d.rect(&f, "wheel", (600.0, 300.0, 80.0, 80.0));
    d.animate(
        &wheel,
        "rotation",
        json!([{"at_ms": 0, "value": 0.0}, {"at_ms": 1000, "value": 35.0}]),
        Some("loop"),
    );
    let out = verify(&d.source(), plain());
    let g3 = out["gates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["code"] == json!("G3"))
        .unwrap()
        .clone();
    assert_eq!(g3["status"], json!("fail"), "{g3}");
}

#[test]
fn a_loop_with_an_unknown_seam_is_unevaluated() {
    let (mut d, f, _) = moving_scene();
    let swing = d.rect(&f, "swing", (600.0, 100.0, 60.0, 60.0));
    d.animate(
        &swing,
        "x",
        json!([{"at_ms": 0, "value": 0.0}, {"at_ms": 1000, "value": 100.0}]),
        None,
    );
    let out = verify(
        &d.source(),
        VerifyArgs {
            declared_loops: vec![DeclaredLoop {
                node: swing.as_str().to_string(),
                prop: od_core::AnimProp::X,
                looping: None,
            }],
            ..plain()
        },
    );
    let g3 = out["gates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["code"] == json!("G3"))
        .unwrap()
        .clone();
    assert_eq!(g3["status"], json!("unevaluated"), "{g3}");
    assert!(g3["reason"].as_str().unwrap().contains("not known"), "{g3}");
}

fn motion(travel: f64, rise: f64, speed: f64, foot: f64, hand: f64) -> Motion {
    Motion {
        node: NodeId::parse("n_1").unwrap(),
        duration_s: travel / speed.max(1e-9),
        travel,
        rise,
        speed,
        rot_range: 0.0,
        scale_delta: 0.0,
        pose_return: 0.0,
        return_h: 0.0,
        foot_range: foot,
        hand_range: hand,
        has_feet: true,
        has_hands: true,
        labels: Vec::new(),
        primary: "in-place",
        intent_inferred: false,
        ambient: false,
    }
}

#[test]
fn the_classifier_names_the_motion() {
    let (labels, primary) = motion_measure::classify(&motion(1.133, 0.019, 0.400, 0.2, 0.1));
    assert_eq!(primary, "walk");
    assert!(labels.contains(&"walk"), "{labels:?}");
    let (labels, primary) = motion_measure::classify(&motion(1.465, 0.021, 0.799, 0.2, 0.1));
    assert_eq!(primary, "run");
    assert!(labels.contains(&"run"), "{labels:?}");
    let (labels, primary) = motion_measure::classify(&motion(2.906, 0.01, 2.250, 0.2, 0.1));
    assert_eq!(primary, "dash");
    assert!(labels.contains(&"dash"), "{labels:?}");
    let (labels, primary) = motion_measure::classify(&motion(0.272, 0.248, 0.5, 0.2, 0.1));
    assert_eq!(primary, "jump");
    assert!(
        labels.contains(&"jump") && labels.contains(&"in-place"),
        "{labels:?}"
    );
    let (labels, primary) = motion_measure::classify(&motion(2.826, 0.517, 2.250, 0.2, 0.1));
    assert_eq!(primary, "leap");
    assert!(
        labels.contains(&"leap") && labels.contains(&"dash"),
        "{labels:?}"
    );
    let (labels, primary) = motion_measure::classify(&motion(0.013, 0.0001, 0.01, 0.05, 0.01));
    assert_eq!(primary, "idle");
    assert!(labels.contains(&"idle"), "{labels:?}");
    let (labels, primary) = motion_measure::classify(&motion(0.050, 0.008, 0.1, 0.05, 0.45));
    assert_eq!(primary, "in-place");
    assert_eq!(
        labels,
        vec!["in-place", "planted", "gesture"],
        "the arms-only clip carries exactly these three"
    );
}

#[test]
fn the_classifier_reads_the_same_at_two_sizes() {
    // The same motion in a picture twice as big reads the same: every length is a fraction of
    // the frame height.
    let small = motion_measure::classify(&motion(1.133, 0.019, 0.400, 0.2, 0.1));
    let big = motion_measure::classify(&motion(2.266, 0.038, 0.400, 0.4, 0.2));
    assert_eq!(small.0, big.0);
    assert_eq!(small.1, big.1);
}

#[test]
fn a_moving_node_is_measured_and_named() {
    let (d, _, block) = moving_scene();
    let out = verify(
        &d.source(),
        VerifyArgs {
            classify: true,
            ..plain()
        },
    );
    let motion = out["motion"].as_array().unwrap();
    let block_motion = motion
        .iter()
        .find(|m| m["node"] == json!(block.as_str()))
        .expect("the block is measured");
    let travel = block_motion["travel_h"].as_f64().unwrap();
    assert!(travel > 0.3, "{block_motion}");
    assert!(block_motion["speed_h_per_s"].as_f64().unwrap() > 0.0);
    assert_eq!(block_motion["labels"], json!(["walk"]));
    assert_eq!(block_motion["primary"], json!("walk"));
}

#[test]
fn ambient_life_that_is_too_big_warns() {
    let (mut d, f, _) = moving_scene();
    let cloud = d.rect(&f, "cloud2", (0.0, 400.0, 300.0, 120.0));
    d.animate(
        &cloud,
        "y",
        json!([{"at_ms": 0, "value": 400.0}, {"at_ms": 1000, "value": 460.0},
               {"at_ms": 2000, "value": 400.0}]),
        Some("loop"),
    );
    let out = verify(
        &d.source(),
        VerifyArgs {
            classify: true,
            ..plain()
        },
    );
    let warnings: Vec<String> = out["motion"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|m| m["warnings"].as_array().unwrap().clone())
        .map(|w| w.as_str().unwrap().to_string())
        .collect();
    assert!(
        warnings.iter().any(|w| w.contains("ambient-too-large")),
        "{warnings:?}"
    );
}

#[test]
fn a_foot_that_slides_fails_and_one_that_holds_passes() {
    let (mut d, f, _) = moving_scene();
    // A foot that slides 0.011 of the frame height: over the limit.
    let foot = d.rect(&f, "foot", (500.0, 600.0, 40.0, 40.0));
    d.animate(
        &foot,
        "x",
        json!([{"at_ms": 0, "value": 500.0}, {"at_ms": 400, "value": 507.92}]),
        None,
    );
    // A foot that slides 0.010 of the frame height: within the limit.
    let hold = d.rect(&f, "foot2", (600.0, 600.0, 40.0, 40.0));
    d.animate(
        &hold,
        "x",
        json!([{"at_ms": 0, "value": 600.0}, {"at_ms": 400, "value": 607.2}]),
        None,
    );
    let doc = d.doc();
    let video = doc.videos[0].clone();
    let gate = motion_measure::gait_gate(
        &doc,
        &video,
        &[
            GaitArg {
                node: foot.as_str().to_string(),
                stance: vec![Stance {
                    from_ms: 0.0,
                    to_ms: 400.0,
                }],
                character_height: None,
                ground: None,
            },
            GaitArg {
                node: hold.as_str().to_string(),
                stance: vec![Stance {
                    from_ms: 0.0,
                    to_ms: 400.0,
                }],
                character_height: None,
                ground: None,
            },
        ],
    );
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(
        gate.failures.iter().any(|f| f.contains("slides")),
        "{gate:?}"
    );
}

#[test]
fn a_gait_without_a_stance_is_unevaluated() {
    let (d, _, block) = moving_scene();
    let doc = d.doc();
    let video = doc.videos[0].clone();
    let gate = motion_measure::gait_gate(
        &doc,
        &video,
        &[GaitArg {
            node: block.as_str().to_string(),
            stance: Vec::new(),
            character_height: None,
            ground: None,
        }],
    );
    assert!(
        matches!(gate.status(), GateStatus::Unevaluated(_)),
        "{gate:?}"
    );
}

#[test]
fn two_sides_must_be_a_reflection() {
    let mut d = start(1280, 720);
    let f = d.frame("scene");
    let left = d.path(&f, "wing-l", (100.0, 100.0, 100.0, 100.0));
    let right = d.path(&f, "wing-r", (300.0, 100.0, 100.0, 100.0));
    d.clip(&f, 4000);
    let mut doc = d.doc();
    let video = doc.videos[0].clone();
    {
        let loc = doc.locate(&right).unwrap();
        doc.node_at_mut(&loc).unwrap().rotation = 180.0;
    }
    let mut plan = vec![
        LayerEntry {
            id: "arm-l".into(),
            node_ref: Some(left.as_str().into()),
            side: Some("l".into()),
            side_source: Some("reference".into()),
            facing: Some("front".into()),
            ..LayerEntry::default()
        },
        LayerEntry {
            id: "arm-r".into(),
            node_ref: Some(right.as_str().into()),
            side: Some("r".into()),
            side_source: Some("reference".into()),
            facing: Some("front".into()),
            ..LayerEntry::default()
        },
    ];
    // The same shape twice, turned half a round: a rotation, not a reflection.
    let gate = structure::chirality_gate(&doc, &video, Some(&plan));
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(
        gate.failures.iter().any(|f| f.contains("rotation")),
        "{gate:?}"
    );

    // The right one mirrored: a reflection.
    let loc = doc.locate(&right).unwrap();
    let node = doc.node_at_mut(&loc).unwrap();
    node.rotation = 0.0;
    node.scale_x = -1.0;
    let doc = doc;
    let gate = structure::chirality_gate(&doc, &video, Some(&plan));
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(
        gate.failures.iter().any(|f| f.contains("viewer")),
        "the sign rule: {gate:?}"
    );

    // The sides assigned by convention: the sign was not measured.
    plan[0].side_source = Some("convention".into());
    let gate = structure::chirality_gate(&doc, &video, Some(&plan));
    assert!(
        matches!(gate.status(), GateStatus::Unevaluated(_)),
        "{gate:?}"
    );

    // Two left sides: no pair to look at.
    plan[1].side = Some("l".into());
    plan[1].side_source = Some("reference".into());
    let gate = structure::chirality_gate(&doc, &video, Some(&plan));
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
}

#[test]
fn the_part_coverage_gate_finds_what_is_missing_and_what_is_extra() {
    let mut d = start(1280, 720);
    let f = d.frame("scene");
    let wing_l = d.path(&f, "wing-l", (100.0, 100.0, 100.0, 100.0));
    let spark = d.path(&f, "spark", (400.0, 100.0, 40.0, 40.0));
    d.clip(&f, 4000);
    let doc = d.doc();
    let video = doc.videos[0].clone();
    let plan = vec![entry("wing-l", wing_l.as_str()), entry("wing-r", "wing-r")];
    let gate = structure::coverage_gate(&doc, &video, Some(&plan), None);
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(
        gate.failures.iter().any(|f| f.contains("wing-r")),
        "{gate:?}"
    );
    assert!(
        gate.warnings.iter().any(|w| w.contains(spark.as_str())),
        "{gate:?}"
    );

    // Two plan entries naming one node: two layers are one.
    let plan = vec![
        entry("wing-l", wing_l.as_str()),
        entry("wing-r", wing_l.as_str()),
    ];
    let gate = structure::coverage_gate(&doc, &video, Some(&plan), None);
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(
        gate.failures.iter().any(|f| f.contains("same node")),
        "{gate:?}"
    );

    // A detail that maps to nothing warns.
    let details = vec![Detail {
        id: "d1".into(),
        maps_to: Some(MapsTo {
            r#type: Some("node".into()),
            reference: Some("n_9999".into()),
        }),
    }];
    let gate = structure::coverage_gate(&doc, &video, Some(&plan), Some(&details));
    assert!(gate.warnings.iter().any(|w| w.contains("d1")), "{gate:?}");
}

#[test]
fn the_freeze_gate_catches_a_changed_shape() {
    let mut d = start(1280, 720);
    let f = d.frame("scene");
    let wing = d.path(&f, "wing", (100.0, 100.0, 100.0, 100.0));
    let other = d.rect(&f, "other", (300.0, 100.0, 100.0, 100.0));
    d.clip(&f, 4000);
    let doc = d.doc();
    let video = doc.videos[0].clone();
    let freeze = structure::snapshot(&doc, &video, &Default::default());
    let frozen = structure::Freeze {
        hashes: freeze.hashes.clone(),
        attributes: freeze.attributes.clone(),
    };

    // The same document changes nothing.
    let gate = structure::freeze_gate(&doc, &video, &frozen, &Default::default());
    assert_eq!(gate.status(), GateStatus::Pass, "{gate:?}");

    // A path changed.
    let mut changed = doc.clone();
    let loc = changed.locate(&wing).unwrap();
    changed.node_at_mut(&loc).unwrap().path = Some("M0 0 L5 0 L5 5 Z".into());
    let gate = structure::freeze_gate(&changed, &video, &frozen, &Default::default());
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(gate.failures.iter().any(|f| f.contains("path")), "{gate:?}");

    // A position changed in the last decimal only.
    let mut changed = doc.clone();
    let loc = changed.locate(&wing).unwrap();
    let node = changed.node_at_mut(&loc).unwrap();
    node.x = f64::from_bits(node.x.to_bits() + 1);
    let gate = structure::freeze_gate(&changed, &video, &frozen, &Default::default());
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(gate.failures.iter().any(|f| f.contains("x")), "{gate:?}");

    // A node is gone.
    let mut changed = doc.clone();
    let mut parent_loc = changed.locate(&wing).unwrap();
    parent_loc.path.pop();
    changed
        .node_at_mut(&parent_loc)
        .unwrap()
        .children
        .retain(|c| c.id != wing);
    let gate = structure::freeze_gate(&changed, &video, &frozen, &Default::default());
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(gate.failures.iter().any(|f| f.contains("gone")), "{gate:?}");

    // The children of a node are in another order.
    let mut changed = doc.clone();
    let loc = changed.locate(&f).unwrap();
    let children = &mut changed.node_at_mut(&loc).unwrap().children;
    children.reverse();
    let gate = structure::freeze_gate(&changed, &video, &frozen, &Default::default());
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(
        gate.failures.iter().any(|f| f.contains("children")),
        "{gate:?}"
    );

    // A track a later pass added changes nothing.
    let mut added = doc.clone();
    added.videos[0].animations.push(od_core::Animation {
        node: other.clone(),
        prop: od_core::AnimProp::Opacity,
        keys: vec![
            od_core::Key {
                at_ms: 0,
                value: 1.0,
                ease: od_core::Ease::default(),
            },
            od_core::Key {
                at_ms: 1000,
                value: 0.5,
                ease: od_core::Ease::default(),
            },
        ],
        repeat: od_core::Repeat::Once,
    });
    let gate = structure::freeze_gate(&added, &video, &frozen, &Default::default());
    assert_eq!(gate.status(), GateStatus::Pass, "{gate:?}");

    // A node a later pass added, and named, changes nothing either.
    let polish = d.rect(&f, "polish", (0.0, 0.0, 10.0, 10.0));
    let after = d.doc();
    let mut added_by = std::collections::BTreeMap::new();
    added_by.insert(polish.as_str().to_string(), "polish".to_string());
    let gate = structure::freeze_gate(&after, &video, &frozen, &added_by);
    assert_eq!(gate.status(), GateStatus::Pass, "{gate:?}");
}

#[test]
fn a_bleeding_layer_must_cover_the_frame() {
    let mut d = start(1280, 720);
    let f = d.frame("scene");
    let sky = d.node(
        &f,
        json!({"kind": "rect", "name": "sky", "x": 0, "y": 0, "width": 1280, "height": 720,
            "depth": 0.5, "fill": "#ffffff"}),
    );
    d.clip(&f, 4000);
    let doc = d.doc();
    let video = doc.videos[0].clone();
    let plan = vec![LayerEntry {
        id: "sky".into(),
        node_ref: Some(sky.as_str().into()),
        bleed: true,
        ..LayerEntry::default()
    }];
    let camera = CameraExtremes {
        x_max_px: Some(120.0),
        ..CameraExtremes::default()
    };
    let gate = geometry::bleed_gate(&doc, &video, Some(&plan), Some(&camera));
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");
    assert!(
        gate.failures.iter().any(|f| f.contains("short")),
        "{gate:?}"
    );

    // The sky reaches further than the camera can pan.
    let mut wide = doc.clone();
    let loc = wide.locate(&sky).unwrap();
    let node = wide.node_at_mut(&loc).unwrap();
    node.x = -60.0;
    node.width = 1400.0;
    let gate = geometry::bleed_gate(&wide, &video, Some(&plan), Some(&camera));
    assert_eq!(gate.status(), GateStatus::Pass, "{gate:?}");
}

#[test]
fn the_bleed_gate_agrees_with_the_renderer() {
    // The gate says the layer does not cover the frame; the picture shows the frame's own
    // background where it does not.
    let mut d = start(1280, 720);
    let f = d.frame("scene");
    let sky = d.node(
        &f,
        json!({"kind": "rect", "name": "sky", "x": 0, "y": 0, "width": 1280, "height": 720,
            "depth": 0.5, "fill": "#ffffff"}),
    );
    d.clip(&f, 4000);
    let doc = d.doc();
    let camera = od_core::Camera {
        x: 120.0,
        ..od_core::Camera::default()
    };
    let background = black_pixels(&doc, &f, &camera);
    assert!(
        background > 0,
        "the background shows where the layer is short"
    );

    let mut wide = doc.clone();
    let loc = wide.locate(&sky).unwrap();
    let node = wide.node_at_mut(&loc).unwrap();
    node.x = -60.0;
    node.width = 1400.0;
    let background = black_pixels(&wide, &f, &camera);
    assert_eq!(background, 0, "the layer covers the frame");
}

/// How many pixels of the frame's own background the camera leaves showing.
fn black_pixels(doc: &Document, frame: &NodeId, camera: &od_core::Camera) -> usize {
    let mut media = od_render::Media::default();
    let rendered = od_render::render_frame_camera(doc, frame, 0.25, None, &mut media, *camera)
        .expect("the frame draws");
    rendered
        .pixmap
        .data()
        .chunks(4)
        .filter(|p| p[0] < 40 && p[1] < 40 && p[2] < 60)
        .count()
}

#[test]
fn the_gates_read_the_same_at_two_sizes() {
    let verdicts = |w: u32, h: u32| {
        let mut d = start(w, h);
        let f = d.frame("scene");
        let sky = d.node(
            &f,
            json!({"kind": "rect", "name": "sky", "x": 0, "y": 0,
                "width": w, "height": h, "depth": 0.5, "fill": "#ffffff"}),
        );
        let wing = d.path(&f, "wing-l", (100.0, 100.0, 100.0, 100.0));
        d.animate(
            &wing,
            "x",
            json!([{"at_ms": 0, "value": 100.0}, {"at_ms": 2000, "value": 400.0}]),
            None,
        );
        d.clip(&f, 4000);
        let doc = d.doc();
        let video = doc.videos[0].clone();
        let plan = vec![
            LayerEntry {
                id: "sky".into(),
                node_ref: Some(sky.as_str().into()),
                bleed: true,
                ..LayerEntry::default()
            },
            LayerEntry {
                id: "wing-l".into(),
                node_ref: Some(wing.as_str().into()),
                side: Some("l".into()),
                side_source: Some("reference".into()),
                ..LayerEntry::default()
            },
        ];
        let camera = CameraExtremes {
            x_max_px: Some(f64::from(w) * 0.09375),
            ..CameraExtremes::default()
        };
        vec![
            gate_status(&geometry::bleed_gate(
                &doc,
                &video,
                Some(&plan),
                Some(&camera),
            )),
            gate_status(&structure::chirality_gate(&doc, &video, Some(&plan))),
            gate_status(&structure::coverage_gate(&doc, &video, Some(&plan), None)),
        ]
    };
    let small = verdicts(1280, 720);
    let big = verdicts(2560, 1440);
    assert_eq!(small, big, "the same scene at two sizes reads the same");
}

#[test]
fn a_child_must_stay_attached_to_its_parent() {
    let mut d = start(1280, 720);
    let f = d.frame("scene");
    let body = d.rect(&f, "body", (100.0, 100.0, 200.0, 200.0));
    let arm = d.rect(&f, "arm", (300.0, 150.0, 100.0, 40.0));
    d.animate(
        &arm,
        "x",
        json!([{"at_ms": 0, "value": 300.0}, {"at_ms": 2000, "value": 900.0}]),
        None,
    );
    d.clip(&f, 4000);
    let doc = d.doc();
    let video = doc.videos[0].clone();
    let plan = vec![
        LayerEntry {
            id: "body".into(),
            node_ref: Some(body.as_str().into()),
            ..LayerEntry::default()
        },
        LayerEntry {
            id: "arm".into(),
            node_ref: Some(arm.as_str().into()),
            parent: Some("body".into()),
            attach: Some(Attach {
                gap_tolerance_px: Some(2.0),
                contact: Some("overlap".into()),
            }),
            ..LayerEntry::default()
        },
    ];
    let gate = geometry::attach_gate(&doc, &video, Some(&plan));
    assert_eq!(gate.status(), GateStatus::Fail, "{gate:?}");

    let mut close = doc.clone();
    {
        let loc = close.locate(&arm).unwrap();
        let node = close.node_at_mut(&loc).unwrap();
        node.x = 295.0;
    }
    for a in close.videos[0]
        .animations
        .iter_mut()
        .filter(|a| a.node == arm)
    {
        for k in a.keys.iter_mut() {
            k.value = 295.0;
        }
    }
    let close_video = close.videos[0].clone();
    let gate = geometry::attach_gate(&close, &close_video, Some(&plan));
    assert_eq!(gate.status(), GateStatus::Pass, "{gate:?}");
}

#[test]
fn the_hit_test_lists_the_nodes_under_a_point() {
    let (d, f, block) = moving_scene();
    let doc = d.doc();
    let video = doc.videos[0].clone();
    let out = geometry::hit_list(&doc, &video, 150.0, 150.0, 0.0).unwrap();
    let nodes = out["nodes"].as_array().unwrap();
    assert_eq!(nodes[0], json!(block.as_str()));
    let empty = geometry::hit_list(&doc, &video, 1200.0, 700.0, 0.0).unwrap();
    assert_eq!(empty["nodes"], json!([f.as_str()]));
}

#[test]
fn the_footprint_counts_the_pixels_a_node_changes() {
    let (d, _, block) = moving_scene();
    let src = d.source();
    let video = src.doc.videos[0].clone();
    let out = geometry::footprint(
        &src,
        &video,
        &geometry::FootprintArgs {
            node: block.as_str().to_string(),
            t_ms: Some(0.0),
        },
    )
    .unwrap();
    assert!(out["pixels"].as_u64().unwrap() > 0, "{out}");
    let empty = geometry::footprint(
        &src,
        &video,
        &geometry::FootprintArgs {
            node: "n_9999".to_string(),
            t_ms: Some(0.0),
        },
    );
    assert!(empty.is_err());
}

#[test]
fn the_tools_are_listed_and_read_only() {
    let defs = definitions();
    let names: Vec<&str> = defs.iter().map(|d| d["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["verify_motion", "scene_check", "hit_test"]);
    for d in &defs {
        assert_eq!(d["annotations"]["readOnlyHint"], json!(true));
    }
    let all = crate::tools::definitions();
    for name in names {
        assert!(all.iter().any(|d| d["name"] == json!(name)), "{name}");
    }
}

#[test]
fn the_planned_class_must_be_the_measured_one() {
    let (d, _, block) = moving_scene();
    let out = verify(
        &d.source(),
        VerifyArgs {
            classify: true,
            expect: vec![ExpectArg {
                node: block.as_str().to_string(),
                class: "run".into(),
            }],
            ..plain()
        },
    );
    assert_eq!(out["status"], json!("fail"), "{out}");
    let g = out["gates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["code"] == json!("motion"))
        .unwrap()
        .clone();
    assert!(g["reason"].as_str().unwrap().contains("run"), "{g}");
}
