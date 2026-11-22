#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::{AnimProp, Repeat};
use od_core::{Actor, NodeId, Operation, Workspace};
use serde_json::{Value, json};

use super::*;
use crate::tools::Turns;

struct Fixture {
    _dir: tempfile::TempDir,
    ws: Workspace,
    video: String,
    node: NodeId,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Noise").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let apply = |ws: &mut Workspace, ops: Value| {
        let ops: Vec<Operation> = serde_json::from_value(ops).unwrap();
        ws.session.apply(Actor::User, None, &ops).unwrap()
    };
    let frame = apply(
        &mut ws,
        json!([{"op": "create", "parent": root.as_str(), "node":
            {"kind": "frame", "name": "scene", "width": 400, "height": 300, "fill": "#000000"}}]),
    )
    .created[0]
        .to_string();
    let node = apply(
        &mut ws,
        json!([{"op": "create", "parent": frame, "node":
            {"kind": "rect", "name": "Box", "x": 100, "y": 100, "width": 50, "height": 50,
             "fill": "#ff0000"}}]),
    )
    .created[0]
        .clone();
    apply(
        &mut ws,
        json!([{"op": "create_video", "name": "V", "fps": 30, "width": 400, "height": 300}]),
    );
    let video = ws.session.document().videos[0].id.to_string();
    apply(
        &mut ws,
        json!([{"op": "add_clip", "video": video, "frame": frame, "duration_ms": 3000}]),
    );
    Fixture {
        _dir: dir,
        ws,
        video,
        node,
    }
}

fn call(f: &mut Fixture, extra: Value) -> Result<Value, String> {
    let mut args = json!({"video": f.video, "node": f.node, "amplitude": 10.0});
    args.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws: &mut f.ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, "animate_noise", args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

impl Fixture {
    fn track(&self, prop: AnimProp) -> od_core::animation::Animation {
        self.ws.session.document().videos[0]
            .animations
            .iter()
            .find(|t| t.node == self.node && t.prop == prop)
            .cloned()
            .unwrap()
    }
}

#[test]
fn two_properties_get_two_different_streams_inside_the_amplitude_on_the_frame_grid() {
    let mut f = fixture();
    let out = call(
        &mut f,
        json!({"props": ["x", "y"], "frequency": 4.0, "seed": 5}),
    )
    .unwrap();
    assert_eq!(out["noise"]["keys_each"], 46);
    let (x, y) = (f.track(AnimProp::X), f.track(AnimProp::Y));
    assert!(x.additive && y.additive);
    assert!(x.keys.iter().all(|k| k.value.abs() <= 10.0));
    assert!(x.keys.windows(2).all(|w| w[0].at_ms < w[1].at_ms));
    assert_ne!(
        x.keys.iter().map(|k| k.value).collect::<Vec<_>>(),
        y.keys.iter().map(|k| k.value).collect::<Vec<_>>()
    );
    assert!(
        x.keys.iter().any(|k| k.value.abs() > 4.0),
        "the curve should move"
    );
}

#[test]
fn the_same_seed_writes_the_same_keys() {
    let (mut a, mut b) = (fixture(), fixture());
    call(&mut a, json!({"prop": "rotation", "seed": 9})).unwrap();
    call(&mut b, json!({"prop": "rotation", "seed": 9})).unwrap();
    assert_eq!(
        a.track(AnimProp::Rotation).keys,
        b.track(AnimProp::Rotation).keys
    );
    call(&mut b, json!({"prop": "rotation", "seed": 10})).unwrap();
    assert_ne!(
        a.track(AnimProp::Rotation).keys,
        b.track(AnimProp::Rotation).keys
    );
}

#[test]
fn a_loop_ends_on_its_first_value_and_repeats() {
    let mut f = fixture();
    let out = call(
        &mut f,
        json!({"prop": "y", "loop": true, "to_ms": 2000, "frequency": 1.0}),
    )
    .unwrap();
    assert_eq!(out["noise"]["loop"], true);
    let track = f.track(AnimProp::Y);
    assert_eq!(track.repeat, Repeat::Loop);
    assert_eq!(
        track.keys.first().unwrap().value,
        track.keys.last().unwrap().value
    );
    assert_eq!(track.keys.last().unwrap().at_ms, 2000);
}

#[test]
fn an_absolute_track_moves_around_its_center() {
    let mut f = fixture();
    call(
        &mut f,
        json!({"prop": "opacity", "amplitude": 0.1, "center": 0.8, "additive": false}),
    )
    .unwrap();
    let track = f.track(AnimProp::Opacity);
    assert!(!track.additive);
    assert!(track.keys.iter().all(|k| (0.7..=0.9).contains(&k.value)));
}

#[test]
fn bad_arguments_are_refused_with_what_would_work() {
    let mut f = fixture();
    for (extra, expect) in [
        (json!({}), "give `prop`"),
        (json!({"prop": "x", "props": ["y"]}), "give `prop`"),
        (json!({"prop": "x", "amplitude": 0.0}), "other than 0"),
        (json!({"prop": "x", "frequency": 99.0}), "frequency"),
        (json!({"prop": "x", "octaves": 9}), "octaves"),
        (json!({"prop": "x", "from_ms": 5000}), "empty"),
        (
            json!({"prop": "x", "keys_per_second": 1}),
            "keys_per_second",
        ),
        (json!({"props": ["x", "y", "x", "y", "x"]}), "1 to 4"),
        (json!({"prop": "nonsense"}), "invalid arguments"),
        (json!({"video": "nope", "prop": "x"}), "does not exist"),
    ] {
        let err = call(&mut f, extra).unwrap_err();
        assert!(err.contains(expect), "{expect}: {err}");
    }
    assert!(f.ws.session.document().videos[0].animations.is_empty());
}
