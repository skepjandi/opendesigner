#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use od_core::{Actor, NewNode, NodeKind, Operation, Paint, ProjectError, Property, project};

fn populated(dir: &std::path::Path) -> od_core::Session {
    let mut s = project::create(dir, "Store assets").unwrap();
    s.set_token("brand.gold", "#f2ca83").unwrap();
    let root = s.document().pages[0].root.id.clone();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.name = Some("iPhone 6.9".to_string());
    frame.width = 1320.0;
    frame.height = 2868.0;
    frame.fill = Some(Paint::parse("#060b1c").unwrap());
    let mut title = NewNode::new(NodeKind::Text);
    title.text = Some("Hold for\neverything.".to_string());
    title.font_size = Some(120.5);
    title.fill = Some(Paint::parse("{brand.gold}").unwrap());
    frame.children.push(title);
    s.apply(
        Actor::agent("claude-code").unwrap(),
        Some("t1".to_string()),
        &[Operation::Create {
            parent: root,
            index: None,
            node: frame,
        }],
    )
    .unwrap();
    s.checkpoint(Actor::User, "first draft").unwrap();
    s
}

fn snapshot(dir: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    fn walk(base: &std::path::Path, dir: &std::path::Path, out: &mut Vec<(String, Vec<u8>)>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap().display().to_string();
                out.push((rel, fs::read(&p).unwrap()));
            }
        }
    }
    walk(dir, dir, &mut files);
    files.sort();
    files
}

#[test]
fn a_saved_project_loads_equal_and_resaves_byte_identically() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let s = populated(&dir);
    project::save(&dir, &s).unwrap();
    let first = snapshot(&dir);

    let loaded = project::load(&dir).unwrap();
    assert_eq!(loaded.document(), s.document());
    assert_eq!(loaded.log(), s.log());
    assert_eq!(loaded.checkpoints(), s.checkpoints());

    project::save(&dir, &loaded).unwrap();
    assert_eq!(snapshot(&dir), first);
}

#[test]
fn loaded_history_still_restores_a_checkpoint() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let mut s = populated(&dir);
    let frame = s.document().pages[0].root.children[0].id.clone();
    s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: frame.clone(),
            prop: Property::Width(10.0),
        }],
    )
    .unwrap();
    project::save(&dir, &s).unwrap();

    let mut loaded = project::load(&dir).unwrap();
    loaded.restore(Actor::User, 1).unwrap();
    assert_eq!(loaded.document().node(&frame).unwrap().width, 1320.0);
}

#[test]
fn creating_over_an_existing_project_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    project::create(&dir, "a").unwrap();
    assert!(matches!(
        project::create(&dir, "b"),
        Err(ProjectError::Invalid(_))
    ));
}

#[test]
fn a_newer_format_is_refused_not_guessed_at() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    project::create(&dir, "a").unwrap();
    let manifest = fs::read_to_string(dir.join("od.toml")).unwrap();
    fs::write(
        dir.join("od.toml"),
        manifest.replace("format = 1", "format = 99"),
    )
    .unwrap();
    assert!(matches!(
        project::load(&dir),
        Err(ProjectError::NewerFormat {
            found: 99,
            supported: 1
        })
    ));
}

#[test]
fn hostile_project_files_are_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    project::create(&dir, "a").unwrap();

    // a page id that tries to leave the pages folder
    let manifest = fs::read_to_string(dir.join("od.toml")).unwrap();
    let page = fs::read_to_string(dir.join("pages/p_1.json")).unwrap();
    fs::write(
        dir.join("od.toml"),
        manifest.replace("\"p_1\"", "\"../../etc/passwd\""),
    )
    .unwrap();
    assert!(matches!(project::load(&dir), Err(ProjectError::Invalid(_))));
    fs::write(dir.join("od.toml"), &manifest).unwrap();

    // duplicate node ids
    let mut value: serde_json::Value = serde_json::from_str(&page).unwrap();
    let root_id = value["root"]["id"].clone();
    value["root"]["children"] = serde_json::json!([{
        "id": root_id, "kind": "rect", "name": "dup", "x": 0, "y": 0, "width": 1, "height": 1
    }]);
    fs::write(dir.join("pages/p_1.json"), value.to_string()).unwrap();
    assert!(matches!(project::load(&dir), Err(ProjectError::Invalid(_))));

    // an id counter that lags behind the ids in use would mint duplicates
    fs::write(dir.join("pages/p_1.json"), &page).unwrap();
    fs::write(
        dir.join("od.toml"),
        manifest.replace("next_id = 3", "next_id = 1"),
    )
    .unwrap();
    assert!(matches!(project::load(&dir), Err(ProjectError::Invalid(_))));
    fs::write(dir.join("od.toml"), &manifest).unwrap();

    // an unknown field in a page
    fs::write(
        dir.join("pages/p_1.json"),
        page.replace(
            "\"name\": \"Page 1\"",
            "\"name\": \"Page 1\", \"evil\": true",
        ),
    )
    .unwrap();
    assert!(matches!(
        project::load(&dir),
        Err(ProjectError::Parse { .. })
    ));
}
