#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use od_core::geometry::{self, Rect};
use od_core::{Actor, AnimProp, Document, NodeId, Operation, Repeat, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;

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
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Characters").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let created = apply(
        &mut ws,
        vec![Operation::Create {
            parent: root,
            index: None,
            node: serde_json::from_value(json!({
                "kind": "frame", "name": "Scene", "width": w, "height": h,
                "fill": {"type": "linear", "angle": 90, "stops": [{"offset": 0, "color": "#26335e"}, {"offset": 1, "color": "#4a5fa3"}]}
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

    fn add(&mut self, block: &str, params: Option<Value>) -> Plan {
        let p = plan(self.doc(), &self.request(block, params))
            .unwrap_or_else(|e| panic!("{block}: {e}"));
        apply(&mut self.ws, p.ops.clone());
        p
    }

    /// The frame as it looks `t_ms` into the video.
    fn shot(&self, t_ms: f64) -> od_render::Pixmap {
        let video = self.doc().video(&self.video).unwrap();
        let doc = self
            .doc()
            .at_time(video, t_ms)
            .unwrap_or_else(|| self.doc().clone());
        let node = doc.node(&self.frame).unwrap();
        let scale = (960.0 / node.width.max(node.height)) as f32;
        od_render::render_frame(&doc, &self.frame, scale, Some(self.dir()))
            .unwrap()
            .pixmap
    }
}

#[test]
fn dump_pictures() {
    let Ok(dir) = std::env::var("OD_DUMP") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for b in all().filter(|b| b.category == Category::Characters) {
        let mut f = fixture(1920, 1080);
        f.add(b.id, None);
        let png = od_render::encode_png(&f.shot(700.0), false).unwrap();
        std::fs::write(format!("{dir}/{}.png", b.id), png).unwrap();
    }
    for (style, expr, extra) in [
        ("bean", "surprised", json!({"hat": true})),
        ("robot", "happy", json!({"glasses": false})),
        (
            "creature",
            "thinking",
            json!({"backpack": true, "facing": "right"}),
        ),
        ("bean", "sad", json!({"glasses": true, "facing": "left"})),
        ("blob", "angry", json!({})),
        ("robot", "neutral", json!({"hat": true, "action": "wave"})),
    ] {
        let mut f = fixture(1920, 1080);
        let mut p = extra.as_object().unwrap().clone();
        p.insert("style".into(), json!(style));
        p.insert("expression".into(), json!(expr));
        f.add("character", Some(Value::Object(p)));
        let png = od_render::encode_png(&f.shot(1000.0), false).unwrap();
        std::fs::write(format!("{dir}/char_{style}_{expr}.png"), png).unwrap();
    }
}

const SIZES: [(u32, u32); 3] = [(1290, 2796), (1920, 1080), (1080, 1080)];

fn part(p: &Plan, name: &str) -> NodeId {
    p.parts
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| {
            panic!(
                "no part {name}: {:?}",
                p.parts.iter().map(|x| &x.0).collect::<Vec<_>>()
            )
        })
        .1
        .clone()
}

fn at(f: &Fixture, t: f64) -> Document {
    f.doc()
        .at_time(f.doc().video(&f.video).unwrap(), t)
        .unwrap()
}

fn bounds(f: &Fixture, doc: &Document, id: &NodeId) -> Rect {
    geometry::bounds_in_frame(doc, &f.frame, id).unwrap()
}

/// The box around everything a block drew.
fn group_bounds(f: &Fixture, root: &NodeId) -> Rect {
    let doc = f.doc();
    doc.node(root)
        .unwrap()
        .children
        .iter()
        .filter_map(|c| geometry::bounds_in_frame(doc, &f.frame, &c.id))
        .reduce(Rect::union)
        .unwrap()
}

fn mid(r: Rect) -> (f64, f64) {
    (r.x + r.width / 2.0, r.y + r.height / 2.0)
}

fn track(f: &Fixture, node: &NodeId, prop: AnimProp) -> od_core::Animation {
    f.doc()
        .video(&f.video)
        .unwrap()
        .animations
        .iter()
        .find(|a| &a.node == node && a.prop == prop)
        .unwrap_or_else(|| panic!("no {prop:?} track on {node}"))
        .clone()
}

/// The share of pixels inside `r` (frame units, picture at half size) that differ.
fn changed_in(a: &od_render::Pixmap, b: &od_render::Pixmap, r: Rect) -> f64 {
    let s = 0.5;
    let (x0, y0) = (
        (r.x * s).floor().max(0.0) as u32,
        (r.y * s).floor().max(0.0) as u32,
    );
    let (x1, y1) = (
        (((r.x + r.width) * s).ceil() as u32).min(a.width()),
        (((r.y + r.height) * s).ceil() as u32).min(a.height()),
    );
    let (mut changed, mut total) = (0.0_f64, 0.0_f64);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = ((y * a.width() + x) * 4) as usize;
            total += 1.0;
            if (0..4).any(|c| a.data()[i + c].abs_diff(b.data()[i + c]) > 12) {
                changed += 1.0;
            }
        }
    }
    changed / total.max(1.0)
}

fn closed_time(f: &Fixture, eye: &NodeId) -> u64 {
    track(f, eye, AnimProp::ScaleY)
        .keys
        .iter()
        .find(|k| k.value < 0.2)
        .unwrap()
        .at_ms
}

#[test]
fn every_action_expands_at_three_sizes_and_undoes_in_one_step() {
    let actions = [
        json!({"action": "idle"}),
        json!({"action": "wave", "loops": 2}),
        json!({"action": "talk"}),
        json!({"action": "walk", "action_start_ms": 200, "action_end_ms": 5200}),
        json!({"action": "jump", "loops": 3}),
        json!({"action": "point", "target": {"x": 100, "y": 100}}),
        json!({"action": "celebrate", "loops": 2}),
        json!({"style": "robot", "hat": true, "glasses": true, "backpack": true, "talk": [[0, 1500], [2500, 4000]]}),
        json!({"style": "creature", "expression": "sad", "facing": "left", "look_at": {"x": 50, "y": 50}}),
    ];
    for (w, h) in SIZES {
        for params in &actions {
            let mut f = fixture(w, h);
            let before = f.doc().clone();
            let len = f.ws.session.log().len();
            let p = f.add("character", Some(params.clone()));
            assert_eq!(f.ws.session.log().len(), len + 1);
            f.doc().validate().unwrap();
            let frame = f.doc().node(&f.frame).unwrap();
            assert!(od_render::check_text(frame).is_empty());
            assert!(p.node_count < 400, "{} nodes", p.node_count);
            f.ws.session.undo(Actor::User).unwrap();
            assert_eq!(f.doc().pages, before.pages);
            assert_eq!(f.doc().videos, before.videos);
        }
    }
}

#[test]
fn the_rig_names_its_parts_and_each_joint_pivots_where_it_should() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "character",
        Some(json!({"hat": true, "glasses": true, "backpack": true})),
    );
    for name in [
        "rig", "body", "head", "eyes", "eye_l", "eye_r", "pupil_l", "pupil_r", "mouth", "arm_l",
        "arm_r", "leg_l", "leg_r", "hat", "glasses", "backpack",
    ] {
        part(&p, name);
    }
    let doc = f.doc();
    let arm = doc.node(&part(&p, "arm_r")).unwrap();
    assert!(
        arm.pivot_x > 0.3 && arm.pivot_x < 0.7,
        "shoulder pivot is at the top middle"
    );
    assert!(
        arm.pivot_y < 0.15,
        "shoulder pivot is at the top: {}",
        arm.pivot_y
    );
    let leg = doc.node(&part(&p, "leg_l")).unwrap();
    assert!(
        leg.pivot_y < 0.4,
        "hip pivot is at the top: {}",
        leg.pivot_y
    );
    let body = doc.node(&part(&p, "body")).unwrap();
    assert!(
        (body.pivot_y - 1.0).abs() < 1e-6,
        "the body breathes from its base"
    );
}

#[test]
fn idle_life_loops_and_differs_by_seed() {
    let (mut a, mut b) = (fixture(1920, 1080), fixture(1920, 1080));
    let pa = a.add("character", Some(json!({"seed": 1})));
    let pb = b.add("character", Some(json!({"seed": 2})));
    for (name, prop) in [
        ("body", AnimProp::ScaleY),
        ("head", AnimProp::Rotation),
        ("eye_l", AnimProp::ScaleY),
        ("arm_l", AnimProp::Rotation),
    ] {
        let ta = track(&a, &part(&pa, name), prop);
        let tb = track(&b, &part(&pb, name), prop);
        assert_ne!(ta.repeat, Repeat::Once, "{name} {prop:?} must repeat");
        assert_ne!(ta.keys, tb.keys, "{name} {prop:?} must differ by seed");
    }
    let period = |f: &Fixture, p: &Plan| {
        let t = track(f, &part(p, "eye_l"), AnimProp::ScaleY);
        t.keys.last().unwrap().at_ms
    };
    for p in [period(&a, &pa), period(&b, &pb)] {
        assert!(
            (1900..=4100).contains(&p),
            "a blink every 2 to 4 s, not {p} ms"
        );
    }
}

#[test]
fn blinking_changes_the_eye_pixels_at_the_blink_and_not_between() {
    let mut f = fixture(1920, 1080);
    let p = f.add("character", Some(json!({"seed": 5})));
    let eye = part(&p, "eye_l");
    let t_closed = closed_time(&f, &eye) as f64;
    let region = bounds(&f, &at(&f, 0.0), &eye);
    let open = f.shot(t_closed - 400.0);
    let shut = f.shot(t_closed);
    let later = f.shot(t_closed - 250.0);
    let blink = changed_in(&open, &shut, region);
    let calm = changed_in(&open, &later, region);
    assert!(
        blink > 0.25,
        "the closed eye changes the eye's pixels: {blink}"
    );
    assert!(blink > calm * 2.0, "{blink} vs {calm}");
}

#[test]
fn a_wave_turns_the_arm_about_its_shoulder() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "character",
        Some(json!({"action": "wave", "action_start_ms": 500})),
    );
    let (arm, hand) = (
        part(&p, "arm_r"),
        f.doc().node(&part(&p, "arm_r")).unwrap().children[1]
            .id
            .clone(),
    );
    let reading = |t: f64| {
        let doc = at(&f, t);
        let a = doc.node(&arm).unwrap();
        let rig = doc.node(&part(&p, "rig")).unwrap();
        let shoulder = (
            rig.x + a.x + a.pivot_x * a.width,
            rig.y + a.y + a.pivot_y * a.height,
        );
        (shoulder, mid(bounds(&f, &doc, &hand)))
    };
    let ((s0, h0), (s1, h1)) = (reading(100.0), reading(1500.0));
    let arm_len = (h0.0 - s0.0).hypot(h0.1 - s0.1);
    let reach = (h1.0 - s1.0).hypot(h1.1 - s1.1);
    assert!(
        (reach - arm_len).abs() < arm_len * 0.04,
        "the hand stays {arm_len} from the shoulder, not {reach}"
    );
    assert!(
        h0.1 > s0.1 + arm_len * 0.8,
        "at rest the hand hangs below the shoulder"
    );
    assert!(
        h1.1 < s1.1 - arm_len * 0.4,
        "waving, the hand is above the shoulder"
    );
    let (still, waving) = (f.shot(100.0), f.shot(1500.0));
    let box_ = Rect {
        x: h1.0 - 60.0,
        y: h1.1 - 60.0,
        width: 120.0,
        height: 120.0,
    };
    assert!(
        changed_in(&still, &waving, box_) > 0.3,
        "pixels appear where the raised hand is"
    );
}

#[test]
fn talking_moves_the_mouth_only_inside_the_given_ranges() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "character",
        Some(json!({"talk": [[1000, 2000], [3000, 3600]], "expression": "neutral"})),
    );
    let open = part(&p, "mouth_open");
    let value = |t: f64| {
        let doc = at(&f, t);
        let n = doc.node(&open).unwrap();
        (n.scale_y, n.opacity)
    };
    let closed = value(500.0);
    for t in [0.0, 500.0, 2400.0, 2900.0, 3900.0, 6000.0] {
        assert_eq!(value(t), closed, "closed outside the ranges at {t}");
    }
    let opened = |from: u64, to: u64| {
        (from..to)
            .step_by(25)
            .any(|t| value(t as f64).1 > 0.9 && value(t as f64).0 > 0.8)
    };
    assert!(opened(1000, 2000), "the mouth opens in the first range");
    assert!(opened(3000, 3600), "the mouth opens in the second range");
    let best = (1000..2000u64)
        .step_by(25)
        .max_by(|a, b| value(*a as f64).0.total_cmp(&value(*b as f64).0))
        .unwrap();
    let mouth = bounds(&f, &at(&f, 0.0), &part(&p, "mouth"));
    let share = changed_in(&f.shot(500.0), &f.shot(best as f64), mouth);
    assert!(share > 0.15, "the open mouth shows in the pixels: {share}");
}

#[test]
fn a_walk_travels_from_from_x_to_to_x_and_swings_the_legs() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "character",
        Some(json!({"action": "walk", "from_x": 300, "to_x": 1500, "action_start_ms": 500, "action_end_ms": 4500})),
    );
    let body = part(&p, "body");
    let cx = |t: f64| mid(bounds(&f, &at(&f, t), &body)).0;
    assert!(
        (cx(0.0) - 300.0).abs() < 4.0,
        "starts at from_x: {}",
        cx(0.0)
    );
    assert!((cx(500.0) - 300.0).abs() < 4.0);
    assert!(
        (cx(2500.0) - 900.0).abs() < 8.0,
        "halfway at the middle: {}",
        cx(2500.0)
    );
    assert!(
        (cx(4500.0) - 1500.0).abs() < 4.0,
        "ends at to_x: {}",
        cx(4500.0)
    );
    assert!((cx(6000.0) - 1500.0).abs() < 4.0, "and stays there");
    let leg = part(&p, "leg_l");
    let angle = |t: f64| at(&f, t).node(&leg).unwrap().rotation;
    assert!(
        (angle(1000.0) - angle(1350.0)).abs() > 20.0,
        "the legs swing"
    );
    assert_eq!(angle(5000.0), 0.0, "and rest when it arrives");
    assert!(f.shot(500.0).data() != f.shot(2500.0).data());
}

#[test]
fn a_jump_squashes_stretches_and_lands() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "character",
        Some(json!({"action": "jump", "action_start_ms": 300})),
    );
    let rig = part(&p, "rig");
    let get = |t: f64| {
        let doc = at(&f, t);
        let n = doc.node(&rig).unwrap();
        (n.y, n.scale_y, n.scale_x)
    };
    let (y0, ..) = get(0.0);
    assert!(get(500.0).1 < 0.9, "squashes before the jump");
    assert!(get(630.0).1 > 1.1, "stretches as it leaves");
    assert!(get(830.0).0 < y0 - 100.0, "rises at the top");
    assert!(
        (get(2500.0).0 - y0).abs() < 0.01 && (get(2500.0).1 - 1.0).abs() < 0.01,
        "lands and settles"
    );
}

#[test]
fn pointing_turns_the_arm_toward_the_target() {
    let mut f = fixture(1920, 1080);
    let target = (1600.0, 300.0);
    let p = f.add(
        "character",
        Some(json!({"action": "point", "target": {"x": target.0, "y": target.1}})),
    );
    let hand = f.doc().node(&part(&p, "arm_r")).unwrap().children[1]
        .id
        .clone();
    let dist = |t: f64| {
        let h = mid(bounds(&f, &at(&f, t), &hand));
        (h.0 - target.0).hypot(h.1 - target.1)
    };
    assert!(
        dist(1500.0) < dist(0.0) - 250.0,
        "{} vs {}",
        dist(1500.0),
        dist(0.0)
    );
    let doc = at(&f, 1500.0);
    let (arm, rig) = (
        doc.node(&part(&p, "arm_r")).unwrap(),
        doc.node(&part(&p, "rig")).unwrap(),
    );
    let shoulder = (
        rig.x + arm.x + arm.pivot_x * arm.width,
        rig.y + arm.y + arm.pivot_y * arm.height,
    );
    let h = mid(bounds(&f, &doc, &hand));
    let want = (target.1 - shoulder.1).atan2(target.0 - shoulder.0);
    let got = (h.1 - shoulder.1).atan2(h.0 - shoulder.0);
    assert!(
        (want - got).abs() < 0.12,
        "the arm points at it: {want} vs {got}"
    );
}

#[test]
fn bad_character_requests_are_refused_with_a_reason() {
    let f = fixture(1920, 1080);
    let err = |p: Value| {
        plan(f.doc(), &f.request("character", Some(p)))
            .err()
            .map(|e| e.to_string())
    };
    assert!(err(json!({"action": "point"})).unwrap().contains("target"));
    assert!(err(json!({"from_x": 10})).unwrap().contains("walk"));
    assert!(
        err(json!({"talk": [[500, 100]]}))
            .unwrap()
            .contains("end after")
    );
    assert!(
        err(json!({"talk": [[0, 500], [400, 900]]}))
            .unwrap()
            .contains("overlap")
    );
    assert!(
        err(json!({"action": "walk", "action_start_ms": 0, "action_end_ms": 100}))
            .unwrap()
            .contains("walk takes")
    );
    assert!(err(json!({"body_color": "{nope}"})).is_some());
    assert!(
        err(json!({"talk": [[0, 590000]]}))
            .unwrap()
            .contains("keys")
    );
}

#[test]
fn bubbles_point_at_their_target_and_pop_in_with_overshoot() {
    for (tx, ty) in [(300.0, 900.0), (1700.0, 900.0), (960.0, 1000.0)] {
        let mut f = fixture(1920, 1080);
        let p = f.add("speech_bubble", Some(json!({"target": {"x": tx, "y": ty}})));
        let doc = f.doc();
        let tail = bounds(
            &f,
            doc,
            &doc.node(&part(&p, "bubble"))
                .unwrap()
                .children
                .iter()
                .find(|c| c.name.ends_with("/tail"))
                .unwrap()
                .id
                .clone(),
        );
        let body = bounds(
            &f,
            doc,
            &doc.node(&part(&p, "bubble"))
                .unwrap()
                .children
                .iter()
                .find(|c| c.name.ends_with("/bubble_body"))
                .unwrap()
                .id
                .clone(),
        );
        let (bc, tc) = (mid(body), mid(tail));
        let (want, got) = (
            (ty - bc.1).atan2(tx - bc.0),
            (tc.1 - bc.1).atan2(tc.0 - bc.0),
        );
        assert!(
            (want - got).abs() < 0.6,
            "tail aims at {tx},{ty}: {want} vs {got}"
        );
        let bubble = part(&p, "bubble");
        let scale = |t: f64| at(&f, t).node(&bubble).unwrap().scale_x;
        assert!(scale(0.0) < 0.1 && (scale(2000.0) - 1.0).abs() < 1e-9);
        assert!(
            (0..420).step_by(20).any(|t| scale(f64::from(t)) > 1.02),
            "overshoots"
        );
    }
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "thought_bubble",
        Some(json!({"target": {"x": 960.0, "y": 1000.0}})),
    );
    let bubble = f.doc().node(&part(&p, "bubble")).unwrap();
    for i in 0..3 {
        let name = format!("/trail_{i}");
        assert!(
            bubble.children.iter().any(|c| c.name.ends_with(&name)),
            "{name}"
        );
    }
}

#[test]
fn a_bubble_whose_text_cannot_fit_is_refused() {
    let f = fixture(1920, 1080);
    let mut req = f.request(
        "speech_bubble",
        Some(json!({"text": "word ".repeat(27).trim_end(), "size": 12})),
    );
    req.region = Some(BoxArgs {
        x: Some(100.0),
        y: Some(100.0),
        width: Some(220.0),
        height: Some(120.0),
    });
    assert!(matches!(plan(f.doc(), &req), Err(BlockError::Fit(_))));
}

#[test]
fn props_are_drawn_and_move_forever() {
    for id in [
        "prop_rocket",
        "prop_lightbulb",
        "prop_atom",
        "prop_planet",
        "prop_tree",
        "prop_house",
        "prop_laptop_2d",
        "prop_gear",
        "prop_cell",
        "prop_dna",
    ] {
        let mut f = fixture(1920, 1080);
        let empty = f.shot(0.0);
        let p = f.add(id, None);
        let video = f.doc().video(&f.video).unwrap();
        assert!(
            video.animations.iter().any(|a| a.repeat != Repeat::Once),
            "{id} has no looping motion"
        );
        let region = group_bounds(&f, &p.parts[0].1);
        let drawn = changed_in(&empty, &f.shot(0.0), region);
        assert!(drawn > 0.05, "{id} draws something: {drawn}");
        let (early, later) = (f.shot(150.0), f.shot(1250.0));
        assert!(changed_in(&early, &later, region) > 0.002, "{id} moves");
        assert!(od_render::check_text(f.doc().node(&f.frame).unwrap()).is_empty());
    }
}

#[test]
fn finite_loops_are_written_out_and_zero_loops_repeat_for_ever() {
    let mut f = fixture(1920, 1080);
    let p = f.add("prop_gear", Some(json!({"loops": 3, "period_ms": 2000})));
    let t = track(&f, &part(&p, "gear"), AnimProp::Rotation);
    assert_eq!(t.repeat, Repeat::Once);
    assert_eq!(t.keys.last().unwrap().at_ms, 6000);
    let mut g = fixture(1920, 1080);
    let p = g.add("prop_gear", Some(json!({"teeth": 14})));
    assert_eq!(
        track(&g, &part(&p, "gear"), AnimProp::Rotation).repeat,
        Repeat::Loop
    );
    let body = g.doc().node(&part(&p, "gear")).unwrap().children[0]
        .path
        .clone()
        .unwrap();
    assert_eq!(body.matches('L').count(), 14 * 3, "teeth are in the path");
}

#[test]
fn the_character_catalog_is_found_by_search() {
    let found: Vec<&str> = search("character", None).iter().map(|b| b.id).collect();
    assert_eq!(found.first(), Some(&"character"));
    let ids: Vec<&str> = search("", Category::parse("characters"))
        .iter()
        .map(|b| b.id)
        .collect();
    for id in [
        "character",
        "speech_bubble",
        "thought_bubble",
        "prop_rocket",
        "prop_lightbulb",
        "prop_atom",
        "prop_planet",
        "prop_tree",
        "prop_house",
        "prop_laptop_2d",
        "prop_gear",
        "prop_cell",
        "prop_dna",
    ] {
        assert!(ids.contains(&id), "{id}");
    }
}
