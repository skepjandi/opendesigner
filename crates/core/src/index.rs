//! Id to position lookup for a document's node tree.
//!
//! Every operation names its nodes by id, and finding one by walking the tree costs a pass over
//! every node. The index records each node's parent and its position among the parent's children,
//! so a lookup climbs the parent chain and then descends with one child access per level.
//!
//! The index is a hint, never an authority: every answer is checked against the tree (the child
//! at the recorded position has the id), a wrong position is repaired by scanning that parent's
//! children, and a node the index does not know is searched for by a plain walk. So a stale index
//! costs time, not correctness, whatever mutated the tree. Operations keep it fresh through
//! [`NodeIndex::note_inserted`] and [`NodeIndex::note_removed`] so the common edits never fall
//! back to a walk.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::model::{Location, Node, NodeId, Page};

/// Deeper than any valid document (`limits::MAX_DEPTH`); stops a corrupt parent cycle.
const CHAIN_LIMIT: usize = 4096;

#[derive(Debug, Clone)]
struct Entry {
    /// The parent node; `None` for a page root, whose `pos` is then the page's index.
    parent: Option<NodeId>,
    pos: usize,
}

#[derive(Debug, Default)]
struct State {
    entries: HashMap<NodeId, Entry>,
    built: bool,
}

#[derive(Debug, Default)]
pub(crate) struct NodeIndex {
    state: Mutex<State>,
}

/// A copy of a document starts without an index: it is rebuilt on its first lookup, so cloning a
/// large document does not copy the map.
impl Clone for NodeIndex {
    fn clone(&self) -> Self {
        NodeIndex::default()
    }
}

/// The index carries no document state, so it never makes two documents differ.
impl PartialEq for NodeIndex {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl NodeIndex {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn locate(&self, pages: &[Page], id: &NodeId) -> Option<Location> {
        let mut state = self.lock();
        if !state.built {
            state.rebuild(pages);
        }
        if let Some(loc) = state.resolve(pages, id) {
            return Some(loc);
        }
        let found = scan(pages, id)?;
        state.rebuild(pages);
        Some(found)
    }

    /// Records that `node` (and everything below it) now sits at `index` in `parent`'s children.
    pub(crate) fn note_inserted(&self, parent: &NodeId, index: usize, node: &Node) {
        let mut state = self.lock();
        if state.built {
            state.record(node, Some(parent), index);
        }
    }

    /// Records a new page whose root is `root`.
    pub(crate) fn note_page(&self, page: usize, root: &Node) {
        let mut state = self.lock();
        if state.built {
            state.record(root, None, page);
        }
    }

    /// Forgets `node` and everything below it.
    pub(crate) fn note_removed(&self, node: &Node) {
        let mut state = self.lock();
        if state.built {
            state.forget(node);
        }
    }
}

impl State {
    fn rebuild(&mut self, pages: &[Page]) {
        self.entries.clear();
        for (i, page) in pages.iter().enumerate() {
            self.record(&page.root, None, i);
        }
        self.built = true;
    }

    fn record(&mut self, node: &Node, parent: Option<&NodeId>, pos: usize) {
        self.entries.insert(
            node.id.clone(),
            Entry {
                parent: parent.cloned(),
                pos,
            },
        );
        for (i, child) in node.children.iter().enumerate() {
            self.record(child, Some(&node.id), i);
        }
    }

    fn forget(&mut self, node: &Node) {
        self.entries.remove(&node.id);
        for child in &node.children {
            self.forget(child);
        }
    }

    /// The node's location if the recorded chain leads to it, repairing positions that moved.
    fn resolve(&mut self, pages: &[Page], id: &NodeId) -> Option<Location> {
        let mut chain = vec![id];
        let mut top = self.entries.get(id)?;
        while let Some(parent) = &top.parent {
            if chain.len() > CHAIN_LIMIT {
                return None;
            }
            chain.push(parent);
            top = self.entries.get(parent)?;
        }
        let mut node = &pages.get(top.pos)?.root;
        let page = top.pos;
        if &node.id != chain.last().copied()? {
            return None;
        }
        let mut path = Vec::with_capacity(chain.len() - 1);
        let mut repairs: Vec<(NodeId, usize)> = Vec::new();
        for want in chain.iter().rev().skip(1) {
            let hint = self.entries.get(*want)?.pos;
            let pos = if node.children.get(hint).is_some_and(|c| &c.id == *want) {
                hint
            } else {
                let found = node.children.iter().position(|c| &c.id == *want)?;
                repairs.push(((*want).clone(), found));
                found
            };
            path.push(pos);
            node = &node.children[pos];
        }
        for (repaired, pos) in repairs {
            if let Some(entry) = self.entries.get_mut(&repaired) {
                entry.pos = pos;
            }
        }
        Some(Location { page, path })
    }
}

/// Finds `id` by walking every page: the reference answer the index must always agree with.
pub(crate) fn scan(pages: &[Page], id: &NodeId) -> Option<Location> {
    for (page, p) in pages.iter().enumerate() {
        let mut path = Vec::new();
        if find_path(&p.root, id, &mut path) {
            return Some(Location { page, path });
        }
    }
    None
}

fn find_path(node: &Node, id: &NodeId, path: &mut Vec<usize>) -> bool {
    if &node.id == id {
        return true;
    }
    for (i, child) in node.children.iter().enumerate() {
        path.push(i);
        if find_path(child, id, path) {
            return true;
        }
        path.pop();
    }
    false
}
