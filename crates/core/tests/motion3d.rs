#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::animation::{self, PresetRequest};
use od_core::{
    Actor, AnimProp, Device3d, DeviceModel, Document, Ease, Key, NewNode, NodeId, NodeKind,
    OpError, Operation, Preset, Property, Session, VideoProperty, project,
};

struct Fixture {
    s: Session,
    frame: NodeId,
    device: NodeId,
    video: NodeId,
}

fn apply(s: &mut Session, ops: &[Operation]) -> Result<od_core::TxReceipt, OpError> {
    s.apply(Actor::Agent("t".into()), None, ops)
}

/// A frame holding a phone turned 20 degrees, and an empty 30 fps video.
fn fixture() -> Fixture {
    let mut s = Session::new(Document::new("3d"));
    let page = s.add_page("Page 1").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 600.0;
    frame.height = 1000.0;
    let mut device = NewNode::new(NodeKind::Device3d);
    device.name = Some("Phone".into());
    device.width = 600.0;
    device.height = 1000.0;
    device.device3d = Some(Device3d {
        rotate_y: 20.0,
        ..Device3d::default()
    });
    frame.children.push(device);
    let r = apply(
        &mut s,
        &[
            Operation::Create {
                parent: root,
                index: None,
                node: frame,
            },
            Operation::CreateVideo {
                name: "Promo".into(),
                fps: 30,
                width: 600,
                height: 1000,
            },
        ],
    )
    .unwrap();
    let frame = r.created[0].clone();
    let device = s.document().node(&frame).unwrap().children[0].id.clone();
    Fixture {
        frame,
        device,
        video: r.created[1].clone(),
        s,
    }
}

fn key(at_ms: u64, value: f64, ease: Ease) -> Key {
    Key { at_ms, value, ease }
}

fn animate(video: &NodeId, device: &NodeId, prop: AnimProp, keys: Vec<Key>) -> Operation {
    Operation::Animate {
        video: video.clone(),
        node: device.clone(),
        prop,
        keys,
    }
}

#[test]
fn a_device_is_created_turned_set_and_undone_like_any_node() {
    let mut f = fixture();
    let d =
        f.s.document()
            .node(&f.device)
            .unwrap()
            .device3d
            .clone()
            .unwrap();
    assert_eq!((d.model, d.rotate_y), (DeviceModel::Phone, 20.0));
    apply(
        &mut f.s,
        &[
            Operation::Set {
                id: f.device.clone(),
                prop: Property::Model(DeviceModel::Laptop),
            },
            Operation::Set {
                id: f.device.clone(),
                prop: Property::RotateX(-12.5),
            },
        ],
    )
    .unwrap();
    let d =
        f.s.document()
            .node(&f.device)
            .unwrap()
            .device3d
            .clone()
            .unwrap();
    assert_eq!((d.model, d.rotate_x), (DeviceModel::Laptop, -12.5));
    f.s.undo(Actor::User).unwrap();
    let d =
        f.s.document()
            .node(&f.device)
            .unwrap()
            .device3d
            .clone()
            .unwrap();
    assert_eq!((d.model, d.rotate_x), (DeviceModel::Phone, 0.0));
}

#[test]
fn device_properties_are_refused_on_other_nodes_and_out_of_bounds() {
    let mut f = fixture();
    let on_frame = apply(
        &mut f.s,
        &[Operation::Set {
            id: f.frame.clone(),
            prop: Property::RotateY(10.0),
        }],
    );
    assert!(matches!(on_frame, Err(OpError::NotApplicable { .. })));
    let too_far = apply(
        &mut f.s,
        &[Operation::Set {
            id: f.device.clone(),
            prop: Property::Fov(170.0),
        }],
    );
    assert!(matches!(too_far, Err(OpError::OutOfBounds { .. })));
    let outside = apply(
        &mut f.s,
        &[Operation::Set {
            id: f.device.clone(),
            prop: Property::Image(Some("../secret.png".into())),
        }],
    );
    assert!(outside.is_err());
}

#[test]
fn animations_interpolate_with_eases_and_undo_back_to_none() {
    let mut f = fixture();
    apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.device,
            AnimProp::RotateY,
            vec![key(0, -30.0, Ease::Linear), key(1000, 30.0, Ease::Linear)],
        )],
    )
    .unwrap();
    let v = f.s.document().video(&f.video).unwrap().clone();
    let at = |ms: f64| {
        f.s.document()
            .at_time(&v, ms)
            .unwrap()
            .node(&f.device)
            .unwrap()
            .device3d
            .clone()
            .unwrap()
            .rotate_y
    };
    assert_eq!((at(0.0), at(500.0), at(2000.0)), (-30.0, 0.0, 30.0));
    // the design itself keeps its own pose
    let designed =
        f.s.document()
            .node(&f.device)
            .unwrap()
            .device3d
            .clone()
            .unwrap();
    assert_eq!(designed.rotate_y, 20.0);
    f.s.undo(Actor::User).unwrap();
    assert!(
        f.s.document()
            .video(&f.video)
            .unwrap()
            .animations
            .is_empty()
    );
    f.s.redo(Actor::User).unwrap();
    assert_eq!(f.s.document().video(&f.video).unwrap().animations.len(), 1);
}

#[test]
fn animation_keys_are_validated_against_the_node() {
    let mut f = fixture();
    let backwards = apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.device,
            AnimProp::Opacity,
            vec![key(500, 0.0, Ease::Linear), key(100, 1.0, Ease::Linear)],
        )],
    );
    assert!(backwards.is_err());
    let out_of_range = apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.device,
            AnimProp::Opacity,
            vec![key(0, 2.0, Ease::Linear)],
        )],
    );
    assert!(out_of_range.is_err());
    let rotate_a_frame = apply(
        &mut f.s,
        &[Operation::Animate {
            video: f.video.clone(),
            node: f.frame.clone(),
            prop: AnimProp::RotateY,
            keys: vec![key(0, 1.0, Ease::Linear)],
        }],
    );
    assert!(matches!(rotate_a_frame, Err(OpError::NotApplicable { .. })));
}

#[test]
fn an_animated_node_cannot_be_deleted_until_its_animation_is_removed() {
    let mut f = fixture();
    apply(
        &mut f.s,
        &[animate(
            &f.video,
            &f.device,
            AnimProp::Zoom,
            vec![key(0, 1.0, Ease::Linear), key(800, 1.2, Ease::EaseOut)],
        )],
    )
    .unwrap();
    let delete_frame = apply(
        &mut f.s,
        &[Operation::Delete {
            id: f.frame.clone(),
        }],
    );
    assert!(matches!(delete_frame, Err(OpError::NodeAnimated { .. })));
    apply(
        &mut f.s,
        &[
            animate(&f.video, &f.device, AnimProp::Zoom, vec![]),
            Operation::Delete {
                id: f.frame.clone(),
            },
        ],
    )
    .unwrap();
}

#[test]
fn presets_land_on_the_designed_pose_and_chain_without_losing_earlier_keys() {
    let mut f = fixture();
    let ops = animation::preset_ops(
        f.s.document(),
        &f.video,
        &f.device,
        PresetRequest {
            preset: Preset::HeroReveal,
            start_ms: 0,
            duration_ms: 1200,
            strength: 1.0,
            ease: None,
        },
    )
    .unwrap();
    apply(&mut f.s, &ops).unwrap();
    let ops = animation::preset_ops(
        f.s.document(),
        &f.video,
        &f.device,
        PresetRequest {
            preset: Preset::Swing,
            start_ms: 2000,
            duration_ms: 3000,
            strength: 1.0,
            ease: None,
        },
    )
    .unwrap();
    apply(&mut f.s, &ops).unwrap();
    let v = f.s.document().video(&f.video).unwrap().clone();
    let y = v
        .animations
        .iter()
        .find(|a| a.prop == AnimProp::RotateY)
        .unwrap();
    let times: Vec<u64> = y.keys.iter().map(|k| k.at_ms).collect();
    assert_eq!(times, vec![0, 1200, 2000, 3000, 4000, 5000]);
    assert_eq!(y.value_at(0.0), Some(0.0));
    assert_eq!(y.value_at(1200.0), Some(20.0));
    assert_eq!(y.value_at(5000.0), Some(20.0));
    assert!(v.animations.iter().any(|a| a.prop == AnimProp::Zoom));
    let story = od_core::view::videos(f.s.document())[0].story.join("\n");
    let expected = format!(
        "Phone ({}) rotate_y: 0 at 0.0 s, then 20 at 1.2 s",
        f.device
    );
    assert!(story.contains(&expected), "{story}");
    assert!(
        !story.contains("rotate_x"),
        "a move that changes nothing is not keyed: {story}"
    );
}

#[test]
fn device_presets_are_refused_on_flat_nodes() {
    let f = fixture();
    let r = animation::preset_ops(
        f.s.document(),
        &f.video,
        &f.frame,
        PresetRequest {
            preset: Preset::Spin,
            start_ms: 0,
            duration_ms: 1000,
            strength: 1.0,
            ease: None,
        },
    );
    assert!(matches!(r, Err(OpError::NotApplicable { .. })));
}

#[test]
fn motion_blur_takes_off_or_two_to_sixteen_samples() {
    let mut f = fixture();
    for (samples, ok) in [(0, true), (1, false), (8, true), (17, false)] {
        let r = apply(
            &mut f.s,
            &[Operation::SetVideo {
                id: f.video.clone(),
                prop: VideoProperty::MotionBlur(samples),
            }],
        );
        assert_eq!(r.is_ok(), ok, "{samples} samples");
    }
}

#[test]
fn devices_and_animations_round_trip_through_a_project_folder() {
    let mut f = fixture();
    apply(
        &mut f.s,
        &[
            animate(
                &f.video,
                &f.device,
                AnimProp::RotateY,
                vec![key(0, 0.0, Ease::Linear), key(900, 360.0, Ease::Spring)],
            ),
            Operation::SetVideo {
                id: f.video.clone(),
                prop: VideoProperty::MotionBlur(6),
            },
        ],
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    project::save(dir.path(), &f.s).unwrap();
    let manifest = std::fs::read_to_string(dir.path().join("od.toml")).unwrap();
    assert!(manifest.contains("format = 2"));
    let loaded = project::load(dir.path()).unwrap();
    assert_eq!(loaded.document(), f.s.document());
}

#[test]
fn a_version_one_project_still_loads() {
    let mut f = fixture();
    // a project with no 3D content and no animation, written by the previous version
    apply(
        &mut f.s,
        &[Operation::Delete {
            id: f.frame.clone(),
        }],
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    project::save(dir.path(), &f.s).unwrap();
    let manifest = dir.path().join("od.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("format = 2", "format = 1")).unwrap();
    let loaded = project::load(dir.path()).unwrap();
    assert_eq!(loaded.document().format, 2);
}
