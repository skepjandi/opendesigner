#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn od(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opendesigner"))
        .args(args)
        .output()
        .unwrap()
}

fn ok(args: &[&str]) -> String {
    let out = od(args);
    assert!(
        out.status.success(),
        "{args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

/// The Phase 1 exit scenario: a project an agent edits through operations, then a validated
/// App Store PNG, then a restore.
#[test]
fn agent_edits_export_validate_and_restore_end_to_end() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("store.od");
    let dir_s = s(&dir);
    ok(&["new", dir_s, "--name", "Store assets"]);

    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let root: serde_json::Value = serde_json::from_str(&page).unwrap();
    let root_id = root["root"]["id"].as_str().unwrap().to_string();

    let ops = tmp.path().join("ops.json");
    fs::write(
        &ops,
        serde_json::json!([{
            "op": "create", "parent": root_id,
            "node": {"kind": "frame", "name": "iPhone 6.9", "width": 1320, "height": 2868,
                     "fill": "#060b1c",
                     "children": [{"kind": "rect", "name": "Card", "x": 100, "y": 200,
                                   "width": 1120, "height": 600, "corner_radius": 60,
                                   "fill": "#f2ca83"}]}
        }])
        .to_string(),
    )
    .unwrap();

    ok(&["checkpoint", dir_s, "empty"]);
    let receipt: serde_json::Value = serde_json::from_str(&ok(&[
        "apply",
        dir_s,
        s(&ops),
        "--agent",
        "claude-code",
        "--turn",
        "t1",
    ]))
    .unwrap();
    let frame_id = receipt["created"][0].as_str().unwrap().to_string();

    let tree = ok(&["show", dir_s]);
    assert!(
        tree.contains("iPhone 6.9") && tree.contains("Card"),
        "{tree}"
    );

    // a valid store export
    let png = tmp.path().join("shot.png");
    let wrote = ok(&[
        "export",
        dir_s,
        "--frame",
        &frame_id,
        "--out",
        s(&png),
        "--preset",
        "apple-iphone-6.9",
    ]);
    assert!(wrote.contains("1320x2868"), "{wrote}");
    ok(&["validate", "--preset", "apple-iphone-6.9", s(&png)]);

    // the same frame fails a preset it does not meet, and nothing is written
    let bad = tmp.path().join("bad.png");
    let out = od(&[
        "export",
        dir_s,
        "--frame",
        &frame_id,
        "--out",
        s(&bad),
        "--preset",
        "google-feature-graphic",
    ]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("1024x500"));
    assert!(!bad.exists());

    // --alpha is ignored when the preset forbids an alpha channel, so the export still validates
    let alpha = tmp.path().join("alpha.png");
    ok(&[
        "export",
        dir_s,
        "--frame",
        &frame_id,
        "--out",
        s(&alpha),
        "--preset",
        "apple-iphone-6.9",
        "--alpha",
    ]);
    ok(&["validate", "--preset", "apple-iphone-6.9", s(&alpha)]);

    // an agent turn is undone by restoring the checkpoint
    ok(&["restore", dir_s, "1"]);
    let tree = ok(&["show", dir_s]);
    assert!(!tree.contains("iPhone 6.9"), "{tree}");
    let log = fs::read_to_string(dir.join(".od/history/log.jsonl")).unwrap();
    assert!(log.contains("agent:claude-code") && log.contains("\"turn\":\"t1\""));
}

#[test]
fn bad_input_fails_with_a_reason_and_no_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    ok(&["new", s(&dir)]);
    let before = fs::read(dir.join("od.toml")).unwrap();

    let ops = tmp.path().join("ops.json");
    fs::write(&ops, r#"[{"op":"delete","id":"n_999"}]"#).unwrap();
    let out = od(&["apply", s(&dir), s(&ops)]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("n_999 does not exist"));
    assert_eq!(fs::read(dir.join("od.toml")).unwrap(), before);

    fs::write(
        &ops,
        r#"[{"op":"restore","parent":"n_2","index":0,"node":{}}]"#,
    )
    .unwrap();
    assert!(!od(&["apply", s(&dir), s(&ops)]).status.success());

    let out = od(&[
        "export",
        s(&dir),
        "--frame",
        "n_2",
        "--out",
        s(&tmp.path().join("x.png")),
    ]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("not a frame"));

    let out = od(&["validate", "--preset", "nope", "x.png"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown preset"));
    assert!(ok(&["presets"]).contains("apple-iphone-6.9"));
}

/// The studio flow without a window: a device frame from the command line, applied as an ordinary
/// operation, then a panorama cut into App Store slides, all-or-nothing.
#[test]
fn a_device_frame_and_a_panorama_work_from_the_command_line() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("studio.od");
    let dir_s = s(&dir);
    ok(&["new", dir_s, "--name", "Studio"]);
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let root: serde_json::Value = serde_json::from_str(&page).unwrap();
    let root_id = root["root"]["id"].as_str().unwrap().to_string();

    let device: serde_json::Value =
        serde_json::from_str(&ok(&["device", "phone", "--screen-width", "800"])).unwrap();
    assert_eq!(device["children"][1]["name"], "Screen");
    let bad = od(&["device", "watch", "--screen-width", "800"]);
    assert!(!bad.status.success());
    assert!(
        !od(&["device", "phone", "--screen-width", "4"])
            .status
            .success()
    );

    let mut frame = serde_json::json!({
        "kind": "frame", "name": "Wide", "width": 3 * 1320, "height": 2868,
        "fill": "#060b1c", "children": [device]
    });
    frame["children"][0]["x"] = 300.into();
    let ops = tmp.path().join("ops.json");
    fs::write(
        &ops,
        serde_json::json!([{"op": "create", "parent": root_id, "node": frame}]).to_string(),
    )
    .unwrap();
    let receipt: serde_json::Value = serde_json::from_str(&ok(&["apply", dir_s, s(&ops)])).unwrap();
    let wide = receipt["created"][0].as_str().unwrap().to_string();

    let out = ok(&[
        "export-panorama",
        dir_s,
        "--frame",
        &wide,
        "--slices",
        "3",
        "--prefix",
        "shot",
        "--preset",
        "apple-iphone-6.9",
    ]);
    assert_eq!(out.matches("wrote exports/shot-0").count(), 3, "{out}");
    let files: Vec<String> = (1..=3)
        .map(|n| s(&dir.join(format!("exports/shot-0{n}.png"))).to_string())
        .collect();
    let mut validate = vec!["validate", "--preset", "apple-iphone-6.9"];
    validate.extend(files.iter().map(String::as_str));
    ok(&validate);

    let refused = od(&[
        "export-panorama",
        dir_s,
        "--frame",
        &wide,
        "--slices",
        "5",
        "--prefix",
        "bad",
        "--preset",
        "apple-iphone-6.9",
    ]);
    assert!(!refused.status.success());
    assert!(
        !dir.join("exports/bad-01.png").exists(),
        "nothing is written on a refusal"
    );

    let set = ok(&["export-set", dir_s, "--prefix", "all"]);
    assert!(set.contains("wrote exports/all-01.png"), "{set}");
}

#[test]
fn a_translation_exports_from_the_command_line() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("l10n.od");
    let dir_s = s(&dir);
    ok(&["new", dir_s, "--name", "L10n"]);
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let root: serde_json::Value = serde_json::from_str(&page).unwrap();
    let root_id = root["root"]["id"].as_str().unwrap().to_string();
    let ops = tmp.path().join("ops.json");
    fs::write(
        &ops,
        serde_json::json!([{"op": "create", "parent": root_id, "node": {
            "kind": "frame", "name": "Slide", "width": 600, "height": 300, "fill": "#ffffff",
            "children": [{"kind": "text", "name": "Headline", "x": 20, "y": 20, "width": 560,
                          "text": "Hold", "font_size": 120, "fill": "#000000"}]}}])
        .to_string(),
    )
    .unwrap();
    let receipt: serde_json::Value = serde_json::from_str(&ok(&["apply", dir_s, s(&ops)])).unwrap();
    let frame = receipt["created"][0].as_str().unwrap().to_string();
    let text = receipt["touched"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .find(|id| *id != frame)
        .unwrap();

    assert!(ok(&["locales", dir_s]).contains("no languages"));
    fs::write(
        &ops,
        serde_json::json!([{"op": "set_localized_text", "locale": "ja", "id": text, "text": "守る"}])
            .to_string(),
    )
    .unwrap();
    ok(&["apply", dir_s, s(&ops)]);
    assert!(ok(&["locales", dir_s]).starts_with("ja\t1 translated"));

    let (en, ja) = (tmp.path().join("en.png"), tmp.path().join("ja.png"));
    ok(&["export", dir_s, "--frame", &frame, "--out", s(&en)]);
    ok(&[
        "export",
        dir_s,
        "--frame",
        &frame,
        "--out",
        s(&ja),
        "--locale",
        "ja",
    ]);
    assert_ne!(fs::read(&en).unwrap(), fs::read(&ja).unwrap());
    let set = ok(&["export-set", dir_s, "--prefix", "slide", "--locale", "ja"]);
    assert!(set.contains("exports/slide-ja-01.png"), "{set}");
    assert!(
        !od(&[
            "export",
            dir_s,
            "--frame",
            &frame,
            "--out",
            s(&en),
            "--locale",
            "fr"
        ])
        .status
        .success()
    );
}

#[test]
fn videos_are_built_with_operations_listed_and_exported_to_an_mp4() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("v.od");
    let dir_s = s(&dir);
    ok(&["new", dir_s, "--name", "Videos"]);
    assert!(ok(&["videos", dir_s]).contains("no videos yet"));

    let apply = |ops: serde_json::Value| {
        let file = tmp.path().join("ops.json");
        fs::write(&file, ops.to_string()).unwrap();
        ok(&["apply", dir_s, s(&file)])
    };
    let made = apply(serde_json::json!([
        {"op": "create", "parent": "n_2", "node": {"kind": "frame", "name": "One", "width": 200, "height": 400, "fill": "#ff0000"}},
        {"op": "create", "parent": "n_2", "node": {"kind": "frame", "name": "Two", "width": 200, "height": 400, "fill": "#0000ff"}},
        {"op": "create_video", "name": "Teaser", "fps": 10, "width": 100, "height": 200}
    ]));
    let created: serde_json::Value = serde_json::from_str(&made).unwrap();
    let ids: Vec<&str> = created["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let video = ids.iter().find(|i| i.starts_with("v_")).unwrap();
    apply(serde_json::json!([
        {"op": "add_clip", "video": video, "frame": ids[0], "duration_ms": 1000},
        {"op": "add_clip", "video": video, "frame": ids[1], "duration_ms": 1000,
         "transition": {"kind": "fade", "duration_ms": 400}}
    ]));
    let listing = ok(&["videos", dir_s]);
    assert!(
        listing.contains("Teaser\t100x200 at 10 fps\t1.6 s\t16 frames"),
        "{listing}"
    );
    assert!(
        listing.contains("starts 600 ms"),
        "the fade overlaps 400 ms: {listing}"
    );

    // Without ffmpeg the export says what to do and writes nothing.
    let out = Command::new(env!("CARGO_BIN_EXE_opendesigner"))
        .args(["export-video", dir_s, "--video", video, "--file", "t.mp4"])
        .env("OPENDESIGNER_FFMPEG", tmp.path().join("no-such-ffmpeg"))
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("did not run as ffmpeg"));
    assert!(!dir.join("exports/t.mp4").exists());

    // With one (this machine's, when it has one) the MP4 appears.
    if od_media_available() {
        let printed = ok(&["export-video", dir_s, "--video", video, "--file", "t.mp4"]);
        assert!(
            printed.starts_with("exports/t.mp4\t100x200 at 10 fps\t16 frames"),
            "{printed}"
        );
        assert_eq!(&fs::read(dir.join("exports/t.mp4")).unwrap()[4..8], b"ftyp");
    } else {
        eprintln!("skipped the encode: no ffmpeg on this machine");
    }
}

fn od_media_available() -> bool {
    od_media::Ffmpeg::locate().is_ok_and(|f| f.h264_encoder().is_ok())
}
