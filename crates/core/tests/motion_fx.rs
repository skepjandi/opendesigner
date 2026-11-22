#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::grade::{Grade, GradeParam, Look};
use od_core::speed::SpeedKey;
use od_core::{
    Actor, AnimProp, ClipProperty, Document, Ease, Footage, Key, NewNode, NodeId, NodeKind,
    OpError, Operation, OverlayProperty, Property, Session, Transition, TransitionKind, timebase,
};

struct Fixture {
    s: Session,
    video: NodeId,
    frame: NodeId,
}

fn fixture(fps: u32) -> Fixture {
    let mut s = Session::new(Document::new("v"));
    let page = s.add_page("Page 1").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut n = NewNode::new(NodeKind::Frame);
    n.width = 400.0;
    n.height = 800.0;
    let frame = s
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: n,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    let video = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "V".into(),
                fps,
                width: 400,
                height: 800,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    Fixture { s, video, frame }
}

fn add_clip(f: &mut Fixture, ms: u32, transition: Option<Transition>) -> (NodeId, Operation) {
    let r =
        f.s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: f.video.clone(),
                frame: Some(f.frame.clone()),
                footage: None,
                duration_ms: ms,
                index: None,
                transition,
                motion: None,
            }],
        )
        .unwrap();
    (r.created[0].clone(), r.ops[0].clone())
}

fn footage_clip(f: &mut Fixture, ms: u32, in_ms: u64) -> NodeId {
    f.s.apply(
        Actor::User,
        None,
        &[Operation::AddClip {
            video: f.video.clone(),
            frame: None,
            footage: Some(Footage {
                file: "media/a.mp4".into(),
                in_ms,
            }),
            duration_ms: ms,
            index: None,
            transition: None,
            motion: None,
        }],
    )
    .unwrap()
    .created[0]
        .clone()
}

fn set_clip(f: &mut Fixture, clip: &NodeId, prop: ClipProperty) -> Result<Operation, OpError> {
    f.s.apply(
        Actor::User,
        None,
        &[Operation::SetClip {
            video: f.video.clone(),
            clip: clip.clone(),
            prop,
        }],
    )
    .map(|r| r.ops[0].clone())
}

fn clip_of<'a>(f: &'a Fixture, id: &NodeId) -> &'a od_core::Clip {
    let v = f.s.document().video(&f.video).unwrap();
    &v.clips[v.clip(id).unwrap()]
}

#[test]
fn clip_durations_and_the_recorded_operation_are_snapped_to_the_frame_grid() {
    for fps in [24, 25, 29, 30, 60] {
        let mut f = fixture(fps);
        let (clip, recorded) = add_clip(&mut f, 1010, None);
        let snapped = clip_of(&f, &clip).duration_ms;
        assert_eq!(snapped as u64, timebase::snap(1010, fps), "{fps} fps");
        assert_eq!(timebase::snap(u64::from(snapped), fps), u64::from(snapped));
        match recorded {
            Operation::RestoreClip { clip, .. } => assert_eq!(clip.duration_ms, snapped),
            other => panic!("{other:?}"),
        }
        let op = set_clip(&mut f, &clip, ClipProperty::DurationMs(333)).unwrap();
        let again = clip_of(&f, &clip).duration_ms;
        assert_eq!(u64::from(again), timebase::snap(333, fps));
        assert_eq!(
            op,
            Operation::SetClip {
                video: f.video.clone(),
                clip: clip.clone(),
                prop: ClipProperty::DurationMs(again)
            }
        );
    }
}

#[test]
fn the_shortest_clip_snaps_up_inside_the_bounds_and_the_longest_down() {
    let mut f = fixture(24);
    let (clip, _) = add_clip(&mut f, 1000, None);
    set_clip(&mut f, &clip, ClipProperty::DurationMs(100)).unwrap();
    assert_eq!(clip_of(&f, &clip).duration_ms, 125);
    set_clip(&mut f, &clip, ClipProperty::DurationMs(60_000)).unwrap();
    assert_eq!(clip_of(&f, &clip).duration_ms, 60_000);
    assert!(set_clip(&mut f, &clip, ClipProperty::DurationMs(60_001)).is_err());
    assert!(set_clip(&mut f, &clip, ClipProperty::DurationMs(99)).is_err());
    assert_eq!(clip_of(&f, &clip).duration_ms, 60_000);
}

#[test]
fn transitions_footage_in_points_overlays_and_keys_are_snapped_too() {
    let mut f = fixture(24);
    add_clip(&mut f, 2000, None);
    let fade = Transition {
        kind: TransitionKind::Fade,
        duration_ms: 510,
        ease: Ease::Linear,
    };
    let (second, _) = add_clip(&mut f, 2000, Some(fade));
    assert_eq!(clip_of(&f, &second).transition.duration_ms, 500);
    let footage = footage_clip(&mut f, 1000, 1234);
    assert_eq!(clip_of(&f, &footage).footage.as_ref().unwrap().in_ms, 1250);

    let overlay =
        f.s.apply(
            Actor::User,
            None,
            &[Operation::AddOverlay {
                video: f.video.clone(),
                frame: f.frame.clone(),
                start_ms: 1001,
                duration_ms: 1001,
                fade_ms: Some(301),
                enter: None,
                exit: None,
                ease: None,
            }],
        )
        .unwrap();
    let o = &f.s.document().video(&f.video).unwrap().overlays[0];
    assert_eq!((o.start_ms, o.duration_ms, o.fade_ms), (1000, 1000, 292));
    assert!(
        matches!(&overlay.ops[0], Operation::RestoreOverlay { overlay, .. } if overlay.start_ms == 1000)
    );

    let id = o.id.clone();
    f.s.apply(
        Actor::User,
        None,
        &[Operation::SetOverlay {
            video: f.video.clone(),
            overlay: id,
            prop: OverlayProperty::StartMs(2010),
        }],
    )
    .unwrap();
    assert_eq!(
        f.s.document().video(&f.video).unwrap().overlays[0].start_ms,
        2000
    );

    let page = f.s.document().pages[0].root.id.clone();
    let mut img = NewNode::new(NodeKind::Rect);
    img.width = 10.0;
    img.height = 10.0;
    let node =
        f.s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: page,
                index: None,
                node: img,
            }],
        )
        .unwrap()
        .created[0]
            .clone();
    let r =
        f.s.apply(
            Actor::User,
            None,
            &[Operation::Animate {
                additive: false,
                video: f.video.clone(),
                node,
                prop: AnimProp::X,
                keys: vec![
                    Key {
                        at_ms: 0,
                        value: 0.0,
                        ease: Ease::Linear,
                    },
                    Key {
                        at_ms: 1010,
                        value: 50.0,
                        ease: Ease::Linear,
                    },
                ],
                repeat: od_core::Repeat::Once,
            }],
        )
        .unwrap();
    match &r.ops[0] {
        Operation::Animate { keys, .. } => assert_eq!(keys[1].at_ms, 1000),
        other => panic!("{other:?}"),
    }
}

#[test]
fn undo_puts_back_the_exact_old_number_and_redo_replays_the_snapped_one() {
    let mut f = fixture(24);
    let (clip, _) = add_clip(&mut f, 1000, None);
    // an old project may hold an off-grid length; load it untouched
    let mut doc = f.s.document().clone();
    doc.videos[0].clips[0].duration_ms = 1001;
    let mut s = Session::new(doc);
    let before = s.document().video(&f.video).unwrap().clips[0].duration_ms;
    assert_eq!(before, 1001);
    s.apply(
        Actor::User,
        None,
        &[Operation::SetClip {
            video: f.video.clone(),
            clip: clip.clone(),
            prop: ClipProperty::DurationMs(2010),
        }],
    )
    .unwrap();
    assert_eq!(
        s.document().video(&f.video).unwrap().clips[0].duration_ms,
        2000
    );
    s.undo(Actor::User).unwrap();
    assert_eq!(
        s.document().video(&f.video).unwrap().clips[0].duration_ms,
        1001
    );
    s.redo(Actor::User).unwrap();
    assert_eq!(
        s.document().video(&f.video).unwrap().clips[0].duration_ms,
        2000
    );
}

#[test]
fn speed_and_ramp_set_validate_and_exclude_each_other() {
    let mut f = fixture(30);
    let clip = footage_clip(&mut f, 2000, 0);
    set_clip(&mut f, &clip, ClipProperty::Speed(2.0)).unwrap();
    assert_eq!(clip_of(&f, &clip).source_span_ms(), 4000);
    assert!(set_clip(&mut f, &clip, ClipProperty::Speed(0.2)).is_err());
    assert!(set_clip(&mut f, &clip, ClipProperty::Speed(4.5)).is_err());
    let key = |at_ms, rate| SpeedKey {
        at_ms,
        rate,
        ease: Ease::Linear,
    };
    let err = set_clip(&mut f, &clip, ClipProperty::SpeedRamp(vec![key(0, 1.0)])).unwrap_err();
    assert!(err.to_string().contains("set speed to 1"), "{err}");
    set_clip(&mut f, &clip, ClipProperty::Speed(1.0)).unwrap();
    set_clip(
        &mut f,
        &clip,
        ClipProperty::SpeedRamp(vec![key(0, 1.0), key(1005, 3.0)]),
    )
    .unwrap();
    assert_eq!(clip_of(&f, &clip).speed_ramp[1].at_ms, 1000);
    f.s.undo(Actor::User).unwrap();
    assert!(clip_of(&f, &clip).speed_ramp.is_empty());
}

#[test]
fn only_footage_can_be_retimed_and_a_frame_clip_stays_unchanged_on_the_wire() {
    let mut f = fixture(30);
    let (frame_clip, _) = add_clip(&mut f, 1000, None);
    let err = set_clip(&mut f, &frame_clip, ClipProperty::Speed(2.0)).unwrap_err();
    assert!(err.to_string().contains("footage"), "{err}");
    let text = serde_json::to_string(&f.s.document().video(&f.video).unwrap().clips[0]).unwrap();
    for field in ["speed", "grade", "speed_ramp"] {
        assert!(!text.contains(field), "{text}");
    }
}

#[test]
fn a_clip_grade_is_stored_as_numbers_validated_and_undoable() {
    let mut f = fixture(30);
    let (clip, _) = add_clip(&mut f, 1000, None);
    let look: Grade = serde_json::from_str(r#"{"look":"mono","blur":2}"#).unwrap();
    set_clip(&mut f, &clip, ClipProperty::Grade(Some(look))).unwrap();
    let g = clip_of(&f, &clip).grade.unwrap();
    assert_eq!((g.saturation, g.blur), (-1.0, 2.0));
    let bad = Grade {
        exposure: 3.5,
        ..Grade::default()
    };
    assert!(set_clip(&mut f, &clip, ClipProperty::Grade(Some(bad))).is_err());
    set_clip(&mut f, &clip, ClipProperty::Grade(Some(Grade::default()))).unwrap();
    assert!(clip_of(&f, &clip).grade.is_none());
    f.s.undo(Actor::User).unwrap();
    assert_eq!(clip_of(&f, &clip).grade, Some(look));
    assert_eq!(Look::Warm.grade().temperature, 0.35);
}

#[test]
fn an_image_node_carries_a_grade_that_keyframes_can_move() {
    let mut f = fixture(30);
    let page = f.s.document().pages[0].root.id.clone();
    let mut n = NewNode::new(NodeKind::Image);
    n.width = 10.0;
    n.height = 10.0;
    let img =
        f.s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: page,
                index: None,
                node: n,
            }],
        )
        .unwrap()
        .created[0]
            .clone();
    let set = |prop| Operation::Set {
        id: img.clone(),
        prop,
    };
    f.s.apply(
        Actor::User,
        None,
        &[set(Property::Grade(Some(Look::Warm.grade())))],
    )
    .unwrap();
    assert_eq!(
        f.s.document().node(&img).unwrap().grade,
        Some(Look::Warm.grade())
    );
    assert!(
        f.s.apply(
            Actor::User,
            None,
            &[set(Property::GradeParam {
                param: GradeParam::Vignette,
                value: 1.5
            })]
        )
        .is_err()
    );
    f.s.apply(
        Actor::User,
        None,
        &[Operation::Animate {
            additive: false,
            video: f.video.clone(),
            node: img.clone(),
            prop: AnimProp::GradeBlur,
            keys: vec![
                Key {
                    at_ms: 0,
                    value: 0.0,
                    ease: Ease::Linear,
                },
                Key {
                    at_ms: 1000,
                    value: 40.0,
                    ease: Ease::Linear,
                },
            ],
            repeat: od_core::Repeat::Once,
        }],
    )
    .unwrap();
    let v = f.s.document().video(&f.video).unwrap();
    let at = f.s.document().at_time(v, 500.0).unwrap();
    assert_eq!(at.node(&img).unwrap().grade.unwrap().blur, 20.0);
    assert_eq!(at.node(&img).unwrap().grade.unwrap().temperature, 0.35);
    let mut rect = NewNode::new(NodeKind::Rect);
    rect.width = 5.0;
    rect.height = 5.0;
    let root = f.s.document().pages[0].root.id.clone();
    let r =
        f.s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: rect,
            }],
        )
        .unwrap()
        .created[0]
            .clone();
    assert!(
        f.s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: r,
                prop: Property::Grade(Some(Look::Warm.grade()))
            }]
        )
        .is_err()
    );
}

#[test]
fn new_clip_fields_leave_old_projects_byte_identical() {
    let mut f = fixture(30);
    add_clip(&mut f, 1000, None);
    let text = serde_json::to_string(f.s.document().video(&f.video).unwrap()).unwrap();
    let back: od_core::Video = serde_json::from_str(&text).unwrap();
    assert_eq!(serde_json::to_string(&back).unwrap(), text);
    for field in ["\"speed\"", "\"grade\"", "\"speed_ramp\""] {
        assert!(!text.contains(field), "{field}");
    }
}

#[test]
fn the_agent_can_write_a_grade_as_json_and_read_it_back() {
    let op: Operation = serde_json::from_str(
        r#"{"op":"set","id":"n_1","prop":{"name":"grade","value":{"look":"warm","exposure":0.25}}}"#,
    )
    .unwrap();
    match op {
        Operation::Set { prop, .. } => match prop {
            Property::Grade(Some(g)) => {
                assert_eq!(g.exposure, 0.25);
                assert_eq!(g.temperature, 0.35);
                assert_eq!(g.saturation, 0.1);
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    let op: Operation = serde_json::from_str(
        r#"{"op":"set_clip","video":"v_1","clip":"c_1","prop":{"name":"speed_ramp","value":[{"at_ms":0,"rate":0.5},{"at_ms":1000,"rate":2}]}}"#,
    )
    .unwrap();
    match op {
        Operation::SetClip { prop, .. } => match prop {
            ClipProperty::SpeedRamp(keys) => {
                assert_eq!(keys.len(), 2);
                assert_eq!((keys[0].rate, keys[1].rate), (0.5, 2.0));
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
    let op: Operation = serde_json::from_str(
        r#"{"op":"set_clip","video":"v_1","clip":"c_1","prop":{"name":"transition","value":{"kind":"glitch_slice","duration_ms":400}}}"#,
    )
    .unwrap();
    match op {
        Operation::SetClip { prop, .. } => match prop {
            ClipProperty::Transition(t) => {
                assert_eq!(t.kind, TransitionKind::GlitchSlice);
                assert_eq!(t.duration_ms, 400);
            }
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}
