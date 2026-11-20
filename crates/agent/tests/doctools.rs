#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_core::Workspace;
use serde_json::{Value, json};

struct Fixture {
    handler: Handler,
    ws: Arc<Mutex<Workspace>>,
    session: String,
    root: String,
    dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let ws = Workspace::create(&dir, "Templates").unwrap();
    let root = ws.session.document().pages[0].root.id.to_string();
    let ws = Arc::new(Mutex::new(ws));
    let handler = Handler::new(ws.clone());
    let out = handler.handle(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"claude-code","version":"1"}}}),
    );
    Fixture {
        handler,
        ws,
        session: out.session_id.unwrap(),
        root,
        dir,
        _tmp: tmp,
    }
}

impl Fixture {
    fn tool(&self, name: &str, args: Value) -> (bool, String) {
        let out = self.handler.handle(
            Some(&self.session),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
                   "params":{"name": name, "arguments": args}}),
        );
        let resp = out.body.unwrap();
        let result = &resp["result"];
        (
            result["isError"].as_bool().unwrap(),
            result["content"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string(),
        )
    }

    fn ok(&self, name: &str, args: Value) -> Value {
        let (err, text) = self.tool(name, args);
        assert!(!err, "{name} failed: {text}");
        serde_json::from_str(&text).unwrap()
    }

    fn history_len(&self) -> usize {
        self.ws.lock().unwrap().session.log().len()
    }

    fn checkpoints(&self) -> usize {
        self.ws.lock().unwrap().session.checkpoints().len()
    }

    /// A 400x200 frame with one headline bound to the `title` variable; returns (frame, text).
    fn card(&self) -> (String, String) {
        let made = self.ok(
            "apply_ops",
            json!({"ops": [{"op": "create", "parent": self.root, "node": {
                "kind": "frame", "name": "Card", "width": 400, "height": 200, "fill": "#ffffff"}}]}),
        );
        let frame = made["created"][0].as_str().unwrap().to_string();
        let made = self.ok(
            "apply_ops",
            json!({"ops": [{"op": "create", "parent": frame, "node": {
                "kind": "text", "name": "Headline", "x": 20, "y": 20, "width": 360,
                "height": 80, "text": "Default", "font_size": 40, "fill": "#000000"}}]}),
        );
        let id = made["created"][0].as_str().unwrap().to_string();
        self.ok(
            "apply_ops",
            json!({"ops": [{"op": "declare_variable", "variable": {
                "id": "title", "kind": "string", "label": "Title", "default": "Default",
                "portrays": "subject_name", "binds": [{"node": id, "prop": "text"}]}}]}),
        );
        (frame, id)
    }
}

#[test]
fn the_new_tools_are_listed_with_the_right_read_only_hints() {
    let f = fixture();
    let resp = f.handler.handle(
        Some(&f.session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    );
    let body = resp.body.unwrap();
    let tools = body["result"]["tools"].as_array().unwrap();
    let hint = |name: &str| {
        tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} is not a tool"))["annotations"]["readOnlyHint"]
            .as_bool()
            .unwrap()
    };
    assert!(hint("validate_document"));
    assert!(hint("list_variables"));
    assert!(!hint("batch_export"));
    let apply = tools.iter().find(|t| t["name"] == "apply_ops").unwrap();
    assert!(apply["inputSchema"]["properties"]["dry_run"].is_object());
}

#[test]
fn dry_run_reports_what_would_change_and_leaves_history_checkpoints_and_files_alone() {
    let f = fixture();
    let ops = json!([{"op": "create", "parent": f.root,
                      "node": {"kind": "frame", "name": "A", "width": 100, "height": 100}}]);
    let before = std::fs::read_dir(f.dir.join("pages")).unwrap().count();
    let dry = f.ok(
        "apply_ops",
        json!({"ops": ops, "dry_run": true, "turn": "t1"}),
    );
    assert_eq!(dry["stage"], "dry_run");
    assert_eq!(dry["ok"], true);
    assert_eq!(f.history_len(), 0);
    assert_eq!(f.checkpoints(), 0);
    assert_eq!(
        std::fs::read_dir(f.dir.join("pages")).unwrap().count(),
        before
    );

    let real = f.ok("apply_ops", json!({"ops": ops, "turn": "t1"}));
    assert_eq!(real["stage"], "applied");
    assert_eq!(real["saved"], true);
    assert_eq!(real["touched"], dry["would_change"]);
    assert_eq!(real["created"], dry["would_create"]);
    assert_eq!(f.history_len(), 1);
    assert!(real["checkpoint_before"].is_u64());
}

#[test]
fn a_rejected_dry_run_carries_a_stable_code_and_a_real_rejection_leads_with_it() {
    let f = fixture();
    let bad = json!([{"op": "set", "id": "n_999", "prop": {"name": "x", "value": 1}}]);
    let dry = f.ok("apply_ops", json!({"ops": bad, "dry_run": true}));
    assert_eq!(dry["ok"], false);
    assert_eq!(dry["ops"][0]["error"]["code"], "E_NODE_NOT_FOUND");
    assert!(dry["ops"][0]["error"]["hint"].as_str().unwrap().len() > 10);
    assert_eq!(f.history_len(), 0);

    let (is_err, text) = f.tool("apply_ops", json!({"ops": bad}));
    assert!(is_err);
    assert!(
        text.starts_with("E_NODE_NOT_FOUND: node n_999 does not exist"),
        "{text}"
    );
}

#[test]
fn validate_document_reports_findings_with_codes_and_the_real_text_layout() {
    let f = fixture();
    let clean = f.ok("validate_document", json!({}));
    assert_eq!(clean["ok"], true);

    f.ok(
        "apply_ops",
        json!({"ops": [{"op": "create", "parent": f.root, "node": {
        "kind": "frame", "name": "F", "width": 1000, "height": 1000, "fill": "#ffffff",
        "children": [
            {"kind": "text", "name": "Squeezed", "x": 10, "y": 10, "width": 120, "height": 30,
             "text": "A headline that cannot possibly fit in one short line of text",
             "font_size": 40, "fill": "#000000"},
            {"kind": "text", "name": "Blank", "x": 10, "y": 500, "width": 100, "height": 30,
             "text": " ", "font_size": 40},
            {"kind": "image", "name": "Photo", "x": 0, "y": 600, "width": 100, "height": 100,
             "image": "media/missing.png"}
        ]}}]}),
    );
    let report = f.ok("validate_document", json!({}));
    let codes: Vec<&str> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["code"].as_str().unwrap())
        .collect();
    for expected in ["OD010", "OD011", "OD020"] {
        assert!(codes.contains(&expected), "{expected} not in {codes:?}");
    }
    assert_eq!(report["ok"], false);
    let errors_only = f.ok("validate_document", json!({"min_severity": "error"}));
    assert!(
        errors_only["findings"]
            .as_array()
            .unwrap()
            .iter()
            .all(|x| x["severity"] == "error")
    );
    assert_eq!(errors_only["warnings"], report["warnings"]);
}

#[test]
fn list_variables_shows_what_an_agent_needs_to_fill_a_template() {
    let f = fixture();
    let (_frame, text) = f.card();
    let listed = f.ok("list_variables", json!({}));
    let var = &listed["variables"][0];
    assert_eq!(var["id"], "title");
    assert_eq!(var["kind"], "string");
    assert_eq!(var["portrays"], "subject_name");
    assert_eq!(var["binds"][0]["node"], text);
}

fn png_size(path: &std::path::Path) -> (u32, u32) {
    let bytes = std::fs::read(path).unwrap();
    let facts = od_store::inspect_png(&bytes).unwrap();
    (facts.width, facts.height)
}

#[test]
fn batch_export_renders_one_png_per_row_and_never_changes_the_project() {
    let f = fixture();
    let (frame, text) = f.card();
    let history = f.history_len();
    let report = f.ok(
        "batch_export",
        json!({"frame": frame, "rows": [
            {"name": "acme", "variables": {"title": "Acme"}},
            {"name": "globex", "variables": {"title": "Globex"}},
        ]}),
    );
    assert_eq!(report["ok"], true, "{report}");
    assert_eq!(report["exported"], 2);
    assert_eq!(report["rows"][0]["files"][0], "exports/acme.png");
    assert_eq!(png_size(&f.dir.join("exports/acme.png")), (400, 200));
    let a = std::fs::read(f.dir.join("exports/acme.png")).unwrap();
    let g = std::fs::read(f.dir.join("exports/globex.png")).unwrap();
    assert_ne!(a, g, "the rows should render different text");

    assert_eq!(f.history_len(), history);
    let ws = f.ws.lock().unwrap();
    let node = ws
        .session
        .document()
        .node(&od_core::NodeId::parse(&text).unwrap())
        .unwrap();
    assert_eq!(node.text.as_deref(), Some("Default"));
    assert_eq!(
        ws.session.document().variables[0].value,
        od_core::VarValue::Text("Default".into())
    );
}

#[test]
fn batch_export_validates_each_row_against_the_preset_and_fails_only_that_row() {
    let f = fixture();
    let (frame, _) = f.card();
    let report = f.ok(
        "batch_export",
        json!({"frame": frame, "preset": "apple-iphone-6.9",
               "rows": [{"name": "acme", "variables": {"title": "Acme"}}]}),
    );
    assert_eq!(report["ok"], false);
    let error = report["rows"][0]["error"].as_str().unwrap();
    assert!(error.contains("apple-iphone-6.9"), "{error}");
    assert!(!f.dir.join("exports/acme.png").exists());
}

#[test]
fn strict_variables_fail_a_row_with_an_unknown_or_mistyped_variable() {
    let f = fixture();
    let (frame, _) = f.card();
    let rows = json!([
        {"name": "unknown", "variables": {"nope": "x"}},
        {"name": "mistyped", "variables": {"title": 12}},
        {"name": "good", "variables": {"title": "Fine"}},
    ]);
    let strict = f.ok(
        "batch_export",
        json!({"frame": frame, "rows": rows, "strict_variables": true}),
    );
    assert_eq!(strict["exported"], 1);
    assert!(
        strict["rows"][0]["error"]
            .as_str()
            .unwrap()
            .contains("no such variable")
    );
    assert!(
        strict["rows"][1]["error"]
            .as_str()
            .unwrap()
            .contains("variable title")
    );
    assert!(!f.dir.join("exports/unknown.png").exists());
    assert!(!f.dir.join("exports/mistyped.png").exists());
    assert!(f.dir.join("exports/good.png").exists());

    let lenient = f.ok("batch_export", json!({"frame": frame, "rows": rows}));
    assert_eq!(lenient["exported"], 3);
    let warning = lenient["rows"][0]["warnings"][0].as_str().unwrap();
    assert!(warning.contains("skipped"), "{warning}");
}

#[test]
fn batch_export_refuses_paths_that_leave_exports_and_odd_row_names() {
    let f = fixture();
    let (frame, _) = f.card();
    let row = json!([{"name": "acme", "variables": {}}]);
    for template in [
        "../escape/{name}.png",
        "exports/../{name}.png",
        "/tmp/{name}.png",
        "{name}.png",
        "exports/a/{name}.png",
        "exports/fixed.png",
    ] {
        let (err, text) = f.tool(
            "batch_export",
            json!({"frame": frame, "rows": [{"name": "a"}, {"name": "b"}], "out_template": template}),
        );
        assert!(err, "{template} was accepted: {text}");
    }
    for name in ["../x", "a/b", "", "a b", "x.png"] {
        let (err, text) = f.tool(
            "batch_export",
            json!({"frame": frame, "rows": [{"name": name}]}),
        );
        assert!(err, "row name {name:?} was accepted: {text}");
    }
    let (err, text) = f.tool("batch_export", json!({"rows": row}));
    assert!(err, "{text}");
    let (err, _) = f.tool(
        "batch_export",
        json!({"frame": frame, "video": "v_1", "rows": row}),
    );
    assert!(err);
    assert!(
        !f.dir.join("exports").exists()
            || std::fs::read_dir(f.dir.join("exports")).unwrap().count() == 0
    );
}

#[cfg(unix)]
#[test]
fn a_symlinked_exports_folder_is_never_written_through() {
    let f = fixture();
    let (frame, _) = f.card();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), f.dir.join("exports")).unwrap();
    let report = f.ok(
        "batch_export",
        json!({"frame": frame, "rows": [{"name": "acme", "variables": {}}]}),
    );
    assert_eq!(report["ok"], false);
    assert!(
        report["rows"][0]["error"]
            .as_str()
            .unwrap()
            .contains("symbolic link")
    );
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 0);
}

#[test]
fn batch_export_makes_one_mp4_per_row_through_the_video_export_path() {
    let Ok(ffmpeg) = od_media::Ffmpeg::locate() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    if ffmpeg.h264_encoder().is_err() {
        eprintln!("skipped: this ffmpeg has no H.264 encoder");
        return;
    }
    let f = fixture();
    let (frame, _) = f.card();
    f.ok(
        "apply_ops",
        json!({"ops": [
            {"op": "create_video", "name": "Promo", "fps": 10, "width": 400, "height": 200},
        ]}),
    );
    let video = f.ws.lock().unwrap().session.document().videos[0]
        .id
        .to_string();
    f.ok(
        "apply_ops",
        json!({"ops": [{"op": "add_clip", "video": video, "frame": frame, "duration_ms": 500}]}),
    );
    let report = f.ok(
        "batch_export",
        json!({"video": video, "rows": [
            {"name": "acme", "variables": {"title": "Acme"}},
            {"name": "globex", "variables": {"title": "Globex"}},
        ]}),
    );
    assert_eq!(report["ok"], true, "{report}");
    for name in ["acme", "globex"] {
        let info = ffmpeg
            .probe_file(&f.dir.join(format!("exports/{name}.mp4")))
            .unwrap();
        assert!(info.duration_ms >= 400, "{name}");
    }
}
