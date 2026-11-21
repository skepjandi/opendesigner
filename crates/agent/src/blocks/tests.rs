#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use od_core::geometry;
use od_core::{Actor, Document, NodeId, NodeKind, Operation, Workspace};
use serde_json::{Map, Value, json};
use tempfile::TempDir;

use super::*;
use crate::review::review_frame;
use crate::tools::{CallCtx, Turns};

const SIZES: [(u32, u32); 3] = [(1290, 2796), (1920, 1080), (1080, 1080)];

struct Fixture {
    _dir: TempDir,
    ws: Workspace,
    frame: NodeId,
    video: NodeId,
}

fn apply(ws: &mut Workspace, ops: Vec<Operation>) -> od_core::TxReceipt {
    ws.session.apply(Actor::User, None, &ops).unwrap()
}

fn fixture(w: u32, h: u32) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Blocks").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let created = apply(
        &mut ws,
        vec![Operation::Create {
            parent: root,
            index: None,
            node: serde_json::from_value(json!({
                "kind": "frame", "name": "Slide", "width": w, "height": h, "fill": "#0d0f14"
            }))
            .unwrap(),
        }],
    );
    let frame = created.created[0].clone();
    apply(
        &mut ws,
        vec![Operation::CreateVideo {
            name: "Cut".into(),
            fps: 30,
            width: w,
            height: h,
        }],
    );
    let video = ws.session.document().videos[0].id.clone();
    Fixture {
        _dir: dir,
        ws,
        frame,
        video,
    }
}

impl Fixture {
    fn doc(&self) -> &Document {
        self.ws.session.document()
    }

    fn dir(&self) -> &Path {
        self.ws.dir().unwrap()
    }

    fn request(&self, block: &str, params: Option<Value>) -> Request {
        Request {
            block: block.into(),
            frame: Some(self.frame.to_string()),
            video: Some(self.video.to_string()),
            params: params.map(|p| p.as_object().cloned().unwrap_or_default()),
            ..Request::default()
        }
    }

    fn plan(&self, block: &str, params: Option<Value>) -> Result<Plan, BlockError> {
        plan(self.doc(), &self.request(block, params))
    }

    fn add(&mut self, block: &str, params: Option<Value>) -> Plan {
        let p = self
            .plan(block, params)
            .unwrap_or_else(|e| panic!("{block}: {e}"));
        write_assets(self.dir(), &p).unwrap();
        apply(&mut self.ws, p.ops.clone());
        p
    }
}

fn every_block() -> Vec<&'static str> {
    all()
        .filter(|b| b.id != "stagger_in")
        .map(|b| b.id)
        .collect()
}

fn changed_share(a: &od_render::Pixmap, b: &od_render::Pixmap) -> f64 {
    let total = (a.width() * a.height()) as f64;
    let changed = a
        .data()
        .chunks(4)
        .zip(b.data().chunks(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 8))
        .count();
    changed as f64 / total
}

fn render(f: &Fixture) -> od_render::Rendered {
    let node = f.doc().node(&f.frame).unwrap();
    let scale = (720.0 / node.width.max(node.height)) as f32;
    od_render::render_frame(f.doc(), &f.frame, scale, Some(f.dir())).unwrap()
}

#[test]
fn the_catalog_has_the_promised_blocks_with_unique_ids() {
    let ids: Vec<&str> = all().map(|b| b.id).collect();
    assert!(ids.len() >= 28, "{} blocks", ids.len());
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), ids.len(), "duplicate block id");
    for needed in [
        "title_card",
        "kinetic_headline",
        "subtitle_under",
        "quote_card",
        "stat_callout",
        "badge_pill",
        "eyebrow_label",
        "lower_third",
        "name_tag",
        "cta_button",
        "tooltip_callout",
        "highlight_box",
        "spotlight_dim",
        "cursor_click",
        "notification_toast",
        "gradient_wash",
        "aurora_backdrop",
        "grid_backdrop",
        "noise_backdrop",
        "bar_chart",
        "line_chart",
        "donut_chart",
        "progress_bar",
        "comparison_table",
        "app_store_headline_pair",
        "feature_list",
        "rating_stars",
        "review_card",
        "stagger_in",
        "logo_sting",
        "end_card",
    ] {
        assert!(ids.contains(&needed), "{needed} is missing");
    }
    for b in all() {
        assert!(
            !b.about.is_empty() && !b.tags.is_empty(),
            "{} lacks text",
            b.id
        );
    }
}

#[test]
fn every_block_expands_at_three_sizes_without_overflow_or_review_errors() {
    for (w, h) in SIZES {
        for id in every_block() {
            let mut f = fixture(w, h);
            let p = f.add(id, None);
            let doc = f.doc();
            let frame = doc.node(&f.frame).unwrap();
            let overflows = od_render::check_text(frame);
            assert!(overflows.is_empty(), "{id} at {w}x{h}: {overflows:?}");
            let review = review_frame(doc, &f.frame).unwrap();
            let errors: Vec<&Value> = review["findings"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v["severity"] == "error")
                .collect();
            assert!(errors.is_empty(), "{id} at {w}x{h}: {errors:?}");
            assert!(p.node_count > 1, "{id} made nothing");
            assert!(p.name.starts_with(&format!("block:{id}#")));
            let root = &p.parts[0].1;
            let group = doc.node(root).unwrap();
            assert_eq!(group.kind, NodeKind::Group);
            assert_eq!(doc.node(&p.frame).unwrap().id, f.frame);
        }
    }
}

#[test]
fn every_block_stays_inside_the_frame() {
    for (w, h) in SIZES {
        for id in every_block()
            .into_iter()
            .filter(|id| !scenery_tests::BLEEDS.contains(id))
        {
            let mut f = fixture(w, h);
            let p = f.add(id, None);
            let doc = f.doc();
            let mut ids = Vec::new();
            doc.node(&p.parts[0].1).unwrap().ids(&mut ids);
            for n in ids {
                let node = doc.node(&n).unwrap();
                let Some(b) = geometry::bounds_in_frame(doc, &f.frame, &n) else {
                    continue;
                };
                if node.kind == NodeKind::Group {
                    continue;
                }
                let (fw, fh) = (f64::from(w), f64::from(h));
                assert!(
                    b.x >= -1.0
                        && b.y >= -1.0
                        && b.x + b.width <= fw + 1.0
                        && b.y + b.height <= fh + 1.0,
                    "{id} at {w}x{h}: {} leaves the frame at {b:?}",
                    node.name
                );
            }
        }
    }
}

#[test]
fn the_same_request_gives_byte_identical_operations() {
    for id in every_block() {
        let ops = |f: &Fixture| serde_json::to_string(&f.plan(id, None).unwrap().ops).unwrap();
        let (a, b) = (fixture(1920, 1080), fixture(1920, 1080));
        assert_eq!(ops(&a), ops(&b), "{id} is not deterministic");
        let again = ops(&a);
        assert_eq!(ops(&a), again, "{id} changed between two plans");
    }
}

#[test]
fn undo_removes_the_whole_block_in_one_step() {
    for id in every_block() {
        let mut f = fixture(1920, 1080);
        let before = f.doc().clone();
        let log_len = f.ws.session.log().len();
        f.add(id, None);
        assert_eq!(
            f.ws.session.log().len(),
            log_len + 1,
            "{id} used more than one transaction"
        );
        f.ws.session.undo(Actor::User).unwrap();
        let after = f.doc();
        assert_eq!(after.pages, before.pages, "{id} left nodes behind");
        assert_eq!(after.videos, before.videos, "{id} left keyframes behind");
        assert_eq!(after.tokens, before.tokens, "{id} left tokens behind");
    }
}

#[test]
fn blocks_with_motion_keyframe_the_video_and_stay_valid() {
    for id in every_block() {
        let mut f = fixture(1920, 1080);
        let p = f.add(id, None);
        assert!(p.animated, "{id} has no motion");
        let animates = p
            .ops
            .iter()
            .filter(|o| matches!(o, Operation::Animate { .. }))
            .count();
        assert!(animates > 0, "{id} made no keys");
        f.doc().validate().unwrap();
        let video = f.doc().video(&f.video).unwrap();
        assert!(!video.animations.is_empty(), "{id}");
    }
}

#[test]
fn without_a_video_a_block_is_a_static_layout() {
    let mut f = fixture(1920, 1080);
    let mut req = f.request("lower_third", None);
    req.video = None;
    apply(
        &mut f.ws,
        vec![Operation::CreateVideo {
            name: "Second".into(),
            fps: 30,
            width: 1920,
            height: 1080,
        }],
    );
    let p = plan(f.doc(), &req).unwrap();
    assert!(!p.animated && p.static_only && p.video.is_none());
    assert!(
        p.ops
            .iter()
            .all(|o| !matches!(o, Operation::Animate { .. }))
    );
}

#[test]
fn start_and_hold_time_the_motion() {
    let f = fixture(1920, 1080);
    let mut req = f.request("lower_third", None);
    req.start_ms = Some(2000);
    req.hold_ms = Some(3000);
    let p = plan(f.doc(), &req).unwrap();
    let keys: Vec<u64> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Operation::Animate { keys, .. } => Some(keys.iter().map(|k| k.at_ms)),
            _ => None,
        })
        .flatten()
        .collect();
    assert!(keys.iter().all(|k| *k >= 2000), "{keys:?}");
    assert!(keys.iter().any(|k| *k >= 5000), "no exit: {keys:?}");
    req.hold_ms = Some(100);
    assert!(
        plan(f.doc(), &req).is_err(),
        "a hold shorter than the entrance is refused"
    );
}

#[test]
fn parameters_are_checked_against_the_declaration() {
    let f = fixture(1920, 1080);
    let err = |block: &str, params: Value| f.plan(block, Some(params)).err().map(|e| e.to_string());
    assert!(
        err("title_card", json!({"nope": 1}))
            .unwrap()
            .contains("no parameter")
    );
    assert!(
        err("title_card", json!({"align": "diagonal"}))
            .unwrap()
            .contains("one of")
    );
    assert!(
        err("title_card", json!({"title": "x".repeat(500)}))
            .unwrap()
            .contains("longer")
    );
    assert!(
        err("progress_bar", json!({"value": 140}))
            .unwrap()
            .contains("not in")
    );
    assert!(
        err("progress_bar", json!({"value": 1.5}))
            .unwrap()
            .contains("whole number")
    );
    assert!(err("kinetic_headline", json!({"text": "a ".repeat(30)})).is_some());
    assert!(err("feature_list", json!({"items": [{"icon": "not-an-icon-xyz", "title": "a"}, {"icon": "zap", "title": "b"}]})).is_some());
    assert!(
        err("no_such_block", json!({}))
            .unwrap()
            .contains("list_blocks")
    );
}

#[test]
fn data_blocks_validate_and_cap_their_json() {
    let f = fixture(1920, 1080);
    let bars = |n: usize| json!({"data": (0..n).map(|i| json!({"label": format!("B{i}"), "value": i})).collect::<Vec<_>>()});
    assert!(f.plan("bar_chart", Some(bars(12))).is_ok());
    assert!(f.plan("bar_chart", Some(bars(13))).is_err());
    assert!(f.plan("bar_chart", Some(bars(1))).is_err());
    assert!(
        f.plan(
            "bar_chart",
            Some(json!({"data": [{"label": "a", "value": -3}, {"label": "b", "value": 1}]}))
        )
        .is_err()
    );
    assert!(
        f.plan(
            "bar_chart",
            Some(json!({"data": [{"label": "a", "value": 1e12}, {"label": "b", "value": 1}]}))
        )
        .is_err()
    );
    let points = |n: usize| json!({"labels": [], "series": [{"name": "a", "values": vec![1; n]}]});
    assert!(f.plan("line_chart", Some(points(24))).is_ok());
    assert!(f.plan("line_chart", Some(points(25))).is_err());
    assert!(
        f.plan(
            "line_chart",
            Some(json!({"labels": ["a"], "series": [{"name": "a", "values": [1, 2]}]}))
        )
        .is_err()
    );
    let four = json!({"labels": [], "series": (0..4).map(|i| json!({"name": format!("s{i}"), "values": [1, 2]})).collect::<Vec<_>>()});
    assert!(f.plan("line_chart", Some(four)).is_err());
    let table = json!({"columns": ["a", "b"], "rows": [["1"]]});
    assert!(f.plan("comparison_table", Some(table)).is_err());
    let wide = json!({"columns": ["a", "b", "c", "d", "e", "f", "g"], "rows": []});
    assert!(f.plan("comparison_table", Some(wide)).is_err());
    assert!(
        f.plan(
            "donut_chart",
            Some(json!({"data": [{"label": "a", "value": 0}, {"label": "b", "value": 0}]}))
        )
        .is_err()
    );
}

#[test]
fn text_that_cannot_fit_is_refused_not_overflowed() {
    let f = fixture(1080, 1080);
    let long = "Supercalifragilisticexpialidocious".repeat(2);
    let e = f
        .plan("title_card", Some(json!({"title": long})))
        .err()
        .unwrap();
    assert!(matches!(e, BlockError::Fit(_)), "{e}");
}

#[test]
fn a_box_places_a_block_and_must_stay_inside_the_frame() {
    let f = fixture(1920, 1080);
    let mut req = f.request("title_card", Some(json!({"panel": false})));
    req.region = Some(BoxArgs {
        x: Some(100.0),
        y: Some(100.0),
        width: Some(800.0),
        height: Some(400.0),
    });
    let p = plan(f.doc(), &req).unwrap();
    let doc = {
        let mut d = f.doc().clone();
        for op in &p.ops {
            od_core::ops::apply(&mut d, op, false).unwrap();
        }
        d
    };
    let mut ids = Vec::new();
    doc.node(&p.parts[0].1).unwrap().ids(&mut ids);
    for id in ids {
        let node = doc.node(&id).unwrap();
        if node.kind == NodeKind::Text {
            let b = geometry::bounds_in_frame(&doc, &f.frame, &id).unwrap();
            assert!(
                b.x >= 100.0 - 0.5
                    && b.x + b.width <= 900.5
                    && b.y >= 100.0 - 0.5
                    && b.y + b.height <= 500.5,
                "{b:?}"
            );
        }
    }
    req.region = Some(BoxArgs {
        x: Some(1800.0),
        width: Some(400.0),
        ..BoxArgs::default()
    });
    assert!(plan(f.doc(), &req).is_err());
}

#[test]
fn colors_come_from_tokens_then_from_created_tokens() {
    let mut f = fixture(1920, 1080);
    let p = f.plan("badge_pill", None).unwrap();
    let made: Vec<&str> = p
        .ops
        .iter()
        .filter_map(|o| match o {
            Operation::SetToken { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert!(made.contains(&"block.accent"), "{made:?}");
    f.ws.session.set_token("brand.accent", "#ff6600").unwrap();
    let p = f.plan("badge_pill", None).unwrap();
    assert!(
        p.ops
            .iter()
            .all(|o| !matches!(o, Operation::SetToken { name, .. } if name == "block.accent"))
    );
    let json = serde_json::to_string(&p.ops).unwrap();
    assert!(json.contains("{brand.accent}") && !json.contains("block.accent"));
    let mut req = f.request("badge_pill", None);
    req.colors = Some(Colors {
        accent: Some("{no.such}".into()),
        ..Colors::default()
    });
    assert!(plan(f.doc(), &req).is_err());
    req.colors = Some(Colors {
        accent: Some("#112233".into()),
        ..Colors::default()
    });
    assert!(
        serde_json::to_string(&plan(f.doc(), &req).unwrap().ops)
            .unwrap()
            .contains("#112233")
    );
}

#[test]
fn one_font_family_per_block() {
    let mut f = fixture(1920, 1080);
    let mut req = f.request("title_card", None);
    req.font = Some("Lato".into());
    let p = plan(f.doc(), &req).unwrap();
    apply(&mut f.ws, p.ops);
    let mut families = std::collections::BTreeSet::new();
    let mut ids = Vec::new();
    f.doc().node(&f.frame).unwrap().ids(&mut ids);
    for id in ids {
        if let Some(fam) = &f.doc().node(&id).unwrap().font_family {
            families.insert(fam.clone());
        }
    }
    assert_eq!(families.len(), 1, "{families:?}");
}

#[test]
fn blocks_are_numbered_and_found_by_their_name_prefix() {
    let mut f = fixture(1920, 1080);
    let a = f.add("badge_pill", None);
    let b = f.add("badge_pill", None);
    assert_eq!(a.name, "block:badge_pill#1");
    assert_eq!(b.name, "block:badge_pill#2");
    let root = f.doc().pages[0].root.clone();
    let frame = &root.children[0];
    let groups: Vec<&str> = frame.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(groups, ["block:badge_pill#1", "block:badge_pill#2"]);
    assert!(
        frame.children[0]
            .children
            .iter()
            .all(|c| c.name.starts_with("block:badge_pill#1/"))
    );
}

#[test]
fn backdrops_go_behind_the_frames_content() {
    let mut f = fixture(1920, 1080);
    f.add("title_card", None);
    f.add("gradient_wash", None);
    let frame = &f.doc().pages[0].root.children[0];
    assert!(frame.children[0].name.starts_with("block:gradient_wash"));
}

#[test]
fn pictures_are_written_once_and_named_by_recipe() {
    let mut f = fixture(1920, 1080);
    let p = f.add("gradient_wash", None);
    assert_eq!(p.assets.len(), 1);
    let file = p.assets[0].file();
    assert!(f.dir().join(&file).exists(), "{file}");
    assert!(f.dir().join(file.replace(".png", ".asset.json")).exists());
    let again = f.add("gradient_wash", None);
    assert_eq!(again.assets[0].file(), file);
    let count = std::fs::read_dir(f.dir().join("media")).unwrap().count();
    assert_eq!(count, 2, "the second block reused the picture");
    let aurora = f.plan("aurora_backdrop", None).unwrap();
    assert_ne!(aurora.assets[0].file(), file);
}

#[test]
fn icons_carry_their_credit() {
    let f = fixture(1920, 1080);
    let p = f.plan("feature_list", None).unwrap();
    let report = p.report();
    assert_eq!(report["attribution_required"], json!(true));
    assert!(report["attribution"].as_str().unwrap().contains("Lucide"));
}

#[test]
fn stagger_in_animates_existing_nodes_and_makes_none() {
    let mut f = fixture(1920, 1080);
    let ids: Vec<String> = (0..3)
        .map(|i| {
            let created = apply(
                &mut f.ws,
                vec![Operation::Create {
                    parent: f.frame.clone(),
                    index: None,
                    node: serde_json::from_value(json!({
                        "kind": "rect", "name": format!("Card {i}"), "x": 100 + i * 300, "y": 400,
                        "width": 240, "height": 240, "fill": "#7c6cff"
                    }))
                    .unwrap(),
                }],
            );
            created.created[0].to_string()
        })
        .collect();
    let before = f.doc().pages.clone();
    let p = f
        .plan("stagger_in", Some(json!({"nodes": ids, "step_ms": 200})))
        .unwrap();
    assert!(p.ops.iter().all(|o| matches!(o, Operation::Animate { .. })));
    assert!(p.ops.len() >= 3 * 2, "opacity and position for each");
    apply(&mut f.ws, p.ops);
    assert_eq!(f.doc().pages, before);
    let video = f.doc().video(&f.video).unwrap();
    let starts: Vec<u64> = ids
        .iter()
        .map(|id| {
            video
                .animations
                .iter()
                .find(|a| a.node.as_str() == id)
                .unwrap()
                .keys[0]
                .at_ms
        })
        .collect();
    assert_eq!(starts, [0, 200, 400]);
    let mut no_video = f.request("stagger_in", Some(json!({"nodes": ids})));
    no_video.video = Some("v_missing".into());
    assert!(plan(f.doc(), &no_video).is_err());
    assert!(
        f.plan("stagger_in", Some(json!({}))).is_err(),
        "nodes are required"
    );
    assert!(
        f.plan("stagger_in", Some(json!({"nodes": ["n_9999"]})))
            .is_err()
    );
}

#[test]
fn search_ranks_by_name_tags_and_description() {
    let top = |q: &str| search(q, None).first().map(|b| b.id);
    assert_eq!(top("lower third"), Some("lower_third"));
    assert_eq!(top("bar chart"), Some("bar_chart"));
    assert_eq!(
        top("testimonial").map(|i| i.contains("quote") || i.contains("review")),
        Some(true)
    );
    assert_eq!(top("cursor"), Some("cursor_click"));
    assert!(search("zzzz nothing", None).is_empty());
    let all_titles = search("", Some(Category::Titles));
    assert_eq!(all_titles.len(), 7);
    assert!(all_titles.iter().all(|b| b.category == Category::Titles));
    assert_eq!(search("", None).len(), all().count());
}

#[test]
fn every_declared_parameter_is_read_by_its_block() {
    for b in all().filter(|b| b.id != "stagger_in") {
        let f = fixture(1920, 1080);
        let args = args::Args::resolve(b.id, b.params, Map::new(), f.doc(), &f.frame).unwrap();
        let mut kit = kit::Kit::new(
            f.doc(),
            kit::at(0.0, 0.0, 1920.0, 1080.0),
            b.place.resolve(kit::at(0.0, 0.0, 1920.0, 1080.0), 65.0),
            "block:t#1".into(),
            None,
            Default::default(),
        );
        (b.build)(&mut kit, &args).unwrap();
        args.check().unwrap();
        let unread = args.unread();
        assert!(unread.is_empty(), "{} never reads {unread:?}", b.id);
    }
}

fn call_tool(ws: &mut Workspace, name: &str, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, name, args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

#[test]
fn the_tools_list_describe_and_add_a_block_as_one_attributed_step() {
    let mut f = fixture(1920, 1080);
    let listed = call_tool(&mut f.ws, "list_blocks", json!({"query": "chart"})).unwrap();
    assert!(listed["count"].as_u64().unwrap() >= 3);
    assert!(call_tool(&mut f.ws, "list_blocks", json!({"category": "nope"})).is_err());
    let one = call_tool(&mut f.ws, "get_block", json!({"block": "lower_third"})).unwrap();
    assert!(
        one["params"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "name")
    );
    let log = f.ws.session.log().len();
    let added = call_tool(
        &mut f.ws,
        "add_block",
        json!({"block": "lower_third", "params": {"name": "Sam Lee", "title": "CTO"}, "start_ms": 500}),
    )
    .unwrap();
    assert_eq!(f.ws.session.log().len(), log + 1);
    let last = f.ws.session.log().last().unwrap();
    assert_eq!(last.actor, Actor::agent("tester").unwrap());
    let root = added["root"].as_str().unwrap();
    assert_eq!(
        f.doc().node(&NodeId::parse(root).unwrap()).unwrap().name,
        "block:lower_third#1"
    );
    assert!(added["description"].as_str().unwrap().contains(root));
    assert!(added["parts"]["content"].is_string());
    assert_eq!(added["animated"], json!(true));
    let dry = call_tool(
        &mut f.ws,
        "add_block",
        json!({"block": "badge_pill", "dry_run": true}),
    )
    .unwrap();
    assert!(dry["ops"].as_array().unwrap().len() > 1);
    assert_eq!(f.ws.session.log().len(), log + 1, "dry run applies nothing");
    assert!(
        call_tool(
            &mut f.ws,
            "add_block",
            json!({"block": "badge_pill", "frame": "n_9999"})
        )
        .is_err()
    );
}

#[test]
fn blocks_that_need_pictures_say_so_without_a_project_folder() {
    let mut ws = Workspace::new(od_core::Session::new(Document::new("x")));
    let page = ws
        .session
        .document()
        .pages
        .first()
        .map(|p| p.root.id.clone());
    let page = match page {
        Some(p) => p,
        None => ws.session.add_page("Page").unwrap(),
    };
    let root = ws
        .session
        .document()
        .page(&page)
        .map_or(page.clone(), |p| p.root.id.clone());
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: serde_json::from_value(json!({"kind": "frame", "name": "F", "width": 1080, "height": 1080, "fill": "#0d0f14"})).unwrap(),
            }],
        )
        .unwrap();
    let err = call_tool(&mut ws, "add_block", json!({"block": "gradient_wash"})).unwrap_err();
    assert!(err.contains("project folder"), "{err}");
    assert!(call_tool(&mut ws, "add_block", json!({"block": "title_card"})).is_ok());
}

fn one_of_each_category_renders(ids: &[&str]) {
    for (w, h) in SIZES {
        for id in ids {
            let mut f = fixture(w, h);
            let blank = render(&f);
            f.add(id, None);
            let drawn = render(&f);
            assert!(
                drawn.warnings.is_empty(),
                "{id} at {w}x{h} warns: {:?}",
                drawn.warnings
            );
            let share = changed_share(&blank.pixmap, &drawn.pixmap);
            assert!(
                share > 0.0004,
                "{id} at {w}x{h} is blank (changed {share:.5})"
            );
        }
    }
}

#[test]
fn titles_render_visibly() {
    one_of_each_category_renders(&[
        "title_card",
        "kinetic_headline",
        "subtitle_under",
        "quote_card",
        "stat_callout",
        "badge_pill",
        "eyebrow_label",
    ]);
}

#[test]
fn overlays_render_visibly() {
    one_of_each_category_renders(&[
        "lower_third",
        "name_tag",
        "cta_button",
        "tooltip_callout",
        "highlight_box",
        "spotlight_dim",
        "notification_toast",
    ]);
}

#[test]
fn backgrounds_render_visibly() {
    one_of_each_category_renders(&[
        "gradient_wash",
        "aurora_backdrop",
        "grid_backdrop",
        "noise_backdrop",
    ]);
}

#[test]
fn data_blocks_render_visibly() {
    one_of_each_category_renders(&[
        "bar_chart",
        "line_chart",
        "donut_chart",
        "progress_bar",
        "comparison_table",
    ]);
}

#[test]
fn store_blocks_render_visibly() {
    one_of_each_category_renders(&[
        "app_store_headline_pair",
        "feature_list",
        "rating_stars",
        "review_card",
    ]);
}

#[test]
fn motion_blocks_render_visibly() {
    one_of_each_category_renders(&["logo_sting", "end_card"]);
}

#[test]
fn a_cursor_click_is_drawn_when_the_video_reaches_it() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "cursor_click",
        Some(json!({"to": {"x": 960, "y": 540}, "from": {"x": 1600, "y": 900}})),
    );
    let video = f.doc().video(&f.video).unwrap().clone();
    let at_start = f.doc().at_time(&video, 0.0).unwrap();
    let at_end = f.doc().at_time(&video, 4000.0).unwrap();
    let cursor = |d: &Document| {
        let id = p
            .parts
            .iter()
            .find(|(n, _)| n == "cursor")
            .unwrap()
            .1
            .clone();
        let n = d.node(&id).unwrap();
        (n.x, n.y, n.opacity)
    };
    let (sx, sy, so) = cursor(&at_start);
    let (ex, ey, eo) = cursor(&at_end);
    assert!(
        sx > ex + 400.0 && sy > ey + 200.0,
        "the cursor travels toward the target"
    );
    assert!(so < 0.01 && eo > 0.99);
    let ripple = p
        .parts
        .iter()
        .find(|(n, _)| n == "ripple-1")
        .unwrap()
        .1
        .clone();
    let mid = f
        .doc()
        .at_time(&video, 300.0 + 900.0 + 60.0 + 400.0)
        .unwrap();
    assert!(
        mid.node(&ripple).unwrap().opacity > 0.2,
        "the ring is visible mid-click"
    );
    assert!(
        at_end.node(&ripple).unwrap().opacity < 0.01,
        "and gone after"
    );
}

#[test]
fn bars_grow_from_the_baseline() {
    let mut f = fixture(1920, 1080);
    let p = f.add("bar_chart", None);
    let video = f.doc().video(&f.video).unwrap().clone();
    let bar = p
        .parts
        .iter()
        .find(|(n, _)| n == "bar-3")
        .unwrap()
        .1
        .clone();
    let final_h = f.doc().node(&bar).unwrap().height;
    let start = f.doc().at_time(&video, 0.0).unwrap();
    assert!(start.node(&bar).unwrap().height < 1.0);
    let end = f.doc().at_time(&video, 5000.0).unwrap();
    assert!((end.node(&bar).unwrap().height - final_h).abs() < 0.01);
}

#[test]
fn a_donut_ring_is_drawn_on_the_ring_not_beside_it() {
    let mut f = fixture(1080, 1080);
    f.add("donut_chart", Some(json!({"legend": false, "title": ""})));
    let rendered = render(&f);
    let pm = &rendered.pixmap;
    let (w, h) = (pm.width() as i32, pm.height() as i32);
    let pixel = |x: i32, y: i32| {
        let i = ((y * w + x) * 4) as usize;
        let d = pm.data();
        (d[i], d[i + 1], d[i + 2])
    };
    let bg = pixel(2, 2);
    let (cx, cy) = (w / 2, h / 2);
    let ring_hit = (0..w / 2).filter(|dx| pixel(cx + dx, cy) != bg).count();
    assert!(
        ring_hit > (w as f32 * 0.05) as usize,
        "a ring crosses the horizontal midline"
    );
}
