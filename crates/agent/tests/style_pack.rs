#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_core::{Actor, Workspace};
use serde_json::{Value, json};

struct Fixture {
    handler: Handler,
    ws: Arc<Mutex<Workspace>>,
    session: String,
    root: String,
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let ws = Workspace::create(&dir, "Packs").unwrap();
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

    fn tokens(&self) -> Value {
        self.ok("list_tokens", json!({}))
    }

    /// A frame to build in.
    fn frame(&self) -> String {
        let made = self.ok(
            "apply_ops",
            json!({"ops": [{"op": "create", "parent": self.root, "node": {
                "kind": "frame", "name": "Scene", "width": 1080, "height": 1080,
                "fill": "{bg}"}}]}),
        );
        made["created"][0].as_str().unwrap().to_string()
    }
}

#[test]
fn style_pack_list_and_get_say_what_the_agent_needs() {
    let f = fixture();
    let packs = f.ok("list_style_packs", json!({}));
    let packs = packs.as_array().unwrap();
    assert_eq!(packs.len(), 9);
    for pack in packs {
        assert!(pack["id"].is_string(), "{pack}");
        assert!(pack["name"].is_string(), "{pack}");
        assert!(pack["mood"].is_string(), "{pack}");
        assert!(pack["motion"].is_string(), "{pack}");
        let swatches = pack["swatches"].as_object().unwrap();
        for role in ["bg", "surface", "fg", "muted", "border", "accent"] {
            let value = swatches[role].as_str().unwrap();
            assert!(value.starts_with('#'), "{role} of {pack}");
        }
    }
    let ids: Vec<&str> = packs.iter().map(|p| p["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"deep-space-flat"), "{ids:?}");
    assert!(ids.contains(&"mono-terminal"), "{ids:?}");

    let one = f.ok("get_style_pack", json!({"id": "swiss-kinetic"}));
    assert_eq!(one["id"], "swiss-kinetic");
    assert_eq!(one["type"]["display"], "Helvetica Neue");
    assert_eq!(one["motion"]["personality"], "snappy");
    assert!(one["rules"].as_array().unwrap().len() >= 4, "{one}");
    assert!(one["anti_patterns"].as_array().unwrap().len() >= 4, "{one}");
    let guidance = one["guidance"].as_str().unwrap();
    assert!(guidance.contains("Swiss kinetic"), "{guidance}");
    assert!(guidance.contains("Rules:"), "{guidance}");

    let (err, text) = f.tool("get_style_pack", json!({"id": "no-such-pack"}));
    assert!(err, "{text}");
    assert!(text.contains("deep-space-flat"), "{text}");
}

#[test]
fn style_pack_apply_is_one_undo_step_and_declares_the_tokens() {
    let f = fixture();
    let applied = f.ok("apply_style_pack", json!({"id": "deep-space-flat"}));
    assert_eq!(applied["stage"], "applied");
    assert_eq!(applied["pack"], "deep-space-flat");
    assert_eq!(applied["motion"], "cinematic");
    assert!(applied["rules"].as_array().unwrap().len() >= 4, "{applied}");
    // one transaction, one checkpoint: undoing it takes the whole pack away
    assert_eq!(f.history_len(), 1);
    assert_eq!(f.checkpoints(), 1);

    let tokens = f.tokens();
    assert_eq!(tokens["bg"], "#0d1226");
    assert_eq!(tokens["surface"], "#161d3a");
    assert_eq!(tokens["fg"], "#eef2ff");
    assert_eq!(tokens["muted"], "#8fa0c8");
    assert_eq!(tokens["border"], "#26325c");
    assert_eq!(tokens["accent"], "#ffb454");
    assert!(tokens["accent_2"].is_string(), "{tokens}");
    assert!(tokens["shadow"].is_string(), "{tokens}");
    assert!(tokens["palette_locked"].is_string(), "{tokens}");

    let mut ws = f.ws.lock().unwrap();
    ws.session.undo(Actor::Agent("claude-code".into())).unwrap();
    let after: Vec<String> = ws.session.document().tokens.keys().cloned().collect();
    assert!(after.is_empty(), "{after:?}");
}

#[test]
fn style_pack_records_itself_in_the_brief() {
    let f = fixture();
    f.ok("apply_style_pack", json!({"id": "mono-terminal"}));
    let brief = f.ok("get_brief", json!({}));
    assert_eq!(brief["style_pack"]["id"], "mono-terminal");
    assert_eq!(brief["style_pack"]["name"], "Mono terminal");
    assert_eq!(brief["style_pack"]["motion"], "snappy");
    let guidance = f.ok("get_guidance", json!({}));
    assert_eq!(guidance["brief"]["style_pack"]["id"], "mono-terminal");
}

#[test]
fn style_pack_overrides_change_one_colour_and_keep_the_rest() {
    let f = fixture();
    let applied = f.ok(
        "apply_style_pack",
        json!({"id": "swiss-kinetic", "overrides": {"accent": "#ff8800"}}),
    );
    assert_eq!(applied["tokens"]["accent"], "#ff8800");
    let tokens = f.tokens();
    assert_eq!(tokens["accent"], "#ff8800");
    assert_eq!(tokens["bg"], "#e4e7ea");
    let (err, text) = f.tool(
        "apply_style_pack",
        json!({"id": "swiss-kinetic", "overrides": {"nonsense": "#ff8800"}}),
    );
    assert!(err, "{text}");
    let (err, text) = f.tool(
        "apply_style_pack",
        json!({"id": "swiss-kinetic", "overrides": {"accent": "not-a-colour"}}),
    );
    assert!(err, "{text}");
}

#[test]
fn style_pack_block_colours_follow_the_pack() {
    let f = fixture();
    f.ok("apply_style_pack", json!({"id": "deep-space-flat"}));
    let frame = f.frame();
    f.ok(
        "add_block",
        json!({"block": "title_card", "frame": frame, "params": {
            "eyebrow": "Episode one", "title": "How it works", "subtitle": "In ninety seconds"}}),
    );
    let ws = f.ws.lock().unwrap();
    let doc = ws.session.document();
    let mut used = Vec::new();
    for page in &doc.pages {
        token_fills(&page.root, &mut used);
    }
    assert!(
        used.contains(&"fg".to_string()),
        "the title should be painted with the pack's fg: {used:?}"
    );
    assert!(
        used.contains(&"accent".to_string()),
        "the eyebrow should be painted with the pack's accent: {used:?}"
    );
    assert_eq!(doc.tokens["accent"], "#ffb454");
}

/// Every token a node and its children are painted with.
fn token_fills(node: &od_core::Node, out: &mut Vec<String>) {
    if let Some(paint) = &node.fill
        && let od_core::Paint::Token(name) = paint
    {
        out.push(name.clone());
    }
    for child in &node.children {
        token_fills(child, out);
    }
}

#[test]
fn style_pack_locks_the_palette_so_the_linter_flags_a_stray_colour() {
    let f = fixture();
    f.ok("apply_style_pack", json!({"id": "deep-space-flat"}));
    let frame = f.frame();
    f.ok(
        "apply_ops",
        json!({"ops": [{"op": "create", "parent": frame, "node": {
            "kind": "rect", "name": "Stray", "x": 10, "y": 10, "width": 100,
            "height": 100, "fill": "#ff00ff"}}]}),
    );
    let report = f.ok("validate_document", json!({}));
    let codes: Vec<&str> = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap())
        .collect();
    assert!(codes.contains(&"OD092"), "{report}");
    let finding = report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == "OD092")
        .unwrap();
    assert_eq!(finding["severity"], "warn");
    assert!(finding["message"].as_str().unwrap().contains("#ff00ff"));
}
