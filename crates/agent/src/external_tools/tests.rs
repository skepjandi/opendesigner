#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The external-program tools with fake programs: scripts that print canned JSON in each real
//! program's format. The speech tests need a real `ffmpeg` (to make the 16 kHz WAV) and skip
//! without one.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use od_core::{Actor, Workspace};
use serde_json::{Value, json};

use super::*;
use crate::tools::{Content, Turns};

const CPP_JSON: &str = r#"{"transcription": [
  {"offsets": {"from": 300, "to": 700}, "text": " Hello"},
  {"offsets": {"from": 700, "to": 1100}, "text": " there."},
  {"offsets": {"from": 2500, "to": 2900}, "text": " Bye"}
]}"#;

struct Project {
    ws: Workspace,
    dir: PathBuf,
    bin: PathBuf,
    _tmp: tempfile::TempDir,
}

fn project() -> Project {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "External").unwrap();
    let dir = ws.dir().unwrap().to_path_buf();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(dir.join("media")).unwrap();
    Project {
        ws,
        dir,
        bin,
        _tmp: tmp,
    }
}

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

fn fake_cpp(bin: &Path) -> PathBuf {
    script(
        bin,
        "whisper-cli",
        &format!(
            "while [ $# -gt 0 ]; do [ \"$1\" = -of ] && of=\"$2\"; shift; done\ncat > \"$of.json\" <<'EOF'\n{CPP_JSON}\nEOF\n"
        ),
    )
}

fn programs(bin: &Path, model: Option<&str>) -> Programs {
    let search = Search {
        explicit: None,
        app_dirs: vec![],
        path_dirs: vec![bin.to_path_buf()],
    };
    Programs {
        whisper: search.clone(),
        rembg: search,
        model: model.map(str::to_string),
    }
}

fn run(p: &mut Project, name: &str, args: Value, progs: &Programs) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws: &mut p.ws,
        actor: Actor::agent("claude").unwrap(),
        turns: &mut turns,
        extra_servers: &[],
    };
    call_with(&mut ctx, name, args, progs)
        .map(|out| match &out.content[0] {
            Content::Text(t) => serde_json::from_str(t).unwrap(),
            Content::Image { .. } => Value::Null,
        })
        .map_err(|e| e.0)
}

fn has_ffmpeg() -> bool {
    Ffmpeg::locate().is_ok()
}

fn make_speech_file(p: &Project, name: &str) {
    let spec = crate::synth::Spec::new(crate::synth::Kind::Drone, 3000);
    crate::synth::make(&p.dir, &spec, Some(name)).unwrap();
}

fn add_video(p: &mut Project) -> String {
    let root = p.ws.session.document().pages[0].root.id.to_string();
    let ops = |v: Value| serde_json::from_value::<Vec<od_core::Operation>>(v).unwrap();
    let frame = p
        .ws
        .session
        .apply(
            Actor::User,
            None,
            &ops(json!([{"op":"create","parent":root,"node":{"kind":"frame","name":"F","width":200,"height":400,"fill":"#102030"}}])),
        )
        .unwrap()
        .created[0]
        .to_string();
    let video =
        p.ws.session
            .apply(
                Actor::User,
                None,
                &ops(json!([{"op":"create_video","name":"V","fps":10,"width":200,"height":400}])),
            )
            .unwrap()
            .created[0]
            .to_string();
    p.ws.session
        .apply(
            Actor::User,
            None,
            &ops(json!([{"op":"add_clip","video":video,"frame":frame,"duration_ms":5000}])),
        )
        .unwrap();
    video
}

#[test]
fn the_three_tools_are_defined_once_with_the_right_hints() {
    let defs = definitions();
    assert_eq!(defs.len(), NAMES.len());
    for name in NAMES {
        assert_eq!(
            defs.iter().filter(|d| d["name"] == name).count(),
            1,
            "{name}"
        );
    }
    let by = |n: &str| defs.iter().find(|d| d["name"] == n).unwrap().clone();
    assert_eq!(by("get_capabilities")["annotations"]["readOnlyHint"], true);
    for n in ["transcribe", "remove_background"] {
        let d = by(n)["description"].as_str().unwrap().to_string();
        assert!(
            d.contains("get_capabilities") && d.contains("install"),
            "{n}"
        );
        assert_eq!(
            by(n)["annotations"]["openWorldHint"],
            false,
            "{n}: we reach no network"
        );
    }
    assert!(
        crate::tools::definitions()
            .iter()
            .any(|d| d["name"] == "transcribe")
    );
}

#[test]
fn the_capabilities_report_names_what_to_install_for_what_is_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let nowhere = Search {
        explicit: None,
        app_dirs: vec![],
        path_dirs: vec![tmp.path().to_path_buf()],
    };
    let caps = Capabilities::probe_with(false, &nowhere, &nowhere, &nowhere, None);
    let report = capabilities_json(&caps);
    assert_eq!(report["can"]["transcribe"], false);
    assert_eq!(report["can"]["remove_background"], false);
    let rembg = report["programs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "rembg")
        .unwrap();
    assert_eq!(
        (
            rembg["found"].clone(),
            rembg["install"].as_str().unwrap().contains("pip")
        ),
        (json!(false), true)
    );
    assert!(report["missing"].as_array().unwrap().len() >= 3);
    assert!(report["note"].as_str().unwrap().contains("never installs"));
}

#[test]
fn guidance_mentions_missing_programs_in_one_line() {
    if let Some(note) = missing_note() {
        assert!(!note.contains('\n') && note.contains("get_capabilities"));
    }
}

#[test]
fn transcribe_returns_words_pauses_and_text_and_changes_nothing_without_apply() {
    if !has_ffmpeg() {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    }
    let mut p = project();
    make_speech_file(&p, "talk");
    fake_cpp(&p.bin);
    std::fs::write(p.dir.join("media/ggml-tiny.bin"), b"model").unwrap();
    let progs = programs(&p.bin, None);
    let before = p.ws.session.document().clone();

    let out = run(
        &mut p,
        "transcribe",
        json!({"file": "media/talk.wav", "model": "ggml-tiny.bin", "language": "en"}),
        &progs,
    )
    .unwrap();

    assert_eq!(out["program"], "whisper-cli");
    assert_eq!(out["text"], "Hello there. Bye");
    assert_eq!(out["word_count"], 3);
    assert_eq!(out["words"][2]["start_ms"], 2500);
    assert_eq!(
        out["pauses"],
        json!([{"start_ms": 1100, "end_ms": 2500, "duration_ms": 1400}])
    );
    assert_eq!(&before, p.ws.session.document(), "no apply, no change");
    let mut media: Vec<String> = std::fs::read_dir(p.dir.join("media"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    media.sort();
    assert_eq!(
        media,
        [
            "ggml-tiny.bin",
            "talk.license.json",
            "talk.synth.json",
            "talk.wav"
        ],
        "nothing written into the project"
    );
}

#[test]
fn transcribe_with_apply_sets_word_timed_captions_attributed_to_the_agent() {
    if !has_ffmpeg() {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    }
    let mut p = project();
    make_speech_file(&p, "talk");
    fake_cpp(&p.bin);
    let model = p.dir.join("media/ggml.bin");
    std::fs::write(&model, b"model").unwrap();
    let progs = programs(&p.bin, model.to_str());
    let video = add_video(&mut p);

    let out = run(
        &mut p,
        "transcribe",
        json!({"file": "media/talk.wav", "video": video, "apply": true, "offset_ms": 1000, "summary": "captions"}),
        &progs,
    )
    .unwrap();

    assert!(out["captions"]["cues"].as_u64().unwrap() >= 1, "{out}");
    let captions = &p
        .ws
        .session
        .document()
        .video(&od_core::NodeId::parse(&video).unwrap())
        .unwrap()
        .captions;
    let first = &captions.cues[0];
    assert_eq!(first.words[0].text, "Hello");
    assert_eq!(first.words[0].start_ms, 1300, "shifted by offset_ms");
    assert_eq!(
        p.ws.session.log().last().unwrap().actor.to_string(),
        "agent:claude"
    );
    p.ws.session.undo(Actor::User).unwrap();
    let video_id = od_core::NodeId::parse(&video).unwrap();
    assert!(
        p.ws.session
            .document()
            .video(&video_id)
            .unwrap()
            .captions
            .is_empty(),
        "one undo removes them"
    );
}

#[test]
fn transcribe_refuses_bad_requests_before_running_anything() {
    let mut p = project();
    fake_cpp(&p.bin);
    let progs = programs(&p.bin, None);
    let mut err = |args: Value| run(&mut p, "transcribe", args, &progs).unwrap_err();
    assert!(err(json!({})).contains("`file`"));
    assert!(err(json!({"file": "media/a.wav", "video": "v_1"})).contains("not both"));
    assert!(err(json!({"file": "media/a.wav", "language": "english"})).contains("language"));
    assert!(err(json!({"file": "media/a.png"})).contains("not an audio or footage"));
    assert!(err(json!({"file": "../a.wav"})).contains("media/"));
    assert!(err(json!({"file": "media/a.wav", "apply": true})).contains("needs `video`"));
    assert!(err(json!({"file": "media/a.wav", "timeout_s": 1})).contains("timeout_s"));
    assert!(err(json!({"file": "media/a.wav", "min_pause_ms": 5})).contains("min_pause_ms"));
    assert!(err(json!({"file": "media/a.wav", "unknown": 1})).contains("invalid arguments"));
}

#[test]
fn transcribe_without_a_program_a_model_or_with_a_hostile_model_name_says_what_to_do() {
    let mut p = project();
    make_speech_file(&p, "talk");
    let empty = tempfile::tempdir().unwrap();
    let args = json!({"file": "media/talk.wav"});

    let none = run(
        &mut p,
        "transcribe",
        args.clone(),
        &programs(empty.path(), None),
    )
    .unwrap_err();
    assert!(
        none.contains("pip install openai-whisper") && none.contains("OPENDESIGNER_WHISPER"),
        "{none}"
    );

    fake_cpp(&p.bin);
    let with_cpp = programs(&p.bin, None);
    let no_model = run(&mut p, "transcribe", args, &with_cpp).unwrap_err();
    assert!(
        no_model.contains("OPENDESIGNER_WHISPER_MODEL"),
        "{no_model}"
    );

    for bad in [
        "../x.bin",
        "/etc/hosts",
        "sub/m.bin",
        ".m.bin",
        "absent.bin",
    ] {
        let e = run(
            &mut p,
            "transcribe",
            json!({"file": "media/talk.wav", "model": bad}),
            &with_cpp,
        )
        .unwrap_err();
        assert!(e.contains("model"), "{bad}: {e}");
    }
}

fn png_file(path: &Path, rgba: [u8; 4]) {
    let mut enc = png::Encoder::new(std::fs::File::create(path).unwrap(), 2, 2);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().unwrap();
    w.write_image_data(&rgba.repeat(4)).unwrap();
}

#[test]
fn remove_background_writes_a_cutout_asset_the_project_can_use() {
    let mut p = project();
    png_file(&p.dir.join("media/photo.png"), [200, 10, 10, 255]);
    let canned = p.bin.join("canned.png");
    png_file(&canned, [200, 10, 10, 0]);
    script(
        &p.bin,
        "rembg",
        &format!("for last; do :; done\ncp '{}' \"$last\"", canned.display()),
    );
    let progs = programs(&p.bin, None);

    let out = run(
        &mut p,
        "remove_background",
        json!({"file": "media/photo.png", "model": "u2net"}),
        &progs,
    )
    .unwrap();
    assert_eq!(out["file"], "media/photo-cutout.png");
    let again = run(
        &mut p,
        "remove_background",
        json!({"file": "media/photo.png"}),
        &progs,
    )
    .unwrap();
    assert_eq!(
        again["file"], "media/photo-cutout-2.png",
        "never overwrites"
    );

    assert!(
        p.ws.media()
            .unwrap()
            .iter()
            .any(|f| f.path == "media/photo-cutout.png")
    );
    let sidecar: Value = serde_json::from_str(
        &std::fs::read_to_string(p.dir.join(out["sidecar"].as_str().unwrap())).unwrap(),
    )
    .unwrap();
    assert_eq!(
        (sidecar["source"].as_str(), sidecar["model"].as_str()),
        (Some("media/photo.png"), Some("u2net"))
    );
}

#[test]
fn remove_background_checks_the_file_the_model_and_the_program() {
    let mut p = project();
    png_file(&p.dir.join("media/photo.png"), [1, 2, 3, 255]);
    std::fs::write(p.dir.join("media/note.txt"), "x").unwrap();
    let empty = tempfile::tempdir().unwrap();
    let missing = run(
        &mut p,
        "remove_background",
        json!({"file": "media/photo.png"}),
        &programs(empty.path(), None),
    )
    .unwrap_err();
    assert!(
        missing.contains("pip install") && missing.contains("OPENDESIGNER_REMBG"),
        "{missing}"
    );

    script(&p.bin, "rembg", "exit 0");
    let progs = programs(&p.bin, None);
    let mut err = |args: Value| run(&mut p, "remove_background", args, &progs).unwrap_err();
    assert!(err(json!({"file": "media/note.txt"})).contains("plain file name"));
    assert!(err(json!({"file": "../photo.png"})).contains("media/"));
    assert!(err(json!({"file": "media/absent.png"})).contains("absent.png"));
    assert!(err(json!({"file": "media/photo.png", "model": "-o"})).contains("model name"));
    assert!(err(json!({"file": "media/photo.png", "timeout_s": 5000})).contains("timeout_s"));
    assert!(
        err(json!({"file": "media/photo.png"})).contains("no picture"),
        "a program that writes nothing is an error"
    );
}

#[test]
fn transcribe_can_read_a_videos_own_sound_mix_and_a_silent_video_is_an_error() {
    if !has_ffmpeg() {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    }
    let mut p = project();
    make_speech_file(&p, "voice");
    fake_cpp(&p.bin);
    let model = p.dir.join("media/ggml.bin");
    std::fs::write(&model, b"model").unwrap();
    let progs = programs(&p.bin, model.to_str());
    let video = add_video(&mut p);

    let silent = run(&mut p, "transcribe", json!({"video": video}), &progs).unwrap_err();
    assert!(silent.contains("no sound"), "{silent}");

    let set: Vec<od_core::Operation> = serde_json::from_value(json!([{
        "op": "set_video", "id": video,
        "prop": {"name": "audio", "value": {"file": "media/voice.wav", "start_ms": 0}}
    }]))
    .unwrap();
    p.ws.session.apply(Actor::User, None, &set).unwrap();
    let out = run(
        &mut p,
        "transcribe",
        json!({"video": video, "apply": true}),
        &progs,
    )
    .unwrap();
    assert_eq!(out["word_count"], 3);
    assert!(out["captions"]["cues"].as_u64().unwrap() >= 1);
}
