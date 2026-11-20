#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, Document, NodeId, OpError, Operation, Session, Workspace};
use serde_json::{Value, json};

fn workspace() -> (Workspace, NodeId) {
    let mut doc = Document::new("Templates");
    doc.add_page("Page 1").unwrap();
    let root = doc.pages[0].root.id.clone();
    (Workspace::new(Session::new(doc)), root)
}

fn ops(v: Value) -> Vec<Operation> {
    serde_json::from_value(v).unwrap()
}

fn apply(ws: &mut Workspace, v: Value) -> od_core::TxReceipt {
    ws.session.apply(Actor::User, None, &ops(v)).unwrap()
}

fn headline(ws: &mut Workspace, root: &NodeId) -> NodeId {
    let made = apply(
        ws,
        json!([{"op": "create", "parent": root.as_str(),
                "node": {"kind": "text", "name": "Headline", "x": 0, "y": 0, "width": 400,
                         "height": 80, "text": "Hello", "font_size": 40}}]),
    );
    made.created[0].clone()
}

fn declare(node: &NodeId) -> Value {
    json!({"op": "declare_variable", "variable": {
        "id": "title", "kind": "string", "label": "Title", "default": "Welcome",
        "portrays": "subject_name",
        "binds": [{"node": node.as_str(), "prop": "text"}]}})
}

fn text_of(ws: &Workspace, id: &NodeId) -> String {
    ws.session
        .document()
        .node(id)
        .unwrap()
        .text
        .clone()
        .unwrap()
}

#[test]
fn declaring_a_variable_writes_its_default_into_the_bound_nodes() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    let receipt = apply(&mut ws, json!([declare(&text)]));
    assert_eq!(text_of(&ws, &text), "Welcome");
    assert_eq!(receipt.touched, vec![text.clone()]);
    let var = &ws.session.document().variables[0];
    assert_eq!(var.id.as_str(), "title");
    assert_eq!(var.label, "Title");
}

#[test]
fn setting_a_value_rewrites_bound_nodes_and_undo_redo_restore_them_exactly() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    apply(&mut ws, json!([declare(&text)]));
    let before = ws.session.document().clone();

    let agent = Actor::agent("claude-code").unwrap();
    ws.session
        .apply(
            agent.clone(),
            Some("t1".into()),
            &ops(json!([{"op": "set_variable_value", "id": "title", "value": "Acme"}])),
        )
        .unwrap();
    assert_eq!(text_of(&ws, &text), "Acme");
    let tx = ws.session.log().last().unwrap();
    assert_eq!(tx.actor, agent);
    let after = ws.session.document().clone();

    ws.session.undo(Actor::User).unwrap();
    assert_eq!(ws.session.document(), &before);
    ws.session.redo(Actor::User).unwrap();
    assert_eq!(ws.session.document(), &after);
}

#[test]
fn restoring_a_checkpoint_before_a_variable_change_puts_the_nodes_back() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    apply(&mut ws, json!([declare(&text)]));
    let cp = ws.session.checkpoint(Actor::User, "before").unwrap();
    apply(
        &mut ws,
        json!([{"op": "set_variable_value", "id": "title", "value": "Acme"}]),
    );
    ws.session.restore(Actor::User, cp).unwrap();
    assert_eq!(text_of(&ws, &text), "Welcome");
    assert_eq!(
        ws.session.document().variables[0].value,
        od_core::VarValue::Text("Welcome".into())
    );
}

#[test]
fn a_value_of_the_wrong_type_or_outside_the_choices_is_rejected_and_changes_nothing() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    apply(&mut ws, json!([declare(&text)]));
    let before = ws.session.document().clone();
    let wrong = ops(json!([{"op": "set_variable_value", "id": "title", "value": 12}]));
    let err = ws.session.apply(Actor::User, None, &wrong).unwrap_err();
    assert!(matches!(
        err,
        OpError::OutOfBounds {
            what: "variable",
            ..
        }
    ));
    assert_eq!(ws.session.document(), &before);

    apply(
        &mut ws,
        json!([{"op": "declare_variable", "variable": {
            "id": "align", "kind": "enum", "label": "Alignment", "default": "left",
            "enum_values": ["left", "center"],
            "binds": [{"node": text.as_str(), "prop": "text_align"}]}}]),
    );
    let bad = ops(json!([{"op": "set_variable_value", "id": "align", "value": "right"}]));
    let err = ws.session.apply(Actor::User, None, &bad).unwrap_err();
    assert!(err.to_string().contains("not one of the choices"), "{err}");
}

#[test]
fn a_binding_a_kind_cannot_drive_is_rejected_at_declaration() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    let bad = ops(json!([{"op": "declare_variable", "variable": {
        "id": "n", "kind": "number", "label": "N", "default": 1,
        "binds": [{"node": text.as_str(), "prop": "text"}]}}]));
    let err = ws.session.apply(Actor::User, None, &bad).unwrap_err();
    assert!(err.to_string().contains("cannot drive"), "{err}");
    let missing = ops(json!([{"op": "declare_variable", "variable": {
        "id": "n", "kind": "string", "label": "N", "default": "x",
        "binds": [{"node": "n_999", "prop": "text"}]}}]));
    assert!(matches!(
        ws.session.apply(Actor::User, None, &missing),
        Err(OpError::NoSuchNode(_))
    ));
}

#[test]
fn every_kind_binds_and_writes_through_the_validated_set_path() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    let id = text.as_str();
    apply(
        &mut ws,
        json!([
            {"op": "declare_variable", "variable": {"id": "size", "kind": "number", "label": "Size",
                "default": 64, "binds": [{"node": id, "prop": "font_size"}]}},
            {"op": "declare_variable", "variable": {"id": "ink", "kind": "color", "label": "Ink",
                "default": "#112233", "binds": [{"node": id, "prop": "fill"}]}},
            {"op": "declare_variable", "variable": {"id": "shown", "kind": "bool", "label": "Shown",
                "default": false, "binds": [{"node": id, "prop": "visible"}]}},
            {"op": "declare_variable", "variable": {"id": "face", "kind": "font", "label": "Font",
                "default": "Lato", "binds": [{"node": id, "prop": "font_family"}]}},
        ]),
    );
    let node = ws.session.document().node(&text).unwrap();
    assert_eq!(node.font_size, Some(64.0));
    assert_eq!(node.fill.as_ref().unwrap().to_string(), "#112233");
    assert!(!node.visible);
    assert_eq!(node.font_family.as_deref(), Some("Lato"));

    let out_of_range = ops(json!([{"op": "set_variable_value", "id": "size", "value": 99999}]));
    assert!(ws.session.apply(Actor::User, None, &out_of_range).is_err());
    assert_eq!(
        ws.session.document().node(&text).unwrap().font_size,
        Some(64.0)
    );
}

#[test]
fn removing_a_variable_keeps_node_values_and_undo_restores_its_place() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    apply(
        &mut ws,
        json!([
            declare(&text),
            {"op": "declare_variable", "variable": {"id": "second", "kind": "number",
                "label": "Second", "default": 1}},
        ]),
    );
    apply(
        &mut ws,
        json!([{"op": "set_variable_value", "id": "title", "value": "Acme"}]),
    );
    let before = ws.session.document().clone();
    apply(&mut ws, json!([{"op": "remove_variable", "id": "title"}]));
    assert_eq!(ws.session.document().variables.len(), 1);
    assert_eq!(text_of(&ws, &text), "Acme");
    ws.session.undo(Actor::User).unwrap();
    assert_eq!(ws.session.document(), &before);
}

#[test]
fn set_variable_changes_definitions_and_new_bindings_take_the_current_value() {
    let (mut ws, root) = workspace();
    let text = headline(&mut ws, &root);
    let other = headline(&mut ws, &root);
    apply(&mut ws, json!([declare(&text)]));
    let before = ws.session.document().clone();
    apply(
        &mut ws,
        json!([
            {"op": "set_variable", "id": "title", "prop": {"name": "label", "value": "Name"}},
            {"op": "set_variable", "id": "title", "prop": {"name": "portrays", "value": "host_identity"}},
            {"op": "set_variable", "id": "title", "prop": {"name": "binds", "value": [
                {"node": text.as_str(), "prop": "text"}, {"node": other.as_str(), "prop": "text"}]}},
        ]),
    );
    assert_eq!(text_of(&ws, &other), "Welcome");
    let var = &ws.session.document().variables[0];
    assert_eq!(var.label, "Name");
    assert_eq!(var.portrays, Some(od_core::Portrays::HostIdentity));
    ws.session.undo(Actor::User).unwrap();
    assert_eq!(ws.session.document(), &before);
}

#[test]
fn the_restore_operation_is_not_available_to_front_ends() {
    let (mut ws, _root) = workspace();
    let forged = ops(json!([{"op": "restore_variable", "id": "x", "index": 0,
                             "variable": null, "sets": []}]));
    assert!(matches!(
        ws.session.apply(Actor::User, None, &forged),
        Err(OpError::InternalOnly(_))
    ));
}

#[test]
fn variables_survive_saving_and_loading_a_project() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    let mut ws = Workspace::create(&dir, "Saved").unwrap();
    let root = ws.session.document().pages[0].root.id.clone();
    let text = headline(&mut ws, &root);
    apply(
        &mut ws,
        json!([declare(&text),
               {"op": "declare_variable", "variable": {"id": "size", "kind": "number",
                "label": "Size", "default": 12.5, "enum_values": []}}]),
    );
    apply(
        &mut ws,
        json!([{"op": "set_variable_value", "id": "title", "value": "Acme"}]),
    );
    ws.save().unwrap();
    let loaded = Workspace::open(&dir).unwrap();
    assert_eq!(loaded.session.document(), ws.session.document());
    let first = std::fs::read(dir.join("od.toml")).unwrap();
    loaded.save().unwrap();
    assert_eq!(std::fs::read(dir.join("od.toml")).unwrap(), first);
}

#[test]
fn a_project_without_variables_writes_no_variables_table() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("p.od");
    Workspace::create(&dir, "Plain").unwrap();
    let toml = std::fs::read_to_string(dir.join("od.toml")).unwrap();
    assert!(!toml.contains("variables"), "{toml}");
}
