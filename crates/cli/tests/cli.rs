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
