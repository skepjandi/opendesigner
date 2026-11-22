#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::AnimProp;
use od_core::{Actor, NodeId, Operation, Paint, Property, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::{MAX_SNAPSHOT_NODES, MAX_SNAPSHOTS, SNAPSHOT_DIR};
use crate::tools::{CallCtx, Content, Turns};

struct Fixture {
    _dir: TempDir,
    ws: Workspace,
    frame: NodeId,
    a: NodeId,
    c: NodeId,
    r: NodeId,
    g: NodeId,
    d: NodeId,
    video: NodeId,
}

fn create(ws: &mut Workspace, parent: &NodeId, spec: Value) -> NodeId {
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: parent.clone(),
                index: None,
                node: serde_json::from_value(spec).unwrap(),
            }],
        )
        .unwrap()
        .created[0]
        .clone()
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Layout").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let frame = create(
        &mut ws,
        &root,
        json!({"kind": "frame", "name": "Slide", "width": 400, "height": 300, "fill": "#ffffff"}),
    );
    let rect = |name: &str, x: f64, y: f64| {
        json!({"kind": "rect", "name": name, "x": x, "y": y, "width": 100, "height": 50,
               "fill": "#112233"})
    };
    let a = create(&mut ws, &frame, rect("A", 0.0, 0.0));
    let c = create(&mut ws, &frame, rect("C", 0.0, 200.0));
    let r = create(&mut ws, &frame, rect("R", 300.0, 0.0));
    let g = create(
        &mut ws,
        &frame,
        json!({"kind": "group", "name": "G1", "x": 400, "y": 0}),
    );
    let d = create(
        &mut ws,
        &g,
        json!({"kind": "rect", "name": "D", "x": 100, "y": 100, "width": 50, "height": 50,
               "fill": "#445566"}),
    );
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
        frame,
        a,
        c,
        r,
        g,
        d,
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

impl Fixture {
    fn edit(&mut self, ops: &[Operation]) {
        self.ws.session.apply(Actor::User, None, ops).unwrap();
    }

    fn set(&mut self, id: &NodeId, prop: Property) {
        self.edit(&[Operation::Set {
            id: id.clone(),
            prop,
        }]);
    }

    fn snapshot(&mut self) -> String {
        let out = call(
            &mut self.ws,
            "snapshot_layout",
            json!({"frame": self.frame}),
        )
        .unwrap();
        out["snapshot"].as_str().unwrap().to_string()
    }

    fn transition(&mut self, snap: &str, extra: Value) -> Result<Value, String> {
        let mut args = json!({"video": self.video, "snapshot": snap, "at_ms": 300,
                              "duration_ms": 300, "ease": "linear"});
        args.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        call(&mut self.ws, "transition_layout", args)
    }

    fn keys(&self, node: &NodeId, prop: AnimProp) -> Vec<(u64, f64)> {
        self.ws.session.document().videos[0]
            .animations
            .iter()
            .find(|t| &t.node == node && t.prop == prop)
            .map(|t| t.keys.iter().map(|k| (k.at_ms, k.value)).collect())
            .unwrap_or_default()
    }

    fn track_count(&self) -> usize {
        self.ws.session.document().videos[0].animations.len()
    }

    fn snapshot_dir(&self) -> std::path::PathBuf {
        self.ws.dir().unwrap().join(SNAPSHOT_DIR)
    }
}

#[test]
fn a_snapshot_is_a_plain_json_file_in_the_project_with_a_default_or_given_name() {
    let mut f = fixture();
    let out = call(&mut f.ws, "snapshot_layout", json!({"frame": f.frame})).unwrap();
    assert_eq!(out["snapshot"], "snap-1");
    assert_eq!(out["nodes"], 5, "A, C, R, G1 and D");
    let second = call(&mut f.ws, "snapshot_layout", json!({"frame": f.frame})).unwrap();
    assert_eq!(second["snapshot"], "snap-2");
    let named = call(
        &mut f.ws,
        "snapshot_layout",
        json!({"frame": f.frame, "name": "before.edit"}),
    )
    .unwrap();
    assert_eq!(named["snapshot"], "before.edit");
    let text = std::fs::read_to_string(f.snapshot_dir().join("snap-1.json")).unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["frame"], json!(f.frame));
    let a = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(f.a))
        .unwrap();
    assert_eq!(a["world"], json!([0.0, 0.0, 100.0, 50.0]));
    assert_eq!(a["opacity"], 1.0);
    let d = parsed["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == json!(f.d))
        .unwrap();
    assert_eq!(
        d["world"],
        json!([500.0, 100.0, 50.0, 50.0]),
        "in the frame, not its group"
    );
    assert_eq!(d["local"], json!([100.0, 100.0, 50.0, 50.0]));
}

#[test]
fn names_are_plain_and_a_project_folder_is_needed() {
    let mut f = fixture();
    for bad in ["../x", "Upper", ".hidden", "a b", "", "a..b"] {
        let err = call(
            &mut f.ws,
            "snapshot_layout",
            json!({"frame": f.frame, "name": bad}),
        )
        .unwrap_err();
        assert!(err.contains("plain snapshot name"), "{bad:?}: {err}");
    }
    assert!(
        !f.snapshot_dir().exists() || std::fs::read_dir(f.snapshot_dir()).unwrap().count() == 0
    );

    let doc = f.ws.session.document().clone();
    let mut scratch = Workspace::new(od_core::Session::new(doc));
    let err = call(&mut scratch, "snapshot_layout", json!({"frame": f.frame})).unwrap_err();
    assert!(err.contains("project folder"), "{err}");

    let missing = call(&mut f.ws, "snapshot_layout", json!({"frame": "n_999999"})).unwrap_err();
    assert!(missing.contains("does not exist"), "{missing}");
}

#[test]
fn a_frame_too_big_to_record_is_refused() {
    let mut f = fixture();
    let ops: Vec<Operation> = (0..MAX_SNAPSHOT_NODES)
        .map(|i| Operation::Create {
            parent: f.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({
                "kind": "rect", "name": format!("n{i}"), "width": 4, "height": 4
            }))
            .unwrap(),
        })
        .collect();
    f.edit(&ops);
    let err = call(&mut f.ws, "snapshot_layout", json!({"frame": f.frame})).unwrap_err();
    assert!(err.contains("at most 500"), "{err}");
}

#[test]
fn old_snapshots_are_dropped_past_the_cap_but_never_the_new_one() {
    let mut f = fixture();
    for i in 0..MAX_SNAPSHOTS + 3 {
        let name = format!("s{i}");
        call(
            &mut f.ws,
            "snapshot_layout",
            json!({"frame": f.frame, "name": name}),
        )
        .unwrap();
    }
    let kept = std::fs::read_dir(f.snapshot_dir()).unwrap().count();
    assert_eq!(kept, MAX_SNAPSHOTS);
    assert!(
        f.snapshot_dir()
            .join(format!("s{}.json", MAX_SNAPSHOTS + 2))
            .exists()
    );
}

#[test]
fn a_move_writes_x_and_y_from_the_old_place_and_no_size_tracks() {
    let mut f = fixture();
    let snap = f.snapshot();
    f.edit(&[Operation::Move {
        id: f.a.clone(),
        x: 200.0,
        y: 100.0,
    }]);
    let out = f.transition(&snap, json!({})).unwrap();
    assert_eq!(f.keys(&f.a, AnimProp::X), vec![(300, 0.0), (600, 200.0)]);
    assert_eq!(f.keys(&f.a, AnimProp::Y), vec![(300, 0.0), (600, 100.0)]);
    assert!(f.keys(&f.a, AnimProp::Width).is_empty());
    assert!(f.keys(&f.a, AnimProp::Height).is_empty());
    assert_eq!(out["moved"].as_array().unwrap().len(), 1);
    let table = out["table"].as_str().unwrap();
    assert!(
        table.contains("| A | moves | x 0->200, y 0->100 |"),
        "{table}"
    );
    assert_eq!(out["unchanged"], 4);
    let doc = f.ws.session.document();
    let shown = doc.at_time(&doc.videos[0], 450.0).unwrap();
    assert_eq!(
        shown.node(&f.a).unwrap().x,
        100.0,
        "halfway at the middle of the move"
    );
    assert_eq!(
        doc.node(&f.a).unwrap().x,
        200.0,
        "the design holds the new place"
    );
}

#[test]
fn a_resize_writes_width_and_height_but_half_a_unit_or_one_writes_nothing() {
    let mut f = fixture();
    let snap = f.snapshot();
    f.set(&f.r.clone(), Property::Width(160.0));
    f.set(&f.a.clone(), Property::Width(100.5));
    f.set(&f.c.clone(), Property::Height(51.0));
    let out = f.transition(&snap, json!({})).unwrap();
    assert_eq!(
        f.keys(&f.r, AnimProp::Width),
        vec![(300, 100.0), (600, 160.0)]
    );
    assert!(f.keys(&f.r, AnimProp::Height).is_empty());
    assert!(f.keys(&f.r, AnimProp::X).is_empty(), "same place");
    assert_eq!(f.track_count(), 1, "0.5 and 1 unit changes are not resizes");
    assert!(out["table"].as_str().unwrap().contains("width 100->160"));

    f.set(&f.c.clone(), Property::Height(51.5));
    let snap2 = f.snapshot();
    f.set(&f.c.clone(), Property::Height(53.5));
    f.transition(&snap2, json!({})).unwrap();
    assert_eq!(
        f.keys(&f.c, AnimProp::Height),
        vec![(300, 51.5), (600, 53.5)]
    );
}

#[test]
fn a_new_node_fades_in_and_a_deleted_one_is_reported_not_animated() {
    let mut f = fixture();
    let snap = f.snapshot();
    let b = create(
        &mut f.ws,
        &f.frame.clone(),
        json!({"kind": "rect", "name": "B", "x": 300, "y": 100, "width": 100, "height": 50,
               "fill": "#ff0000"}),
    );
    f.edit(&[Operation::Delete { id: f.c.clone() }]);
    let out = f.transition(&snap, json!({})).unwrap();
    assert_eq!(f.keys(&b, AnimProp::Opacity), vec![(300, 0.0), (600, 1.0)]);
    assert!(f.keys(&b, AnimProp::X).is_empty());
    assert_eq!(f.track_count(), 1);
    let leaving = out["leaving"].as_array().unwrap();
    assert_eq!(leaving.len(), 1);
    assert_eq!(leaving[0]["id"], json!(f.c));
    assert!(
        out["leaving_note"]
            .as_str()
            .unwrap()
            .contains("cannot be animated")
    );
    let table = out["table"].as_str().unwrap();
    assert!(table.contains("| B | enters | opacity 0->1 |"), "{table}");
    assert!(table.contains("| C | leaves |"), "{table}");
}

#[test]
fn a_node_that_changed_parent_starts_where_it_was_on_the_page() {
    let mut f = fixture();
    let snap = f.snapshot();
    f.edit(&[Operation::Reparent {
        id: f.d.clone(),
        parent: f.frame.clone(),
        index: None,
    }]);
    let live_x = f.ws.session.document().node(&f.d).unwrap().x;
    f.transition(&snap, json!({})).unwrap();
    assert_eq!(f.keys(&f.d, AnimProp::X), vec![(300, 500.0), (600, live_x)]);
    let _ = &f.g;
}

#[test]
fn opacity_and_a_solid_fill_are_carried_and_an_unchanged_node_writes_nothing() {
    let mut f = fixture();
    let snap = f.snapshot();
    f.set(&f.a.clone(), Property::Opacity(0.5));
    f.set(
        &f.r.clone(),
        Property::Fill(Some(Paint::parse("#ff0000").unwrap())),
    );
    f.transition(&snap, json!({})).unwrap();
    assert_eq!(
        f.keys(&f.a, AnimProp::Opacity),
        vec![(300, 1.0), (600, 0.5)]
    );
    let fill = f.keys(&f.r, AnimProp::FillColor);
    assert_eq!(fill.len(), 2);
    assert_eq!(
        fill[0].1,
        od_core::Rgba::parse_hex("#112233").unwrap().pack()
    );
    assert_eq!(
        fill[1].1,
        od_core::Rgba::parse_hex("#ff0000").unwrap().pack()
    );
    assert_eq!(f.track_count(), 2);
}

#[test]
fn a_stagger_spreads_the_starts_in_document_order() {
    let mut f = fixture();
    let snap = f.snapshot();
    f.edit(&[
        Operation::Move {
            id: f.a.clone(),
            x: 50.0,
            y: 0.0,
        },
        Operation::Move {
            id: f.c.clone(),
            x: 50.0,
            y: 200.0,
        },
        Operation::Move {
            id: f.r.clone(),
            x: 250.0,
            y: 0.0,
        },
    ]);
    f.transition(&snap, json!({"stagger": {"each_ms": 100}}))
        .unwrap();
    assert_eq!(f.keys(&f.a, AnimProp::X)[0].0, 300);
    assert_eq!(f.keys(&f.c, AnimProp::X)[0].0, 400);
    assert_eq!(f.keys(&f.r, AnimProp::X)[0].0, 500);
    assert_eq!(f.keys(&f.r, AnimProp::X)[1].0, 800);
}

#[test]
fn it_replaces_what_the_node_had_from_the_start_and_is_one_undoable_turn() {
    let mut f = fixture();
    f.edit(&[Operation::Animate {
        video: f.video.clone(),
        node: f.a.clone(),
        prop: AnimProp::X,
        keys: vec![
            od_core::Key {
                at_ms: 0,
                value: 0.0,
                ease: od_core::Ease::Linear,
            },
            od_core::Key {
                at_ms: 2000,
                value: 20.0,
                ease: od_core::Ease::Linear,
            },
        ],
        repeat: od_core::Repeat::Once,
        additive: false,
    }]);
    let snap = f.snapshot();
    f.edit(&[Operation::Move {
        id: f.a.clone(),
        x: 200.0,
        y: 100.0,
    }]);
    let before = f.ws.session.document().videos[0].animations.clone();
    f.transition(&snap, json!({"turn": "layout"})).unwrap();
    assert_eq!(
        f.keys(&f.a, AnimProp::X),
        vec![(0, 0.0), (300, 0.0), (600, 200.0)]
    );
    assert_eq!(f.track_count(), 2);
    f.ws.session.undo(Actor::agent("tester").unwrap()).unwrap();
    assert_eq!(f.ws.session.document().videos[0].animations, before);
}

#[test]
fn nothing_to_animate_and_bad_snapshots_are_refused_with_a_reason() {
    let mut f = fixture();
    let snap = f.snapshot();
    let same = f.transition(&snap, json!({})).unwrap_err();
    assert!(same.contains("nothing to animate"), "{same}");

    f.edit(&[Operation::Delete { id: f.c.clone() }]);
    let gone = f.transition(&snap, json!({})).unwrap_err();
    assert!(gone.contains("cannot be animated"), "{gone}");

    let unknown = f.transition("nope", json!({})).unwrap_err();
    assert!(unknown.contains("no snapshot"), "{unknown}");
    let invalid = f.transition("../etc", json!({})).unwrap_err();
    assert!(invalid.contains("plain snapshot name"), "{invalid}");

    std::fs::write(f.snapshot_dir().join("broken.json"), "{\"frame\": 1}").unwrap();
    let broken = f.transition("broken", json!({})).unwrap_err();
    assert!(broken.contains("not a layout snapshot"), "{broken}");

    let video = call(
        &mut f.ws,
        "transition_layout",
        json!({"video": "n_999999", "snapshot": snap, "at_ms": 0}),
    )
    .unwrap_err();
    assert!(video.contains("does not exist"), "{video}");
    assert_eq!(f.track_count(), 0, "a refused call changes nothing");
}

#[cfg(unix)]
#[test]
fn a_snapshot_folder_that_is_a_link_is_never_followed() {
    let mut f = fixture();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), f.snapshot_dir()).unwrap();
    let err = call(&mut f.ws, "snapshot_layout", json!({"frame": f.frame})).unwrap_err();
    assert!(err.contains("link"), "{err}");
    assert_eq!(std::fs::read_dir(elsewhere.path()).unwrap().count(), 0);
}

#[test]
fn both_tools_are_listed() {
    let tools = crate::tools::definitions();
    for name in ["snapshot_layout", "transition_layout"] {
        assert!(tools.iter().any(|t| t["name"] == name), "{name}");
    }
}
