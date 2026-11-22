#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::{AnimProp, Key, Repeat};
use od_core::{Actor, Ease, Node, NodeId, Operation, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::arc::{ArcSpec, Bulge, bend, curve, sample_times, tracks};
use super::keyframes::{Entry, parse_percent, percent_keys};
use super::text::{Align, Letter, Line, lay};
use crate::scene_motion::Track;
use crate::tools::{CallCtx, Content, Turns};

struct Scene {
    _dir: TempDir,
    ws: Workspace,
    frame: NodeId,
    video: NodeId,
}

fn scene() -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Paths").unwrap();
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
                    "kind": "frame", "name": "Scene", "width": 400, "height": 300, "fill": "#ffffff"
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
    Scene {
        _dir: dir,
        ws,
        frame,
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

impl Scene {
    fn add(&mut self, spec: Value) -> NodeId {
        self.ws
            .session
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: self.frame.clone(),
                    index: None,
                    node: serde_json::from_value(spec).unwrap(),
                }],
            )
            .unwrap()
            .created[0]
            .clone()
    }

    fn rect(&mut self, x: f64, y: f64) -> NodeId {
        self.add(
            json!({"kind": "rect", "x": x, "y": y, "width": 40, "height": 40, "fill": "#ff0000"}),
        )
    }

    fn path(&mut self, d: &str, x: f64, y: f64) -> NodeId {
        self.add(json!({"kind": "path", "path": d, "x": x, "y": y, "fill": "#ff0000"}))
    }

    fn doc(&self) -> &od_core::Document {
        self.ws.session.document()
    }

    fn node(&self, id: &NodeId) -> &Node {
        self.doc().node(id).unwrap()
    }

    fn keys(&self, id: &NodeId, prop: AnimProp) -> Vec<Key> {
        self.doc()
            .video(&self.video)
            .unwrap()
            .animations
            .iter()
            .find(|a| a.node == *id && a.prop == prop)
            .map(|a| a.keys.clone())
            .unwrap_or_default()
    }

    fn value(&self, id: &NodeId, prop: AnimProp, ms: f64) -> f64 {
        self.doc()
            .video(&self.video)
            .unwrap()
            .animations
            .iter()
            .find(|a| a.node == *id && a.prop == prop)
            .unwrap()
            .value_at(ms)
            .unwrap()
    }
}

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

fn up_arc() -> ArcSpec {
    ArcSpec::default()
}

#[test]
fn the_control_point_sits_off_the_line_by_strength_times_distance() {
    let c = curve((0.0, 0.0), (100.0, 0.0), &up_arc()).unwrap();
    assert_eq!(c.control, (50.0, -50.0));
    assert_eq!(c.at(0.0), (0.0, 0.0));
    assert_eq!(c.at(1.0), (100.0, 0.0));
    let mid = c.at(0.5);
    assert!(near(mid.0, 50.0, 1e-9) && near(mid.1, -25.0, 1e-9));
    assert!(near(c.heading(0.0), -45.0, 1e-9));
    assert!(near(c.heading(1.0), 45.0, 1e-9));
}

#[test]
fn the_bulge_stays_on_one_side_whichever_way_the_node_goes() {
    let there = curve((0.0, 0.0), (100.0, 0.0), &up_arc()).unwrap();
    let back = curve((100.0, 0.0), (0.0, 0.0), &up_arc()).unwrap();
    assert!(
        there.control.1 < 0.0 && back.control.1 < 0.0,
        "up both ways"
    );
    let down = curve((0.0, 0.0), (0.0, 100.0), &up_arc()).unwrap();
    let up = curve((0.0, 100.0), (0.0, 0.0), &up_arc()).unwrap();
    assert!(
        down.control.0 > 0.0 && up.control.0 > 0.0,
        "right both ways"
    );
    let cw = ArcSpec {
        direction: Bulge::Cw,
        ..up_arc()
    };
    let ccw = ArcSpec {
        direction: Bulge::Ccw,
        ..up_arc()
    };
    assert_eq!(
        curve((0.0, 0.0), (100.0, 0.0), &cw).unwrap().control,
        (50.0, -50.0)
    );
    assert_eq!(
        curve((0.0, 0.0), (100.0, 0.0), &ccw).unwrap().control,
        (50.0, 50.0)
    );
}

#[test]
fn peak_moves_the_bulge_along_the_run_and_zero_strength_is_straight() {
    let late = ArcSpec {
        peak: 0.8,
        ..up_arc()
    };
    assert_eq!(
        curve((0.0, 0.0), (100.0, 0.0), &late).unwrap().control,
        (80.0, -50.0)
    );
    let flat = ArcSpec {
        strength: 0.0,
        ..up_arc()
    };
    let c = curve((0.0, 0.0), (100.0, 40.0), &flat).unwrap();
    let mid = c.at(0.5);
    assert!(near(mid.0, 50.0, 1e-9) && near(mid.1, 20.0, 1e-9));
}

#[test]
fn an_arc_with_nowhere_to_go_or_out_of_range_numbers_is_refused() {
    assert!(curve((5.0, 5.0), (5.0, 5.0), &up_arc()).is_err());
    let strong = ArcSpec {
        strength: 1.5,
        ..up_arc()
    };
    assert!(curve((0.0, 0.0), (10.0, 0.0), &strong).is_err());
    let off = ArcSpec {
        peak: -0.1,
        ..up_arc()
    };
    assert!(curve((0.0, 0.0), (10.0, 0.0), &off).is_err());
}

#[test]
fn the_heading_is_unwrapped_so_it_never_jumps_by_a_turn() {
    let c = curve((100.0, 0.0), (0.0, 0.0), &up_arc()).unwrap();
    let samples = sample_times(0, 1000, 30);
    let progress: Vec<f64> = samples.iter().map(|(_, f)| *f).collect();
    let t = tracks(&c, &samples, &progress, Some(10.0));
    let rotation = t.iter().find(|t| t.prop == AnimProp::Rotation).unwrap();
    let values: Vec<f64> = rotation.keys.iter().map(|k| k.value).collect();
    assert!(near(values[0], 10.0 - 135.0, 1e-6));
    assert!(near(*values.last().unwrap(), 10.0 - 225.0, 1e-6));
    assert!(
        values.windows(2).all(|w| (w[1] - w[0]).abs() < 30.0),
        "no jump: {values:?}"
    );
}

#[test]
fn samples_are_about_33_ms_apart_on_the_frame_grid() {
    let s = sample_times(100, 1000, 30);
    assert_eq!(s.first().unwrap().0, 100);
    assert_eq!(s.last().unwrap().0, 1100);
    assert!(s.windows(2).all(|w| w[1].0 > w[0].0));
    assert!(s.windows(2).all(|w| w[1].0 - w[0].0 <= 67));
    assert!(s.len() >= 30 && s.len() <= 32);
    assert!(
        s.iter()
            .all(|(at, _)| od_core::timebase::snap_within(*at, 30, 0, 600_000) == Some(*at)),
        "every sample is on a frame"
    );
    assert_eq!(s.last().unwrap().1, 1.0);
}

fn line_track(prop: AnimProp, a: f64, b: f64, ease: Ease) -> Track {
    Track {
        prop,
        keys: vec![
            Key {
                at_ms: 0,
                value: a,
                ease: Ease::Linear,
            },
            Key {
                at_ms: 1000,
                value: b,
                ease,
            },
        ],
        repeat: Repeat::Once,
    }
}

#[test]
fn bending_a_move_keeps_its_ends_and_its_ease() {
    let moves = vec![
        line_track(AnimProp::X, 0.0, 100.0, Ease::EaseInOut),
        line_track(AnimProp::Y, 0.0, 0.0, Ease::EaseInOut),
        line_track(AnimProp::Opacity, 0.0, 1.0, Ease::Linear),
    ];
    // y does not travel, so the run is along x only
    let bent = bend(moves, &up_arc(), 30, (0.0, 0.0)).unwrap();
    assert_eq!(bent.len(), 3);
    let x = &bent[0].keys;
    let y = &bent[1].keys;
    assert_eq!((x[0].value, y[0].value), (0.0, 0.0));
    assert!(near(x.last().unwrap().value, 100.0, 1e-9) && near(y.last().unwrap().value, 0.0, 1e-9));
    assert!(y.iter().any(|k| k.value < -10.0), "it bulges up");
    // at half time the eased progress is 0.5, so the point is the middle of the curve
    let mid = x.iter().position(|k| k.at_ms >= 500).unwrap();
    assert!(near(x[mid].value, 50.0, 2.0), "{}", x[mid].value);
    assert!(near(y[mid].value, -25.0, 1.0), "{}", y[mid].value);
    assert_eq!(bent[2].keys.len(), 2, "other tracks are left alone");
}

#[test]
fn a_move_that_does_not_travel_cannot_be_bent() {
    let fade = vec![line_track(AnimProp::Opacity, 0.0, 1.0, Ease::Linear)];
    let err = bend(fade, &up_arc(), 30, (0.0, 0.0)).unwrap_err();
    assert!(err.0.contains("x or y"), "{}", err.0);
    let still = vec![
        line_track(AnimProp::X, 5.0, 5.0, Ease::Linear),
        line_track(AnimProp::Y, 7.0, 7.0, Ease::Linear),
    ];
    assert!(bend(still, &up_arc(), 30, (5.0, 7.0)).is_err());
}

#[test]
fn a_move_in_one_axis_is_bent_sideways_from_where_the_node_rests() {
    let slide = vec![line_track(AnimProp::X, 0.0, 100.0, Ease::Linear)];
    let bent = bend(slide, &up_arc(), 30, (0.0, 40.0)).unwrap();
    let y = bent.iter().find(|t| t.prop == AnimProp::Y).unwrap();
    assert_eq!(
        (y.keys[0].value, y.keys.last().unwrap().value),
        (40.0, 40.0)
    );
    let top = y.keys.iter().map(|k| k.value).fold(f64::MAX, f64::min);
    assert!(
        near(top, 15.0, 1.0),
        "the bulge is a quarter of the distance high: {top}"
    );
}

#[test]
fn animate_arc_writes_x_and_y_keys_that_follow_the_curve() {
    let mut s = scene();
    let node = s.rect(0.0, 100.0);
    let video = s.video.to_string();
    let out = call(
        &mut s.ws,
        "animate_arc",
        json!({
            "video": video, "node": node.to_string(),
            "from": [0, 100], "to": {"x": 100, "y": 100},
            "at_ms": 0, "duration_ms": 1000, "ease": "linear"
        }),
    )
    .unwrap();
    assert_eq!(out["control"], json!([50.0, 50.0]));
    let x = s.keys(&node, AnimProp::X);
    assert!(x.len() >= 30);
    assert!(near(s.value(&node, AnimProp::X, 500.0), 50.0, 0.5));
    assert!(near(s.value(&node, AnimProp::Y, 500.0), 75.0, 0.5));
    assert!(near(s.value(&node, AnimProp::X, 1000.0), 100.0, 1e-6));
    assert!(near(s.value(&node, AnimProp::Y, 0.0), 100.0, 1e-6));
    assert!(s.keys(&node, AnimProp::Rotation).is_empty());
}

#[test]
fn animate_arc_starts_where_the_node_is_and_keeps_earlier_keys() {
    let mut s = scene();
    let node = s.rect(20.0, 30.0);
    let video = s.video.to_string();
    let id = node.to_string();
    call(
        &mut s.ws,
        "animate_arc",
        json!({"video": video, "node": id, "to": [200, 30], "at_ms": 0, "duration_ms": 500}),
    )
    .unwrap();
    assert!(near(s.value(&node, AnimProp::X, 0.0), 20.0, 1e-6));
    call(
        &mut s.ws,
        "animate_arc",
        json!({"video": video, "node": id, "to": [20, 30], "at_ms": 1000, "duration_ms": 500, "arc": {"direction": "cw"}}),
    )
    .unwrap();
    assert!(
        near(s.value(&node, AnimProp::X, 500.0), 200.0, 1e-6),
        "first move kept"
    );
    assert!(
        near(s.value(&node, AnimProp::X, 1500.0), 20.0, 1e-6),
        "second move starts from where the first left it"
    );
    assert!(
        s.keys(&node, AnimProp::Y)
            .iter()
            .any(|k| k.at_ms >= 1000 && k.value > 30.0)
    );
}

#[test]
fn animate_arc_can_turn_the_node_along_the_curve_and_undoes_in_one_step() {
    let mut s = scene();
    let node = s.rect(0.0, 100.0);
    let video = s.video.to_string();
    call(
        &mut s.ws,
        "animate_arc",
        json!({"video": video, "node": node.to_string(), "to": [200, 100], "at_ms": 0, "duration_ms": 1000, "orient": true, "ease": "linear"}),
    )
    .unwrap();
    let r = s.keys(&node, AnimProp::Rotation);
    assert!(near(r.first().unwrap().value, -45.0, 1.0), "{r:?}");
    assert!(near(r.last().unwrap().value, 45.0, 1.0));
    s.ws.session.undo(Actor::agent("tester").unwrap()).unwrap();
    assert!(s.doc().video(&s.video).unwrap().animations.is_empty());
}

#[test]
fn animate_arc_refuses_bad_requests() {
    let mut s = scene();
    let node = s.rect(0.0, 0.0);
    let video = s.video.to_string();
    let id = node.to_string();
    let frame = s.frame.to_string();
    assert!(
        call(
            &mut s.ws,
            "animate_arc",
            json!({"video": video, "node": id, "to": [0, 0], "at_ms": 0, "duration_ms": 500})
        )
        .is_err(),
        "same point"
    );
    assert!(
        call(
            &mut s.ws,
            "animate_arc",
            json!({"video": video, "node": id, "to": [9, 9], "at_ms": 0, "duration_ms": 10})
        )
        .is_err(),
        "too short"
    );
    assert!(
        call(
            &mut s.ws,
            "animate_arc",
            json!({"video": video, "node": frame, "to": [9, 9], "at_ms": 0, "duration_ms": 500})
        )
        .is_err(),
        "a frame"
    );
    assert!(call(&mut s.ws, "animate_arc", json!({"video": video, "node": id, "to": [9, 9], "at_ms": 0, "duration_ms": 500, "arc": {"strength": 3}})).is_err());
    assert!(call(&mut s.ws, "animate_arc", json!({"video": video, "node": id, "to": [9, 9], "at_ms": 0, "duration_ms": 500, "arc": {"bend": 1}})).is_err(), "unknown arc field");
}

#[test]
fn animate_preset_takes_arc_on_a_move_that_travels_and_refuses_it_elsewhere() {
    let mut s = scene();
    let node = s.rect(50.0, 50.0);
    let video = s.video.to_string();
    let slide = |arc: Value| {
        let mut args = json!({"video": video, "node": node.to_string(), "preset": "slide_in_left", "start_ms": 0, "duration_ms": 500, "ease": "linear"});
        if !arc.is_null() {
            args["arc"] = arc;
        }
        args
    };
    call(&mut s.ws, "animate_preset", slide(json!({"strength": 0.4}))).unwrap();
    let y = s.keys(&node, AnimProp::Y);
    assert!(y.len() > 10, "the slide gained a y track");
    assert!(
        y.iter().any(|k| k.value < 40.0),
        "it arcs up from its rest height"
    );
    assert!(
        near(y.first().unwrap().value, 50.0, 1e-6) && near(y.last().unwrap().value, 50.0, 1e-6)
    );
    let x = s.keys(&node, AnimProp::X);
    assert!(
        near(x.last().unwrap().value, 50.0, 1e-6),
        "it still lands at x"
    );
    let err = call(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "node": node.to_string(), "preset": "pop_in", "start_ms": 0, "duration_ms": 500, "arc": {}}),
    )
    .unwrap_err();
    assert!(err.contains("x or y"), "{err}");
    let err = call(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "preset": "camera_push_in", "start_ms": 0, "duration_ms": 500, "arc": {}}),
    )
    .unwrap_err();
    assert!(err.contains("camera"), "{err}");
    let path = s.path("M0 0 L100 0", 0.0, 0.0);
    let err = call(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "node": node.to_string(), "preset": "animate_along_path", "path": path.to_string(), "start_ms": 0, "duration_ms": 500, "arc": {}}),
    )
    .unwrap_err();
    assert!(err.contains("already follows"), "{err}");
    let mut plain = scene();
    let other = plain.rect(50.0, 50.0);
    let video = plain.video.to_string();
    call(
        &mut plain.ws,
        "animate_preset",
        json!({"video": video, "node": other.to_string(), "preset": "slide_in_left", "start_ms": 0, "duration_ms": 500}),
    )
    .unwrap();
    assert!(
        plain.keys(&other, AnimProp::Y).is_empty(),
        "without arc the slide stays straight"
    );
}

fn entry(percent: f64, value: f64, ease: Option<Ease>) -> Entry {
    Entry {
        percent,
        value,
        ease,
    }
}

fn k(at_ms: u64, value: f64, ease: Ease) -> Key {
    Key { at_ms, value, ease }
}

#[test]
fn percentages_read_like_css() {
    assert_eq!(parse_percent("40%").unwrap(), 40.0);
    assert_eq!(parse_percent(" 12.5% ").unwrap(), 12.5);
    assert_eq!(parse_percent("from").unwrap(), 0.0);
    assert_eq!(parse_percent("to").unwrap(), 100.0);
    assert!(parse_percent("40").is_err());
    assert!(parse_percent("-5%").is_err());
    assert!(parse_percent("abc%").is_err());
    assert!(parse_percent("NaN%").is_err());
}

#[test]
fn a_keyframes_ease_shapes_the_move_that_leaves_it() {
    let keys = percent_keys(
        0,
        1000,
        &[
            entry(0.0, 0.0, Some(Ease::EaseOut)),
            entry(50.0, 100.0, Some(Ease::EaseIn)),
            entry(100.0, 200.0, None),
        ],
        0.0,
        Ease::EaseInOut,
    );
    assert_eq!(
        keys,
        vec![
            k(0, 0.0, Ease::Linear),
            k(500, 100.0, Ease::EaseOut),
            k(1000, 200.0, Ease::EaseIn)
        ]
    );
}

#[test]
fn a_first_keyframe_after_zero_starts_from_the_current_value() {
    let keys = percent_keys(
        0,
        1000,
        &[entry(100.0, 80.0, None), entry(25.0, 50.0, None)],
        10.0,
        Ease::EaseInOut,
    );
    assert_eq!(
        keys,
        vec![
            k(0, 10.0, Ease::Linear),
            k(250, 50.0, Ease::EaseInOut),
            k(1000, 80.0, Ease::EaseInOut)
        ]
    );
    let late = percent_keys(
        500,
        1000,
        &[entry(100.0, 1.0, None), entry(0.0, 0.0, None)],
        9.0,
        Ease::Linear,
    );
    assert_eq!(late[0].at_ms, 500);
    assert_eq!(late[1].at_ms, 1500);
}

#[test]
fn animate_keyframes_writes_one_track_per_property() {
    let mut s = scene();
    let node = s.rect(100.0, 60.0);
    let video = s.video.to_string();
    let out = call(
        &mut s.ws,
        "animate_keyframes",
        json!({
            "video": video, "node": node.to_string(), "duration_ms": 1000,
            "keyframes": {
                "0%": {"x": 0, "y": 0, "ease": "ease_out"},
                "40%": {"x": 200, "y": -80},
                "50%": {"x": 300},
                "100%": {"x": 400, "y": 0, "ease": "ease_in"}
            }
        }),
    )
    .unwrap();
    let x = s.keys(&node, AnimProp::X);
    assert_eq!(
        x.iter().map(|k| (k.at_ms, k.value)).collect::<Vec<_>>(),
        vec![(0, 0.0), (400, 200.0), (500, 300.0), (1000, 400.0)]
    );
    assert_eq!(x[1].ease, Ease::EaseOut, "the 0% ease leaves 0%");
    assert_eq!(x[2].ease, Ease::EaseInOut, "40% names none, so the default");
    let y = s.keys(&node, AnimProp::Y);
    assert_eq!(y.len(), 3, "y has no 50% key");
    assert_eq!((y[1].at_ms, y[1].value), (400, -80.0));
    assert!(out["note"].as_str().unwrap().contains("last"), "{out}");
}

#[test]
fn a_property_first_keyed_late_starts_from_where_the_node_is() {
    let mut s = scene();
    let node = s.rect(100.0, 60.0);
    let video = s.video.to_string();
    call(
        &mut s.ws,
        "animate_keyframes",
        json!({"video": video, "node": node.to_string(), "duration_ms": 1000, "at_ms": 300,
               "keyframes": {"50%": {"x": 300}, "100%": {"x": 500}}}),
    )
    .unwrap();
    let x = s.keys(&node, AnimProp::X);
    assert_eq!(
        x.iter().map(|k| (k.at_ms, k.value)).collect::<Vec<_>>(),
        vec![(300, 100.0), (800, 300.0), (1300, 500.0)]
    );
}

#[test]
fn keyframes_past_a_hundred_percent_need_allow_over() {
    let mut s = scene();
    let node = s.rect(0.0, 0.0);
    let video = s.video.to_string();
    let args = |over: bool| {
        json!({"video": video, "node": node.to_string(), "duration_ms": 1000, "allow_over": over,
               "keyframes": {"0%": {"x": 0}, "120%": {"x": 10}}})
    };
    assert!(
        call(&mut s.ws, "animate_keyframes", args(false))
            .unwrap_err()
            .contains("100%")
    );
    call(&mut s.ws, "animate_keyframes", args(true)).unwrap();
    assert_eq!(s.keys(&node, AnimProp::X).last().unwrap().at_ms, 1200);
}

#[test]
fn bad_keyframes_are_refused() {
    let mut s = scene();
    let node = s.rect(0.0, 0.0);
    let video = s.video.to_string();
    let id = node.to_string();
    let mut run = |kf: Value| {
        call(
            &mut s.ws,
            "animate_keyframes",
            json!({"video": video, "node": id, "duration_ms": 1000, "keyframes": kf}),
        )
        .unwrap_err()
    };
    assert!(run(json!({"0%": {"wiggle": 1}})).contains("not an animatable property"));
    assert!(run(json!({"0%": {"x": "far"}})).contains("number"));
    assert!(run(json!({"0%": {"x": 0}, "0.01%": {"x": 1}})).contains("same frame"));
    assert!(run(json!({"soon": {"x": 0}})).contains("percentage"));
    assert!(run(json!({})).contains("keyframes"));
    assert!(run(json!({"0%": {"ease": "ease_out"}})).contains("no property"));
    assert!(run(json!({"0%": {"x": 0, "ease": "wobbly"}})).len() > 3);
}

#[test]
fn camera_keyframes_need_no_node() {
    let mut s = scene();
    let video = s.video.to_string();
    call(
        &mut s.ws,
        "animate_keyframes",
        json!({"video": video, "duration_ms": 1000,
               "keyframes": {"0%": {"camera_zoom": 1.0}, "100%": {"camera_zoom": 1.5}}}),
    )
    .unwrap();
    let v = s.doc().video(&s.video).unwrap();
    assert!((v.camera_at(1000.0).zoom - 1.5).abs() < 1e-9);
    let err = call(
        &mut s.ws,
        "animate_keyframes",
        json!({"video": video, "duration_ms": 1000, "keyframes": {"0%": {"x": 1}}}),
    )
    .unwrap_err();
    assert!(err.contains("node"), "{err}");
}

#[test]
fn a_colour_keyframe_takes_a_hex_string() {
    let mut s = scene();
    let node = s.rect(0.0, 0.0);
    let video = s.video.to_string();
    call(
        &mut s.ws,
        "animate_keyframes",
        json!({"video": video, "node": node.to_string(), "duration_ms": 1000,
               "keyframes": {"0%": {"fill_color": "#ff0000"}, "100%": {"fill_color": "#0000ff"}}}),
    )
    .unwrap();
    let keys = s.keys(&node, AnimProp::FillColor);
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0].value, keys[1].value);
}

fn letter(text: &str, start: f64, advance: f64) -> Letter {
    Letter {
        text: text.into(),
        start,
        advance,
        blank: text.trim().is_empty(),
    }
}

fn straight(len: f64) -> Line {
    Line::new(vec![(0.0, 0.0), (len, 0.0)], false)
}

#[test]
fn letters_stand_at_the_middle_of_their_advance_along_the_line() {
    let letters = [
        letter("A", 0.0, 10.0),
        letter(" ", 10.0, 5.0),
        letter("B", 15.0, 10.0),
    ];
    let line = straight(100.0);
    let start = lay(&letters, &line, 0.0, Align::Start);
    assert_eq!(start.stands.len(), 2, "the space is not a node");
    assert!(near(start.stands[0].1.0, 5.0, 1e-9) && near(start.stands[1].1.0, 20.0, 1e-9));
    assert_eq!(start.stands[0].2, 0.0);
    let centre = lay(&letters, &line, 0.0, Align::Center);
    assert!(near(centre.first_s, 37.5, 1e-9) && near(centre.stands[0].1.0, 42.5, 1e-9));
    let end = lay(&letters, &line, 0.0, Align::End);
    assert!(near(end.stands[1].1.0, 95.0, 1e-9));
    let shifted = lay(&letters, &line, 10.0, Align::Start);
    assert!(near(shifted.stands[0].1.0, 15.0, 1e-9));
}

#[test]
fn letters_off_the_end_of_an_open_line_are_left_out_but_a_closed_line_wraps() {
    let letters = [
        letter("A", 0.0, 10.0),
        letter("B", 10.0, 10.0),
        letter("C", 20.0, 10.0),
    ];
    let open = lay(&letters, &straight(24.0), 0.0, Align::Start);
    assert_eq!((open.stands.len(), open.off_the_end), (2, 1));
    let before = lay(&letters, &straight(100.0), -12.0, Align::Start);
    assert_eq!(
        (before.stands.len(), before.off_the_end),
        (2, 1),
        "A's middle is at -7"
    );
    let ring = Line::new(
        vec![
            (0.0, 0.0),
            (20.0, 0.0),
            (20.0, 20.0),
            (0.0, 20.0),
            (0.0, 0.0),
        ],
        true,
    );
    let wrapped = lay(&letters, &ring, 0.0, Align::Start);
    assert_eq!((wrapped.stands.len(), wrapped.off_the_end), (3, 0));
}

#[test]
fn a_letter_turns_to_the_tangent_of_the_line() {
    let letters = [letter("A", 0.0, 10.0)];
    let down = Line::new(vec![(0.0, 0.0), (0.0, 100.0)], false);
    let l = lay(&letters, &down, 20.0, Align::Start);
    assert!(near(l.stands[0].2, 90.0, 1e-9));
    assert!(near(l.stands[0].1.1, 25.0, 1e-9));
    let diagonal = Line::new(vec![(0.0, 100.0), (100.0, 0.0)], false);
    let l = lay(&letters, &diagonal, 0.0, Align::Start);
    assert!(near(l.stands[0].2, -45.0, 1e-9));
}

impl Scene {
    fn text(&mut self, text: &str) -> NodeId {
        self.add(
            json!({"kind": "text", "name": "Title", "x": 20, "y": 30, "width": 300, "height": 60,
                        "text": text, "font_size": 24, "font_weight": 700, "fill": "#101010"}),
        )
    }

    fn ink(&self) -> (usize, usize, usize, usize, usize) {
        let data = od_render::render_frame(self.doc(), &self.frame, 1.0, None)
            .unwrap()
            .pixmap
            .data()
            .to_vec();
        let (mut x0, mut y0, mut x1, mut y1, mut count) = (usize::MAX, usize::MAX, 0, 0, 0);
        for (i, p) in data.chunks(4).enumerate() {
            if p[0] < 160 {
                let (x, y) = (i % 400, i / 400);
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                count += 1;
            }
        }
        (x0, y0, x1, y1, count)
    }
}

fn group_of(s: &Scene, report: &Value) -> Node {
    s.node(&NodeId::parse(report["group"].as_str().unwrap()).unwrap())
        .clone()
}

#[test]
fn text_on_a_path_makes_one_node_per_letter_standing_on_the_curve() {
    let mut s = scene();
    let text = s.text("Hello path");
    let path = s.path("M0 0 L200 0", 40.0, 200.0);
    let out = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": text.to_string(), "path_node": path.to_string()}),
    )
    .unwrap();
    let group = group_of(&s, &out);
    assert_eq!(group.children.len(), 9, "ten characters, one a space");
    assert_eq!(out["count"], 9);
    assert_eq!(out["off_the_end"], 0);
    assert_eq!(group.name, "Title");
    assert!(
        s.doc().node(&text).is_none(),
        "replace removed the original"
    );
    let xs: Vec<f64> = group.children.iter().map(|c| c.x).collect();
    assert!(
        xs.windows(2).all(|w| w[1] > w[0]),
        "reading order along the line: {xs:?}"
    );
    assert!(group.children.iter().all(|c| c.rotation == 0.0));
    let baseline = group.children[0].y + group.children[0].pivot_y * group.children[0].height;
    assert!(
        (baseline - 200.0).abs() < 0.5,
        "the baseline is on the path: {baseline}"
    );
    assert_eq!(group.children[0].text.as_deref(), Some("H"));
    s.ws.session.undo(Actor::agent("tester").unwrap()).unwrap();
    assert!(s.doc().node(&text).is_some(), "one undo puts the text back");
}

#[test]
fn text_on_a_rising_line_turns_every_letter_to_it() {
    let mut s = scene();
    let text = s.text("Rise");
    let path = s.path("M0 100 L100 0", 40.0, 120.0);
    let out = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": text.to_string(), "path_node": path.to_string(), "mode": "keep"}),
    )
    .unwrap();
    let group = group_of(&s, &out);
    assert!(
        group
            .children
            .iter()
            .all(|c| (c.rotation + 45.0).abs() < 0.5),
        "{:?}",
        group
            .children
            .iter()
            .map(|c| c.rotation)
            .collect::<Vec<_>>()
    );
    assert!(!s.node(&text).visible, "keep hides the original");
    assert_eq!(group.name, "Title on path");
}

#[test]
fn the_ink_of_text_on_a_slope_covers_more_height_than_text_on_a_flat_line() {
    let mut flat = scene();
    let t = flat.text("Sloping");
    let p = flat.path("M0 0 L200 0", 40.0, 150.0);
    call(
        &mut flat.ws,
        "text_on_path",
        json!({"text_node": t.to_string(), "path_node": p.to_string()}),
    )
    .unwrap();
    let (_, fy0, _, fy1, fcount) = flat.ink();
    let mut slope = scene();
    let t = slope.text("Sloping");
    let p = slope.path("M0 100 L200 0", 40.0, 100.0);
    call(
        &mut slope.ws,
        "text_on_path",
        json!({"text_node": t.to_string(), "path_node": p.to_string()}),
    )
    .unwrap();
    let (_, sy0, _, sy1, scount) = slope.ink();
    assert!(fcount > 100 && scount > 100, "ink was drawn");
    assert!(
        sy1 - sy0 > (fy1 - fy0) + 15,
        "flat {} sloped {}",
        fy1 - fy0,
        sy1 - sy0
    );
}

#[test]
fn text_that_does_not_fit_an_open_path_loses_its_end_and_says_so() {
    let mut s = scene();
    let text = s.text("A long line of text");
    let path = s.path("M0 0 L60 0", 40.0, 150.0);
    let out = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": text.to_string(), "path_node": path.to_string()}),
    )
    .unwrap();
    assert!(out["off_the_end"].as_u64().unwrap() > 0);
    assert!(out["note"].as_str().unwrap().contains("left out"));
    assert!(out["count"].as_u64().unwrap() < 15);
}

#[test]
fn text_on_a_closed_path_wraps_round() {
    let mut s = scene();
    let text = s.text("Round and round");
    let path = s.path("M0 0 H60 V60 H0 Z", 40.0, 100.0);
    let out = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": text.to_string(), "path_node": path.to_string(), "align": "center"}),
    )
    .unwrap();
    assert_eq!(out["off_the_end"], 0);
    assert_eq!(out["closed"], true);
}

#[test]
fn text_on_path_refuses_what_is_not_text_or_not_a_path() {
    let mut s = scene();
    let text = s.text("Hi");
    let path = s.path("M0 0 L100 0", 0.0, 0.0);
    let rect = s.rect(0.0, 0.0);
    let (t, p, r) = (text.to_string(), path.to_string(), rect.to_string());
    assert!(
        call(
            &mut s.ws,
            "text_on_path",
            json!({"text_node": r, "path_node": p})
        )
        .unwrap_err()
        .contains("text node")
    );
    assert!(
        call(
            &mut s.ws,
            "text_on_path",
            json!({"text_node": t, "path_node": r})
        )
        .unwrap_err()
        .contains("not a path")
    );
    let err = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": t, "path_node": p, "start_offset": 5000}),
    )
    .unwrap_err();
    assert!(err.contains("none of the text"), "{err}");
}

#[test]
fn a_reveal_staggers_a_scene_move_over_the_letters() {
    let mut s = scene();
    let text = s.text("Pop");
    let path = s.path("M0 0 L200 0", 40.0, 150.0);
    let video = s.video.to_string();
    let out = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": text.to_string(), "path_node": path.to_string(),
               "reveal": {"video": video, "each_ms": 100, "at_ms": 200}}),
    )
    .unwrap();
    let group = group_of(&s, &out);
    assert_eq!(group.children.len(), 3);
    let starts: Vec<u64> = group
        .children
        .iter()
        .map(|c| s.keys(&c.id, AnimProp::ScaleX).first().unwrap().at_ms)
        .collect();
    assert_eq!(starts, vec![200, 300, 400]);
    assert_eq!(out["reveal"]["preset"], "pop_in");
    let err = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": group.children[0].id.to_string(), "path_node": path.to_string(), "reveal": {"video": video, "preset": "camera_push_in"}}),
    )
    .unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn a_reveal_with_a_camera_move_is_refused_before_anything_changes() {
    let mut s = scene();
    let text = s.text("Pop");
    let path = s.path("M0 0 L200 0", 40.0, 150.0);
    let video = s.video.to_string();
    let before = s.doc().node_count();
    let err = call(
        &mut s.ws,
        "text_on_path",
        json!({"text_node": text.to_string(), "path_node": path.to_string(),
               "reveal": {"video": video, "preset": "camera_push_in"}}),
    )
    .unwrap_err();
    assert!(err.contains("scene move"), "{err}");
    assert_eq!(s.doc().node_count(), before);
    assert!(s.doc().node(&text).is_some());
}

fn morph_pair(s: &mut Scene) -> (NodeId, NodeId) {
    let a = s.path("M0 0 H40 V40 H0 Z", 10.0, 10.0);
    let b = s.path("M0 0 H20 V20 H0 Z", 70.0, 10.0);
    (a, b)
}

fn centroid(points: &[[f64; 2]]) -> (f64, f64) {
    let n = points.len() as f64;
    (
        points.iter().map(|p| p[0]).sum::<f64>() / n,
        points.iter().map(|p| p[1]).sum::<f64>() / n,
    )
}

#[test]
fn morph_to_resamples_both_shapes_and_keys_the_progress() {
    let mut s = scene();
    let (a, b) = morph_pair(&mut s);
    let video = s.video.to_string();
    let out = call(
        &mut s.ws,
        "morph_to",
        json!({"video": video, "node": a.to_string(), "to_path": b.to_string(), "at_ms": 0, "duration_ms": 1000}),
    )
    .unwrap();
    let node = s.node(&a);
    let morph = node.morph.as_ref().unwrap();
    assert_eq!(morph.stops.len(), 2);
    assert_eq!(morph.closed, vec![true]);
    let n = morph.stops[0][0].len();
    assert_eq!(n, morph.stops[1][0].len());
    assert_eq!(n, 53, "160 units of outline at 0.33 a unit");
    assert_eq!(out["points"], json!([53]));
    let from = centroid(&morph.stops[0][0]);
    let to = centroid(&morph.stops[1][0]);
    assert!(
        near(from.0, 20.0, 0.5) && near(from.1, 20.0, 0.5),
        "{from:?}"
    );
    assert!(
        near(to.0, 70.0, 0.5) && near(to.1, 10.0, 0.5),
        "the target keeps its place: {to:?}"
    );
    let keys = s.keys(&a, AnimProp::MorphProgress);
    assert_eq!(
        keys.iter().map(|k| (k.at_ms, k.value)).collect::<Vec<_>>(),
        vec![(0, 0.0), (1000, 1.0)]
    );
    assert_eq!(
        node.path.as_deref(),
        Some("M0 0 H40 V40 H0 Z"),
        "the node keeps its own path data"
    );
    let shown = s
        .doc()
        .at_time(s.doc().video(&s.video).unwrap(), 1000.0)
        .unwrap();
    let d = shown.node(&a).unwrap().path.clone().unwrap();
    assert!(d.starts_with('M') && d.ends_with('Z'), "{d}");
    s.ws.session.undo(Actor::agent("tester").unwrap()).unwrap();
    assert!(s.node(&a).morph.is_none(), "one undo takes the morph away");
    assert!(s.keys(&a, AnimProp::MorphProgress).is_empty());
}

#[test]
fn a_second_morph_continues_from_the_first_and_reset_starts_over() {
    let mut s = scene();
    let (a, b) = morph_pair(&mut s);
    let c = s.path("M0 0 H30 V10 H0 Z", 200.0, 100.0);
    let video = s.video.to_string();
    let go = |s: &mut Scene, to: &NodeId, at: u64, extra: Value| {
        let mut args = json!({"video": video, "node": a.to_string(), "to_path": to.to_string(), "at_ms": at, "duration_ms": 500});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        call(&mut s.ws, "morph_to", args)
    };
    go(&mut s, &b, 0, json!({})).unwrap();
    let early = go(&mut s, &c, 200, json!({})).unwrap_err();
    assert!(early.contains("runs until"), "{early}");
    go(&mut s, &c, 1000, json!({})).unwrap();
    let morph = s.node(&a).morph.clone().unwrap();
    assert_eq!(morph.stops.len(), 3);
    let keys = s.keys(&a, AnimProp::MorphProgress);
    assert_eq!(
        keys.iter().map(|k| (k.at_ms, k.value)).collect::<Vec<_>>(),
        vec![(0, 0.0), (500, 1.0), (1000, 1.0), (1500, 2.0)]
    );
    let last = centroid(&morph.stops[2][0]);
    assert!(
        near(last.0, 200.0 + 15.0 - 10.0, 1.0) && near(last.1, 100.0 + 5.0 - 10.0, 1.0),
        "{last:?}"
    );
    go(&mut s, &b, 0, json!({"reset": true})).unwrap();
    assert_eq!(s.node(&a).morph.as_ref().unwrap().stops.len(), 2);
    assert_eq!(s.keys(&a, AnimProp::MorphProgress).len(), 2);
}

#[test]
fn a_shape_with_a_hole_pairs_its_contours_and_an_unpaired_one_shrinks() {
    let mut s = scene();
    let ring = s.path("M0 0 H60 V60 H0 Z M20 20 V40 H40 V20 Z", 10.0, 10.0);
    let solid = s.path("M0 0 H30 V30 H0 Z", 100.0, 10.0);
    let video = s.video.to_string();
    let out = call(
        &mut s.ws,
        "morph_to",
        json!({"video": video, "node": ring.to_string(), "to_path": solid.to_string(), "at_ms": 0, "duration_ms": 500}),
    )
    .unwrap();
    assert_eq!(out["contours"], 2);
    let morph = s.node(&ring).morph.clone().unwrap();
    let hole_end = centroid(&morph.stops[1][1]);
    assert!(
        morph.stops[1][1]
            .iter()
            .all(|p| near(p[0], hole_end.0, 1e-3) && near(p[1], hole_end.1, 1e-3)),
        "the hole shrinks to a point"
    );
    assert!(
        near(hole_end.0, 30.0, 0.5) && near(hole_end.1, 30.0, 0.5),
        "{hole_end:?}"
    );
}

#[test]
fn morph_to_refuses_what_it_cannot_morph() {
    let mut s = scene();
    let (a, b) = morph_pair(&mut s);
    let rect = s.rect(0.0, 0.0);
    let line = s.path("M0 0 H100", 0.0, 0.0);
    let video = s.video.to_string();
    let run = |s: &mut Scene, node: &NodeId, to: &NodeId, ms: u64| {
        call(
            &mut s.ws,
            "morph_to",
            json!({"video": video, "node": node.to_string(), "to_path": to.to_string(), "at_ms": 0, "duration_ms": ms}),
        )
        .unwrap_err()
    };
    assert!(run(&mut s, &rect, &b, 500).contains("path node"));
    assert!(run(&mut s, &a, &rect, 500).contains("path node"));
    assert!(run(&mut s, &a, &a, 500).contains("itself"));
    assert!(run(&mut s, &a, &b, 10).contains("duration_ms"));
    assert!(run(&mut s, &line, &b, 500).contains("no size"));
    assert!(
        s.node(&a).morph.is_none(),
        "a refused morph changes nothing"
    );
}
