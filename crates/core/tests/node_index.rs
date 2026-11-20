#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The id lookup index must answer exactly what a walk over the tree answers, through every kind
//! of edit: create, move, reparent, delete, undo, redo, restore and a reload.

use od_core::{Actor, Document, Node, NodeId, NodeKind, Operation, Session, model::Location};

fn walk(node: &Node, id: &NodeId, path: &mut Vec<usize>) -> bool {
    if &node.id == id {
        return true;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if walk(child, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

fn linear(doc: &Document, id: &NodeId) -> Option<Location> {
    doc.pages.iter().enumerate().find_map(|(page, p)| {
        let mut path = Vec::new();
        walk(&p.root, id, &mut path).then_some(Location { page, path })
    })
}

fn all_ids(doc: &Document) -> Vec<NodeId> {
    let mut ids = Vec::new();
    for page in &doc.pages {
        page.root.ids(&mut ids);
    }
    ids
}

fn agrees(doc: &Document, ghosts: &[NodeId]) {
    for id in all_ids(doc).iter().chain(ghosts) {
        assert_eq!(doc.locate(id), linear(doc, id), "lookup of {id}");
        assert_eq!(doc.node(id).map(|n| &n.id), linear(doc, id).map(|_| id));
    }
}

/// A small deterministic generator, so a failure reproduces.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        ((self.0 >> 33) as usize) % below.max(1)
    }
}

fn container_ids(doc: &Document) -> Vec<NodeId> {
    all_ids(doc)
        .into_iter()
        .filter(|id| doc.node(id).is_some_and(|n| n.kind.is_container()))
        .collect()
}

fn random_op(rng: &mut Lcg, doc: &Document) -> Operation {
    let ids = all_ids(doc);
    let containers = container_ids(doc);
    let pick = |rng: &mut Lcg, from: &[NodeId]| from[rng.next(from.len())].clone();
    match rng.next(6) {
        0 | 1 => {
            let kind = [NodeKind::Rect, NodeKind::Group, NodeKind::Frame][rng.next(3)];
            let parent = pick(rng, &containers);
            let len = doc.node(&parent).map_or(0, |n| n.children.len());
            let mut node = od_core::NewNode::new(kind);
            node.width = 10.0;
            node.height = 10.0;
            Operation::Create {
                parent,
                index: (rng.next(2) == 0).then(|| rng.next(len + 1)),
                node,
            }
        }
        2 => Operation::Move {
            id: pick(rng, &ids),
            x: rng.next(500) as f64,
            y: rng.next(500) as f64,
        },
        3 => Operation::Reparent {
            id: pick(rng, &ids),
            parent: pick(rng, &containers),
            index: None,
        },
        _ => Operation::Delete {
            id: pick(rng, &ids),
        },
    }
}

#[test]
fn indexed_lookup_matches_a_linear_walk_through_random_edits() {
    for seed in 1..=20u64 {
        let mut rng = Lcg(seed);
        let mut s = Session::new(Document::new("index"));
        s.add_page("one").unwrap();
        s.add_page("two").unwrap();
        let mut ghosts: Vec<NodeId> = vec![NodeId::parse("missing").unwrap()];
        let mut checkpoint = None;
        for step in 0..150 {
            let op = random_op(&mut rng, s.document());
            if let Operation::Delete { id } = &op {
                ghosts.push(id.clone());
            }
            let _ = s.apply(Actor::User, None, &[op]);
            match rng.next(12) {
                0 => {
                    let _ = s.undo(Actor::User);
                }
                1 => {
                    let _ = s.redo(Actor::User);
                }
                2 => checkpoint = s.checkpoint(Actor::User, "cp").ok(),
                3 if step > 60 => {
                    if let Some(cp) = checkpoint {
                        let _ = s.restore(Actor::User, cp);
                    }
                }
                _ => {}
            }
            agrees(s.document(), &ghosts);
        }
    }
}

#[test]
fn a_cloned_document_looks_up_the_same_nodes() {
    let mut s = Session::new(Document::new("clone"));
    let page = s.add_page("p").unwrap();
    let root = s.document().page(&page).unwrap().root.id.clone();
    for _ in 0..5 {
        let mut node = od_core::NewNode::new(NodeKind::Rect);
        node.width = 4.0;
        node.height = 4.0;
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root.clone(),
                index: Some(0),
                node,
            }],
        )
        .unwrap();
    }
    let copy = s.document().clone();
    agrees(&copy, &[]);
    assert_eq!(&copy, s.document());
}

#[test]
fn mutating_the_tree_directly_never_yields_a_wrong_answer() {
    let mut s = Session::new(Document::new("direct"));
    let page = s.add_page("p").unwrap();
    let mut doc = s.document().clone();
    let root = doc.page(&page).unwrap().root.id.clone();
    let first = doc.locate(&root);
    assert!(first.is_some());
    let mut a = Node::clone(&doc.pages[0].root);
    a.id = NodeId::parse("direct_a").unwrap();
    let mut b = a.clone();
    b.id = NodeId::parse("direct_b").unwrap();
    doc.pages[0].root.children.push(a);
    doc.pages[0].root.children.insert(0, b);
    agrees(&doc, &[]);
    doc.pages[0].root.children.remove(0);
    agrees(&doc, &[NodeId::parse("direct_b").unwrap()]);
}
