#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, AnimProp, NodeId, Operation, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::tools::{CallCtx, Content, Turns};

use super::position::{Position, TimelineState, check_label, resolve_position};
use super::*;

/// The timeline the research table is written against: two steps and one label.
fn fixture() -> TimelineState {
    let mut tl = TimelineState::default();
    tl.push(0.0, 500.0);
    tl.push(600.0, 300.0);
    tl.set_label("x", 300.0).unwrap();
    tl
}

fn at(text: &str) -> Result<f64, PositionError> {
    resolve_position(&fixture(), Some(&Position::Expr(text.into())))
}

#[test]
fn a_position_is_read_against_the_timeline() {
    let tl = fixture();
    assert_eq!(resolve_position(&tl, None).unwrap(), 900.0);
    assert_eq!(
        resolve_position(&tl, Some(&Position::Abs(250.0))).unwrap(),
        250.0
    );
    assert_eq!(
        resolve_position(&tl, Some(&Position::Expr("250".into()))).unwrap(),
        250.0
    );
    assert_eq!(at("+=100").unwrap(), 1000.0);
    assert_eq!(at("-=200").unwrap(), 700.0);
    assert_eq!(at("*=.5").unwrap(), 450.0);
    assert_eq!(at("<").unwrap(), 900.0);
    assert_eq!(at("<<").unwrap(), 600.0);
    assert_eq!(at("<+=50").unwrap(), 950.0);
    assert_eq!(at("<<+=250").unwrap(), 850.0);
    assert_eq!(at("<-=100").unwrap(), 800.0);
    assert_eq!(at("<<-=100").unwrap(), 500.0);
    assert_eq!(at("<*=2").unwrap(), 1800.0);
    assert_eq!(at("x").unwrap(), 300.0);
    assert_eq!(at("x+=100").unwrap(), 400.0);
    assert_eq!(at("x-=50").unwrap(), 250.0);
    assert_eq!(at("x*=2").unwrap(), 600.0);
}

#[test]
fn an_unknown_label_is_refused_and_an_empty_timeline_starts_at_zero() {
    assert!(matches!(at("y"), Err(PositionError::UnknownLabel(_))));
    assert!(matches!(at("y+=100"), Err(PositionError::UnknownLabel(_))));
    assert!(matches!(at(""), Err(PositionError::Empty)));
    let empty = TimelineState::default();
    assert_eq!(
        resolve_position(&empty, Some(&Position::Expr("<".into()))).unwrap(),
        0.0
    );
    assert_eq!(resolve_position(&empty, None).unwrap(), 0.0);
    assert!(matches!(at("-=1000"), Err(PositionError::OutOfBounds(_))));
}

#[test]
fn a_step_with_a_delay_starts_after_its_offset() {
    let mut tl = TimelineState::default();
    tl.push(1_100.0, 200.0);
    assert_eq!(
        resolve_position(&tl, Some(&Position::Expr("<".into()))).unwrap(),
        1_300.0
    );
    assert_eq!(
        resolve_position(&tl, Some(&Position::Expr("<<".into()))).unwrap(),
        1_100.0
    );
}

#[test]
fn a_retime_scales_every_step_and_label() {
    let mut tl = TimelineState::default();
    tl.push(500.0, 500.0);
    tl.push(1_500.0, 500.0);
    tl.set_label("a", 500.0).unwrap();
    assert_eq!(tl.duration(), 2_000.0);
    tl.retime(1_000.0).unwrap();
    assert_eq!(tl.children[0].start_ms, 250.0);
    assert_eq!(tl.children[0].dur_ms, 250.0);
    assert_eq!(tl.labels["a"], 250.0);
    assert_eq!(tl.duration(), 1_000.0);
}

#[test]
fn a_label_must_not_read_as_a_position() {
    assert!(check_label("intro").is_ok());
    assert!(check_label("").is_err());
    assert!(check_label("two words").is_err());
    assert!(check_label("<intro").is_err());
    assert!(check_label("in+=tro").is_err());
    assert!(check_label("in+tro").is_ok());
    assert!(check_label("250").is_err());
}

struct Scene {
    _dir: TempDir,
    ws: Workspace,
    frame: NodeId,
    video: NodeId,
}

fn scene(w: u32, h: u32) -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Scene").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let created = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: serde_json::from_value(json!({
                    "kind": "frame", "name": "Scene", "width": w, "height": h, "fill": "#0d0f14"
                }))
                .unwrap(),
            }],
        )
        .unwrap();
    let frame = created.created[0].clone();
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "Cut".into(),
                fps: 30,
                width: w,
                height: h,
            }],
        )
        .unwrap();
    let video = ws.session.document().videos[0].id.clone();
    Scene {
        _dir: dir,
        ws,
        frame,
        video,
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

fn rect(scene: &mut Scene, x: f64) -> String {
    let created = scene
        .ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: scene.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({
                    "kind": "rect", "x": x, "y": 100, "width": 100, "height": 100, "fill": "#ffffff"
                }))
                .unwrap(),
            }],
        )
        .unwrap();
    created.created[0].to_string()
}

#[test]
fn a_sequence_places_its_steps_and_undoes_in_one_step() {
    let mut s = scene(1280, 720);
    let title = rect(&mut s, 100.0);
    let sub = rect(&mut s, 300.0);
    let video = s.video.to_string();
    let out = call_tool(
        &mut s.ws,
        "sequence",
        json!({
            "video": video,
            "items": [
                {"label": "intro", "node": title, "preset": "pop_in", "duration_ms": 400, "at": 0},
                {"node": sub, "preset": "fade_up", "duration_ms": 300, "at": "intro+=200"}
            ]
        }),
    )
    .unwrap();
    let table = out["sequence"].as_array().unwrap();
    assert_eq!(table.len(), 2);
    assert_eq!(table[0]["start_ms"], 0);
    assert_eq!(table[0]["end_ms"], 400);
    assert_eq!(table[1]["start_ms"], 200);
    assert_eq!(table[1]["end_ms"], 500);
    assert_eq!(out["labels"]["intro"], 0);
    {
        let v = s.ws.session.document().video(&s.video).unwrap();
        let title_id = NodeId::parse(&title).unwrap();
        let sub_id = NodeId::parse(&sub).unwrap();
        let times = |id: &NodeId, prop: AnimProp| {
            v.animations
                .iter()
                .find(|a| a.node == *id && a.prop == prop)
                .map(|a| a.keys.iter().map(|k| k.at_ms).collect::<Vec<u64>>())
                .unwrap_or_default()
        };
        let scale = times(&title_id, AnimProp::ScaleX);
        assert_eq!(scale.first().copied(), Some(0));
        assert_eq!(scale.last().copied(), Some(400));
        let y = times(&sub_id, AnimProp::Y);
        assert_eq!(y.first().copied(), Some(200));
    }
    s.ws.session.undo(Actor::agent("tester").unwrap()).unwrap();
    let v = s.ws.session.document().video(&s.video).unwrap();
    assert!(
        v.animations.is_empty(),
        "one undo takes the whole sequence away"
    );
}

#[test]
fn a_sequence_fits_itself_to_a_length() {
    let mut s = scene(1280, 720);
    let title = rect(&mut s, 100.0);
    let sub = rect(&mut s, 300.0);
    let video = s.video.to_string();
    let out = call_tool(
        &mut s.ws,
        "sequence",
        json!({
            "video": video,
            "items": [
                {"node": title, "preset": "pop_in", "duration_ms": 400, "at": 0},
                {"node": sub, "preset": "fade_up", "duration_ms": 400, "at": "<+=200"}
            ],
            "fit_to_ms": 500
        }),
    )
    .unwrap();
    let table = out["sequence"].as_array().unwrap();
    assert_eq!(table[0]["start_ms"], 0);
    assert_eq!(table[0]["end_ms"], 200);
    assert_eq!(table[1]["start_ms"], 300);
    assert_eq!(table[1]["end_ms"], 500);
    assert_eq!(out["total_ms"], 500);
}

#[test]
fn a_sequence_refuses_what_it_cannot_place() {
    let mut s = scene(1280, 720);
    let title = rect(&mut s, 100.0);
    let video = s.video.to_string();
    let mut bad = |items: Value| {
        call_tool(
            &mut s.ws,
            "sequence",
            json!({"video": video, "items": items}),
        )
        .is_err()
    };
    assert!(bad(
        json!([{"node": title, "preset": "pop_in", "at": "nowhere"}])
    ));
    assert!(bad(
        json!([{"node": title, "preset": "pop_in", "at": "-=500"}])
    ));
    assert!(bad(json!([{"preset": "pop_in", "at": 0}])));
    assert!(bad(
        json!([{"node": title, "preset": "not_a_move", "at": 0}])
    ));
}

#[test]
fn a_spring_sends_a_property_to_its_value_and_says_when_it_is_done() {
    let mut s = scene(1280, 720);
    let title = rect(&mut s, 100.0);
    let video = s.video.to_string();
    let out = call_tool(
        &mut s.ws,
        "spring_to",
        json!({"video": video, "node": title, "prop": "x", "to": 400.0, "visual_ms": 300, "bounce": 0.2}),
    )
    .unwrap();
    assert_eq!(out["at_ms"], 0);
    assert_eq!(out["settle_ms"], out["end_ms"].as_u64().unwrap());
    assert!(out["settle_ms"].as_u64().unwrap() > 300);
    assert_eq!(out["perceived_ms"], 300);
    let v = s.ws.session.document().video(&s.video).unwrap();
    let title_id = NodeId::parse(&title).unwrap();
    let x = v
        .animations
        .iter()
        .find(|a| a.node == title_id && a.prop == AnimProp::X)
        .unwrap();
    assert_eq!(x.keys.len(), 2);
    assert_eq!(x.keys[0].value, 100.0);
    assert_eq!(x.keys[1].value, 400.0);
    assert_eq!(x.keys[1].at_ms, out["end_ms"].as_u64().unwrap());
}
