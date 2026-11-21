#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Paths, gradients, stroke styles and the new animatable numbers, through typed operations.

use od_core::geometry::hit_test;
use od_core::lint::{LintOptions, lint};
use od_core::{
    Actor, Document, Node, NodeId, NodeKind, OpError, Operation, Paint, Property, Session,
    Workspace,
};
use serde_json::{Value, json};

struct Fixture {
    ws: Workspace,
    frame: NodeId,
}

impl Fixture {
    fn new() -> Fixture {
        let mut doc = Document::new("Vector");
        doc.add_page("Page 1").unwrap();
        let root = doc.pages[0].root.id.clone();
        let mut ws = Workspace::new(Session::new(doc));
        let ops: Vec<Operation> = serde_json::from_value(json!([{
            "op": "create", "parent": root.as_str(),
            "node": {"kind": "frame", "name": "F", "width": 400, "height": 400, "fill": "#ffffff"}
        }]))
        .unwrap();
        let frame = ws.session.apply(Actor::User, None, &ops).unwrap().created[0].clone();
        Fixture { ws, frame }
    }

    fn try_apply(&mut self, v: Value) -> Result<od_core::TxReceipt, OpError> {
        let ops: Vec<Operation> = serde_json::from_value(v).unwrap();
        self.ws.session.apply(Actor::User, None, &ops)
    }

    fn apply(&mut self, v: Value) -> od_core::TxReceipt {
        self.try_apply(v).unwrap()
    }

    fn create(&mut self, node: Value) -> NodeId {
        let frame = self.frame.clone();
        self.apply(json!([{"op": "create", "parent": frame.as_str(), "node": node}]))
            .created[0]
            .clone()
    }

    fn set(
        &mut self,
        id: &NodeId,
        name: &str,
        value: Value,
    ) -> Result<od_core::TxReceipt, OpError> {
        self.try_apply(
            json!([{"op": "set", "id": id.as_str(), "prop": {"name": name, "value": value}}]),
        )
    }

    fn node(&self, id: &NodeId) -> Node {
        self.ws.session.document().node(id).unwrap().clone()
    }

    fn doc(&self) -> Document {
        self.ws.session.document().clone()
    }
}

#[test]
fn nodes_without_the_new_fields_serialize_exactly_as_before() {
    let old = json!({
        "id": "n_2", "kind": "rect", "name": "Card", "x": 1.0, "y": 2.0, "width": 30.0, "height": 40.0,
        "fill": "{brand.gold}", "stroke": {"paint": "#000000", "width": 2.0}, "corner_radius": 8.0
    });
    let node: Node = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(serde_json::to_value(&node).unwrap(), old);
    let text = serde_json::to_string(&node).unwrap();
    for new_field in [
        "cap",
        "join",
        "dash",
        "trim_",
        "path",
        "view_box",
        "fill_rule",
    ] {
        assert!(!text.contains(new_field), "{new_field} leaked into {text}");
    }
}

#[test]
fn gradients_and_stroke_styles_round_trip_through_json() {
    let node = json!({
        "id": "n_2", "kind": "path", "name": "Wave", "x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0,
        "fill": {"type": "linear", "angle": 45.0, "stops": [
            {"offset": 0.0, "color": "#ff0000"}, {"offset": 1.0, "color": "{sky}"}]},
        "stroke": {"paint": {"type": "radial", "cx": 0.5, "cy": 0.5, "r": 0.5, "stops": [
            {"offset": 0.0, "color": "#ffffff"}, {"offset": 1.0, "color": "#000000cc"}]},
            "width": 3.0, "cap": "round", "join": "bevel", "dash": [4.0, 2.0],
            "trim_start": 0.25, "trim_end": 0.75},
        "path": "M0 0 L10 10", "view_box": {"x": 0.0, "y": 0.0, "width": 10.0, "height": 10.0},
        "fill_rule": "evenodd"
    });
    let parsed: Node = serde_json::from_value(node.clone()).unwrap();
    assert_eq!(serde_json::to_value(&parsed).unwrap(), node);
}

#[test]
fn gradient_defaults_and_malformed_gradients() {
    let p: Paint = serde_json::from_value(json!({"type": "linear", "stops": [
        {"offset": 0, "color": "#000000"}, {"offset": 1, "color": "#ffffff"}]}))
    .unwrap();
    assert!(matches!(p, Paint::Linear { angle, .. } if angle == 90.0));
    let r: Paint = serde_json::from_value(json!({"type": "radial", "stops": [
        {"offset": 0, "color": "#000000"}, {"offset": 1, "color": "#ffffff"}]}))
    .unwrap();
    assert!(matches!(r, Paint::Radial { cx, cy, r, .. } if (cx, cy, r) == (0.5, 0.5, 0.5)));
    for bad in [
        json!({"type": "conic", "stops": []}),
        json!({"type": "linear", "angle": 0, "stops": [{"offset": 0, "color": "red"}]}),
        json!({"type": "linear", "stops": [{"offset": 0, "color": {"type": "linear", "stops": []}}]}),
        json!({"type": "linear", "stops": [], "extra": 1}),
        json!(12),
    ] {
        assert!(
            serde_json::from_value::<Paint>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

fn gradient(stops: Value) -> Value {
    json!({"type": "linear", "angle": 0, "stops": stops})
}

#[test]
fn a_gradient_fill_is_validated_when_set_and_undone_as_a_whole() {
    let mut f = Fixture::new();
    let rect = f.create(json!({"kind": "rect", "width": 50, "height": 50, "fill": "#123456"}));
    let two = json!([{"offset": 0, "color": "#ff0000"}, {"offset": 1, "color": "#0000ff"}]);
    f.set(&rect, "fill", gradient(two)).unwrap();
    assert!(matches!(f.node(&rect).fill, Some(Paint::Linear { .. })));
    let nine: Vec<Value> = (0..9)
        .map(|i| json!({"offset": f64::from(i) / 8.0, "color": "#000000"}))
        .collect();
    let cases = [
        gradient(json!([{"offset": 0, "color": "#ff0000"}])),
        gradient(Value::Array(nine)),
        gradient(json!([{"offset": -0.5, "color": "#ff0000"}, {"offset": 1, "color": "#0000ff"}])),
        gradient(json!([{"offset": 0, "color": "{missing}"}, {"offset": 1, "color": "#0000ff"}])),
        json!({"type": "radial", "r": 0.0, "stops": [{"offset": 0, "color": "#000000"}, {"offset": 1, "color": "#ffffff"}]}),
        json!({"type": "linear", "angle": 1e9, "stops": [{"offset": 0, "color": "#000000"}, {"offset": 1, "color": "#ffffff"}]}),
    ];
    for bad in cases {
        assert!(f.set(&rect, "fill", bad.clone()).is_err(), "{bad}");
    }
    f.ws.session.undo(Actor::User).unwrap();
    assert_eq!(f.node(&rect).fill.unwrap().to_string(), "#123456");
    f.doc().validate().unwrap();
}

#[test]
fn a_token_in_a_gradient_stop_keeps_the_token_from_being_removed() {
    let mut f = Fixture::new();
    f.apply(json!([{"op": "set_token", "name": "sky", "value": "#88ccff"}]));
    let rect = f.create(
        json!({"kind": "rect", "width": 50, "height": 50, "fill": gradient(json!([
        {"offset": 0, "color": "{sky}"}, {"offset": 1, "color": "#ffffff"}]))}),
    );
    let err = f
        .try_apply(json!([{"op": "set_token", "name": "sky", "value": null}]))
        .unwrap_err();
    assert!(matches!(err, OpError::TokenInUse { .. }), "{err}");
    f.apply(json!([{"op": "set_token", "name": "sky", "value": "#112233"}]));
    let touched = f.doc().token_users("sky");
    assert_eq!(touched, vec![rect]);
}

#[test]
fn a_path_node_takes_its_box_from_the_path_and_refuses_bad_data() {
    let mut f = Fixture::new();
    let id = f.create(json!({"kind": "path", "name": "Blob", "x": 10, "y": 20,
        "path": "M5 5 L45 5 L45 25 Z", "fill": "#ff0000"}));
    let n = f.node(&id);
    assert_eq!((n.width, n.height), (40.0, 20.0));
    assert_eq!(n.path.as_deref(), Some("M5 5 L45 5 L45 25 Z"));

    let frame = f.frame.clone();
    for bad in [
        json!({"kind": "path"}),
        json!({"kind": "path", "path": "L1 1"}),
        json!({"kind": "path", "path": ""}),
        json!({"kind": "rect", "path": "M0 0 L1 1"}),
        json!({"kind": "rect", "fill_rule": "evenodd"}),
        json!({"kind": "rect", "view_box": {"x": 0, "y": 0, "width": 1, "height": 1}}),
    ] {
        let err = f.try_apply(json!([{"op": "create", "parent": frame.as_str(), "node": bad}]));
        assert!(err.is_err(), "{bad}");
    }
    assert!(f.set(&id, "path", json!("M0 0 L")).is_err());
    assert_eq!(
        f.node(&id).path.as_deref(),
        Some("M5 5 L45 5 L45 25 Z"),
        "a refused set changes nothing"
    );
    let long = format!("M0 0{}", " l1 1".repeat(5000));
    assert!(matches!(
        f.set(&id, "path", json!(long)),
        Err(OpError::OutOfBounds { .. })
    ));
    f.set(&id, "path", json!("M0 0 H10 V10 Z")).unwrap();
    f.ws.session.undo(Actor::User).unwrap();
    assert_eq!(f.node(&id).path.as_deref(), Some("M5 5 L45 5 L45 25 Z"));
    let rect = f.create(json!({"kind": "rect", "width": 5, "height": 5}));
    assert!(matches!(
        f.set(&rect, "path", json!("M0 0 L1 1")),
        Err(OpError::NotApplicable { .. })
    ));
    f.doc().validate().unwrap();
}

#[test]
fn a_view_box_is_set_cleared_and_bounded() {
    let mut f = Fixture::new();
    let id = f.create(json!({"kind": "path", "path": "M10 10 L20 20", "width": 50, "height": 50}));
    f.set(
        &id,
        "view_box",
        json!({"x": 0, "y": 0, "width": 100, "height": 100}),
    )
    .unwrap();
    assert_eq!(f.node(&id).view_box.unwrap().width, 100.0);
    assert!(
        f.set(
            &id,
            "view_box",
            json!({"x": 0, "y": 0, "width": 0, "height": 10})
        )
        .is_err()
    );
    assert!(
        f.set(
            &id,
            "view_box",
            json!({"x": 1e9, "y": 0, "width": 1, "height": 1})
        )
        .is_err()
    );
    f.set(&id, "view_box", Value::Null).unwrap();
    assert!(f.node(&id).view_box.is_none());
}

#[test]
fn stroke_styles_are_bounded_and_trims_need_a_stroke() {
    let mut f = Fixture::new();
    let id = f.create(json!({"kind": "path", "path": "M0 0 L100 0", "width": 100, "height": 0}));
    assert!(f.set(&id, "trim_end", json!(0.5)).is_err(), "no stroke yet");
    let stroke = |extra: Value| {
        let mut s = json!({"paint": "#000000", "width": 4});
        s.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        s
    };
    f.set(
        &id,
        "stroke",
        stroke(json!({"cap": "round", "join": "round", "dash": [6, 3]})),
    )
    .unwrap();
    for bad in [
        json!({"dash": [-1, 2]}),
        json!({"dash": [0, 0]}),
        json!({"dash": vec![1; 17]}),
        json!({"trim_end": 1.5}),
        json!({"trim_start": -0.1}),
        json!({"dash": [1e9]}),
    ] {
        assert!(f.set(&id, "stroke", stroke(bad.clone())).is_err(), "{bad}");
    }
    f.set(&id, "trim_end", json!(0.4)).unwrap();
    f.set(&id, "trim_start", json!(0.1)).unwrap();
    f.set(&id, "stroke_width", json!(9)).unwrap();
    let s = f.node(&id).stroke.unwrap();
    assert_eq!((s.trim_start, s.trim_end, s.width), (0.1, 0.4, 9.0));
    assert_eq!(s.dash, vec![6.0, 3.0]);
    assert!(f.set(&id, "trim_end", json!(2)).is_err());
    assert!(f.set(&id, "stroke_width", json!(-1)).is_err());
    f.ws.session.undo(Actor::User).unwrap();
    assert_eq!(f.node(&id).stroke.unwrap().width, 4.0);
}

fn animate(
    f: &mut Fixture,
    video: &NodeId,
    node: &NodeId,
    prop: &str,
    keys: Value,
) -> Result<od_core::TxReceipt, OpError> {
    f.try_apply(json!([{"op": "animate", "video": video.as_str(), "node": node.as_str(), "prop": prop, "keys": keys}]))
}

fn video(f: &mut Fixture) -> NodeId {
    f.apply(json!([{"op": "create_video", "name": "V", "fps": 30, "width": 400, "height": 400}]))
        .created[0]
        .clone()
}

#[test]
fn trim_and_stroke_width_animate_and_fill_color_mixes_in_srgb() {
    let mut f = Fixture::new();
    let id = f.create(
        json!({"kind": "path", "path": "M0 0 L100 0", "width": 100, "height": 0,
        "fill": "#ff0000", "stroke": {"paint": "#000000", "width": 2}}),
    );
    let v = video(&mut f);
    animate(
        &mut f,
        &v,
        &id,
        "trim_end",
        json!([{"at_ms": 0, "value": 0}, {"at_ms": 1000, "value": 1}]),
    )
    .unwrap();
    animate(
        &mut f,
        &v,
        &id,
        "stroke_width",
        json!([{"at_ms": 0, "value": 2}, {"at_ms": 1000, "value": 10}]),
    )
    .unwrap();
    animate(
        &mut f,
        &v,
        &id,
        "fill_color",
        json!([{"at_ms": 0, "value": "#ff0000"}, {"at_ms": 1000, "value": "#0000ff"}]),
    )
    .unwrap();
    let doc = f.doc();
    let video = doc.videos.iter().find(|x| x.id == v).unwrap();
    let at = |ms: f64| doc.at_time(video, ms).unwrap().node(&id).unwrap().clone();
    assert_eq!(at(0.0).stroke.unwrap().trim_end, 0.0);
    let mid = at(500.0);
    assert!((mid.stroke.as_ref().unwrap().trim_end - 0.5).abs() < 1e-9);
    assert!((mid.stroke.as_ref().unwrap().width - 6.0).abs() < 1e-9);
    assert_eq!(mid.fill.unwrap().to_string(), "#800080");
    assert_eq!(at(1000.0).stroke.unwrap().trim_end, 1.0);
    assert_eq!(at(2000.0).fill.unwrap().to_string(), "#0000ff");
}

#[test]
fn animations_need_what_they_move() {
    let mut f = Fixture::new();
    let line = f.create(json!({"kind": "path", "path": "M0 0 L100 0", "width": 100, "height": 0, "fill": "#ff0000"}));
    let text = f.create(json!({"kind": "text", "text": "hi", "width": 50, "height": 20}));
    let v = video(&mut f);
    let keys = json!([{"at_ms": 0, "value": 0}, {"at_ms": 500, "value": 1}]);
    assert!(
        animate(&mut f, &v, &line, "trim_end", keys.clone()).is_err(),
        "no stroke"
    );
    assert!(
        animate(&mut f, &v, &text, "trim_end", keys).is_err(),
        "text has no outline"
    );
    let colors = json!([{"at_ms": 0, "value": "#000000"}, {"at_ms": 500, "value": "not a color"}]);
    let op = json!([{"op": "animate", "video": v.as_str(), "node": line.as_str(), "prop": "fill_color", "keys": colors}]);
    assert!(
        serde_json::from_value::<Vec<Operation>>(op).is_err(),
        "a key colour must be a colour"
    );
}

#[test]
fn a_filled_path_is_hit_where_it_is_filled_and_a_line_where_it_is_stroked() {
    let mut f = Fixture::new();
    let ring = f.create(json!({"kind": "path", "x": 100, "y": 100, "width": 100, "height": 100,
        "path": "M0 0 H100 V100 H0 Z M30 30 H70 V70 H30 Z", "fill": "#ff0000", "fill_rule": "evenodd"}));
    let line = f.create(
        json!({"kind": "path", "x": 0, "y": 300, "width": 100, "height": 0,
        "path": "M0 0 H100", "stroke": {"paint": "#000000", "width": 10}}),
    );
    let doc = f.doc();
    let hit = |x, y| hit_test(&doc, &f.frame, x, y);
    assert_eq!(hit(110.0, 110.0), Some(ring.clone()));
    assert_eq!(
        hit(150.0, 150.0),
        Some(f.frame.clone()),
        "the hole is not the ring"
    );
    assert_eq!(hit(50.0, 302.0), Some(line.clone()));
    assert_eq!(hit(50.0, 330.0), Some(f.frame.clone()));
    let nonzero = {
        f.set(&ring, "fill_rule", json!("nonzero")).unwrap();
        f.doc()
    };
    assert_eq!(hit_test(&nonzero, &f.frame, 150.0, 150.0), Some(ring));
}

fn finding_codes(doc: &Document) -> Vec<&'static str> {
    lint(doc, &LintOptions::default())
        .iter()
        .map(|f| f.code)
        .collect()
}

#[test]
fn lint_flags_invisible_paths_unsorted_stops_and_trims_that_hide_the_stroke() {
    let mut f = Fixture::new();
    assert!(finding_codes(&f.doc()).is_empty());
    f.create(json!({"kind": "path", "path": "M0 0 L9 9", "width": 9, "height": 9}));
    assert!(finding_codes(&f.doc()).contains(&"OD070"));

    let mut f = Fixture::new();
    f.create(
        json!({"kind": "rect", "width": 9, "height": 9, "fill": gradient(json!([
        {"offset": 0.8, "color": "#000000"}, {"offset": 0.2, "color": "#ffffff"}]))}),
    );
    assert!(finding_codes(&f.doc()).contains(&"OD071"));

    let mut f = Fixture::new();
    let id = f.create(
        json!({"kind": "path", "path": "M0 0 L90 0", "width": 90, "height": 0,
        "stroke": {"paint": "#000000", "width": 3, "trim_start": 0.5, "trim_end": 0.5}}),
    );
    assert!(finding_codes(&f.doc()).contains(&"OD072"));
    let v = video(&mut f);
    animate(
        &mut f,
        &v,
        &id,
        "trim_end",
        json!([{"at_ms": 0, "value": 0.5}, {"at_ms": 500, "value": 1}]),
    )
    .unwrap();
    assert!(
        !finding_codes(&f.doc()).contains(&"OD072"),
        "an animated trim is the point"
    );
}

#[test]
fn a_path_node_round_trips_through_a_copy_of_the_node() {
    let mut f = Fixture::new();
    let id = f.create(json!({"kind": "path", "path": "M0 0 C10 20 30 20 40 0", "width": 40, "height": 15,
        "fill": "#112233", "fill_rule": "evenodd", "view_box": {"x": 0, "y": 0, "width": 40, "height": 15},
        "stroke": {"paint": "#ffffff", "width": 2, "dash": [3, 1], "trim_end": 0.5}}));
    let copy = f.node(&id);
    let again: Node = serde_json::from_value(serde_json::to_value(&copy).unwrap()).unwrap();
    assert_eq!(again, copy);
    assert_eq!(copy.kind, NodeKind::Path);
    let prop = Property::Path("M0 0 L1 1".into());
    assert_eq!(prop.name(), "path");
}
