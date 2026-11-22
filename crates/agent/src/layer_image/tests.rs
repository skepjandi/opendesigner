#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, Operation, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::tools::{CallCtx, Content, Turns};

struct Fixture {
    _dir: TempDir,
    ws: Workspace,
    frame: String,
}

fn write_png(path: &std::path::Path, w: u32, h: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) {
    let mut data = Vec::new();
    for y in 0..h {
        for x in 0..w {
            data.extend(pixel(x, y));
        }
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    std::fs::write(path, out).unwrap();
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Layers").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let frame = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: serde_json::from_value(
                    json!({"kind": "frame", "name": "Scene", "width": 160, "height": 90}),
                )
                .unwrap(),
            }],
        )
        .unwrap()
        .created[0]
        .to_string();
    let media = ws.dir().unwrap().join("media");
    std::fs::create_dir_all(&media).unwrap();
    write_png(&media.join("photo.png"), 16, 9, |x, _| {
        [(x * 15) as u8, 80, 120, 255]
    });
    write_png(&media.join("depth.png"), 16, 9, |x, _| {
        let v = (x * 16) as u8;
        [v, v, v, 255]
    });
    write_png(&media.join("mask.png"), 16, 9, |x, _| {
        let v = if x >= 8 { 255 } else { 0 };
        [v, v, v, 255]
    });
    Fixture {
        _dir: dir,
        ws,
        frame,
    }
}

fn call(f: &mut Fixture, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws: &mut f.ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, "layer_image", args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

#[test]
fn a_depth_map_becomes_layers_with_rising_depth_in_one_group() {
    let mut f = fixture();
    let frame = f.frame.clone();
    let out = call(
        &mut f,
        json!({"file": "media/photo.png", "depth": "media/depth.png", "bands": 3, "parent": frame}),
    )
    .unwrap();
    assert_eq!(out["bands"], 3);
    let layers = out["layers"].as_array().unwrap();
    assert_eq!(layers.len(), 3);
    let depths: Vec<f64> = out["depths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d.as_f64().unwrap())
        .collect();
    assert!((depths[0] - 0.15).abs() < 1e-9 && (depths[2] - 1.0).abs() < 1e-9);
    let media = f.ws.dir().unwrap().join("media");
    for l in layers {
        assert!(
            media
                .join(l.as_str().unwrap().strip_prefix("media/").unwrap())
                .exists()
        );
    }
    let sizes: Vec<u64> = out["band_pixels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap())
        .collect();
    assert_eq!(sizes.iter().sum::<u64>(), 16 * 9);
    let doc = f.ws.session.document();
    let id = od_core::NodeId::parse(out["created"][0].as_str().unwrap()).unwrap();
    let group = doc.node(&id).unwrap();
    assert_eq!(group.children.len(), 3);
    assert!(group.children[0].depth < group.children[2].depth);
    assert!(
        group.children[0].width > 160.0,
        "overscanned past the frame"
    );
}

#[test]
fn a_mask_makes_two_layers_and_the_lowest_fills_the_picture() {
    let mut f = fixture();
    let frame = f.frame.clone();
    let out = call(
        &mut f,
        json!({"file": "media/photo.png", "mask": "media/mask.png", "parent": frame}),
    )
    .unwrap();
    assert_eq!(out["bands"], 2);
    let media = f.ws.dir().unwrap().join("media");
    let (w, h, back) = od_render::decode_rgba(&media.join("photo-layer-0.png"), 1 << 20).unwrap();
    assert_eq!((w, h), (16, 9));
    assert!(
        back.chunks(4).all(|p| p[3] == 255),
        "no hole in the background layer"
    );
    let (_, _, front) = od_render::decode_rgba(&media.join("photo-layer-1.png"), 1 << 20).unwrap();
    assert_eq!(
        front[3], 0,
        "the subject layer is empty where the background shows"
    );
    assert_eq!(front[(12 * 4) + 3], 255, "solid inside the subject");
    assert!(front[(8 * 4) + 3] > 0, "the edge is only softened");
}

#[test]
fn a_second_run_never_overwrites_and_one_level_is_refused() {
    let mut f = fixture();
    let frame = f.frame.clone();
    let a = call(
        &mut f,
        json!({"file": "media/photo.png", "mask": "media/mask.png", "parent": frame.clone()}),
    )
    .unwrap();
    let b = call(
        &mut f,
        json!({"file": "media/photo.png", "mask": "media/mask.png", "parent": frame.clone()}),
    )
    .unwrap();
    assert_ne!(a["layers"], b["layers"]);
    let media = f.ws.dir().unwrap().join("media");
    write_png(&media.join("flat.png"), 16, 9, |_, _| [90, 90, 90, 255]);
    let err = call(
        &mut f,
        json!({"file": "media/photo.png", "depth": "media/flat.png", "parent": frame}),
    )
    .unwrap_err();
    assert!(err.contains("only one level"), "{err}");
}

#[test]
fn it_needs_exactly_one_plane_and_plain_media_names() {
    let mut f = fixture();
    let frame = f.frame.clone();
    let none = call(
        &mut f,
        json!({"file": "media/photo.png", "parent": frame.clone()}),
    )
    .unwrap_err();
    assert!(none.contains("exactly one"));
    let both = call(&mut f, json!({"file": "media/photo.png", "mask": "media/mask.png", "depth": "media/depth.png", "parent": frame.clone()})).unwrap_err();
    assert!(both.contains("exactly one"));
    let bad = call(
        &mut f,
        json!({"file": "../photo.png", "mask": "media/mask.png", "parent": frame}),
    )
    .unwrap_err();
    assert!(bad.contains("media/<file>"));
}

#[test]
fn the_group_is_one_undoable_step() {
    let mut f = fixture();
    let frame = f.frame.clone();
    let before = f.ws.session.document().clone();
    call(
        &mut f,
        json!({"file": "media/photo.png", "mask": "media/mask.png", "parent": frame}),
    )
    .unwrap();
    assert_ne!(&before, f.ws.session.document());
    f.ws.session.undo(Actor::User).unwrap();
    assert_eq!(before.pages, f.ws.session.document().pages);
}
