#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! `import_svg_vector`: a flat illustration becomes editable nodes that draw as the SVG does, in
//! one undoable change; hostile SVGs are refused.

use std::sync::{Arc, Mutex};

use od_agent::Handler;
use od_agent::svg_vector::{self, Options};
use od_core::{
    Actor, Document, NewNode, Node, NodeId, NodeKind, Operation, Paint, Session, Workspace,
};
use resvg::{tiny_skia, usvg};
use serde_json::{Value, json};

const CHARACTER: &str = include_str!("fixtures/character.svg");

fn walk<'a>(node: &'a Node, out: &mut Vec<&'a Node>) {
    out.push(node);
    for child in &node.children {
        walk(child, out);
    }
}

fn find<'a>(group: &'a Node, name: &str) -> &'a Node {
    let mut all = Vec::new();
    walk(group, &mut all);
    all.into_iter()
        .find(|n| n.name == name)
        .unwrap_or_else(|| panic!("no node named {name}"))
}

fn session_with_frame(w: f64, h: f64) -> (Session, NodeId) {
    let mut s = Session::new(Document::new("illustration"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = w;
    frame.height = h;
    let frame = s
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: frame,
            }],
        )
        .unwrap()
        .created[0]
        .clone();
    (s, frame)
}

fn import_into(s: &mut Session, frame: &NodeId, node: NewNode) -> NodeId {
    s.apply(
        Actor::User,
        None,
        &[Operation::Create {
            parent: frame.clone(),
            index: None,
            node,
        }],
    )
    .unwrap()
    .created[0]
        .clone()
}

#[test]
fn a_character_becomes_named_shape_nodes_with_their_paint_kept() {
    let imported = svg_vector::import(CHARACTER, &Options::default()).unwrap();
    assert_eq!(imported.size, (200.0, 300.0));
    let (mut s, frame) = session_with_frame(200.0, 300.0);
    let group = import_into(&mut s, &frame, imported.node);
    let doc = s.document();
    let root = doc.node(&group).unwrap();
    assert_eq!(root.kind, NodeKind::Group);

    let background = find(root, "background");
    assert_eq!(background.kind, NodeKind::Rect, "a plain rect stays a rect");
    assert_eq!((background.width, background.height), (200.0, 300.0));
    let Some(Paint::Linear { stops, .. }) = &background.fill else {
        panic!("the sky gradient was lost: {:?}", background.fill);
    };
    assert_eq!(stops.len(), 2);
    assert_eq!(stops[0].color.to_string(), "#7ad0ff");

    let face = find(root, "face");
    assert_eq!(face.kind, NodeKind::Ellipse, "a circle becomes an ellipse");
    let head = find(root, "head");
    assert_eq!(
        (head.x + face.x, head.y + face.y, face.width, face.height),
        (52.0, 72.0, 96.0, 96.0),
        "placed exactly as drawn, relative to its group"
    );
    assert!(matches!(face.fill, Some(Paint::Radial { .. })));

    let shirt = find(root, "torso");
    assert_eq!(shirt.kind, NodeKind::Path, "a rounded rect is a path");
    assert!(matches!(shirt.fill, Some(Paint::Linear { .. })));

    let mouth = find(root, "mouth");
    let stroke = mouth.stroke.as_ref().unwrap();
    assert!(mouth.fill.is_none());
    assert_eq!(stroke.width, 5.0);
    assert_eq!(stroke.cap, od_core::LineCap::Round);
    assert_eq!(stroke.paint.to_string(), "#a03030");

    assert_eq!(
        find(root, "hat").fill_rule,
        Some(od_core::FillRule::Evenodd)
    );
    let shadow = find(root, "shadow");
    assert_eq!(shadow.fill.as_ref().unwrap().to_string(), "#00000033");
    doc.validate().unwrap();
}

#[test]
fn svg_groups_with_ids_stay_groups_and_transforms_are_flattened_into_the_shapes() {
    let imported = svg_vector::import(CHARACTER, &Options::default()).unwrap();
    let (mut s, frame) = session_with_frame(200.0, 300.0);
    let group = import_into(&mut s, &frame, imported.node);
    let root = s.document().node(&group).unwrap();
    let body = find(root, "body");
    let head = find(root, "head");
    assert_eq!(body.kind, NodeKind::Group);
    assert_eq!(head.children.len(), 5);
    let arm = find(root, "arm-left");
    assert_eq!(arm.kind, NodeKind::Group);
    let sleeve = find(arm, "sleeve-left");
    assert_eq!(
        sleeve.kind,
        NodeKind::Path,
        "a rotated rect cannot stay a rect"
    );
    assert!(
        sleeve.width > 40.0,
        "the turned sleeve is wider than it was: {sleeve:?}"
    );
    assert!(
        sleeve.x >= 0.0 && sleeve.y >= 0.0,
        "children sit inside their group's origin"
    );
    assert!(
        body.x + arm.x > 20.0 && body.y + arm.y > 150.0,
        "the group sits where its content is drawn"
    );
}

/// Draws `svg` with resvg at `width` pixels and returns RGBA bytes.
fn reference(svg: &str, width: u32) -> (u32, u32, Vec<u8>) {
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).unwrap();
    let k = width as f32 / tree.size().width();
    let h = (tree.size().height() * k).round() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(width, h).unwrap();
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(k, k),
        &mut pixmap.as_mut(),
    );
    (width, h, pixmap.data().to_vec())
}

fn drawn(svg: &str, width: f64) -> (u32, u32, Vec<u8>) {
    let imported = svg_vector::import(
        svg,
        &Options {
            width: Some(width),
            ..Options::default()
        },
    )
    .unwrap();
    let (w, h) = imported.size;
    let (mut s, frame) = session_with_frame(w, h);
    import_into(&mut s, &frame, imported.node);
    let r = od_render::render_frame(s.document(), &frame, 1.0, None).unwrap();
    (
        r.pixmap.width(),
        r.pixmap.height(),
        r.pixmap.data().to_vec(),
    )
}

/// The mean difference per channel, and how many pixels differ by more than 48.
fn compare(a: &[u8], b: &[u8]) -> (f64, f64) {
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    let far = a
        .chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p.iter().zip(*q).any(|(x, y)| x.abs_diff(*y) > 48))
        .count();
    (
        total as f64 / a.len() as f64,
        far as f64 / (a.len() / 4) as f64,
    )
}

#[test]
fn the_imported_character_draws_like_the_svg_at_two_sizes() {
    for width in [200u32, 400] {
        let (rw, rh, want) = reference(CHARACTER, width);
        let (w, h, got) = drawn(CHARACTER, f64::from(width));
        assert_eq!((w, h), (rw, rh));
        let (mean, far) = compare(&want, &got);
        assert!(mean < 2.0, "width {width}: mean difference {mean}");
        assert!(far < 0.01, "width {width}: {far} of the pixels are far off");
    }
}

#[test]
fn imports_are_deterministic() {
    let a = svg_vector::import(CHARACTER, &Options::default()).unwrap();
    let b = svg_vector::import(CHARACTER, &Options::default()).unwrap();
    assert_eq!(a.node, b.node);
}

struct Fixture {
    handler: Handler,
    ws: Arc<Mutex<Workspace>>,
    session: String,
    frame: String,
    dir: std::path::PathBuf,
    _tmp: tempfile::TempDir,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let mut ws = Workspace::create(&dir, "Scenes").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 200.0;
    frame.height = 300.0;
    let frame = ws
        .session
        .apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: frame,
            }],
        )
        .unwrap()
        .created[0]
        .to_string();
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
        frame,
        dir,
        _tmp: tmp,
    }
}

impl Fixture {
    fn tool(&self, name: &str, args: Value) -> (bool, Value) {
        let out = self.handler.handle(
            Some(&self.session),
            json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
                   "params":{"name": name, "arguments": args}}),
        );
        let result = out.body.unwrap()["result"].clone();
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (result["isError"].as_bool().unwrap(), value)
    }

    fn nodes(&self) -> usize {
        self.ws.lock().unwrap().session.document().node_count()
    }
}

#[test]
fn the_tool_imports_in_one_transaction_and_one_undo_removes_the_whole_group() {
    let f = fixture();
    let before = f.nodes();
    let (err, v) = f.tool(
        "import_svg_vector",
        json!({"parent": f.frame, "svg": CHARACTER, "name": "Kid", "x": 10, "y": 20, "summary": "add the kid"}),
    );
    assert!(!err, "{v}");
    let nodes = v["nodes"].as_u64().unwrap() as usize;
    assert!(nodes >= 15, "{nodes}");
    assert_eq!(f.nodes(), before + nodes);
    assert_eq!(v["width"], 200.0);
    let parts = v["parts"].as_array().unwrap();
    assert_eq!(parts[0]["name"], "Kid");
    assert!(
        parts
            .iter()
            .any(|p| p["name"] == "eye-left" && p["kind"] == "ellipse")
    );
    assert!(v["notes"].as_array().unwrap().is_empty(), "{v}");
    {
        let mut ws = f.ws.lock().unwrap();
        ws.session.undo(Actor::User).unwrap();
    }
    assert_eq!(f.nodes(), before, "one undo removes every part");
    {
        let mut ws = f.ws.lock().unwrap();
        ws.session.redo(Actor::User).unwrap();
    }
    assert_eq!(f.nodes(), before + nodes);
}

#[test]
fn a_media_file_is_imported_by_name_and_a_wrong_name_is_refused() {
    let f = fixture();
    std::fs::create_dir_all(f.dir.join("media")).unwrap();
    std::fs::write(f.dir.join("media/kid.svg"), CHARACTER).unwrap();
    let (err, v) = f.tool(
        "import_svg_vector",
        json!({"parent": f.frame, "file": "media/kid.svg"}),
    );
    assert!(!err, "{v}");
    for bad in ["media/../kid.svg", "media/kid.png", "kid"] {
        let (err, _) = f.tool("import_svg_vector", json!({"parent": f.frame, "file": bad}));
        assert!(err, "{bad} should be refused");
    }
    let (err, _) = f.tool("import_svg_vector", json!({"parent": f.frame}));
    assert!(err, "a source is required");
    let (err, _) = f.tool(
        "import_svg_vector",
        json!({"parent": f.frame, "svg": CHARACTER, "file": "media/kid.svg"}),
    );
    assert!(err, "only one source");
}

#[test]
fn a_bundled_icon_is_imported_as_vector_nodes_with_its_credit() {
    let f = fixture();
    let (err, v) = f.tool(
        "import_svg_vector",
        json!({"parent": f.frame, "asset": "lucide:heart", "width": 64}),
    );
    assert!(!err, "{v}");
    assert_eq!(v["credit"]["attribution_required"], true);
    assert!(
        f.dir.join("media").read_dir().unwrap().count() >= 2,
        "svg and sidecar kept"
    );
    assert!(v["nodes"].as_u64().unwrap() >= 2);
}

fn refused(svg: &str) -> String {
    svg_vector::import(svg, &Options::default())
        .err()
        .unwrap_or_else(|| panic!("should be refused: {svg:.80}"))
        .to_string()
}

const HEAD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 10 10">"#;

#[test]
fn hostile_svgs_are_refused_with_a_reason() {
    let cases = [
        (
            "external href",
            format!(r#"{HEAD}<use href="http://evil.example/a.svg#x"/></svg>"#),
            "outside",
        ),
        (
            "xlink href",
            format!(r#"{HEAD}<use xlink:href="file:///etc/passwd"/></svg>"#),
            "outside",
        ),
        (
            "picture",
            format!(r#"{HEAD}<image href="data:image/png;base64,AAAA"/></svg>"#),
            "picture",
        ),
        (
            "script",
            format!("{HEAD}<script>alert(1)</script></svg>"),
            "scripts",
        ),
        (
            "handler",
            format!(r#"{HEAD}<rect width="1" height="1" onclick="x()"/></svg>"#),
            "event handler",
        ),
        (
            "url fill",
            format!(
                r#"{HEAD}<rect width="1" height="1" fill="url(http://evil.example/p)"/></svg>"#
            ),
            "outside",
        ),
        (
            "css import",
            format!("{HEAD}<style>@import url(http://evil.example/a.css);</style></svg>"),
            "outside",
        ),
        (
            "foreign",
            format!("{HEAD}<foreignObject width=\"5\" height=\"5\"/></svg>"),
            "foreign",
        ),
        ("not xml", "not svg at all".to_string(), "not an SVG"),
    ];
    for (label, svg, needle) in cases {
        let why = refused(&svg);
        assert!(why.contains(needle), "{label}: {why}");
    }
}

#[test]
fn drawings_that_expand_into_too_much_are_refused() {
    let shapes: String = (0..svg_vector::MAX_NODES + 50)
        .map(|i| {
            format!(
                r##"<rect x="{}" y="0" width="1" height="1" fill="#000"/>"##,
                i % 9
            )
        })
        .collect();
    let why = refused(&format!("{HEAD}{shapes}</svg>"));
    assert!(why.contains("more than 500"), "{why}");

    let many: String = (0..2_100).map(|_| r#"<path d="M0 0L1 1"/>"#).collect();
    assert!(refused(&format!("{HEAD}{many}</svg>")).contains("2000"));

    let bomb = format!(
        r##"{HEAD}<defs><g id="a"><rect width="1" height="1"/></g><g id="b"><use href="#a"/><use href="#a"/></g></defs><use href="#b"/></svg>"##
    );
    assert!(refused(&bomb).contains("<use>"));

    let rects: String = (0..400)
        .map(|_| r#"<rect width="1" height="1"/>"#)
        .collect();
    let uses: String = (0..400).map(|_| r##"<use href="#a"/>"##).collect();
    let fan = format!(r#"{HEAD}<defs><g id="a">{rects}</g></defs>{uses}</svg>"#);
    assert!(
        refused(&fan).contains("2000"),
        "400 uses of 400 shapes are 160,000 shapes"
    );

    let big = " ".repeat(svg_vector::MAX_SVG_BYTES + 1);
    assert!(refused(&format!("{HEAD}{big}</svg>")).contains("larger"));
}

#[test]
fn shapes_over_the_path_limits_are_skipped_and_reported() {
    let long = format!(
        "M0 0{}",
        " l0.01 0.01".repeat(od_core::path::MAX_SEGMENTS + 10)
    );
    let svg = format!(
        r##"{HEAD}<rect width="5" height="5" fill="#f00"/><path d="{long}" fill="#00f"/></svg>"##
    );
    let imported = svg_vector::import(&svg, &Options::default()).unwrap();
    assert_eq!(imported.node.children.len(), 1);
    assert!(
        imported.notes.iter().any(|n| n.contains("path limits")),
        "{:?}",
        imported.notes
    );
}

#[test]
fn what_cannot_be_carried_is_reported_not_hidden() {
    let svg = format!(
        r##"{HEAD}<defs><clipPath id="c"><rect width="5" height="5"/></clipPath><pattern id="p" width="2" height="2" patternUnits="userSpaceOnUse"><rect width="1" height="1"/></pattern></defs><g clip-path="url(#c)"><rect width="9" height="9" fill="#00f"/></g><rect width="4" height="4" fill="url(#p)" stroke="#000"/><text x="1" y="5">Hi</text></svg>"##
    );
    let imported = svg_vector::import(&svg, &Options::default()).unwrap();
    let notes = imported.notes.join(" | ");
    for needle in ["clip path", "pattern", "<text>"] {
        assert!(notes.contains(needle), "{needle} in {notes}");
    }
}

#[test]
fn an_svg_with_nothing_to_draw_is_refused() {
    assert!(refused(&format!("{HEAD}<text x=\"1\" y=\"5\">Hi</text></svg>")).contains("no shapes"));
}
