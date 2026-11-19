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
