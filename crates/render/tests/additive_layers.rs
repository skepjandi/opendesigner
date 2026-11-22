#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! An additive track moves the picture and its signature like any other track.

use od_core::{
    Actor, AnimProp, Document, Ease, Key, NewNode, NodeId, NodeKind, Operation, Paint, Repeat,
    Session,
};
use od_render::Pixmap;
use od_render::video::VideoRenderer;

fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 3] {
    let i = ((y * p.width() + x) * 4) as usize;
    let d = p.data();
    [d[i], d[i + 1], d[i + 2]]
}

fn key(at_ms: u64, value: f64) -> Key {
    Key {
        at_ms,
        value,
        ease: Ease::Linear,
    }
}

/// A white 20x20 square at x 0 on black; its x travels 0 to 80 over a second when `travel`, and
/// an additive bob adds 0, 20, 0 over the first second.
fn scene(travel: bool) -> (Session, NodeId) {
    let mut s = Session::new(Document::new("v"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    (frame.width, frame.height) = (100.0, 100.0);
    frame.fill = Paint::parse("#000000").ok();
    let mut square = NewNode::new(NodeKind::Rect);
    (square.y, square.width, square.height) = (40.0, 20.0, 20.0);
    square.fill = Paint::parse("#ffffff").ok();
    frame.children = vec![square];
    let r = s
        .apply(
            Actor::User,
            None,
            &[
                Operation::Create {
                    parent: root,
                    index: None,
                    node: frame,
                },
                Operation::CreateVideo {
                    name: "a".into(),
                    fps: 10,
                    width: 100,
                    height: 100,
                },
            ],
        )
        .unwrap();
    let (frame, video) = (r.created[0].clone(), r.created[1].clone());
    let square = s.document().node(&frame).unwrap().children[0].id.clone();
    let animate = |keys: Vec<Key>, additive: bool| Operation::Animate {
        video: video.clone(),
        node: square.clone(),
        prop: AnimProp::X,
        keys,
        repeat: Repeat::Once,
        additive,
    };
    let mut ops = vec![
        Operation::AddClip {
            video: video.clone(),
            frame: Some(frame),
            footage: None,
            duration_ms: 2000,
            index: None,
            transition: None,
            motion: None,
        },
        animate(vec![key(0, 0.0), key(500, 20.0), key(1000, 0.0)], true),
    ];
    if travel {
        ops.push(animate(vec![key(0, 0.0), key(1000, 80.0)], false));
    }
    s.apply(Actor::User, None, &ops).unwrap();
    (s, video)
}

fn at(s: &Session, video: &NodeId, ms: u64) -> Pixmap {
    let doc = s.document();
    VideoRenderer::new(doc, doc.video(video).unwrap(), None)
        .frame_at(ms)
        .unwrap()
}

#[test]
fn a_bob_rides_on_a_travel_and_a_lone_bob_moves_the_node_on_its_own() {
    let (s, v) = scene(true);
    // halfway the travel is at 40 and the bob adds 20: the square covers 60..80
    let mid = at(&s, &v, 500);
    assert_eq!(px(&mid, 70, 50), [255, 255, 255]);
    assert_eq!(
        px(&mid, 50, 50),
        [0, 0, 0],
        "not where the travel alone would put it"
    );
    assert_eq!(
        px(&at(&s, &v, 1000), 90, 50),
        [255, 255, 255],
        "the bob is over, the travel ends at 80"
    );

    let (alone, v2) = scene(false);
    assert_eq!(px(&at(&alone, &v2, 0), 10, 50), [255, 255, 255]);
    assert_eq!(
        px(&at(&alone, &v2, 500), 30, 50),
        [255, 255, 255],
        "the bob alone lifts it 20"
    );
    assert_eq!(px(&at(&alone, &v2, 500), 10, 50), [0, 0, 0]);
}

#[test]
fn a_still_base_under_a_moving_bob_changes_the_signature_and_a_finished_one_does_not() {
    let (s, v) = scene(false);
    let doc = s.document();
    let r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
    assert_ne!(r.signature(200), r.signature(300), "the bob is moving");
    assert_eq!(
        r.signature(1200),
        r.signature(1800),
        "after its last key it holds still"
    );
}
