#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_core::Workspace;
use serde_json::{Value, json};

struct Fixture {
    handler: Handler,
    session: String,
    root: String,
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "Store assets").unwrap();
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
        session: out.session_id.unwrap(),
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

    /// A frame with a 3D phone in it and a video that shows the frame.
    fn phone_scene(&self) -> (String, String, String) {
        let frame = self.ok(
            "apply_ops",
            json!({"ops": [{"op":"create","parent":self.root,"node":{
                "kind":"frame","name":"Hero","width":600,"height":1000,"fill":"#101014",
                "children":[{"kind":"device3d","name":"Phone","x":100,"y":50,
                             "width":400,"height":900,
                             "device3d":{"model":"phone","rotate_y":-18.0}}]}}]}),
        );
        let frame = frame["created"][0].as_str().unwrap().to_string();
        let tree = self.ok("get_node", json!({"id": self.root, "depth": 3}));
        let device = find_device(&tree);
        let video = self.ok(
            "apply_ops",
            json!({"ops": [{"op":"create_video","name":"Promo","fps":30,"width":600,"height":1000}]}),
        );
        let video = video["created"][0].as_str().unwrap().to_string();
        self.ok(
            "apply_ops",
            json!({"ops": [{"op":"add_clip","video":video,"frame":frame,"duration_ms":4000}]}),
        );
        (frame, device, video)
    }
}

/// The first device3d in a node tree.
fn find_device(node: &Value) -> String {
    fn walk(node: &Value) -> Option<String> {
        if node["kind"] == "device3d" {
            return Some(node["id"].as_str().unwrap().to_string());
        }
        node["children"].as_array()?.iter().find_map(walk)
    }
    walk(node).expect("a device in the tree")
}

#[test]
fn an_agent_makes_a_3d_phone_animates_it_and_sees_the_motion() {
    let f = fixture();
    let (_frame, device, video) = f.phone_scene();

    // the device is a real node with its model
    let node = f.ok("get_node", json!({"id": device, "depth": 0}));
    assert_eq!(node["kind"], "device3d");
    assert_eq!(node["device3d"]["model"], "phone");

    // a named move becomes keyframes
    let out = f.ok(
        "animate_preset",
        json!({"video": video, "node": device, "preset": "hero_reveal",
               "start_ms": 0, "duration_ms": 1200}),
    );
    assert_eq!(out["touched"].as_array().map(Vec::len), Some(2));

    let listed = f.ok("list_videos", json!({}));
    let animations = listed[0]["animations"].as_array().unwrap();
    assert!(!animations.is_empty(), "the move is keyframed");
    let story = listed[0]["story"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(story.contains("rotate_y"), "{story}");

    // the story says what moves, and the contact sheet shows the motion
    let sheet = f.rpc(
        "tools/call",
        json!({"name":"video_contact_sheet","arguments":{
            "video":video,"count":4,"cell_side":64}}),
    );
    let content = &sheet["result"]["content"];
    assert!(
        content[0]["data"].as_str().unwrap().len() > 100,
        "a contact sheet image"
    );
    assert!(
        content[1]["text"]
            .as_str()
            .unwrap()
            .contains("contact sheet"),
        "{}",
        content[1]["text"]
    );
}

#[test]
fn a_move_that_cannot_work_is_refused_with_a_reason() {
    let f = fixture();
    let (frame, device, video) = f.phone_scene();
    // a 3D move on a flat node
    let (err, text) = f.tool(
        "animate_preset",
        json!({"video": video, "node": frame, "preset": "spin",
               "start_ms": 0, "duration_ms": 1000}),
    );
    assert!(err, "{text}");
    assert!(text.contains("does not apply to frame nodes"), "{text}");
    // a strength beyond what the presets allow
    let (err, text) = f.tool(
        "animate_preset",
        json!({"video": video, "node": device, "preset": "spin",
               "start_ms": 0, "duration_ms": 1000, "strength": 5.0}),
    );
    assert!(err, "{text}");
    assert!(text.contains("out of bounds"), "{text}");
}

#[test]
fn motion_blur_is_set_and_told_back_in_the_video_list() {
    let f = fixture();
    let (_frame, _device, video) = f.phone_scene();
    f.ok(
        "apply_ops",
        json!({"ops": [{"op":"set_video","id":video,
                        "prop":{"name":"motion_blur","value":8}}]}),
    );
    let listed = f.ok("list_videos", json!({}));
    assert_eq!(listed[0]["motion_blur"], 8);
    let (err, text) = f.tool(
        "apply_ops",
        json!({"ops": [{"op":"set_video","id":video,
                        "prop":{"name":"motion_blur","value":1}}]}),
    );
    assert!(err, "{text}");
}
