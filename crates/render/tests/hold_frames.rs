#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! "On twos": a video with `hold_frames` shows each drawing for several frames.

use od_core::{
    Actor, Document, Ease, Motion, NewNode, NodeId, NodeKind, Operation, Paint, Session,
    VideoProperty, timebase,
};
use od_render::export::{SourceFactory, export_frames};
use od_render::video::VideoRenderer;

/// A 24 fps video (frames fall between whole milliseconds) of one clip that zooms steadily, so
/// every frame is different unless frames are held.
fn zooming_video(hold: u32) -> (Session, NodeId) {
    let mut s = Session::new(Document::new("v"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    (frame.width, frame.height) = (64.0, 48.0);
    frame.fill = Some(Paint::parse("#ffffff").unwrap());
    let frame = create(&mut s, root, frame);
    let mut dot = NewNode::new(NodeKind::Rect);
    (dot.x, dot.y, dot.width, dot.height) = (20.0, 14.0, 24.0, 20.0);
    dot.fill = Some(Paint::parse("#cc2244").unwrap());
    create(&mut s, frame.clone(), dot);
    let video = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "v".into(),
                fps: 24,
                width: 64,
                height: 48,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    let ops = [
        Operation::AddClip {
            video: video.clone(),
            frame: Some(frame),
            footage: None,
            duration_ms: 1000,
            index: None,
            transition: None,
            motion: Some(Motion {
                scale_from: 1.0,
                scale_to: 1.6,
                ease: Ease::Linear,
            }),
        },
        Operation::SetVideo {
            id: video.clone(),
            prop: VideoProperty::HoldFrames(hold),
        },
    ];
    s.apply(Actor::User, None, &ops).unwrap();
    (s, video)
}

fn create(s: &mut Session, parent: NodeId, node: NewNode) -> NodeId {
    s.apply(
        Actor::User,
        None,
        &[Operation::Create {
            parent,
            index: None,
            node,
        }],
    )
    .unwrap()
    .created[0]
        .clone()
}

fn frame_pixels(s: &Session, video: &NodeId, n: u64) -> Vec<u8> {
    let doc = s.document();
    let video = doc.video(video).unwrap();
    VideoRenderer::new(doc, video, None)
        .frame_at(timebase::frame_ms(n, video.fps))
        .unwrap()
        .data()
        .to_vec()
}

#[test]
fn two_frames_share_a_drawing_and_the_next_pair_differs() {
    let (s, v) = zooming_video(2);
    let frames: Vec<Vec<u8>> = (0..6).map(|n| frame_pixels(&s, &v, n)).collect();
    assert!(frames[0] == frames[1]);
    assert!(frames[0] != frames[2]);
    assert!(frames[2] == frames[3]);
    assert!(frames[3] != frames[4]);
    assert!(frames[4] == frames[5]);
}

#[test]
fn three_frames_hold_in_threes_and_one_holds_nothing() {
    let (s, v) = zooming_video(3);
    let f: Vec<Vec<u8>> = (0..7).map(|n| frame_pixels(&s, &v, n)).collect();
    assert!(f[0] == f[1] && f[1] == f[2] && f[2] != f[3] && f[3] == f[5] && f[5] != f[6]);
    let (s, v) = zooming_video(1);
    assert!(frame_pixels(&s, &v, 0) != frame_pixels(&s, &v, 1));
}

#[test]
fn a_held_frame_is_the_drawing_of_its_first_frame_wherever_it_is_asked_for() {
    let (s, v) = zooming_video(2);
    let doc = s.document();
    let video = doc.video(&v).unwrap();
    let mut r = VideoRenderer::new(doc, video, None);
    let first = r.frame_at(timebase::frame_ms(4, 24)).unwrap();
    // 5 starts at 208 ms; anything up to the end of frame 5 still shows frame 4's drawing
    let mid = r.frame_at(timebase::frame_ms(5, 24) + 20).unwrap();
    assert_eq!(first.data(), mid.data());
    assert_eq!(
        r.signature(timebase::frame_ms(4, 24)),
        r.signature(timebase::frame_ms(5, 24))
    );
    assert_ne!(
        r.signature(timebase::frame_ms(5, 24)),
        r.signature(timebase::frame_ms(6, 24))
    );
}

#[test]
fn any_worker_count_exports_held_frames_as_a_plain_loop_draws_them() {
    let (s, v) = zooming_video(2);
    let doc = s.document();
    let video = doc.video(&v).unwrap();
    let expected: Vec<Vec<u8>> = (0..video.frame_count())
        .map(|n| frame_pixels(&s, &v, n))
        .collect();
    let none: &SourceFactory = &|| None;
    for workers in [1, 2, 5] {
        let mut got: Vec<Vec<u8>> = Vec::new();
        export_frames(doc, video, None, none, workers, &mut |_, px| {
            got.push(px.to_vec());
            Ok(true)
        })
        .unwrap();
        assert!(got == expected, "{workers} workers");
    }
    assert!(
        expected
            .chunks(2)
            .all(|pair| pair.len() < 2 || pair[0] == pair[1])
    );
}
