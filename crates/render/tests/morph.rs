#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! A path node that morphs: the `morph_progress` animation turns the node's shape into the next
//! one, frame by frame, through `Document::at_time`.

use od_core::animation::Repeat;
use od_core::morph::{PathMorph, match_pair};
use od_core::{
    Actor, AnimProp, Document, Ease, Key, NewNode, NodeId, NodeKind, Operation, Paint, Property,
    Session,
};
use od_render::video::VideoRenderer;

const RED: [u8; 3] = [255, 0, 0];
const WHITE: [u8; 3] = [255, 255, 255];

fn create(s: &mut Session, parent: &NodeId, node: NewNode) -> NodeId {
    s.apply(
        Actor::User,
        None,
        &[Operation::Create {
            parent: parent.clone(),
            index: None,
            node,
        }],
    )
    .unwrap()
    .created[0]
        .clone()
}

fn square(x: f64) -> Vec<[f64; 2]> {
    vec![[x, 0.0], [x + 40.0, 0.0], [x + 40.0, 40.0], [x, 40.0]]
}

/// A 40 x 40 red square at (10, 10) that morphs into the same square 60 units to the right
/// over a second.
fn scene(stops: usize) -> (Session, NodeId, NodeId) {
    let mut s = Session::new(Document::new("morph"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 120.0;
    frame.height = 60.0;
    frame.fill = Some(Paint::parse("#ffffff").unwrap());
    let frame = create(&mut s, &root, frame);
    let mut shape = NewNode::new(NodeKind::Path);
    shape.path = Some("M0 0 H40 V40 H0 Z".into());
    shape.x = 10.0;
    shape.y = 10.0;
    shape.fill = Some(Paint::parse("#ff0000").unwrap());
    let node = create(&mut s, &frame, shape);
    let m = match_pair(&square(0.0), true, &square(60.0), true);
    let morph = PathMorph {
        width: 40.0,
        height: 40.0,
        closed: vec![true],
        stops: (0..stops)
            .map(|i| vec![if i == 0 { m.from.clone() } else { m.to.clone() }])
            .collect(),
    };
    s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: node.clone(),
            prop: Property::PathMorph(Some(morph)),
        }],
    )
    .unwrap();
    let video = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "V".into(),
                fps: 30,
                width: 120,
                height: 60,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    s.apply(
        Actor::User,
        None,
        &[Operation::AddClip {
            video: video.clone(),
            frame: Some(frame),
            footage: None,
            duration_ms: 1000,
            index: None,
            transition: None,
            motion: None,
        }],
    )
    .unwrap();
    (s, video, node)
}

fn animate(s: &mut Session, video: &NodeId, node: &NodeId, last: f64) -> Result<(), String> {
    let key = |at_ms, value| Key {
        at_ms,
        value,
        ease: Ease::Linear,
    };
    s.apply(
        Actor::User,
        None,
        &[Operation::Animate {
            additive: false,
            video: video.clone(),
            node: node.clone(),
            prop: AnimProp::MorphProgress,
            repeat: Repeat::Once,
            keys: vec![key(0, 0.0), key(1000, last)],
        }],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn pixel(r: &mut VideoRenderer, ms: u64, x: u32, y: u32) -> [u8; 4] {
    let p = r.frame_at(ms).unwrap();
    let c = p.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn is(got: [u8; 4], want: [u8; 3]) -> bool {
    (0..3).all(|i| (i32::from(got[i]) - i32::from(want[i])).abs() <= 1)
}

#[test]
fn the_square_slides_across_as_the_morph_plays() {
    let (mut s, video, node) = scene(2);
    animate(&mut s, &video, &node, 1.0).unwrap();
    let doc = s.document();
    let v = doc.videos.iter().find(|v| v.id == video).unwrap();
    let mut r = VideoRenderer::new(doc, v, None);
    assert!(is(pixel(&mut r, 0, 30, 30), RED), "starts on the left");
    assert!(is(pixel(&mut r, 0, 100, 30), WHITE));
    // halfway the square is at x 40..80
    assert!(is(pixel(&mut r, 500, 60, 30), RED), "halfway in the middle");
    assert!(is(pixel(&mut r, 500, 15, 30), WHITE));
    assert!(is(pixel(&mut r, 500, 100, 30), WHITE));
    assert!(is(pixel(&mut r, 1000, 100, 30), RED), "ends on the right");
    assert!(is(pixel(&mut r, 1000, 30, 30), WHITE));
}

#[test]
fn a_node_resting_with_a_morph_still_draws_its_own_shape() {
    let (s, _, node) = scene(2);
    let n = s.document().node(&node).unwrap();
    assert_eq!(n.path.as_deref(), Some("M0 0 H40 V40 H0 Z"));
}

#[test]
fn progress_past_the_last_stop_is_refused() {
    let (mut s, video, node) = scene(2);
    assert!(animate(&mut s, &video, &node, 2.0).is_err());
}

#[test]
fn progress_needs_a_morph_on_the_node() {
    let (mut s, video, node) = scene(2);
    s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: node.clone(),
            prop: Property::PathMorph(None),
        }],
    )
    .unwrap();
    assert!(animate(&mut s, &video, &node, 1.0).is_err());
}

#[test]
fn a_third_stop_continues_the_morph() {
    let (mut s, video, node) = scene(3);
    animate(&mut s, &video, &node, 2.0).unwrap();
    let doc = s.document();
    let v = doc.videos.iter().find(|v| v.id == video).unwrap();
    let mut r = VideoRenderer::new(doc, v, None);
    assert!(
        is(pixel(&mut r, 250, 60, 30), RED),
        "a quarter in: halfway to stop 1"
    );
    assert!(is(pixel(&mut r, 500, 90, 30), RED), "halfway in: at stop 1");
    assert!(is(pixel(&mut r, 500, 30, 30), WHITE));
}

#[test]
fn a_morph_cannot_be_cleared_or_shortened_under_the_keys_that_use_it() {
    let (mut s, video, node) = scene(3);
    animate(&mut s, &video, &node, 2.0).unwrap();
    let set = |s: &mut Session, morph: Option<PathMorph>| {
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: node.clone(),
                prop: Property::PathMorph(morph),
            }],
        )
    };
    assert!(set(&mut s, None).is_err(), "cleared under its keys");
    let mut shorter = s.document().node(&node).unwrap().morph.clone().unwrap();
    shorter.stops.truncate(2);
    assert!(
        set(&mut s, Some(shorter.clone())).is_err(),
        "shortened under its keys"
    );
    animate(&mut s, &video, &node, 1.0).unwrap();
    set(&mut s, Some(shorter)).unwrap();
    animate(&mut s, &video, &node, 0.0).unwrap();
    assert!(set(&mut s, None).is_err(), "a key at 0 still needs a morph");
}
