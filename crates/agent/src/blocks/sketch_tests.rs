#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, Document, NodeId, NodeKind, Operation, Workspace};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;

pub(super) struct Scene {
    _dir: TempDir,
    pub ws: Workspace,
    pub frame: NodeId,
    pub video: NodeId,
}

fn apply(ws: &mut Workspace, ops: Vec<Operation>) -> od_core::TxReceipt {
    ws.session.apply(Actor::User, None, &ops).unwrap()
}

pub(super) fn scene(w: u32, h: u32, fill: &str) -> Scene {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Sketch").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let created = apply(
        &mut ws,
        vec![Operation::Create {
            parent: root,
            index: None,
            node: serde_json::from_value(json!({
                "kind": "frame", "name": "Scene", "width": w, "height": h, "fill": fill
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
    pub fn doc(&self) -> &Document {
        self.ws.session.document()
    }

    pub fn request(&self, block: &str, params: Option<Value>) -> Request {
        Request {
            block: block.into(),
            frame: Some(self.frame.to_string()),
            video: Some(self.video.to_string()),
            params: params.map(|p| p.as_object().cloned().unwrap_or_default()),
            ..Request::default()
        }
    }

    pub fn plan(&self, block: &str, params: Option<Value>) -> Result<Plan, BlockError> {
        plan(self.doc(), &self.request(block, params))
    }

    pub fn add(&mut self, block: &str, params: Option<Value>) -> Plan {
        self.add_with(block, params, None)
    }

    pub fn add_with(&mut self, block: &str, params: Option<Value>, colors: Option<Colors>) -> Plan {
        let mut req = self.request(block, params);
        req.colors = colors;
        let p = plan(self.doc(), &req).unwrap_or_else(|e| panic!("{block}: {e}"));
        write_assets(self.ws.dir().unwrap(), &p).unwrap();
        apply(&mut self.ws, p.ops.clone());
        p
    }

    pub fn shot(&self, ms: f64, longest: f64) -> od_render::Pixmap {
        let doc = self.doc();
        let video = doc.video(&self.video).unwrap();
        let at = doc.at_time(video, ms).unwrap_or_else(|| doc.clone());
        let frame = at.node(&self.frame).unwrap();
        let scale = (longest / frame.width.max(frame.height)) as f32;
        od_render::render_frame_camera(
            &at,
            &self.frame,
            scale,
            Some(self.ws.dir().unwrap()),
            &mut od_render::Media::default(),
            video.camera_at(ms),
        )
        .unwrap()
        .pixmap
    }
}

const NEW: [&str; 7] = [
    "sketch",
    "boil",
    "watercolor_wash",
    "paper_grain",
    "flow_strokes",
    "marker_underline",
    "scribble_circle",
];
const SIZES: [(u32, u32); 3] = [(1290, 2796), (1920, 1080), (1080, 1080)];
const PAPER: &str = "#f4efe6";

fn light() -> Colors {
    Colors {
        background: Some(PAPER.into()),
        foreground: Some("#2b2a33".into()),
        accent: Some("#d9534f".into()),
        muted: Some("#7a8b99".into()),
    }
}

fn paper(w: u32, h: u32) -> Scene {
    scene(w, h, PAPER)
}

fn ink_count(pm: &od_render::Pixmap, keep: impl Fn(&[u8]) -> bool) -> usize {
    pm.data().chunks(4).filter(|p| keep(p)).count()
}

fn dark(p: &[u8]) -> bool {
    p[0] < 110 && p[1] < 110 && p[2] < 120
}

/// Any pixel that is noticeably redder than the paper.
fn reddish(p: &[u8]) -> bool {
    p[0] > 180 && i32::from(p[0]) - i32::from(p[1]) > 45
}

fn luma(p: &[u8]) -> f64 {
    0.299 * f64::from(p[0]) + 0.587 * f64::from(p[1]) + 0.114 * f64::from(p[2])
}

/// The mean brightness of the pixels whose position satisfies `keep`.
fn mean_luma(pm: &od_render::Pixmap, keep: impl Fn(f64, f64) -> bool) -> f64 {
    let w = pm.width() as usize;
    let (mut sum, mut n) = (0.0_f64, 0.0_f64);
    for (i, p) in pm.data().chunks(4).enumerate() {
        let (x, y) = (
            (i % w) as f64 / w as f64,
            (i / w) as f64 / pm.height() as f64,
        );
        if keep(x, y) {
            sum += luma(p);
            n += 1.0;
        }
    }
    sum / n.max(1.0)
}

fn changed(a: &od_render::Pixmap, b: &od_render::Pixmap) -> usize {
    a.data()
        .chunks(4)
        .zip(b.data().chunks(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 3))
        .count()
}

fn ops_json(s: &Scene, block: &str, params: Option<Value>) -> String {
    let mut req = s.request(block, params);
    req.colors = Some(light());
    serde_json::to_string(&plan(s.doc(), &req).unwrap().ops).unwrap()
}

fn descendants<'a>(s: &'a Scene, p: &Plan) -> Vec<&'a od_core::Node> {
    let mut ids = Vec::new();
    s.doc().node(&p.parts[0].1).unwrap().ids(&mut ids);
    ids.iter().filter_map(|id| s.doc().node(id)).collect()
}

fn named<'a>(s: &'a Scene, p: &Plan, part: &str) -> &'a od_core::Node {
    let want = format!("{}/{part}", p.name);
    descendants(s, p)
        .into_iter()
        .find(|n| n.name == want)
        .unwrap_or_else(|| panic!("no part {want}"))
}

fn tracks_of(s: &Scene, p: &Plan) -> Vec<od_core::Animation> {
    let ids: Vec<NodeId> = descendants(s, p).iter().map(|n| n.id.clone()).collect();
    s.doc()
        .video(&s.video)
        .unwrap()
        .animations
        .iter()
        .filter(|a| ids.contains(&a.node))
        .cloned()
        .collect()
}

fn call_tool(ws: &mut Workspace, name: &str, args: Value) -> Result<Value, String> {
    let mut turns = crate::tools::Turns::default();
    let mut ctx = crate::tools::CallCtx {
        ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, name, args) {
        Ok(out) => match out.content.first() {
            Some(crate::tools::Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

#[test]
fn the_hand_made_blocks_are_in_the_catalog_and_found_by_what_they_look_like() {
    for id in NEW {
        let b = find(id).unwrap();
        assert!(!b.about.is_empty() && b.tags.len() >= 5, "{id}");
    }
    let hits = |q: &str| search(q, None).iter().map(|b| b.id).collect::<Vec<_>>();
    assert!(hits("hand drawn").contains(&"sketch") && hits("hand drawn").contains(&"boil"));
    assert!(hits("watercolor").contains(&"watercolor_wash"));
    assert!(hits("film grain").contains(&"paper_grain"));
    assert!(hits("grass wind").contains(&"flow_strokes"));
    assert!(hits("underline").contains(&"marker_underline"));
    assert!(hits("circle arrow").contains(&"scribble_circle"));
}

#[test]
fn node_budgets_hold_at_three_sizes_even_with_the_biggest_settings() {
    let twelve: Vec<Value> = (0..12)
        .map(|i| json!({"kind": "ellipse", "x": 40 + i * 20, "y": 40 + i * 20, "width": 300, "height": 200}))
        .collect();
    let cases = [
        (
            "sketch",
            json!({"shape": twelve, "fill_style": "cross_hatch", "passes": 3}),
        ),
        (
            "boil",
            json!({"shape": twelve, "fill_style": "cross_hatch", "variants": 4, "passes": 2}),
        ),
        (
            "watercolor_wash",
            json!({"shape": twelve, "layers": 12, "edge": 1.0, "bleed": 1.0}),
        ),
        ("flow_strokes", json!({"count": 140})),
        ("paper_grain", json!({"variants": 4, "loop_ms": 8000})),
        ("marker_underline", json!({"style": "double"})),
        ("scribble_circle", json!({"mark": "arrow"})),
    ];
    for (w, h) in SIZES {
        for (id, params) in &cases {
            let mut s = paper(w, h);
            let p = s.add(id, Some(params.clone()));
            assert!(
                p.node_count <= 400,
                "{id} at {w}x{h}: {} nodes",
                p.node_count
            );
        }
    }
}

#[test]
fn the_same_seed_gives_the_same_operations_and_another_seed_others() {
    let s = paper(1280, 720);
    for id in NEW {
        let a = ops_json(&s, id, Some(json!({"seed": 11})));
        assert_eq!(a, ops_json(&s, id, Some(json!({"seed": 11}))), "{id}");
        assert_ne!(
            a,
            ops_json(&s, id, Some(json!({"seed": 12}))),
            "{id} ignores its seed"
        );
    }
}

#[test]
fn each_block_is_one_undoable_step_with_everything_it_made() {
    let params = [
        ("sketch", json!({"fill_style": "zigzag", "draw_on": false})),
        ("boil", json!({"fill_style": "dots", "variants": 4})),
        ("watercolor_wash", json!({"layers": 12, "bloom": false})),
        ("paper_grain", json!({"loop_ms": 0, "tone": "dark"})),
        ("flow_strokes", json!({"style": "wind", "sway": 0.0})),
        ("marker_underline", json!({"style": "wavy"})),
        ("scribble_circle", json!({"mark": "cross"})),
    ];
    for (id, p) in params {
        let mut s = paper(1280, 720);
        let before = s.doc().clone();
        let log = s.ws.session.log().len();
        s.add(id, Some(p));
        assert_eq!(s.ws.session.log().len(), log + 1, "{id}");
        s.ws.session.undo(Actor::User).unwrap();
        assert_eq!(s.doc().pages, before.pages, "{id}");
        assert_eq!(s.doc().videos, before.videos, "{id}");
    }
}

#[test]
fn the_sketch_draws_itself_on_and_stops_when_drawn() {
    let mut s = paper(1280, 720);
    s.add_with(
        "sketch",
        Some(json!({
            "shape": {"kind": "rect", "x": 300, "y": 200, "width": 600, "height": 300},
            "fill_style": "none", "passes": 1, "duration_ms": 2000, "roughness": 0.5
        })),
        Some(light()),
    );
    let counts: Vec<usize> = [0.0, 500.0, 1_000.0, 1_500.0, 2_000.0, 3_000.0]
        .iter()
        .map(|t| ink_count(&s.shot(*t, 640.0), dark))
        .collect();
    assert_eq!(counts[0], 0, "nothing is drawn before it starts");
    assert!(
        counts.windows(2).take(4).all(|w| w[1] > w[0]),
        "it keeps growing: {counts:?}"
    );
    assert_eq!(counts[4], counts[5], "and stops when drawn");
}

#[test]
fn every_fill_style_fills_inside_the_shape_and_not_outside_it() {
    for style in [
        "solid",
        "hachure",
        "cross_hatch",
        "zigzag",
        "dots",
        "dashed",
    ] {
        let mut s = paper(1280, 720);
        s.add_with(
            "sketch",
            Some(json!({
                "shape": {"kind": "rect", "x": 400, "y": 200, "width": 400, "height": 300},
                "fill_style": style, "color": "muted", "fill_color": "accent",
                "fill_opacity": 1.0, "draw_on": false, "duration_ms": 200
            })),
            Some(light()),
        );
        let pm = s.shot(0.0, 1280.0);
        let w = pm.width() as usize;
        let (mut inside, mut outside) = (0, 0);
        for (i, p) in pm.data().chunks(4).enumerate() {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            if !reddish(p) {
                continue;
            }
            if (400.0..=800.0).contains(&x) && (200.0..=500.0).contains(&y) {
                inside += 1;
            } else if x < 380.0 || x > 820.0 || y < 180.0 || y > 520.0 {
                outside += 1;
            }
        }
        assert!(inside > 400, "{style} fills {inside} pixels");
        assert_eq!(outside, 0, "{style} spills");
    }
}

#[test]
fn nodes_are_redrawn_by_their_own_shape_and_bad_targets_are_refused() {
    let mut s = paper(1280, 720);
    let made = apply(
        &mut s.ws,
        vec![
            Operation::Create {
                parent: s.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({"kind": "rect", "name": "r", "x": 100, "y": 100, "width": 300, "height": 200, "corner_radius": 30, "fill": "#cccccc"})).unwrap(),
            },
            Operation::Create {
                parent: s.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({"kind": "ellipse", "name": "e", "x": 500, "y": 100, "width": 300, "height": 200, "fill": "#cccccc"})).unwrap(),
            },
            Operation::Create {
                parent: s.frame.clone(),
                index: None,
                node: serde_json::from_value(json!({"kind": "path", "name": "p", "x": 100, "y": 400, "width": 400, "height": 200, "path": "M0,200 L200,0 L400,200 Z", "fill": "#cccccc"})).unwrap(),
            },
        ],
    );
    let ids: Vec<String> = made.created.iter().map(ToString::to_string).collect();
    let p = s.add_with(
        "sketch",
        Some(json!({"targets": ids, "fill_style": "hachure", "passes": 1})),
        Some(light()),
    );
    for i in 1..=3 {
        named(&s, &p, &format!("s{i}-line-1"));
        named(&s, &p, &format!("s{i}-fill"));
    }
    let triangle = named(&s, &p, "s3-line-1");
    assert!(
        triangle.y >= 380.0 && triangle.y + triangle.height <= 640.0,
        "{triangle:?}"
    );
    let missing = s.plan("sketch", Some(json!({"targets": ["n_nope"]})));
    assert!(missing.is_err());
    let frame_id = s.frame.to_string();
    let other = s.plan("sketch", Some(json!({"targets": [frame_id]})));
    assert!(other.is_ok(), "the frame itself is a valid target");
}

#[test]
fn shapes_are_checked_and_refused_with_a_reason() {
    let s = paper(1280, 720);
    let refused = |shape: Value, why: &str| {
        let e = s
            .plan("sketch", Some(json!({"shape": shape})))
            .unwrap_err()
            .to_string();
        assert!(e.contains(why), "{e}");
    };
    refused(json!({"kind": "star"}), "no shape kind");
    refused(
        json!({"kind": "rect", "x": 0, "y": 0, "width": -5, "height": 4}),
        "positive",
    );
    refused(
        json!({"kind": "rect", "x": 1e9, "y": 0, "width": 5, "height": 4}),
        "out of range",
    );
    refused(json!({"kind": "polygon", "points": [[0, 0]]}), "points");
    refused(json!({"kind": "path", "d": "garbage"}), "path");
    refused(
        json!(vec![
            json!({"kind": "circle", "cx": 5, "cy": 5, "r": 3});
            13
        ]),
        "at most",
    );
    refused(
        json!({"kind": "polygon", "points": [[0, 0], [5, "x"], [3, 3]]}),
        "finite",
    );
}

#[test]
fn boil_shows_exactly_one_drawing_at_every_frame_and_the_drawings_differ() {
    for variants in [2u64, 3, 4] {
        let mut s = paper(1280, 720);
        let p = s.add_with("boil", Some(json!({"variants": variants})), Some(light()));
        let groups: Vec<&od_core::Node> = descendants(&s, &p)
            .into_iter()
            .filter(|n| n.kind == NodeKind::Group && n.name.contains("/boil-"))
            .collect();
        assert_eq!(groups.len() as u64, variants);
        let video = s.doc().video(&s.video).unwrap();
        let anims: Vec<&od_core::Animation> = groups
            .iter()
            .map(|g| video.animations.iter().find(|a| a.node == g.id).unwrap())
            .collect();
        assert!(anims.iter().all(|a| a.repeat == od_core::Repeat::Loop));
        for frame in 0..90 {
            let t = f64::from(frame) * 1000.0 / 30.0;
            let shown: Vec<f64> = anims.iter().map(|a| a.value_at(t).unwrap()).collect();
            assert!(
                shown.iter().all(|v| *v < 0.05 || *v > 0.95),
                "frame {frame}: {shown:?}"
            );
            assert!(
                (shown.iter().sum::<f64>() - 1.0).abs() < 0.06,
                "frame {frame}: {shown:?}"
            );
        }
        let line = |g: &od_core::Node| {
            g.children
                .iter()
                .filter_map(|c| c.path.clone())
                .collect::<Vec<_>>()
        };
        let drawn: Vec<Vec<String>> = groups.iter().map(|g| line(g)).collect();
        assert!(
            drawn.windows(2).all(|w| w[0] != w[1]),
            "the variants are different drawings"
        );
    }
}

#[test]
fn boil_redraws_a_dozen_times_a_second_in_pixels() {
    let mut s = paper(1280, 720);
    let p = s.add_with("boil", Some(json!({"fps": 12})), Some(light()));
    let video = s.doc().video(&s.video).unwrap();
    let first = descendants(&s, &p)
        .into_iter()
        .find(|n| n.name.ends_with("/boil-1"))
        .unwrap();
    let keys = &video
        .animations
        .iter()
        .find(|a| a.node == first.id)
        .unwrap()
        .keys;
    let slot = keys.iter().map(|k| k.at_ms).find(|t| *t > 40).unwrap();
    assert!((60..=120).contains(&slot), "one drawing per {slot} ms");
    let frames: Vec<od_render::Pixmap> = (0..3)
        .map(|i| s.shot(f64::from(i) * 100.0 + 5.0, 640.0))
        .collect();
    assert!(ink_count(&frames[0], dark) > 500);
    assert!(changed(&frames[0], &frames[1]) > 100 && changed(&frames[1], &frames[2]) > 100);
    let again = s.shot(5.0 + 3.0 * 100.0 * 0.0, 640.0);
    assert_eq!(
        changed(&frames[0], &again),
        0,
        "the same frame renders the same"
    );
}

fn band((x, y): (f64, f64), (cx, cy, rx, ry): (f64, f64, f64, f64), (lo, hi): (f64, f64)) -> bool {
    let d = (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt();
    (lo..hi).contains(&d)
}

#[test]
fn a_wash_is_painted_and_its_edge_band_is_darker_when_asked() {
    let shape = json!({"kind": "ellipse", "x": 240, "y": 120, "width": 800, "height": 480});
    let render = |edge: f64| {
        let mut s = paper(1280, 720);
        s.add_with(
            "watercolor_wash",
            Some(json!({"shape": shape, "edge": edge, "bloom": false, "deform": 0.3, "bleed": 0.0, "softness": 0.2})),
            Some(light()),
        );
        s.shot(0.0, 1280.0)
    };
    let (plain, edged) = (render(0.0), render(1.0));
    let blank = mean_luma(&plain, |_, _| true);
    assert!(blank < 230.0, "the paper is no longer blank: {blank}");
    let rim = |pm: &od_render::Pixmap| {
        mean_luma(pm, |x, y| {
            band(
                (x * 1280.0, y * 720.0),
                (640.0, 360.0, 400.0, 240.0),
                (0.82, 0.97),
            )
        })
    };
    assert!(
        rim(&edged) + 4.0 < rim(&plain),
        "rim {} vs {}",
        rim(&edged),
        rim(&plain)
    );
    let core = |pm: &od_render::Pixmap| {
        mean_luma(pm, |x, y| {
            band(
                (x * 1280.0, y * 720.0),
                (640.0, 360.0, 400.0, 240.0),
                (0.0, 0.5),
            )
        })
    };
    assert!(
        (core(&edged) - core(&plain)).abs() < 3.0,
        "the middle is the same"
    );
    let paper_luma = luma(&[0xf4, 0xef, 0xe6]);
    let corner = mean_luma(&plain, |x, y| x < 0.1 && y < 0.1);
    assert!(
        (corner - paper_luma).abs() < 2.0,
        "paper stays paper outside the wash"
    );
}

#[test]
fn a_wash_has_the_layers_asked_for_blurred_and_with_a_bleed() {
    for layers in [6u64, 9, 12] {
        let mut s = paper(1280, 720);
        let p = s.add_with(
            "watercolor_wash",
            Some(json!({"layers": layers})),
            Some(light()),
        );
        let nodes = descendants(&s, &p);
        let count = |what: &str| nodes.iter().filter(|n| n.name.contains(what)).count() as u64;
        assert_eq!(count("-layer-"), layers);
        assert_eq!(count("-bleed"), 1);
        assert_eq!(count("-pool-"), 3);
        let paint = nodes.iter().find(|n| n.name.ends_with("/paint")).unwrap();
        assert!(paint.blur > 0.0, "the edge is soft");
        assert!(
            nodes
                .iter()
                .filter(|n| n.name.contains("-layer-"))
                .all(|n| n.opacity < 0.35)
        );
    }
    let mut s = paper(1280, 720);
    let p = s.add("watercolor_wash", Some(json!({"bleed": 0.0, "edge": 0.0})));
    let nodes = descendants(&s, &p);
    assert!(
        nodes
            .iter()
            .all(|n| !n.name.contains("-bleed") && !n.name.contains("-edge-"))
    );
}

#[test]
fn a_wash_blooms_in_and_can_stay_still() {
    let mut s = paper(1280, 720);
    let p = s.add_with("watercolor_wash", None, Some(light()));
    let (early, late) = (s.shot(0.0, 480.0), s.shot(3_000.0, 480.0));
    assert!(
        changed(&early, &late) > 5_000,
        "nothing at first, a wash after"
    );
    assert_eq!(tracks_of(&s, &p).len(), 2);
    let mut still = paper(1280, 720);
    let p = still.add("watercolor_wash", Some(json!({"bloom": false})));
    assert!(tracks_of(&still, &p).is_empty());
}

#[test]
fn grain_changes_the_paper_and_boils_between_loop_phases() {
    let mut s = scene(1280, 720, "#808080");
    let blank = s.shot(0.0, 640.0);
    let p = s.add_with(
        "paper_grain",
        Some(json!({"strength": 0.9, "loop_ms": 1000})),
        Some(light()),
    );
    let (a, b) = (s.shot(10.0, 640.0), s.shot(110.0, 640.0));
    assert!(changed(&blank, &a) > 20_000, "the grain shows");
    assert!(
        changed(&a, &b) > 20_000,
        "and is another grain a moment later"
    );
    let period = s.shot(10.0 + 1_000.0 * 2.0, 640.0);
    assert_eq!(changed(&a, &period), 0, "a loop later it is the same grain");
    assert!(
        tracks_of(&s, &p)
            .iter()
            .all(|t| t.repeat == od_core::Repeat::Loop)
    );
    let mut still = scene(1280, 720, "#808080");
    still.add_with(
        "paper_grain",
        Some(json!({"strength": 0.9, "loop_ms": 0})),
        Some(light()),
    );
    assert_eq!(
        changed(&still.shot(10.0, 640.0), &still.shot(700.0, 640.0)),
        0,
        "loop_ms 0 holds one grain"
    );
}

#[test]
fn grain_pictures_are_named_by_recipe_and_written_into_media() {
    let mut s = paper(1280, 720);
    let p = s.add(
        "paper_grain",
        Some(json!({"variants": 3, "tone": "both", "seed": 4})),
    );
    let files: Vec<String> = descendants(&s, &p)
        .iter()
        .filter_map(|n| n.image.clone())
        .collect();
    assert_eq!(files.len(), 6);
    let mut unique = files.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), 6, "every grain is its own picture");
    for f in &files {
        assert!(f.starts_with("media/block-grain-"), "{f}");
        assert!(s.ws.dir().unwrap().join(f).exists(), "{f} is written");
    }
    let mut again = paper(1280, 720);
    let q = again.add(
        "paper_grain",
        Some(json!({"variants": 3, "tone": "both", "seed": 4})),
    );
    let same: Vec<String> = descendants(&again, &q)
        .iter()
        .filter_map(|n| n.image.clone())
        .collect();
    assert_eq!(files, same, "the same recipe is the same file");
}

#[test]
fn the_vignette_darkens_the_corners_and_the_tint_warms_the_paper() {
    let render = |params: Value| {
        let mut s = paper(1280, 720);
        s.add_with("paper_grain", Some(params), Some(light()));
        s.shot(0.0, 640.0)
    };
    let off = render(json!({"vignette": 0.0, "tint": 0.0, "strength": 0.05, "loop_ms": 0}));
    let on = render(json!({"vignette": 1.0, "tint": 0.0, "strength": 0.05, "loop_ms": 0}));
    let corner = |pm: &od_render::Pixmap| {
        mean_luma(pm, |x, y| (x - 0.5).abs() > 0.42 && (y - 0.5).abs() > 0.4)
    };
    let middle = |pm: &od_render::Pixmap| {
        mean_luma(pm, |x, y| (x - 0.5).abs() < 0.15 && (y - 0.5).abs() < 0.15)
    };
    assert!(
        corner(&on) + 15.0 < corner(&off),
        "{} vs {}",
        corner(&on),
        corner(&off)
    );
    assert!((middle(&on) - middle(&off)).abs() < 3.0);
    let tinted = render(json!({"vignette": 0.0, "tint": 1.0, "strength": 0.05, "loop_ms": 0}));
    assert!(mean_luma(&tinted, |_, _| true) < mean_luma(&off, |_, _| true) - 1.0);
}

fn tuft_boxes(s: &Scene, p: &Plan) -> Vec<od_core::geometry::Rect> {
    descendants(s, p)
        .into_iter()
        .filter(|n| n.name.contains("/stroke-"))
        .map(|n| od_core::geometry::Rect {
            x: n.x,
            y: n.y,
            width: n.width,
            height: n.height,
        })
        .collect()
}

#[test]
fn flow_strokes_follow_their_style_and_stay_in_the_box() {
    for style in ["grass", "hair", "wind", "water", "flow"] {
        let mut s = paper(1280, 720);
        let mut req = s.request("flow_strokes", Some(json!({"style": style, "count": 50})));
        req.region = Some(BoxArgs {
            x: Some(100.0),
            y: Some(100.0),
            width: Some(800.0),
            height: Some(500.0),
        });
        let p = plan(s.doc(), &req).unwrap();
        ws_apply(&mut s, &p);
        let boxes = tuft_boxes(&s, &p);
        assert!(boxes.len() >= 45, "{style}: {} strokes", boxes.len());
        for r in &boxes {
            assert!(
                r.x >= 97.0 && r.y >= 97.0 && r.x + r.width <= 903.0 && r.y + r.height <= 603.0,
                "{style} {r:?}"
            );
        }
        let tall = boxes.iter().filter(|r| r.height > r.width).count();
        match style {
            "grass" | "hair" => assert!(tall * 4 > boxes.len() * 3, "{style} strokes are upright"),
            "wind" | "water" => assert!(tall * 4 < boxes.len(), "{style} strokes lie flat"),
            _ => {}
        }
    }
}

fn ws_apply(s: &mut Scene, p: &Plan) {
    write_assets(s.ws.dir().unwrap(), p).unwrap();
    apply(&mut s.ws, p.ops.clone());
}

#[test]
fn grass_grows_from_the_bottom_and_hair_from_the_top() {
    let mut s = paper(1280, 720);
    let g = s.add("flow_strokes", Some(json!({"style": "grass"})));
    let rooted = tuft_boxes(&s, &g)
        .iter()
        .filter(|r| r.y + r.height > 640.0)
        .count();
    assert!(
        rooted * 10 > tuft_boxes(&s, &g).len() * 9,
        "blades stand on the bottom edge"
    );
    let mut t = paper(1280, 720);
    let h = t.add("flow_strokes", Some(json!({"style": "hair"})));
    let hung = tuft_boxes(&t, &h).iter().filter(|r| r.y < 100.0).count();
    assert!(
        hung * 10 > tuft_boxes(&t, &h).len() * 9,
        "hair hangs from the top edge"
    );
}

#[test]
fn flow_bands_sway_in_seamless_loops_out_of_step() {
    let mut s = paper(1280, 720);
    let p = s.add(
        "flow_strokes",
        Some(json!({"style": "grass", "sway": 1.0, "sway_ms": 4000})),
    );
    let all = tracks_of(&s, &p);
    assert!(all.len() >= 3);
    let mut periods = Vec::new();
    for t in &all {
        assert_eq!(t.repeat, od_core::Repeat::Loop);
        assert_eq!(t.prop, od_core::AnimProp::Rotation);
        let (first, last) = (t.keys[0].value, t.keys[t.keys.len() - 1].value);
        assert!((first - last).abs() < 1e-6, "no seam");
        periods.push(t.keys[t.keys.len() - 1].at_ms);
    }
    periods.sort_unstable();
    periods.dedup();
    assert!(
        periods.len() > 1,
        "bands do not move in lockstep: {periods:?}"
    );
    let mut still = paper(1280, 720);
    let q = still.add("flow_strokes", Some(json!({"sway": 0.0})));
    assert!(tracks_of(&still, &q).is_empty());
}

#[test]
fn wind_drifts_sideways_instead_of_rotating() {
    let mut s = paper(1280, 720);
    let p = s.add("flow_strokes", Some(json!({"style": "wind"})));
    assert!(
        tracks_of(&s, &p)
            .iter()
            .all(|t| t.prop == od_core::AnimProp::X)
    );
}

#[test]
fn an_underline_sits_under_its_target_and_draws_on_without_touching_it() {
    for style in ["marker", "double", "wavy", "scribble"] {
        let mut s = paper(1280, 720);
        s.add_with(
            "marker_underline",
            Some(json!({"style": style, "target": {"x": 300, "y": 250, "width": 600, "height": 90}, "duration_ms": 1000})),
            Some(light()),
        );
        let (start, end) = (s.shot(0.0, 1280.0), s.shot(2_000.0, 1280.0));
        assert_eq!(
            ink_count(&start, reddish),
            0,
            "{style}: nothing before it draws"
        );
        let w = end.width() as usize;
        let (mut below, mut elsewhere) = (0, 0);
        for (i, p) in end.data().chunks(4).enumerate() {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            if !reddish(p) {
                continue;
            }
            if y > 340.0 && y < 400.0 && x > 230.0 && x < 970.0 {
                below += 1;
            } else {
                elsewhere += 1;
            }
        }
        assert!(below > 300, "{style}: {below}");
        assert_eq!(elsewhere, 0, "{style} stays under the target");
    }
}

#[test]
fn a_scribbled_circle_goes_around_its_target_and_an_arrow_points_at_it() {
    let target = json!({"x": 500, "y": 300, "width": 280, "height": 80});
    let mut s = paper(1280, 720);
    let p = s.add_with(
        "scribble_circle",
        Some(json!({"target": target, "mark": "circle"})),
        Some(light()),
    );
    let mark = named(&s, &p, "mark");
    assert!(
        mark.x < 500.0
            && mark.y < 300.0
            && mark.x + mark.width > 780.0
            && mark.y + mark.height > 380.0,
        "{mark:?}"
    );
    let end = s.shot(3_000.0, 1280.0);
    let w = end.width() as usize;
    let inside_target = end
        .data()
        .chunks(4)
        .enumerate()
        .filter(|(i, px)| {
            let (x, y) = ((i % w) as f64, (i / w) as f64);
            reddish(px) && (510.0..770.0).contains(&x) && (310.0..370.0).contains(&y)
        })
        .count();
    assert_eq!(inside_target, 0, "the target stays clear");
    let mut t = paper(1280, 720);
    let q = t.add_with(
        "scribble_circle",
        Some(json!({"target": target, "mark": "arrow", "arrow_from": 180})),
        Some(light()),
    );
    let arrow = named(&t, &q, "mark");
    assert!(
        arrow.x + arrow.width <= 500.0 + 1.0 && arrow.x < 400.0,
        "the arrow comes from the left: {arrow:?}"
    );
    assert!(
        (arrow.y - 340.0).abs() < 60.0,
        "and points level at the target: {arrow:?}"
    );
}

#[test]
fn marks_draw_on_with_a_trim_in_order() {
    let mut s = paper(1280, 720);
    let p = s.add(
        "marker_underline",
        Some(json!({"style": "marker", "duration_ms": 1200})),
    );
    let all = tracks_of(&s, &p);
    assert_eq!(all.len(), 2);
    assert!(all.iter().all(|t| t.prop == od_core::AnimProp::TrimEnd));
    let (a, b) = (&all[0], &all[1]);
    assert_eq!(a.keys[0].at_ms, 0);
    assert!(
        b.keys[0].at_ms >= a.keys[1].at_ms - 1,
        "the second stroke starts when the first ends"
    );
    assert!(b.keys[1].at_ms <= 1_201);
}

#[test]
fn the_tool_text_and_the_playbook_name_every_hand_made_block() {
    let defs = definitions();
    let text = defs[0]["description"].as_str().unwrap();
    let book = crate::playbook::topic("sketch").unwrap();
    for id in NEW {
        assert!(text.contains(id), "list_blocks does not mention {id}");
        assert!(
            book.contains(id),
            "the sketch playbook does not mention {id}"
        );
    }
    assert!(text.contains("get_playbook"));
}

#[test]
fn add_block_works_through_the_tool_for_every_hand_made_block() {
    for id in NEW {
        let mut s = paper(1280, 720);
        let frame = s.frame.to_string();
        let out = call_tool(&mut s.ws, "add_block", json!({"block": id, "frame": frame})).unwrap();
        assert_eq!(out["block"], id);
        assert!(out["created_count"].as_u64().unwrap() > 1);
    }
    let mut s = paper(1280, 720);
    let e = call_tool(
        &mut s.ws,
        "add_block",
        json!({"block": "sketch", "params": {"roughness": 9}}),
    )
    .unwrap_err();
    assert!(e.contains("roughness"), "{e}");
}
