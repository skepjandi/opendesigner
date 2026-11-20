//! Exports with several threads match exports with one, and footage played at another speed
//! comes out at the length the timeline gives it. The tests need `ffmpeg`; without one they say so
//! and pass.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::Command;

use od_agent::video_export::{self, Source};
use od_core::{
    Actor, ClipProperty, Ease, Footage, Look, Motion, NewNode, NodeId, NodeKind, Operation,
    Transition, TransitionKind, Workspace,
};
use od_media::Ffmpeg;

fn ffmpeg() -> Option<Ffmpeg> {
    let Ok(ffmpeg) = Ffmpeg::locate() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return None;
    };
    if ffmpeg.h264_encoder().is_err() {
        eprintln!("skipped: this ffmpeg has no H.264 encoder");
        return None;
    }
    Some(ffmpeg)
}

fn frame(ws: &mut Workspace, color: &str) -> NodeId {
    let root = ws.session.document().pages[0].root.id.clone();
    let mut node = NewNode::new(NodeKind::Frame);
    node.width = 100.0;
    node.height = 200.0;
    node.fill = od_core::Paint::parse(color).ok();
    let mut label = NewNode::new(NodeKind::Rect);
    label.x = 20.0;
    label.y = 40.0;
    label.width = 60.0;
    label.height = 30.0;
    label.fill = od_core::Paint::parse("#ffffff").ok();
    node.children.push(label);
    ws.session
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
        .clone()
}

fn video(ws: &mut Workspace, fps: u32) -> NodeId {
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::CreateVideo {
                name: "V".into(),
                fps,
                width: 100,
                height: 200,
            }],
        )
        .unwrap()
        .created[0]
        .clone()
}

fn clip(ws: &mut Workspace, v: &NodeId, source: Source1, ms: u32, t: Option<Transition>) -> NodeId {
    let (frame, footage) = match source {
        Source1::Frame(f) => (Some(f), None),
        Source1::Footage(f) => (None, Some(f)),
    };
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::AddClip {
                video: v.clone(),
                frame,
                footage,
                duration_ms: ms,
                index: None,
                transition: t,
                motion: Some(Motion {
                    scale_from: 1.0,
                    scale_to: 1.1,
                    ease: Ease::Linear,
                }),
            }],
        )
        .unwrap()
        .created[0]
        .clone()
}

enum Source1 {
    Frame(NodeId),
    Footage(Footage),
}

fn wipe(kind: TransitionKind) -> Option<Transition> {
    Some(Transition {
        kind,
        duration_ms: 400,
        ease: Ease::EaseInOut,
    })
}

fn export(ws: &Workspace, v: &NodeId, ffmpeg: &Ffmpeg, file: &str, workers: usize) {
    let source = Source::of(ws);
    video_export::export_video_with(
        &source,
        v,
        file,
        None,
        None,
        ffmpeg,
        workers,
        &mut |_, _| true,
    )
    .unwrap();
}

/// A hash of every decoded picture of the file. The encoder may differ in a byte or two of its
/// container from run to run (VideoToolbox does) even for the same input, so exports are
/// compared by what they show.
fn picture_hash(ffmpeg: &Ffmpeg, file: &Path) -> String {
    let out = Command::new(&ffmpeg.path)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(file)
        .args(["-map", "0:v", "-f", "md5", "-"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Four seconds of moving test picture and a tone, 30 fps, as `media/a.mp4`.
fn make_footage(ffmpeg: &Ffmpeg, project: &Path) {
    std::fs::create_dir_all(project.join("media")).unwrap();
    let status = Command::new(&ffmpeg.path)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=160x90:rate=30:duration=4",
        ])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=4"])
        .args([
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(project.join("media/a.mp4"))
        .status();
    assert!(
        status.is_ok_and(|s| s.success()),
        "could not make test footage"
    );
}

#[test]
fn one_thread_and_several_write_the_same_file_for_design_clips() {
    let Some(ffmpeg) = ffmpeg() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("p.od");
    let mut ws = Workspace::create(&project, "Parallel").unwrap();
    let frames: Vec<NodeId> = ["#cc3300", "#0066cc", "#33aa55"]
        .iter()
        .map(|c| frame(&mut ws, c))
        .collect();
    let v = video(&mut ws, 24);
    let kinds = [
        TransitionKind::Cut,
        TransitionKind::WipeLeft,
        TransitionKind::BlurDissolve,
    ];
    let mut first = None;
    for (f, kind) in frames.into_iter().zip(kinds) {
        let id = clip(
            &mut ws,
            &v,
            Source1::Frame(f),
            1000,
            wipe(kind).filter(|t| t.kind != TransitionKind::Cut),
        );
        first.get_or_insert(id);
    }
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::SetClip {
                video: v.clone(),
                clip: first.unwrap(),
                prop: ClipProperty::Grade(Some(Look::Warm.grade())),
            }],
        )
        .unwrap();
    ws.save().unwrap();
    export(&ws, &v, &ffmpeg, "one.mp4", 1);
    export(&ws, &v, &ffmpeg, "many.mp4", 4);
    assert_eq!(
        picture_hash(&ffmpeg, &project.join("exports/one.mp4")),
        picture_hash(&ffmpeg, &project.join("exports/many.mp4")),
        "one thread and four show different pictures"
    );
}

#[test]
fn footage_decoded_by_several_workers_writes_the_same_file_and_speed_sets_the_length() {
    let Some(ffmpeg) = ffmpeg() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("p.od");
    let mut ws = Workspace::create(&project, "Footage").unwrap();
    make_footage(&ffmpeg, &project);
    let v = video(&mut ws, 30);
    let footage = |in_ms| {
        Source1::Footage(Footage {
            file: "media/a.mp4".into(),
            in_ms,
        })
    };
    let a = clip(&mut ws, &v, footage(0), 1500, None);
    clip(&mut ws, &v, footage(2000), 1000, wipe(TransitionKind::Iris));
    let set = |ws: &mut Workspace, clip: &NodeId, prop| {
        ws.session.apply(
            Actor::User,
            None,
            &[Operation::SetClip {
                video: v.clone(),
                clip: clip.clone(),
                prop,
            }],
        )
    };
    // 2 s of timeline at 2x plays 4 s of the 4 s file; the second clip plays 2 s to 3 s
    set(&mut ws, &a, ClipProperty::DurationMs(2000)).unwrap();
    set(&mut ws, &a, ClipProperty::Speed(2.0)).unwrap();
    ws.save().unwrap();

    export(&ws, &v, &ffmpeg, "one.mp4", 1);
    export(&ws, &v, &ffmpeg, "many.mp4", 4);
    assert_eq!(
        picture_hash(&ffmpeg, &project.join("exports/one.mp4")),
        picture_hash(&ffmpeg, &project.join("exports/many.mp4")),
        "one thread and four show different pictures"
    );

    let info = ffmpeg.probe_file(&project.join("exports/one.mp4")).unwrap();
    // 2000 ms + 1000 ms - the 400 ms the iris overlaps
    assert!(info.has_audio);
    assert!(
        info.duration_ms.abs_diff(2600) <= 100,
        "{} ms",
        info.duration_ms
    );

    // asking for more of the file than there is must be refused with the numbers
    set(&mut ws, &a, ClipProperty::Speed(4.0)).unwrap();
    let source = Source::of(&ws);
    let err = video_export::export_video_with(
        &source,
        &v,
        "bad.mp4",
        None,
        None,
        &ffmpeg,
        2,
        &mut |_, _| true,
    )
    .err()
    .unwrap();
    assert!(err.0.contains("only 4000 ms long"), "{}", err.0);
    assert!(!project.join("exports/bad.mp4").exists());
    let leftovers: Vec<_> = std::fs::read_dir(project.join("exports"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files left behind: {leftovers:?}"
    );
}

#[test]
fn a_speed_ramp_keeps_the_clip_length_and_its_sound() {
    let Some(ffmpeg) = ffmpeg() else { return };
    let tmp = tempfile::tempdir().unwrap();
    let project = tmp.path().join("p.od");
    let mut ws = Workspace::create(&project, "Ramp").unwrap();
    make_footage(&ffmpeg, &project);
    let v = video(&mut ws, 30);
    let c = clip(
        &mut ws,
        &v,
        Source1::Footage(Footage {
            file: "media/a.mp4".into(),
            in_ms: 0,
        }),
        2000,
        None,
    );
    let key = |at_ms, rate| od_core::SpeedKey {
        at_ms,
        rate,
        ease: Ease::Linear,
    };
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::SetClip {
                video: v.clone(),
                clip: c,
                // 0.5x to 1.5x over 2 s of timeline plays 2 s of file
                prop: ClipProperty::SpeedRamp(vec![key(0, 0.5), key(2000, 1.5)]),
            }],
        )
        .unwrap();
    ws.save().unwrap();
    export(&ws, &v, &ffmpeg, "ramp.mp4", 2);
    let info = ffmpeg
        .probe_file(&project.join("exports/ramp.mp4"))
        .unwrap();
    assert!(info.has_audio);
    assert!(
        info.duration_ms.abs_diff(2000) <= 100,
        "{} ms",
        info.duration_ms
    );
}
