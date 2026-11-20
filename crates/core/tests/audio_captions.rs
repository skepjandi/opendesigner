#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::captions::{self, parse_subtitles};
use od_core::{
    Actor, AudioTrack, CaptionPreset, CaptionStyle, Document, Duck, NewCue, NewNode, NewTrack,
    NodeId, NodeKind, Operation, Session, TrackPatch, TrackRole, VideoProperty, project,
};

/// A session with an empty video that has one 4 s clip.
fn fixture() -> (Session, NodeId) {
    let mut s = Session::new(Document::new("v"));
    let page = s.add_page("Page 1").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut node = NewNode::new(NodeKind::Frame);
    node.width = 400.0;
    node.height = 800.0;
    let frame = s
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node,
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
                fps: 30,
                width: 400,
                height: 800,
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
            duration_ms: 4000,
            index: None,
            transition: None,
            motion: None,
        }],
    )
    .unwrap();
    (s, video)
}

fn set(s: &mut Session, v: &NodeId, prop: VideoProperty) -> Result<(), od_core::OpError> {
    s.apply(
        Actor::Agent("test".into()),
        None,
        &[Operation::SetVideo {
            id: v.clone(),
            prop,
        }],
    )
    .map(|_| ())
}

fn new_track(role: TrackRole, file: &str) -> NewTrack {
    serde_json::from_value(serde_json::json!({"role": role.name(), "file": file})).unwrap()
}

#[test]
fn tracks_are_added_changed_and_removed_as_undoable_edits_with_ids_that_are_not_reused_live() {
    let (mut s, v) = fixture();
    let (tracks, first) = s
        .document()
        .video(&v)
        .unwrap()
        .tracks_with_added(new_track(TrackRole::Voice, "media/vo.wav"))
        .unwrap();
    set(&mut s, &v, VideoProperty::Tracks(tracks)).unwrap();
    let (tracks, second) = s
        .document()
        .video(&v)
        .unwrap()
        .tracks_with_added(new_track(TrackRole::Music, "media/bed.mp3"))
        .unwrap();
    set(&mut s, &v, VideoProperty::Tracks(tracks)).unwrap();
    assert_eq!((first.as_str(), second.as_str()), ("a1", "a2"));

    let patch: TrackPatch = serde_json::from_str(
        r#"{"gain": 0.2, "duck": {"depth": 0.9}, "fade_in_ms": 300, "out_ms": 3000}"#,
    )
    .unwrap();
    let changed = s
        .document()
        .video(&v)
        .unwrap()
        .tracks_with_changed(&second, &patch)
        .unwrap();
    set(&mut s, &v, VideoProperty::Tracks(changed)).unwrap();
    let music = &s.document().video(&v).unwrap().tracks[1];
    assert_eq!((music.gain, music.out_ms), (0.2, Some(3000)));
    assert_eq!(music.duck.map(|d| d.depth), Some(0.9));

    let removed = s
        .document()
        .video(&v)
        .unwrap()
        .tracks_with_removed(&first)
        .unwrap();
    set(&mut s, &v, VideoProperty::Tracks(removed)).unwrap();
    assert_eq!(s.document().video(&v).unwrap().tracks.len(), 1);
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().video(&v).unwrap().tracks.len(), 2);
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().video(&v).unwrap().tracks[1].gain, 1.0);
}

#[test]
fn a_bad_track_is_refused_by_the_document_itself_and_leaves_it_unchanged() {
    let (mut s, v) = fixture();
    let (tracks, _) = s
        .document()
        .video(&v)
        .unwrap()
        .tracks_with_added(new_track(TrackRole::Music, "media/bed.mp3"))
        .unwrap();
    let mut loud = tracks.clone();
    loud[0].gain = 9.0;
    assert!(set(&mut s, &v, VideoProperty::Tracks(loud)).is_err());
    let mut ducking_voice = tracks.clone();
    ducking_voice[0].role = TrackRole::Voice;
    ducking_voice[0].duck = Some(Duck::default());
    assert!(set(&mut s, &v, VideoProperty::Tracks(ducking_voice)).is_err());
    let mut outside = tracks;
    outside[0].file = "../bed.mp3".into();
    assert!(set(&mut s, &v, VideoProperty::Tracks(outside)).is_err());
    assert!(s.document().video(&v).unwrap().tracks.is_empty());

    let mut full = Vec::new();
    for n in 0..=od_core::audio::MAX_TRACKS {
        let mut t = od_core::SoundTrack::from_legacy(&AudioTrack {
            file: "media/x.wav".into(),
            start_ms: 0,
            gain: 1.0,
            fade_in_ms: 0,
            fade_out_ms: 0,
        })
        .unwrap();
        t.id = NodeId::parse(&format!("a{n}")).unwrap();
        full.push(t);
    }
    assert!(set(&mut s, &v, VideoProperty::Tracks(full)).is_err());
}

#[test]
fn the_legacy_audio_is_track_zero_alongside_the_new_tracks() {
    let (mut s, v) = fixture();
    set(
        &mut s,
        &v,
        VideoProperty::Audio(Some(AudioTrack {
            file: "media/song.mp3".into(),
            start_ms: 100,
            gain: 0.5,
            fade_in_ms: 0,
            fade_out_ms: 0,
        })),
    )
    .unwrap();
    let (tracks, _) = s
        .document()
        .video(&v)
        .unwrap()
        .tracks_with_added(new_track(TrackRole::Voice, "media/vo.wav"))
        .unwrap();
    set(&mut s, &v, VideoProperty::Tracks(tracks)).unwrap();
    let all = s.document().video(&v).unwrap().sound_tracks();
    assert_eq!(all.len(), 2);
    assert_eq!((all[0].id.as_str(), all[0].gain), ("audio", 0.5));
    assert_eq!(all[1].role, TrackRole::Voice);
}

#[test]
fn captions_are_set_one_cue_at_a_time_and_stay_in_time_order_inside_the_video() {
    let (mut s, v) = fixture();
    let add = |s: &mut Session, start, end, text: &str| {
        let (c, id) = s.document().video(&v).unwrap().captions_with_cue(NewCue {
            start_ms: start,
            end_ms: end,
            text: text.into(),
            words: Vec::new(),
        })?;
        set(s, &v, VideoProperty::Captions(c))?;
        Ok::<_, od_core::OpError>(id)
    };
    let late = add(&mut s, 2000, 3000, "late").unwrap();
    let early = add(&mut s, 500, 1500, "early").unwrap();
    let cues = &s.document().video(&v).unwrap().captions.cues;
    assert_eq!(
        cues.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
        ["early", "late"]
    );
    assert_eq!((late.as_str(), early.as_str()), ("q1", "q2"));
    assert!(add(&mut s, 1000, 2500, "overlaps").is_err());
    assert!(add(&mut s, 3500, 4500, "past the end").is_err());
    assert!(add(&mut s, 100, 100, "empty span").is_err());

    let without = s
        .document()
        .video(&v)
        .unwrap()
        .captions_without_cue(&late)
        .unwrap();
    set(&mut s, &v, VideoProperty::Captions(without)).unwrap();
    assert_eq!(s.document().video(&v).unwrap().captions.cues.len(), 1);
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().video(&v).unwrap().captions.cues.len(), 2);
}

#[test]
fn a_style_is_set_with_a_preset_and_survives_undo_and_a_reload() {
    let (mut s, v) = fixture();
    let cues = parse_subtitles("1\n00:00:00,500 --> 00:00:01,500\nHello there\n").unwrap();
    let c = s
        .document()
        .video(&v)
        .unwrap()
        .captions_with_cues(cues)
        .unwrap();
    set(&mut s, &v, VideoProperty::Captions(c)).unwrap();
    let styled = s
        .document()
        .video(&v)
        .unwrap()
        .captions_with_style(CaptionStyle::preset(CaptionPreset::Karaoke))
        .unwrap();
    set(&mut s, &v, VideoProperty::Captions(styled.clone())).unwrap();

    let dir = tempfile::tempdir().unwrap();
    project::save(&dir.path().join("p.od"), &s).unwrap();
    let loaded = project::load(&dir.path().join("p.od")).unwrap();
    assert_eq!(loaded.document().video(&v).unwrap().captions, styled);

    s.undo(Actor::User).unwrap();
    assert_eq!(
        s.document().video(&v).unwrap().captions.style,
        CaptionStyle::default()
    );
}

#[test]
fn a_video_without_either_feature_serializes_exactly_as_before() {
    let (s, v) = fixture();
    let json = serde_json::to_value(s.document().video(&v).unwrap()).unwrap();
    let object = json.as_object().unwrap();
    assert!(!object.contains_key("tracks") && !object.contains_key("captions"));
}

#[test]
fn captions_may_run_past_a_shortened_video_and_an_undo_still_restores_them() {
    let (mut s, v) = fixture();
    let c = s
        .document()
        .video(&v)
        .unwrap()
        .captions_with_cues(parse_subtitles("1\n00:00:03,000 --> 00:00:04,000\nend\n").unwrap())
        .unwrap();
    set(&mut s, &v, VideoProperty::Captions(c)).unwrap();
    let clip = s.document().video(&v).unwrap().clips[0].id.clone();
    s.apply(
        Actor::User,
        None,
        &[Operation::SetClip {
            video: v.clone(),
            clip,
            prop: od_core::ClipProperty::DurationMs(1000),
        }],
    )
    .unwrap();
    assert!(captions::check_captions(&s.document().video(&v).unwrap().captions).is_ok());
    assert!(
        captions::check_within(&s.document().video(&v).unwrap().captions, 1000).is_err(),
        "a front end asking for strict bounds still sees the cue past the end"
    );
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().video(&v).unwrap().duration_ms(), 4000);
}

fn footage_video(duration_ms: u32, in_ms: u64) -> (Session, NodeId, NodeId) {
    let mut s = Session::new(Document::new("f"));
    let v = s
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "F".into(),
                fps: 30,
                width: 400,
                height: 800,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    let clip = s
        .apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame: None,
                footage: Some(od_core::Footage {
                    file: "media/talk.mp4".into(),
                    in_ms,
                }),
                duration_ms,
                index: None,
                transition: None,
                motion: Some(od_core::Motion {
                    scale_from: 1.0,
                    scale_to: 1.2,
                    ease: od_core::Ease::EaseInOut,
                }),
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    (s, v, clip)
}

#[test]
fn cutting_pauses_closes_the_gaps_in_one_undoable_transaction() {
    let (mut s, v, clip) = footage_video(10_000, 2_000);
    // pauses in the file's own time: one inside the clip (4 s to 6 s), one before it
    let (ops, cut) = s
        .document()
        .video(&v)
        .unwrap()
        .pause_cut_ops(&clip, &[(0, 2_500), (4_000, 6_000)], 200)
        .unwrap();
    assert_eq!(cut.kept, [(2_300, 4_200), (5_800, 12_000)]);
    assert_eq!(cut.removed_ms, 10_000 - 1_900 - 6_200);
    s.apply(Actor::Agent("test".into()), None, &ops).unwrap();

    let video = s.document().video(&v).unwrap();
    let plays: Vec<(u64, u32)> = video
        .clips
        .iter()
        .map(|c| (c.footage.as_ref().unwrap().in_ms, c.duration_ms))
        .collect();
    assert_eq!(plays, [(2_300, 1_900), (5_800, 6_200)]);
    assert_eq!(video.duration_ms(), 8_100);
    let zoom = |i: usize| {
        (
            video.clips[i].motion.scale_from,
            video.clips[i].motion.scale_to,
        )
    };
    assert_eq!(zoom(0).0, 1.0);
    assert!(
        (zoom(0).1 - zoom(1).0).abs() < 1e-3,
        "the zoom carries on across the cut"
    );
    assert!((zoom(1).1 - 1.2).abs() < 1e-3);

    s.undo(Actor::User).unwrap();
    let video = s.document().video(&v).unwrap();
    assert_eq!(video.clips.len(), 1);
    assert_eq!(
        (
            video.clips[0].duration_ms,
            video.clips[0].footage.as_ref().unwrap().in_ms
        ),
        (10_000, 2_000)
    );
}

#[test]
fn a_clip_with_no_pause_or_no_footage_is_not_cut() {
    let (s, v, clip) = footage_video(5_000, 0);
    let video = s.document().video(&v).unwrap();
    assert!(video.pause_cut_ops(&clip, &[], 0).is_err());
    assert!(
        video.pause_cut_ops(&clip, &[(0, 5_000)], 0).is_err(),
        "nothing would be left"
    );
    assert!(
        video
            .pause_cut_ops(&NodeId::parse("c_999").unwrap(), &[(1_000, 2_000)], 0)
            .is_err()
    );
    let (s, v, clip) = fixture_with_frame_clip();
    assert!(
        s.document()
            .video(&v)
            .unwrap()
            .pause_cut_ops(&clip, &[(1_000, 2_000)], 0)
            .is_err()
    );
}

fn fixture_with_frame_clip() -> (Session, NodeId, NodeId) {
    let (s, v) = fixture();
    let clip = s.document().video(&v).unwrap().clips[0].id.clone();
    (s, v, clip)
}
