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

fn root_id(dir: &Path) -> String {
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let root: serde_json::Value = serde_json::from_str(&page).unwrap();
    root["root"]["id"].as_str().unwrap().to_string()
}

fn project(tmp: &Path) -> std::path::PathBuf {
    let dir = tmp.join("p.od");
    ok(&["new", s(&dir), "--name", "Templates"]);
    dir
}

fn write_ops(tmp: &Path, dir: &Path, ops: serde_json::Value) -> std::path::PathBuf {
    let file = tmp.join("ops.json");
    fs::write(&file, ops.to_string()).unwrap();
    let _ = dir;
    file
}

#[test]
fn lint_reports_findings_and_fails_with_the_chosen_severity() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = project(tmp.path());
    let root = root_id(&dir);
    let ops = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([{
            "op": "create", "parent": root,
            "node": {"kind": "frame", "name": "F", "width": 1000, "height": 1000,
                     "children": [{"kind": "text", "name": "Empty", "x": 10, "y": 10,
                                   "width": 200, "height": 40, "text": " ", "font_size": 30}]}
        }]),
    );
    ok(&["apply", s(&dir), s(&ops)]);

    let report = ok(&["lint", s(&dir), "--json"]);
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    let codes: Vec<&str> = json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"OD011"), "{codes:?}");
    assert_eq!(json["ok"], true, "an empty text is a warning, not an error");

    let out = od(&["lint", s(&dir), "--fail-on", "warn"]);
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("OD011"), "{text}");

    let out = od(&["lint", s(&dir)]);
    assert!(out.status.success());
}

#[test]
fn apply_dry_run_reports_without_changing_the_project() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = project(tmp.path());
    let root = root_id(&dir);
    let ops = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([{
            "op": "create", "parent": root,
            "node": {"kind": "frame", "name": "F", "width": 100, "height": 100}
        }]),
    );
    let before = fs::read_to_string(dir.join("od.toml")).unwrap();
    let report = ok(&["apply", s(&dir), s(&ops), "--dry-run"]);
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(json["stage"], "dry_run");
    assert_eq!(json["ok"], true);
    assert_eq!(fs::read_to_string(dir.join("od.toml")).unwrap(), before);

    let applied = ok(&["apply", s(&dir), s(&ops)]);
    let json: serde_json::Value = serde_json::from_str(&applied).unwrap();
    assert_eq!(json["stage"], "applied");
    assert_eq!(json["touched"], json["created"]);

    let bad = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([{"op": "set", "id": "n_999", "prop": {"name": "x", "value": 1}}]),
    );
    let out = od(&["apply", s(&dir), s(&bad), "--dry-run"]);
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("E_NODE_NOT_FOUND"), "{text}");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("E_NODE_NOT_FOUND"), "{err}");
}

#[test]
fn export_batch_renders_one_png_per_row_into_exports() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = project(tmp.path());
    let root = root_id(&dir);
    let ops = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([
            {"op": "create", "parent": root, "node": {
                "kind": "frame", "name": "Card", "width": 400, "height": 200, "fill": "#ffffff"}},
        ]),
    );
    ok(&["apply", s(&dir), s(&ops)]);
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let page: serde_json::Value = serde_json::from_str(&page).unwrap();
    let frame = page["root"]["children"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let bind = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([
            {"op": "create", "parent": frame, "node": {
                "kind": "text", "name": "Headline", "x": 20, "y": 20, "width": 360, "height": 60,
                "text": "Default", "font_size": 32, "fill": "#000000"}},
        ]),
    );
    ok(&["apply", s(&dir), s(&bind)]);
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let page: serde_json::Value = serde_json::from_str(&page).unwrap();
    let text = page["root"]["children"][0]["children"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let declare = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([{"op": "declare_variable", "variable": {
            "id": "title", "kind": "string", "label": "Title", "default": "Default",
            "binds": [{"node": text, "prop": "text"}]}}]),
    );
    ok(&["apply", s(&dir), s(&declare)]);

    let rows = tmp.path().join("rows.json");
    fs::write(
        &rows,
        serde_json::json!([
            {"name": "acme", "variables": {"title": "Acme"}},
            {"name": "globex", "variables": {"title": "Globex"}}
        ])
        .to_string(),
    )
    .unwrap();
    let report = ok(&[
        "export",
        s(&dir),
        "--batch",
        s(&rows),
        "--frame",
        &frame,
        "--out-template",
        "exports/{name}.png",
    ]);
    let json: serde_json::Value = serde_json::from_str(&report).unwrap();
    assert_eq!(json["ok"], true, "{report}");
    assert_eq!(json["exported"], 2);
    assert!(dir.join("exports/acme.png").exists());
    assert!(dir.join("exports/globex.png").exists());

    // the project itself is unchanged: the headline still shows the default
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    assert!(page.contains("Default"));
}

#[test]
fn export_batch_refuses_an_output_path_outside_exports() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = project(tmp.path());
    let root = root_id(&dir);
    let ops = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([{
            "op": "create", "parent": root,
            "node": {"kind": "frame", "name": "Card", "width": 400, "height": 200}
        }]),
    );
    ok(&["apply", s(&dir), s(&ops)]);
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let page: serde_json::Value = serde_json::from_str(&page).unwrap();
    let frame = page["root"]["children"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rows = tmp.path().join("rows.json");
    fs::write(&rows, serde_json::json!([{"name": "acme"}]).to_string()).unwrap();

    for template in ["../{name}.png", "/tmp/{name}.png", "exports/../{name}.png"] {
        let out = od(&[
            "export",
            s(&dir),
            "--batch",
            s(&rows),
            "--frame",
            &frame,
            "--out-template",
            template,
        ]);
        assert!(!out.status.success(), "{template} was accepted");
        assert!(
            !tmp.path().join("escape.png").exists() && !Path::new("/tmp/acme.png").exists(),
            "{template} wrote outside the project"
        );
    }
}

#[test]
fn export_batch_strict_variables_fail_only_their_row() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = project(tmp.path());
    let root = root_id(&dir);
    let ops = write_ops(
        tmp.path(),
        &dir,
        serde_json::json!([{
            "op": "create", "parent": root,
            "node": {"kind": "frame", "name": "Card", "width": 400, "height": 200}
        }]),
    );
    ok(&["apply", s(&dir), s(&ops)]);
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    let page: serde_json::Value = serde_json::from_str(&page).unwrap();
    let frame = page["root"]["children"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let rows = tmp.path().join("rows.json");
    fs::write(
        &rows,
        serde_json::json!([
            {"name": "good", "variables": {}},
            {"name": "unknown", "variables": {"nope": "x"}}
        ])
        .to_string(),
    )
    .unwrap();
    let out = od(&[
        "export",
        s(&dir),
        "--batch",
        s(&rows),
        "--frame",
        &frame,
        "--strict-variables",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("\"exported\":1"), "{text}");
    assert!(text.contains("no such variable"), "{text}");
    assert!(dir.join("exports/good.png").exists());
    assert!(!dir.join("exports/unknown.png").exists());
}

#[test]
fn package_plugin_check_fails_on_a_stale_plugin_and_the_script_writes_it() {
    let tmp = tempfile::tempdir().unwrap();
    // a fresh folder has no plugin: the check says so
    let out = od(&["package-plugin", "--out", s(tmp.path()), "--check"]);
    assert_eq!(out.status.code(), Some(2));
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("missing:"), "{text}");

    ok(&["package-plugin", "--out", s(tmp.path())]);
    let out = od(&["package-plugin", "--out", s(tmp.path()), "--check"]);
    assert!(out.status.success(), "{:?}", out);
    assert!(
        tmp.path()
            .join("plugins/opendesigner/skills/opendesigner/SKILL.md")
            .exists()
    );
    assert!(tmp.path().join(".claude-plugin/marketplace.json").exists());
    assert!(
        tmp.path()
            .join("plugins/opendesigner/.claude-plugin/plugin.json")
            .exists()
    );
    assert!(
        tmp.path()
            .join("plugins/opendesigner/.codex-plugin/plugin.json")
            .exists()
    );
}
