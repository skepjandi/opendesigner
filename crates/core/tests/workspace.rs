#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{
    Actor, AnnotationTarget, NewNode, NodeId, NodeKind, OpError, Operation, Property, Workspace,
};

fn workspace() -> (Workspace, tempfile::TempDir, NodeId) {
    let tmp = tempfile::tempdir().unwrap();
    let ws = Workspace::create(&tmp.path().join("p.od"), "Store assets").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    (ws, tmp, root)
}

fn add_rect(ws: &mut Workspace, parent: &NodeId, actor: Actor) -> NodeId {
    let mut n = NewNode::new(NodeKind::Rect);
    n.width = 10.0;
    n.height = 10.0;
    let r = ws
        .session
        .apply(
            actor,
            None,
            &[Operation::Create {
                parent: parent.clone(),
                index: None,
                node: n,
            }],
        )
        .unwrap();
    r.created[0].clone()
}

#[test]
fn guidance_carries_selection_open_pins_and_the_users_corrections() {
    let (mut ws, _tmp, root) = workspace();
    let agent = Actor::agent("claude-code").unwrap();
    let title = add_rect(&mut ws, &root, agent.clone());
    let pin = ws
        .annotations
        .add(
            ws.session.document(),
            AnnotationTarget::Node { id: title.clone() },
            "headline too small",
            Actor::User,
        )
        .unwrap();

    // the user nudges the node after the agent's turn: the agent can read the correction
    ws.session
        .apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: title.clone(),
                prop: Property::X(42.0),
            }],
        )
        .unwrap();
    ws.select(vec![title.clone()]).unwrap();

    let g = ws.guidance();
    assert_eq!(g.project, "Store assets");
    assert_eq!(g.selection.len(), 1);
    assert_eq!(g.selection[0].node.id, title);
    assert_eq!(g.selection[0].parent.as_ref(), Some(&root));
    assert_eq!(g.annotations.len(), 1);
    assert!(g.annotations[0].target_exists);
    assert_eq!(g.annotations[0].annotation.id, pin);
    assert_eq!(g.recent_user_edits.len(), 1);
    assert!(matches!(
        &g.recent_user_edits[0].ops[0],
        Operation::Set { prop: Property::X(x), .. } if *x == 42.0
    ));

    // once the agent acts again, the earlier user edits are no longer "recent"
    add_rect(&mut ws, &root, agent.clone());
    assert!(ws.guidance().recent_user_edits.is_empty());

    ws.annotations
        .resolve(pin, agent, Some("raised the size"))
        .unwrap();
    assert!(ws.guidance().annotations.is_empty());
}

#[test]
fn selection_rejects_unknown_nodes_and_drops_deleted_ones() {
    let (mut ws, _tmp, root) = workspace();
    let a = add_rect(&mut ws, &root, Actor::User);
    assert!(matches!(
        ws.select(vec![NodeId::parse("n_999").unwrap()]),
        Err(OpError::NoSuchNode(_))
    ));
    ws.select(vec![a.clone()]).unwrap();
    ws.session
        .apply(Actor::User, None, &[Operation::Delete { id: a }])
        .unwrap();
    assert!(ws.selection().is_empty());
    assert!(ws.guidance().selection.is_empty());
}

#[test]
fn a_pin_on_a_deleted_node_is_reported_as_orphaned() {
    let (mut ws, _tmp, root) = workspace();
    let a = add_rect(&mut ws, &root, Actor::User);
    ws.annotations
        .add(
            ws.session.document(),
            AnnotationTarget::Node { id: a.clone() },
            "check this",
            Actor::User,
        )
        .unwrap();
    ws.session
        .apply(Actor::User, None, &[Operation::Delete { id: a }])
        .unwrap();
    assert!(!ws.guidance().annotations[0].target_exists);
}

#[test]
fn annotations_persist_with_the_project() {
    let (mut ws, tmp, root) = workspace();
    let a = add_rect(&mut ws, &root, Actor::User);
    let id = ws
        .annotations
        .add(
            ws.session.document(),
            AnnotationTarget::Node { id: a },
            "more contrast",
            Actor::User,
        )
        .unwrap();
    ws.save().unwrap();

    let reopened = Workspace::open(&tmp.path().join("p.od")).unwrap();
    assert_eq!(reopened.annotations.get(id).unwrap().note, "more contrast");
    assert!(
        reopened.selection().is_empty(),
        "selection is not persisted"
    );

    std::fs::write(
        tmp.path().join("p.od/annotations.json"),
        r#"{"next_id":1,"items":[{"id":5,"target":{"type":"node","id":"n_3"},"note":"x","status":"open","author":"user"}]}"#,
    )
    .unwrap();
    assert!(Workspace::open(&tmp.path().join("p.od")).is_err());
}

#[test]
fn token_changes_are_saved_with_the_project_and_restore_after_reopening() {
    let (mut ws, tmp, _root) = workspace();
    let set = |ws: &mut Workspace, value: &str| {
        ws.session
            .apply(
                Actor::User,
                None,
                &[Operation::SetToken {
                    name: "brand.gold".into(),
                    value: Some(value.into()),
                }],
            )
            .unwrap();
    };
    set(&mut ws, "#f2ca83");
    let cp = ws.session.checkpoint(Actor::User, "gold").unwrap();
    set(&mut ws, "#ff0000");
    ws.save().unwrap();

    let mut reopened = Workspace::open(&tmp.path().join("p.od")).unwrap();
    assert_eq!(reopened.session.document().tokens["brand.gold"], "#ff0000");
    assert!(
        std::fs::read_to_string(tmp.path().join("p.od/.od/history/log.jsonl"))
            .unwrap()
            .contains("set_token")
    );
    // the log carries each change's inverse, so a checkpoint still restores after a reopen
    reopened.session.restore(Actor::User, cp).unwrap();
    assert_eq!(reopened.session.document().tokens["brand.gold"], "#f2ca83");
}
