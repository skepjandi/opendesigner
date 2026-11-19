//! Where nodes are: bounds in page and frame coordinates, and hit testing. The editor draws
//! selection outlines and picks nodes with these, so the geometry lives in the core and the shells
//! only present it.

use serde::Serialize;

use crate::model::{Document, Node, NodeId, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    fn union(self, o: Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        let right = (self.x + self.width).max(o.x + o.width);
        let bottom = (self.y + self.height).max(o.y + o.height);
        Rect {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    fn translate(self, dx: f64, dy: f64) -> Rect {
        Rect {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }
}

/// Bounds of `node` with its position taken as `(ox + node.x, oy + node.y)`. A group has no size
/// of its own, so its bounds are the union of its visible children; an empty group has none.
fn bounds_at(node: &Node, ox: f64, oy: f64) -> Option<Rect> {
    let (x, y) = (ox + node.x, oy + node.y);
    if node.kind == NodeKind::Group {
        return node
            .children
            .iter()
            .filter(|c| c.visible)
            .filter_map(|c| bounds_at(c, x, y))
            .reduce(Rect::union);
    }
    Some(Rect {
        x,
        y,
        width: node.width,
        height: node.height,
    })
}

/// The offset of a node's parent origin in page coordinates: the sum of its ancestors' positions
/// (the page root sits at the origin).
fn parent_origin(doc: &Document, id: &NodeId) -> Option<(f64, f64)> {
    let loc = doc.locate(id)?;
    let mut node = &doc.pages.get(loc.page)?.root;
    let (mut x, mut y) = (0.0, 0.0);
    let Some((_, ancestors)) = loc.path.split_last() else {
        return Some((0.0, 0.0));
    };
    for &i in ancestors {
        x += node.x;
        y += node.y;
        node = node.children.get(i)?;
    }
    // the last ancestor is `node` now; its own offset applies to the child
    x += node.x;
    y += node.y;
    // the page root's own position is not part of the page's coordinates
    let root = &doc.pages.get(loc.page)?.root;
    Some((x - root.x, y - root.y))
}

/// Bounds of a node in its page's coordinates.
pub fn page_bounds(doc: &Document, id: &NodeId) -> Option<Rect> {
    let node = doc.node(id)?;
    let (ox, oy) = parent_origin(doc, id)?;
    bounds_at(node, ox, oy)
}

/// Bounds of `id` relative to the origin of `frame`, or `None` when `id` is not that frame or
/// inside it.
pub fn bounds_in_frame(doc: &Document, frame: &NodeId, id: &NodeId) -> Option<Rect> {
    let f = page_bounds(doc, frame)?;
    let loc_f = doc.locate(frame)?;
    let loc_n = doc.locate(id)?;
    if loc_f.page != loc_n.page || !loc_n.path.starts_with(&loc_f.path) {
        return None;
    }
    let b = page_bounds(doc, id)?;
    Some(b.translate(-f.x, -f.y))
}

fn inside(node: &Node, x: f64, y: f64) -> bool {
    if x < 0.0 || y < 0.0 || x > node.width || y > node.height {
        return false;
    }
    if node.kind == NodeKind::Ellipse && node.width > 0.0 && node.height > 0.0 {
        let (rx, ry) = (node.width / 2.0, node.height / 2.0);
        let (dx, dy) = ((x - rx) / rx, (y - ry) / ry);
        return dx * dx + dy * dy <= 1.0;
    }
    true
}

/// The topmost visible node under a point given in `node`'s parent coordinates.
fn hit(node: &Node, x: f64, y: f64) -> Option<NodeId> {
    if !node.visible {
        return None;
    }
    let (lx, ly) = (x - node.x, y - node.y);
    match node.kind {
        NodeKind::Group => node.children.iter().rev().find_map(|c| hit(c, lx, ly)),
        NodeKind::Frame => {
            if !inside(node, lx, ly) {
                return None;
            }
            node.children
                .iter()
                .rev()
                .find_map(|c| hit(c, lx, ly))
                .or_else(|| Some(node.id.clone()))
        }
        _ => inside(node, lx, ly).then(|| node.id.clone()),
    }
}

/// The node under `(x, y)`, given in the frame's own coordinates (0,0 is its top-left corner):
/// the deepest topmost visible node, the frame itself when nothing inside it is hit, `None`
/// outside the frame.
pub fn hit_test(doc: &Document, frame: &NodeId, x: f64, y: f64) -> Option<NodeId> {
    let node = doc.node(frame)?;
    if node.kind != NodeKind::Frame || !inside(node, x, y) {
        return None;
    }
    node.children
        .iter()
        .rev()
        .find_map(|c| hit(c, x, y))
        .or_else(|| Some(node.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, NewNode, Operation, Session};

    fn build() -> (Session, NodeId, NodeId, NodeId, NodeId) {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut frame = NewNode::new(NodeKind::Frame);
        frame.x = 500.0;
        frame.y = 300.0;
        frame.width = 400.0;
        frame.height = 300.0;
        let mut back = NewNode::new(NodeKind::Rect);
        back.name = Some("back".into());
        back.x = 10.0;
        back.y = 10.0;
        back.width = 200.0;
        back.height = 200.0;
        let mut group = NewNode::new(NodeKind::Group);
        group.x = 100.0;
        group.y = 50.0;
        let mut front = NewNode::new(NodeKind::Ellipse);
        front.name = Some("front".into());
        front.x = 20.0;
        front.y = 20.0;
        front.width = 100.0;
        front.height = 100.0;
        group.children.push(front);
        frame.children.push(back);
        frame.children.push(group);
        let r = s
            .apply(
                Actor::User,
                None,
                &[Operation::Create {
                    parent: root,
                    index: None,
                    node: frame,
                }],
            )
            .unwrap();
        let frame_id = r.created[0].clone();
        let f = s.document().node(&frame_id).unwrap();
        let back_id = f.children[0].id.clone();
        let group_id = f.children[1].id.clone();
        let front_id = f.children[1].children[0].id.clone();
        (s, frame_id, back_id, group_id, front_id)
    }

    #[test]
    fn bounds_accumulate_ancestor_offsets() {
        let (s, frame, back, group, front) = build();
        let d = s.document();
        assert_eq!(
            page_bounds(d, &frame).unwrap(),
            Rect {
                x: 500.0,
                y: 300.0,
                width: 400.0,
                height: 300.0
            }
        );
        // group at (100,50) in the frame, ellipse at (20,20) in the group
        assert_eq!(
            page_bounds(d, &front).unwrap(),
            Rect {
                x: 620.0,
                y: 370.0,
                width: 100.0,
                height: 100.0
            }
        );
        assert_eq!(
            bounds_in_frame(d, &frame, &front).unwrap(),
            Rect {
                x: 120.0,
                y: 70.0,
                width: 100.0,
                height: 100.0
            }
        );
        assert_eq!(
            bounds_in_frame(d, &frame, &group).unwrap(),
            bounds_in_frame(d, &frame, &front).unwrap(),
            "a group's bounds are its children's union"
        );
        assert_eq!(
            bounds_in_frame(d, &frame, &back).unwrap(),
            Rect {
                x: 10.0,
                y: 10.0,
                width: 200.0,
                height: 200.0
            }
        );
        assert!(bounds_in_frame(d, &back, &frame).is_none());
    }

    #[test]
    fn hit_testing_picks_the_topmost_deepest_node() {
        let (s, frame, back, _group, front) = build();
        let d = s.document();
        // inside both the ellipse (center 170,120) and the rect behind it: the ellipse wins
        assert_eq!(hit_test(d, &frame, 170.0, 120.0), Some(front.clone()));
        // inside the rect only
        assert_eq!(hit_test(d, &frame, 20.0, 20.0), Some(back));
        // the ellipse's bounding-box corner is outside the ellipse, so the rect behind is hit
        let corner = hit_test(d, &frame, 121.0, 71.0);
        assert_ne!(corner, Some(front));
        // empty frame area is the frame; outside the frame is nothing
        assert_eq!(hit_test(d, &frame, 380.0, 280.0), Some(frame.clone()));
        assert_eq!(hit_test(d, &frame, -1.0, 5.0), None);
        assert_eq!(hit_test(d, &frame, 401.0, 5.0), None);
    }

    #[test]
    fn hidden_nodes_are_not_hit() {
        let (mut s, frame, back, _group, front) = build();
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: front,
                prop: crate::Property::Visible(false),
            }],
        )
        .unwrap();
        assert_eq!(hit_test(s.document(), &frame, 170.0, 120.0), Some(back));
    }
}
