//! `cargo test --release -p opendesigner-agent --test export_bench -- --ignored --nocapture`
//! times the export of a 886 x 1920 video of 5.4 s at 30 fps with one thread and with all of them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Instant;

use od_agent::video_export::{self, Source};
use od_core::{
    Actor, Ease, NewNode, NodeId, NodeKind, Operation, Paint, Transition, TransitionKind, Workspace,
};
use od_media::Ffmpeg;

fn screen(ws: &mut Workspace, color: &str, title: &str) -> NodeId {
    let root = ws.session.document().pages[0].root.id.clone();
    let mut node = NewNode::new(NodeKind::Frame);
    node.width = 886.0;
    node.height = 1920.0;
    node.fill = Paint::parse(color).ok();
    let mut card = NewNode::new(NodeKind::Rect);
    (card.x, card.y, card.width, card.height) = (60.0, 400.0, 766.0, 900.0);
    card.corner_radius = Some(48.0);
    card.fill = Paint::parse("#ffffff").ok();
    let mut text = NewNode::new(NodeKind::Text);
    (text.x, text.y, text.width, text.height) = (100.0, 480.0, 686.0, 300.0);
    text.text = Some(title.into());
    text.font_size = Some(96.0);
    text.fill = Paint::parse("#111111").ok();
    node.children.push(card);
    node.children.push(text);
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node,
            }],
        )
        .unwrap()
        .created[0]
        .clone()
}

#[test]
#[ignore = "a benchmark: run it in release mode"]
fn export_with_one_thread_and_with_all() {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("no ffmpeg");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("p.od");
    let mut ws = Workspace::create(&project, "Bench").unwrap();
    let frames: Vec<NodeId> = [("#cc3300", "One"), ("#0066cc", "Two"), ("#33aa55", "Three")]
        .iter()
        .map(|(c, t)| screen(&mut ws, c, t))
        .collect();
    let video = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "Bench".into(),
                fps: 30,
                width: 886,
                height: 1920,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    for (n, frame) in frames.into_iter().enumerate() {
        let transition = (n > 0).then_some(Transition {
            kind: TransitionKind::SlideLeft,
            duration_ms: 600,
            ease: Ease::EaseInOut,
        });
        ws.session
            .apply(
                Actor::User,
                None,
                &[Operation::AddClip {
                    video: video.clone(),
                    frame: Some(frame),
                    footage: None,
                    duration_ms: 1800,
                    index: None,
                    transition,
                    motion: Some(od_core::Motion {
                        scale_from: 1.0,
                        scale_to: 1.1,
                        ease: Ease::Linear,
                    }),
                }],
            )
            .unwrap();
    }
    ws.save().unwrap();
    let source = Source::of(&ws);
    let all = od_render::export::default_workers();
    for workers in [1, 2, 4, all] {
        let mut best = f64::MAX;
        for _ in 0..3 {
            let started = Instant::now();
            video_export::export_video_with(
                &source,
                &video,
                "bench.mp4",
                None,
                None,
                &ffmpeg,
                workers,
                &mut |_, _| true,
            )
            .unwrap();
            best = best.min(started.elapsed().as_secs_f64());
        }
        println!("{workers} worker(s): {best:.2} s");
    }
}
