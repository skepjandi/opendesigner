#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, Operation, Workspace};
use serde_json::{Value, json};

use crate::tools::{CallCtx, Content, Turns};

struct Fixture {
    _dir: tempfile::TempDir,
    ws: Workspace,
    video: String,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Timeline").unwrap();
    let ops: Vec<Operation> = serde_json::from_value(json!([
        {"op": "create_video", "name": "Cut", "fps": 25, "width": 400, "height": 300}
    ]))
    .unwrap();
    ws.session.apply(Actor::User, None, &ops).unwrap();
    let video = ws.session.document().videos[0].id.to_string();
    let ops: Vec<Operation> = serde_json::from_value(json!([
        {"op": "add_clip", "video": video, "footage": {"file": "media/a.mp4", "in_ms": 2000},
         "duration_ms": 4000}
    ]))
    .unwrap();
    ws.session.apply(Actor::User, None, &ops).unwrap();
    Fixture {
        _dir: dir,
        ws,
        video,
    }
}

fn call(ws: &mut Workspace, name: &str, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws,
        actor: Actor::agent("tester").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    match crate::tools::call(&mut ctx, name, args) {
        Ok(out) => match out.content.first() {
            Some(Content::Text(t)) => Ok(serde_json::from_str(t).unwrap()),
            _ => Err("no text".into()),
        },
        Err(e) => Err(e.0),
    }
}

#[test]
fn an_edl_is_written_to_exports_with_the_clip_in_source_and_record_time() {
    let mut f = fixture();
    let out = call(
        &mut f.ws,
        "export_timeline",
        json!({"video": f.video, "format": "edl", "file": "cut.edl"}),
    )
    .unwrap();
    assert_eq!(out["file"], "exports/cut.edl");
    assert_eq!(out["events"], 1);
    let text = std::fs::read_to_string(f.ws.dir().unwrap().join("exports/cut.edl")).unwrap();
    assert!(
        text.starts_with("TITLE: Cut\nFCM: NON-DROP FRAME\n"),
        "{text}"
    );
    assert!(
        text.contains("00:00:02:00 00:00:06:00 01:00:00:00 01:00:04:00"),
        "{text}"
    );
    assert!(text.contains("* SOURCE FILE: media/a.mp4"));
}

#[test]
fn captions_become_srt_and_webvtt_and_a_video_without_captions_is_refused() {
    let mut f = fixture();
    let err = call(
        &mut f.ws,
        "export_timeline",
        json!({"video": f.video, "format": "srt"}),
    )
    .unwrap_err();
    assert!(err.contains("no captions"), "{err}");

    call(
        &mut f.ws,
        "import_captions",
        json!({"video": f.video, "format": "srt",
               "text": "1\n00:00:01,000 --> 00:00:02,500\nHello there\n"}),
    )
    .unwrap();
    for (format, expect) in [
        ("srt", "00:00:01,000 --> 00:00:02,500\nHello there"),
        (
            "vtt",
            "WEBVTT\n\n00:00:01.000 --> 00:00:02.500\nHello there",
        ),
    ] {
        let out = call(
            &mut f.ws,
            "export_timeline",
            json!({"video": f.video, "format": format}),
        )
        .unwrap();
        assert_eq!(out["cues"], 1);
        let path =
            f.ws.dir()
                .unwrap()
                .join(format!("exports/timeline.{format}"));
        assert!(
            std::fs::read_to_string(path).unwrap().contains(expect),
            "{format}"
        );
    }
}

#[test]
fn only_plain_names_with_the_right_ending_are_written() {
    let mut f = fixture();
    for bad in [
        "../cut.edl",
        "cut.txt",
        "a/b.edl",
        ".hidden.edl",
        "cut.srt",
        ".edl",
    ] {
        let err = call(
            &mut f.ws,
            "export_timeline",
            json!({"video": f.video, "format": "edl", "file": bad}),
        )
        .unwrap_err();
        assert!(err.contains("plain name"), "{bad}: {err}");
    }
    assert!(
        call(
            &mut f.ws,
            "export_timeline",
            json!({"video": f.video, "format": "xml"})
        )
        .is_err()
    );
    assert!(
        call(
            &mut f.ws,
            "export_timeline",
            json!({"video": "nope", "format": "edl"})
        )
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn an_exports_folder_that_is_a_link_is_not_written_through() {
    let mut f = fixture();
    let elsewhere = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), f.ws.dir().unwrap().join("exports")).unwrap();
    let err = call(
        &mut f.ws,
        "export_timeline",
        json!({"video": f.video, "format": "edl"}),
    )
    .unwrap_err();
    assert!(err.contains("link"), "{err}");
    assert!(
        std::fs::read_dir(elsewhere.path())
            .unwrap()
            .next()
            .is_none()
    );
}
