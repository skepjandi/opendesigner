#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{
    Actor, AnimProp, Camera, Document, Ease, Key, NewNode, NodeId, NodeKind, OpError, Operation,
    Paint, Property, Repeat, Session, Shadow, project,
};

fn apply(s: &mut Session, ops: &[Operation]) -> Result<od_core::TxReceipt, OpError> {
    s.apply(Actor::Agent("t".into()), None, ops)
}

fn set(id: &NodeId, prop: Property) -> Operation {
    Operation::Set {
        id: id.clone(),
        prop,
    }
}

struct Fixture {
    s: Session,
    frame: NodeId,
    rect: NodeId,
    group: NodeId,
    video: NodeId,
}

/// A frame holding a rect and an (unsized) group, and a 30 fps video of it.
fn fixture() -> Fixture {
    fixture_in(Session::new(Document::new("t")))
}

fn fixture_in(mut s: Session) -> Fixture {
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    (frame.width, frame.height) = (400.0, 300.0);
    let mut rect = NewNode::new(NodeKind::Rect);
    (rect.width, rect.height) = (100.0, 50.0);
    let group = NewNode::new(NodeKind::Group);
    frame.children = vec![rect, group];
    let r = apply(
        &mut s,
        &[
            Operation::Create {
                parent: root,
                index: None,
                node: frame,
            },
            Operation::CreateVideo {
                name: "v".into(),
                fps: 30,
                width: 400,
                height: 300,
            },
        ],
    )
    .unwrap();
    let frame = r.created[0].clone();
    let kids = s.document().node(&frame).unwrap().children.clone();
    Fixture {
        frame,
        rect: kids[0].id.clone(),
        group: kids[1].id.clone(),
        video: r.created[1].clone(),
        s,
    }
}

fn key(at_ms: u64, value: f64, ease: Ease) -> Key {
    Key { at_ms, value, ease }
}

fn animate(
    video: &NodeId,
    node: &NodeId,
    prop: AnimProp,
    keys: Vec<Key>,
    repeat: Repeat,
) -> Operation {
    Operation::Animate {
        video: video.clone(),
        node: node.clone(),
        prop,
        keys,
        repeat,
    }
}

#[test]
fn a_node_that_uses_none_of_it_serializes_without_the_new_fields() {
    let f = fixture();
    let text = serde_json::to_string(f.s.document().node(&f.rect).unwrap()).unwrap();
    for field in [
        "rotation", "scale_x", "scale_y", "pivot_x", "pivot_y", "blur", "shadow", "clip", "depth",
    ] {
        assert!(!text.contains(field), "{field} in {text}");
    }
    let video = serde_json::to_string(f.s.document().video(&f.video).unwrap()).unwrap();
    assert!(!video.contains("repeat"));
}

#[test]
fn every_property_sets_validates_and_undoes() {
    let mut f = fixture();
    let shadow = Shadow {
        dx: 4.0,
        dy: 8.0,
        blur: 12.0,
        color: Paint::parse("#00000066").unwrap(),
    };
    let sets = [
        Property::Rotation(33.5),
        Property::ScaleX(-1.0),
        Property::ScaleY(2.0),
        Property::PivotX(0.0),
        Property::PivotY(1.0),
        Property::Blur(3.0),
        Property::DropShadow(Some(shadow.clone())),
        Property::Depth(0.5),
    ];
    for prop in sets {
        apply(&mut f.s, &[set(&f.rect, prop)]).unwrap();
    }
    let n = f.s.document().node(&f.rect).unwrap().clone();
    assert_eq!((n.rotation, n.scale_x, n.scale_y), (33.5, -1.0, 2.0));
    assert_eq!(
        (n.pivot_x, n.pivot_y, n.blur, n.depth),
        (0.0, 1.0, 3.0, 0.5)
    );
    assert_eq!(n.shadow, Some(shadow));
    apply(&mut f.s, &[set(&f.rect, Property::ShadowDy(20.0))]).unwrap();
    assert_eq!(
        f.s.document()
            .node(&f.rect)
            .unwrap()
            .shadow
            .as_ref()
            .unwrap()
            .dy,
        20.0
    );
    for _ in 0..9 {
        f.s.undo(Actor::User).unwrap();
    }
    let n = f.s.document().node(&f.rect).unwrap();
    assert!(!n.paints_beyond_its_box() && n.shadow.is_none() && n.pivot_x == 0.5 && n.depth == 1.0);
    f.s.redo(Actor::User).unwrap();
    assert_eq!(f.s.document().node(&f.rect).unwrap().rotation, 33.5);
}

#[test]
fn scale_sets_both_and_undoes_to_each_axis_as_it_was() {
    let mut f = fixture();
    apply(
        &mut f.s,
        &[set(&f.rect, Property::ScaleXy { x: 2.0, y: 3.0 })],
    )
    .unwrap();
    let receipt = apply(&mut f.s, &[set(&f.rect, Property::Scale(0.5))]).unwrap();
    assert!(matches!(
        receipt.ops[0],
        Operation::Set {
            prop: Property::Scale(_),
            ..
        }
    ));
    let n = f.s.document().node(&f.rect).unwrap();
    assert_eq!((n.scale_x, n.scale_y), (0.5, 0.5));
    f.s.undo(Actor::User).unwrap();
    let n = f.s.document().node(&f.rect).unwrap();
    assert_eq!((n.scale_x, n.scale_y), (2.0, 3.0));
}

#[test]
fn out_of_range_and_inapplicable_values_are_refused_and_change_nothing() {
    let mut f = fixture();
    let before = f.s.document().clone();
    for prop in [
        Property::Rotation(f64::NAN),
        Property::Rotation(1.0e9),
        Property::ScaleX(1000.0),
        Property::Blur(-1.0),
        Property::Blur(10_000.0),
        Property::Depth(-0.1),
        Property::PivotX(f64::INFINITY),
        Property::ShadowDx(1.0),
        Property::DropShadow(Some(Shadow {
            dx: 1.0e9,
            dy: 0.0,
            blur: 0.0,
            color: Paint::parse("#000000").unwrap(),
        })),
        Property::DropShadow(Some(Shadow {
            dx: 0.0,
            dy: 0.0,
            blur: 0.0,
            color: Paint::parse("{no.such.token}").unwrap(),
        })),
        Property::Clip(true),
    ] {
        assert!(
            apply(&mut f.s, &[set(&f.rect, prop.clone())]).is_err(),
            "{prop:?}"
        );
    }
    assert_eq!(f.s.document(), &before);
    apply(&mut f.s, &[set(&f.group, Property::Clip(true))]).unwrap();
}

#[test]
fn nodes_are_created_with_a_transform_and_effects() {
    let mut f = fixture();
    let mut node = NewNode::new(NodeKind::Group);
    node.rotation = Some(10.0);
    node.scale_x = Some(1.5);
    node.clip = Some(true);
    node.shadow = Some(Shadow {
        dx: 1.0,
        dy: 2.0,
        blur: 3.0,
        color: Paint::parse("#112233").unwrap(),
    });
    node.depth = Some(0.0);
    let created = apply(
        &mut f.s,
        &[Operation::Create {
            parent: f.frame.clone(),
            index: None,
            node,
        }],
    )
    .unwrap()
    .created[0]
        .clone();
    let n = f.s.document().node(&created).unwrap();
    assert_eq!(
        (n.rotation, n.scale_x, n.clip, n.depth),
        (10.0, 1.5, true, 0.0)
    );
    assert_eq!(n.shadow.as_ref().unwrap().blur, 3.0);
    let mut bad = NewNode::new(NodeKind::Rect);
    bad.clip = Some(true);
    assert!(
        apply(
            &mut f.s,
            &[Operation::Create {
                parent: f.frame.clone(),
                index: None,
                node: bad
            }]
        )
        .is_err()
    );
}

#[test]
fn an_agent_writes_these_as_json() {
    let op: Operation = serde_json::from_str(
        r##"{"op":"set","id":"n_2","prop":{"name":"drop_shadow","value":{"dy":10,"blur":20}}}"##,
    )
    .unwrap();
    match op {
        Operation::Set {
            prop: Property::DropShadow(Some(s)),
            ..
        } => {
            assert_eq!((s.dx, s.dy, s.blur), (0.0, 10.0, 20.0));
            assert_eq!(s.color, Paint::parse("#00000040").unwrap());
        }
        other => panic!("{other:?}"),
    }
    let op: Operation = serde_json::from_str(
        r#"{"op":"animate","video":"v_1","node":"v_1","prop":"camera_zoom","keys":[{"at_ms":0,"value":1},{"at_ms":1000,"value":1.4,"ease":"ease_out_back"}],"repeat":"pingpong"}"#,
    )
    .unwrap();
    assert!(matches!(
        op,
        Operation::Animate {
            prop: AnimProp::CameraZoom,
            repeat: Repeat::Pingpong,
            ..
        }
    ));
}

#[test]
fn a_looping_track_keeps_cycling_in_the_document_at_any_time() {
    let mut f = fixture();
    let keys = vec![
        key(0, 0.0, Ease::Linear),
        key(1000, 20.0, Ease::Linear),
        key(2000, 0.0, Ease::Linear),
    ];
    apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.rect,
            AnimProp::Rotation,
            keys,
            Repeat::Loop,
        )],
    )
    .unwrap();
    let doc = f.s.document();
    let video = doc.video(&f.video).unwrap();
    let rotation_at = |ms: f64| {
        doc.at_time(video, ms)
            .and_then(|d| d.node(&f.rect).map(|n| n.rotation))
            .unwrap()
    };
    assert_eq!(rotation_at(500.0), 10.0);
    assert_eq!(rotation_at(2500.0), 10.0);
    assert_eq!(rotation_at(60_000.0 + 1000.0), 20.0);
    assert_eq!(rotation_at(2000.0), 0.0);
    f.s.undo(Actor::User).unwrap();
    assert!(
        f.s.document()
            .video(&f.video)
            .unwrap()
            .animations
            .is_empty()
    );
}

#[test]
fn the_repeat_mode_is_part_of_the_track_and_of_its_undo() {
    let mut f = fixture();
    let keys = vec![key(0, 1.0, Ease::Linear), key(1000, 2.0, Ease::Linear)];
    apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.rect,
            AnimProp::ScaleX,
            keys.clone(),
            Repeat::Loop,
        )],
    )
    .unwrap();
    apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.rect,
            AnimProp::ScaleX,
            keys,
            Repeat::Once,
        )],
    )
    .unwrap();
    let repeat = |s: &Session| s.document().video(&f.video).unwrap().animations[0].repeat;
    assert_eq!(repeat(&f.s), Repeat::Once);
    f.s.undo(Actor::User).unwrap();
    assert_eq!(repeat(&f.s), Repeat::Loop);
}

#[test]
fn the_camera_is_keyframed_on_the_video_itself() {
    let mut f = fixture();
    let keys = vec![key(0, 1.0, Ease::Linear), key(2000, 1.5, Ease::EaseOut)];
    apply(
        &mut f.s,
        &[
            animate(
                &f.video,
                &f.video,
                AnimProp::CameraZoom,
                keys.clone(),
                Repeat::Once,
            ),
            animate(
                &f.video,
                &f.video,
                AnimProp::CameraX,
                vec![key(0, 0.0, Ease::Linear), key(2000, 80.0, Ease::Linear)],
                Repeat::Once,
            ),
        ],
    )
    .unwrap();
    let video = f.s.document().video(&f.video).unwrap();
    let cam = video.camera_at(1000.0);
    assert_eq!(cam.x, 40.0);
    assert!(cam.zoom > 1.25 && cam.zoom < 1.5);
    assert_eq!(
        video.camera_at(99_000.0),
        Camera {
            x: 80.0,
            y: 0.0,
            zoom: 1.5,
            rotation: 0.0
        }
    );
    assert!(video.has_camera());

    // a camera property needs the video as its node, and a node property a node
    assert!(
        apply(
            &mut f.s,
            &[animate(
                &f.video,
                &f.rect,
                AnimProp::CameraZoom,
                keys.clone(),
                Repeat::Once
            )]
        )
        .is_err()
    );
    assert!(
        apply(
            &mut f.s,
            &[animate(
                &f.video,
                &f.video,
                AnimProp::Rotation,
                keys.clone(),
                Repeat::Once
            )]
        )
        .is_err()
    );
    let too_far = vec![key(0, 50.0, Ease::Linear)];
    assert!(
        apply(
            &mut f.s,
            &[animate(
                &f.video,
                &f.video,
                AnimProp::CameraZoom,
                too_far,
                Repeat::Once
            )]
        )
        .is_err()
    );
}

#[test]
fn shadow_numbers_can_only_be_keyframed_once_there_is_a_shadow() {
    let mut f = fixture();
    let keys = vec![key(0, 0.0, Ease::Linear), key(1000, 30.0, Ease::Back)];
    let no_shadow = apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.rect,
            AnimProp::ShadowDy,
            keys.clone(),
            Repeat::Once,
        )],
    );
    assert!(no_shadow.is_err());
    apply(
        &mut f.s,
        &[set(
            &f.rect,
            Property::DropShadow(Some(Shadow {
                dx: 0.0,
                dy: 5.0,
                blur: 0.0,
                color: Paint::parse("#000000").unwrap(),
            })),
        )],
    )
    .unwrap();
    apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.rect,
            AnimProp::ShadowDy,
            keys,
            Repeat::Once,
        )],
    )
    .unwrap();
    let doc = f.s.document();
    let moved = doc.at_time(doc.video(&f.video).unwrap(), 1000.0).unwrap();
    assert_eq!(
        moved.node(&f.rect).unwrap().shadow.as_ref().unwrap().dy,
        30.0
    );
}

#[test]
fn a_project_using_all_of_it_loads_equal_and_resaves_byte_identically() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let mut f = fixture_in(project::create(&dir, "t").unwrap());
    let ops = [
        set(&f.rect, Property::Rotation(12.0)),
        set(&f.rect, Property::PivotX(0.25)),
        set(&f.rect, Property::Blur(2.0)),
        set(
            &f.rect,
            Property::DropShadow(Some(Shadow {
                dx: 1.0,
                dy: 2.0,
                blur: 3.0,
                color: Paint::parse("#00000080").unwrap(),
            })),
        ),
        set(&f.group, Property::Clip(true)),
        set(&f.group, Property::Width(50.0)),
        set(&f.group, Property::Height(40.0)),
        set(&f.group, Property::Depth(0.25)),
        animate(
            &f.video,
            &f.rect,
            AnimProp::Rotation,
            vec![key(0, 0.0, Ease::Linear), key(1000, 90.0, Ease::Elastic)],
            Repeat::Pingpong,
        ),
        animate(
            &f.video,
            &f.video,
            AnimProp::CameraRotation,
            vec![key(0, 0.0, Ease::Linear), key(1000, 5.0, Ease::Bounce)],
            Repeat::Once,
        ),
    ];
    apply(&mut f.s, &ops).unwrap();
    project::save(&dir, &f.s).unwrap();
    let first = snapshot(&dir);
    let loaded = project::load(&dir).unwrap();
    assert_eq!(loaded.document(), f.s.document());
    project::save(&dir, &loaded).unwrap();
    assert_eq!(snapshot(&dir), first);
    let all = String::from_utf8(first.iter().flat_map(|(_, b)| b.clone()).collect()).unwrap();
    for needle in [
        "\"rotation\"",
        "\"pivot_x\"",
        "\"shadow\"",
        "\"clip\"",
        "\"repeat\":\"pingpong\"",
        "camera_rotation",
        "\"bounce\"",
    ] {
        assert!(all.contains(needle), "{needle} missing");
    }
}

fn snapshot(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap().display().to_string();
                out.push((rel, std::fs::read(&p).unwrap()));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    files
}

mod geometry_and_lint {
    use super::*;
    use od_core::geometry;
    use od_core::lint::{LintOptions, lint};

    fn rotated(f: &mut Fixture, degrees: f64) {
        apply(&mut f.s, &[set(&f.rect, Property::X(100.0))]).unwrap();
        apply(&mut f.s, &[set(&f.rect, Property::Y(100.0))]).unwrap();
        apply(&mut f.s, &[set(&f.rect, Property::Rotation(degrees))]).unwrap();
    }

    #[test]
    fn bounds_are_the_box_around_the_turned_node() {
        let mut f = fixture();
        rotated(&mut f, 90.0);
        // a 100 x 50 box at (100, 100) turned about its center (150, 125)
        let b = geometry::bounds_in_frame(f.s.document(), &f.frame, &f.rect).unwrap();
        for (got, want) in [
            (b.x, 125.0),
            (b.y, 75.0),
            (b.width, 50.0),
            (b.height, 100.0),
        ] {
            assert!((got - want).abs() < 1e-9, "{b:?}");
        }
        let page = geometry::page_bounds(f.s.document(), &f.rect).unwrap();
        assert!((page.x - b.x).abs() < 1e-9 || page.x > 0.0);
        rotated(&mut f, 45.0);
        let b = geometry::bounds_in_frame(f.s.document(), &f.frame, &f.rect).unwrap();
        let side = (100.0 + 50.0) / 2.0_f64.sqrt();
        assert!(
            (b.width - side).abs() < 1e-6 && (b.height - side).abs() < 1e-6,
            "{b:?}"
        );
    }

    #[test]
    fn a_click_lands_on_the_turned_node_and_not_on_where_it_used_to_be() {
        let mut f = fixture();
        rotated(&mut f, 90.0);
        let hit = |x, y| geometry::hit_test(f.s.document(), &f.frame, x, y);
        assert_eq!(
            hit(150.0, 80.0),
            Some(f.rect.clone()),
            "the turned box stands up"
        );
        assert_eq!(
            hit(110.0, 125.0),
            Some(f.frame.clone()),
            "its old left end is empty now"
        );
    }

    #[test]
    fn a_turned_node_is_judged_by_where_it_really_is() {
        let mut f = fixture();
        apply(&mut f.s, &[set(&f.rect, Property::X(-100.0))]).unwrap();
        apply(&mut f.s, &[set(&f.rect, Property::Y(100.0))]).unwrap();
        let codes = |s: &Session| -> Vec<&'static str> {
            lint(s.document(), &LintOptions::default())
                .iter()
                .map(|f| f.code)
                .collect()
        };
        // 100 wide at x -100 ends exactly at the frame's edge: wholly outside
        assert!(codes(&f.s).contains(&"OD001"));
        // turned 45 degrees about its center it reaches 3 px into the frame
        apply(&mut f.s, &[set(&f.rect, Property::Rotation(45.0))]).unwrap();
        assert!(!codes(&f.s).contains(&"OD001"));
    }

    #[test]
    fn the_new_rules_find_a_flat_scale_a_clip_without_area_and_a_loop_that_jumps() {
        let mut f = fixture();
        apply(&mut f.s, &[set(&f.rect, Property::ScaleY(0.0))]).unwrap();
        apply(&mut f.s, &[set(&f.group, Property::Clip(true))]).unwrap();
        let keys = vec![key(0, 0.0, Ease::Linear), key(1000, 10.0, Ease::Linear)];
        apply(
            &mut f.s,
            &[animate(
                &f.video,
                &f.rect,
                AnimProp::Rotation,
                keys,
                Repeat::Loop,
            )],
        )
        .unwrap();
        let found = lint(f.s.document(), &LintOptions::default());
        let has = |code: &str| found.iter().any(|x| x.code == code);
        assert!(has("OD080") && has("OD081") && has("OD082"), "{found:?}");
        apply(&mut f.s, &[set(&f.rect, Property::ScaleY(1.0))]).unwrap();
        apply(&mut f.s, &[set(&f.group, Property::Clip(false))]).unwrap();
        let keys = vec![key(0, 0.0, Ease::Linear), key(1000, 0.0, Ease::Linear)];
        apply(
            &mut f.s,
            &[animate(
                &f.video,
                &f.rect,
                AnimProp::Rotation,
                keys,
                Repeat::Loop,
            )],
        )
        .unwrap();
        let found = lint(f.s.document(), &LintOptions::default());
        assert!(
            !["OD080", "OD081", "OD082"]
                .iter()
                .any(|c| found.iter().any(|x| &x.code == c)),
            "{found:?}"
        );
    }
}
