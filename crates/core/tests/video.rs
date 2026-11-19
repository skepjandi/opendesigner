#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::video::{self, MAX_CLIP_MS};
use od_core::{
    Actor, AudioTrack, ClipProperty, Document, Motion, NewNode, NodeId, NodeKind, OpError,
    Operation, Session, Transition, TransitionKind, VideoProperty, project,
};

fn frame(s: &mut Session, root: &NodeId, name: &str) -> NodeId {
    let mut n = NewNode::new(NodeKind::Frame);
    n.name = Some(name.to_string());
    n.width = 400.0;
    n.height = 800.0;
    let r = s
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root.clone(),
                index: None,
                node: n,
            }],
        )
        .unwrap();
    r.created[0].clone()
}

/// A session with two frames and an empty 30 fps video of 400x800.
fn fixture() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = Session::new(Document::new("v"));
    let page = s.add_page("Page 1").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let (a, b) = (frame(&mut s, &root, "A"), frame(&mut s, &root, "B"));
    let r = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "Preview".into(),
                fps: 30,
                width: 400,
                height: 800,
            }],
        )
        .unwrap();
    (s, r.created[0].clone(), a, b)
}

fn add_clip(s: &mut Session, video: &NodeId, frame: &NodeId, ms: u32) -> NodeId {
    let r = s
        .apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: video.clone(),
                frame: Some(frame.clone()),
                footage: None,
                duration_ms: ms,
                index: None,
                transition: None,
                motion: None,
            }],
        )
        .unwrap();
    r.created[0].clone()
}

fn fade(ms: u32) -> Transition {
    Transition {
        kind: TransitionKind::Fade,
        duration_ms: ms,
        ease: od_core::Ease::default(),
    }
}

#[test]
fn transitions_overlap_the_previous_clip_and_shorten_the_video() {
    let (mut s, v, a, b) = fixture();
    let c1 = add_clip(&mut s, &v, &a, 3000);
    let c2 = add_clip(&mut s, &v, &b, 2000);
    assert_eq!(s.document().video(&v).unwrap().duration_ms(), 5000);
    s.apply(
        Actor::User,
        None,
        &[Operation::SetClip {
            video: v.clone(),
            clip: c2.clone(),
            prop: ClipProperty::Transition(fade(500)),
        }],
    )
    .unwrap();
    let video = s.document().video(&v).unwrap();
    let placed = video.placed();
    assert_eq!(video.duration_ms(), 4500);
    assert_eq!((placed[0].start_ms, placed[0].end_ms), (0, 3000));
    assert_eq!((placed[1].start_ms, placed[1].end_ms), (2500, 4500));
    assert_eq!(placed[1].overlap_ms, 500);
    assert_eq!(video.frame_count(), 135, "4.5 s at 30 fps");
    assert_eq!(video.clips[0].id, c1);
}

#[test]
fn a_transition_longer_than_its_clip_or_a_bad_value_is_refused() {
    let (mut s, v, a, _) = fixture();
    let c = add_clip(&mut s, &v, &a, 1000);
    let set = |prop| Operation::SetClip {
        video: v.clone(),
        clip: c.clone(),
        prop,
    };
    for bad in [
        fade(2000),
        fade(0),
        Transition {
            kind: TransitionKind::Cut,
            duration_ms: 100,
            ease: od_core::Ease::default(),
        },
    ] {
        let err = s.apply(Actor::User, None, &[set(ClipProperty::Transition(bad))]);
        assert!(
            matches!(
                err,
                Err(OpError::OutOfBounds {
                    what: "transition",
                    ..
                })
            ),
            "{err:?}"
        );
    }
    for ms in [0, 99, MAX_CLIP_MS + 1] {
        assert!(
            s.apply(Actor::User, None, &[set(ClipProperty::DurationMs(ms))])
                .is_err()
        );
    }
    let too_zoomed = Motion {
        scale_from: 1.0,
        scale_to: 3.5,
        ease: od_core::Ease::default(),
    };
    assert!(
        s.apply(Actor::User, None, &[set(ClipProperty::Motion(too_zoomed))])
            .is_err()
    );
    assert_eq!(
        s.document().video(&v).unwrap().clips[0].duration_ms,
        1000,
        "nothing changed"
    );
}

#[test]
fn clips_can_be_moved_edited_and_removed_and_every_step_undoes() {
    let (mut s, v, a, b) = fixture();
    let before = s.document().clone();
    let c1 = add_clip(&mut s, &v, &a, 1000);
    let c2 = add_clip(&mut s, &v, &b, 2000);
    s.apply(
        Actor::User,
        None,
        &[
            Operation::MoveClip {
                video: v.clone(),
                clip: c2.clone(),
                index: 0,
            },
            Operation::SetClip {
                video: v.clone(),
                clip: c1.clone(),
                prop: ClipProperty::Motion(Motion {
                    scale_from: 1.0,
                    scale_to: 1.2,
                    ease: od_core::Ease::default(),
                }),
            },
            Operation::SetVideo {
                id: v.clone(),
                prop: VideoProperty::Fps(24),
            },
            Operation::SetVideo {
                id: v.clone(),
                prop: VideoProperty::Size {
                    width: 200,
                    height: 400,
                },
            },
            Operation::RemoveClip {
                video: v.clone(),
                clip: c1.clone(),
            },
        ],
    )
    .unwrap();
    let video = s.document().video(&v).unwrap();
    assert_eq!(video.clips.iter().map(|c| &c.id).collect::<Vec<_>>(), [&c2]);
    assert_eq!((video.fps, video.width, video.height), (24, 200, 400));

    for _ in 0..3 {
        s.undo(Actor::User).unwrap();
    }
    assert_eq!(
        s.document().video(&v).unwrap().clips.len(),
        0,
        "back to the empty video"
    );
    assert_eq!(s.document().videos, before.videos);
    for _ in 0..3 {
        s.redo(Actor::User).unwrap();
    }
    assert_eq!(s.document().video(&v).unwrap().clips.len(), 1);
}

#[test]
fn deleting_a_video_comes_back_whole_and_a_used_frame_cannot_be_deleted() {
    let (mut s, v, a, b) = fixture();
    add_clip(&mut s, &v, &a, 1000);
    add_clip(&mut s, &v, &b, 1000);
    let err = s
        .apply(Actor::User, None, &[Operation::Delete { id: a.clone() }])
        .unwrap_err();
    assert!(matches!(err, OpError::FrameInUse { .. }), "{err:?}");
    assert!(err.to_string().contains("remove that clip"));

    let with_clips = s.document().videos.clone();
    s.apply(
        Actor::User,
        None,
        &[Operation::DeleteVideo { id: v.clone() }],
    )
    .unwrap();
    assert!(s.document().videos.is_empty());
    s.apply(Actor::User, None, &[Operation::Delete { id: a.clone() }])
        .unwrap();
    s.undo(Actor::User).unwrap();
    s.undo(Actor::User).unwrap();
    assert_eq!(
        s.document().videos,
        with_clips,
        "ids, order and settings are back"
    );
}

#[test]
fn clips_show_frames_only() {
    let (mut s, v, a, _) = fixture();
    let child = {
        let mut n = NewNode::new(NodeKind::Rect);
        n.width = 10.0;
        n.height = 10.0;
        let r = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: a.clone(),
                    index: None,
                    node: n,
                }],
            )
            .unwrap();
        r.created[0].clone()
    };
    for bad in [child, NodeId::parse("n_999").unwrap()] {
        let r = s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame: Some(bad),
                footage: None,
                duration_ms: 1000,
                index: None,
                transition: None,
                motion: None,
            }],
        );
        assert!(r.is_err());
    }
}

#[test]
fn sizes_frame_rates_and_lengths_are_bounded_and_history_only_ops_are_refused() {
    let (mut s, v, a, _) = fixture();
    for (w, h, fps) in [
        (401, 800, 30),
        (400, 8, 30),
        (5000, 800, 30),
        (400, 800, 0),
        (400, 800, 61),
    ] {
        let r = s.apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "x".into(),
                fps,
                width: w,
                height: h,
            }],
        );
        assert!(r.is_err(), "{w}x{h} at {fps} fps");
    }
    let c = add_clip(&mut s, &v, &a, MAX_CLIP_MS);
    for _ in 0..video::MAX_VIDEO_MS / u64::from(MAX_CLIP_MS) {
        let r = s.apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame: Some(a.clone()),
                footage: None,
                duration_ms: MAX_CLIP_MS,
                index: None,
                transition: None,
                motion: None,
            }],
        );
        if let Err(e) = r {
            assert!(e.to_string().contains("minutes"));
            let video = s.document().video(&v).unwrap();
            assert!(video.duration_ms() <= video::MAX_VIDEO_MS);
            assert!(video.clip(&c).is_some());
            let restore = Operation::RestoreVideo {
                video: video.clone(),
            };
            assert!(matches!(
                s.apply(Actor::User, None, &[restore]),
                Err(OpError::InternalOnly(_))
            ));
            return;
        }
    }
    panic!("the length limit never applied");
}

#[test]
fn videos_survive_saving_and_loading_and_a_deleted_video_leaves_no_file() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let mut s = project::create(&dir, "Videos").unwrap();
    let root = s.document().pages[0].root.id.clone();
    let (a, b) = (frame(&mut s, &root, "A"), frame(&mut s, &root, "B"));
    let r = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "Teaser".into(),
                fps: 30,
                width: 400,
                height: 800,
            }],
        )
        .unwrap();
    let v = r.created[0].clone();
    add_clip(&mut s, &v, &a, 2000);
    let c2 = add_clip(&mut s, &v, &b, 3000);
    s.apply(
        Actor::User,
        None,
        &[Operation::SetClip {
            video: v.clone(),
            clip: c2,
            prop: ClipProperty::Transition(fade(400)),
        }],
    )
    .unwrap();
    project::save(&dir, &s).unwrap();
    assert!(dir.join(format!("videos/{v}.json")).exists());
    let loaded = project::load(&dir).unwrap();
    assert_eq!(loaded.document().videos, s.document().videos);

    s.apply(
        Actor::User,
        None,
        &[Operation::DeleteVideo { id: v.clone() }],
    )
    .unwrap();
    project::save(&dir, &s).unwrap();
    assert!(!dir.join(format!("videos/{v}.json")).exists());
    assert!(project::load(&dir).unwrap().document().videos.is_empty());
}

#[test]
fn a_project_with_a_clip_on_a_missing_frame_does_not_load() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let mut s = project::create(&dir, "Videos").unwrap();
    let root = s.document().pages[0].root.id.clone();
    let a = frame(&mut s, &root, "A");
    let r = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "T".into(),
                fps: 30,
                width: 400,
                height: 800,
            }],
        )
        .unwrap();
    add_clip(&mut s, &r.created[0], &a, 1000);
    project::save(&dir, &s).unwrap();
    let path = dir.join(format!("videos/{}.json", r.created[0]));
    let broken = std::fs::read_to_string(&path)
        .unwrap()
        .replace(a.as_str(), "n_77");
    std::fs::write(&path, broken).unwrap();
    assert!(project::load(&dir).is_err());
}

use od_core::{Footage, OverlayProperty};

fn footage(file: &str, in_ms: u64) -> Footage {
    Footage {
        file: file.into(),
        in_ms,
    }
}

#[test]
fn a_clip_shows_a_frame_or_footage_and_footage_names_a_plain_media_file() {
    let (mut s, v, a, _) = fixture();
    let add = |frame: Option<NodeId>, footage: Option<Footage>| Operation::AddClip {
        video: v.clone(),
        frame,
        footage,
        duration_ms: 2000,
        index: None,
        transition: None,
        motion: None,
    };
    for (frame, f) in [
        (Some(a.clone()), Some(footage("media/x.mp4", 0))),
        (None, None),
        (None, Some(footage("x.mp4", 0))),
        (None, Some(footage("media/../x.mp4", 0))),
        (None, Some(footage("media/x.png", 0))),
        (
            None,
            Some(footage("media/x.mp4", od_core::video::MAX_IN_MS + 1)),
        ),
    ] {
        assert!(
            s.apply(Actor::User, None, &[add(frame.clone(), f.clone())])
                .is_err(),
            "{frame:?} {f:?}"
        );
    }
    let r = s
        .apply(
            Actor::User,
            None,
            &[add(None, Some(footage("media/talk.mp4", 5000)))],
        )
        .unwrap();
    let clip = r.created[0].clone();
    // changing what the clip shows is one undoable step
    s.apply(
        Actor::User,
        None,
        &[Operation::SetClip {
            video: v.clone(),
            clip: clip.clone(),
            prop: ClipProperty::Source(od_core::ClipSource {
                frame: Some(a.clone()),
                footage: None,
            }),
        }],
    )
    .unwrap();
    let c = &s.document().video(&v).unwrap().clips[0];
    assert_eq!((c.frame.as_ref(), c.footage.as_ref()), (Some(&a), None));
    s.undo(Actor::User).unwrap();
    let c = &s.document().video(&v).unwrap().clips[0];
    assert_eq!(c.footage, Some(footage("media/talk.mp4", 5000)));
    assert!(c.frame.is_none());
}

#[test]
fn overlays_fade_in_and_out_hold_a_frame_and_undo_cleanly() {
    let (mut s, v, a, b) = fixture();
    add_clip(&mut s, &v, &a, 4000);
    let before = s.document().videos.clone();
    let r = s
        .apply(
            Actor::User,
            None,
            &[Operation::AddOverlay {
                video: v.clone(),
                frame: b.clone(),
                start_ms: 1000,
                duration_ms: 2000,
                fade_ms: Some(400),
                enter: None,
                exit: None,
                ease: None,
            }],
        )
        .unwrap();
    let o = r.created[0].clone();
    assert!(o.as_str().starts_with("o_"));
    let set = |prop| Operation::SetOverlay {
        video: v.clone(),
        overlay: o.clone(),
        prop,
    };
    assert!(
        s.apply(Actor::User, None, &[set(OverlayProperty::FadeMs(1001))])
            .is_err(),
        "more than half each way"
    );
    assert!(
        s.apply(Actor::User, None, &[set(OverlayProperty::DurationMs(50))])
            .is_err()
    );
    s.apply(Actor::User, None, &[set(OverlayProperty::StartMs(500))])
        .unwrap();
    assert_eq!(s.document().video(&v).unwrap().overlays[0].start_ms, 500);

    let err = s
        .apply(Actor::User, None, &[Operation::Delete { id: b.clone() }])
        .unwrap_err();
    assert!(matches!(err, OpError::FrameInUse { .. }), "{err:?}");

    s.apply(
        Actor::User,
        None,
        &[Operation::RemoveOverlay {
            video: v.clone(),
            overlay: o.clone(),
        }],
    )
    .unwrap();
    assert!(s.document().video(&v).unwrap().overlays.is_empty());
    for _ in 0..4 {
        s.undo(Actor::User).unwrap();
    }
    assert_eq!(s.document().videos[0].overlays, before[0].overlays);
    assert!(matches!(
        s.apply(
            Actor::User,
            None,
            &[Operation::RestoreOverlay {
                video: v.clone(),
                index: 0,
                overlay: od_core::Overlay {
                    id: o,
                    frame: b,
                    start_ms: 0,
                    duration_ms: 1000,
                    fade_ms: 0,
                    enter: Default::default(),
                    exit: Default::default(),
                    ease: Default::default(),
                }
            }]
        ),
        Err(OpError::InternalOnly(_))
    ));
}

fn audio(file: &str) -> AudioTrack {
    AudioTrack {
        file: file.into(),
        start_ms: 0,
        gain: 1.0,
        fade_in_ms: 0,
        fade_out_ms: 0,
    }
}

#[test]
fn a_video_can_have_one_sound_track_and_undoing_takes_it_away() {
    let (mut s, v, a, _b) = fixture();
    add_clip(&mut s, &v, &a, 2000);
    let track = AudioTrack {
        file: "media/song.mp3".into(),
        start_ms: 500,
        gain: 0.8,
        fade_in_ms: 200,
        fade_out_ms: 400,
    };
    s.apply(
        Actor::User,
        None,
        &[Operation::SetVideo {
            id: v.clone(),
            prop: VideoProperty::Audio(Some(track.clone())),
        }],
    )
    .unwrap();
    let video = s.document().video(&v).unwrap();
    assert_eq!(video.audio.as_ref(), Some(&track));

    // it is one undoable step
    s.undo(Actor::User).unwrap();
    assert!(s.document().video(&v).unwrap().audio.is_none());
    s.redo(Actor::User).unwrap();
    assert_eq!(s.document().video(&v).unwrap().audio.as_ref(), Some(&track));

    // taking it away is an edit too
    s.apply(
        Actor::User,
        None,
        &[Operation::SetVideo {
            id: v.clone(),
            prop: VideoProperty::Audio(None),
        }],
    )
    .unwrap();
    assert!(s.document().video(&v).unwrap().audio.is_none());
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().video(&v).unwrap().audio.as_ref(), Some(&track));
}

#[test]
fn a_sound_track_names_a_media_file_and_stays_within_its_bounds() {
    let (mut s, v, a, _b) = fixture();
    add_clip(&mut s, &v, &a, 2000);
    let mut set = |track: AudioTrack| {
        s.apply(
            Actor::User,
            None,
            &[Operation::SetVideo {
                id: v.clone(),
                prop: VideoProperty::Audio(Some(track)),
            }],
        )
    };
    for bad in [
        "song.mp3",
        "media/../song.mp3",
        "media/song.txt",
        "media/song.mp3/x.mp3",
    ] {
        assert!(set(audio(bad)).is_err(), "{bad} should be refused");
    }
    assert!(set(audio("media/song.mp3")).is_ok());
    for extension in ["wav", "m4a", "aac", "ogg"] {
        assert!(
            set(audio(&format!("media/song.{extension}"))).is_ok(),
            "{extension} should be allowed"
        );
    }
    let mut loud = audio("media/song.mp3");
    loud.gain = 2.5;
    assert!(
        set(loud.clone()).is_err(),
        "a gain above two should be refused"
    );
    loud.gain = -1.0;
    assert!(set(loud).is_err(), "a negative gain should be refused");
    let mut late = audio("media/song.mp3");
    late.start_ms = 600_001;
    assert!(
        set(late).is_err(),
        "a start beyond ten minutes should be refused"
    );
    let mut faded = audio("media/song.mp3");
    faded.fade_in_ms = 600_001;
    assert!(
        set(faded).is_err(),
        "a fade beyond ten minutes should be refused"
    );
}

#[test]
fn a_video_with_a_sound_track_survives_saving_and_loading() {
    let (mut s, v, a, _b) = fixture();
    add_clip(&mut s, &v, &a, 2000);
    s.apply(
        Actor::User,
        None,
        &[Operation::SetVideo {
            id: v.clone(),
            prop: VideoProperty::Audio(Some(audio("media/song.mp3"))),
        }],
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    project::save(&dir.path().join("p.od"), &s).unwrap();
    let loaded = od_core::project::load(&dir.path().join("p.od")).unwrap();
    assert_eq!(
        loaded.document().video(&v).unwrap().audio,
        Some(audio("media/song.mp3"))
    );
}
