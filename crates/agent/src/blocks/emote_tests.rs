#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::geometry::Rect;
use od_core::{Actor, AnimProp, Repeat};
use serde_json::{Value, json};

use super::character_tests::{
    Fixture, SIZES, apply, at, bounds, changed_in, fixture, mid, part, track,
};
use super::*;

const EXPRESSIONS: [&str; 18] = [
    "neutral",
    "happy",
    "excited",
    "surprised",
    "shocked",
    "sad",
    "crying",
    "angry",
    "annoyed",
    "thinking",
    "confused",
    "sleepy",
    "bored",
    "smug",
    "worried",
    "scared",
    "love",
    "determined",
];

const MARKS: [&str; 13] = [
    "exclamation",
    "question",
    "sweat",
    "heart",
    "lightbulb",
    "zzz",
    "anger",
    "steam",
    "sparkle",
    "music",
    "rain_cloud",
    "dizzy",
    "stars",
];

fn boxed(f: &Fixture, block: &str, params: Value, region: Rect) -> Plan {
    let mut req = f.request(block, Some(params));
    req.region = Some(BoxArgs {
        x: Some(region.x),
        y: Some(region.y),
        width: Some(region.width),
        height: Some(region.height),
    });
    plan(f.doc(), &req).unwrap_or_else(|e| panic!("{block}: {e}"))
}

fn place(f: &mut Fixture, block: &str, params: Value, region: Rect) -> Plan {
    let p = boxed(f, block, params, region);
    apply(&mut f.ws, p.ops.clone());
    p
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn render(f: &Fixture, t_ms: f64, scale: f32) -> od_render::Pixmap {
    let video = f.doc().video(&f.video).unwrap();
    let doc = f
        .doc()
        .at_time(video, t_ms)
        .unwrap_or_else(|| f.doc().clone());
    od_render::render_frame(&doc, &f.frame, scale, f.ws.dir())
        .unwrap()
        .pixmap
}

/// A character whose head is about 120 px wide when the frame is 1920 x 1080.
fn small_character() -> Rect {
    rect(0.0, 0.0, 130.0, 182.0)
}

fn cell(i: usize, cols: usize, w: f64, h: f64, inner: Rect) -> Rect {
    let (col, row) = ((i % cols) as f64, (i / cols) as f64);
    rect(
        col * w + (w - inner.width) / 2.0,
        row * h + (h - inner.height) / 2.0,
        inner.width,
        inner.height,
    )
}

fn save(dir: &str, name: &str, pixmap: &od_render::Pixmap) {
    std::fs::create_dir_all(dir).unwrap();
    let png = od_render::encode_png(pixmap, false).unwrap();
    std::fs::write(format!("{dir}/{name}.png"), png).unwrap();
}

#[test]
fn dump_expression_sheet() {
    let Ok(dir) = std::env::var("OD_DUMP") else {
        return;
    };
    for style in ["blob", "bean", "robot", "creature"] {
        let mut f = fixture(1920, 1080);
        for (i, name) in EXPRESSIONS.iter().enumerate() {
            let at = cell(i, 6, 320.0, 360.0, small_character());
            place(
                &mut f,
                "character",
                json!({"expression": name, "style": style, "seed": i + 1}),
                at,
            );
        }
        save(&dir, &format!("faces_{style}"), &render(&f, 100.0, 1.0));
    }
    for (sheet, group) in EXPRESSIONS.chunks(6).enumerate() {
        let mut f = fixture(1920, 1080);
        for (i, name) in group.iter().enumerate() {
            place(
                &mut f,
                "character",
                json!({"expression": name, "style": "blob", "seed": i + 1}),
                cell(i, 3, 640.0, 540.0, rect(0.0, 0.0, 390.0, 520.0)),
            );
        }
        save(&dir, &format!("zoom_{sheet}"), &render(&f, 100.0, 1.0));
    }
}

#[test]
fn dump_motion_sheet() {
    let Ok(dir) = std::env::var("OD_DUMP") else {
        return;
    };
    let sheets: [(&str, [i64; 8]); 2] = [
        ("change", [-160, -80, -30, 0, 40, 80, 150, 300]),
        ("take", [110, 300, 600, 870, 950, 1030, 1190, 1400]),
    ];
    for (name, phases) in sheets {
        let mut f = fixture(1920, 1080);
        for (i, phase) in phases.iter().enumerate() {
            let when = u64::try_from(2000 - phase).unwrap();
            let params = if name == "change" {
                json!({"expression": "neutral", "expression_changes": [{"at_ms": when, "expression": "surprised"}], "seed": 3})
            } else {
                json!({"expression": "worried", "reaction": "take", "reaction_at_ms": when, "seed": 3})
            };
            place(
                &mut f,
                "character",
                params,
                cell(i, 4, 480.0, 540.0, rect(0.0, 0.0, 260.0, 364.0)),
            );
        }
        save(&dir, &format!("motion_{name}"), &render(&f, 2000.0, 1.0));
    }
}

#[test]
fn dump_emote_sheet() {
    let Ok(dir) = std::env::var("OD_DUMP") else {
        return;
    };
    let mut f = fixture(1920, 1080);
    for (i, mark) in MARKS.iter().enumerate() {
        let at = cell(i, 5, 384.0, 360.0, small_character());
        let who = place(
            &mut f,
            "character",
            json!({"expression": "worried", "seed": i + 1}),
            at,
        );
        place(
            &mut f,
            "emote",
            json!({"mark": mark, "target": part(&who, "head").to_string(), "seed": i + 1}),
            rect(0.0, 0.0, 300.0, 300.0),
        );
    }
    save(&dir, "emotes", &render(&f, 1200.0, 1.0));
}

#[test]
fn every_expression_draws_its_own_face() {
    let mut f = fixture(1920, 1080);
    let mut faces = Vec::new();
    let mut shots = Vec::new();
    for (i, name) in EXPRESSIONS.iter().enumerate() {
        let mut g = fixture(1920, 1080);
        let p = g.add("character", Some(json!({"expression": name, "seed": 3})));
        faces.push(bounds(&g, g.doc(), &part(&p, "head")));
        shots.push(g.shot(100.0));
        let _ = (&mut f, i);
    }
    let region = faces[0];
    for a in 0..EXPRESSIONS.len() {
        for b in (a + 1)..EXPRESSIONS.len() {
            let share = changed_in(&shots[a], &shots[b], region);
            assert!(
                share > 0.015,
                "{} and {} look alike in the face region: {share}",
                EXPRESSIONS[a],
                EXPRESSIONS[b]
            );
        }
    }
}

#[test]
fn every_expression_expands_at_three_sizes_in_one_undoable_step() {
    for (w, h) in SIZES {
        for style in ["blob", "bean", "robot", "creature"] {
            for name in EXPRESSIONS {
                let mut f = fixture(w, h);
                let before = f.doc().clone();
                let len = f.ws.session.log().len();
                let p = f.add(
                    "character",
                    Some(json!({"expression": name, "style": style, "glasses": true, "action": "talk"})),
                );
                assert_eq!(f.ws.session.log().len(), len + 1, "{style} {name}");
                f.doc().validate().unwrap();
                assert!(od_render::check_text(f.doc().node(&f.frame).unwrap()).is_empty());
                assert!(p.node_count < 400, "{style} {name}: {} nodes", p.node_count);
                f.ws.session.undo(Actor::User).unwrap();
                assert_eq!(f.doc().pages, before.pages);
            }
        }
    }
}

const CHANGES: &str = r#"[{"at_ms": 800, "expression": "surprised"}, {"at_ms": 2400, "expression": "sad"}, {"at_ms": 4000, "expression": "happy"}, {"at_ms": 5200, "expression": "surprised"}]"#;

fn acting(f: &mut Fixture) -> Plan {
    let changes: Value = serde_json::from_str(CHANGES).unwrap();
    f.add(
        "character",
        Some(json!({"expression": "neutral", "expression_changes": changes, "seed": 2})),
    )
}

#[test]
fn expression_changes_write_valid_monotonic_keys_that_swap_the_faces() {
    let mut f = fixture(1920, 1080);
    let p = acting(&mut f);
    f.doc().validate().unwrap();
    for name in ["face", "surprised_face", "sad_face", "happy_face"] {
        let id = part(&p, name);
        for prop in [AnimProp::Opacity, AnimProp::ScaleX, AnimProp::ScaleY] {
            let keys = track(&f, &id, prop).keys;
            assert!(
                keys.windows(2).all(|w| w[0].at_ms < w[1].at_ms),
                "{name} {prop:?} keys increase"
            );
            assert!(keys.len() < 60);
        }
    }
    let opacity = |t: f64, name: &str| {
        let doc = at(&f, t);
        doc.node(&part(&p, name)).unwrap().opacity
    };
    assert_eq!(opacity(0.0, "face"), 1.0);
    assert_eq!(opacity(0.0, "surprised_face"), 0.0);
    for (t, shown) in [
        (1200.0, "surprised_face"),
        (3000.0, "sad_face"),
        (4600.0, "happy_face"),
        (6000.0, "surprised_face"),
    ] {
        assert_eq!(opacity(t, shown), 1.0, "{shown} shows at {t}");
        for other in ["face", "surprised_face", "sad_face", "happy_face"] {
            if other != shown {
                assert_eq!(opacity(t, other), 0.0, "{other} is hidden at {t}");
            }
        }
    }
    assert!(
        (0..120).any(|i| {
            let t = 800.0 + f64::from(i) * 10.0;
            let o = opacity(t, "face");
            o > 0.05 && o < 0.95
        }),
        "the swap cross-fades instead of cutting"
    );
}

#[test]
fn a_change_anticipates_then_overshoots_instead_of_snapping() {
    let mut f = fixture(1920, 1080);
    let p = acting(&mut f);
    let scale_y = |t: f64, name: &str| at(&f, t).node(&part(&p, name)).unwrap().scale_y;
    assert!(scale_y(770.0, "face") < 0.9, "the old face squints first");
    assert!(
        (860..1000).any(|t| scale_y(f64::from(t), "surprised_face") > 1.08),
        "the new face overshoots"
    );
    assert!(
        (scale_y(1300.0, "surprised_face") - 1.0).abs() < 0.01,
        "and settles"
    );
    assert!((scale_y(0.0, "face") - 1.0).abs() < 1e-9);
}

#[test]
fn the_new_face_has_its_own_blink_pupils_and_mouth() {
    let mut f = fixture(1920, 1080);
    let changes: Value = serde_json::from_str(CHANGES).unwrap();
    let p = f.add(
        "character",
        Some(
            json!({"expression": "neutral", "expression_changes": changes, "talk": [[1000, 2000]]}),
        ),
    );
    for name in [
        "sad_eye_l",
        "sad_eye_r",
        "sad_pupil_l",
        "sad_mouth_open",
        "surprised_eyes",
        "happy_brows",
    ] {
        part(&p, name);
    }
    let blink = track(&f, &part(&p, "sad_eye_l"), AnimProp::ScaleY);
    assert_ne!(blink.repeat, Repeat::Once);
    track(&f, &part(&p, "happy_mouth_open"), AnimProp::ScaleY);
}

#[test]
fn bad_expression_changes_are_refused_with_a_reason() {
    let f = fixture(1920, 1080);
    let err = |changes: Value| {
        plan(
            f.doc(),
            &f.request(
                "character",
                Some(json!({"expression": "happy", "expression_changes": changes})),
            ),
        )
        .err()
        .map(|e| e.to_string())
    };
    assert!(
        err(json!([{"at_ms": 100, "expression": "sad"}]))
            .unwrap()
            .contains("200")
    );
    assert!(
        err(json!([{"at_ms": 1000, "expression": "sad"}, {"at_ms": 1200, "expression": "angry"}]))
            .unwrap()
            .contains("500")
    );
    assert!(
        err(json!([{"at_ms": 1000, "expression": "sad"}, {"at_ms": 900, "expression": "angry"}]))
            .unwrap()
            .contains("time order")
    );
    assert!(
        err(json!([{"at_ms": 1000, "expression": "happy"}]))
            .unwrap()
            .contains("already")
    );
    assert!(
        err(json!([{"at_ms": 1000, "expression": "gloomy"}]))
            .unwrap()
            .contains("one of")
    );
    assert!(err(json!({"at_ms": 1000})).unwrap().contains("list"));
    let many: Vec<Value> = ["sad", "angry", "shocked", "love", "bored", "smug", "scared"]
        .iter()
        .enumerate()
        .map(|(i, e)| json!({"at_ms": 1000 + 600 * i, "expression": e}))
        .collect();
    assert!(err(Value::Array(many)).unwrap().contains("different"));
    let too_many: Vec<Value> = (0..41)
        .map(|i| json!({"at_ms": 1000 + 600 * i, "expression": if i % 2 == 0 {"sad"} else {"happy"}}))
        .collect();
    assert!(err(Value::Array(too_many)).unwrap().contains("at most"));
}

#[test]
fn a_character_with_many_faces_stays_inside_the_node_cap() {
    let changes: Vec<Value> = ["sad", "angry", "shocked", "love", "bored"]
        .iter()
        .enumerate()
        .map(|(i, e)| json!({"at_ms": 1000 + 700 * i, "expression": e}))
        .collect();
    for (w, h) in SIZES {
        let mut f = fixture(w, h);
        let before = f.doc().clone();
        let len = f.ws.session.log().len();
        let p = f.add(
            "character",
            Some(json!({"expression": "crying", "style": "creature", "hat": true, "glasses": true, "backpack": true, "expression_changes": changes})),
        );
        assert_eq!(f.ws.session.log().len(), len + 1);
        assert!(p.node_count < 400, "{} nodes", p.node_count);
        f.doc().validate().unwrap();
        f.ws.session.undo(Actor::User).unwrap();
        assert_eq!(f.doc().videos, before.videos);
    }
}

#[test]
fn a_take_squashes_and_stretches_the_body_and_looks_away_first() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "character",
        Some(json!({"reaction": "take", "reaction_at_ms": 500, "seed": 4})),
    );
    let (rig, head) = (part(&p, "rig"), part(&p, "head"));
    let get = |t: f64, id: &od_core::NodeId| {
        let doc = at(&f, t);
        let n = doc.node(id).unwrap();
        (n.scale_x, n.scale_y, n.x)
    };
    assert_eq!(get(0.0, &rig).1, 1.0);
    assert!(
        get(500.0 + 870.0, &rig).1 < 0.93,
        "squashes before the snap back"
    );
    assert!(
        get(500.0 + 1030.0, &rig).1 > 1.12,
        "stretches as it snaps back"
    );
    assert!(get(500.0 + 1030.0, &rig).0 < 0.93, "and narrows");
    assert!((get(3000.0, &rig).1 - 1.0).abs() < 1e-6, "then settles");
    let rest = get(0.0, &head).2;
    assert!(
        get(500.0 + 600.0, &head).2 < rest - 2.0,
        "the head turns away in between"
    );
    let (calm, taking) = (f.shot(300.0), f.shot(500.0 + 1030.0));
    let region = bounds(&f, &at(&f, 0.0), &head);
    assert!(
        changed_in(&calm, &taking, region) > 0.05,
        "the take shows in the pixels"
    );
}

#[test]
fn nod_shake_shrug_and_laugh_move_what_they_should() {
    let at_ms = 400;
    let react = |reaction: &str, extra: Value| {
        let mut f = fixture(1920, 1080);
        let mut params = json!({"reaction": reaction, "reaction_at_ms": at_ms, "seed": 4});
        params
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let p = f.add("character", Some(params));
        (f, p)
    };
    let value = |f: &Fixture, p: &Plan, part_name: &str, t: f64, prop: AnimProp| {
        let doc = at(f, t);
        let n = doc.node(&part(p, part_name)).unwrap();
        match prop {
            AnimProp::X => n.x,
            AnimProp::Y => n.y,
            AnimProp::ScaleY => n.scale_y,
            AnimProp::Rotation => n.rotation,
            _ => unreachable!(),
        }
    };
    let (f, p) = react("nod", json!({}));
    assert!(
        value(&f, &p, "head", 400.0 + 130.0, AnimProp::ScaleY) < 0.92,
        "the head dips"
    );
    assert!(
        value(&f, &p, "head", 400.0 + 300.0, AnimProp::ScaleY) > 0.99,
        "and comes up"
    );
    let (f, p) = react("shake_head", json!({}));
    let (left, right) = (
        value(&f, &p, "head", 400.0 + 110.0, AnimProp::X),
        value(&f, &p, "head", 400.0 + 330.0, AnimProp::X),
    );
    assert!(
        right - left > 4.0,
        "the head swings both ways: {left} {right}"
    );
    let (f, p) = react("shrug", json!({}));
    let (rest, up) = (
        value(&f, &p, "arm_r", 0.0, AnimProp::Rotation),
        value(&f, &p, "arm_r", 400.0 + 900.0, AnimProp::Rotation),
    );
    assert!(rest - up > 25.0, "the arms go out: {rest} {up}");
    assert!(
        value(&f, &p, "arm_l", 400.0 + 900.0, AnimProp::Y)
            < value(&f, &p, "arm_l", 0.0, AnimProp::Y) - 1.0,
        "and the shoulders rise"
    );
    let (f, p) = react("laugh", json!({"reaction_ms": 1500}));
    let y0 = value(&f, &p, "rig", 0.0, AnimProp::Y);
    assert!(
        value(&f, &p, "rig", 400.0 + 110.0, AnimProp::Y) < y0 - 3.0,
        "the body hops"
    );
    assert!(
        (value(&f, &p, "rig", 400.0 + 330.0, AnimProp::Y) - y0).abs() < 0.5,
        "and lands"
    );
    assert!(
        (value(&f, &p, "rig", 6000.0, AnimProp::Y) - y0).abs() < 1e-6,
        "then stops"
    );
    let open = track(&f, &part(&p, "mouth_open"), AnimProp::ScaleY);
    assert!(
        open.keys.first().unwrap().at_ms <= 400,
        "the mouth flaps while it laughs"
    );
}

#[test]
fn a_reaction_that_fights_the_action_is_refused() {
    let f = fixture(1920, 1080);
    let err = |p: Value| {
        plan(f.doc(), &f.request("character", Some(p)))
            .err()
            .map(|e| e.to_string())
    };
    assert!(
        err(json!({"action": "jump", "reaction": "take"}))
            .unwrap()
            .contains("body")
    );
    assert!(
        err(json!({"action": "wave", "reaction": "shrug"}))
            .unwrap()
            .contains("arms")
    );
    assert!(
        err(json!({"action": "walk", "reaction": "laugh"}))
            .unwrap()
            .contains("body")
    );
    assert!(err(json!({"action": "talk", "reaction": "nod"})).is_none());
}

#[test]
fn looks_send_the_eyes_first_and_the_face_after() {
    let mut f = fixture(1920, 1080);
    let p = f.add(
        "character",
        Some(json!({"looks": [{"at_ms": 1000, "x": 100, "y": 300}, {"at_ms": 2500, "x": 1800, "y": 200}]})),
    );
    let pupil = part(&p, "pupil_l");
    let face = part(&p, "face");
    let x = |t: f64, id: &od_core::NodeId| at(&f, t).node(id).unwrap().x;
    let (p0, f0) = (x(900.0, &pupil), x(900.0, &face));
    assert!(
        x(1130.0, &pupil) < p0 - 1.0,
        "the eyes have turned left by 1130 ms"
    );
    assert!(
        (x(1050.0, &face) - f0).abs() < 1e-6,
        "the face has not moved yet at 1050 ms"
    );
    assert!(x(1500.0, &face) < f0 - 0.5, "and follows afterwards");
    assert!(
        x(2800.0, &pupil) > p0 + 1.0,
        "the next look sends the eyes right"
    );
    let bad = |looks: Value| {
        plan(
            f.doc(),
            &f.request("character", Some(json!({"looks": looks}))),
        )
        .err()
        .map(|e| e.to_string())
    };
    assert!(
        bad(json!([{"at_ms": 1000, "x": 1, "y": 1}, {"at_ms": 1200, "x": 2, "y": 2}]))
            .unwrap()
            .contains("400")
    );
    assert!(
        bad(json!([{"at_ms": 1000, "x": 1}]))
            .unwrap()
            .contains("at_ms")
    );
}

#[test]
fn look_at_turns_the_face_toward_the_target() {
    let mut f = fixture(1920, 1080);
    let p = f.add("character", Some(json!({"look_at": {"x": 100, "y": 500}})));
    let doc = f.doc();
    let face = doc.node(&part(&p, "face")).unwrap();
    assert!(
        face.x < -0.5,
        "the face sits left of the head's middle: {}",
        face.x
    );
}

fn mark_region(f: &Fixture, p: &Plan) -> Rect {
    bounds(f, &at(f, 5000.0), &part(p, "mark"))
}

#[test]
fn every_emote_pops_with_overshoot_holds_and_moves_by_itself() {
    for mark in MARKS {
        let mut f = fixture(1920, 1080);
        let who = f.add("character", Some(json!({"expression": "worried"})));
        let head = part(&who, "head").to_string();
        let mut req = f.request("emote", Some(json!({"mark": mark, "target": head})));
        req.start_ms = Some(1000);
        req.hold_ms = Some(2500);
        let p = plan(f.doc(), &req).unwrap_or_else(|e| panic!("{mark}: {e}"));
        apply(&mut f.ws, p.ops.clone());
        f.doc().validate().unwrap();
        assert!(p.node_count < 120, "{mark}: {} nodes", p.node_count);
        let root = part(&p, "mark");
        let scale = |t: f64| at(&f, t).node(&root).unwrap().scale_x;
        assert!(scale(900.0) < 0.05, "{mark} is not there before start_ms");
        assert!(
            (1000..1380).step_by(20).any(|t| scale(f64::from(t)) > 1.02),
            "{mark} overshoots"
        );
        for t in [1500.0, 2500.0, 3300.0] {
            assert!(
                (scale(t) - 1.0).abs() < 1e-6,
                "{mark} holds its size at {t}"
            );
        }
        let video = f.doc().video(&f.video).unwrap();
        assert!(
            video
                .animations
                .iter()
                .any(|a| a.repeat != Repeat::Once && a.node != root),
            "{mark} has idle motion"
        );
        let region = bounds(&f, &at(&f, 2000.0), &root);
        let (before, shown) = (f.shot(900.0), f.shot(2000.0));
        assert!(
            changed_in(&before, &shown, region) > 0.05,
            "{mark} is drawn on the frame"
        );
        let (a, b) = (f.shot(2000.0), f.shot(2240.0));
        assert!(
            changed_in(&a, &b, region) > 0.003,
            "{mark} moves while it holds"
        );
        let faded_out = at(&f, 1000.0 + 2500.0 + 600.0);
        assert!(
            faded_out.node(&part(&p, "mark")).is_some()
                && faded_out.node(&p.parts[0].1).unwrap().opacity < 0.01,
            "{mark} is gone after hold_ms"
        );
    }
}

#[test]
fn emotes_stay_in_frame_at_three_sizes_and_undo_in_one_step() {
    for (w, h) in SIZES {
        for mark in MARKS {
            for corner in [(0.02, 0.05), (0.5, 0.0), (0.97, 0.3)] {
                let mut f = fixture(w, h);
                let (fw, fh) = (f64::from(w), f64::from(h));
                let target = json!({"x": fw * corner.0, "y": fh * corner.1, "width": fw * 0.12, "height": fw * 0.12});
                let before = f.doc().clone();
                let len = f.ws.session.log().len();
                let p = f.add(
                    "emote",
                    Some(json!({"mark": mark, "target": target, "side": "above", "scale": 2.0})),
                );
                assert_eq!(f.ws.session.log().len(), len + 1);
                let r = mark_region(&f, &p);
                assert!(
                    r.x >= -0.5
                        && r.y >= -0.5
                        && r.x + r.width <= fw + 0.5
                        && r.y + r.height <= fh + 0.5,
                    "{mark} at {corner:?} {w}x{h}: {r:?}"
                );
                f.doc().validate().unwrap();
                f.ws.session.undo(Actor::User).unwrap();
                assert_eq!(f.doc().pages, before.pages);
                assert_eq!(f.doc().videos, before.videos);
            }
        }
    }
}

#[test]
fn an_emote_sits_beside_its_target_and_takes_its_color_from_the_tokens() {
    let fill_with_accent = |accent: &str| {
        let mut f = fixture(1920, 1080);
        apply(
            &mut f.ws,
            vec![od_core::Operation::SetToken {
                name: "brand.accent".into(),
                value: Some(accent.into()),
            }],
        );
        let who = f.add("character", Some(json!({"style": "bean", "seed": 2})));
        let head = bounds(&f, f.doc(), &part(&who, "head"));
        let p = f.add(
            "emote",
            Some(json!({"mark": "sparkle", "target": part(&who, "head").to_string()})),
        );
        let mark = mid(mark_region(&f, &p));
        assert!(
            mark.1 < head.y + head.height * 0.2,
            "a sparkle sits up by the head"
        );
        assert!(mark.0 > head.x + head.width * 0.5, "and to its right");
        let item = f.doc().node(&part(&p, "item_0")).unwrap();
        serde_json::to_string(&item.children[0].fill).unwrap()
    };
    let (warm, cool) = (fill_with_accent("#ff5500"), fill_with_accent("#0055ff"));
    assert_ne!(warm, cool, "the mark's palette follows the accent token");
}

#[test]
fn bad_emote_requests_are_refused() {
    let f = fixture(1920, 1080);
    let err = |p: Value| {
        plan(f.doc(), &f.request("emote", Some(p)))
            .err()
            .map(|e| e.to_string())
    };
    assert!(err(json!({"mark": "poop"})).unwrap().contains("one of"));
    assert!(err(json!({"scale": 9})).is_some());
    assert!(err(json!({"count": 9})).is_some());
    assert!(err(json!({"color": "{nope}"})).is_some());
    assert!(err(json!({"side": "inside"})).is_some());
    let mut req = f.request("emote", Some(json!({})));
    req.hold_ms = Some(10);
    assert!(
        plan(f.doc(), &req)
            .err()
            .unwrap()
            .to_string()
            .contains("entrance")
    );
}
