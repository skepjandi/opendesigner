#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::{Command, Stdio};

use od_core::{Actor, Operation, Workspace};
use serde_json::{Value, json};

use super::*;
use crate::tools::Turns;

struct Fixture {
    _dir: tempfile::TempDir,
    ws: Workspace,
    video: String,
    design: String,
    footage: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Fit").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let apply = |ws: &mut Workspace, ops: Value| {
        let ops: Vec<Operation> = serde_json::from_value(ops).unwrap();
        ws.session.apply(Actor::User, None, &ops).unwrap()
    };
    let frame = apply(
        &mut ws,
        json!([{"op": "create", "parent": root.as_str(), "node":
            {"kind": "frame", "name": "card", "width": 64, "height": 48, "fill": "#000000"}}]),
    )
    .created[0]
        .to_string();
    apply(
        &mut ws,
        json!([{"op": "create_video", "name": "V", "fps": 10, "width": 64, "height": 48}]),
    );
    let video = ws.session.document().videos[0].id.to_string();
    apply(
        &mut ws,
        json!([
            {"op": "add_clip", "video": video, "frame": frame, "duration_ms": 1000},
            {"op": "add_clip", "video": video, "footage": {"file": "media/clip.mp4", "in_ms": 1000},
             "duration_ms": 1000}
        ]),
    );
    let clips = &ws.session.document().videos[0].clips;
    let (design, footage) = (clips[0].id.to_string(), clips[1].id.to_string());
    Fixture {
        _dir: dir,
        ws,
        video,
        design,
        footage,
    }
}

fn call(f: &mut Fixture, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws: &mut f.ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, "fit_clip_to_media", args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

#[test]
fn the_rest_of_a_file_at_a_speed_is_the_length_left_divided_by_it() {
    assert_eq!(rest_of_file_ms(4000, 1000, 2.0), Some(1500));
    assert_eq!(rest_of_file_ms(4000, 1000, 0.5), Some(6000));
    assert_eq!(rest_of_file_ms(1000, 1000, 1.0), None);
}

#[test]
fn refusals_come_before_ffmpeg() {
    let mut f = fixture();
    let design = f.design.clone();
    let video = f.video.clone();
    let err = call(&mut f, json!({"video": video, "clip": design})).unwrap_err();
    assert!(err.contains("needs `file`"), "{err}");
    let err = call(&mut f, json!({"video": video, "clip": "nope"})).unwrap_err();
    assert!(
        err.contains("not in video") || err.contains("invalid"),
        "{err}"
    );
    let err = call(
        &mut f,
        json!({"video": video, "clip": design, "file": "media/x.txt"}),
    )
    .unwrap_err();
    assert!(err.contains("not an audio or footage"), "{err}");
}

fn make(ff: &od_media::Ffmpeg, dir: &std::path::Path, input: &[&str], name: &str, extra: &[&str]) {
    std::fs::create_dir_all(dir.join("media")).unwrap();
    let status = Command::new(&ff.path)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(input)
        .args(extra)
        .arg(dir.join("media").join(name))
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn a_clip_takes_its_length_from_a_sound_or_from_the_rest_of_its_own_footage() {
    let Ok(ff) = od_media::Ffmpeg::locate() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    let mut f = fixture();
    let dir = f.ws.dir().unwrap().to_path_buf();
    make(
        &ff,
        &dir,
        &["-f", "lavfi", "-i", "sine=f=440:d=2.5:r=16000"],
        "voice.wav",
        &[],
    );
    make(
        &ff,
        &dir,
        &["-f", "lavfi", "-i", "color=c=gray:s=64x48:d=4:r=10"],
        "clip.mp4",
        &["-c:v", "mpeg4"],
    );
    let (video, design, footage) = (f.video.clone(), f.design.clone(), f.footage.clone());

    let out = call(
        &mut f,
        json!({"video": video, "clip": design, "file": "media/voice.wav", "pad_ms": 500}),
    )
    .unwrap();
    assert!(
        out["fit"]["file_ms"].as_u64().unwrap().abs_diff(2500) <= 50,
        "{out}"
    );
    let first = f.ws.session.document().videos[0].clips[0].duration_ms;
    assert!(u64::from(first).abs_diff(3000) <= 50, "{first}");
    assert_eq!(out["fit"]["capped"], false);

    f.ws.session
        .apply(
            Actor::User,
            None,
            &[
                serde_json::from_value::<Operation>(json!({"op": "set_clip", "video": video,
                "clip": footage, "prop": {"name": "speed", "value": 2.0}}))
                .unwrap(),
            ],
        )
        .unwrap();
    let out = call(&mut f, json!({"video": video, "clip": footage})).unwrap();
    let second = f.ws.session.document().videos[0].clips[1].duration_ms;
    assert!(u64::from(second).abs_diff(1500) <= 50, "{second}: {out}");
    assert!(
        out["fit"]["video_ms_after"].as_u64().unwrap()
            > out["fit"]["video_ms_before"].as_u64().unwrap()
            || out["fit"]["video_ms_after"].as_u64().unwrap()
                == u64::from(first) + u64::from(second)
    );
}
