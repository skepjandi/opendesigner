#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_core::{Actor, AnnotationTarget, Workspace};
use serde_json::{Value, json};

struct Fixture {
    handler: Handler,
    ws: Arc<Mutex<Workspace>>,
    session: String,
    root: String,
    _tmp: tempfile::TempDir,
}

fn init(handler: &Handler, client: &str) -> String {
    let out = handler.handle(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":client,"version":"1"}}}),
    );
    assert_eq!(out.status, 200);
    out.session_id.unwrap()
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "Store assets").unwrap();
    let root = ws.session.document().pages[0].root.id.to_string();
    let ws = Arc::new(Mutex::new(ws));
    let handler = Handler::new(ws.clone());
    let session = init(&handler, "claude-code");
    Fixture {
        handler,
        ws,
        session,
        root,
        _tmp: tmp,
    }
}

impl Fixture {
    fn rpc(&self, method: &str, params: Value) -> Value {
        let out = self.handler.handle(
            Some(&self.session),
            json!({"jsonrpc":"2.0","id":7,"method":method,"params":params}),
        );
        assert_eq!(out.status, 200, "{method}");
        out.body.unwrap()
    }

    /// Calls a tool and returns (is_error, text of the first content block).
    fn tool(&self, name: &str, args: Value) -> (bool, String) {
        let resp = self.rpc("tools/call", json!({"name": name, "arguments": args}));
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
        serde_json::from_str(&text).unwrap_or(Value::String(text))
    }

    fn create_frame(&self) -> String {
        let r = self.ok(
            "apply_ops",
            json!({"ops": [{"op":"create","parent":self.root,"node":{
                "kind":"frame","name":"iPhone 6.9","width":1320,"height":2868,"fill":"#060b1c",
                "children":[{"kind":"rect","name":"Card","x":100,"y":200,"width":1120,
                             "height":600,"fill":"#f2ca83"}]}}]}),
        );
        r["created"][0].as_str().unwrap().to_string()
    }
}

#[test]
fn initialize_negotiates_the_version_and_names_the_client() {
    let h = Handler::new(Arc::new(Mutex::new(
        Workspace::create(&tempfile::tempdir().unwrap().path().join("p.od"), "x").unwrap(),
    )));
    let out = h.handle(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"1999-01-01","clientInfo":{"name":"weird <client>/name"}}}),
    );
    let body = out.body.unwrap();
    assert_eq!(body["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(body["result"]["serverInfo"]["name"], "opendesigner");
    assert!(body["result"]["capabilities"]["tools"].is_object());
    assert_eq!(out.session_id.unwrap().len(), 32);
}

#[test]
fn requests_need_a_live_session_and_notifications_are_accepted() {
    let f = fixture();
    let no_session = f
        .handler
        .handle(None, json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}));
    assert_eq!(no_session.status, 400);
    let unknown = f.handler.handle(
        Some("deadbeef"),
        json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
    );
    assert_eq!(unknown.status, 404);
    let note = f.handler.handle(
        Some(&f.session),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    assert_eq!(note.status, 202);
    assert!(note.body.is_none());
    assert_eq!(f.handler.handle(None, json!([1, 2])).status, 400);
    assert_eq!(
        f.handler
            .handle(None, json!({"jsonrpc":"1.0","id":1,"method":"x"}))
            .status,
        400
    );
    let ping = f.rpc("ping", json!({}));
    assert_eq!(ping["result"], json!({}));
    let nope = f.rpc("resources/list", json!({}));
    assert_eq!(nope["error"]["code"], -32601);
    assert!(f.handler.end_session(&f.session));
    assert!(!f.handler.end_session(&f.session));
}

#[test]
fn tools_list_describes_every_tool_with_a_closed_schema() {
    let f = fixture();
    let list = f.rpc("tools/list", json!({}));
    let tools = list["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "get_guidance",
        "get_selection",
        "get_annotations",
        "resolve_annotation",
        "list_pages",
        "get_node",
        "get_tree",
        "find_nodes",
        "list_tokens",
        "apply_ops",
        "list_checkpoints",
        "diff_checkpoint",
        "restore_checkpoint",
        "list_export_presets",
        "render_preview",
        "export",
        "export_set",
        "export_panorama",
        "device_template",
        "list_media",
        "list_locales",
        "list_videos",
        "find_files",
        "import_file",
        "list_video_presets",
        "render_video_frame",
        "export_video",
        "validate_export",
    ] {
        assert!(names.contains(&expected), "missing {expected}");
    }
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object");
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
        assert!(t["description"].as_str().unwrap().len() > 10);
    }
}

#[test]
fn the_guidance_loop_pin_read_act_resolve_with_attribution() {
    let f = fixture();
    let frame = f.create_frame();
    {
        let mut guard = f.ws.lock().unwrap();
        let ws = &mut *guard;
        let card = ws
            .session
            .document()
            .node(&od_core::NodeId::parse(&frame).unwrap())
            .unwrap()
            .children[0]
            .id
            .clone();
        ws.annotations
            .add(
                ws.session.document(),
                AnnotationTarget::Node { id: card.clone() },
                "card too narrow",
                Actor::User,
            )
            .unwrap();
        ws.select(vec![card]).unwrap();
    }
    let g = f.ok("get_guidance", json!({}));
    assert_eq!(g["selection"][0]["node"]["name"], "Card");
    assert_eq!(g["annotations"][0]["note"], "card too narrow");

    let card_id = g["selection"][0]["node"]["id"].as_str().unwrap();
    let r = f.ok(
        "apply_ops",
        json!({"turn":"t-wide","summary":"widen the card",
               "ops":[{"op":"set","id":card_id,"prop":{"name":"width","value":1200}}]}),
    );
    assert!(r["checkpoint_before"].is_number());
    assert!(
        r["touched"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t == card_id)
    );
    f.ok(
        "resolve_annotation",
        json!({"id": 1, "note": "widened to 1200"}),
    );
    assert_eq!(
        f.ok("get_annotations", json!({})).as_array().unwrap().len(),
        0
    );
    let all = f.ok("get_annotations", json!({"include_resolved": true}));
    assert_eq!(all[0]["resolved_by"], "agent:claude-code");

    let ws = f.ws.lock().unwrap();
    let last = ws.session.log().last().unwrap();
    assert_eq!(last.actor, Actor::agent("claude-code").unwrap());
    assert_eq!(last.turn.as_deref(), Some("t-wide"));
}

#[test]
fn every_turn_gets_a_checkpoint_before_its_first_change_and_it_restores() {
    let f = fixture();
    let first = f.ok(
        "apply_ops",
        json!({"turn":"a","ops":[{"op":"create","parent":f.root,"node":{"kind":"rect","width":10,"height":10}}]}),
    );
    let second = f.ok(
        "apply_ops",
        json!({"turn":"a","ops":[{"op":"create","parent":f.root,"node":{"kind":"rect","width":20,"height":20}}]}),
    );
    assert_eq!(
        first["checkpoint_before"], second["checkpoint_before"],
        "one checkpoint per turn"
    );
    let third = f.ok(
        "apply_ops",
        json!({"ops":[{"op":"create","parent":f.root,"node":{"kind":"rect","width":30,"height":30}}]}),
    );
    assert_ne!(first["checkpoint_before"], third["checkpoint_before"]);

    let cps = f.ok("list_checkpoints", json!({}));
    assert_eq!(cps.as_array().unwrap().len(), 2);
    let cp = first["checkpoint_before"].as_u64().unwrap();
    let changed = f.ok("diff_checkpoint", json!({"checkpoint": cp}));
    assert_eq!(
        changed["changed"].as_array().unwrap().len(),
        3,
        "the three rects created in the turns since the checkpoint"
    );

    f.ok("restore_checkpoint", json!({"checkpoint": cp}));
    let pages = f.ok("list_pages", json!({}));
    assert!(pages[0]["nodes"].as_array().unwrap().is_empty());
}

#[test]
fn invalid_calls_come_back_as_tool_errors_with_a_reason_and_change_nothing() {
    let f = fixture();
    let before = f.ws.lock().unwrap().session.log().len();
    let (err, text) = f.tool(
        "apply_ops",
        json!({"ops":[{"op":"create","parent":f.root,"node":{"kind":"rect","width":10,"height":10}},
                       {"op":"delete","id":"n_999"}]}),
    );
    assert!(err);
    assert!(text.contains("n_999 does not exist"), "{text}");
    assert_eq!(f.ws.lock().unwrap().session.log().len(), before);

    let (err, text) = f.tool(
        "apply_ops",
        json!({"ops":[{"op":"restore","parent":f.root,"index":0,"node":{}}]}),
    );
    assert!(err && text.contains("invalid arguments"), "{text}");
    let (err, text) = f.tool(
        "apply_ops",
        json!({"ops":[{"op":"delete","id":"n_1"}], "extra": 1}),
    );
    assert!(err && text.contains("unknown field"), "{text}");
    let (err, text) = f.tool("nope", json!({}));
    assert!(err && text.contains("unknown tool"));
    let (err, text) = f.tool("get_node", json!({"id": "../x"}));
    assert!(err && text.contains("invalid node id"), "{text}");
    let (err, _) = f.tool(
        "apply_ops",
        json!({"ops":[{"op":"set","id":f.root,"prop":{"name":"x","value":1}}]}),
    );
    assert!(err, "page roots are immutable");
}

#[test]
fn reading_tools_return_structure_the_agent_can_use() {
    let f = fixture();
    let frame = f.create_frame();
    let node = f.ok("get_node", json!({"id": frame, "depth": 1}));
    assert_eq!(node["name"], "iPhone 6.9");
    assert_eq!(node["children"][0]["name"], "Card");
    let shallow = f.ok("get_node", json!({"id": frame, "depth": 0}));
    assert_eq!(shallow["child_count"], 1);
    let (_, tree) = f.tool("get_tree", json!({}));
    assert!(
        tree.contains("iPhone 6.9") && tree.contains("Card"),
        "{tree}"
    );
    let found = f.ok(
        "find_nodes",
        json!({"name_contains": "car", "kind": "rect"}),
    );
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["name"], "Card");
    assert_eq!(f.ok("list_tokens", json!({})), json!({}));
    let presets = f.ok("list_export_presets", json!({}));
    assert!(
        presets
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == "apple-iphone-6.9")
    );
}

#[test]
fn preview_returns_a_png_the_agent_can_look_at() {
    let f = fixture();
    let frame = f.create_frame();
    let resp = f.rpc(
        "tools/call",
        json!({"name":"render_preview","arguments":{"frame":frame,"max_side":512}}),
    );
    let content = &resp["result"]["content"];
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["mimeType"], "image/png");
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(content[0]["data"].as_str().unwrap())
        .unwrap();
    assert!(bytes.starts_with(b"\x89PNG"));
    let facts = od_store::inspect_png(&bytes).unwrap();
    assert!(facts.width.max(facts.height) <= 512);
    assert!(!facts.has_alpha);
    assert!(content[1]["text"].as_str().unwrap().contains("preview of"));
}

#[test]
fn text_that_overflows_its_box_is_reported_by_the_preview_and_the_export() {
    let f = fixture();
    let r = f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":f.root,"node":{
            "kind":"frame","name":"Slide","width":1320,"height":2868,"fill":"#ffffff",
            "children":[{"kind":"text","name":"Headline","x":100,"y":100,"width":300,"height":60,
                         "font_size":80,"text":"A headline far too long for its little box"}]}}]}),
    );
    let frame = r["created"][0].as_str().unwrap().to_string();
    let resp = f.rpc(
        "tools/call",
        json!({"name":"render_preview","arguments":{"frame":frame,"max_side":512}}),
    );
    let note = resp["result"]["content"][1]["text"].as_str().unwrap();
    assert!(
        note.contains("warnings:") && note.contains("overflows its box"),
        "{note}"
    );
    let out = f.ok("export", json!({"frame": frame, "file": "slide.png"}));
    assert!(
        out["warnings"][0]
            .as_str()
            .unwrap()
            .contains("overflows its box"),
        "{out}"
    );
}

#[test]
fn export_validates_against_the_preset_and_stays_inside_the_project() {
    let f = fixture();
    let frame = f.create_frame();
    let ok = f.ok(
        "export",
        json!({"frame": frame, "file": "shot-01.png", "preset": "apple-iphone-6.9"}),
    );
    assert_eq!(ok["path"], "exports/shot-01.png");
    assert_eq!(ok["width"], 1320);
    let dir = f.ws.lock().unwrap().dir().unwrap().to_path_buf();
    assert!(dir.join("exports/shot-01.png").exists());
    let v = f.ok(
        "validate_export",
        json!({"preset":"apple-iphone-6.9","files":["shot-01.png"]}),
    );
    assert_eq!(v["ok"], true);

    let (err, text) = f.tool(
        "export",
        json!({"frame": frame, "file": "bad.png", "preset": "google-feature-graphic"}),
    );
    assert!(err && text.contains("1024x500"), "{text}");
    assert!(!dir.join("exports/bad.png").exists());

    for evil in [
        "../escape.png",
        "/etc/x.png",
        "a/b.png",
        ".hidden.png",
        "x.txt",
        "a..b.png",
    ] {
        let (err, text) = f.tool("export", json!({"frame": frame, "file": evil}));
        assert!(err && text.contains("plain name"), "{evil}: {text}");
    }
    let (err, _) = f.tool(
        "validate_export",
        json!({"preset":"apple-iphone-6.9","files":["../od.toml"]}),
    );
    assert!(err);
    let set = f.ok(
        "validate_export",
        json!({"preset":"google-phone","files":["shot-01.png"]}),
    );
    assert_eq!(
        set["ok"], false,
        "one image fails the two-to-eight rule and the size rule"
    );
}

#[test]
fn the_change_hook_reports_what_an_agent_touched() {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "x").unwrap();
    let root = ws.session.document().pages[0].root.id.to_string();
    let seen: Arc<Mutex<Vec<(String, usize)>>> = Arc::default();
    let sink = seen.clone();
    let handler = Handler::new(Arc::new(Mutex::new(ws))).with_change_hook(Arc::new(move |e| {
        sink.lock().unwrap().push((e.actor, e.touched.len()));
    }));
    let session = init(&handler, "codex");
    handler.handle(
        Some(&session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"apply_ops","arguments":{
            "ops":[{"op":"create","parent":root,"node":{"kind":"rect","width":1,"height":1}}]}}}),
    );
    handler.handle(
        Some(&session),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"get_guidance","arguments":{}}}),
    );
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.as_slice(),
        [("agent:codex".to_string(), 1)],
        "reads do not fire the hook"
    );
}

fn wide_frame(f: &Fixture, width: u32) -> String {
    let r = f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":f.root,"node":{
            "kind":"frame","name":"Panorama","width":width,"height":2868,"fill":"#060b1c",
            "children":[{"kind":"rect","name":"Ribbon","x":0,"y":1000,"width":width,
                         "height":300,"fill":"#f2ca83"}]}}]}),
    );
    r["created"][0].as_str().unwrap().to_string()
}

#[test]
fn a_panorama_is_cut_into_validated_slides_or_nothing_is_written() {
    let f = fixture();
    let wide = wide_frame(&f, 3 * 1320);
    let dir = f.ws.lock().unwrap().dir().unwrap().to_path_buf();

    let ok = f.ok(
        "export_panorama",
        json!({"frame": wide, "slices": 3, "prefix": "shot", "preset": "apple-iphone-6.9"}),
    );
    let files: Vec<&str> = ok["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        [
            "exports/shot-01.png",
            "exports/shot-02.png",
            "exports/shot-03.png"
        ]
    );
    assert_eq!(ok["files"][0]["width"], 1320);
    for n in 1..=3 {
        assert!(dir.join(format!("exports/shot-0{n}.png")).exists());
    }
    let v = f.ok(
        "validate_export",
        json!({"preset":"apple-iphone-6.9","files":["shot-01.png","shot-02.png","shot-03.png"]}),
    );
    assert_eq!(v["ok"], true);

    // a count that does not divide the width, and slices that break the preset: nothing is written
    let (err, text) = f.tool(
        "export_panorama",
        json!({"frame": wide, "slices": 7, "prefix": "odd", "preset": "apple-iphone-6.9"}),
    );
    assert!(err && text.contains("equal slices"), "{text}");
    let (err, text) = f.tool(
        "export_panorama",
        json!({"frame": wide, "slices": 11, "prefix": "many", "preset": "apple-iphone-6.9"}),
    );
    assert!(
        err && text.contains("the set") && text.contains("many-01.png"),
        "{text}"
    );
    let (err, text) = f.tool(
        "export_panorama",
        json!({"frame": wide, "slices": 3, "prefix": "wrong", "preset": "google-feature-graphic"}),
    );
    assert!(err && text.contains("wrong-01.png"), "{text}");
    for stem in ["odd", "many", "wrong"] {
        assert!(
            !dir.join(format!("exports/{stem}-01.png")).exists(),
            "{stem}"
        );
    }
}

#[test]
fn a_set_is_validated_as_a_set_including_its_count() {
    let f = fixture();
    let a = wide_frame(&f, 1080);
    let b = wide_frame(&f, 1080);
    let dir = f.ws.lock().unwrap().dir().unwrap().to_path_buf();
    // Google wants at least two phone screenshots; these are 1080x2868 (under the 2:1 limit? no)
    let (err, text) = f.tool(
        "export_set",
        json!({"frames": [a], "prefix": "play", "preset": "google-phone"}),
    );
    assert!(
        err && text.contains("the set"),
        "one image breaks the count rule: {text}"
    );
    assert!(!dir.join("exports/play-01.png").exists());

    let ok = f.ok("export_set", json!({"frames": [a, b], "prefix": "free"}));
    assert_eq!(ok["files"].as_array().unwrap().len(), 2);
    assert!(dir.join("exports/free-02.png").exists());

    let (err, text) = f.tool("export_set", json!({"frames": [], "prefix": "x"}));
    assert!(err, "{text}");
    let (err, text) = f.tool("export_set", json!({"frames": [a], "prefix": "../x"}));
    assert!(err && text.contains("plain name"), "{text}");
}

#[test]
fn a_device_template_is_a_valid_create_payload_that_applies_as_is() {
    let f = fixture();
    let t = f.ok(
        "device_template",
        json!({"kind": "phone", "screen_width": 600}),
    );
    assert_eq!(t["node"]["kind"], "group");
    assert_eq!(t["node"]["children"][1]["name"], "Screen");
    let created = f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":f.root,"node":t["node"]}]}),
    );
    assert_eq!(
        created["created"].as_array().unwrap().len(),
        1,
        "the group is the one top-level node"
    );
    let found = f.ok("find_nodes", json!({"name_contains": "Screen"}));
    assert_eq!(
        found.as_array().unwrap().len(),
        1,
        "the screen is inside the group"
    );

    for bad in [
        json!({"kind": "watch", "screen_width": 600}),
        json!({"kind": "phone", "screen_width": 5}),
        json!({"kind": "phone", "screen_width": f64::NAN}),
        json!({"kind": "phone", "screen_width": 600, "image": "nope"}),
    ] {
        let (err, text) = f.tool("device_template", bad.clone());
        if bad["image"] == "nope" {
            // the template itself is just data; the create operation is what refuses a bad path
            let node = serde_json::from_str::<Value>(&text).unwrap();
            let (err, _) = f.tool(
                "apply_ops",
                json!({"ops": [{"op":"create","parent":f.root,"node":node["node"]}]}),
            );
            assert!(err, "a bad image path is refused when applied");
        } else {
            assert!(err, "{bad}: {text}");
        }
    }
}

#[test]
fn a_design_exports_in_each_language_without_touching_the_default() {
    let f = fixture();
    let made = f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":f.root,"node":{
            "kind":"frame","name":"Slide","width":1320,"height":2868,"fill":"#ffffff",
            "children":[{"kind":"text","name":"Headline","x":60,"y":100,"width":1200,
                         "text":"Hold","font_size":300,"fill":"#000000"}]}}]}),
    );
    let frame = made["created"][0].as_str().unwrap().to_string();
    let found = f.ok("find_nodes", json!({"kind": "text"}));
    let text = found[0]["id"].as_str().unwrap().to_string();

    let before = f.ok("list_locales", json!({}));
    assert_eq!(before["languages"].as_array().unwrap().len(), 0);
    assert_eq!(before["default_language_nodes"][0]["text"], "Hold");

    f.ok(
        "apply_ops",
        json!({"ops": [{"op": "set_localized_text", "locale": "ja", "id": text, "text": "守る"}]}),
    );
    let after = f.ok("list_locales", json!({}));
    assert_eq!(after["languages"][0]["locale"], "ja");
    assert_eq!(after["languages"][0]["translated"], 1);
    assert_eq!(
        f.ok("get_node", json!({"id": text}))["text"],
        "Hold",
        "the default text is untouched"
    );

    let dir = f.ws.lock().unwrap().dir().unwrap().to_path_buf();
    let en = f.ok("export_set", json!({"frames": [frame], "prefix": "slide"}));
    let ja = f.ok(
        "export_set",
        json!({"frames": [frame], "prefix": "slide", "locale": "ja"}),
    );
    assert_eq!(en["files"][0]["path"], "exports/slide-01.png");
    assert_eq!(ja["files"][0]["path"], "exports/slide-ja-01.png");
    let (a, b) = (
        std::fs::read(dir.join("exports/slide-01.png")).unwrap(),
        std::fs::read(dir.join("exports/slide-ja-01.png")).unwrap(),
    );
    assert_ne!(a, b, "the translated slide looks different");

    let (err, message) = f.tool(
        "export_set",
        json!({"frames": [frame], "prefix": "x", "locale": "fr"}),
    );
    assert!(
        err && message.contains("no translations") && message.contains("ja"),
        "{message}"
    );
    let (err, _) = f.tool(
        "apply_ops",
        json!({"ops": [{"op": "set_localized_text", "locale": "not a tag", "id": text, "text": "x"}]}),
    );
    assert!(err);
}

/// A video of two frames made only through tools, the way an agent would.
fn make_video(f: &Fixture) -> (String, String, String) {
    let r = f.ok(
        "apply_ops",
        json!({"ops": [
            {"op":"create","parent":f.root,"node":{"kind":"frame","name":"One","width":400,"height":800,"fill":"#ff0000"}},
            {"op":"create","parent":f.root,"node":{"kind":"frame","name":"Two","width":400,"height":800,"fill":"#0000ff"}},
            {"op":"create_video","name":"Preview","fps":10,"width":200,"height":400}
        ]}),
    );
    let created = r["created"].as_array().unwrap();
    let ids: Vec<String> = created
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let video = ids.iter().find(|i| i.starts_with("v_")).unwrap().clone();
    let frames: Vec<&String> = ids.iter().filter(|i| i.starts_with("n_")).collect();
    let (one, two) = (frames[0].clone(), frames[1].clone());
    f.ok(
        "apply_ops",
        json!({"ops": [
            {"op":"add_clip","video":video,"frame":one,"duration_ms":2000},
            {"op":"add_clip","video":video,"frame":two,"duration_ms":2000,
             "transition":{"kind":"fade","duration_ms":500},"motion":{"scale_from":1.0,"scale_to":1.5}}
        ]}),
    );
    (video, one, two)
}

#[test]
fn an_agent_builds_looks_at_and_lists_a_video_and_cannot_use_history_only_ops() {
    let f = fixture();
    let (video, one, _) = make_video(&f);
    let listed = f.ok("list_videos", json!({}));
    assert_eq!(listed[0]["id"], video);
    assert_eq!(listed[0]["duration_ms"], 3500, "the fade overlaps 500 ms");
    assert_eq!(listed[0]["frame_count"], 35);
    assert_eq!(listed[0]["clips"][0]["frame"], one);
    assert_eq!(listed[0]["clips"][0]["frame_name"], "One");
    assert_eq!(listed[0]["clips"][1]["start_ms"], 1500);
    assert_eq!(listed[0]["clips"][1]["transition_ms"], 500);
    let story = listed[0]["story"].as_array().unwrap();
    assert_eq!(story.len(), 2);
    assert!(
        story[0]
            .as_str()
            .unwrap()
            .starts_with("0.0 s starts with frame One for 2.0 s"),
        "{story:?}"
    );
    assert!(
        story[1]
            .as_str()
            .unwrap()
            .contains("1.5 s fade over 0.5 s (ease_in_out), then frame Two")
            && story[1].as_str().unwrap().contains("zooming 1.00 to 1.50"),
        "{story:?}"
    );

    let resp = f.rpc(
        "tools/call",
        json!({"name":"render_video_frame","arguments":{"video":video,"time_ms":0,"max_side":100}}),
    );
    let content = &resp["result"]["content"];
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD
        .decode(content[0]["data"].as_str().unwrap())
        .unwrap();
    let facts = od_store::inspect_png(&png).unwrap();
    assert_eq!((facts.width, facts.height), (50, 100), "shrunk to max_side");
    assert!(content[1]["text"].as_str().unwrap().contains("at 0 ms"));

    let (err, text) = f.tool(
        "apply_ops",
        json!({"ops":[{"op":"restore_clip","video":video,"index":0,"clip":{
            "id":"c_99","frame":one,"duration_ms":1000}}]}),
    );
    assert!(err, "{text}");
    let (err, text) = f.tool("apply_ops", json!({"ops":[{"op":"delete","id":one}]}));
    assert!(err && text.contains("remove that clip"), "{text}");
    let (err, text) = f.tool("export_video", json!({"video": video, "file": "../x.mp4"}));
    assert!(err && text.contains("plain name"), "{text}");
}

#[test]
fn export_video_writes_an_mp4_with_a_real_ffmpeg_and_says_what_is_missing_without_one() {
    let f = fixture();
    let (video, ..) = make_video(&f);
    let (err, text) = f.tool(
        "export_video",
        json!({"video": video, "file": "preview.mp4"}),
    );
    if od_media::Ffmpeg::locate().is_err() {
        assert!(err && text.contains("ffmpeg was not found"), "{text}");
        eprintln!("skipped the encode: no ffmpeg on this machine");
        return;
    }
    assert!(!err, "{text}");
    let out: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(out["path"], "exports/preview.mp4");
    assert_eq!(
        (out["width"].as_u64(), out["height"].as_u64()),
        (Some(200), Some(400))
    );
    assert_eq!(out["frames"], 35);
    let file = f._tmp.path().join("p.od/exports/preview.mp4");
    let data = std::fs::read(&file).unwrap();
    assert_eq!(&data[4..8], b"ftyp");
    assert_eq!(data.len() as u64, out["bytes"].as_u64().unwrap());
}

#[test]
fn a_video_that_breaks_a_store_rule_is_refused_before_anything_is_encoded() {
    let f = fixture();
    let (video, ..) = make_video(&f);
    let presets = f.ok("list_video_presets", json!({}));
    let iphone = presets
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "apple-app-preview-iphone")
        .unwrap();
    assert_eq!(iphone["sizes"], json!(["886x1920", "1920x886"]));
    assert_eq!(iphone["max_fps"], 30);
    assert!(
        iphone["source"]
            .as_str()
            .unwrap()
            .starts_with("https://developer.apple.com/")
    );

    // 200x400 and 3.5 s break the size and the minimum length; no ffmpeg is needed to know
    let (err, text) = f.tool(
        "export_video",
        json!({"video": video, "file": "p.mp4", "preset": "apple-app-preview-iphone"}),
    );
    assert!(err, "{text}");
    assert!(
        text.contains("not written") && text.contains("886x1920"),
        "{text}"
    );
    assert!(text.contains("minimum"), "{text}");
    assert!(!f._tmp.path().join("p.od/exports/p.mp4").exists());
    let (err, text) = f.tool(
        "export_video",
        json!({"video": video, "file": "p.mp4", "preset": "nope"}),
    );
    assert!(err && text.contains("unknown video preset"), "{text}");
}

/// A 3 s test video with a tone, made by the real ffmpeg.
fn make_footage(ff: &od_media::Ffmpeg, path: &std::path::Path) {
    let out = std::process::Command::new(&ff.path)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args([
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x240:rate=10:duration=3",
        ])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=3"])
        .args(["-c:v", "mpeg4", "-c:a", "aac", "-shortest"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn footage_is_listed_cut_into_a_video_with_an_overlay_and_exported_with_its_sound() {
    let Ok(ff) = od_media::Ffmpeg::locate() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    if ff.h264_encoder().is_err() || !ff.has_encoder("aac") || !ff.has_encoder("mpeg4") {
        eprintln!("skipped: {} lacks an encoder this test needs", ff.version);
        return;
    }
    let f = fixture();
    let src = f._tmp.path().join("clip.mp4");
    make_footage(&ff, &src);
    let reference = f.ws.lock().unwrap().import_footage(&src).unwrap();
    assert_eq!(reference, "media/clip.mp4");

    let listed = f.ok("list_footage", json!({}));
    assert_eq!(listed[0]["file"], "media/clip.mp4");
    assert_eq!(listed[0]["has_audio"], true);
    assert_eq!(
        (listed[0]["width"].as_u64(), listed[0]["height"].as_u64()),
        (Some(320), Some(240))
    );
    assert!((listed[0]["duration_ms"].as_u64().unwrap() as i64 - 3000).abs() < 200);

    let r = f.ok(
        "apply_ops",
        json!({"ops": [
            {"op":"create","parent":f.root,"node":{"kind":"frame","name":"Title","width":200,"height":400,
              "children":[{"kind":"rect","name":"Bar","x":0,"y":300,"width":200,"height":60,"fill":"#ff0000"}]}},
            {"op":"create_video","name":"Cut","fps":10,"width":200,"height":400}
        ]}),
    );
    let ids: Vec<String> = r["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let title = ids.iter().find(|i| i.starts_with("n_")).unwrap().clone();
    let video = ids.iter().find(|i| i.starts_with("v_")).unwrap().clone();
    f.ok(
        "apply_ops",
        json!({"ops": [
            {"op":"add_clip","video":video,"footage":{"file":"media/clip.mp4","in_ms":0},"duration_ms":1000},
            {"op":"add_clip","video":video,"footage":{"file":"media/clip.mp4","in_ms":1500},"duration_ms":1000},
            {"op":"add_overlay","video":video,"frame":title,"start_ms":500,"duration_ms":1000,"fade_ms":200}
        ]}),
    );
    let listed = f.ok("list_videos", json!({}));
    assert_eq!(listed[0]["duration_ms"], 2000);
    assert_eq!(listed[0]["clips"][1]["footage"], "media/clip.mp4");
    assert_eq!(listed[0]["overlays"][0]["frame"], title);

    // a clip past the end of its file is refused with the numbers
    f.ok("apply_ops", json!({"ops":[{"op":"add_clip","video":video,"footage":{"file":"media/clip.mp4","in_ms":2800},"duration_ms":2000}]}));
    let (err, text) = f.tool("export_video", json!({"video": video, "file": "bad.mp4"}));
    assert!(
        err && text.contains("only") && text.contains("ms long"),
        "{text}"
    );
    let last = listed[0]["clips"][1]["id"].as_str().unwrap().to_string();
    let _ = last;
    let tail = f.ok("list_videos", json!({}))[0]["clips"][2]["id"]
        .as_str()
        .unwrap()
        .to_string();
    f.ok(
        "apply_ops",
        json!({"ops":[{"op":"remove_clip","video":video,"clip":tail}]}),
    );

    let (err, text) = f.tool("export_video", json!({"video": video, "file": "cut.mp4"}));
    assert!(!err, "{text}");
    let out: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(out["frames"], 20);
    assert_eq!(out["footage_sounds_mixed"], 2);
    let file = f._tmp.path().join("p.od/exports/cut.mp4");
    let info = ff.probe_file(&file).unwrap();
    assert!(info.has_audio, "the footage's sound is in the file");
    assert_eq!((info.width, info.height), (200, 400));
    assert!(
        (info.duration_ms as i64 - 2000).abs() < 250,
        "{}",
        info.duration_ms
    );
    assert!(
        !f._tmp
            .path()
            .join("p.od/exports/.cut.mp4.picture.mp4")
            .exists()
    );
}

#[test]
fn the_agent_can_copy_a_picture_from_anywhere_into_the_project_but_only_real_media() {
    let f = fixture();
    let outside = tempfile::tempdir().unwrap();
    let png = outside.path().join("Logo Final.png");
    let bytes = od_render::encode_png(&od_render::Pixmap::new(4, 4).unwrap(), false).unwrap();
    std::fs::write(&png, &bytes).unwrap();

    let out = f.ok("import_file", json!({"path": png.display().to_string()}));
    assert_eq!(out["reference"], "media/Logo-Final.png");
    assert!(png.exists(), "the original is untouched");
    assert_eq!(
        f.ok("list_media", json!({}))[0]["path"],
        "media/Logo-Final.png"
    );

    let text = outside.path().join("notes.png");
    std::fs::write(&text, b"not a picture").unwrap();
    let (err, why) = f.tool("import_file", json!({"path": text.display().to_string()}));
    assert!(err, "{why}");
    let (err, _) = f.tool("import_file", json!({"path": "relative/x.png"}));
    assert!(err);
    let (err, _) = f.tool(
        "import_file",
        json!({"path": outside.path().display().to_string()}),
    );
    assert!(err, "a folder is not a file");

    // searching stays inside the home folder
    let (err, why) = f.tool(
        "find_files",
        json!({"folder": outside.path().display().to_string()}),
    );
    assert!(err && why.contains("home folder"), "{why}");
    let (err, _) = f.tool("find_files", json!({"kind": "sound"}));
    assert!(err);
}

#[test]
fn a_long_export_does_not_hold_the_editor_reports_progress_and_can_be_stopped() {
    let Ok(ff) = od_media::Ffmpeg::locate() else {
        eprintln!("skipped: no ffmpeg on this machine");
        return;
    };
    if ff.h264_encoder().is_err() {
        eprintln!("skipped: no H.264 encoder");
        return;
    }
    let f = fixture();
    let (video, ..) = make_video(&f);
    let seen: Arc<Mutex<Vec<od_agent::mcp::JobUpdate>>> = Arc::default();
    let jobs = Arc::new(od_agent::mcp::Jobs::default());
    let (log, to_cancel, ws_probe) = (seen.clone(), jobs.clone(), f.ws.clone());
    let locked_during = Arc::new(Mutex::new(Vec::new()));
    let probe = locked_during.clone();
    let handler = Handler::new(f.ws.clone()).with_job_hook(
        Arc::new(move |u: od_agent::mcp::JobUpdate| {
            // the editor's state must be free while the job runs
            probe.lock().unwrap().push(ws_probe.try_lock().is_ok());
            if u.state == od_agent::mcp::JobState::Running && u.done >= 5 && u.id == 2 {
                to_cancel.cancel(u.id);
            }
            log.lock().unwrap().push(u);
        }),
        jobs,
    );
    let session = init(&handler, "t");
    let call = |file: &str| {
        let out = handler.handle(
            Some(&session),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
                "name":"export_video","arguments":{"video":video,"file":file}}}),
        );
        out.body.unwrap()["result"].clone()
    };
    let ok = call("one.mp4");
    assert_eq!(ok["isError"], false, "{ok}");
    {
        let seen = seen.lock().unwrap();
        assert_eq!(
            seen.first().unwrap().state,
            od_agent::mcp::JobState::Running
        );
        assert_eq!(
            seen.last().unwrap().state,
            od_agent::mcp::JobState::Finished
        );
        assert!(
            seen.iter()
                .any(|u| u.total == 35 && u.done > 0 && u.done <= 35),
            "{seen:?}"
        );
        assert!(seen.iter().all(|u| u.label == "Exporting one.mp4"));
    }
    assert!(
        locked_during.lock().unwrap().iter().all(|free| *free),
        "the editor was held"
    );
    assert!(f._tmp.path().join("p.od/exports/one.mp4").exists());

    let stopped = call("two.mp4");
    assert_eq!(stopped["isError"], true);
    assert_eq!(
        seen.lock().unwrap().last().unwrap().state,
        od_agent::mcp::JobState::Cancelled
    );
    assert!(
        !f._tmp.path().join("p.od/exports/two.mp4").exists(),
        "a stopped export leaves nothing"
    );
}

#[test]
fn an_agent_keeps_separate_deliverables_in_separate_workspaces_and_switching_is_announced() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("opendesigner");
    let ws = Arc::new(Mutex::new(Workspace::create(&dir, "old").unwrap()));
    let announced: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = announced.clone();
    let handler = Handler::new(ws.clone()).with_change_hook(Arc::new(
        move |e: od_agent::mcp::ChangeEvent| {
            log.lock().unwrap().push(e.tool);
        },
    ));
    let session = init(&handler, "claude-code");
    let call = |name: &str, args: Value| -> (bool, Value) {
        let out = handler.handle(
            Some(&session),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":name,"arguments":args}}),
        );
        let r = out.body.unwrap()["result"].clone();
        let text = r["content"][0]["text"].as_str().unwrap_or("").to_string();
        (
            r["isError"].as_bool().unwrap(),
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    };

    let (err, listed) = call("list_workspaces", json!({}));
    assert!(!err);
    assert_eq!(listed["open"], "main");
    assert_eq!(listed["workspaces"][0]["open"], true);

    // work in the first, then start a second one for something else
    let root = ws.lock().unwrap().session.document().pages[0]
        .root
        .id
        .to_string();
    let (err, r) = call(
        "apply_ops",
        json!({"ops":[{"op":"create","parent":root,"node":{"kind":"frame","name":"Store 1","width":100,"height":100}}]}),
    );
    assert!(!err, "{r}");
    let (err, r) = call("create_workspace", json!({"name": "instagram reel"}));
    assert!(!err, "{r}");
    assert!(
        ws.lock().unwrap().session.document().pages[0]
            .root
            .children
            .is_empty(),
        "the new one is empty"
    );
    assert_eq!(
        ws.lock().unwrap().dir().unwrap(),
        dir.join("workspaces/instagram reel")
    );
    assert!(
        announced
            .lock()
            .unwrap()
            .contains(&"create_workspace".to_string())
    );

    let (_, listed) = call("list_workspaces", json!({}));
    let names: Vec<&str> = listed["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["main", "instagram reel"]);
    assert_eq!(
        listed["workspaces"][0]["frames"], 1,
        "the first keeps its frame"
    );

    let (err, _) = call("open_workspace", json!({"name": "main"}));
    assert!(!err);
    assert_eq!(
        ws.lock().unwrap().session.document().pages[0]
            .root
            .children
            .len(),
        1
    );
    for bad in [
        json!({"name": "main"}),
        json!({"name": "../x"}),
        json!({"name": "instagram reel"}),
    ] {
        let (err, _) = call("create_workspace", bad);
        assert!(err);
    }
    let (err, why) = call("open_workspace", json!({"name": "nope"}));
    assert!(err && why.as_str().unwrap().contains("there are:"));
}

#[test]
fn every_agent_is_told_the_bar_when_it_connects_and_can_read_the_playbooks() {
    let f = fixture();
    let out = f.handler.handle(
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-06-18","capabilities":{},
            "clientInfo":{"name":"some-agent","version":"1"}}}),
    );
    let instructions = out.body.unwrap()["result"]["instructions"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(instructions.contains("art director") && instructions.contains("get_playbook"));
    assert!(instructions.contains("review_frame"));

    let guidance = f.ok("get_guidance", json!({}));
    assert_eq!(guidance["brief"]["chosen_by"], "none");
    assert!(
        guidance["before_you_build"]
            .as_str()
            .unwrap()
            .contains("get_playbook")
    );

    let list = f.ok("get_playbook", json!({}));
    for topic in ["design", "style", "video", "motion", "formats"] {
        assert!(list.as_str().unwrap().contains(topic), "{topic}");
        let (err, text) = f.tool("get_playbook", json!({"topic": topic}));
        assert!(!err && text.len() > 1500, "{topic}");
    }
    assert!(f.tool("get_playbook", json!({"topic": "nope"})).0);
}

#[test]
fn review_frame_finds_what_an_art_director_would_and_the_chosen_size() {
    let f = fixture();
    let frame = f.create_frame();
    let r = f.ok("review_frame", json!({"frame": frame}));
    assert_eq!(r["size"], json!([1320.0, 2868.0]));
    assert!(r["findings"].is_array() && r["summary"].is_string());
    // add dark text on the dark frame: contrast must be flagged
    f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":frame,"node":{
            "kind":"text","name":"Headline","x":120,"y":900,"width":1000,"height":300,
            "text":"Hello","font_size":120,"fill":"#0a1024"}}]}),
    );
    let r = f.ok("review_frame", json!({"frame": frame}));
    let text = r["findings"].to_string();
    assert!(
        text.contains("contrast") && r["summary"].as_str().unwrap().starts_with("1 to fix"),
        "{text}"
    );

    f.ok(
        "set_format",
        json!({"format": "google-phone", "reason": "play listing"}),
    );
    let r = f.ok("review_frame", json!({"frame": frame}));
    assert!(
        r["findings"].to_string().contains("you set"),
        "a frame of another size is noted"
    );
    f.ok(
        "set_format",
        json!({"format": "apple-iphone-6.9", "reason": "the app store"}),
    );
    let r = f.ok("review_frame", json!({"frame": frame}));
    assert!(
        !r["findings"].to_string().contains("you set"),
        "the right size is not"
    );
    assert!(f.tool("review_frame", json!({"frame": "n_99999"})).0);
}

#[test]
fn a_format_the_user_chose_is_enforced_and_changes_only_with_their_answer() {
    let f = fixture();
    // nothing chosen: the agent picks and may change its own pick
    let r = f.ok(
        "set_format",
        json!({"format": "reel", "reason": "for a reel"}),
    );
    assert_eq!(r["applied"], true);
    let r = f.ok(
        "set_format",
        json!({"kind": "video", "width": 1080, "height": 1350, "fps": 24, "reason": "feed"}),
    );
    assert_eq!(r["applied"], true);
    assert_eq!(f.ok("get_brief", json!({}))["chosen_by"], "agent");
    assert!(
        f.tool("set_format", json!({"format": "reel"})).0,
        "a reason is required"
    );
    assert!(
        f.tool("set_format", json!({"format": "nope", "reason": "x"}))
            .0
    );
    assert!(
        f.tool(
            "set_format",
            json!({"kind": "video", "width": 1081, "height": 1920, "reason": "x"})
        )
        .0
    );

    // the user chooses
    let dir = f.ws.lock().unwrap().dir().unwrap().to_path_buf();
    let mut brief = od_agent::formats::load_brief(&dir);
    brief.user_set(od_agent::formats::find("reel").map(od_agent::formats::BriefFormat::from));
    od_agent::formats::save_brief(&dir, &brief).unwrap();

    let make = |w: u32, h: u32| {
        f.tool(
            "apply_ops",
            json!({"ops": [{"op":"create_video","name":"Teaser","fps":30,"width":w,"height":h}]}),
        )
    };
    let (err, text) = make(1920, 1080);
    assert!(
        err && text.contains("the user chose") && text.contains("set_format"),
        "{text}"
    );
    assert!(!make(1080, 1920).0);

    let r = f.ok(
        "set_format",
        json!({"format": "youtube", "reason": "the footage is landscape"}),
    );
    assert_eq!(r["applied"], false);
    assert_eq!(r["waiting_for_user"], true);
    let b = f.ok("get_brief", json!({}));
    assert_eq!(b["chosen_by"], "user");
    assert_eq!(
        b["format"]["id"], "reel",
        "unchanged until the user answers"
    );
    assert_eq!(b["pending_request"]["reason"], "the footage is landscape");

    // the user approves in the window
    let mut brief = od_agent::formats::load_brief(&dir);
    brief.answer(true).unwrap();
    od_agent::formats::save_brief(&dir, &brief).unwrap();
    let b = f.ok("get_brief", json!({}));
    assert_eq!(b["format"]["id"], "youtube");
    assert!(b["last_decision"].as_str().unwrap().contains("approved"));
    assert!(!make(1920, 1080).0);
}

#[test]
fn the_formats_list_is_in_sections() {
    let f = fixture();
    let v = f.ok("list_formats", json!({}));
    let names: Vec<&str> = v
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["section"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        ["Video", "Store images", "Social images", "Web and icons"]
    );
    assert!(
        v[0]["formats"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["id"] == "reel" && f["width"] == 1080)
    );
}

#[test]
fn an_export_says_when_the_frame_was_never_reviewed_or_changed_since() {
    let f = fixture();
    let frame = f.create_frame();
    let export = |file: &str| f.ok("export", json!({"frame": frame, "file": file}));

    let r = export("a.png");
    let note = r["before_you_call_it_done"].as_str().unwrap();
    assert!(
        note.contains("never checked with review_frame") && note.contains("iPhone 6.9"),
        "{note}"
    );

    f.ok("review_frame", json!({"frame": frame}));
    assert!(
        export("b.png").get("before_you_call_it_done").is_none(),
        "reviewed: nothing to say"
    );

    // an unrelated change elsewhere does not make the review stale; a change inside the frame does
    f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":f.root,"node":{"kind":"frame","name":"Other","width":100,"height":100}}]}),
    );
    assert!(export("c.png").get("before_you_call_it_done").is_none());
    f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":frame,"node":{"kind":"rect","name":"New","x":0,"y":0,"width":50,"height":50,"fill":"#ffffff"}}]}),
    );
    let note = export("d.png")["before_you_call_it_done"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        note.contains("changed after its last review_frame"),
        "{note}"
    );
    // sets and panoramas say it too
    let set = f.ok("export_set", json!({"frames": [frame], "prefix": "s"}));
    assert!(set["before_you_call_it_done"].is_string());
}

#[test]
fn an_export_of_a_frame_with_no_background_says_so() {
    let f = fixture();
    // a frame with a fill is fine
    let filled = f.create_frame();
    let out = f.ok("export", json!({"frame": filled, "file": "filled.png"}));
    assert!(
        !out["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w.as_str().unwrap().contains("no background")),
        "{out:?}"
    );
    // a frame without one is transparent, and the file is white there
    let bare = f.ok(
        "apply_ops",
        json!({"ops": [{"op":"create","parent":f.root,"node":{
            "kind":"frame","name":"Bare","width":600,"height":600}}]}),
    );
    let id = bare["created"][0].as_str().unwrap();
    let out = f.ok("export", json!({"frame": id, "file": "bare.png"}));
    let warnings: Vec<String> = out["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap().to_string())
        .collect();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("no background") && w.contains("white")),
        "{warnings:?}"
    );
}

#[test]
fn an_agent_keeps_notes_in_memory_and_guidance_indexes_them() {
    let f = fixture();
    let notes = f._tmp.path().join("p.od/.opendesigner/memory");

    let g = f.ok("get_guidance", json!({}));
    assert_eq!(g["memory"]["notes"], json!([]));
    assert!(
        g["memory"]["instruction"]
            .as_str()
            .unwrap()
            .contains("read_memory")
    );
    assert_eq!(f.ok("list_memory", json!({}))["notes"], json!([]));

    let w = f.ok(
        "write_memory",
        json!({"name": "brand.md", "text": "# Brand\nPrimary #112233 (from the website)"}),
    );
    assert_eq!(w["name"], "brand.md");
    assert_eq!(w["title"], "Brand");
    f.ok(
        "write_memory",
        json!({"name": "brand.md", "text": "User wants Inter"}),
    );
    let text = f.ok("read_memory", json!({"name": "brand.md"}));
    let text = text.as_str().unwrap();
    assert!(
        text.contains("#112233") && text.contains("User wants Inter"),
        "{text}"
    );
    f.ok(
        "write_memory",
        json!({"name": "lessons.md", "text": "Do not stack cards", "mode": "replace"}),
    );

    let listed = f.ok("list_memory", json!({}));
    let names: Vec<&str> = listed["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["brand.md", "lessons.md"]);

    let g = f.ok("get_guidance", json!({}));
    assert_eq!(g["memory"]["notes"].as_array().unwrap().len(), 2);
    assert!(
        !g.to_string().contains("#112233"),
        "guidance lists names, not bodies"
    );
    assert!(f._tmp.path().join("p.od/.opendesigner/README.md").is_file());
    assert!(
        !f._tmp.path().join(".opendesigner").exists(),
        "nothing outside the project"
    );

    for bad in [
        "../evil.md",
        "a/b.md",
        ".hidden.md",
        "x.png",
        "..",
        "/etc/passwd",
    ] {
        let (err, _) = f.tool("write_memory", json!({"name": bad, "text": "x"}));
        assert!(err, "{bad} must be refused");
        let (err, _) = f.tool("read_memory", json!({"name": bad}));
        assert!(err, "{bad} must be refused");
    }
    let (err, msg) = f.tool(
        "write_memory",
        json!({"name": "big.md", "text": "x".repeat(70 * 1024), "mode": "replace"}),
    );
    assert!(err && msg.contains("at most"), "{msg}");
    let (err, _) = f.tool(
        "write_memory",
        json!({"name": "a.md", "text": "x", "mode": "wipe"}),
    );
    assert!(err);
    let (err, _) = f.tool("read_memory", json!({"name": "missing.md"}));
    assert!(err);
    assert!(!f._tmp.path().join("evil.md").exists());
    assert_eq!(std::fs::read_dir(&notes).unwrap().count(), 2);

    let defs = f.rpc("tools/list", json!({}))["result"]["tools"].clone();
    let hint = |n: &str| {
        defs.as_array()
            .unwrap()
            .iter()
            .find(|d| d["name"] == n)
            .unwrap()["annotations"]["readOnlyHint"]
            .as_bool()
            .unwrap()
    };
    assert!(hint("list_memory") && hint("read_memory") && !hint("write_memory"));
}

#[test]
fn memory_tools_say_so_when_there_is_no_project_folder() {
    let ws = Workspace::new(od_core::Session::new(od_core::Document::new("x")));
    let handler = Handler::new(Arc::new(Mutex::new(ws)));
    let session = init(&handler, "claude-code");
    let call = |name: &str, args: Value| {
        let out = handler.handle(
            Some(&session),
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":name,"arguments":args}}),
        );
        out.body.unwrap()["result"].clone()
    };
    let r = call("write_memory", json!({"name": "a.md", "text": "x"}));
    assert_eq!(r["isError"], true);
    assert!(
        r["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no project folder")
    );
    let g = call("get_guidance", json!({}));
    let g: Value = serde_json::from_str(g["content"][0]["text"].as_str().unwrap()).unwrap();
    assert!(g["memory"]["unavailable"].is_string());
}

#[test]
fn an_imported_or_made_sound_becomes_the_video_track_and_the_export_has_sound() {
    let f = fixture();
    let (video, ..) = make_video(&f);
    // a wav on the "user's computer", made by the sound maker so no binary fixture is needed
    let home = tempfile::tempdir().unwrap();
    let wav = home.path().join("Theme Song.wav");
    let rendered = od_agent::synth::render(&od_agent::synth::Spec::new(
        od_agent::synth::Kind::Drone,
        3000,
    ))
    .unwrap();
    std::fs::write(&wav, od_agent::synth::wav_bytes(&rendered.channels)).unwrap();
    // an mp3 that is not one is refused, a real wav is found and copied
    let fake = home.path().join("fake.mp3");
    std::fs::write(&fake, b"just words").unwrap();
    let (err, _) = f.tool("import_file", json!({"path": fake.display().to_string()}));
    assert!(err);
    let imported = f.ok("import_file", json!({"path": wav.display().to_string()}));
    assert_eq!(imported["reference"], "media/Theme Song.wav");

    let made = f.ok(
        "make_sound",
        json!({"kind": "hit", "duration_ms": 900, "name": "boom"}),
    );
    assert_eq!(made["file"], "media/boom.wav");
    assert_eq!(made["source"], "synthesized");
    let (err, why) = f.tool("make_sound", json!({"kind": "laser"}));
    assert!(err && why.contains("kind is one of"), "{why}");

    let sounds = f.ok("list_sounds", json!({}));
    let rows = sounds.as_array().unwrap();
    let by = |file: &str| rows.iter().find(|r| r["file"] == file).unwrap();
    assert_eq!(by("media/boom.wav")["source"], "synthesized");
    assert_eq!(by("media/Theme Song.wav")["source"], "imported");
    assert!(by("media/boom.wav").get("attribution").is_none());
    if od_media::Ffmpeg::locate().is_ok() {
        let ms = by("media/boom.wav")["duration_ms"].as_u64().unwrap();
        assert!((890..=910).contains(&ms), "{ms}");
    }

    // the imported file is the video's sound track, end to end
    f.ok(
        "apply_ops",
        json!({"ops": [{"op":"set_video","id":video,"prop":{"name":"audio","value":{
            "file":"media/Theme Song.wav","start_ms":0,"gain":0.3,"fade_in_ms":200,"fade_out_ms":300}}}]}),
    );
    let Ok(ffmpeg) = od_media::Ffmpeg::locate() else {
        eprintln!("skipped the export: no ffmpeg on this machine");
        return;
    };
    if ffmpeg.h264_encoder().is_err() {
        eprintln!("skipped the export: no H.264 encoder");
        return;
    }
    let out = f.ok(
        "export_video",
        json!({"video": video, "file": "with-sound.mp4"}),
    );
    assert_eq!(out["path"], "exports/with-sound.mp4");
    let info = ffmpeg
        .probe_file(&f._tmp.path().join("p.od/exports/with-sound.mp4"))
        .unwrap();
    assert!(info.has_audio, "the export has no sound");
}

#[test]
fn get_guidance_lists_the_servers_the_user_added_by_name_and_what_they_are_for() {
    let f = fixture();
    let none = f.ok("get_guidance", json!({}));
    assert_eq!(none["extra_mcp_servers"], json!([]));
    assert!(none["sound"].as_str().unwrap().contains("make_sound"));
    f.handler
        .set_extra_servers(vec!["minimax-mcp".into(), "notes".into()]);
    let g = f.ok("get_guidance", json!({}));
    let servers = g["extra_mcp_servers"].as_array().unwrap();
    assert_eq!(servers.len(), 2);
    assert_eq!(servers[0]["name"], "minimax-mcp");
    assert!(
        servers[0]["about"]
            .as_str()
            .unwrap()
            .contains("voice-overs")
    );
    assert!(servers[1].get("about").is_none());
}

/// RGB pixels of a PNG (alpha dropped).
fn rgb_pixels(png_bytes: &[u8]) -> Vec<[u8; 3]> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(png_bytes))
        .read_info()
        .unwrap();
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut buf).unwrap();
    let channels = info.color_type.samples();
    buf[..info.buffer_size()]
        .chunks(channels)
        .map(|p| [p[0], p[1], p[2]])
        .collect()
}

#[test]
fn a_generated_background_and_a_bundled_icon_end_up_in_a_rendered_video_frame() {
    let f = fixture();
    // 1. the assets, through the same tools an agent calls
    let bg = f.ok(
        "make_asset",
        json!({"generator": "gradient", "width": 400, "height": 800,
               "colors": ["#00ff00", "#00ff00"], "name": "scene-bg"}),
    );
    assert_eq!(bg["file"], "media/scene-bg.png");
    let icon = f.ok(
        "import_asset",
        json!({"id": "lucide:camera", "width": 200, "height": 200, "color": "#ff0000",
               "name": "cam"}),
    );
    assert_eq!(icon["file"], "media/cam.png");
    let media = f.ok("list_media", json!({}));
    assert!(
        media.to_string().contains("scene-bg.png") && media.to_string().contains("cam.png"),
        "list_media shows what the asset tools made: {media}"
    );

    // 2. frames hold them as image nodes; one frame is a clip, the other an overlay
    let r = f.ok(
        "apply_ops",
        json!({"ops": [
            {"op":"create","parent":f.root,"node":{"kind":"frame","name":"Scene","width":400,"height":800,
              "children":[{"kind":"image","name":"Background","x":0,"y":0,"width":400,"height":800,
                           "image":"media/scene-bg.png","image_fit":"cover"}]}},
            {"op":"create","parent":f.root,"node":{"kind":"frame","name":"Badge","width":400,"height":800,
              "children":[{"kind":"image","name":"Camera","x":100,"y":300,"width":200,"height":200,
                           "image":"media/cam.png"}]}},
            {"op":"create_video","name":"Promo","fps":10,"width":200,"height":400}
        ]}),
    );
    let ids: Vec<String> = r["created"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    let video = ids.iter().find(|i| i.starts_with("v_")).unwrap().clone();
    let frames: Vec<&String> = ids.iter().filter(|i| i.starts_with("n_")).collect();
    let (scene, badge) = (frames[0].clone(), frames[1].clone());
    f.ok(
        "apply_ops",
        json!({"ops": [
            {"op":"add_clip","video":video,"frame":scene,"duration_ms":2000},
            {"op":"add_overlay","video":video,"frame":badge,"start_ms":0,"duration_ms":2000,
             "enter":"none","exit":"none"}
        ]}),
    );

    // 3. the picture really contains both assets
    let resp = f.rpc(
        "tools/call",
        json!({"name":"render_video_frame","arguments":{"video":video,"time_ms":1000,"max_side":400}}),
    );
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(resp["result"]["content"][0]["data"].as_str().unwrap())
        .unwrap();
    let px = rgb_pixels(&bytes);
    let green = px.iter().filter(|p| p[1] > 200 && p[0] < 60).count();
    let red = px
        .iter()
        .filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60)
        .count();
    assert!(
        green > px.len() / 2,
        "the generated background fills the frame ({green}/{})",
        px.len()
    );
    assert!(
        red > 50,
        "the recolored icon is drawn over it ({red} red pixels)"
    );
}
