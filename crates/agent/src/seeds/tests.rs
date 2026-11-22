#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::recipes::SEEDS;
use crate::tools::{CallCtx, Content, Turns};

struct Fixture {
    _dir: TempDir,
    ws: Workspace,
    turns: Turns,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&dir.path().join("p.od"), "Seeds").unwrap();
    Fixture {
        _dir: dir,
        ws,
        turns: Turns::default(),
    }
}

impl Fixture {
    fn call(&mut self, name: &str, args: Value) -> Result<Value, String> {
        let mut ctx = CallCtx {
            ws: &mut self.ws,
            actor: Actor::agent("tester").unwrap(),
            turns: &mut self.turns,
            extra_servers: &[],
        };
        match crate::tools::call(&mut ctx, name, args) {
            Ok(out) => match out.content.first() {
                Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
                _ => panic!("no text"),
            },
            Err(e) => Err(e.0),
        }
    }

    fn frames(&self) -> usize {
        self.ws.session.document().pages[0].root.children.len()
    }
}

fn collect_text(n: &od_core::Node, out: &mut Vec<String>) {
    out.extend(n.text.clone());
    for c in &n.children {
        collect_text(c, out);
    }
}

#[test]
fn list_seeds_names_all_six_with_beats_and_layouts() {
    let mut f = fixture();
    let v = f.call("list_seeds", json!({})).unwrap();
    assert_eq!(v["count"], 6);
    for s in v["seeds"].as_array().unwrap() {
        assert!(s["beats"].as_array().unwrap().len() >= 3, "{}", s["id"]);
        assert!(s["layouts"].as_str().unwrap().len() > 200, "{}", s["id"]);
        assert!(!s["blocks"].as_array().unwrap().is_empty());
    }
}

#[test]
fn every_seed_applies_and_passes_the_first_gates() {
    for seed in SEEDS {
        let mut f = fixture();
        let out = f
            .call("apply_seed", json!({"id": seed.id}))
            .unwrap_or_else(|e| panic!("{}: {e}", seed.id));
        let spec = out["spec"].as_str().unwrap();
        assert_eq!(spec, format!("scene_specs/{}.json", seed.id));
        assert!(!out["beat_times"].as_object().unwrap().is_empty());
        assert!(out["layouts"].as_str().unwrap().starts_with("# "));

        let lint = f
            .call("validate_document", json!({"min_severity": "error"}))
            .unwrap();
        assert_eq!(lint["errors"], 0, "{}: {}", seed.id, lint["findings"]);

        let check = f
            .call("scene_spec_validate", json!({"name": seed.id}))
            .unwrap();
        assert_eq!(check["ok"], true, "{}: {}", seed.id, check["errors"]);
        assert_eq!(check["errors"].as_array().unwrap().len(), 0);

        let review = f
            .call(
                "review_video",
                json!({"video": out["video"], "allow_unevaluated": true}),
            )
            .unwrap();
        let codes: Vec<&str> = review["findings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["code"].as_str().unwrap())
            .collect();
        for banned in ["RV001", "RV002"] {
            assert!(
                !codes.contains(&banned),
                "{}: {banned} in {codes:?}",
                seed.id
            );
        }

        let status = f.call("scene_status", json!({"name": seed.id})).unwrap();
        assert!(
            status["next_beat"].is_null(),
            "{}: a seed builds every beat",
            seed.id
        );
        let gates = out["p0_gates"].as_array().unwrap();
        assert!(gates.iter().any(|g| g["tool"] == "review_video"));
        assert_eq!(gates[0]["status"], "pass");
        assert_eq!(gates[1]["status"], "pass");
        assert_eq!(gates[2]["status"], "pass", "{}: {}", seed.id, gates[2]);
    }
}

#[test]
fn a_seed_spec_has_a_beat_map_that_validates_and_names_real_layers() {
    for seed in SEEDS {
        let mut f = fixture();
        f.call("apply_seed", json!({"id": seed.id})).unwrap();
        let v = f
            .call(
                "scene_spec_validate",
                json!({"name": seed.id, "include_spec": true}),
            )
            .unwrap();
        let spec = &v["spec"];
        assert_eq!(spec["seed"], seed.id);
        let beats = spec["beat_map"].as_array().unwrap();
        assert!(beats.len() >= 3);
        let layers: Vec<&str> = spec["layer_plan"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["id"].as_str().unwrap())
            .collect();
        for b in beats {
            assert!(
                layers.contains(&b["focal_node"].as_str().unwrap()),
                "{} {b}",
                seed.id
            );
        }
        assert_eq!(spec["brief"]["duration_ms"], seed.duration_ms);
    }
}

#[test]
fn applying_twice_does_not_collide() {
    let mut f = fixture();
    let a = f
        .call("apply_seed", json!({"id": "explainer-opener"}))
        .unwrap();
    let b = f
        .call("apply_seed", json!({"id": "explainer-opener"}))
        .unwrap();
    assert_ne!(a["frame"], b["frame"]);
    assert_ne!(a["video"], b["video"]);
    assert_eq!(a["spec"], "scene_specs/explainer-opener.json");
    assert_eq!(b["spec"], "scene_specs/explainer-opener-2.json");
    assert_eq!(f.frames(), 2);
    let doc = f.ws.session.document();
    let frames = &doc.pages[0].root.children;
    assert!(
        frames[1].x >= frames[0].x + frames[0].width,
        "the second frame sits beside the first"
    );
    assert_eq!(doc.videos.len(), 2);
    let lint = f
        .call("validate_document", json!({"min_severity": "error"}))
        .unwrap();
    assert_eq!(lint["errors"], 0, "{}", lint["findings"]);
    let one = f
        .call("scene_spec_validate", json!({"name": "explainer-opener-2"}))
        .unwrap();
    assert_eq!(one["ok"], true);
}

#[test]
fn a_seed_is_one_turn_and_one_checkpoint_removes_it() {
    let mut f = fixture();
    f.call("apply_seed", json!({"id": "product-reveal"}))
        .unwrap();
    let doc = f.ws.session.document();
    assert_eq!(doc.pages[0].root.children.len(), 1);
    assert_eq!(doc.videos.len(), 1);
    let turns: std::collections::BTreeSet<Option<String>> =
        f.ws.session.log().iter().map(|t| t.turn.clone()).collect();
    assert_eq!(turns.len(), 1, "{turns:?}");
    assert!(
        turns
            .iter()
            .next()
            .unwrap()
            .as_deref()
            .unwrap()
            .starts_with("seed-product-reveal")
    );
    let before = f.ws.session.checkpoints()[0].id;
    f.call("restore_checkpoint", json!({"checkpoint": before}))
        .unwrap();
    let doc = f.ws.session.document();
    assert!(doc.pages[0].root.children.is_empty());
    assert!(doc.videos.is_empty());
}

#[test]
fn brand_copy_and_colors_land_and_a_style_pack_palette_is_used() {
    let mut f = fixture();
    f.call(
        "apply_style_pack",
        json!({"id": crate::style_pack::PACKS[0].id}),
    )
    .unwrap();
    let out = f
        .call("apply_seed", json!({"id": "logo-sting", "brand": {"name": "Zephyr", "tagline": "Lift off", "accent": "#ff3366"}}))
        .unwrap();
    let doc = f.ws.session.document();
    let mut texts = Vec::new();
    collect_text(&doc.pages[0].root, &mut texts);
    assert!(texts.iter().any(|t| t.contains("Zephyr")), "{texts:?}");
    let v = f
        .call(
            "scene_spec_validate",
            json!({"name": "logo-sting", "include_spec": true}),
        )
        .unwrap();
    let tokens = v["spec"]["palette"]["tokens"].as_array().unwrap();
    assert_eq!(tokens.len(), 8);
    let accent = tokens.iter().find(|t| t["id"] == "accent").unwrap();
    assert_eq!(accent["hex"], "#ff3366");
    assert_eq!(
        v["spec"]["brief"]["style"]["style_pack"],
        crate::style_pack::PACKS[0].id
    );
    assert_eq!(out["p0_gates"][0]["status"], "pass");
}

#[test]
fn bad_input_is_refused_and_nothing_is_left_behind() {
    let mut f = fixture();
    assert!(
        f.call("apply_seed", json!({"id": "nope"}))
            .unwrap_err()
            .contains("the seeds are")
    );
    let e = f
        .call(
            "apply_seed",
            json!({"id": "logo-sting", "brand": {"accent": "red"}}),
        )
        .unwrap_err();
    assert!(e.contains("brand.accent"), "{e}");
    let long = "x".repeat(400);
    let e = f
        .call(
            "apply_seed",
            json!({"id": "kinetic-quote", "brand": {"headline": long}}),
        )
        .unwrap_err();
    assert!(e.contains("rolled back"), "{e}");
    let doc = f.ws.session.document();
    assert!(
        doc.pages[0].root.children.is_empty(),
        "the failed seed left a frame"
    );
    assert!(doc.videos.is_empty(), "the failed seed left a video");
    assert!(!f._dir.path().join("p.od").join("scene_specs").exists());
}

#[test]
fn a_video_with_clips_is_refused_and_an_empty_one_is_used() {
    let mut f = fixture();
    let first = f.call("apply_seed", json!({"id": "logo-sting"})).unwrap();
    let e = f
        .call(
            "apply_seed",
            json!({"id": "logo-sting", "video": first["video"]}),
        )
        .unwrap_err();
    assert!(e.contains("already has clips"), "{e}");
    f.call("apply_ops", json!({"ops": [{"op": "create_video", "name": "Empty", "fps": 24, "width": 1080, "height": 1080}]})).unwrap();
    let id =
        f.ws.session
            .document()
            .videos
            .last()
            .unwrap()
            .id
            .to_string();
    let out = f
        .call("apply_seed", json!({"id": "stats-callout", "video": id}))
        .unwrap();
    assert_eq!(out["video"], id.as_str());
    let spec = f
        .call(
            "scene_spec_validate",
            json!({"name": "stats-callout", "include_spec": true}),
        )
        .unwrap();
    assert_eq!(spec["spec"]["brief"]["format"]["width"], 1080);
}
