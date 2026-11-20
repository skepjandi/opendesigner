#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_agent::showcase;
use od_core::Workspace;
use serde_json::{Value, json};

struct Fixture {
    handler: Handler,
    ws: Arc<Mutex<Workspace>>,
    session: String,
    root: String,
    events: Arc<Mutex<Vec<String>>>,
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "Showcase").unwrap();
    let root = ws.session.document().pages[0].root.id.to_string();
    let ws = Arc::new(Mutex::new(ws));
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    let handler = Handler::new(ws.clone()).with_change_hook(Arc::new(move |e| {
        seen.lock().unwrap().push(e.tool);
    }));
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
        events,
        _tmp: tmp,
    }
}

impl Fixture {
    fn tool(&self, name: &str, args: Value) -> (bool, String) {
        let out = self.handler.handle(
            Some(&self.session),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":name,"arguments":args}}),
        );
        let result = &out.body.unwrap()["result"];
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

    fn frames(&self, n: usize) -> Vec<String> {
        let ops: Vec<Value> = (0..n)
            .map(|i| {
                json!({"op":"create","parent":self.root,"node":{
                "kind":"frame","name":format!("Direction {i}"),"width":400,"height":300}})
            })
            .collect();
        let r = self.ok("apply_ops", json!({ "ops": ops }));
        r["created"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    }

    fn dir(&self) -> std::path::PathBuf {
        self.ws.lock().unwrap().dir().unwrap().to_path_buf()
    }
}

fn option(label: &str, frame: &str) -> Value {
    json!({"label": label, "kind": "frame", "target": frame, "note": "n"})
}

#[test]
fn the_showcase_tools_are_listed_and_only_get_variants_is_read_only() {
    let f = fixture();
    let out = f.handler.handle(
        Some(&f.session),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let body = out.body.unwrap();
    let tools = body["result"]["tools"].as_array().unwrap();
    let hint = |n: &str| {
        tools
            .iter()
            .find(|t| t["name"] == n)
            .unwrap_or_else(|| panic!("{n} missing"))["annotations"]["readOnlyHint"]
            .as_bool()
            .unwrap()
    };
    assert!(hint("get_variants"));
    assert!(!hint("show_preview") && !hint("offer_variants") && !hint("clear_variants"));
}

#[test]
fn an_offer_naming_a_missing_frame_is_refused_with_a_reason_and_leaves_nothing_open() {
    let f = fixture();
    let frames = f.frames(2);
    let ghost = "n_99999".to_string();
    let (err, text) = f.tool(
        "offer_variants",
        json!({"title": "T", "options": [option("A", &frames[0]), option("B", &ghost)]}),
    );
    assert!(
        err && text.contains("does not exist") && text.contains(&ghost),
        "{text}"
    );
    let (err, text) = f.tool(
        "offer_variants",
        json!({"title": "T", "options": [option("A", &frames[0])]}),
    );
    assert!(err && text.contains("between 2 and 8"), "{text}");
    assert_eq!(f.ok("get_variants", json!({}))["status"], "none");
    assert!(showcase::load(&f.dir()).variants.is_none());
}

#[test]
fn a_pick_the_user_stored_is_what_get_variants_returns() {
    let f = fixture();
    let frames = f.frames(3);
    let offered = f.ok(
        "offer_variants",
        json!({"title": "Directions", "question": "Which feels right?",
               "options": [option("Bold", &frames[0]), option("Calm", &frames[1]), option("Loud", &frames[2])]}),
    );
    assert_eq!(offered["offered"]["round"], 1);
    assert!(offered["next"].as_str().unwrap().contains("STOP"));
    let waiting = f.ok("get_variants", json!({}));
    assert_eq!(
        (waiting["status"].as_str(), waiting["picked"].is_null()),
        (Some("waiting"), true)
    );

    let dir = f.dir();
    let mut s = showcase::load(&dir);
    s.pick("v2").unwrap();
    showcase::save(&dir, &s).unwrap();
    let picked = f.ok("get_variants", json!({}));
    assert_eq!(picked["status"], "picked");
    assert_eq!(picked["picked"]["label"], "Calm");
    assert_eq!(picked["picked"]["target"], frames[1]);

    let again = f.ok(
        "offer_variants",
        json!({"title": "Round two",
        "options": [option("X", &frames[0]), option("Y", &frames[1])]}),
    );
    assert_eq!(again["offered"]["round"], 2);
    assert_eq!(
        f.ok("get_variants", json!({}))["status"],
        "waiting",
        "a new offer drops the old pick"
    );
    assert_eq!(f.ok("clear_variants", json!({}))["cleared"], true);
    assert_eq!(f.ok("get_variants", json!({}))["status"], "none");
}

#[test]
fn show_preview_is_stored_attributed_and_announced_to_the_window() {
    let f = fixture();
    let frames = f.frames(1);
    f.events.lock().unwrap().clear();
    let shown = f.ok(
        "show_preview",
        json!({"kind": "frame", "target": frames[0], "title": "Hero", "note": "final"}),
    );
    assert_eq!(shown["shown"]["seq"], 1);
    let stored = showcase::load(&f.dir()).preview.unwrap();
    assert_eq!(
        (stored.target.as_str(), stored.note.as_deref()),
        (frames[0].as_str(), Some("final"))
    );
    assert!(stored.by.contains("claude-code"), "{}", stored.by);
    assert_eq!(*f.events.lock().unwrap(), ["show_preview"]);
    let (err, text) = f.tool(
        "show_preview",
        json!({"kind": "video", "target": frames[0]}),
    );
    assert!(err && text.contains("does not exist"), "{text}");
    assert_eq!(
        f.events.lock().unwrap().len(),
        1,
        "a refused call announces nothing"
    );
}
