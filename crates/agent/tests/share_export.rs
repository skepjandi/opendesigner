//! `export_share` writes a JPEG, a PDF and a self-contained HTML page of real frames.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_agent::tools::{self, CallCtx, Content, Turns};
use od_core::{Actor, NewNode, NodeId, NodeKind, Operation, Paint, Workspace};
use serde_json::{Value, json};

fn project() -> (tempfile::TempDir, Workspace, Vec<NodeId>) {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(&dir.path().join("p.od"), "Share").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let mut ids = Vec::new();
    for (name, color, words) in [
        ("one", "#336699", "Hello <world>"),
        ("two", "#993366", "Second"),
    ] {
        let mut frame = NewNode::new(NodeKind::Frame);
        frame.name = Some(name.into());
        frame.width = 80.0;
        frame.height = 60.0;
        frame.fill = Paint::parse(color).ok();
        let mut text = NewNode::new(NodeKind::Text);
        text.text = Some(words.into());
        text.width = 70.0;
        text.height = 20.0;
        frame.children.push(text);
        let created = ws
            .session
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: root.clone(),
                    index: None,
                    node: frame,
                }],
            )
            .unwrap()
            .created[0]
            .clone();
        ids.push(created);
    }
    (dir, ws, ids)
}

fn share(ws: &mut Workspace, args: Value) -> Result<Value, String> {
    let mut turns = Turns::default();
    let mut ctx = CallCtx {
        ws,
        actor: Actor::User,
        turns: &mut turns,
        extra_servers: &[],
    };
    let out = tools::call(&mut ctx, "export_share", args).map_err(|e| e.0)?;
    let Content::Text(text) = &out.content[0] else {
        panic!("text expected")
    };
    Ok(serde_json::from_str(text).unwrap())
}

#[test]
fn a_jpeg_a_pdf_and_an_html_page_are_written_to_exports() {
    let (dir, mut ws, ids) = project();
    let frames: Vec<String> = ids.iter().map(ToString::to_string).collect();
    let exports = dir.path().join("p.od/exports");

    let jpg = share(&mut ws, json!({"frames": [frames[0]], "file": "one.jpg"})).unwrap();
    assert_eq!(jpg["pages"], 1);
    let bytes = std::fs::read(exports.join("one.jpg")).unwrap();
    assert_eq!(
        image::load_from_memory(&bytes)
            .unwrap()
            .to_rgb8()
            .dimensions(),
        (80, 60)
    );

    share(&mut ws, json!({"frames": frames, "file": "deck.pdf"})).unwrap();
    let pdf = std::fs::read(exports.join("deck.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-1.4"));
    assert_eq!(
        String::from_utf8_lossy(&pdf)
            .matches("/Type /Page ")
            .count(),
        2
    );

    share(
        &mut ws,
        json!({"frames": frames, "file": "page.html", "title": "My <deck>"}),
    )
    .unwrap();
    let html = std::fs::read_to_string(exports.join("page.html")).unwrap();
    assert!(html.contains("<title>My &lt;deck&gt;</title>"));
    assert_eq!(html.matches("data:image/png;base64,").count(), 2);
    assert!(html.contains("Hello &lt;world&gt; / ") || html.contains("alt=\"Hello &lt;world&gt;"));
    assert!(!html.contains("<script"));
}

#[test]
fn bad_requests_write_nothing() {
    let (dir, mut ws, ids) = project();
    let frames: Vec<String> = ids.iter().map(ToString::to_string).collect();
    for (args, why) in [
        (
            json!({"frames": frames, "file": "two.jpg"}),
            "a JPEG holds one frame",
        ),
        (
            json!({"frames": [frames[0]], "file": "../x.pdf"}),
            "plain name",
        ),
        (
            json!({"frames": [frames[0]], "file": "x.png"}),
            "plain name",
        ),
        (json!({"frames": ["9999"], "file": "x.pdf"}), ""),
    ] {
        let error = share(&mut ws, args).unwrap_err();
        assert!(error.contains(why), "{error}");
    }
    let exports = dir.path().join("p.od/exports");
    assert!(!exports.exists() || std::fs::read_dir(exports).unwrap().next().is_none());
}
