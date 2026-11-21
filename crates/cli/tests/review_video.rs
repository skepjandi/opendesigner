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

fn apply(dir: &Path, name: &str, ops: serde_json::Value) -> serde_json::Value {
    let file = dir.parent().unwrap().join(name);
    fs::write(&file, ops.to_string()).unwrap();
    serde_json::from_str(ok(&["apply", s(dir), s(&file)]).trim()).unwrap()
}

#[test]
fn review_video_says_so_when_there_is_no_video_and_scores_a_picture_with_text_low() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    ok(&["new", s(&dir), "--name", "Reviews"]);

    let none = ok(&["review-video", s(&dir)]);
    assert!(none.contains("no video"), "{none}");

    let page: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("pages/p_1.json")).unwrap()).unwrap();
    let root = page["root"]["id"].as_str().unwrap();
    let made = apply(
        &dir,
        "one.json",
        serde_json::json!([
            {"op": "create", "parent": root, "node": {"kind": "frame", "name": "scene",
                "width": 640, "height": 360, "fill": "#101830"}},
            {"op": "create_video", "name": "V", "fps": 30, "width": 640, "height": 360}
        ]),
    );
    let ids: Vec<String> = made["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    apply(
        &dir,
        "two.json",
        serde_json::json!([
            {"op": "create", "parent": ids[0], "node": {"kind": "image", "name": "Backdrop",
                "x": 0, "y": 0, "width": 640, "height": 360, "image": "media/x.jpg"}},
            {"op": "add_clip", "video": ids[1], "frame": ids[0], "duration_ms": 4000}
        ]),
    );

    let text = ok(&["review-video", s(&dir)]);
    assert!(
        text.contains("RV001") && text.contains("not ready"),
        "{text}"
    );
    let json: serde_json::Value =
        serde_json::from_str(ok(&["review-video", s(&dir), "--json"]).trim()).unwrap();
    assert!(json["score"].as_u64().unwrap() < 50);
    assert_eq!(json["top_fixes"].as_array().unwrap().len(), 3);
    assert!(
        !od(&["review-video", s(&dir), "--video", "v_999"])
            .status
            .success()
    );
}
