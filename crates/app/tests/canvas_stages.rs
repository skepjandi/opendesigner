//! What the canvas picture costs on its way to the window, stage by stage: what the Rust core
//! spends drawing and encoding it, what Qt spends reading it back, and what the raw path costs
//! instead. It prints a table and asserts nothing; the numbers are for the machine that ran it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use cxx_qt_lib::QImage;
use od_core::{Actor, Document, NewNode, NodeKind, Operation, Paint, Session};

fn build(nodes: usize) -> (Session, od_core::NodeId) {
    let mut s = Session::new(Document::new("stages"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 1320.0;
    frame.height = 2868.0;
    frame.fill = Paint::parse("#060b1c").ok();
    let mut children = Vec::new();
    for i in 0..nodes {
        let mut n = NewNode::new(if i % 3 == 0 {
            NodeKind::Ellipse
        } else {
            NodeKind::Rect
        });
        n.x = (i % 40) as f64 * 32.0;
        n.y = i as f64 * 2.7;
        n.width = 60.0;
        n.height = 40.0;
        n.fill = Paint::parse(&format!("#{:02x}{:02x}{:02x}", i % 256, (i * 7) % 256, 90)).ok();
        children.push(n);
    }
    frame.children = children;
    let receipt = s
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: frame,
            }],
        )
        .unwrap();
    let id = receipt.created[0].clone();
    (s, id)
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[test]
fn the_canvas_picture_is_measured_stage_by_stage() {
    let (mut s, frame) = build(10_000);
    let scale = 0.3_f32;
    println!("canvas stages at 10,000 nodes, scale {scale} (milliseconds, median of 7)");

    let mut render: Vec<std::time::Duration> = Vec::new();
    let mut encode: Vec<std::time::Duration> = Vec::new();
    let mut base64: Vec<std::time::Duration> = Vec::new();
    let mut decode: Vec<std::time::Duration> = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        let drawn = od_render::render_frame(s.document(), &frame, scale, None).unwrap();
        render.push(t.elapsed());
        let t = Instant::now();
        let png = od_render::encode_png(&drawn.pixmap, true).unwrap();
        encode.push(t.elapsed());
        let t = Instant::now();
        let url = STANDARD.encode(&png);
        base64.push(t.elapsed());
        let t = Instant::now();
        let image = QImage::from_data(&png, None).expect("Qt reads the png");
        decode.push(t.elapsed());
        assert_eq!(image.width(), drawn.pixmap.width() as i32);
        std::hint::black_box(&url);
    }
    // the raw path: the same pixels, handed over without a PNG
    let mut retained = None;
    let mut kept = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        retained = od_render::Retained::render(s.document(), &frame, scale, None).unwrap();
        kept.push(t.elapsed());
    }
    let mut retained = retained.unwrap();
    let target = s
        .document()
        .node(&frame)
        .and_then(|f| f.children.get(5000).map(|c| c.id.clone()))
        .unwrap();
    let mut steps = Vec::new();
    for round in 0..7 {
        let position = s.log().len() as u64;
        let t = Instant::now();
        s.apply(
            Actor::User,
            None,
            &[Operation::Move {
                id: target.clone(),
                x: 10.0 + f64::from(round),
                y: 900.0,
            }],
        )
        .unwrap();
        let changed = s.touched_since(position);
        retained.update(s.document(), &changed, None).unwrap();
        steps.push(t.elapsed());
    }
    let mut copy = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        let bytes = retained.pixmap().data().to_vec();
        copy.push(t.elapsed());
        std::hint::black_box(bytes);
    }
    let median = |mut v: Vec<std::time::Duration>| {
        v.sort_unstable();
        v[v.len() / 2]
    };
    println!(
        "  old path: render {:.1}, png encode {:.1}, base64 {:.1}, Qt decode {:.1}, total {:.1}",
        ms(median(render.clone())),
        ms(median(encode.clone())),
        ms(median(base64.clone())),
        ms(median(decode.clone())),
        ms(median(render) + median(encode) + median(base64) + median(decode))
    );
    println!(
        "  new path: tiled draw {:.1}, one moved node {:.2}, hand the pixels over {:.1}",
        ms(median(kept)),
        ms(median(steps)),
        ms(median(copy))
    );
}
