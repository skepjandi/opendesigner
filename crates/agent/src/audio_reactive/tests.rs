#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::process::{Command, Stdio};

use od_core::animation::AnimProp;
use od_core::{Actor, NodeId, Operation, Workspace};
use serde_json::{Value, json};

use super::*;
use crate::tools::Turns;

struct Fixture {
    _dir: tempfile::TempDir,
    ws: Workspace,
    video: String,
    node: NodeId,
}

fn fixture(video_ms: u32) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Reactive").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let apply = |ws: &mut Workspace, ops: Value| {
        let ops: Vec<Operation> = serde_json::from_value(ops).unwrap();
        ws.session.apply(Actor::User, None, &ops).unwrap()
    };
    let frame = apply(
        &mut ws,
        json!([{"op": "create", "parent": root.as_str(), "node":
            {"kind": "frame", "name": "scene", "width": 400, "height": 300, "fill": "#000000"}}]),
    )
    .created[0]
        .to_string();
    let node = apply(
        &mut ws,
        json!([{"op": "create", "parent": frame, "node":
            {"kind": "rect", "name": "Bar", "x": 10, "y": 10, "width": 50, "height": 50,
             "fill": "#ff0000"}}]),
    )
    .created[0]
        .clone();
    apply(
        &mut ws,
        json!([{"op": "create_video", "name": "V", "fps": 30, "width": 400, "height": 300}]),
    );
    let video = ws.session.document().videos[0].id.to_string();
    apply(
        &mut ws,
        json!([{"op": "add_clip", "video": video, "frame": frame, "duration_ms": video_ms}]),
    );
    Fixture {
        _dir: dir,
        ws,
        video,
        node,
    }
}

/// The arguments every test starts from: scale the bar by the sound in `file`, plus `extra`.
fn args(f: &Fixture, file: &str, extra: Value) -> Value {
    let mut args = json!({"video": f.video, "node": f.node, "prop": "scale", "file": file});
    args.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    args
}

fn call(f: &mut Fixture, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws: &mut f.ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, "audio_reactive", args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

impl Fixture {
    fn keys(&self, prop: AnimProp) -> Vec<(u64, f64)> {
        self.ws.session.document().videos[0]
            .animations
            .iter()
            .find(|t| t.node == self.node && t.prop == prop)
            .map(|t| t.keys.iter().map(|k| (k.at_ms, k.value)).collect())
            .unwrap_or_default()
    }
}

#[test]
fn keys_sit_on_the_frame_grid_and_a_flat_stretch_keeps_only_its_ends() {
    let keys = keys_of(&[0.0, 0.0, 0.0, 1.0, 1.0, 0.0], 0.0, 2.0, 0, 2, 30);
    assert_eq!(
        keys,
        [(0, 0.0), (133, 0.0), (200, 2.0), (267, 2.0), (333, 0.0)]
    );
    let offset = keys_of(&[0.5, 1.0], 1.0, 3.0, 30, 1, 30);
    assert_eq!(offset, [(1000, 2.0), (1033, 3.0)]);
}

// ---- needs ffmpeg ----

fn ffmpeg_here() -> Option<od_media::Ffmpeg> {
    od_media::Ffmpeg::locate().ok()
}

/// `media/<name>`: `tone_hz` for `loud_s` seconds, then `quiet_s` of silence.
fn make_sound(
    ff: &od_media::Ffmpeg,
    dir: &Path,
    name: &str,
    tone_hz: u32,
    loud_s: u32,
    quiet_s: u32,
) {
    std::fs::create_dir_all(dir.join("media")).unwrap();
    let mut command = Command::new(&ff.path);
    command
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(["-f", "lavfi", "-i"])
        .arg(format!("sine=f={tone_hz}:d={loud_s}:r=16000"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!("anullsrc=r=16000:cl=mono:d={}", quiet_s.max(1)));
    if quiet_s > 0 {
        command.args([
            "-filter_complex",
            "[0:a][1:a]concat=n=2:v=0:a=1[a]",
            "-map",
            "[a]",
        ]);
    } else {
        command.args(["-map", "0:a"]);
    }
    let status = command
        .arg(dir.join("media").join(name))
        .stdin(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn a_value_follows_the_loudness_of_a_sound_and_falls_back_in_silence() {
    let Some(ff) = ffmpeg_here() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    let mut f = fixture(2000);
    make_sound(&ff, f.ws.dir().unwrap(), "beat.wav", 440, 1, 1);
    let a = args(
        &f,
        "media/beat.wav",
        json!({"min": 1.0, "max": 1.5, "attack": 1.0, "release": 1.0}),
    );
    let out = call(&mut f, a).unwrap();
    assert!(
        out["audio_reactive"]["keys"].as_u64().unwrap() >= 2,
        "{out}"
    );
    let keys = f.keys(AnimProp::Scale);
    let at = |ms: u64| keys.iter().rev().find(|(t, _)| *t <= ms).unwrap().1;
    assert!(at(500) > 1.45, "loud half: {}", at(500));
    assert!(at(1600) < 1.05, "quiet half: {}", at(1600));
    assert!(keys.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(keys.iter().all(|(_, v)| (1.0..=1.5).contains(v)));

    let pulse = args(
        &f,
        "media/beat.wav",
        json!({"additive": true, "min": 0.0, "max": 0.12}),
    );
    call(&mut f, pulse).unwrap();
    let mouth = args(
        &f,
        "media/beat.wav",
        json!({"prop": "scale_y", "min": 0.1, "max": 1.0, "attack": 1.0}),
    );
    call(&mut f, mouth).unwrap();
    let tracks = &f.ws.session.document().videos[0].animations;
    let on_node = |prop: AnimProp| {
        tracks
            .iter()
            .filter(|t| t.node == f.node && t.prop == prop)
            .count()
    };
    assert_eq!(
        on_node(AnimProp::Scale),
        2,
        "one replacing and one additive track"
    );
    assert_eq!(on_node(AnimProp::ScaleY), 1);
}

#[test]
fn a_band_the_sound_does_not_have_is_refused_instead_of_stretched() {
    let Some(ff) = ffmpeg_here() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    let mut f = fixture(2000);
    make_sound(&ff, f.ws.dir().unwrap(), "bass.wav", 100, 1, 1);
    let a = args(
        &f,
        "media/bass.wav",
        json!({"prop": "opacity", "band": "low", "attack": 1.0}),
    );
    let low = call(&mut f, a);
    assert!(low.is_ok(), "{low:?}");
    let a = args(&f, "media/bass.wav", json!({"prop": "y", "band": "high"}));
    let err = call(&mut f, a).unwrap_err();
    assert!(err.contains("no movement"), "{err}");
    assert!(f.keys(AnimProp::Y).is_empty());
}

#[test]
fn a_range_with_too_many_keys_is_refused_with_what_would_fit() {
    let Some(ff) = ffmpeg_here() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    let mut f = fixture(40_000);
    make_sound(&ff, f.ws.dir().unwrap(), "long.wav", 440, 40, 0);
    let a = args(&f, "media/long.wav", json!({}));
    let err = call(&mut f, a).unwrap_err();
    assert!(err.contains("keys_per_second 12 or less"), "{err}");
    let a = args(&f, "media/long.wav", json!({"keys_per_second": 10}));
    assert!(call(&mut f, a).is_ok());
}

#[test]
fn arguments_are_checked_before_ffmpeg_runs() {
    let mut f = fixture(1000);
    for (extra, expect) in [
        (json!({"band": "bass"}), "band is level"),
        (json!({"keys_per_second": 99}), "keys_per_second"),
        (json!({"floor": 0.95}), "floor"),
        (json!({"start_ms": 5000}), "not inside the video"),
        (json!({"file": "media/x.txt"}), "not an audio or footage"),
        (json!({"video": "nope"}), "does not exist"),
    ] {
        let a = args(&f, "media/x.wav", extra);
        let err = call(&mut f, a).unwrap_err();
        assert!(err.contains(expect), "{expect}: {err}");
    }
}
