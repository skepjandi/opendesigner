#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use od_core::{
    Actor, NewNode, NodeId, NodeKind, OpError, Operation, Paint, Property, Session, Stroke,
    TransactionKind,
};

fn session() -> (Session, NodeId) {
    let mut s = Session::new(od_core::Document::new("test"));
    let page = s.add_page("Page 1").unwrap();
    s.set_token("brand.gold", "#f2ca83").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    (s, root)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NewNode {
    let mut n = NewNode::new(NodeKind::Rect);
    n.x = x;
    n.y = y;
    n.width = w;
    n.height = h;
    n
}

fn create(parent: &NodeId, node: NewNode) -> Operation {
    Operation::Create {
        parent: parent.clone(),
        index: None,
        node,
    }
}

/// Content equality: ids are never reused, so `next_id` is allowed to have moved on.
fn same_content(a: &od_core::Document, b: &od_core::Document) -> bool {
    a.pages == b.pages && a.tokens == b.tokens && a.name == b.name
}

fn agent() -> Actor {
    Actor::agent("claude-code").unwrap()
}

fn created_id(s: &mut Session, parent: &NodeId, node: NewNode) -> NodeId {
    let r = s.apply(Actor::User, None, &[create(parent, node)]).unwrap();
    r.created[0].clone()
}

#[test]
fn create_set_move_delete_round_trip_through_undo_and_redo() {
    let (mut s, root) = session();
    let before = s.document().clone();
    let id = created_id(&mut s, &root, rect(10.0, 20.0, 100.0, 50.0));
    s.apply(
        Actor::User,
        None,
        &[
            Operation::Set {
                id: id.clone(),
                prop: Property::Fill(Some(Paint::parse("{brand.gold}").unwrap())),
            },
            Operation::Move {
                id: id.clone(),
                x: 5.0,
                y: 6.0,
            },
        ],
    )
    .unwrap();
    s.apply(Actor::User, None, &[Operation::Delete { id: id.clone() }])
        .unwrap();
    assert!(s.document().node(&id).is_none());

    s.undo(Actor::User).unwrap();
    let node = s.document().node(&id).unwrap();
    assert_eq!((node.x, node.y), (5.0, 6.0));
    s.undo(Actor::User).unwrap();
    let node = s.document().node(&id).unwrap();
    assert_eq!((node.x, node.y, node.fill.clone()), (10.0, 20.0, None));
    s.undo(Actor::User).unwrap();
    assert!(same_content(s.document(), &before));
    assert!(
        s.document().next_id > before.next_id,
        "ids are never reused"
    );

    s.redo(Actor::User).unwrap();
    s.redo(Actor::User).unwrap();
    s.redo(Actor::User).unwrap();
    assert!(s.document().node(&id).is_none());
    assert!(!s.can_redo());
}

#[test]
fn redo_reuses_the_ids_minted_by_the_original_create() {
    let (mut s, root) = session();
    let id = created_id(&mut s, &root, rect(0.0, 0.0, 10.0, 10.0));
    s.undo(Actor::User).unwrap();
    s.redo(Actor::User).unwrap();
    assert!(s.document().node(&id).is_some());
    let next = created_id(&mut s, &root, rect(0.0, 0.0, 1.0, 1.0));
    assert_ne!(next, id);
}

#[test]
fn a_failing_operation_rolls_the_whole_transaction_back() {
    let (mut s, root) = session();
    let before = s.document().clone();
    let err = s
        .apply(
            Actor::User,
            None,
            &[
                create(&root, rect(0.0, 0.0, 10.0, 10.0)),
                Operation::Move {
                    id: NodeId::parse("n_999").unwrap(),
                    x: 1.0,
                    y: 1.0,
                },
            ],
        )
        .unwrap_err();
    assert_eq!(err, OpError::NoSuchNode("n_999".to_string()));
    assert!(same_content(s.document(), &before));
    assert!(s.log().is_empty());
}

#[test]
fn restore_is_internal_and_refused_from_front_ends() {
    let (mut s, root) = session();
    let id = created_id(&mut s, &root, rect(0.0, 0.0, 10.0, 10.0));
    let node = s.document().node(&id).unwrap().clone();
    s.apply(Actor::User, None, &[Operation::Delete { id }])
        .unwrap();
    let err = s
        .apply(
            agent(),
            None,
            &[Operation::Restore {
                parent: root,
                index: 0,
                node,
            }],
        )
        .unwrap_err();
    assert_eq!(err, OpError::InternalOnly("restore"));
}

#[test]
fn checkpoint_restore_undoes_an_agent_turn_and_is_itself_undoable() {
    let (mut s, root) = session();
    let keep = created_id(&mut s, &root, rect(0.0, 0.0, 10.0, 10.0));
    let cp = s.checkpoint(Actor::User, "before agent").unwrap();
    let position = s.checkpoints()[0].position;

    let turn = Some("turn-1".to_string());
    let r = s
        .apply(
            agent(),
            turn.clone(),
            &[
                Operation::Set {
                    id: keep.clone(),
                    prop: Property::Width(999.0),
                },
                create(&root, rect(1.0, 1.0, 5.0, 5.0)),
            ],
        )
        .unwrap();
    let added = r.created[0].clone();
    let changed = s.changed_since(position);
    assert!(changed.contains(&keep) && changed.contains(&added));
    assert_eq!(s.log().last().unwrap().actor, agent());
    assert_eq!(s.log().last().unwrap().turn, turn);

    s.restore(Actor::User, cp).unwrap();
    assert_eq!(s.document().node(&keep).unwrap().width, 10.0);
    assert!(s.document().node(&added).is_none());
    assert!(matches!(
        s.log().last().unwrap().kind,
        TransactionKind::Restore { checkpoint } if checkpoint == cp
    ));

    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().node(&keep).unwrap().width, 999.0);
    assert!(s.document().node(&added).is_some());
}

#[test]
fn restore_after_undo_and_redo_still_reaches_the_checkpoint() {
    let (mut s, root) = session();
    let id = created_id(&mut s, &root, rect(0.0, 0.0, 10.0, 10.0));
    let cp = s.checkpoint(Actor::User, "start").unwrap();
    s.apply(
        Actor::User,
        None,
        &[Operation::Set {
            id: id.clone(),
            prop: Property::Width(50.0),
        }],
    )
    .unwrap();
    s.undo(Actor::User).unwrap();
    s.redo(Actor::User).unwrap();
    s.apply(Actor::User, None, &[Operation::Delete { id: id.clone() }])
        .unwrap();
    s.restore(Actor::User, cp).unwrap();
    assert_eq!(s.document().node(&id).unwrap().width, 10.0);
}

#[test]
fn reparent_keeps_order_and_refuses_cycles() {
    let (mut s, root) = session();
    let mut frame = NewNode::new(NodeKind::Frame);
    frame.width = 100.0;
    frame.height = 100.0;
    let a = created_id(&mut s, &root, frame.clone());
    let b = created_id(&mut s, &a, frame);
    let c = created_id(&mut s, &root, rect(0.0, 0.0, 1.0, 1.0));

    let err = s
        .apply(
            Actor::User,
            None,
            &[Operation::Reparent {
                id: a.clone(),
                parent: b.clone(),
                index: None,
            }],
        )
        .unwrap_err();
    assert!(matches!(err, OpError::Cycle { .. }));

    s.apply(
        Actor::User,
        None,
        &[Operation::Reparent {
            id: c.clone(),
            parent: b.clone(),
            index: Some(0),
        }],
    )
    .unwrap();
    let parent_b = s.document().node(&b).unwrap();
    assert_eq!(parent_b.children[0].id, c);
    s.undo(Actor::User).unwrap();
    assert!(s.document().node(&b).unwrap().children.is_empty());
    assert_eq!(s.document().node(&root).unwrap().children[1].id, c);
}

#[test]
fn reparent_within_the_same_parent_uses_post_removal_indices() {
    let (mut s, root) = session();
    let ids: Vec<NodeId> = (0..3)
        .map(|i| created_id(&mut s, &root, rect(f64::from(i), 0.0, 1.0, 1.0)))
        .collect();
    s.apply(
        Actor::User,
        None,
        &[Operation::Reparent {
            id: ids[0].clone(),
            parent: root.clone(),
            index: Some(2),
        }],
    )
    .unwrap();
    let order: Vec<_> = s
        .document()
        .node(&root)
        .unwrap()
        .children
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(order, vec![ids[1].clone(), ids[2].clone(), ids[0].clone()]);
    s.undo(Actor::User).unwrap();
    let order: Vec<_> = s
        .document()
        .node(&root)
        .unwrap()
        .children
        .iter()
        .map(|n| n.id.clone())
        .collect();
    assert_eq!(order, ids);
}

#[test]
fn validation_rejects_hostile_values_with_a_reason() {
    let (mut s, root) = session();
    let id = created_id(&mut s, &root, rect(0.0, 0.0, 10.0, 10.0));
    let try_set = |s: &mut Session, prop: Property| {
        s.apply(
            agent(),
            None,
            &[Operation::Set {
                id: id.clone(),
                prop,
            }],
        )
    };
    assert!(matches!(
        try_set(&mut s, Property::X(f64::NAN)),
        Err(OpError::OutOfBounds { what: "x", .. })
    ));
    assert!(matches!(
        try_set(&mut s, Property::Width(1e9)),
        Err(OpError::OutOfBounds { what: "width", .. })
    ));
    assert!(matches!(
        try_set(&mut s, Property::Opacity(1.5)),
        Err(OpError::OutOfBounds {
            what: "opacity",
            ..
        })
    ));
    assert!(matches!(
        try_set(&mut s, Property::Text("hi".into())),
        Err(OpError::NotApplicable {
            property: "text",
            kind: "rect"
        })
    ));
    assert_eq!(
        try_set(
            &mut s,
            Property::Fill(Some(Paint::parse("{nope}").unwrap()))
        ),
        Err(OpError::UnknownToken("nope".to_string()))
    );
    assert!(matches!(
        try_set(&mut s, Property::Name("a\u{0}b".into())),
        Err(OpError::OutOfBounds { what: "name", .. })
    ));
    let too_wide_stroke = Stroke {
        paint: Paint::parse("#000000").unwrap(),
        width: 1e6,
    };
    assert!(try_set(&mut s, Property::Stroke(Some(too_wide_stroke))).is_err());
}

#[test]
fn page_roots_are_immutable_and_leaves_cannot_have_children() {
    let (mut s, root) = session();
    assert!(matches!(
        s.apply(Actor::User, None, &[Operation::Delete { id: root.clone() }]),
        Err(OpError::RootImmutable(_))
    ));
    assert!(matches!(
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: root.clone(),
                prop: Property::Name("x".into())
            }]
        ),
        Err(OpError::RootImmutable(_))
    ));
    let leaf = created_id(&mut s, &root, rect(0.0, 0.0, 1.0, 1.0));
    assert!(matches!(
        s.apply(
            Actor::User,
            None,
            &[create(&leaf, rect(0.0, 0.0, 1.0, 1.0))]
        ),
        Err(OpError::NoChildren {
            parent_kind: "rect"
        })
    ));
}

#[test]
fn nesting_and_size_limits_hold() {
    let (mut s, root) = session();
    let mut deep = NewNode::new(NodeKind::Group);
    for _ in 0..70 {
        let mut outer = NewNode::new(NodeKind::Group);
        outer.children.push(deep);
        deep = outer;
    }
    assert!(matches!(
        s.apply(Actor::User, None, &[create(&root, deep)]),
        Err(OpError::Limit(_))
    ));
    let many = vec![create(&root, rect(0.0, 0.0, 1.0, 1.0)); 1001];
    assert!(matches!(
        s.apply(Actor::User, None, &many),
        Err(OpError::Limit(_))
    ));
}

#[test]
fn operations_round_trip_through_json_as_agents_will_send_them() {
    let json = r##"[
        {"op":"create","parent":"n_2","node":{"kind":"text","name":"Headline","x":40,"y":80,"text":"Hold for everything.","font_size":96,"fill":"#fcf1d9"}},
        {"op":"set","id":"n_3","prop":{"name":"opacity","value":0.5}},
        {"op":"set","id":"n_3","prop":{"name":"fill","value":null}},
        {"op":"reparent","id":"n_3","parent":"n_2"},
        {"op":"delete","id":"n_3"}
    ]"##;
    let ops: Vec<Operation> = serde_json::from_str(json).unwrap();
    assert_eq!(ops.len(), 5);
    let back = serde_json::to_string(&ops).unwrap();
    let again: Vec<Operation> = serde_json::from_str(&back).unwrap();
    assert_eq!(ops, again);
    assert!(
        serde_json::from_str::<Operation>(
            r#"{"op":"create","parent":"n_2","node":{"kind":"rect","evil":1}}"#
        )
        .is_err()
    );
    assert!(serde_json::from_str::<Operation>(r#"{"op":"delete","id":"../etc"}"#).is_err());
}

#[test]
fn text_styling_is_validated_undoable_and_only_for_text() {
    use od_core::TextAlign;
    let (mut s, root) = session();
    let mut headline = NewNode::new(NodeKind::Text);
    headline.text = Some("Hold".into());
    headline.font_weight = Some(700.0);
    headline.text_align = Some(TextAlign::Center);
    let text = created_id(&mut s, &root, headline);
    let node = s.document().node(&text).unwrap();
    assert_eq!(node.font_weight, Some(700.0));
    assert_eq!(node.text_align, Some(TextAlign::Center));

    let set = |s: &mut Session, id: &NodeId, prop: Property| {
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: id.clone(),
                prop,
            }],
        )
    };
    let before = s.document().clone();
    for bad in [
        Property::FontWeight(Some(50.0)),
        Property::FontWeight(Some(f64::NAN)),
        Property::LineHeight(Some(9.0)),
        Property::FontFamily(Some("  ".into())),
        Property::FontFamily(Some("a\nb".into())),
        Property::FontFamily(Some("x".repeat(101))),
    ] {
        assert!(
            matches!(
                set(&mut s, &text, bad.clone()),
                Err(OpError::OutOfBounds { .. })
            ),
            "{bad:?} should be refused"
        );
    }
    assert!(
        same_content(&before, s.document()),
        "a refused set changes nothing"
    );

    set(&mut s, &text, Property::FontFamily(Some("Inter".into()))).unwrap();
    set(&mut s, &text, Property::LineHeight(Some(1.5))).unwrap();
    assert_eq!(
        s.document().node(&text).unwrap().font_family.as_deref(),
        Some("Inter")
    );
    s.undo(Actor::User).unwrap();
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().node(&text).unwrap().font_family, None);
    assert_eq!(s.document().node(&text).unwrap().line_height, None);

    let rect_id = created_id(&mut s, &root, rect(0.0, 0.0, 10.0, 10.0));
    assert!(matches!(
        set(&mut s, &rect_id, Property::FontWeight(Some(400.0))),
        Err(OpError::NotApplicable { .. })
    ));
}

#[test]
fn image_nodes_only_reference_plain_media_files_and_fit_is_undoable() {
    use od_core::ImageFit;
    let (mut s, root) = session();
    let mut shot = NewNode::new(NodeKind::Image);
    shot.width = 100.0;
    shot.height = 50.0;
    shot.image = Some("media/shot.png".into());
    shot.image_fit = Some(ImageFit::Contain);
    shot.corner_radius = Some(12.0);
    let id = created_id(&mut s, &root, shot);
    let node = s.document().node(&id).unwrap();
    assert_eq!(node.image.as_deref(), Some("media/shot.png"));
    assert_eq!(node.image_fit, Some(ImageFit::Contain));
    assert_eq!(node.corner_radius, 12.0);

    let set = |s: &mut Session, prop: Property| {
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: id.clone(),
                prop,
            }],
        )
    };
    let before = s.document().clone();
    for bad in [
        "shot.png",
        "media/../secret.png",
        "/etc/passwd",
        "media/a/b.png",
        "media/shot.svg",
        "https://example.com/x.png",
        "media/.hidden.png",
    ] {
        assert!(
            matches!(
                set(&mut s, Property::Image(Some(bad.into()))),
                Err(OpError::OutOfBounds { .. })
            ),
            "{bad:?} should be refused"
        );
    }
    let mut hostile = NewNode::new(NodeKind::Image);
    hostile.image = Some("../../etc/passwd".into());
    assert!(
        s.apply(Actor::User, None, &[create(&root, hostile)])
            .is_err()
    );
    assert!(
        same_content(&before, s.document()),
        "refused edits change nothing"
    );

    set(&mut s, Property::ImageFit(Some(ImageFit::Fill))).unwrap();
    s.undo(Actor::User).unwrap();
    assert_eq!(
        s.document().node(&id).unwrap().image_fit,
        Some(ImageFit::Contain)
    );

    let mut text = NewNode::new(NodeKind::Text);
    text.text = Some("x".into());
    let text = created_id(&mut s, &root, text);
    assert!(matches!(
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: text,
                prop: Property::ImageFit(Some(ImageFit::Fill))
            }]
        ),
        Err(OpError::NotApplicable { .. })
    ));
}

#[test]
fn tokens_change_through_history_recolor_their_users_and_refuse_to_vanish_while_used() {
    let (mut s, root) = session();
    let mut card = rect(0.0, 0.0, 10.0, 10.0);
    card.fill = Some(Paint::parse("{brand.gold}").unwrap());
    let card = created_id(&mut s, &root, card);
    let cp = s.checkpoint(Actor::User, "before").unwrap();

    let set = |s: &mut Session, name: &str, value: Option<&str>| {
        s.apply(
            agent(),
            None,
            &[Operation::SetToken {
                name: name.into(),
                value: value.map(String::from),
            }],
        )
    };

    // changing a token reports the nodes that use it as touched, and stores the canonical hex
    let receipt = set(&mut s, "brand.gold", Some("#FF0000")).unwrap();
    assert_eq!(s.document().tokens["brand.gold"], "#ff0000");
    assert_eq!(receipt.touched, vec![card.clone()]);
    set(&mut s, "brand.ink", Some("#101010")).unwrap();
    assert_eq!(s.document().tokens.len(), 2);

    // the old value comes back with undo, and redo reapplies the exact stored value
    s.undo(Actor::User).unwrap();
    assert!(!s.document().tokens.contains_key("brand.ink"));
    s.undo(Actor::User).unwrap();
    assert_eq!(s.document().tokens["brand.gold"], "#f2ca83");
    s.redo(Actor::User).unwrap();
    assert_eq!(s.document().tokens["brand.gold"], "#ff0000");

    // an unused token can be removed; a used one cannot
    set(&mut s, "brand.ink", Some("#101010")).unwrap();
    set(&mut s, "brand.ink", None).unwrap();
    let before = s.document().clone();
    assert!(matches!(
        set(&mut s, "brand.gold", None),
        Err(OpError::TokenInUse { count: 1, .. })
    ));
    assert!(matches!(
        set(&mut s, "brand.nope", None),
        Err(OpError::UnknownToken(_))
    ));
    for bad in ["", "has space", "x#"] {
        assert!(set(&mut s, bad, Some("#000000")).is_err(), "{bad:?}");
    }
    assert!(set(&mut s, "brand.gold", Some("red")).is_err());
    assert!(
        same_content(&before, s.document()),
        "refused operations change nothing"
    );

    // a transaction can define a token and use it, and a failure rolls both back
    let fails = s.apply(
        agent(),
        None,
        &[
            Operation::SetToken {
                name: "brand.new".into(),
                value: Some("#00ff00".into()),
            },
            Operation::Set {
                id: card.clone(),
                prop: Property::Fill(Some(Paint::parse("{brand.missing}").unwrap())),
            },
        ],
    );
    assert!(fails.is_err());
    assert!(!s.document().tokens.contains_key("brand.new"));

    // restoring the checkpoint puts the original color back
    s.restore(Actor::User, cp).unwrap();
    assert_eq!(s.document().tokens["brand.gold"], "#f2ca83");
}
