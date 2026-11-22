#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Pixel checks of vector drawing: paths, fill rules, gradients, dashes and trims.

use od_core::{
    Actor, AnimProp, Document, Ease, Key, LineCap, NewNode, NodeId, NodeKind, Operation, Paint,
    Property, Session, Stop, Stroke,
};
use od_render::{Rendered, render_frame};

struct Scene {
    s: Session,
    frame: NodeId,
}

fn scene(w: f64, h: f64) -> Scene {
    let mut s = Session::new(Document::new("vector"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = w;
    frame.height = h;
    frame.fill = Some(Paint::parse("#ffffff").unwrap());
    let frame = create(&mut s, &root, frame);
    Scene { s, frame }
}

fn create(s: &mut Session, parent: &NodeId, node: NewNode) -> NodeId {
    s.apply(
        Actor::User,
        None,
        &[Operation::Create {
            parent: parent.clone(),
            index: None,
            node,
        }],
    )
    .unwrap()
    .created[0]
        .clone()
}

impl Scene {
    fn add(&mut self, node: NewNode) -> NodeId {
        let frame = self.frame.clone();
        create(&mut self.s, &frame, node)
    }

    fn render(&self, scale: f32) -> Rendered {
        render_frame(self.s.document(), &self.frame, scale, None).unwrap()
    }
}

fn px(r: &Rendered, x: u32, y: u32) -> [u8; 4] {
    let c = r.pixmap.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn near(got: [u8; 4], want: [u8; 3], tolerance: i32) -> bool {
    (0..3).all(|i| (i32::from(got[i]) - i32::from(want[i])).abs() <= tolerance)
}

fn path(d: &str, x: f64, y: f64) -> NewNode {
    let mut n = NewNode::new(NodeKind::Path);
    n.path = Some(d.to_string());
    n.x = x;
    n.y = y;
    n
}

fn solid(hex: &str) -> Option<Paint> {
    Some(Paint::parse(hex).unwrap())
}

fn stops(list: &[(f64, &str)]) -> Vec<Stop> {
    list.iter()
        .map(|(offset, color)| Stop {
            offset: *offset,
            color: Paint::parse(color).unwrap(),
        })
        .collect()
}

const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [255, 0, 0];

#[test]
fn a_filled_path_covers_its_shape_and_only_its_shape() {
    let mut sc = scene(100.0, 100.0);
    let mut tri = path("M0 0 L60 0 L0 60 Z", 20.0, 20.0);
    tri.fill = solid("#ff0000");
    sc.add(tri);
    let r = sc.render(1.0);
    assert!(near(px(&r, 30, 30), RED, 1));
    assert!(near(px(&r, 70, 70), WHITE, 1));
    assert!(near(px(&r, 75, 25), WHITE, 1));
}

#[test]
fn the_fill_rule_decides_whether_a_nested_shape_is_a_hole() {
    let nested = "M0 0 H100 V100 H0 Z M30 30 H70 V70 H30 Z";
    let mut sc = scene(100.0, 100.0);
    let mut nonzero = path(nested, 0.0, 0.0);
    nonzero.fill = solid("#ff0000");
    let id = sc.add(nonzero);
    assert!(
        near(px(&sc.render(1.0), 50, 50), RED, 1),
        "same direction fills the middle"
    );
    sc.s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id,
            prop: od_core::Property::FillRule(Some(od_core::FillRule::Evenodd)),
        }],
    )
    .unwrap();
    let r = sc.render(1.0);
    assert!(near(px(&r, 50, 50), WHITE, 1), "evenodd leaves a hole");
    assert!(near(px(&r, 10, 10), RED, 1));
}

#[test]
fn a_path_is_stretched_to_its_box_so_resizing_scales_it() {
    let mut sc = scene(200.0, 100.0);
    let mut line = path("M0 0 L10 0 L10 10 L0 10 Z", 0.0, 0.0);
    line.fill = solid("#ff0000");
    line.width = 100.0;
    line.height = 50.0;
    let id = sc.add(line);
    let r = sc.render(1.0);
    assert!(near(px(&r, 90, 45), RED, 1));
    assert!(near(px(&r, 110, 45), WHITE, 1));
    sc.s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id,
            prop: od_core::Property::Width(200.0),
        }],
    )
    .unwrap();
    assert!(near(px(&sc.render(1.0), 150, 45), RED, 1));
}

#[test]
fn a_view_box_keeps_imported_art_where_it_was_drawn() {
    let mut sc = scene(100.0, 100.0);
    let mut dot = path("M40 40 H60 V60 H40 Z", 0.0, 0.0);
    dot.fill = solid("#ff0000");
    dot.width = 100.0;
    dot.height = 100.0;
    dot.view_box = Some(od_core::ViewBox {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    });
    sc.add(dot);
    let r = sc.render(1.0);
    assert!(near(px(&r, 50, 50), RED, 1));
    assert!(near(px(&r, 20, 20), WHITE, 1));
}

#[test]
fn a_linear_gradient_runs_across_the_box_at_its_angle() {
    let mut sc = scene(100.0, 100.0);
    let mut bar = NewNode::new(NodeKind::Rect);
    bar.width = 100.0;
    bar.height = 20.0;
    bar.fill = Some(Paint::Linear {
        angle: 0.0,
        stops: stops(&[(0.0, "#ff0000"), (1.0, "#0000ff")]),
    });
    sc.add(bar);
    let mut tall = NewNode::new(NodeKind::Rect);
    tall.y = 40.0;
    tall.width = 20.0;
    tall.height = 60.0;
    tall.fill = Some(Paint::Linear {
        angle: 90.0,
        stops: stops(&[(0.0, "#00ff00"), (1.0, "#000000")]),
    });
    sc.add(tall);
    let r = sc.render(1.0);
    assert!(near(px(&r, 1, 10), RED, 8));
    assert!(near(px(&r, 98, 10), [3, 0, 252], 8));
    assert!(near(px(&r, 50, 10), [128, 0, 128], 8));
    assert!(near(px(&r, 10, 41), [0, 250, 0], 10));
    assert!(near(px(&r, 10, 98), [0, 5, 0], 10));
}

#[test]
fn a_radial_gradient_is_centered_and_reaches_its_last_stop_at_the_radius() {
    let mut sc = scene(100.0, 100.0);
    let mut disc = NewNode::new(NodeKind::Rect);
    disc.width = 100.0;
    disc.height = 100.0;
    disc.fill = Some(Paint::Radial {
        cx: 0.5,
        cy: 0.5,
        r: 0.5,
        stops: stops(&[(0.0, "#ffffff"), (0.5, "#ff0000"), (1.0, "#000000")]),
    });
    sc.add(disc);
    let r = sc.render(1.0);
    assert!(near(px(&r, 50, 50), WHITE, 8));
    assert!(near(px(&r, 75, 50), RED, 10), "{:?}", px(&r, 75, 50));
    assert!(near(px(&r, 99, 50), [0, 0, 0], 12), "{:?}", px(&r, 99, 50));
    assert!(
        near(px(&r, 2, 2), [0, 0, 0], 3),
        "the corner is past the radius"
    );
}

#[test]
fn a_gradient_fills_ellipses_paths_and_strokes_and_stops_take_tokens() {
    let mut sc = scene(100.0, 100.0);
    sc.s.apply(
        Actor::User,
        None,
        &[Operation::SetToken {
            name: "sky".into(),
            value: Some("#ff0000".into()),
        }],
    )
    .unwrap();
    let gradient = Paint::Linear {
        angle: 0.0,
        stops: vec![
            Stop {
                offset: 0.0,
                color: Paint::parse("{sky}").unwrap(),
            },
            Stop {
                offset: 1.0,
                color: Paint::parse("#0000ff").unwrap(),
            },
        ],
    };
    let mut egg = NewNode::new(NodeKind::Ellipse);
    egg.width = 100.0;
    egg.height = 40.0;
    egg.fill = Some(gradient.clone());
    sc.add(egg);
    let mut blob = path("M0 0 H100 V20 H0 Z", 0.0, 50.0);
    blob.fill = Some(gradient.clone());
    sc.add(blob);
    let mut line = path("M0 0 H100", 0.0, 80.0);
    line.stroke = Some(Stroke::new(gradient, 10.0));
    sc.add(line);
    let r = sc.render(1.0);
    assert!(
        near(px(&r, 20, 20), [204, 0, 51], 12),
        "ellipse {:?}",
        px(&r, 20, 20)
    );
    assert!(
        near(px(&r, 15, 60), [217, 0, 38], 14),
        "path {:?}",
        px(&r, 15, 60)
    );
    assert!(
        near(px(&r, 85, 80), [38, 0, 217], 14),
        "stroke {:?}",
        px(&r, 85, 80)
    );
}

#[test]
fn gradient_stops_out_of_order_are_drawn_sorted() {
    let mut sc = scene(100.0, 100.0);
    let mut bar = NewNode::new(NodeKind::Rect);
    bar.width = 100.0;
    bar.height = 10.0;
    bar.fill = Some(Paint::Linear {
        angle: 0.0,
        stops: stops(&[(1.0, "#0000ff"), (0.0, "#ff0000")]),
    });
    sc.add(bar);
    let r = sc.render(1.0);
    assert!(near(px(&r, 2, 5), RED, 10));
}

#[test]
fn a_gradient_renders_the_same_at_two_scales() {
    let mut sc = scene(100.0, 100.0);
    let mut bar = NewNode::new(NodeKind::Rect);
    bar.width = 100.0;
    bar.height = 100.0;
    bar.fill = Some(Paint::Linear {
        angle: 45.0,
        stops: stops(&[(0.0, "#ff0000"), (1.0, "#0000ff")]),
    });
    sc.add(bar);
    let small = sc.render(1.0);
    let big = sc.render(2.0);
    assert_eq!(big.pixmap.width(), 200);
    let (a, b) = (px(&small, 50, 50), px(&big, 100, 100));
    assert!(near(a, [b[0], b[1], b[2]], 3), "{a:?} {b:?}");
}

fn line_scene(stroke: Stroke) -> (Scene, NodeId) {
    let mut sc = scene(100.0, 40.0);
    let mut line = path("M0 0 H100", 0.0, 20.0);
    line.stroke = Some(stroke);
    let id = sc.add(line);
    (sc, id)
}

#[test]
fn a_dashed_stroke_alternates_ink_and_gaps() {
    let mut stroke = Stroke::new(Paint::parse("#ff0000").unwrap(), 6.0);
    stroke.dash = vec![10.0, 10.0];
    let (sc, _) = line_scene(stroke);
    let r = sc.render(1.0);
    assert!(near(px(&r, 5, 20), RED, 1));
    assert!(near(px(&r, 15, 20), WHITE, 1));
    assert!(near(px(&r, 25, 20), RED, 1));
    assert!(near(px(&r, 35, 20), WHITE, 1));
}

#[test]
fn round_caps_reach_past_the_dash_ends() {
    let mut stroke = Stroke::new(Paint::parse("#ff0000").unwrap(), 8.0);
    stroke.dash = vec![10.0, 20.0];
    let (butt, _) = line_scene(stroke.clone());
    stroke.cap = LineCap::Round;
    let (round, _) = line_scene(stroke);
    assert!(near(px(&butt.render(1.0), 12, 20), WHITE, 1));
    assert!(near(px(&round.render(1.0), 12, 20), RED, 1));
}

#[test]
fn trim_keeps_only_the_chosen_stretch_of_the_outline() {
    let mut stroke = Stroke::new(Paint::parse("#ff0000").unwrap(), 6.0);
    stroke.trim_end = 0.5;
    let (sc, id) = line_scene(stroke);
    let r = sc.render(1.0);
    assert!(near(px(&r, 25, 20), RED, 1));
    assert!(near(px(&r, 75, 20), WHITE, 1));
    let mut sc = sc;
    sc.s.apply(
        Actor::User,
        None,
        &[
            Operation::Set {
                id: id.clone(),
                prop: od_core::Property::TrimStart(0.5),
            },
            Operation::Set {
                id,
                prop: od_core::Property::TrimEnd(1.0),
            },
        ],
    )
    .unwrap();
    let r = sc.render(1.0);
    assert!(near(px(&r, 25, 20), WHITE, 1));
    assert!(near(px(&r, 75, 20), RED, 1));
}

#[test]
fn trim_works_on_closed_shapes_and_rounded_rects() {
    let mut sc = scene(100.0, 100.0);
    let mut square = NewNode::new(NodeKind::Rect);
    square.x = 10.0;
    square.y = 10.0;
    square.width = 80.0;
    square.height = 80.0;
    let mut stroke = Stroke::new(Paint::parse("#ff0000").unwrap(), 6.0);
    stroke.trim_end = 0.25;
    square.stroke = Some(stroke);
    sc.add(square);
    let r = sc.render(1.0);
    assert!(
        near(px(&r, 50, 10), RED, 1),
        "top edge is the first quarter"
    );
    assert!(
        near(px(&r, 50, 90), WHITE, 1),
        "bottom edge is trimmed away"
    );
    assert!(near(px(&r, 10, 50), WHITE, 1), "left edge is trimmed away");
}

#[test]
fn a_dashed_trimmed_gradient_stroke_on_a_rounded_rect_draws() {
    let mut sc = scene(100.0, 100.0);
    let mut card = NewNode::new(NodeKind::Rect);
    card.x = 10.0;
    card.y = 10.0;
    card.width = 80.0;
    card.height = 80.0;
    card.corner_radius = Some(20.0);
    let mut stroke = Stroke::new(
        Paint::Linear {
            angle: 0.0,
            stops: stops(&[(0.0, "#ff0000"), (1.0, "#0000ff")]),
        },
        4.0,
    );
    stroke.dash = vec![6.0, 6.0];
    stroke.trim_start = 0.1;
    stroke.trim_end = 0.9;
    card.stroke = Some(stroke);
    sc.add(card);
    let r = sc.render(1.0);
    let inked = (0..100)
        .flat_map(|y| (0..100).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&r, x, y) != [255, 255, 255, 255])
        .count();
    assert!(inked > 200, "{inked}");
}

#[test]
fn a_zero_width_stroke_draws_nothing() {
    let stroke = Stroke::new(Paint::parse("#ff0000").unwrap(), 0.0);
    let (sc, _) = line_scene(stroke);
    assert!(near(px(&sc.render(1.0), 50, 20), WHITE, 1));
}

#[test]
fn rendering_is_deterministic() {
    let mut sc = scene(120.0, 80.0);
    let mut blob = path("M0 30 C20 0 60 0 80 30 S120 60 100 80 Z", 10.0, 10.0);
    blob.fill = Some(Paint::Radial {
        cx: 0.3,
        cy: 0.4,
        r: 0.7,
        stops: stops(&[(0.0, "#ffcc00"), (1.0, "#cc0044")]),
    });
    let mut stroke = Stroke::new(Paint::parse("#223344").unwrap(), 3.0);
    stroke.dash = vec![5.0, 3.0];
    stroke.trim_end = 0.8;
    blob.stroke = Some(stroke);
    sc.add(blob);
    let (a, b) = (sc.render(1.0), sc.render(1.0));
    assert_eq!(a.pixmap.data(), b.pixmap.data());
}

fn animated_line() -> (Session, NodeId) {
    let sc = scene(100.0, 40.0);
    let stroke = Stroke::new(Paint::parse("#ff0000").unwrap(), 6.0);
    let (mut s, frame) = (sc.s, sc.frame);
    let mut line = path("M0 0 H100", 0.0, 20.0);
    line.stroke = Some(stroke);
    let node = create(&mut s, &frame, line);
    let video = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "V".into(),
                fps: 30,
                width: 100,
                height: 40,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    s.apply(
        Actor::User,
        None,
        &[
            Operation::AddClip {
                video: video.clone(),
                frame: Some(frame),
                footage: None,
                duration_ms: 1000,
                index: None,
                transition: None,
                motion: None,
            },
            Operation::Animate {
                additive: false,
                video: video.clone(),
                node: node.clone(),
                prop: AnimProp::TrimEnd,
                repeat: od_core::animation::Repeat::Once,
                keys: vec![
                    Key {
                        at_ms: 0,
                        value: 0.0,
                        ease: Ease::Linear,
                    },
                    Key {
                        at_ms: 1000,
                        value: 1.0,
                        ease: Ease::Linear,
                    },
                ],
            },
        ],
    )
    .unwrap();
    (s, video)
}

#[test]
fn animating_trim_end_draws_the_stroke_on_over_time() {
    let (s, video) = animated_line();
    let doc = s.document();
    let v = doc.videos.iter().find(|v| v.id == video).unwrap();
    let mut renderer = od_render::video::VideoRenderer::new(doc, v, None);
    let at = |renderer: &mut od_render::video::VideoRenderer, ms: u64| {
        let p = renderer.frame_at(ms).unwrap();
        let c = |x: u32| {
            let c = p.pixel(x, 20).unwrap();
            [c.red(), c.green(), c.blue(), c.alpha()]
        };
        (c(10), c(50), c(90))
    };
    let (a, b, c) = at(&mut renderer, 0);
    assert!(
        near(a, WHITE, 1) && near(b, WHITE, 1) && near(c, WHITE, 1),
        "t=0 nothing drawn"
    );
    let (a, b, c) = at(&mut renderer, 500);
    assert!(
        near(a, RED, 1) && near(b, WHITE, 6) && near(c, WHITE, 1),
        "t=mid half drawn: {a:?} {b:?} {c:?}"
    );
    let (a, b, c) = at(&mut renderer, 1000);
    assert!(
        near(a, RED, 1) && near(b, RED, 1) && near(c, RED, 1),
        "t=end all drawn"
    );
}

fn bar(sc: &mut Scene) -> NodeId {
    let mut node = path("M0 0 H80 V20 H0 Z", 10.0, 40.0);
    node.fill = solid("#ff0000");
    sc.add(node)
}

fn turn(sc: &mut Scene, id: &NodeId, prop: Property) {
    sc.s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: id.clone(),
            prop,
        }],
    )
    .unwrap();
}

#[test]
fn a_path_node_turns_about_its_center_with_its_rotation() {
    let mut sc = scene(100.0, 100.0);
    let id = bar(&mut sc);
    let upright = sc.render(1.0);
    assert!(near(px(&upright, 20, 50), RED, 1) && near(px(&upright, 50, 15), WHITE, 1));
    turn(&mut sc, &id, Property::Rotation(90.0));
    let turned = sc.render(1.0);
    assert!(near(px(&turned, 50, 15), RED, 1), "the bar now stands up");
    assert!(near(px(&turned, 20, 50), WHITE, 1));
    assert!(near(px(&turned, 50, 50), RED, 1), "about its center");
}

#[test]
fn a_path_node_scales_and_flips_about_its_pivot() {
    let mut sc = scene(200.0, 100.0);
    let id = bar(&mut sc);
    turn(&mut sc, &id, Property::ScaleX(2.0));
    let wide = sc.render(1.0);
    assert!(
        near(px(&wide, 5, 50), RED, 1),
        "grown to the left of its box"
    );
    assert!(near(px(&wide, 105, 50), RED, 1), "and to the right");
    assert!(near(px(&wide, 150, 50), WHITE, 1));
    turn(&mut sc, &id, Property::ScaleX(-1.0));
    assert!(
        near(px(&sc.render(1.0), 20, 50), RED, 1),
        "a flip about the center keeps a symmetric bar in place"
    );
}

#[test]
fn a_gradient_and_a_dashed_stroke_turn_with_their_shape() {
    let mut sc = scene(100.0, 100.0);
    let mut node = path("M0 0 H80 V20 H0 Z", 10.0, 40.0);
    node.fill = Some(Paint::Linear {
        angle: 0.0,
        stops: stops(&[(0.0, "#ff0000"), (1.0, "#0000ff")]),
    });
    let id = sc.add(node);
    turn(&mut sc, &id, Property::Rotation(90.0));
    let r = sc.render(1.0);
    assert!(
        near(px(&r, 50, 12), RED, 24),
        "the red end is on top now: {:?}",
        px(&r, 50, 12)
    );
    assert!(
        near(px(&r, 50, 88), [0, 0, 255], 24),
        "{:?}",
        px(&r, 50, 88)
    );

    let mut sc = scene(100.0, 100.0);
    let mut line = path("M0 0 H80", 10.0, 50.0);
    let mut stroke = Stroke::new(Paint::parse("#ff0000").unwrap(), 6.0);
    stroke.dash = vec![10.0, 10.0];
    stroke.trim_end = 0.5;
    line.stroke = Some(stroke);
    let id = sc.add(line);
    turn(&mut sc, &id, Property::Rotation(90.0));
    let r = sc.render(1.0);
    assert!(
        near(px(&r, 50, 15), RED, 1),
        "the first dash sits at the top now"
    );
    assert!(near(px(&r, 50, 25), WHITE, 1), "then its gap");
    assert!(near(px(&r, 50, 35), RED, 1), "then the second dash");
    assert!(near(px(&r, 50, 80), WHITE, 1), "the trimmed half is gone");
    assert!(
        near(px(&r, 20, 50), WHITE, 1),
        "nothing is left where the line was"
    );
}

#[test]
fn a_group_of_paths_turns_as_one() {
    let mut sc = scene(100.0, 100.0);
    let group = sc.add(NewNode::new(NodeKind::Group));
    for (x, color) in [(10.0, "#ff0000"), (50.0, "#0000ff")] {
        let mut n = path("M0 0 H40 V20 H0 Z", x, 40.0);
        n.fill = solid(color);
        create(&mut sc.s, &group, n);
    }
    turn(&mut sc, &group, Property::Rotation(90.0));
    let r = sc.render(1.0);
    assert!(
        near(px(&r, 50, 20), RED, 1),
        "the left bar went to the top: {:?}",
        px(&r, 50, 20)
    );
    assert!(
        near(px(&r, 50, 80), [0, 0, 255], 1),
        "the right bar went to the bottom: {:?}",
        px(&r, 50, 80)
    );
    assert!(near(px(&r, 20, 50), WHITE, 1));
}

#[test]
fn a_turned_path_is_hit_where_it_is_drawn() {
    let mut sc = scene(100.0, 100.0);
    let id = bar(&mut sc);
    turn(&mut sc, &id, Property::Rotation(90.0));
    let doc = sc.s.document();
    assert_eq!(
        od_core::geometry::hit_test(doc, &sc.frame, 50.0, 15.0),
        Some(id.clone())
    );
    assert_eq!(
        od_core::geometry::hit_test(doc, &sc.frame, 20.0, 50.0),
        Some(sc.frame.clone())
    );
}
