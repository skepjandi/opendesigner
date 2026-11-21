//! Arranging a selection: align, distribute, stack, duplicate, group, ungroup, z-order, copy and
//! paste. Every function reads the document and returns ordinary operations; the caller applies
//! them as one transaction, so one gesture is one undo step and every limit, id rule and
//! attribution of `ops` still holds.
//!
//! Coordinates: nodes are positioned in their parent's coordinates, and a group is a transform
//! only (it has no size; its bounds are its visible children's). Layout is computed in page
//! coordinates and converted back by translating, which is exact because the transforms are
//! offsets.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::geometry::{self, Rect};
use crate::model::{Document, Location, Node, NodeId, NodeKind};
use crate::ops::{NewNode, Operation};

/// How far a duplicate or a repeated paste sits from what it copies.
pub const COPY_OFFSET: f64 = 20.0;
/// The space left between a copied frame and the frames already on the page.
pub const FRAME_GAP: f64 = 100.0;
const MAX_PASTE_SHIFTS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignKind {
    Left,
    HCenter,
    Right,
    Top,
    VMiddle,
    Bottom,
}

impl AlignKind {
    pub fn parse(s: &str) -> Result<AlignKind, OpError> {
        Ok(match s {
            "left" => AlignKind::Left,
            "hcenter" => AlignKind::HCenter,
            "right" => AlignKind::Right,
            "top" => AlignKind::Top,
            "vmiddle" => AlignKind::VMiddle,
            "bottom" => AlignKind::Bottom,
            other => {
                return Err(OpError::Arrange(format!(
                    "unknown alignment {other:?}; use left, hcenter, right, top, vmiddle or bottom"
                )));
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub fn parse(s: &str) -> Result<Axis, OpError> {
        match s {
            "h" => Ok(Axis::Horizontal),
            "v" => Ok(Axis::Vertical),
            other => Err(OpError::Arrange(format!(
                "unknown axis {other:?}; use h or v"
            ))),
        }
    }

    fn start(self, r: Rect) -> f64 {
        match self {
            Axis::Horizontal => r.x,
            Axis::Vertical => r.y,
        }
    }

    fn len(self, r: Rect) -> f64 {
        match self {
            Axis::Horizontal => r.width,
            Axis::Vertical => r.height,
        }
    }

    fn other(self) -> Axis {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }

    fn shift(self, along: f64, across: f64) -> (f64, f64) {
        match self {
            Axis::Horizontal => (along, across),
            Axis::Vertical => (across, along),
        }
    }
}

/// A selected node with where it is on the page.
struct Item {
    id: NodeId,
    x: f64,
    y: f64,
    bounds: Rect,
}

/// The selected nodes that no other selected node contains, in document (stacking) order. A page
/// root, unknown ids and duplicates are dropped.
pub fn top_level(doc: &Document, ids: &[NodeId]) -> Vec<NodeId> {
    fn walk(node: &Node, wanted: &HashSet<&NodeId>, out: &mut Vec<NodeId>) {
        for child in &node.children {
            if wanted.contains(&child.id) {
                out.push(child.id.clone());
            } else {
                walk(child, wanted, out);
            }
        }
    }
    let wanted: HashSet<&NodeId> = ids.iter().collect();
    let mut out = Vec::new();
    for page in &doc.pages {
        walk(&page.root, &wanted, &mut out);
    }
    out
}

fn items(doc: &Document, ids: &[NodeId]) -> Vec<Item> {
    top_level(doc, ids)
        .into_iter()
        .filter_map(|id| {
            let node = doc.node(&id)?;
            let bounds = geometry::page_bounds(doc, &id)?;
            Some(Item {
                x: node.x,
                y: node.y,
                bounds,
                id,
            })
        })
        .collect()
}

fn moved(item: &Item, dx: f64, dy: f64) -> Option<Operation> {
    const EPS: f64 = 1e-9;
    (dx.abs() > EPS || dy.abs() > EPS).then(|| Operation::Move {
        id: item.id.clone(),
        x: item.x + dx,
        y: item.y + dy,
    })
}

fn union_of(items: &[Item]) -> Option<Rect> {
    items.iter().map(|i| i.bounds).reduce(Rect::union)
}

/// The page bounds of the nearest frame above `id`.
fn enclosing_frame(doc: &Document, id: &NodeId) -> Option<(NodeId, Rect)> {
    let loc = doc.locate(id)?;
    (1..loc.path.len()).rev().find_map(|depth| {
        let ancestor = doc.node_at(&Location {
            page: loc.page,
            path: loc.path[..depth].to_vec(),
        })?;
        (ancestor.kind == NodeKind::Frame)
            .then(|| geometry::page_bounds(doc, &ancestor.id).map(|b| (ancestor.id.clone(), b)))
            .flatten()
    })
}

/// Aligns the selection to the union of its bounds; one node aligns to the frame around it
/// (nothing happens for a node that is not inside a frame).
pub fn align(doc: &Document, ids: &[NodeId], kind: AlignKind) -> Result<Vec<Operation>, OpError> {
    let items = items(doc, ids);
    let target = match items.as_slice() {
        [] => return Ok(Vec::new()),
        [one] => match enclosing_frame(doc, &one.id) {
            Some((_, frame)) => frame,
            None => return Ok(Vec::new()),
        },
        many => union_of(many).ok_or_else(|| OpError::Arrange("nothing to align".into()))?,
    };
    Ok(items
        .iter()
        .filter_map(|item| {
            let (b, t) = (item.bounds, target);
            let (dx, dy) = match kind {
                AlignKind::Left => (t.x - b.x, 0.0),
                AlignKind::HCenter => (t.x + t.width / 2.0 - (b.x + b.width / 2.0), 0.0),
                AlignKind::Right => (t.right() - b.right(), 0.0),
                AlignKind::Top => (0.0, t.y - b.y),
                AlignKind::VMiddle => (0.0, t.y + t.height / 2.0 - (b.y + b.height / 2.0)),
                AlignKind::Bottom => (0.0, t.bottom() - b.bottom()),
            };
            moved(item, dx, dy)
        })
        .collect())
}

fn sorted_along(mut items: Vec<Item>, axis: Axis) -> Vec<Item> {
    items.sort_by(|a, b| axis.start(a.bounds).total_cmp(&axis.start(b.bounds)));
    items
}

/// Spreads three or more nodes so the gaps between them are equal; the outermost two stay put.
pub fn distribute(doc: &Document, ids: &[NodeId], axis: Axis) -> Result<Vec<Operation>, OpError> {
    let items = sorted_along(items(doc, ids), axis);
    if items.len() < 3 {
        return Err(OpError::Arrange(
            "distributing needs at least three nodes".into(),
        ));
    }
    let first = axis.start(items[0].bounds);
    let last = items
        .last()
        .map_or(first, |i| axis.start(i.bounds) + axis.len(i.bounds));
    let sizes: f64 = items.iter().map(|i| axis.len(i.bounds)).sum();
    let gap = (last - first - sizes) / (items.len() - 1) as f64;
    let mut cursor = first;
    let mut ops = Vec::new();
    for item in &items {
        let (dx, dy) = axis.shift(cursor - axis.start(item.bounds), 0.0);
        ops.extend(moved(item, dx, dy));
        cursor += axis.len(item.bounds) + gap;
    }
    Ok(ops)
}

/// Lays the selection edge to edge along `axis` with `gap` between neighbours, in the order they
/// already have along it. The first node keeps its place and the others take its position on the
/// other axis. This is a one-time arrangement, not a persistent layout.
pub fn stack(
    doc: &Document,
    ids: &[NodeId],
    axis: Axis,
    gap: f64,
) -> Result<Vec<Operation>, OpError> {
    if !gap.is_finite() {
        return Err(OpError::Arrange("the gap must be a number".into()));
    }
    let items = sorted_along(items(doc, ids), axis);
    let Some(first) = items.first() else {
        return Ok(Vec::new());
    };
    let across = axis.other().start(first.bounds);
    let mut cursor = axis.start(first.bounds);
    let mut ops = Vec::new();
    for item in &items {
        let (dx, dy) = axis.shift(
            cursor - axis.start(item.bounds),
            across - axis.other().start(item.bounds),
        );
        ops.extend(moved(item, dx, dy));
        cursor += axis.len(item.bounds) + gap;
    }
    Ok(ops)
}

/// Moves every selected node by `(dx, dy)`.
pub fn translate(
    doc: &Document,
    ids: &[NodeId],
    dx: f64,
    dy: f64,
) -> Result<Vec<Operation>, OpError> {
    if !dx.is_finite() || !dy.is_finite() {
        return Err(OpError::Arrange("the offset must be a number".into()));
    }
    Ok(items(doc, ids)
        .iter()
        .filter_map(|item| moved(item, dx, dy))
        .collect())
}

/// The parent of `id` and its place among the parent's children.
fn parent_and_index(doc: &Document, id: &NodeId) -> Option<(NodeId, usize)> {
    let loc = doc.locate(id)?;
    let (&index, rest) = loc.path.split_last()?;
    let parent = doc.node_at(&Location {
        page: loc.page,
        path: rest.to_vec(),
    })?;
    Some((parent.id.clone(), index))
}

/// Moves the selection `delta` places forward (positive) or backward in the stacking order of
/// each parent; selected nodes keep their order and never jump over each other. A `delta` larger
/// than the number of siblings sends them to the front or back.
pub fn bring(doc: &Document, ids: &[NodeId], delta: i32) -> Vec<Operation> {
    let selected: HashSet<NodeId> = top_level(doc, ids).into_iter().collect();
    let mut parents: Vec<NodeId> = Vec::new();
    for id in top_level(doc, ids) {
        if let Some((parent, _)) = parent_and_index(doc, &id) {
            if !parents.contains(&parent) {
                parents.push(parent);
            }
        }
    }
    let mut ops = Vec::new();
    for parent in parents {
        let Some(node) = doc.node(&parent) else {
            continue;
        };
        let before: Vec<(NodeId, bool)> = node
            .children
            .iter()
            .map(|c| (c.id.clone(), selected.contains(&c.id)))
            .collect();
        let after = stepped(before.clone(), delta);
        if after == before {
            continue;
        }
        ops.extend(after.iter().enumerate().filter(|(_, (_, sel))| *sel).map(
            |(index, (id, _))| Operation::Reparent {
                id: id.clone(),
                parent: parent.clone(),
                index: Some(index),
            },
        ));
    }
    ops
}

fn stepped(mut order: Vec<(NodeId, bool)>, delta: i32) -> Vec<(NodeId, bool)> {
    let steps = (delta.unsigned_abs() as usize).min(order.len());
    for _ in 0..steps {
        if delta > 0 {
            for i in (0..order.len().saturating_sub(1)).rev() {
                if order[i].1 && !order[i + 1].1 {
                    order.swap(i, i + 1);
                }
            }
        } else {
            for i in 1..order.len() {
                if order[i].1 && !order[i - 1].1 {
                    order.swap(i, i - 1);
                }
            }
        }
    }
    order
}

fn new_node_from(node: &Node) -> NewNode {
    NewNode {
        kind: node.kind,
        name: Some(node.name.clone()),
        x: node.x,
        y: node.y,
        width: node.width,
        height: node.height,
        visible: Some(node.visible),
        opacity: Some(node.opacity),
        rotation: Some(node.rotation),
        scale_x: Some(node.scale_x),
        scale_y: Some(node.scale_y),
        pivot_x: Some(node.pivot_x),
        pivot_y: Some(node.pivot_y),
        blur: Some(node.blur),
        shadow: node.shadow.clone(),
        clip: node.kind.is_container().then_some(node.clip),
        depth: Some(node.depth),
        fill: node.fill.clone(),
        stroke: node.stroke.clone(),
        corner_radius: matches!(
            node.kind,
            NodeKind::Frame | NodeKind::Rect | NodeKind::Image
        )
        .then_some(node.corner_radius),
        text: node.text.clone(),
        font_size: node.font_size,
        font_family: node.font_family.clone(),
        font_weight: node.font_weight,
        text_align: node.text_align,
        line_height: node.line_height,
        image: node.image.clone(),
        image_fit: node.image_fit,
        device3d: node.device3d.clone(),
        children: node.children.iter().map(new_node_from).collect(),
    }
}

/// Frames never nest: a copy of one goes to the right of the frames its parent already holds,
/// keeping the copies' own spacing.
fn beside_shift(parent: &Node, frames_x: impl Iterator<Item = f64>) -> f64 {
    let right = parent
        .children
        .iter()
        .map(|c| c.x + c.width)
        .fold(0.0, f64::max);
    right + FRAME_GAP - frames_x.fold(f64::INFINITY, f64::min)
}

/// Copies each selected node (with its subtree) next to the original, on top of it in the same
/// parent, with new ids. Frames are placed beside the existing frames, everything else
/// `COPY_OFFSET` down and right. The copies are the `created` nodes of the transaction, last
/// original first. Translations of copied text are not copied.
pub fn duplicate(doc: &Document, ids: &[NodeId]) -> Result<Vec<Operation>, OpError> {
    let top = top_level(doc, ids);
    let mut ops = Vec::new();
    for id in top.iter().rev() {
        let (Some(node), Some((parent_id, index))) = (doc.node(id), parent_and_index(doc, id))
        else {
            continue;
        };
        let Some(parent) = doc.node(&parent_id) else {
            continue;
        };
        let mut copy = new_node_from(node);
        if node.kind == NodeKind::Frame {
            let selected_frames = top
                .iter()
                .filter_map(|t| doc.node(t))
                .filter(|n| n.kind == NodeKind::Frame && parent_has(parent, &n.id))
                .map(|n| n.x);
            copy.x += beside_shift(parent, selected_frames);
        } else {
            copy.x += COPY_OFFSET;
            copy.y += COPY_OFFSET;
        }
        ops.push(Operation::Create {
            parent: parent_id,
            index: Some(index + 1),
            node: copy,
        });
    }
    Ok(ops)
}

fn parent_has(parent: &Node, id: &NodeId) -> bool {
    parent.children.iter().any(|c| &c.id == id)
}

/// Wraps the selected nodes, which must share one parent, in a new group that takes the place of
/// the topmost of them in the stacking order. Nodes keep their order and their place on the page.
/// The group is the one created node.
pub fn group(doc: &Document, ids: &[NodeId]) -> Result<Vec<Operation>, OpError> {
    let items = items(doc, ids);
    let Some(first) = items.first() else {
        return Err(OpError::Arrange("select something to group".into()));
    };
    let (parent, _) = parent_and_index(doc, &first.id)
        .ok_or_else(|| OpError::Arrange("the selection cannot be grouped".into()))?;
    let mut top_index = 0;
    for item in &items {
        match parent_and_index(doc, &item.id) {
            Some((p, index)) if p == parent => top_index = top_index.max(index),
            _ => {
                return Err(OpError::Arrange(
                    "only nodes in the same layer can be grouped".into(),
                ));
            }
        }
    }
    let bounds = union_of(&items)
        .ok_or_else(|| OpError::Arrange("the selection has nothing visible to group".into()))?;
    let (ox, oy) = geometry::parent_origin(doc, &first.id)
        .ok_or_else(|| OpError::Arrange("the selection cannot be grouped".into()))?;
    let (gx, gy) = (bounds.x - ox, bounds.y - oy);
    let group_id = NodeId::parse(&format!("n_{}", doc.next_id))?;
    let mut shell = NewNode::new(NodeKind::Group);
    shell.name = Some("Group".to_string());
    shell.x = gx;
    shell.y = gy;
    let mut ops = vec![Operation::Create {
        parent: parent.clone(),
        index: Some(top_index + 1),
        node: shell,
    }];
    for item in &items {
        ops.push(Operation::Reparent {
            id: item.id.clone(),
            parent: group_id.clone(),
            index: None,
        });
        ops.push(Operation::Move {
            id: item.id.clone(),
            x: item.x - gx,
            y: item.y - gy,
        });
    }
    Ok(ops)
}

/// Whether `group` would work: something is selected and it all shares a parent.
pub fn can_group(doc: &Document, ids: &[NodeId]) -> bool {
    let top = top_level(doc, ids);
    let mut parents = top.iter().filter_map(|id| parent_and_index(doc, id));
    match parents.next() {
        Some((first, _)) => top.len() == 1 || parents.all(|(p, _)| p == first),
        None => false,
    }
}

/// Whether the selection holds a group `ungroup` can dissolve.
pub fn can_ungroup(doc: &Document, ids: &[NodeId]) -> bool {
    top_level(doc, ids)
        .iter()
        .any(|id| doc.node(id).is_some_and(|n| n.kind == NodeKind::Group))
}

/// Dissolves each selected group: its children move into the group's parent at the group's place,
/// keeping their order and their place on the page. Returns the operations and the freed ids.
pub fn ungroup(doc: &Document, ids: &[NodeId]) -> Result<(Vec<Operation>, Vec<NodeId>), OpError> {
    let mut ops = Vec::new();
    let mut freed = Vec::new();
    for id in top_level(doc, ids) {
        let Some(node) = doc.node(&id).filter(|n| n.kind == NodeKind::Group) else {
            continue;
        };
        let (parent, index) = parent_and_index(doc, &id)
            .ok_or_else(|| OpError::Arrange("the group cannot be dissolved".into()))?;
        for (k, child) in node.children.iter().enumerate() {
            ops.push(Operation::Reparent {
                id: child.id.clone(),
                parent: parent.clone(),
                index: Some(index + k),
            });
            ops.push(Operation::Move {
                id: child.id.clone(),
                x: child.x + node.x,
                y: child.y + node.y,
            });
            freed.push(child.id.clone());
        }
        ops.push(Operation::Delete { id });
    }
    if ops.is_empty() {
        return Err(OpError::Arrange("select a group to ungroup".into()));
    }
    Ok((ops, freed))
}

/// Copied subtrees. Each top-level node's position is relative to the frame it came from (to the
/// page for frames), so it lands in the same place of another frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Clip {
    pub nodes: Vec<Node>,
}

impl Clip {
    pub fn to_json(&self) -> Result<String, OpError> {
        serde_json::to_string(self).map_err(|e| OpError::Internal(e.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Clip, OpError> {
        serde_json::from_str(json).map_err(|e| OpError::Arrange(format!("bad clipboard: {e}")))
    }
}

pub fn copy(doc: &Document, ids: &[NodeId]) -> Result<Clip, OpError> {
    let mut nodes = Vec::new();
    for id in top_level(doc, ids) {
        let Some(node) = doc.node(&id) else { continue };
        let (ox, oy) = geometry::parent_origin(doc, &id).unwrap_or((0.0, 0.0));
        let (fx, fy) = match enclosing_frame(doc, &id) {
            Some((_, frame)) if node.kind != NodeKind::Frame => (frame.x, frame.y),
            _ => (0.0, 0.0),
        };
        let mut clone = node.clone();
        clone.x = ox + node.x - fx;
        clone.y = oy + node.y - fy;
        nodes.push(clone);
    }
    if nodes.is_empty() {
        return Err(OpError::Arrange("select something to copy".into()));
    }
    Ok(Clip { nodes })
}

/// Pastes a clip into `frame`: new ids, same position, shifted by `COPY_OFFSET` steps while an
/// identical node already sits there (so pasting twice does not stack copies exactly). Frames in
/// the clip go beside the page's frames instead of into `frame`.
pub fn paste(
    doc: &Document,
    clip: &Clip,
    frame: Option<&NodeId>,
) -> Result<Vec<Operation>, OpError> {
    let target = frame
        .and_then(|f| doc.node(f))
        .filter(|n| n.kind == NodeKind::Frame);
    let root = page_root_for(doc, target)?;
    let frames_x = clip
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Frame)
        .map(|n| n.x);
    let frame_shift = beside_shift(root, frames_x);
    let flat = || clip.nodes.iter().filter(|n| n.kind != NodeKind::Frame);
    let shift = match target {
        Some(t) => free_shift(t, flat()),
        None if flat().next().is_none() => 0.0,
        None => return Err(OpError::Arrange("open a frame to paste into".into())),
    };
    let mut ops = Vec::new();
    for node in &clip.nodes {
        let mut new = new_node_from(node);
        let parent = if node.kind == NodeKind::Frame {
            new.x += frame_shift;
            root.id.clone()
        } else {
            new.x += shift;
            new.y += shift;
            match target {
                Some(t) => t.id.clone(),
                None => continue,
            }
        };
        ops.push(Operation::Create {
            parent,
            index: None,
            node: new,
        });
    }
    Ok(ops)
}

fn page_root_for<'a>(doc: &'a Document, target: Option<&Node>) -> Result<&'a Node, OpError> {
    let page = match target.and_then(|t| doc.locate(&t.id)) {
        Some(loc) => doc.pages.get(loc.page),
        None => doc.pages.first(),
    };
    page.map(|p| &p.root)
        .ok_or_else(|| OpError::Arrange("the project has no page".into()))
}

/// The smallest multiple of `COPY_OFFSET` that moves every node clear of an identical sibling.
fn free_shift<'a>(target: &Node, nodes: impl Iterator<Item = &'a Node> + Clone) -> f64 {
    let occupied = |shift: f64| {
        nodes.clone().any(|n| {
            target.children.iter().any(|c| {
                c.kind == n.kind
                    && c.x == n.x + shift
                    && c.y == n.y + shift
                    && c.width == n.width
                    && c.height == n.height
            })
        })
    };
    (0..MAX_PASTE_SHIFTS)
        .map(|k| k as f64 * COPY_OFFSET)
        .find(|&s| !occupied(s))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, Session, TxReceipt};

    struct Fx {
        s: Session,
        root: NodeId,
        frame: NodeId,
        other: NodeId,
        a: NodeId,
        b: NodeId,
        c: NodeId,
        g: NodeId,
        d: NodeId,
    }

    fn rect(name: &str, x: f64, y: f64, w: f64, h: f64) -> NewNode {
        let mut n = NewNode::new(NodeKind::Rect);
        n.name = Some(name.into());
        n.x = x;
        n.y = y;
        n.width = w;
        n.height = h;
        n
    }

    fn frame_at(x: f64, w: f64, h: f64) -> NewNode {
        let mut f = NewNode::new(NodeKind::Frame);
        f.x = x;
        f.width = w;
        f.height = h;
        f
    }

    fn fixture() -> Fx {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut frame = frame_at(0.0, 800.0, 600.0);
        let mut group = NewNode::new(NodeKind::Group);
        group.x = 300.0;
        group.y = 300.0;
        group.children.push(rect("d", 10.0, 10.0, 40.0, 40.0));
        frame.children = vec![
            rect("a", 0.0, 0.0, 100.0, 50.0),
            rect("b", 200.0, 40.0, 50.0, 50.0),
            rect("c", 400.0, 100.0, 100.0, 100.0),
            group,
        ];
        let created = |s: &mut Session, node| {
            let op = Operation::Create {
                parent: root.clone(),
                index: None,
                node,
            };
            s.apply(Actor::User, None, &[op]).unwrap().created[0].clone()
        };
        let frame_id = created(&mut s, frame);
        let other = created(&mut s, frame_at(1000.0, 300.0, 300.0));
        let kids = s.document().node(&frame_id).unwrap().children.clone();
        Fx {
            a: kids[0].id.clone(),
            b: kids[1].id.clone(),
            c: kids[2].id.clone(),
            g: kids[3].id.clone(),
            d: kids[3].children[0].id.clone(),
            s,
            root,
            frame: frame_id,
            other,
        }
    }

    fn run(s: &mut Session, ops: Vec<Operation>) -> TxReceipt {
        s.apply(Actor::User, None, &ops).unwrap()
    }

    fn bounds(s: &Session, id: &NodeId) -> Rect {
        geometry::page_bounds(s.document(), id).unwrap()
    }

    fn children(s: &Session, id: &NodeId) -> Vec<NodeId> {
        let node = s.document().node(id).unwrap();
        node.children.iter().map(|c| c.id.clone()).collect()
    }

    /// Applies `ops` as one command and checks one undo restores the pages exactly.
    fn undoable(s: &mut Session, ops: Vec<Operation>) -> TxReceipt {
        let before = s.document().pages.clone();
        let log = s.log().len();
        let receipt = run(s, ops);
        assert_eq!(s.log().len(), log + 1, "one command is one transaction");
        s.document().validate().unwrap();
        let after = s.document().pages.clone();
        s.undo(Actor::User).unwrap();
        assert_eq!(s.document().pages, before, "one undo reverts the command");
        s.redo(Actor::User).unwrap();
        assert_eq!(s.document().pages, after, "redo reapplies it");
        receipt
    }

    #[test]
    fn every_alignment_uses_the_union_of_the_selection() {
        let mut f = fixture();
        let ids = [f.a.clone(), f.b.clone(), f.c.clone()];
        let kinds = [
            (AlignKind::Left, f.b.clone(), (0.0, 40.0)),
            (AlignKind::Right, f.b.clone(), (450.0, 40.0)),
            (AlignKind::HCenter, f.b.clone(), (225.0, 40.0)),
            (AlignKind::Top, f.b.clone(), (200.0, 0.0)),
            (AlignKind::Bottom, f.b.clone(), (200.0, 150.0)),
            (AlignKind::VMiddle, f.b.clone(), (200.0, 75.0)),
        ];
        for (kind, probe, (x, y)) in kinds {
            let ops = align(f.s.document(), &ids, kind).unwrap();
            undoable(&mut f.s, ops);
            let r = bounds(&f.s, &probe);
            assert_eq!((r.x, r.y), (x, y), "{kind:?}");
            f.s.undo(Actor::User).unwrap();
        }
    }

    #[test]
    fn aligning_inside_a_group_works_in_page_space() {
        let mut f = fixture();
        let ids = [f.a.clone(), f.d.clone()];
        let ops = align(f.s.document(), &ids, AlignKind::Left).unwrap();
        run(&mut f.s, ops);
        assert_eq!(bounds(&f.s, &f.d).x, 0.0);
        assert_eq!(f.s.document().node(&f.d).unwrap().x, -300.0);
        assert_eq!(bounds(&f.s, &f.d).y, 310.0);
    }

    #[test]
    fn one_node_aligns_to_its_frame_and_a_lone_frame_does_nothing() {
        let mut f = fixture();
        let ops = align(f.s.document(), &[f.a.clone()], AlignKind::Right).unwrap();
        run(&mut f.s, ops);
        assert_eq!(bounds(&f.s, &f.a).x, 700.0);
        let ops = align(f.s.document(), &[f.a.clone()], AlignKind::Bottom).unwrap();
        run(&mut f.s, ops);
        assert_eq!(bounds(&f.s, &f.a).y, 550.0);
        let ops = align(f.s.document(), &[f.frame.clone()], AlignKind::Left).unwrap();
        assert!(ops.is_empty());
        assert!(
            align(f.s.document(), &[], AlignKind::Left)
                .unwrap()
                .is_empty()
        );
        assert!(AlignKind::parse("middle").is_err());
    }

    #[test]
    fn distributing_equalizes_gaps_and_keeps_the_ends() {
        let mut f = fixture();
        let ids = [f.c.clone(), f.a.clone(), f.b.clone()];
        let ops = distribute(f.s.document(), &ids, Axis::Horizontal).unwrap();
        undoable(&mut f.s, ops);
        let (a, b, c) = (bounds(&f.s, &f.a), bounds(&f.s, &f.b), bounds(&f.s, &f.c));
        assert_eq!((a.x, c.x), (0.0, 400.0));
        assert_eq!(b.x - a.right(), c.x - b.right());
        let ops = distribute(f.s.document(), &ids, Axis::Vertical).unwrap();
        run(&mut f.s, ops);
        assert_eq!(bounds(&f.s, &f.b).y, 50.0);
        let two = distribute(f.s.document(), &ids[..2], Axis::Horizontal);
        assert!(matches!(two, Err(OpError::Arrange(_))));
        assert!(Axis::parse("x").is_err());
    }

    #[test]
    fn stacking_lays_nodes_edge_to_edge_from_the_first() {
        let mut f = fixture();
        let ids = [f.c.clone(), f.a.clone(), f.b.clone()];
        let ops = stack(f.s.document(), &ids, Axis::Horizontal, 10.0).unwrap();
        undoable(&mut f.s, ops);
        let at = |id| {
            let r = bounds(&f.s, id);
            (r.x, r.y)
        };
        assert_eq!(at(&f.a), (0.0, 0.0));
        assert_eq!(at(&f.b), (110.0, 0.0));
        assert_eq!(at(&f.c), (170.0, 0.0));
        let ops = stack(f.s.document(), &ids, Axis::Vertical, 5.0).unwrap();
        run(&mut f.s, ops);
        assert_eq!(bounds(&f.s, &f.b).y, 55.0);
        assert_eq!(bounds(&f.s, &f.c).y, 110.0);
        assert!(stack(f.s.document(), &ids, Axis::Vertical, f64::NAN).is_err());
    }

    #[test]
    fn translating_moves_every_top_level_node_once() {
        let mut f = fixture();
        // the group and its own child are both selected: only the group moves
        let ids = [f.g.clone(), f.d.clone(), f.a.clone()];
        let ops = translate(f.s.document(), &ids, 5.0, -5.0).unwrap();
        assert_eq!(ops.len(), 2);
        undoable(&mut f.s, ops);
        assert_eq!(bounds(&f.s, &f.d).x, 315.0);
        assert_eq!(f.s.document().node(&f.d).unwrap().x, 10.0);
    }

    #[test]
    fn duplicates_are_deep_offset_new_and_sit_above_the_original() {
        let mut f = fixture();
        let before = f.s.document().node_count();
        let ids = [f.a.clone(), f.g.clone()];
        let ops = duplicate(f.s.document(), &ids).unwrap();
        let receipt = undoable(&mut f.s, ops);
        assert_eq!(f.s.document().node_count(), before + 3);
        assert_eq!(receipt.created.len(), 2);
        let kids = children(&f.s, &f.frame);
        assert_eq!(kids.len(), 6);
        assert_eq!(kids[0], f.a);
        assert_eq!(kids[4], f.g);
        let copy_of_a = f.s.document().node(&kids[1]).unwrap();
        assert_eq!(
            (copy_of_a.name.as_str(), copy_of_a.x, copy_of_a.y),
            ("a", 20.0, 20.0)
        );
        let copy_of_d = &f.s.document().node(&kids[5]).unwrap().children;
        assert_eq!(copy_of_d.len(), 1);
        assert_ne!(copy_of_d[0].id, f.d);
        f.s.document().validate().unwrap();
    }

    #[test]
    fn a_duplicated_frame_goes_beside_the_others() {
        let mut f = fixture();
        let nodes = f.s.document().node_count();
        let ops = duplicate(f.s.document(), &[f.frame.clone()]).unwrap();
        let receipt = undoable(&mut f.s, ops);
        assert_eq!(f.s.document().node_count(), nodes + 6);
        let copy = f.s.document().node(&receipt.created[0]).unwrap();
        assert_eq!((copy.x, copy.y, copy.children.len()), (1400.0, 0.0, 4));
        assert_eq!(children(&f.s, &f.root).len(), 3);
    }

    #[test]
    fn grouping_wraps_same_parent_nodes_in_place() {
        let mut f = fixture();
        let ids = [f.c.clone(), f.a.clone()];
        let (a, c) = (bounds(&f.s, &f.a), bounds(&f.s, &f.c));
        let ops = group(f.s.document(), &ids).unwrap();
        let receipt = undoable(&mut f.s, ops);
        let grp = receipt.created[0].clone();
        assert_eq!(
            children(&f.s, &f.frame),
            vec![f.b.clone(), grp.clone(), f.g.clone()]
        );
        assert_eq!(children(&f.s, &grp), vec![f.a.clone(), f.c.clone()]);
        assert_eq!((bounds(&f.s, &f.a), bounds(&f.s, &f.c)), (a, c));
        let wrapper = f.s.document().node(&grp).unwrap();
        assert_eq!(
            (wrapper.kind, wrapper.x, wrapper.y),
            (NodeKind::Group, 0.0, 0.0)
        );
    }

    #[test]
    fn groups_nest_and_keep_page_positions_at_every_level() {
        let mut f = fixture();
        let d = bounds(&f.s, &f.d);
        let inner = group(f.s.document(), &[f.d.clone()]).unwrap();
        let receipt = run(&mut f.s, inner);
        let g2 = receipt.created[0].clone();
        assert_eq!(bounds(&f.s, &f.d), d);
        assert_eq!(f.s.document().node(&g2).unwrap().x, 10.0);
        let outer = group(f.s.document(), &[f.g.clone(), f.b.clone()]).unwrap();
        run(&mut f.s, outer);
        assert_eq!(bounds(&f.s, &f.d), d);
        f.s.document().validate().unwrap();
    }

    #[test]
    fn grouping_refuses_nodes_from_different_parents() {
        let f = fixture();
        let ids = [f.b.clone(), f.d.clone()];
        assert!(matches!(
            group(f.s.document(), &ids),
            Err(OpError::Arrange(_))
        ));
        assert!(!can_group(f.s.document(), &ids));
        assert!(can_group(f.s.document(), &[f.a.clone(), f.b.clone()]));
        assert!(can_group(f.s.document(), std::slice::from_ref(&f.a)));
        assert!(!can_group(f.s.document(), &[]));
        assert!(group(f.s.document(), &[]).is_err());
    }

    #[test]
    fn ungrouping_returns_children_to_the_parent_where_they_were() {
        let mut f = fixture();
        let d = bounds(&f.s, &f.d);
        let (ops, freed) = ungroup(f.s.document(), &[f.g.clone()]).unwrap();
        undoable(&mut f.s, ops);
        assert_eq!(freed, vec![f.d.clone()]);
        assert_eq!(
            children(&f.s, &f.frame),
            vec![f.a.clone(), f.b.clone(), f.c.clone(), f.d.clone()]
        );
        assert_eq!(bounds(&f.s, &f.d), d);
        assert!(f.s.document().node(&f.g).is_none());
        assert!(ungroup(f.s.document(), &[f.a.clone()]).is_err());
        assert!(can_ungroup(f.s.document(), &[f.g.clone()]) || f.s.document().node(&f.g).is_none());
    }

    #[test]
    fn group_then_ungroup_round_trips_positions() {
        let mut f = fixture();
        let before = f.s.document().node(&f.frame).unwrap().clone();
        let ids = [f.a.clone(), f.b.clone()];
        let ops = group(f.s.document(), &ids).unwrap();
        let receipt = run(&mut f.s, ops);
        let (ops, _) = ungroup(f.s.document(), &receipt.created).unwrap();
        run(&mut f.s, ops);
        assert_eq!(f.s.document().node(&f.frame).unwrap(), &before);
    }

    #[test]
    fn the_whole_selection_moves_through_the_stacking_order() {
        let mut f = fixture();
        let ids = [f.a.clone(), f.c.clone()];
        let (a, b, c, g) = (f.a.clone(), f.b.clone(), f.c.clone(), f.g.clone());
        let ops = bring(f.s.document(), &ids, 1);
        undoable(&mut f.s, ops);
        assert_eq!(
            children(&f.s, &f.frame),
            vec![b.clone(), a.clone(), g.clone(), c.clone()]
        );
        let ops = bring(f.s.document(), &ids, i32::MAX);
        run(&mut f.s, ops);
        assert_eq!(
            children(&f.s, &f.frame),
            vec![b.clone(), g.clone(), a.clone(), c.clone()]
        );
        assert!(bring(f.s.document(), &ids, 1).is_empty());
        let ops = bring(f.s.document(), &ids, i32::MIN);
        run(&mut f.s, ops);
        assert_eq!(children(&f.s, &f.frame), vec![a, c, b, g]);
    }

    #[test]
    fn a_selection_stays_in_its_own_order_and_blocks_on_itself() {
        let mut f = fixture();
        let ids = [f.c.clone(), f.g.clone()];
        assert!(bring(f.s.document(), &ids, 1).is_empty());
        let ops = bring(f.s.document(), &ids, -1);
        run(&mut f.s, ops);
        let want = vec![f.a.clone(), f.c.clone(), f.g.clone(), f.b.clone()];
        assert_eq!(children(&f.s, &f.frame), want);
    }

    #[test]
    fn copy_and_paste_keep_frame_relative_positions_with_fresh_ids() {
        let mut f = fixture();
        let clip = copy(f.s.document(), &[f.a.clone(), f.d.clone()]).unwrap();
        assert_eq!(
            (clip.nodes[0].x, clip.nodes[1].x, clip.nodes[1].y),
            (0.0, 310.0, 310.0)
        );
        let ops = paste(f.s.document(), &clip, Some(&f.other)).unwrap();
        let receipt = undoable(&mut f.s, ops);
        let b = |s: &Session, id| geometry::bounds_in_frame(s.document(), &f.other, id).unwrap();
        assert_eq!(
            (
                b(&f.s, &receipt.created[0]).x,
                b(&f.s, &receipt.created[1]).y
            ),
            (0.0, 310.0)
        );
        assert!(!receipt.created.contains(&f.a));
        let again = paste(f.s.document(), &clip, Some(&f.other)).unwrap();
        let second = run(&mut f.s, again);
        assert_eq!(b(&f.s, &second.created[0]).x, 20.0);
        assert_eq!(b(&f.s, &second.created[1]).x, 330.0);
        f.s.document().validate().unwrap();
    }

    #[test]
    fn pasting_over_the_original_offsets_and_a_frame_goes_beside() {
        let mut f = fixture();
        let clip = copy(f.s.document(), &[f.a.clone()]).unwrap();
        let ops = paste(f.s.document(), &clip, Some(&f.frame)).unwrap();
        let receipt = run(&mut f.s, ops);
        let pasted = f.s.document().node(&receipt.created[0]).unwrap();
        assert_eq!((pasted.x, pasted.y), (20.0, 20.0));
        let frames = copy(f.s.document(), &[f.frame.clone()]).unwrap();
        let ops = paste(f.s.document(), &frames, Some(&f.other)).unwrap();
        let receipt = run(&mut f.s, ops);
        let copy = f.s.document().node(&receipt.created[0]).unwrap();
        assert_eq!(copy.x, 1400.0);
        assert!(children(&f.s, &f.root).contains(&receipt.created[0]));
        assert!(paste(f.s.document(), &clip, None).is_err());
        assert!(paste(f.s.document(), &frames, None).is_ok());
    }

    #[test]
    fn the_clipboard_survives_json_and_rejects_garbage() {
        let f = fixture();
        let clip = copy(f.s.document(), std::slice::from_ref(&f.g)).unwrap();
        assert_eq!(Clip::from_json(&clip.to_json().unwrap()).unwrap(), clip);
        assert!(Clip::from_json("{\"nodes\": 4}").is_err());
        assert!(copy(f.s.document(), &[]).is_err());
    }

    #[test]
    fn a_command_over_the_operation_limit_fails_whole_and_changes_nothing() {
        let mut f = fixture();
        let mut big = frame_at(3000.0, 100.0, 100.0);
        big.children = (0..1001)
            .map(|i| rect(&format!("r{i}"), 0.0, 0.0, 1.0, 1.0))
            .collect();
        let op = Operation::Create {
            parent: f.root.clone(),
            index: None,
            node: big,
        };
        let big_id = f.s.apply(Actor::User, None, &[op]).unwrap().created[0].clone();
        let all = children(&f.s, &big_id);
        let ops = duplicate(f.s.document(), &all).unwrap();
        let before = f.s.document().pages.clone();
        let err = f.s.apply(Actor::User, None, &ops).unwrap_err();
        assert!(matches!(err, OpError::Limit(_)), "{err:?}");
        assert_eq!(f.s.document().pages, before);
    }
}
