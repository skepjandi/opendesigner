//! Times the render path the editor's canvas uses, against the budgets in `docs/wiki/roadmap.md`.
//!
//! `cargo run --release -p opendesigner-render --example bench`
//!
//! It builds a store-slide frame (1320x2868) holding N shapes and text lines, renders it at the
//! scale the canvas uses for a window (about 0.3) and at export scale (1.0), and prints the median
//! of several runs for each stage: render and PNG encode (the editor then base64-encodes the PNG
//! into the data URL the QML `Image` takes, which is not timed here). Numbers are for the machine that ran them; they are not a pass/fail gate.

#![allow(clippy::panic, clippy::unwrap_used, clippy::expect_used)]

use std::time::{Duration, Instant};

use od_core::{
    Actor, Device3d, DeviceModel, Document, NewNode, NodeKind, Operation, Paint, Rgba, Session,
};
use od_render::three_d::{Scene, render_device, screen_texture_size};

fn build(nodes: usize) -> (Session, od_core::NodeId) {
    let mut s = Session::new(Document::new("bench"));
    let page = s.add_page("p").unwrap_or_else(|e| panic!("{e}"));
    let root = s.document().page(&page).map(|p| p.root.id.clone());
    let root = root.unwrap_or_else(|| panic!("no page"));
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 1320.0;
    frame.height = 2868.0;
    frame.fill = Paint::parse("#060b1c").ok();
    let mut children = Vec::new();
    for i in 0..nodes {
        let (col, row) = ((i % 20) as f64, (i / 20) as f64);
        let mut n = if i % 10 == 0 {
            let mut t = NewNode::new(NodeKind::Text);
            t.text = Some(format!("Headline {i}"));
            t.font_size = Some(28.0);
            t.fill = Paint::parse("#fcf1d9").ok();
            t.width = 200.0;
            t
        } else {
            let kind = if i % 3 == 0 {
                NodeKind::Ellipse
            } else {
                NodeKind::Rect
            };
            let mut r = NewNode::new(kind);
            r.width = 50.0;
            r.height = 40.0;
            if kind == NodeKind::Rect {
                r.corner_radius = Some(8.0);
            }
            r.fill = Paint::parse("#f2ca83").ok();
            r.opacity = Some(0.9);
            r
        };
        n.x = (col * 62.0).rem_euclid(1260.0);
        n.y = (row * 50.0).rem_euclid(2800.0);
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
        .unwrap_or_else(|e| panic!("{e}"));
    let id = receipt
        .created
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("no frame"));
    (s, id)
}

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Applies a 100-operation transaction and a checkpoint to a document of `nodes` nodes.
fn apply_timing(nodes: usize) -> Duration {
    let (mut s, frame) = build(nodes);
    let kids: Vec<od_core::NodeId> = s
        .document()
        .node(&frame)
        .map(|f| f.children.iter().map(|c| c.id.clone()).collect())
        .unwrap_or_default();
    let mut runs = Vec::new();
    for round in 0..7 {
        let ops: Vec<Operation> = kids
            .iter()
            .rev()
            .take(100)
            .map(|id| Operation::Move {
                id: id.clone(),
                x: f64::from(round) + 1.0,
                y: 2.0,
            })
            .collect();
        let t = Instant::now();
        s.apply(Actor::User, None, &ops)
            .unwrap_or_else(|e| panic!("{e}"));
        s.checkpoint(Actor::User, "bench")
            .unwrap_or_else(|e| panic!("{e}"));
        runs.push(t.elapsed());
    }
    median(runs)
}

/// A 3D device: a phone turned a little, with a capture on its screen.
fn device_timing(model: DeviceModel, w: u32, h: u32) -> Duration {
    let mut device = Device3d {
        model,
        rotate_x: 8.0,
        rotate_y: -22.0,
        ..Device3d::default()
    };
    device.shadow = 0.35;
    let (tw, th) = screen_texture_size(&device, w, h);
    let mut screen = tiny_skia::Pixmap::new(tw, th).expect("screen picture");
    {
        let data = screen.data_mut();
        for (i, px) in data.chunks_exact_mut(4).enumerate() {
            let x = i as u32 % tw;
            let y = i as u32 / tw;
            px[0] = (x * 255 / tw.max(1)) as u8;
            px[1] = (y * 255 / th.max(1)) as u8;
            px[2] = 128;
            px[3] = 255;
        }
    }
    let scene = Scene {
        device: &device,
        body: Rgba {
            r: 58,
            g: 61,
            b: 68,
            a: 255,
        },
        screen: Some(&screen),
        width: w,
        height: h,
    };
    let mut runs = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        let pix = render_device(&scene).expect("device picture");
        runs.push(t.elapsed());
        std::hint::black_box(pix.data().len());
    }
    median(runs)
}

fn main() {
    println!("render timings, median of 7 runs (milliseconds)");
    println!("nodes   scale   render   png    total");
    for nodes in [10usize, 100, 1000, 10_000] {
        let (s, frame) = build(nodes);
        for scale in [0.3f32, 1.0] {
            let (mut r, mut p) = (Vec::new(), Vec::new());
            for _ in 0..7 {
                let t = Instant::now();
                let rendered = od_render::render_frame(s.document(), &frame, scale, None)
                    .unwrap_or_else(|e| panic!("{e}"));
                r.push(t.elapsed());
                let t = Instant::now();
                let png = od_render::encode_png(&rendered.pixmap, false)
                    .unwrap_or_else(|e| panic!("{e}"));
                p.push(t.elapsed());
                std::hint::black_box(png.len());
            }
            let (r, p) = (median(r), median(p));
            println!(
                "{nodes:>6}  {scale:>5}  {:>7.1}  {:>5.1}  {:>6.1}",
                ms(r),
                ms(p),
                ms(r + p)
            );
        }
        println!(
            "{nodes:>6}  100-operation transaction + checkpoint: {:.2} ms",
            ms(apply_timing(nodes))
        );
    }

    println!();
    println!("3D device timings, median of 7 runs (milliseconds)");
    println!("model          size         render   budget");
    for (model, w, h, budget) in [
        (DeviceModel::Phone, 1206u32, 2622u32, 250.0),
        (DeviceModel::Phone, 360, 640, 15.0),
        (DeviceModel::Tablet, 1206, 1608, 250.0),
        (DeviceModel::Laptop, 1920, 1200, 250.0),
        (DeviceModel::Monitor, 1920, 1080, 250.0),
    ] {
        let t = device_timing(model, w, h);
        println!(
            "{:<13}  {:>4}x{:<4}  {:>7.1}  {:>6.0}",
            model.as_str(),
            w,
            h,
            ms(t),
            budget
        );
    }
}
