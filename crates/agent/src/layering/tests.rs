#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::{AnimProp, Animation};
use od_core::{Actor, Ease, Key, NodeId, Operation, Repeat, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::tools::{CallCtx, Content, Turns};

struct Fixture {
    _dir: TempDir,
    ws: Workspace,
    rect: NodeId,
    video: NodeId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Layers").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let frame = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: serde_json::from_value(json!({
                    "kind": "frame", "name": "Slide", "width": 400, "height": 300, "fill": "#ffffff"
                }))
                .unwrap(),
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    let rect = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: frame,
                index: None,
                node: serde_json::from_value(json!({
                    "kind": "rect", "name": "Card", "x": 100, "y": 20, "width": 80, "height": 40,
                    "fill": "#112233"
                }))
                .unwrap(),
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "Cut".into(),
                fps: 30,
                width: 400,
                height: 300,
            }],
        )
        .unwrap();
    let video = ws.session.document().videos[0].id.clone();
    Fixture {
        _dir: dir,
        ws,
        rect,
        video,
    }
}

fn call(ws: &mut Workspace, name: &str, args: Value) -> Result<Value, String> {
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

fn key(at_ms: u64, value: f64, ease: Ease) -> Key {
    Key { at_ms, value, ease }
}

impl Fixture {
    fn put(&mut self, prop: AnimProp, keys: Vec<Key>, repeat: Repeat, additive: bool) {
        self.ws
            .session
            .apply(
                Actor::User,
                None,
                &[Operation::Animate {
                    video: self.video.clone(),
                    node: self.rect.clone(),
                    prop,
                    keys,
                    repeat,
                    additive,
                }],
            )
            .unwrap();
    }

    fn animate_to(&mut self, extra: Value) -> Result<Value, String> {
        let mut args = json!({"video": self.video, "node": self.rect});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        call(&mut self.ws, "animate_to", args)
    }

    fn track(&self, prop: AnimProp, additive: bool) -> Option<Animation> {
        self.ws.session.document().videos[0]
            .animations
            .iter()
            .find(|a| a.prop == prop && a.additive == additive)
            .cloned()
    }

    fn keys(&self, prop: AnimProp) -> Vec<(u64, f64)> {
        self.track(prop, false)
            .unwrap()
            .keys
            .iter()
            .map(|k| (k.at_ms, k.value))
            .collect()
    }
}

fn lin() -> Ease {
    Ease::Linear
}

#[test]
fn a_node_with_no_track_starts_from_its_own_value() {
    let mut f = fixture();
    let out = f
        .animate_to(
            json!({"prop": "x", "to": 300, "at_ms": 1000, "duration_ms": 500, "ease": "linear"}),
        )
        .unwrap();
    assert_eq!(out["from"], 100.0);
    assert_eq!(f.keys(AnimProp::X), vec![(1000, 100.0), (1500, 300.0)]);
    let track = f.track(AnimProp::X, false).unwrap();
    assert_eq!(
        track.value_at(0.0),
        Some(100.0),
        "held at the design before it starts"
    );
    assert_eq!(track.value_at(1250.0), Some(200.0));
}

#[test]
fn an_interrupting_move_starts_where_the_old_track_is_with_no_jump() {
    let mut f = fixture();
    f.put(
        AnimProp::X,
        vec![key(0, 0.0, lin()), key(1000, 100.0, lin())],
        Repeat::Once,
        false,
    );
    let old = f.track(AnimProp::X, false).unwrap();
    let out = f
        .animate_to(
            json!({"prop": "x", "to": 300, "at_ms": 400, "duration_ms": 500, "ease": "linear"}),
        )
        .unwrap();
    assert_eq!(out["from"], 40.0);
    assert_eq!(out["replaced_keys"], 1);
    assert_eq!(
        f.keys(AnimProp::X),
        vec![(0, 0.0), (400, 40.0), (900, 300.0)]
    );
    let new = f.track(AnimProp::X, false).unwrap();
    for (t, v) in [(200.0, 20.0), (400.0, 40.0), (650.0, 170.0), (900.0, 300.0)] {
        assert!((new.value_at(t).unwrap() - v).abs() < 1e-9, "at {t}");
    }
    for t in (0..=400).step_by(20) {
        let t = f64::from(t);
        assert!((new.value_at(t).unwrap() - old.value_at(t).unwrap()).abs() < 1e-9);
    }
    assert_eq!(
        new.value_at(5000.0),
        Some(300.0),
        "the old key at 1000 is gone"
    );
}

#[test]
fn a_cut_curved_segment_keeps_its_shape_instead_of_being_squeezed() {
    let mut f = fixture();
    f.put(
        AnimProp::X,
        vec![key(0, 0.0, lin()), key(1000, 100.0, Ease::EaseInOut)],
        Repeat::Once,
        false,
    );
    let old = f.track(AnimProp::X, false).unwrap();
    f.animate_to(
        json!({"prop": "x", "to": 300, "at_ms": 400, "duration_ms": 500, "ease": "linear"}),
    )
    .unwrap();
    let new = f.track(AnimProp::X, false).unwrap();
    let worst = (0..=400)
        .step_by(10)
        .map(|t| (new.value_at(f64::from(t)).unwrap() - old.value_at(f64::from(t)).unwrap()).abs())
        .fold(0.0, f64::max);
    assert!(worst < 0.5, "the baked curve is {worst} off the old one");
    assert!((new.value_at(400.0).unwrap() - old.value_at(400.0).unwrap()).abs() < 1e-9);
    let naive = [(0.0, 0.0), (400.0, old.value_at(400.0).unwrap())];
    let squeezed = (0..=400)
        .step_by(10)
        .map(|t| {
            let t = f64::from(t);
            let line = naive[0].1 + (naive[1].1 - naive[0].1) * t / 400.0;
            (line - old.value_at(t).unwrap()).abs()
        })
        .fold(0.0, f64::max);
    assert!(
        squeezed > 5.0,
        "a plain two-key cut would be {squeezed} off"
    );
}

#[test]
fn a_move_at_a_key_keeps_that_keys_arrival_and_starts_from_its_value() {
    let mut f = fixture();
    f.put(
        AnimProp::X,
        vec![
            key(0, 0.0, lin()),
            key(500, 50.0, Ease::EaseIn),
            key(1000, 90.0, lin()),
        ],
        Repeat::Once,
        false,
    );
    f.animate_to(json!({"prop": "x", "to": 10, "at_ms": 500, "duration_ms": 300}))
        .unwrap();
    let keys = f.track(AnimProp::X, false).unwrap().keys;
    assert_eq!(keys[1], key(500, 50.0, Ease::EaseIn));
    assert_eq!(keys.len(), 3);
}

#[test]
fn keep_later_keeps_the_keys_after_the_move_and_the_default_replaces_them() {
    let mut f = fixture();
    let old = vec![
        key(0, 0.0, lin()),
        key(1000, 100.0, lin()),
        key(2000, 0.0, lin()),
    ];
    f.put(AnimProp::X, old.clone(), Repeat::Once, false);
    let out = f
        .animate_to(
            json!({"prop": "x", "to": 300, "at_ms": 400, "duration_ms": 200, "ease": "linear"}),
        )
        .unwrap();
    assert_eq!(out["replaced_keys"], 2);
    assert_eq!(f.keys(AnimProp::X).last(), Some(&(600, 300.0)));

    f.put(AnimProp::X, old, Repeat::Once, false);
    let out = f
        .animate_to(
            json!({"prop": "x", "to": 300, "at_ms": 400, "duration_ms": 200,
            "ease": "linear", "keep_later": true}),
        )
        .unwrap();
    assert_eq!(out["replaced_keys"], 0);
    assert_eq!(
        f.keys(AnimProp::X),
        vec![
            (0, 0.0),
            (400, 40.0),
            (600, 300.0),
            (1000, 100.0),
            (2000, 0.0)
        ]
    );
}

#[test]
fn the_additive_layer_is_left_alone_and_does_not_count_as_the_base() {
    let mut f = fixture();
    let bob = vec![
        key(0, 0.0, lin()),
        key(500, 5.0, lin()),
        key(1000, 0.0, lin()),
    ];
    f.put(AnimProp::X, bob.clone(), Repeat::Once, true);
    let out = f
        .animate_to(
            json!({"prop": "x", "to": 300, "at_ms": 0, "duration_ms": 1000, "ease": "linear"}),
        )
        .unwrap();
    assert_eq!(out["from"], 100.0, "the design's value, not the bob's");
    assert_eq!(f.track(AnimProp::X, true).unwrap().keys, bob);
    assert_eq!(f.keys(AnimProp::X), vec![(0, 100.0), (1000, 300.0)]);
}

#[test]
fn it_is_one_undoable_turn() {
    let mut f = fixture();
    f.put(
        AnimProp::X,
        vec![key(0, 0.0, lin()), key(1000, 100.0, lin())],
        Repeat::Once,
        false,
    );
    let before = f.keys(AnimProp::X);
    let out = f
        .animate_to(
            json!({"prop": "x", "to": 300, "at_ms": 400, "duration_ms": 500, "turn": "t-to"}),
        )
        .unwrap();
    assert_eq!(out["turn"], "t-to");
    assert_ne!(f.keys(AnimProp::X), before);
    f.ws.session.undo(Actor::agent("tester").unwrap()).unwrap();
    assert_eq!(f.keys(AnimProp::X), before);
}

#[test]
fn a_looping_track_a_missing_value_and_an_empty_move_are_refused() {
    let mut f = fixture();
    f.put(
        AnimProp::Y,
        vec![key(0, 0.0, lin()), key(500, 10.0, lin())],
        Repeat::Loop,
        false,
    );
    let looped = f
        .animate_to(json!({"prop": "y", "to": 90, "at_ms": 100, "duration_ms": 200}))
        .unwrap_err();
    assert!(looped.contains("loops"), "{looped}");

    let same = f
        .animate_to(json!({"prop": "width", "to": 80, "at_ms": 100, "duration_ms": 200}))
        .unwrap_err();
    assert!(same.contains("already"), "{same}");

    let device = f
        .animate_to(json!({"prop": "rotate_y", "to": 90, "at_ms": 100, "duration_ms": 200}))
        .unwrap_err();
    assert!(device.contains("no rotate_y"), "{device}");

    let none = f
        .animate_to(json!({"prop": "x", "to": 90, "at_ms": 100, "duration_ms": 0}))
        .unwrap_err();
    assert!(none.contains("empty"), "{none}");

    let colour = f
        .animate_to(json!({"prop": "fill_color", "to": 3, "at_ms": 100, "duration_ms": 200}))
        .unwrap_err();
    assert!(colour.contains("colour"), "{colour}");
    assert_eq!(f.ws.session.document().videos[0].animations.len(), 1);
}

#[test]
fn a_colour_goes_to_a_hex_string_and_the_camera_needs_no_node() {
    let mut f = fixture();
    f.animate_to(json!({"prop": "fill_color", "to": "#ff0000", "at_ms": 0, "duration_ms": 300}))
        .unwrap();
    let track = f.track(AnimProp::FillColor, false).unwrap();
    assert_eq!(
        track.keys[0].value,
        od_core::Rgba::parse_hex("#112233").unwrap().pack()
    );
    assert_eq!(
        track.keys[1].value,
        od_core::Rgba::parse_hex("#ff0000").unwrap().pack()
    );

    let args =
        json!({"video": f.video, "prop": "camera_zoom", "to": 2.0, "at_ms": 0, "duration_ms": 600});
    let out = call(&mut f.ws, "animate_to", args).unwrap();
    assert_eq!(out["from"], 1.0);
}

#[test]
fn the_tool_is_listed_with_its_required_arguments() {
    let tools = crate::tools::definitions();
    let tool = tools.iter().find(|t| t["name"] == "animate_to").unwrap();
    let required: Vec<&str> = tool["inputSchema"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(required, ["video", "prop", "to", "at_ms", "duration_ms"]);
}

fn preset(f: &mut Fixture, name: &str, extra: Value) -> Result<Value, String> {
    let mut args = json!({"video": f.video, "node": f.rect, "preset": name,
                          "start_ms": 0, "duration_ms": 900});
    args.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    call(&mut f.ws, "animate_preset", args)
}

#[test]
fn a_scene_move_on_the_additive_layer_rides_on_the_travel_without_touching_it() {
    let mut f = fixture();
    f.put(
        AnimProp::Y,
        vec![key(0, 20.0, lin()), key(900, 320.0, lin())],
        Repeat::Once,
        false,
    );
    let travel = f.track(AnimProp::Y, false).unwrap();
    preset(&mut f, "bob", json!({"layer": "additive", "turn": "bob"})).unwrap();
    assert_eq!(
        f.track(AnimProp::Y, false).unwrap(),
        travel,
        "the travel is untouched"
    );
    let bob = f.track(AnimProp::Y, true).expect("an additive track");
    assert!(bob.additive);
    let (lowest, highest) = bob.keys.iter().fold((f64::MAX, f64::MIN), |(lo, hi), k| {
        (lo.min(k.value), hi.max(k.value))
    });
    assert!(
        lowest < -1.0 && highest > 1.0,
        "offsets swing around rest: {lowest} to {highest}"
    );

    let doc = f.ws.session.document();
    let video = &doc.videos[0];
    for t in [150.0, 300.0, 450.0, 600.0] {
        let shown = doc.at_time(video, t).unwrap();
        let y = shown.node(&f.rect).unwrap().y;
        let expected = travel.value_at(t).unwrap() + bob.value_at(t).unwrap();
        assert!((y - expected).abs() < 1e-3, "at {t}: {y} vs {expected}");
    }
}

#[test]
fn the_same_move_without_a_layer_replaces_the_track_and_an_old_move_can_be_additive_too() {
    let mut f = fixture();
    preset(&mut f, "bob", json!({})).unwrap();
    assert!(
        f.track(AnimProp::Y, false).is_some(),
        "replace is the default"
    );
    assert!(f.track(AnimProp::Y, true).is_none());

    let mut g = fixture();
    preset(&mut g, "rise_in", json!({"layer": "additive"})).unwrap();
    assert!(g.track(AnimProp::Y, true).is_some());
    assert!(g.track(AnimProp::Y, false).is_none());
    let offset = g.track(AnimProp::Y, true).unwrap();
    assert!(offset.keys[0].value > 1.0 && offset.keys.last().unwrap().value.abs() < 1e-6);
    let doc = g.ws.session.document();
    let first = doc.at_time(&doc.videos[0], 0.0).unwrap();
    assert_eq!(first.node(&g.rect).unwrap().y, 20.0 + offset.keys[0].value);
    let last = doc.at_time(&doc.videos[0], 900.0).unwrap();
    assert_eq!(
        last.node(&g.rect).unwrap().y,
        20.0,
        "it ends where the design has it"
    );
}
