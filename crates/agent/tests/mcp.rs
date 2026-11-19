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
