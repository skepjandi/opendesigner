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

/// Redraws one moved node into the retained picture: the editor's cost after a drag step. The
/// first number is a full draw into tiles, the second the update after one `Move`.
fn incremental_timing(nodes: usize, scale: f32) -> (Duration, Duration) {
    let (mut s, frame) = build(nodes);
    let mut full = Vec::new();
    let mut retained = None;
    for _ in 0..7 {
        let t = Instant::now();
        retained = od_render::Retained::render(s.document(), &frame, scale, None)
            .unwrap_or_else(|e| panic!("{e}"));
        full.push(t.elapsed());
    }
    let mut retained = retained.unwrap_or_else(|| panic!("not retained"));
    let target = s
        .document()
        .node(&frame)
        .and_then(|f| f.children.get(nodes / 2).map(|c| c.id.clone()))
        .unwrap_or_else(|| panic!("no node"));
    let mut steps = Vec::new();
    for round in 0..7 {
        let position = s.log().len() as u64;
        let op = Operation::Move {
            id: target.clone(),
            x: 100.0 + f64::from(round) * 7.0,
            y: 900.0 + f64::from(round) * 3.0,
        };
        let t = Instant::now();
        s.apply(Actor::User, None, &[op])
            .unwrap_or_else(|e| panic!("{e}"));
        let changed = s.touched_since(position);
        retained
            .update(s.document(), &changed, None)
            .unwrap_or_else(|e| panic!("{e}"));
        steps.push(t.elapsed());
    }
    (median(full), median(steps))
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

/// A 1080x1920 frame of 300 plain shapes in which `count` are given `style`: what a scene that
/// turns, shadows or blurs its nodes costs against the same scene without.
fn effects_timing(count: usize, style: impl Fn(&mut NewNode)) -> Duration {
    let mut s = Session::new(Document::new("effects"));
    let page = s.add_page("p").unwrap_or_else(|e| panic!("{e}"));
    let root = s
        .document()
        .page(&page)
        .map(|p| p.root.id.clone())
        .unwrap_or_else(|| panic!("no page"));
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 1080.0;
    frame.height = 1920.0;
    frame.fill = Paint::parse("#0b1020").ok();
    for i in 0..300usize {
        let mut r = NewNode::new(NodeKind::Rect);
        (r.width, r.height) = (120.0, 80.0);
        r.x = (i % 8) as f64 * 130.0;
        r.y = (i / 8) as f64 * 50.0;
        r.corner_radius = Some(10.0);
        r.fill = Paint::parse("#f2ca83").ok();
        if i < count {
            style(&mut r);
        }
        frame.children.push(r);
    }
    let made = s
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
    let id = made
        .created
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("no frame"));
    let mut runs = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        let rendered =
            od_render::render_frame(s.document(), &id, 1.0, None).unwrap_or_else(|e| panic!("{e}"));
        runs.push(t.elapsed());
        std::hint::black_box(rendered.pixmap.data().len());
    }
    median(runs)
}

/// A 1080x1920 scene of `shapes` path nodes with gradient fills and dashed, trimmed strokes: the
/// load of a flat-vector explainer frame. Median render time at scale 1.
fn vector_timing(shapes: usize) -> Duration {
    let mut s = Session::new(Document::new("vector bench"));
    let page = s.add_page("p").unwrap_or_else(|e| panic!("{e}"));
    let root = s.document().page(&page).map(|p| p.root.id.clone());
    let root = root.unwrap_or_else(|| panic!("no page"));
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 1080.0;
    frame.height = 1920.0;
    frame.fill = Some(Paint::Linear {
        angle: 90.0,
        stops: vec![
            od_core::Stop {
                offset: 0.0,
                color: Paint::parse("#7ad0ff").unwrap_or_else(|e| panic!("{e}")),
            },
            od_core::Stop {
                offset: 1.0,
                color: Paint::parse("#e8f7ff").unwrap_or_else(|e| panic!("{e}")),
            },
        ],
    });
    for i in 0..shapes {
        let mut n = NewNode::new(NodeKind::Path);
        n.path = Some("M0 40 C30 0 60 80 90 40 S150 0 180 40 L180 90 Q90 130 0 90 Z".to_string());
        n.x = (i % 12) as f64 * 88.0;
        n.y = (i / 12) as f64 * 70.0;
        n.width = 120.0;
        n.height = 80.0;
        n.fill = Paint::parse("#f2ca83").ok();
        if i % 3 == 0 {
            let mut stroke = od_core::Stroke::new(
                Paint::parse("#223344").unwrap_or_else(|e| panic!("{e}")),
                3.0,
            );
            stroke.dash = vec![8.0, 4.0];
            stroke.trim_end = 0.8;
            n.stroke = Some(stroke);
        }
        frame.children.push(n);
    }
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
    let mut runs = Vec::new();
    for _ in 0..7 {
        let t = Instant::now();
        let rendered =
            od_render::render_frame(s.document(), &id, 1.0, None).unwrap_or_else(|e| panic!("{e}"));
        runs.push(t.elapsed());
        std::hint::black_box(rendered.pixmap.width());
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
        let (full, step) = incremental_timing(nodes, 0.3);
        println!(
            "{nodes:>6}  tiled draw at 0.3: {:.1} ms, one moved node (apply + update): {:.2} ms",
            ms(full),
            ms(step)
        );
        println!(
            "{nodes:>6}  100-operation transaction + checkpoint: {:.2} ms",
            ms(apply_timing(nodes))
        );
    }

    println!();
    println!("node effects, 1080x1920 at scale 1, 300 shapes, median of 7 runs (milliseconds)");
    let shadow = od_core::Shadow {
        dx: 0.0,
        dy: 10.0,
        blur: 16.0,
        color: Paint::Color(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 100,
        }),
    };
    type Style<'a> = &'a dyn Fn(&mut NewNode);
    let styles: [(&str, usize, Style); 4] = [
        ("none", 0, &|_| {}),
        ("rotated x300", 300, &|n| n.rotation = Some(17.0)),
        ("shadowed x30", 30, &|n| n.shadow = Some(shadow.clone())),
        ("blurred x30", 30, &|n| n.blur = Some(6.0)),
    ];
    for (name, count, style) in &styles {
        println!("{name:<16} {:>7.1}", ms(effects_timing(*count, style)));
    }
    println!();
    println!("vector scene, 1080x1920 at scale 1, median of 7 runs (milliseconds)");
    for shapes in [100usize, 300] {
        println!("{shapes:>6} paths: {:.1}", ms(vector_timing(shapes)));
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
