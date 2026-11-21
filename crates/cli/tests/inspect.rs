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

/// A project with a 640 x 360 video of one scene: a red square that slides.
fn project(tmp: &Path) -> (std::path::PathBuf, String, String) {
    let dir = tmp.join("p.od");
    ok(&["new", s(&dir), "--name", "Inspect"]);
    let page: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("pages/p_1.json")).unwrap()).unwrap();
    let root = page["root"]["id"].as_str().unwrap().to_string();
    let apply = |name: &str, ops: serde_json::Value| -> serde_json::Value {
        let file = tmp.join(name);
        fs::write(&file, ops.to_string()).unwrap();
        serde_json::from_str(ok(&["apply", s(&dir), s(&file)]).trim()).unwrap()
    };
    let made = apply(
        "one.json",
        serde_json::json!([
            {"op": "create", "parent": root, "node": {"kind": "frame", "name": "scene",
                "width": 640, "height": 360, "fill": "#000000"}},
            {"op": "create_video", "name": "V", "fps": 10, "width": 640, "height": 360}
        ]),
    );
    let ids: Vec<String> = made["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let square = apply(
        "two.json",
        serde_json::json!([{"op": "create", "parent": ids[0], "node": {"kind": "rect",
            "name": "Square", "x": 100, "y": 100, "width": 60, "height": 60, "fill": "#ff0000"}}]),
    )["created"][0]
        .as_str()
        .unwrap()
        .to_string();
    apply(
        "three.json",
        serde_json::json!([
            {"op": "add_clip", "video": ids[1], "frame": ids[0], "duration_ms": 2000},
            {"op": "animate", "video": ids[1], "node": square, "prop": "x",
             "keys": [{"at_ms": 0, "value": 100.0, "ease": "linear"},
                      {"at_ms": 1000, "value": 300.0, "ease": "linear"}]}
        ]),
    );
    (dir, ids[1].clone(), square)
}

fn png_size(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).unwrap();
    assert_eq!(&bytes[1..4], b"PNG");
    (
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    )
}

#[test]
fn inspect_writes_each_view_into_exports() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, video, square) = project(tmp.path());
    let said = ok(&[
        "inspect",
        s(&dir),
        "strip",
        "--video",
        &video,
        "--from-ms",
        "0",
        "--to-ms",
        "300",
        "--cell-side",
        "100",
        "--out",
        "s.png",
    ]);
    assert!(
        said.contains("exports/s.png") && said.contains("4 frames"),
        "{said}"
    );
    assert_eq!(
        png_size(&dir.join("exports/s.png")),
        (4 * 100 + 5 * 6, 56 + 12)
    );

    ok(&[
        "inspect",
        s(&dir),
        "crop",
        "--video",
        &video,
        "--time-ms",
        "500",
        "--node",
        &square,
        "--padding",
        "0",
        "--out",
        "c.png",
    ]);
    assert_eq!(png_size(&dir.join("exports/c.png")), (60, 60));

    ok(&[
        "inspect",
        s(&dir),
        "grid",
        "--video",
        &video,
        "--time-ms",
        "0",
        "--out",
        "g.png",
    ]);
    assert_eq!(png_size(&dir.join("exports/g.png")), (640, 360));

    let said = ok(&[
        "inspect",
        s(&dir),
        "diff",
        "--video",
        &video,
        "--time-ms",
        "0",
        "--other-time-ms",
        "1000",
        "--overlay",
        "--max-side",
        "640",
        "--out",
        "d.png",
    ]);
    assert!(said.contains("pixels changed"), "{said}");
    assert_eq!(png_size(&dir.join("exports/d.png")), (640, 360));
}

#[test]
fn inspect_refuses_bad_names_and_bad_requests() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, video, _) = project(tmp.path());
    let bad_name = od(&[
        "inspect",
        s(&dir),
        "grid",
        "--video",
        &video,
        "--time-ms",
        "0",
        "--out",
        "../x.png",
    ]);
    assert!(!bad_name.status.success());
    assert!(!dir.join("x.png").exists() && !tmp.path().join("x.png").exists());
    let past = od(&[
        "inspect",
        s(&dir),
        "grid",
        "--video",
        &video,
        "--time-ms",
        "99999",
    ]);
    assert!(!past.status.success());
    assert!(String::from_utf8_lossy(&past.stderr).contains("past the end"));
}

#[test]
fn review_video_takes_a_tempo() {
    let tmp = tempfile::tempdir().unwrap();
    let (dir, _, _) = project(tmp.path());
    assert!(ok(&["review-video", s(&dir), "--bpm", "120"]).contains("Score"));
    assert!(
        !od(&["review-video", s(&dir), "--bpm", "5"])
            .status
            .success()
    );
}
