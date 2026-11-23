#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Pixel checks of crisp drawing: nearest-neighbour images (per node and for a whole render) and
//! shapes without anti-aliasing.

use std::collections::BTreeSet;

use od_core::{
    Actor, Document, ImageSmoothing, NewNode, NodeId, NodeKind, Operation, Paint, Property, Session,
};
use od_render::{Media, Rendered, render_frame, render_frame_pixel_art};

const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const YELLOW: [u8; 4] = [255, 255, 0, 255];

struct Scene {
    s: Session,
    frame: NodeId,
    dir: tempfile::TempDir,
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

fn scene(w: f64, h: f64, fill: &str) -> Scene {
    let mut s = Session::new(Document::new("pixel"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = w;
    frame.height = h;
    frame.fill = Some(Paint::parse(fill).unwrap());
    let frame = create(&mut s, &root, frame);
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("media")).unwrap();
    Scene { s, frame, dir }
}

impl Scene {
    fn add(&mut self, node: NewNode) -> NodeId {
        let frame = self.frame.clone();
        create(&mut self.s, &frame, node)
    }

    /// A 2x2 picture of four distinct colours, written as `media/sprite.png`.
    fn add_sprite(&mut self, size: f64) -> NodeId {
        let rgba: Vec<u8> = [RED, GREEN, BLUE, YELLOW].concat();
        let pixmap = od_render::pixmap_from_rgba(2, 2, &rgba).unwrap();
        let png = od_render::encode_png(&pixmap, true).unwrap();
        std::fs::write(self.dir.path().join("media/sprite.png"), png).unwrap();
        let mut node = NewNode::new(NodeKind::Image);
        node.width = size;
        node.height = size;
        node.image = Some("media/sprite.png".into());
        self.add(node)
    }

    fn render(&self, scale: f32) -> Rendered {
        render_frame(self.s.document(), &self.frame, scale, Some(self.dir.path())).unwrap()
    }

    fn render_pixel_art(&self, scale: f32) -> Rendered {
        render_frame_pixel_art(
            self.s.document(),
            &self.frame,
            scale,
            Some(self.dir.path()),
            &mut Media::default(),
        )
        .unwrap()
    }
}

fn colours(r: &Rendered) -> BTreeSet<[u8; 4]> {
    r.pixmap
        .data()
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect()
}

fn px(r: &Rendered, x: u32, y: u32) -> [u8; 4] {
    let c = r.pixmap.pixel(x, y).unwrap();
    [c.red(), c.green(), c.blue(), c.alpha()]
}

fn only_source_colours(r: &Rendered) -> bool {
    let source: BTreeSet<[u8; 4]> = [RED, GREEN, BLUE, YELLOW].into_iter().collect();
    colours(r).is_subset(&source)
}

/// Every 2x2 block of an image drawn at 2x is one colour: the picture was enlarged by copying.
fn blocks_are_flat(r: &Rendered) -> bool {
    (0..r.pixmap.height() / 2).all(|by| {
        (0..r.pixmap.width() / 2).all(|bx| {
            let want = px(r, bx * 2, by * 2);
            [(1, 0), (0, 1), (1, 1)]
                .iter()
                .all(|(dx, dy)| px(r, bx * 2 + dx, by * 2 + dy) == want)
        })
    })
}

#[test]
fn a_smooth_image_enlarged_twice_blends_its_colours() {
    let mut sc = scene(2.0, 2.0, "#000000");
    sc.add_sprite(2.0);
    let r = sc.render(2.0);
    assert_eq!((r.pixmap.width(), r.pixmap.height()), (4, 4));
    assert!(
        !only_source_colours(&r),
        "the default filter must blend, or this suite proves nothing: {:?}",
        colours(&r)
    );
}

#[test]
fn a_nearest_image_enlarged_twice_has_no_intermediate_colours() {
    let mut sc = scene(2.0, 2.0, "#000000");
    let sprite = sc.add_sprite(2.0);
    sc.s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: sprite,
            prop: Property::ImageSmoothing(Some(ImageSmoothing::Nearest)),
        }],
    )
    .unwrap();
    for scale in [2.0, 4.0, 8.0] {
        let r = sc.render(scale);
        assert!(only_source_colours(&r), "scale {scale}: {:?}", colours(&r));
        let k = scale as u32;
        let block = |bx: u32, by: u32| px(&r, bx * k, by * k);
        assert_eq!(
            [block(0, 0), block(1, 0), block(0, 1), block(1, 1)],
            [RED, GREEN, BLUE, YELLOW]
        );
        assert!(
            (0..r.pixmap.height())
                .all(|y| (0..r.pixmap.width()).all(|x| px(&r, x, y) == block(x / k, y / k))),
            "every source pixel is a flat {k}x{k} block"
        );
    }
}

#[test]
fn pixel_art_rendering_makes_every_image_nearest() {
    let mut sc = scene(2.0, 2.0, "#000000");
    sc.add_sprite(2.0);
    let r = sc.render_pixel_art(2.0);
    assert!(only_source_colours(&r), "{:?}", colours(&r));
    assert!(blocks_are_flat(&r));
}

#[test]
fn a_nearest_image_is_not_blurred_when_drawn_smaller_either() {
    let mut sc = scene(1.0, 1.0, "#000000");
    let sprite = sc.add_sprite(1.0);
    sc.s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: sprite,
            prop: Property::ImageSmoothing(Some(ImageSmoothing::Nearest)),
        }],
    )
    .unwrap();
    let r = sc.render(1.0);
    assert!(only_source_colours(&r), "{:?}", colours(&r));
}

fn edge_rect(sc: &mut Scene) {
    let mut rect = NewNode::new(NodeKind::Rect);
    rect.x = 0.5;
    rect.y = 0.5;
    rect.width = 3.0;
    rect.height = 3.0;
    rect.fill = Some(Paint::parse("#000000").unwrap());
    sc.add(rect);
}

#[test]
fn a_shape_on_a_half_pixel_is_anti_aliased_by_default() {
    let mut sc = scene(5.0, 5.0, "#ffffff");
    edge_rect(&mut sc);
    let r = sc.render(1.0);
    assert!(colours(&r).len() > 2, "{:?}", colours(&r));
}

#[test]
fn pixel_art_shapes_have_no_anti_aliasing() {
    let mut sc = scene(5.0, 5.0, "#ffffff");
    edge_rect(&mut sc);
    let r = sc.render_pixel_art(1.0);
    let want: BTreeSet<[u8; 4]> = [[255, 255, 255, 255], [0, 0, 0, 255]].into_iter().collect();
    assert!(colours(&r).is_subset(&want), "{:?}", colours(&r));
    assert!(colours(&r).len() == 2);
}

#[test]
fn pixel_art_circles_and_integer_scaling_stay_crisp() {
    let mut sc = scene(8.0, 8.0, "#ffffff");
    let mut dot = NewNode::new(NodeKind::Ellipse);
    dot.x = 1.0;
    dot.y = 1.0;
    dot.width = 6.0;
    dot.height = 6.0;
    dot.fill = Some(Paint::parse("#ff0000").unwrap());
    sc.add(dot);
    for scale in [1.0, 3.0] {
        let r = sc.render_pixel_art(scale);
        let want: BTreeSet<[u8; 4]> = [[255, 255, 255, 255], RED].into_iter().collect();
        assert!(
            colours(&r).is_subset(&want),
            "scale {scale}: {:?}",
            colours(&r)
        );
    }
}
