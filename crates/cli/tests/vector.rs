#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><circle id="sun" cx="50" cy="50" r="40" fill="#ffcc00"/><path id="ray" d="M10 90 L90 90" stroke="#cc3300" stroke-width="4" fill="none"/></svg>"##;

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

#[test]
fn import_svg_vector_adds_a_named_group_and_refuses_a_hostile_svg() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    ok(&["new", s(&dir), "--name", "Scenes"]);
    let page: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join("pages/p_1.json")).unwrap()).unwrap();
    let root = page["root"]["id"].as_str().unwrap().to_string();
    let svg = tmp.path().join("sun.svg");
    fs::write(&svg, SVG).unwrap();

    let out = ok(&[
        "import-svg-vector",
        s(&dir),
        "--parent",
        &root,
        "--svg",
        s(&svg),
        "--name",
        "Sun",
        "--width",
        "200",
        "--agent",
        "claude",
    ]);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["nodes"], 3);
    assert_eq!(v["width"], 200.0);
    assert_eq!(v["parts"][0]["name"], "Sun");
    assert_eq!(v["parts"][1]["kind"], "ellipse");
    let shown = ok(&["show", s(&dir)]);
    assert!(shown.contains("sun"), "{shown}");

    let hostile = tmp.path().join("bad.svg");
    fs::write(
        &hostile,
        r#"<svg xmlns="http://www.w3.org/2000/svg"><script>1</script></svg>"#,
    )
    .unwrap();
    let refused = od(&[
        "import-svg-vector",
        s(&dir),
        "--parent",
        &root,
        "--svg",
        s(&hostile),
    ]);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("scripts"));
}
