#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{Actor, Document, NodeId, OpError, Operation, Session, Workspace};
use serde_json::{Value, json};

fn workspace() -> (Workspace, NodeId) {
    let mut doc = Document::new("Dry");
    doc.add_page("Page 1").unwrap();
    let root = doc.pages[0].root.id.clone();
    (Workspace::new(Session::new(doc)), root)
}

fn ops(v: Value) -> Vec<Operation> {
    serde_json::from_value(v).unwrap()
}

fn batch(root: &NodeId) -> Vec<Operation> {
    ops(json!([
        {"op": "create", "parent": root.as_str(),
         "node": {"kind": "frame", "name": "Hero", "width": 100, "height": 100,
                  "children": [{"kind": "text", "name": "T", "text": "Hi", "font_size": 20,
                                "width": 50, "height": 30}]}},
        {"op": "set_token", "name": "brand.gold", "value": "#c9a227"},
        {"op": "create_video", "name": "Promo", "fps": 30, "width": 640, "height": 360},
    ]))
}

#[test]
fn a_dry_run_reports_exactly_what_the_real_apply_does_and_changes_nothing() {
    let (mut ws, root) = workspace();
    let ops = batch(&root);
    let history_before = ws.session.log().len();
    let doc_before = ws.session.document().clone();

    let check = ws.check_ops(&ops);
    assert!(check.ok, "{check:?}");
    assert_eq!(check.ops.len(), 3);
    assert_eq!(ws.session.log().len(), history_before);
    assert_eq!(ws.session.document(), &doc_before);

    let mut copy = ws.scratch(doc_before);
    let real = copy.session.apply(Actor::User, None, &ops).unwrap();
    assert_eq!(check.would_change, real.touched);
    assert_eq!(check.would_create, real.created);

    let real_on_original = ws.session.apply(Actor::User, None, &ops).unwrap();
    assert_eq!(real_on_original.touched, check.would_change);
    assert_eq!(ws.session.document(), copy.session.document());
}

#[test]
fn a_rejected_dry_run_names_the_failing_operation_with_a_stable_code_and_a_hint() {
    let (ws, root) = workspace();
    let mut batch = batch(&root);
    batch.push(
        serde_json::from_value(json!({"op": "set", "id": "n_999",
                                      "prop": {"name": "x", "value": 1}}))
        .unwrap(),
    );
    let check = ws.check_ops(&batch);
    assert!(!check.ok);
    assert!(check.would_change.is_empty() && check.would_create.is_empty());
    let failed = check.ops.last().unwrap();
    assert_eq!((failed.index, failed.op, failed.ok), (3, "set", false));
    let error = failed.error.as_ref().unwrap();
    assert_eq!(error.code, "E_NODE_NOT_FOUND");
    assert_eq!(
        error.message,
        OpError::NoSuchNode("n_999".into()).to_string()
    );
    assert!(!error.hint.is_empty());
    assert!(check.ops[..3].iter().all(|r| r.ok));

    let mut copy = ws.scratch(ws.session.document().clone());
    let real = copy.session.apply(Actor::User, None, &batch).unwrap_err();
    assert_eq!(real.code(), error.code);
}

#[test]
fn an_empty_or_oversized_batch_is_a_batch_level_rejection() {
    let (ws, root) = workspace();
    let empty = ws.check_ops(&[]);
    assert!(!empty.ok);
    assert_eq!(empty.ops[0].error.as_ref().unwrap().code, "E_NOTHING_TO");

    let one = batch(&root).remove(1);
    let many = vec![one; od_core::limits::MAX_OPS_PER_TRANSACTION + 1];
    let big = ws.check_ops(&many);
    assert_eq!(big.ops[0].error.as_ref().unwrap().code, "E_LIMIT");
}

#[test]
fn a_dry_run_cannot_issue_internal_operations() {
    let (ws, _root) = workspace();
    let forged = ops(json!([{"op": "restore_variable", "id": "x", "index": 0,
                             "variable": null, "sets": []}]));
    let check = ws.check_ops(&forged);
    assert_eq!(check.ops[0].error.as_ref().unwrap().code, "E_INTERNAL_ONLY");
}

#[test]
fn codes_are_stable_and_the_display_text_is_unchanged() {
    let cases: Vec<(OpError, &str, &str)> = vec![
        (
            OpError::NoSuchNode("n_1".into()),
            "E_NODE_NOT_FOUND",
            "node n_1 does not exist",
        ),
        (
            OpError::OutOfBounds {
                what: "x",
                detail: "d".into(),
            },
            "E_OUT_OF_BOUNDS",
            "x is out of bounds: d",
        ),
        (
            OpError::Limit("at most 3".into()),
            "E_LIMIT",
            "limit exceeded: at most 3",
        ),
        (
            OpError::UnknownToken("a".into()),
            "E_UNKNOWN_TOKEN",
            "unknown design token a",
        ),
        (
            OpError::RootImmutable("p_1".into()),
            "E_ROOT_IMMUTABLE",
            "the root of page p_1 cannot be moved, deleted or restyled",
        ),
    ];
    for (err, code, text) in cases {
        assert_eq!(err.code(), code);
        assert_eq!(err.to_string(), text);
        assert!(err.hint().len() > 10);
        assert!(code.starts_with("E_") && code.chars().all(|c| c.is_ascii_uppercase() || c == '_'));
    }
}
