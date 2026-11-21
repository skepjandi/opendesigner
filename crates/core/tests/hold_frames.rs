#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
//! `Video.hold_frames`: bounds, the default left out of the file, and the held time.

use od_core::{Actor, Document, Operation, Session, Video, VideoProperty, timebase};

fn video(s: &mut Session) -> od_core::NodeId {
    s.apply(
        Actor::User,
        None,
        &[Operation::CreateVideo {
            name: "v".into(),
            fps: 24,
            width: 64,
            height: 48,
        }],
    )
    .unwrap()
    .created[0]
        .clone()
}

fn hold(s: &mut Session, v: &od_core::NodeId, n: u32) -> Result<(), od_core::OpError> {
    let set = Operation::SetVideo {
        id: v.clone(),
        prop: VideoProperty::HoldFrames(n),
    };
    s.apply(Actor::User, None, &[set]).map(|_| ())
}

#[test]
fn hold_frames_is_one_to_six_undoes_and_the_default_is_not_written() {
    let mut s = Session::new(Document::new("d"));
    let v = video(&mut s);
    let json = serde_json::to_value(s.document().video(&v).unwrap()).unwrap();
    assert!(json.get("hold_frames").is_none());
    for bad in [0, 7] {
        assert!(hold(&mut s, &v, bad).is_err(), "{bad}");
    }
    hold(&mut s, &v, 6).unwrap();
    let json = serde_json::to_value(s.document().video(&v).unwrap()).unwrap();
    assert_eq!(json["hold_frames"], 6);
    let back: Video = serde_json::from_value(json).unwrap();
    assert_eq!(back.hold_frames, 6);
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().video(&v).unwrap().hold_frames, 1);
}

#[test]
fn an_old_video_without_the_field_holds_nothing() {
    let mut s = Session::new(Document::new("d"));
    let v = video(&mut s);
    let mut json = serde_json::to_value(s.document().video(&v).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("hold_frames");
    let old: Video = serde_json::from_value(json).unwrap();
    assert_eq!(old.hold_frames, 1);
    for ms in [0, 41, 42, 1234] {
        assert_eq!(old.held_ms(ms), ms);
    }
}

#[test]
fn held_time_is_the_start_of_the_group_and_stays_put_when_asked_again() {
    let mut s = Session::new(Document::new("d"));
    let v = video(&mut s);
    hold(&mut s, &v, 3).unwrap();
    let video = s.document().video(&v).unwrap();
    for n in 0..60 {
        let group = n - n % 3;
        let start = timebase::frame_ms(group, 24);
        assert_eq!(video.held_ms(timebase::frame_ms(n, 24)), start, "frame {n}");
        assert_eq!(video.held_ms(start), start);
    }
}
