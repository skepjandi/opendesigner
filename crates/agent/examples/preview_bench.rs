//! How long the video preview takes: the first picture, the whole run, and what an edit costs
//! when the pictures that did not change are kept.
//!
//! `cargo run --release -p opendesigner-agent --example preview_bench`

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use od_agent::video_export::Source;
use od_agent::video_preview::{Limits, Preview, Reuse};
use od_core::{Actor, Document, NewNode, NodeKind, Operation, Paint, Session};

fn frame(s: &mut Session, root: &od_core::NodeId, color: &str, y: f64) -> od_core::NodeId {
    let mut n = NewNode::new(NodeKind::Frame);
    n.width = 1000.0;
    n.height = 2000.0;
    n.fill = Paint::parse(color).ok();
    n.y = y;
    s.apply(
        Actor::User,
        None,
        &[Operation::Create {
            parent: root.clone(),
            index: None,
            node: n,
        }],
    )
    .unwrap()
    .created[0]
        .clone()
}

/// A ten second video of six design frames, each holding for a second and a half.
fn video() -> (Session, od_core::NodeId) {
    let mut s = Session::new(Document::new("bench"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let v = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "v".into(),
                fps: 30,
                width: 1000,
                height: 2000,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    for i in 0..6 {
        let color = format!("#{:02x}3355", 40 + i * 30);
        let f = frame(&mut s, &root, &color, 0.0);
        s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame: Some(f),
                footage: None,
                duration_ms: 1500,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
    }
    (s, v)
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let (session, v) = video();
    // what one picture costs to keep: a PNG (what the preview used to keep) against the
    // run-length code it keeps now
    {
        let doc = session.document();
        let mut renderer = od_render::video::VideoRenderer::new(doc, doc.video(&v).unwrap(), None)
            .with_size(480, 960);
        let picture = renderer.frame_at(100).unwrap();
        let t = Instant::now();
        let png = od_render::encode_png(&picture, false).unwrap();
        let png_ms = ms(t.elapsed());
        let t = Instant::now();
        let coded = od_agent::preview_frames::Frame::encode(&picture);
        let coded_ms = ms(t.elapsed());
        println!(
            "  one picture kept:         png {:.1} ms ({} bytes), run-length {:.1} ms ({} bytes)",
            png_ms,
            png.len(),
            coded_ms,
            coded.len()
        );
    }
    println!("preview timings (milliseconds)");

    // what the preview cost before: every picture drawn and kept as a PNG, and every change
    // throwing all of it away
    {
        let doc = session.document();
        let v = doc.video(&v).unwrap();
        let total = (v.duration_ms() * 30).div_ceil(1000);
        let mut renderer = od_render::video::VideoRenderer::new(doc, v, None).with_size(480, 960);
        let t = Instant::now();
        let mut held: Option<(Vec<i64>, Vec<u8>)> = None;
        let mut drawn = 0;
        for n in 0..total {
            let time_ms = n * 1000 / 30;
            let sig = renderer.signature(time_ms);
            let _png = match (&sig, &held) {
                (Some(s), Some((last, png))) if s == last => png.clone(),
                _ => {
                    drawn += 1;
                    let picture = renderer.frame_at(time_ms).unwrap();
                    let png = od_render::encode_png(&picture, false).unwrap();
                    held = sig.map(|s| (s, png.clone()));
                    png
                }
            };
        }
        println!(
            "  the old way (png, no reuse): {:.1}  ({drawn} pictures drawn and encoded)",
            ms(t.elapsed())
        );
    }

    // the first picture: how long until the user sees something
    let mut first = Preview::start(
        Source::of_document(session.document().clone()),
        &v,
        Limits::default(),
        Reuse::default(),
    )
    .unwrap();
    let t = Instant::now();
    while first.status().ready == 0 {
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("  first picture            {:.1}", ms(t.elapsed()));
    let t = Instant::now();
    first.wait();
    let whole = t.elapsed();
    let status = first.status();
    println!(
        "  whole preview            {:.1}  ({} pictures at {}x{}, {} drawn, {} reused)",
        ms(whole),
        status.ready,
        status.width,
        status.height,
        status.drawn,
        status.reused
    );

    // an edit that changes one frame: the pictures that did not change are kept
    let mut edited = session.document().clone();
    let blue = edited.video(&v).unwrap().clips[3].frame.clone().unwrap();
    let loc = edited.locate(&blue).unwrap();
    edited.node_at_mut(&loc).unwrap().fill = Paint::parse("#00ff00").ok();
    let t = Instant::now();
    let mut after = Preview::start(
        Source::of_document(edited),
        &v,
        Limits::default(),
        first.reusable(),
    )
    .unwrap();
    let mut drawn = 0;
    let mut first_picture = None;
    while !after.status().done {
        let status = after.status();
        if status.ready > 0 && first_picture.is_none() {
            first_picture = Some(t.elapsed());
        }
        drawn = status.drawn;
        std::thread::sleep(Duration::from_millis(1));
    }
    after.wait();
    let status = after.status();
    println!(
        "  after an edit             {:.1}  ({} pictures, {} drawn, {} reused)",
        ms(t.elapsed()),
        status.ready,
        drawn,
        status.reused
    );
    if let Some(f) = first_picture {
        println!("  first picture after it    {:.1}", ms(f));
    }
}
