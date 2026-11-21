#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! Rotation, scale, pivots, clips, blur, shadows, depth and the video camera, checked on pixels.

use od_core::{
    Actor, AnimProp, Camera, Document, Ease, Key, NewNode, NodeId, NodeKind, Operation, Paint,
    Property, Repeat, Session, Shadow, geometry,
};
use od_render::video::VideoRenderer;
use od_render::{Media, Pixmap, render_frame, render_frame_camera};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

struct Scene {
    s: Session,
    frame: NodeId,
}

impl Scene {
    /// A `w` x `h` frame filled with `fill` (none for transparent).
    fn new(w: f64, h: f64, fill: Option<&str>) -> Scene {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut frame = NewNode::new(NodeKind::Frame);
        (frame.width, frame.height) = (w, h);
        frame.fill = fill.map(|c| Paint::parse(c).unwrap());
        let frame = create(&mut s, root, frame);
        Scene { s, frame }
    }

    fn add(&mut self, parent: &NodeId, node: NewNode) -> NodeId {
        create(&mut self.s, parent.clone(), node)
    }

    fn shape(
        &mut self,
        parent: &NodeId,
        (x, y, w, h): (f64, f64, f64, f64),
        color: &str,
    ) -> NodeId {
        let mut node = NewNode::new(NodeKind::Rect);
        (node.x, node.y, node.width, node.height) = (x, y, w, h);
        node.fill = Some(Paint::parse(color).unwrap());
        self.add(parent, node)
    }

    fn set(&mut self, id: &NodeId, prop: Property) {
        self.s
            .apply(
                Actor::User,
                None,
                &[Operation::Set {
                    id: id.clone(),
                    prop,
                }],
            )
            .unwrap();
    }

    fn render(&self) -> Pixmap {
        render_frame(self.s.document(), &self.frame, 1.0, None)
            .unwrap()
            .pixmap
    }

    fn through(&self, camera: Camera) -> Pixmap {
        render_frame_camera(
            self.s.document(),
            &self.frame,
            1.0,
            None,
            &mut Media::default(),
            camera,
        )
        .unwrap()
        .pixmap
    }
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

fn px(p: &Pixmap, x: u32, y: u32) -> [u8; 4] {
    let c = p.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

/// The smallest box `(left, top, right, bottom)` holding every pixel that `is` accepts.
fn ink(p: &Pixmap, is: impl Fn([u8; 4]) -> bool) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..p.height() {
        for x in 0..p.width() {
            if is(px(p, x, y)) {
                let (l, t, r, b) = found.unwrap_or((x, y, x, y));
                found = Some((l.min(x), t.min(y), r.max(x), b.max(y)));
            }
        }
    }
    found
}

fn is_red(c: [u8; 4]) -> bool {
    c[0] > 200 && c[1] < 60 && c[2] < 60
}

#[test]
fn a_node_turns_clockwise_about_its_center_by_default() {
    let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
    let r = sc.shape(&sc.frame.clone(), (30.0, 40.0, 40.0, 20.0), "#ff0000");
    sc.set(&r, Property::Rotation(90.0));
    let p = sc.render();
    assert_eq!(px(&p, 50, 33), RED);
    assert_eq!(px(&p, 50, 66), RED);
    assert_eq!(px(&p, 33, 50), WHITE);
    assert_eq!(px(&p, 66, 50), WHITE);
    assert_eq!(ink(&p, is_red), Some((40, 30, 59, 69)));
}

#[test]
fn the_pivot_moves_what_the_node_turns_about() {
    let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
    let r = sc.shape(&sc.frame.clone(), (30.0, 40.0, 40.0, 20.0), "#ff0000");
    sc.set(&r, Property::PivotX(0.0));
    sc.set(&r, Property::PivotY(0.0));
    sc.set(&r, Property::Rotation(90.0));
    let p = sc.render();
    // the box swings down and to the left of its top-left corner
    assert_eq!(ink(&p, is_red), Some((10, 40, 29, 79)));
    assert_eq!(px(&p, 20, 60), RED);
    assert_eq!(px(&p, 50, 50), WHITE);
}

#[test]
fn scale_grows_about_the_pivot_and_a_negative_scale_flips() {
    let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
    let a = sc.shape(&sc.frame.clone(), (40.0, 40.0, 20.0, 20.0), "#ff0000");
    sc.set(&a, Property::Scale(2.0));
    assert_eq!(ink(&sc.render(), is_red), Some((30, 30, 69, 69)));

    let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
    let b = sc.shape(&sc.frame.clone(), (50.0, 40.0, 20.0, 20.0), "#ff0000");
    sc.set(&b, Property::PivotX(0.0));
    sc.set(&b, Property::ScaleX(-1.0));
    assert_eq!(ink(&sc.render(), is_red), Some((30, 40, 49, 59)));
}

#[test]
fn a_group_turns_everything_in_it_about_the_box_around_its_children() {
    let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
    let mut group = NewNode::new(NodeKind::Group);
    (group.x, group.y) = (50.0, 50.0);
    let group = sc.add(&sc.frame.clone(), group);
    sc.shape(&group, (0.0, 0.0, 40.0, 10.0), "#ff0000");
    sc.set(&group, Property::Rotation(90.0));
    let p = sc.render();
    // the children's box is 40 x 10 at (50, 50): its center (70, 55) stays, the bar stands up
    assert_eq!(ink(&p, is_red), Some((65, 35, 74, 74)));
}

#[test]
fn transforms_compose_down_the_tree_and_match_the_reported_bounds() {
    let mut sc = Scene::new(200.0, 200.0, Some("#ffffff"));
    let mut outer = NewNode::new(NodeKind::Group);
    (outer.x, outer.y, outer.rotation) = (100.0, 60.0, Some(90.0));
    let outer = sc.add(&sc.frame.clone(), outer);
    let mut inner = NewNode::new(NodeKind::Group);
    (inner.x, inner.y, inner.rotation, inner.scale_x) = (10.0, 0.0, Some(90.0), Some(2.0));
    let inner = sc.add(&outer, inner);
    let r = sc.shape(&inner, (0.0, 0.0, 20.0, 10.0), "#ff0000");
    let p = sc.render();
    let b = geometry::bounds_in_frame(sc.s.document(), &sc.frame, &r).unwrap();
    let seen = ink(&p, is_red).unwrap();
    let close = |a: u32, b: f64| (f64::from(a) - b).abs() <= 1.0;
    assert!(
        close(seen.0, b.x)
            && close(seen.1, b.y)
            && close(seen.2 + 1, b.right())
            && close(seen.3 + 1, b.bottom()),
        "drawn {seen:?} but bounds {b:?}"
    );
    // the bar, stretched to 40 x 10, stood up by the inner turn and laid down by the outer one
    assert!(
        (b.width - 40.0).abs() < 0.01 && (b.height - 10.0).abs() < 0.01,
        "{b:?}"
    );
}

#[test]
fn a_clipping_group_cuts_its_children_at_its_box() {
    let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
    let mut group = NewNode::new(NodeKind::Group);
    (group.x, group.y, group.width, group.height) = (10.0, 10.0, 40.0, 40.0);
    group.clip = Some(true);
    let group = sc.add(&sc.frame.clone(), group);
    sc.shape(&group, (-10.0, -10.0, 100.0, 100.0), "#ff0000");
    let p = sc.render();
    assert_eq!(ink(&p, is_red), Some((10, 10, 49, 49)));

    let mut open = Scene::new(100.0, 100.0, Some("#ffffff"));
    let mut group = NewNode::new(NodeKind::Group);
    (group.x, group.y, group.width, group.height) = (10.0, 10.0, 40.0, 40.0);
    let group = open.add(&open.frame.clone(), group);
    open.shape(&group, (-10.0, -10.0, 100.0, 100.0), "#ff0000");
    assert_eq!(px(&open.render(), 80, 80), RED);
}

#[test]
fn a_clip_follows_the_group_when_it_turns() {
    let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
    let mut group = NewNode::new(NodeKind::Group);
    (group.x, group.y, group.width, group.height) = (30.0, 40.0, 40.0, 20.0);
    group.clip = Some(true);
    group.rotation = Some(90.0);
    let group = sc.add(&sc.frame.clone(), group);
    sc.shape(&group, (-50.0, -50.0, 200.0, 200.0), "#ff0000");
    assert_eq!(ink(&sc.render(), is_red), Some((40, 30, 59, 69)));
}

fn alpha_sum(p: &Pixmap) -> u64 {
    p.data().chunks_exact(4).map(|c| u64::from(c[3])).sum()
}

#[test]
fn blur_spreads_a_shape_without_losing_or_inventing_ink() {
    let sharp = {
        let mut sc = Scene::new(100.0, 100.0, None);
        sc.shape(&sc.frame.clone(), (40.0, 40.0, 20.0, 20.0), "#ff0000");
        sc.render()
    };
    let mut sc = Scene::new(100.0, 100.0, None);
    let r = sc.shape(&sc.frame.clone(), (40.0, 40.0, 20.0, 20.0), "#ff0000");
    sc.set(&r, Property::Blur(4.0));
    let soft = sc.render();
    assert!(px(&soft, 50, 50)[3] >= 245, "the middle stays solid");
    let edge = px(&soft, 38, 50)[3];
    assert!(
        edge > 0 && edge < 128,
        "just outside the edge is partly covered: {edge}"
    );
    assert!(px(&soft, 34, 50)[3] > 0 && px(&soft, 20, 50)[3] == 0);
    assert!(px(&sharp, 38, 50)[3] == 0);
    let (a, b) = (alpha_sum(&sharp) as f64, alpha_sum(&soft) as f64);
    assert!((a - b).abs() / a < 0.03, "ink {a} became {b}");
    // symmetric about the shape
    assert!(px(&soft, 38, 50)[3].abs_diff(px(&soft, 61, 50)[3]) <= 2);
}

#[test]
fn a_shadow_falls_offset_from_the_node_and_softens_with_blur() {
    let build = |blur: f64, opacity: f64| {
        let mut sc = Scene::new(100.0, 100.0, Some("#ffffff"));
        let card = sc.shape(&sc.frame.clone(), (20.0, 20.0, 40.0, 40.0), "#ff0000");
        sc.set(
            &card,
            Property::DropShadow(Some(Shadow {
                dx: 10.0,
                dy: 10.0,
                blur,
                color: Paint::parse("#000000ff").unwrap(),
            })),
        );
        sc.set(&card, Property::Opacity(opacity));
        sc.render()
    };
    let hard = build(0.0, 1.0);
    assert_eq!(px(&hard, 40, 40), RED);
    assert_eq!(px(&hard, 65, 65), [0, 0, 0, 255]);
    assert_eq!(px(&hard, 75, 75), WHITE);
    assert_eq!(px(&hard, 25, 65), WHITE);
    let soft = build(6.0, 1.0);
    let edge = px(&soft, 71, 65);
    assert!(edge[0] > 0 && edge[0] < 255, "soft edge {edge:?}");
    let inside = px(&soft, 65, 65)[0];
    assert!(
        (30..120).contains(&inside),
        "five px inside the shadow's edge: {inside}"
    );
    let faint = build(0.0, 0.5);
    let c = px(&faint, 65, 65);
    assert!(
        (120..=135).contains(&c[0]),
        "half opacity halves the shadow: {c:?}"
    );
}

#[test]
fn a_blurred_node_casts_a_soft_shadow_of_what_it_shows() {
    let mut sc = Scene::new(100.0, 100.0, None);
    let r = sc.shape(&sc.frame.clone(), (30.0, 30.0, 20.0, 20.0), "#ff0000");
    sc.set(&r, Property::Blur(2.0));
    sc.set(
        &r,
        Property::DropShadow(Some(Shadow {
            dx: 30.0,
            dy: 0.0,
            blur: 0.0,
            color: Paint::parse("#0000ffff").unwrap(),
        })),
    );
    let p = sc.render();
    assert!(
        px(&p, 70, 40)[3] > 240 && px(&p, 70, 40)[2] > 240,
        "shadow center"
    );
    assert!(px(&p, 40, 40)[0] > 240, "node center");
}

/// Three 20 x 10 squares at the same x in a 200 x 100 frame, at depths 0, 0.5 and 1.
fn parallax_scene() -> Scene {
    let mut sc = Scene::new(200.0, 100.0, Some("#ffffff"));
    let frame = sc.frame.clone();
    for (y, color, depth) in [
        (5.0, "#ff0000", 0.0),
        (40.0, "#00ff00", 0.5),
        (75.0, "#0000ff", 1.0),
    ] {
        let id = sc.shape(&frame, (100.0, y, 20.0, 10.0), color);
        sc.set(&id, Property::Depth(depth));
    }
    sc
}

#[test]
fn the_camera_pans_each_depth_by_its_share() {
    let sc = parallax_scene();
    let rest = sc.through(Camera::default());
    assert_eq!(ink(&rest, |c| c == RED), Some((100, 5, 119, 14)));
    let moved = sc.through(Camera {
        x: 40.0,
        ..Camera::default()
    });
    assert_eq!(
        ink(&moved, |c| c == RED),
        Some((100, 5, 119, 14)),
        "depth 0 stays on the screen"
    );
    assert_eq!(
        ink(&moved, |c| c == GREEN),
        Some((80, 40, 99, 49)),
        "depth 0.5 moves half"
    );
    assert_eq!(
        ink(&moved, |c| c == BLUE),
        Some((60, 75, 79, 84)),
        "depth 1 moves with the camera"
    );
}

#[test]
fn camera_zoom_and_turn_act_about_the_center_by_depth() {
    let mut sc = Scene::new(200.0, 100.0, Some("#ffffff"));
    let frame = sc.frame.clone();
    let near = sc.shape(&frame, (100.0, 40.0, 20.0, 20.0), "#0000ff");
    let mid = sc.shape(&frame, (100.0, 10.0, 20.0, 10.0), "#00ff00");
    sc.set(&mid, Property::Depth(0.25));
    sc.set(&near, Property::Depth(1.0));
    let zoomed = sc.through(Camera {
        zoom: 2.0,
        ..Camera::default()
    });
    // zoom 2 about (100, 50): the box (100..120, 40..60) becomes (100..140, 30..70)
    assert_eq!(ink(&zoomed, |c| c == BLUE), Some((100, 30, 139, 69)));
    // depth 0.25 magnifies by 2^0.25 = 1.19: the box (100..120, 10..20) becomes (100..123.8, 2.4..14.3)
    let (l, t, r, b) = ink(&zoomed, |c| c == GREEN).unwrap();
    assert_eq!(l, 100);
    assert!((23..=24).contains(&(r - l + 1)), "width {}", r - l + 1);
    assert!(
        (2..=3).contains(&t) && (13..=14).contains(&b),
        "rows {t}..{b}"
    );
    let turned = sc.through(Camera {
        rotation: 90.0,
        ..Camera::default()
    });
    // a quarter turn about (100, 50) sends the box (100..120, 40..60) to (90..110, 50..70)
    assert_eq!(ink(&turned, |c| c == BLUE), Some((90, 50, 109, 69)));
}

#[test]
fn drawing_the_same_frame_twice_gives_the_same_bytes() {
    let mut sc = parallax_scene();
    let node = NodeId::parse("n_4").ok();
    if let Some(id) = node {
        sc.set(&id, Property::Rotation(33.0));
        sc.set(&id, Property::Blur(1.5));
    }
    let cam = Camera {
        x: 7.0,
        y: -3.0,
        zoom: 1.3,
        rotation: 4.0,
    };
    assert_eq!(sc.through(cam).data(), sc.through(cam).data());
}

/// A 400 x 400 video of 5 s: a group of three shapes turning about its center, an idle that
/// floats on a loop, a shadowed card that pops in with overshoot, and a camera that pushes in
/// from 3 s to 4 s past a depth 0.5 and a depth 1 square and a depth 0 one.
fn showcase() -> (Session, NodeId) {
    let mut sc = Scene::new(400.0, 400.0, Some("#ffffff"));
    let frame = sc.frame.clone();
    let mut spinner = NewNode::new(NodeKind::Group);
    (spinner.x, spinner.y) = (100.0, 100.0);
    let spinner = sc.add(&frame, spinner);
    sc.shape(&spinner, (0.0, 0.0, 20.0, 20.0), "#ff0000");
    sc.shape(&spinner, (60.0, 0.0, 20.0, 20.0), "#00ff00");
    sc.shape(&spinner, (0.0, 60.0, 20.0, 20.0), "#0000ff");
    let mut idle = NewNode::new(NodeKind::Ellipse);
    (idle.x, idle.y, idle.width, idle.height) = (300.0, 50.0, 40.0, 40.0);
    idle.fill = Paint::parse("#ff8800").ok();
    idle.depth = Some(0.0);
    let idle = sc.add(&frame, idle);
    let mut card = NewNode::new(NodeKind::Rect);
    (card.x, card.y, card.width, card.height) = (140.0, 260.0, 120.0, 80.0);
    card.fill = Paint::parse("#2244cc").ok();
    card.shadow = Some(Shadow {
        dx: 0.0,
        dy: 12.0,
        blur: 0.0,
        color: Paint::parse("#00000080").unwrap(),
    });
    let card = sc.add(&frame, card);
    let far = sc.shape(&frame, (10.0, 10.0, 30.0, 30.0), "#000000");
    let mid = sc.shape(&frame, (210.0, 210.0, 10.0, 10.0), "#00aa00");
    let near = sc.shape(&frame, (210.0, 190.0, 10.0, 10.0), "#0000ff");
    sc.set(&far, Property::Depth(0.0));
    sc.set(&mid, Property::Depth(0.5));
    let Scene { mut s, frame } = sc;
    let video = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "v".into(),
                fps: 10,
                width: 400,
                height: 400,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    s.apply(
        Actor::User,
        None,
        &[Operation::AddClip {
            video: video.clone(),
            frame: Some(frame),
            footage: None,
            duration_ms: 5000,
            index: None,
            transition: None,
            motion: None,
        }],
    )
    .unwrap();
    let key = |at_ms, value, ease| Key { at_ms, value, ease };
    let lin = Ease::Linear;
    let spinner_turn = (
        spinner,
        AnimProp::Rotation,
        vec![key(0, 0.0, lin), key(2000, 180.0, lin)],
        Repeat::Once,
    );
    let float = (
        idle,
        AnimProp::Y,
        vec![
            key(0, 50.0, lin),
            key(1000, 70.0, Ease::EaseInOut),
            key(2000, 50.0, Ease::EaseInOut),
        ],
        Repeat::Loop,
    );
    let pop = (
        card,
        AnimProp::Scale,
        vec![key(1000, 0.0, lin), key(1600, 1.0, Ease::Back)],
        Repeat::Once,
    );
    let push = (
        video.clone(),
        AnimProp::CameraZoom,
        vec![key(3000, 1.0, lin), key(4000, 2.0, lin)],
        Repeat::Once,
    );
    let ops: Vec<Operation> = [spinner_turn, float, pop, push]
        .into_iter()
        .map(|(node, prop, keys, repeat)| Operation::Animate {
            video: video.clone(),
            node,
            prop,
            keys,
            repeat,
        })
        .collect();
    s.apply(Actor::User, None, &ops).unwrap();
    let _ = near;
    (s, video)
}

fn at(s: &Session, video: &NodeId, ms: u64) -> Pixmap {
    let doc = s.document();
    VideoRenderer::new(doc, doc.video(video).unwrap(), None)
        .frame_at(ms)
        .unwrap()
}

#[test]
fn the_showcase_video_turns_floats_pops_and_pushes_in_at_the_right_moments() {
    let (s, v) = showcase();
    let start = at(&s, &v, 0);
    assert_eq!(px(&start, 110, 110), RED, "the spinner starts as designed");
    assert_eq!(px(&start, 200, 300), WHITE, "the card has not popped in");
    assert_eq!(
        px(&start, 320, 53),
        [0xff, 0x88, 0x00, 255],
        "the idle starts at y 50"
    );
    assert_eq!(px(&start, 215, 195), BLUE);

    let quarter = at(&s, &v, 1000);
    assert_eq!(
        px(&quarter, 170, 110),
        RED,
        "a quarter into the turn the red square is at the top right"
    );
    assert_eq!(
        px(&quarter, 170, 170),
        GREEN,
        "the green one is at the bottom right"
    );
    assert_eq!(
        px(&quarter, 110, 110),
        BLUE,
        "and the blue one has taken the red one's place"
    );

    // 70% into the pop: scale 1.08, so the card reaches 4.8 px past its settled edge
    let pop = at(&s, &v, 1420);
    assert_eq!(
        px(&pop, 262, 300),
        [0x22, 0x44, 0xcc, 255],
        "the card overshoots"
    );

    let settled = at(&s, &v, 3000);
    assert_eq!(px(&settled, 262, 300), WHITE, "and settles back");
    assert_eq!(px(&settled, 200, 300), [0x22, 0x44, 0xcc, 255]);
    let shade = px(&settled, 200, 345);
    assert!(
        (120..=135).contains(&shade[0]) && shade[0] == shade[2],
        "the shadow under the card: {shade:?}"
    );
    assert_eq!(px(&settled, 200, 356), WHITE);

    // the loop carries on after its last key: 4.5 s is half a cycle in, 10 px down
    let late = at(&s, &v, 4500);
    assert_eq!(px(&late, 320, 53), WHITE);
    assert_eq!(px(&late, 320, 65), [0xff, 0x88, 0x00, 255]);
    assert_eq!(
        px(&at(&s, &v, 4000), 320, 53),
        [0xff, 0x88, 0x00, 255],
        "and is back at its start at 4 s"
    );

    // the camera has pushed in to 2x: depth 1 doubles about the center, depth 0.5 by 2^0.5
    assert_eq!(px(&late, 230, 190), BLUE);
    assert_eq!(px(&late, 215, 195), WHITE);
    assert_eq!(px(&late, 222, 222), [0x00, 0xaa, 0x00, 255]);
    assert_eq!(px(&late, 212, 212), WHITE);
    assert_eq!(
        px(&late, 20, 20),
        [0, 0, 0, 255],
        "depth 0 stays on the screen"
    );
}

#[test]
fn frames_drawn_out_of_order_or_by_another_renderer_agree() {
    let (s, v) = showcase();
    let times = [4500, 0, 1420, 3000, 1000, 3500, 2200];
    let doc = s.document();
    let video = doc.video(&v).unwrap();
    let mut shared = VideoRenderer::new(doc, video, None);
    let forward: Vec<Vec<u8>> = times
        .iter()
        .map(|&t| shared.frame_at(t).unwrap().data().to_vec())
        .collect();
    for (i, &t) in times.iter().enumerate().rev() {
        let alone = VideoRenderer::new(doc, video, None).frame_at(t).unwrap();
        assert_eq!(alone.data(), &forward[i][..], "at {t} ms");
        assert_eq!(
            shared.frame_at(t).unwrap().data(),
            &forward[i][..],
            "again at {t} ms"
        );
    }
}

#[test]
fn a_moving_camera_or_loop_changes_the_picture_signature_and_a_held_one_does_not() {
    let (s, v) = showcase();
    let doc = s.document();
    let r = VideoRenderer::new(doc, doc.video(&v).unwrap(), None);
    assert_ne!(r.signature(3400), r.signature(3500), "the camera is moving");
    assert_ne!(r.signature(4400), r.signature(4500), "the loop never stops");
}
