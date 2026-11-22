#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Additive animation tracks: an offset laid over the node's value (or its replacing track), so a
//! bob can ride on a travel (`docs/wiki/video-editing.md#additive-layers`).

use od_core::{
    Actor, AnimProp, Document, Ease, Key, NewNode, NodeId, NodeKind, OpError, Operation, Property,
    Repeat, Session,
};

fn apply(s: &mut Session, ops: &[Operation]) -> Result<od_core::TxReceipt, OpError> {
    s.apply(Actor::Agent("t".into()), None, ops)
}

struct Fixture {
    s: Session,
    rect: NodeId,
    video: NodeId,
}

fn fixture() -> Fixture {
    let mut s = Session::new(Document::new("t"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    (frame.width, frame.height) = (400.0, 300.0);
    let mut rect = NewNode::new(NodeKind::Rect);
    (rect.width, rect.height) = (100.0, 50.0);
    rect.opacity = Some(0.5);
    frame.children = vec![rect];
    let r = apply(
        &mut s,
        &[
            Operation::Create {
                parent: root,
                index: None,
                node: frame,
            },
            Operation::CreateVideo {
                name: "v".into(),
                fps: 30,
                width: 400,
                height: 300,
            },
        ],
    )
    .unwrap();
    let frame = r.created[0].clone();
    let rect = s.document().node(&frame).unwrap().children[0].id.clone();
    Fixture {
        rect,
        video: r.created[1].clone(),
        s,
    }
}

fn key(at_ms: u64, value: f64) -> Key {
    Key {
        at_ms,
        value,
        ease: Ease::Linear,
    }
}

impl Fixture {
    fn animate(
        &mut self,
        prop: AnimProp,
        keys: Vec<Key>,
        additive: bool,
    ) -> Result<od_core::TxReceipt, OpError> {
        apply(
            &mut self.s,
            &[Operation::Animate {
                video: self.video.clone(),
                node: self.rect.clone(),
                prop,
                keys,
                repeat: Repeat::Once,
                additive,
            }],
        )
    }

    fn value(&self, prop: AnimProp, ms: f64) -> f64 {
        let doc = self.s.document();
        let video = doc.video(&self.video).unwrap();
        let shown = doc.at_time(video, ms).unwrap();
        prop.read(shown.node(&self.rect).unwrap()).unwrap()
    }

    fn tracks(&self) -> Vec<(AnimProp, bool)> {
        self.s
            .document()
            .video(&self.video)
            .unwrap()
            .animations
            .iter()
            .map(|a| (a.prop, a.additive))
            .collect()
    }
}

#[test]
fn an_additive_track_is_summed_onto_the_replacing_track() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(1000, 100.0)], false)
        .unwrap();
    f.animate(
        AnimProp::X,
        vec![key(0, 0.0), key(300, 10.0), key(600, 0.0)],
        true,
    )
    .unwrap();
    assert_eq!(f.value(AnimProp::X, 300.0), 30.0 + 10.0);
    assert_eq!(f.value(AnimProp::X, 600.0), 60.0);
    assert_eq!(f.value(AnimProp::X, 1000.0), 100.0, "the bob has ended");
}

#[test]
fn the_same_keys_without_the_flag_replace_instead_of_adding() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(1000, 100.0)], false)
        .unwrap();
    f.animate(AnimProp::Y, vec![key(0, 0.0), key(300, 10.0)], false)
        .unwrap();
    assert_eq!(f.value(AnimProp::Y, 300.0), 10.0);
    f.animate(AnimProp::X, vec![key(0, 0.0), key(300, 10.0)], false)
        .unwrap();
    assert_eq!(
        f.value(AnimProp::X, 300.0),
        10.0,
        "one replacing track per property"
    );
    assert_eq!(f.tracks().len(), 2);
}

#[test]
fn an_additive_track_alone_adds_to_the_nodes_own_value() {
    let mut f = fixture();
    apply(
        &mut f.s,
        &[Operation::Set {
            id: f.rect.clone(),
            prop: Property::X(20.0),
        }],
    )
    .unwrap();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(1000, 50.0)], true)
        .unwrap();
    assert_eq!(f.value(AnimProp::X, 0.0), 20.0);
    assert_eq!(f.value(AnimProp::X, 500.0), 45.0);
}

#[test]
fn tracks_are_ordered_replacing_before_additive_whatever_the_order_they_were_written_in() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], true)
        .unwrap();
    f.animate(AnimProp::X, vec![key(0, 10.0), key(100, 20.0)], false)
        .unwrap();
    assert_eq!(f.tracks(), vec![(AnimProp::X, false), (AnimProp::X, true)]);
    assert_eq!(f.value(AnimProp::X, 100.0), 25.0);
}

#[test]
fn a_second_additive_animate_replaces_the_first_one() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], true)
        .unwrap();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 7.0)], true)
        .unwrap();
    assert_eq!(f.tracks(), vec![(AnimProp::X, true)]);
    assert_eq!(f.value(AnimProp::X, 100.0), 7.0);
}

#[test]
fn the_sum_is_held_to_the_property_range() {
    let mut f = fixture();
    f.animate(AnimProp::Opacity, vec![key(0, 0.0), key(100, 0.4)], true)
        .unwrap();
    assert!((f.value(AnimProp::Opacity, 100.0) - 0.9).abs() < 1e-9);
    f.animate(AnimProp::Opacity, vec![key(0, 0.0), key(100, 0.9)], true)
        .unwrap();
    assert_eq!(
        f.value(AnimProp::Opacity, 100.0),
        1.0,
        "0.5 + 0.9 is held to 1"
    );
    f.animate(AnimProp::Opacity, vec![key(0, 0.0), key(100, -0.9)], true)
        .unwrap();
    assert_eq!(
        f.value(AnimProp::Opacity, 100.0),
        0.0,
        "0.5 - 0.9 is held to 0"
    );
}

#[test]
fn offsets_may_be_negative_but_not_wider_than_the_property_ever_spans() {
    let mut f = fixture();
    f.animate(AnimProp::Opacity, vec![key(0, -0.3)], true)
        .unwrap();
    assert!(matches!(
        f.animate(AnimProp::Opacity, vec![key(0, 1.5)], true),
        Err(OpError::OutOfBounds { .. })
    ));
    assert!(matches!(
        f.animate(AnimProp::Opacity, vec![key(0, 1.5)], false),
        Err(OpError::OutOfBounds { .. })
    ));
}

#[test]
fn a_colour_cannot_be_additive_and_a_property_the_node_lacks_is_refused() {
    let mut f = fixture();
    let colour = f.animate(AnimProp::FillColor, vec![key(0, 255.0)], true);
    assert!(matches!(
        colour,
        Err(OpError::OutOfBounds {
            what: "additive",
            ..
        })
    ));
    let turn = f.animate(AnimProp::RotateY, vec![key(0, 5.0)], true);
    assert!(matches!(turn, Err(OpError::NotApplicable { .. })));
    assert!(f.tracks().is_empty());
}

#[test]
fn undo_and_redo_put_the_additive_track_back_and_take_it_away() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(1000, 100.0)], false)
        .unwrap();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], true)
        .unwrap();
    assert_eq!(f.tracks().len(), 2);
    f.s.undo(Actor::Agent("t".into())).unwrap();
    assert_eq!(
        f.tracks(),
        vec![(AnimProp::X, false)],
        "only the additive one went"
    );
    f.s.redo(Actor::Agent("t".into())).unwrap();
    assert_eq!(f.tracks(), vec![(AnimProp::X, false), (AnimProp::X, true)]);
    assert_eq!(f.value(AnimProp::X, 100.0), 15.0);
}

#[test]
fn replacing_an_additive_track_undoes_to_the_earlier_additive_keys() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], true)
        .unwrap();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 9.0)], true)
        .unwrap();
    f.s.undo(Actor::Agent("t".into())).unwrap();
    assert_eq!(f.value(AnimProp::X, 100.0), 5.0);
}

#[test]
fn the_flag_round_trips_in_operations_and_files_and_is_absent_when_false() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], false)
        .unwrap();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], true)
        .unwrap();
    let video = serde_json::to_value(f.s.document().video(&f.video).unwrap()).unwrap();
    let tracks = video["animations"].as_array().unwrap();
    assert!(
        tracks[0].get("additive").is_none(),
        "old files read the same"
    );
    assert_eq!(tracks[1]["additive"], true);

    let op: Operation = serde_json::from_value(serde_json::json!({
        "op": "animate", "video": f.video, "node": f.rect, "prop": "x",
        "keys": [{"at_ms": 0, "value": 1.0}], "additive": true
    }))
    .unwrap();
    assert!(matches!(op, Operation::Animate { additive: true, .. }));
    let plain: Operation = serde_json::from_value(serde_json::json!({
        "op": "animate", "video": f.video, "node": f.rect, "prop": "x",
        "keys": [{"at_ms": 0, "value": 1.0}]
    }))
    .unwrap();
    assert!(matches!(
        plain,
        Operation::Animate {
            additive: false,
            ..
        }
    ));
    assert!(
        serde_json::to_value(&plain)
            .unwrap()
            .get("additive")
            .is_none()
    );

    let back: od_core::Video = serde_json::from_value(video).unwrap();
    assert_eq!(back, *f.s.document().video(&f.video).unwrap());
}

#[test]
fn a_track_out_of_order_or_twice_is_refused_when_a_video_is_validated() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], false)
        .unwrap();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(100, 5.0)], true)
        .unwrap();
    let mut doc = f.s.document().clone();
    doc.validate().unwrap();
    doc.videos[0].animations.reverse();
    assert!(
        doc.validate().is_err(),
        "an additive track before its replacing one"
    );
    doc.videos[0].animations.reverse();
    let twin = doc.videos[0].animations[1].clone();
    doc.videos[0].animations.push(twin);
    assert!(
        doc.validate().is_err(),
        "two additive tracks on one property"
    );
}

#[test]
fn an_additive_track_counts_as_moving_where_its_keys_change_even_over_a_still_base() {
    let mut f = fixture();
    f.animate(AnimProp::X, vec![key(0, 10.0), key(1000, 10.0)], false)
        .unwrap();
    f.animate(AnimProp::X, vec![key(0, 0.0), key(500, 8.0)], true)
        .unwrap();
    let video = f.s.document().video(&f.video).unwrap();
    let moving = |ms: f64| video.animations.iter().any(|a| a.moving_at(ms));
    assert!(moving(250.0), "the bob moves");
    assert!(!moving(750.0), "after the bob's last key nothing moves");
    let (base, bob) = (&video.animations[0], &video.animations[1]);
    assert!(!base.moving_at(250.0) && bob.moving_at(250.0));
}

#[test]
fn camera_props_take_additive_tracks_too() {
    let mut f = fixture();
    let camera = |prop, keys, additive| Operation::Animate {
        video: f.video.clone(),
        node: f.video.clone(),
        prop,
        keys,
        repeat: Repeat::Once,
        additive,
    };
    let ops = [
        camera(
            AnimProp::CameraX,
            vec![key(0, 0.0), key(1000, 100.0)],
            false,
        ),
        camera(AnimProp::CameraX, vec![key(0, 4.0), key(1000, 4.0)], true),
    ];
    apply(&mut f.s, &ops).unwrap();
    let video = f.s.document().video(&f.video).unwrap();
    assert_eq!(video.camera_at(500.0).x, 54.0);
    assert_eq!(video.camera_at(0.0).x, 4.0);
}

#[test]
fn animated_value_stacks_the_tracks_over_a_given_base() {
    let mut f = fixture();
    f.animate(AnimProp::Y, vec![key(0, 0.0), key(100, 10.0)], true)
        .unwrap();
    let video = f.s.document().video(&f.video).unwrap();
    assert_eq!(
        video.animated_value(&f.rect, AnimProp::Y, 100.0, 30.0),
        40.0
    );
    assert!(video.base_track(&f.rect, AnimProp::Y).is_none());
}
