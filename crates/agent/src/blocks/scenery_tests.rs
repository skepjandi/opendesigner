#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::Repeat;
use od_core::{Actor, AnimProp, Document, NodeId, NodeKind, Operation, Property, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;
use crate::tools::{CallCtx, Content, Turns};

/// Scenery that reaches past the frame on purpose, so a camera move or a loop never shows an edge.
pub(super) const BLEEDS: &[&str] = &[
    "sky_gradient",
    "sun_glow",
    "clouds",
    "parallax_hills",
    "ocean_waves",
    "city_skyline",
    "cells_backdrop",
    "particles",
];

struct Scene {
    _dir: TempDir,
    ws: Workspace,
    frame: NodeId,
    video: NodeId,
}

fn apply(ws: &mut Workspace, ops: Vec<Operation>) -> od_core::TxReceipt {
    ws.session.apply(Actor::User, None, &ops).unwrap()
}

fn scene(w: u32, h: u32) -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Scenery").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let created = apply(
        &mut ws,
        vec![Operation::Create {
            parent: root,
            index: None,
            node: serde_json::from_value(json!({
                "kind": "frame", "name": "Scene", "width": w, "height": h, "fill": "#0d0f14"
            }))
            .unwrap(),
        }],
    );
    let frame = created.created[0].clone();
    apply(
        &mut ws,
        vec![Operation::CreateVideo {
            name: "Cut".into(),
            fps: 30,
            width: w,
            height: h,
        }],
    );
    let video = ws.session.document().videos[0].id.clone();
    Scene {
        _dir: dir,
        ws,
        frame,
        video,
    }
}

impl Scene {
    fn doc(&self) -> &Document {
        self.ws.session.document()
    }

    fn request(&self, block: &str, params: Option<Value>) -> Request {
        Request {
            block: block.into(),
            frame: Some(self.frame.to_string()),
            video: Some(self.video.to_string()),
            params: params.map(|p| p.as_object().cloned().unwrap_or_default()),
            ..Request::default()
        }
    }

    fn add(&mut self, block: &str, params: Option<Value>) -> Plan {
        let req = self.request(block, params);
        let p = plan(self.doc(), &req).unwrap_or_else(|e| panic!("{block}: {e}"));
        apply(&mut self.ws, p.ops.clone());
        p
    }

    /// The frame at `ms` of the video, camera included.
    fn shot(&self, ms: f64, longest: f64) -> od_render::Pixmap {
        let doc = self.doc();
        let video = doc.video(&self.video).unwrap();
        let at = doc.at_time(video, ms).unwrap_or_else(|| doc.clone());
        let frame = at.node(&self.frame).unwrap();
        let scale = (longest / frame.width.max(frame.height)) as f32;
        od_render::render_frame_camera(
            &at,
            &self.frame,
            scale,
            None,
            &mut od_render::Media::default(),
            video.camera_at(ms),
        )
        .unwrap()
        .pixmap
    }
}

const IDS: [&str; 12] = [
    "sky_gradient",
    "parallax_hills",
    "starfield",
    "clouds",
    "sun_glow",
    "ocean_waves",
    "city_skyline",
    "cells_backdrop",
    "particles",
    "grid_floor",
    "connector",
    "highlight_ring",
];

fn call_tool(ws: &mut Workspace, name: &str, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, name, args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

fn same(a: &od_render::Pixmap, b: &od_render::Pixmap) -> bool {
    a.data() == b.data()
}

fn changed_pixels(a: &od_render::Pixmap, b: &od_render::Pixmap) -> usize {
    a.data()
        .chunks(4)
        .zip(b.data().chunks(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 3))
        .count()
}

fn tracks(f: &Scene, plan: &Plan) -> Vec<od_core::Animation> {
    let root = f.doc().node(&plan.parts[0].1).unwrap();
    let mut ids = Vec::new();
    root.ids(&mut ids);
    f.doc()
        .video(&f.video)
        .unwrap()
        .animations
        .iter()
        .filter(|a| ids.contains(&a.node))
        .cloned()
        .collect()
}

#[test]
fn the_catalog_lists_every_scenery_block_and_search_finds_them() {
    for id in IDS {
        assert!(find(id).is_ok(), "{id} is missing");
    }
    let found = search("parallax", None);
    assert_eq!(found.first().map(|b| b.id), Some("parallax_hills"));
    assert!(
        search("confetti bubbles", None)
            .iter()
            .any(|b| b.id == "particles")
    );
}

#[test]
fn node_counts_stay_bounded() {
    for (w, h) in [(1290, 2796), (1920, 1080), (1080, 1080)] {
        for id in IDS {
            let mut s = scene(w, h);
            let p = s.add(id, None);
            assert!(p.node_count <= 400, "{id} made {} nodes", p.node_count);
        }
        let mut s = scene(w, h);
        let p = s.add(
            "starfield",
            Some(json!({"count": 220, "shooting": 3, "nebula": 3})),
        );
        assert!(p.node_count <= 300, "starfield {}", p.node_count);
        let mut s = scene(w, h);
        let p = s.add("particles", Some(json!({"count": 70})));
        assert!(p.node_count <= 150, "particles {}", p.node_count);
    }
}

#[test]
fn every_scenery_block_draws_something_and_moves() {
    for id in IDS {
        let mut s = scene(1280, 720);
        let blank = s.shot(0.0, 480.0);
        s.add(id, None);
        let first = s.shot(2_250.0, 480.0);
        let later = s.shot(6_750.0, 480.0);
        assert!(changed_pixels(&blank, &first) > 200, "{id} draws nothing");
        assert!(changed_pixels(&first, &later) > 20, "{id} does not move");
    }
}

#[test]
fn every_scenery_block_runs_for_the_whole_video_with_loops() {
    for id in IDS {
        let mut s = scene(1920, 1080);
        let p = s.add(id, None);
        let all = tracks(&s, &p);
        assert!(!all.is_empty(), "{id} has no motion");
        for t in &all {
            assert!(
                t.keys.windows(2).all(|w| w[0].at_ms < w[1].at_ms),
                "{id} {:?}",
                t.prop
            );
        }
        let looping = all.iter().filter(|t| t.repeat == Repeat::Loop).count();
        assert!(looping > 0, "{id} has no ambient loop");
    }
}

#[test]
fn loop_tracks_end_where_they_begin_or_are_straight_runs() {
    for id in IDS {
        let mut s = scene(1920, 1080);
        let p = s.add(id, None);
        for t in tracks(&s, &p).iter().filter(|t| t.repeat == Repeat::Loop) {
            let (first, last) = (t.keys[0].value, t.keys[t.keys.len() - 1].value);
            let seamless = (first - last).abs() < 1e-6;
            let ramp = t.keys.len() == 2;
            let turn = t.prop == AnimProp::Rotation && ((last - first) % 360.0).abs() < 1e-6;
            assert!(
                seamless || ramp,
                "{id} {:?} jumps from {last} to {first}",
                t.prop
            );
            if ramp && !seamless {
                let scroll = matches!(t.prop, AnimProp::X | AnimProp::Y | AnimProp::Rotation);
                assert!(scroll || turn, "{id} {:?} is a one-way loop", t.prop);
            }
        }
    }
}

#[test]
fn scenery_sits_in_depth_order_whatever_order_it_is_added_in() {
    let mut s = scene(1920, 1080);
    for id in [
        "particles",
        "parallax_hills",
        "starfield",
        "sky_gradient",
        "clouds",
    ] {
        s.add(id, None);
    }
    let frame = s.doc().node(&s.frame).unwrap();
    let order: Vec<&str> = frame
        .children
        .iter()
        .map(|c| c.name.split(['#', ':']).nth(1).unwrap())
        .collect();
    assert_eq!(
        order,
        [
            "sky_gradient",
            "starfield",
            "clouds",
            "parallax_hills",
            "particles"
        ]
    );
    s.add("title_card", None);
    let last = s
        .doc()
        .node(&s.frame)
        .unwrap()
        .children
        .last()
        .unwrap()
        .name
        .clone();
    assert!(
        last.starts_with("block:title_card"),
        "content stays in front: {last}"
    );
}

#[test]
fn hill_layers_are_ordered_far_to_near_with_rising_depth() {
    let mut s = scene(1920, 1080);
    let p = s.add(
        "parallax_hills",
        Some(json!({"layers": 5, "min_depth": 0.2, "max_depth": 1.1})),
    );
    let root = s.doc().node(&p.parts[0].1).unwrap();
    let layers: Vec<_> = root
        .children
        .iter()
        .filter(|c| c.name.contains("/layer-"))
        .collect();
    assert_eq!(layers.len(), 5);
    let depths: Vec<f64> = layers.iter().map(|l| l.depth).collect();
    assert!(depths.windows(2).all(|w| w[0] < w[1]), "{depths:?}");
    assert!((depths[0] - 0.2).abs() < 1e-9 && (depths[4] - 1.1).abs() < 1e-9);
    for (i, l) in layers.iter().enumerate() {
        assert!(
            l.name.ends_with(&format!("layer-{}", i + 1)),
            "stacked far to near"
        );
        assert_eq!(l.children[0].kind, NodeKind::Path);
    }
    let tops: Vec<f64> = layers.iter().map(|l| l.children[0].y).collect();
    assert!(
        tops.windows(2).all(|w| w[0] <= w[1] + 1.0),
        "nearer layers sit lower: {tops:?}"
    );
}

fn column_tops(pm: &od_render::Pixmap) -> Vec<i32> {
    let (w, h) = (pm.width() as usize, pm.height() as usize);
    let bg = &pm.data()[0..4];
    (0..w)
        .map(|x| {
            (0..h)
                .find(|y| {
                    let i = (y * w + x) * 4;
                    pm.data()[i..i + 4]
                        .iter()
                        .zip(bg)
                        .any(|(p, q)| p.abs_diff(*q) > 6)
                })
                .map_or(h as i32, |y| y as i32)
        })
        .collect()
}

/// How far the ridge `after` has moved from `before`, in columns, among shifts `-reach..=0`: the
/// shift that makes the two outlines agree best.
fn ridge_shift(before: &[i32], after: &[i32], reach: i32) -> i32 {
    let cost = |k: i32| -> i64 {
        (130..before.len() as i32 - 130)
            .map(|x| i64::from((before[x as usize] - after[(x + k) as usize]).abs()))
            .sum()
    };
    (-reach..=0).min_by_key(|k| cost(*k)).unwrap()
}

#[test]
fn a_camera_pan_moves_each_hill_layer_by_its_depth() {
    let mut s = scene(1920, 1080);
    let p = s.add(
        "parallax_hills",
        Some(json!({"layers": 3, "min_depth": 0.2, "max_depth": 1.0, "shadow": false, "sway": false, "haze": 0.0})),
    );
    let video = s.video.to_string();
    call_tool(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "preset": "camera_pan", "direction": "right", "amount": 160, "start_ms": 1500, "duration_ms": 1000}),
    )
    .unwrap();
    let ids: Vec<NodeId> = p
        .parts
        .iter()
        .filter(|(n, _)| n.starts_with("layer-"))
        .map(|(_, id)| id.clone())
        .collect();
    assert_eq!(ids.len(), 3);
    let mut shifts = Vec::new();
    for keep in 0..3 {
        let hide: Vec<Operation> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| Operation::Set {
                id: id.clone(),
                prop: Property::Visible(i == keep),
            })
            .collect();
        apply(&mut s.ws, hide);
        let (rest, panned) = (
            column_tops(&s.shot(1_500.0, 960.0)),
            column_tops(&s.shot(2_500.0, 960.0)),
        );
        shifts.push(f64::from(ridge_shift(&rest, &panned, 100)));
    }
    let want = [-0.2 * 160.0 / 2.0, -0.6 * 160.0 / 2.0, -160.0 / 2.0];
    for (got, want) in shifts.iter().zip(want) {
        assert!(
            (got - want).abs() <= 4.0,
            "shifts {shifts:?}, wanted about {want}"
        );
    }
    assert!(
        shifts[0] > shifts[1] && shifts[1] > shifts[2],
        "nearer layers move farther: {shifts:?}"
    );
}

#[test]
fn a_camera_push_in_grows_near_layers_more_than_far_ones() {
    let mut s = scene(1280, 720);
    s.add(
        "parallax_hills",
        Some(json!({"layers": 3, "shadow": false, "sway": false})),
    );
    let video = s.video.to_string();
    call_tool(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "preset": "camera_push_in", "start_ms": 1500, "duration_ms": 1000, "intensity": 2}),
    )
    .unwrap();
    assert!(s.doc().video(&s.video).unwrap().has_camera());
    assert!(changed_pixels(&s.shot(1_500.0, 480.0), &s.shot(2_500.0, 480.0)) > 1_000);
}

fn pixel_equal_after(blocks: &[(&str, Value)], at: f64, later: f64, label: &str) {
    let mut s = scene(1280, 720);
    for (id, params) in blocks {
        s.add(id, Some(params.clone()));
    }
    let (a, b) = (s.shot(at, 480.0), s.shot(later, 480.0));
    assert!(
        same(&a, &b),
        "{label}: the picture at {at} ms and at {later} ms differ in {} pixels",
        changed_pixels(&a, &b)
    );
    assert!(
        changed_pixels(&a, &s.shot(at + 1_300.0, 480.0)) > 20,
        "{label} must move in between"
    );
}

#[test]
fn scrolling_loops_tile_without_a_seam() {
    pixel_equal_after(
        &[("clouds", json!({"layers": 1, "loop_ms": 20_000}))],
        3_000.0,
        83_000.0,
        "clouds",
    );
    pixel_equal_after(
        &[("ocean_waves", json!({"loop_ms": 4_000}))],
        2_000.0,
        18_000.0,
        "ocean",
    );
    pixel_equal_after(
        &[("grid_floor", json!({"loop_ms": 4_000}))],
        2_000.0,
        6_000.0,
        "grid",
    );
}

#[test]
fn particles_repeat_exactly_every_loop() {
    for kind in ["bubbles", "dust", "confetti", "snow", "embers", "sparkles"] {
        pixel_equal_after(
            &[("particles", json!({"kind": kind, "loop_ms": 8_000}))],
            1_500.0,
            9_500.0,
            kind,
        );
    }
}

#[test]
fn the_connector_and_ring_loops_repeat_after_they_are_drawn() {
    pixel_equal_after(
        &[
            ("connector", json!({"label": "Flow"})),
            ("highlight_ring", json!({})),
        ],
        4_000.0,
        10_000.0,
        "guides",
    );
}

#[test]
fn a_box_smaller_than_the_frame_clips_scrolling_scenery() {
    for id in [
        "clouds",
        "ocean_waves",
        "parallax_hills",
        "particles",
        "city_skyline",
        "sun_glow",
        "cells_backdrop",
    ] {
        let mut s = scene(1280, 720);
        let blank = s.shot(0.0, 640.0);
        let mut req = s.request(id, None);
        req.region = Some(BoxArgs {
            x: Some(320.0),
            y: Some(180.0),
            width: Some(640.0),
            height: Some(360.0),
        });
        let p = plan(s.doc(), &req).unwrap();
        apply(&mut s.ws, p.ops);
        let shot = s.shot(2_500.0, 640.0);
        let (w, h) = (shot.width() as usize, shot.height() as usize);
        let outside = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .filter(|(x, y)| !(160..480).contains(x) || !(90..270).contains(y))
            .filter(|(x, y)| {
                let i = (y * w + x) * 4;
                shot.data()[i..i + 4] != blank.data()[i..i + 4]
            })
            .count();
        assert_eq!(outside, 0, "{id} paints {outside} pixels outside its box");
        assert!(
            changed_pixels(&blank, &shot) > 100,
            "{id} paints nothing inside it"
        );
    }
}

#[test]
fn colors_follow_the_documents_tokens() {
    let mut s = scene(1280, 720);
    apply(
        &mut s.ws,
        vec![
            Operation::SetToken {
                name: "brand.background".into(),
                value: Some("#103020".into()),
            },
            Operation::SetToken {
                name: "brand.accent".into(),
                value: Some("#ff8800".into()),
            },
        ],
    );
    s.add("sky_gradient", None);
    let shot = s.shot(2_000.0, 320.0);
    let top = &shot.data()[0..3];
    assert!(
        top[0].abs_diff(0x10) <= 3 && top[1].abs_diff(0x30) <= 3 && top[2].abs_diff(0x20) <= 3,
        "{top:?}"
    );
    let low = &shot.data()[(shot.height() as usize - 1) * shot.width() as usize * 4..][..3];
    assert!(
        low[0] > low[2],
        "the horizon takes the warm accent: {low:?}"
    );
    assert!(
        !s.doc().tokens.contains_key("block.background"),
        "no token is invented when the brand has one"
    );
}

#[test]
fn starfield_has_three_sizes_desynchronized_twinkles_and_timed_shooting_stars() {
    let mut s = scene(1920, 1080);
    let p = s.add(
        "starfield",
        Some(json!({"count": 200, "shooting": 2, "nebula": 2})),
    );
    let doc = s.doc();
    let root = doc.node(&p.parts[0].1).unwrap();
    let mut sizes: Vec<u32> = Vec::new();
    for g in root.children.iter().filter(|c| c.name.contains("/stars-")) {
        for st in &g.children {
            sizes.push(((st.width * 10.0).round() as u32 / 12) * 12);
        }
    }
    let small = sizes.iter().copied().filter(|w| *w < 30).count();
    let big = sizes.iter().copied().filter(|w| *w >= 60).count();
    assert!(small > 80 && big > 5, "sizes {small} small, {big} large");
    let twinkles: Vec<_> = tracks(&s, &p)
        .into_iter()
        .filter(|t| t.prop == AnimProp::Opacity && t.repeat == Repeat::Loop && t.keys.len() > 10)
        .collect();
    assert!(twinkles.len() >= 7);
    let mut firsts: Vec<i64> = twinkles
        .iter()
        .map(|t| (t.keys[0].value * 1000.0) as i64)
        .collect();
    let mut spans: Vec<u64> = twinkles
        .iter()
        .map(|t| t.keys[t.keys.len() - 1].at_ms)
        .collect();
    firsts.sort_unstable();
    firsts.dedup();
    spans.sort_unstable();
    spans.dedup();
    assert!(
        firsts.len() >= 5 && spans.len() >= 5,
        "twinkles are out of step: {firsts:?} {spans:?}"
    );
    let trims: Vec<_> = tracks(&s, &p)
        .into_iter()
        .filter(|t| t.prop == AnimProp::TrimEnd)
        .collect();
    assert_eq!(trims.len(), 2);
    for t in trims {
        assert!(t.keys.iter().all(|k| (0.0..=1.0).contains(&k.value)));
        assert_eq!(t.keys[0].value, 0.0);
        assert!(t.keys.iter().any(|k| k.value == 1.0), "a streak is drawn");
    }
}

fn stroke_pixels(pm: &od_render::Pixmap) -> usize {
    pm.data()
        .chunks(4)
        .filter(|p| p[0] > 200 && p[1] < 80 && p[2] < 80)
        .count()
}

#[test]
fn draw_on_draws_the_stroke_along_the_path() {
    let mut s = scene(1280, 720);
    let created = apply(
        &mut s.ws,
        vec![Operation::Create {
            parent: s.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({
                "kind": "path", "name": "curve", "x": 100, "y": 100, "width": 1000, "height": 400,
                "path": "M0,400 C300,0 700,0 1000,400",
                "stroke": {"paint": "#ff0000", "width": 12, "cap": "round"}
            }))
            .unwrap(),
        }],
    );
    let node = created.created[0].to_string();
    let video = s.video.to_string();
    call_tool(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "node": node, "preset": "draw_on", "start_ms": 500, "duration_ms": 2000, "ease": "linear"}),
    )
    .unwrap();
    let counts: Vec<usize> = [0.0, 500.0, 1_000.0, 1_500.0, 2_000.0, 2_500.0, 4_000.0]
        .iter()
        .map(|t| stroke_pixels(&s.shot(*t, 640.0)))
        .collect();
    assert_eq!(counts[0], 0);
    assert_eq!(counts[1], 0, "nothing is drawn before it starts");
    assert!(
        counts.windows(2).skip(1).take(4).all(|w| w[1] > w[0]),
        "it keeps growing: {counts:?}"
    );
    assert_eq!(counts[5], counts[6], "and stops when drawn");
    let share = counts[3] as f64 / counts[5] as f64;
    assert!(
        (share - 0.5).abs() < 0.12,
        "half way is half drawn: {share}"
    );
}

#[test]
fn animate_along_path_hits_the_path_ends_and_faces_along_it() {
    let mut s = scene(1280, 720);
    let made = apply(
        &mut s.ws,
        vec![
            Operation::Create {
                parent: s.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({
                    "kind": "path", "name": "road", "x": 200, "y": 150, "width": 800, "height": 300,
                    "path": "M0,300 C200,0 600,0 800,300",
                    "stroke": {"paint": "#ffffff", "width": 4}
                }))
                .unwrap(),
            },
            Operation::Create {
                parent: s.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({
                    "kind": "rect", "name": "car", "x": 10, "y": 10, "width": 60, "height": 30, "fill": "#00ff00"
                }))
                .unwrap(),
            },
        ],
    );
    let (road, car) = (made.created[0].to_string(), made.created[1].clone());
    let video = s.video.to_string();
    call_tool(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "node": car.to_string(), "preset": "animate_along_path", "path": road,
               "start_ms": 1000, "duration_ms": 3000, "orient": true}),
    )
    .unwrap();
    let at = |ms: f64| {
        let v = s.doc().video(&s.video).unwrap();
        let d = s.doc().at_time(v, ms).unwrap();
        let n = d.node(&car).unwrap().clone();
        (n.x + n.width / 2.0, n.y + n.height / 2.0, n.rotation)
    };
    let (x0, y0, r0) = at(1_000.0);
    assert!(
        (x0 - 200.0).abs() < 1.5 && (y0 - 450.0).abs() < 1.5,
        "starts on the path: {x0},{y0}"
    );
    let (x1, y1, r1) = at(4_000.0);
    assert!(
        (x1 - 1000.0).abs() < 1.5 && (y1 - 450.0).abs() < 1.5,
        "ends on it: {x1},{y1}"
    );
    assert!(
        r0 < -40.0 && r1 > 40.0,
        "faces up the climb then down the descent: {r0} {r1}"
    );
    let (xm, ym, _) = at(2_500.0);
    assert!(
        (xm - 600.0).abs() < 25.0 && ym < 330.0,
        "and crests in the middle: {xm},{ym}"
    );
}

#[test]
fn every_preset_applies_through_the_tool_and_leaves_a_valid_document() {
    let mut s = scene(1920, 1080);
    let made = apply(
        &mut s.ws,
        vec![Operation::Create {
            parent: s.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({
                "kind": "rect", "name": "box", "x": 800, "y": 400, "width": 240, "height": 160, "fill": "#ff00aa",
                "stroke": {"paint": "#ffffff", "width": 4}
            }))
            .unwrap(),
        }],
    );
    let node = made.created[0].to_string();
    let video = s.video.to_string();
    for name in crate::scene_motion::Named::NAMES {
        let mut args = json!({"video": video, "preset": name, "start_ms": 500});
        if !name.starts_with("camera") {
            args["node"] = json!(node);
        }
        let before = s.ws.session.log().len();
        call_tool(&mut s.ws, "animate_preset", args).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(s.ws.session.log().len(), before + 1, "{name} is one step");
        s.doc().validate().unwrap();
    }
    let v = s.doc().video(&s.video).unwrap();
    assert!(v.has_camera());
    for a in &v.animations {
        assert!(a.keys.windows(2).all(|w| w[0].at_ms < w[1].at_ms));
    }
}

#[test]
fn a_preset_is_undone_in_one_step() {
    let mut s = scene(1280, 720);
    let made = apply(
        &mut s.ws,
        vec![Operation::Create {
            parent: s.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({"kind": "rect", "x": 100, "y": 100, "width": 100, "height": 100, "fill": "#ffffff"})).unwrap(),
        }],
    );
    let before = s.doc().videos.clone();
    let (node, video) = (made.created[0].to_string(), s.video.to_string());
    call_tool(
        &mut s.ws,
        "animate_preset",
        json!({"video": video, "node": node, "preset": "pop_in", "start_ms": 0}),
    )
    .unwrap();
    assert_ne!(s.doc().videos, before);
    s.ws.session.undo(Actor::User).unwrap();
    assert_eq!(s.doc().videos, before);
}

#[test]
fn pop_in_overshoots_then_settles_on_the_node() {
    let mut s = scene(1280, 720);
    let made = apply(
        &mut s.ws,
        vec![Operation::Create {
            parent: s.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({"kind": "rect", "x": 100, "y": 100, "width": 100, "height": 100, "fill": "#ffffff"})).unwrap(),
        }],
    );
    let (id, video) = (made.created[0].clone(), s.video.to_string());
    call_tool(&mut s.ws, "animate_preset", json!({"video": video, "node": id.to_string(), "preset": "pop_in", "start_ms": 200, "duration_ms": 500})).unwrap();
    let scale = |ms: f64| {
        let v = s.doc().video(&s.video).unwrap();
        s.doc().at_time(v, ms).unwrap().node(&id).unwrap().scale_x
    };
    assert!(scale(200.0) < 0.01);
    let peak = (200..=700)
        .map(|ms| scale(f64::from(ms)))
        .fold(0.0_f64, f64::max);
    assert!(peak > 1.05, "overshoots: {peak}");
    assert!(
        (scale(700.0) - 1.0).abs() < 1e-6 && (scale(5_000.0) - 1.0).abs() < 1e-6,
        "settles"
    );
}

#[test]
fn stagger_offsets_each_node_by_its_step_from_the_chosen_end() {
    let mut s = scene(1280, 720);
    let ops: Vec<Operation> = (0..4)
        .map(|i| Operation::Create {
            parent: s.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({"kind": "rect", "x": 100 + i * 150, "y": 100, "width": 100, "height": 100, "fill": "#ffffff"})).unwrap(),
        })
        .collect();
    let made = apply(&mut s.ws, ops);
    let ids: Vec<String> = made.created.iter().map(ToString::to_string).collect();
    let video = s.video.to_string();
    for (from, order) in [("first", [0, 1, 2, 3]), ("last", [3, 2, 1, 0])] {
        call_tool(
            &mut s.ws,
            "animate_preset",
            json!({"video": video, "preset": "stagger", "of": "fade_up", "nodes": ids, "start_ms": 1000, "duration_ms": 400, "step_ms": 200, "from": from}),
        )
        .unwrap();
        let v = s.doc().video(&s.video).unwrap();
        for (i, id) in made.created.iter().enumerate() {
            let a = v
                .animations
                .iter()
                .find(|a| &a.node == id && a.prop == AnimProp::Opacity)
                .unwrap();
            let want = 1000 + 200 * order[i] as u64;
            assert!(
                a.keys.iter().any(|k| k.at_ms == want),
                "{from} node {i} starts at {want}: {:?}",
                a.keys
            );
        }
    }
    let mut random = |seed: u64| {
        call_tool(
            &mut s.ws,
            "animate_preset",
            json!({"video": video, "preset": "stagger", "of": "pop_in", "nodes": ids, "start_ms": 0, "step_ms": 100, "from": "random", "seed": seed}),
        )
        .unwrap();
        let v = s.doc().video(&s.video).unwrap();
        made.created
            .iter()
            .map(|id| {
                v.animations
                    .iter()
                    .find(|a| &a.node == id && a.prop == AnimProp::ScaleX)
                    .unwrap()
                    .keys[0]
                    .at_ms
            })
            .collect::<Vec<_>>()
    };
    let (a, b) = (random(3), random(3));
    assert_eq!(a, b, "seeded");
    let mut sorted = a.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, [0, 100, 200, 300]);
}

#[test]
fn bad_scene_moves_are_refused_with_a_reason() {
    let mut s = scene(1280, 720);
    let video = s.video.to_string();
    let err =
        |s: &mut Scene, args: Value| call_tool(&mut s.ws, "animate_preset", args).unwrap_err();
    assert!(
        err(
            &mut s,
            json!({"video": video, "preset": "float", "start_ms": 0})
        )
        .contains("node")
    );
    let made = apply(
        &mut s.ws,
        vec![Operation::Create {
            parent: s.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({"kind": "rect", "x": 1, "y": 1, "width": 50, "height": 50, "fill": "#ffffff"})).unwrap(),
        }],
    );
    let node = made.created[0].to_string();
    assert!(
        err(
            &mut s,
            json!({"video": video, "node": node, "preset": "draw_on", "start_ms": 0})
        )
        .contains("stroke")
    );
    assert!(err(&mut s, json!({"video": video, "node": node, "preset": "pop_in", "start_ms": 0, "duration_ms": 50})).contains("duration"));
    assert!(
        err(
            &mut s,
            json!({"video": video, "node": node, "preset": "pop_in", "start_ms": 0, "intensity": 9})
        )
        .contains("intensity")
    );
    assert!(err(&mut s, json!({"video": video, "node": node, "preset": "animate_along_path", "path": node, "start_ms": 0})).contains("path"));
    assert!(
        err(
            &mut s,
            json!({"video": video, "preset": "camera_pan", "direction": "sideways", "start_ms": 0})
        )
        .contains("direction")
    );
}

#[test]
fn the_connector_labels_pass_the_text_check_and_stay_in_the_frame() {
    for (w, h) in [(1290, 2796), (1920, 1080), (1080, 1080)] {
        let mut s = scene(w, h);
        s.add(
            "connector",
            Some(json!({"label": "Light takes eight minutes", "curve": "s_curve"})),
        );
        let frame = s.doc().node(&s.frame).unwrap();
        assert!(od_render::check_text(frame).is_empty());
    }
}

#[test]
fn a_connector_between_two_nodes_leaves_their_edges() {
    let mut s = scene(1280, 720);
    let made = apply(
        &mut s.ws,
        vec![
            Operation::Create {
                parent: s.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({"kind": "rect", "x": 100, "y": 300, "width": 200, "height": 120, "fill": "#334455"})).unwrap(),
            },
            Operation::Create {
                parent: s.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({"kind": "rect", "x": 900, "y": 300, "width": 200, "height": 120, "fill": "#334455"})).unwrap(),
            },
        ],
    );
    let (a, b) = (made.created[0].to_string(), made.created[1].to_string());
    let p = s.add(
        "connector",
        Some(json!({"from": a, "to": b, "curve": "straight", "arrow": false, "flow": false})),
    );
    let line = s
        .doc()
        .node(&p.parts.iter().find(|(n, _)| n == "line").unwrap().1)
        .unwrap();
    assert!(
        line.x > 280.0 && line.x + line.width < 920.0,
        "runs between the boxes: {} {}",
        line.x,
        line.x + line.width
    );
}

#[test]
fn the_ring_surrounds_its_target() {
    let mut s = scene(1280, 720);
    let made = apply(
        &mut s.ws,
        vec![Operation::Create {
            parent: s.frame.clone(),
            index: None,
            node: serde_json::from_value(json!({"kind": "rect", "x": 500, "y": 300, "width": 200, "height": 100, "fill": "#334455"})).unwrap(),
        }],
    );
    let p = s.add(
        "highlight_ring",
        Some(json!({"target": made.created[0].to_string(), "pulse": false})),
    );
    let ring = s
        .doc()
        .node(&p.parts.iter().find(|(n, _)| n == "ring").unwrap().1)
        .unwrap();
    assert!(
        ring.x < 500.0
            && ring.y < 300.0
            && ring.x + ring.width > 700.0
            && ring.y + ring.height > 400.0
    );
    let v = s.doc().video(&s.video).unwrap();
    let a = v
        .animations
        .iter()
        .find(|a| a.node == ring.id && a.prop == AnimProp::TrimEnd)
        .unwrap();
    assert_eq!(
        (a.keys[0].value, a.keys[a.keys.len() - 1].value),
        (0.0, 1.0)
    );
}

#[test]
fn the_tool_text_names_every_scene_move() {
    let defs = crate::motion::definitions();
    let text = defs[0]["description"].as_str().unwrap();
    for name in [
        "pop_in",
        "bounce_in",
        "elastic_in",
        "slide_in_left",
        "fade_up",
        "zoom_in",
        "draw_on",
        "orbit",
        "stagger",
        "animate_along_path",
        "camera_push_in",
        "camera_pull_back",
        "camera_pan",
        "camera_drift",
        "camera_shake",
        "camera_whip",
        "float",
        "bob",
        "breathe",
        "pulse",
        "spin",
        "wobble",
        "shake",
        "swing",
    ] {
        assert!(text.contains(name), "{name} is not described");
    }
}
