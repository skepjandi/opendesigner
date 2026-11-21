//! Where nodes are: bounds in page and frame coordinates, and hit testing. The editor draws
//! selection outlines and picks nodes with these, so the geometry lives in the core and the shells
//! only present it.

use serde::Serialize;

use crate::model::{Document, Node, NodeId, NodeKind};
use crate::transform::Affine;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn union(self, o: Rect) -> Rect {
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

    pub fn translate(self, dx: f64, dy: f64) -> Rect {
        Rect {
            x: self.x + dx,
            y: self.y + dy,
            ..self
        }
    }
}

/// Bounds of `node` where `parent` maps its parent's coordinates (an upright rectangle around
/// the node's box after its rotation and scale). A group that is not a clip has no size of its
/// own, so its bounds are the union of its visible children; an empty group has none.
fn bounds_at(node: &Node, parent: Affine) -> Option<Rect> {
    let here = parent.after(node.local_affine());
    if node.kind == NodeKind::Group && !(node.clip && node.layout_box().is_some()) {
        return node
            .children
            .iter()
            .filter(|c| c.visible)
            .filter_map(|c| bounds_at(c, here))
            .reduce(Rect::union);
    }
    let own = node.layout_box()?;
    Some(here.bounds_of(Rect {
        x: 0.0,
        y: 0.0,
        ..own
    }))
}

/// Where the coordinates of the node at `path` below `from` sit in `from`'s: the product of the
/// local maps of the nodes between them, `from` itself and the node excluded.
fn affine_below(from: &Node, path: &[usize]) -> Option<Affine> {
    let mut node = from;
    let mut affine = Affine::IDENTITY;
    for &i in path.split_last().map_or(&[][..], |(_, above)| above) {
        node = node.children.get(i)?;
        affine = affine.after(node.local_affine());
    }
    Some(affine)
}

/// The offset of a node's parent origin in page coordinates: the sum of its ancestors' positions
/// (the page root sits at the origin).
pub(crate) fn parent_origin(doc: &Document, id: &NodeId) -> Option<(f64, f64)> {
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
    let loc = doc.locate(id)?;
    let root = &doc.pages.get(loc.page)?.root;
    bounds_at(node, affine_below(root, &loc.path)?)
}

/// Bounds of `id` relative to the origin of `frame`, or `None` when `id` is not that frame or
/// inside it.
pub fn bounds_in_frame(doc: &Document, frame: &NodeId, id: &NodeId) -> Option<Rect> {
    let frame_node = doc.node(frame)?;
    let loc_f = doc.locate(frame)?;
    let loc_n = doc.locate(id)?;
    if loc_f.page != loc_n.page || !loc_n.path.starts_with(&loc_f.path) {
        return None;
    }
    if frame == id {
        return Some(Rect {
            x: 0.0,
            y: 0.0,
            width: frame_node.width,
            height: frame_node.height,
        });
    }
    let node = doc.node(id)?;
    let below = loc_n.path.get(loc_f.path.len()..)?;
    bounds_at(node, affine_below(frame_node, below)?)
}

fn inside(node: &Node, x: f64, y: f64) -> bool {
    if node.kind == NodeKind::Path {
        return crate::vector::hits(node, x, y);
    }
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

/// A point given in `node`'s parent coordinates, in the node's own (before its rotation and
/// scale); `None` when the node is scaled flat.
fn to_local(node: &Node, x: f64, y: f64) -> Option<(f64, f64)> {
    if !node.is_transformed() {
        return Some((x - node.x, y - node.y));
    }
    node.local_affine().inverse().map(|i| i.apply(x, y))
}

/// The topmost visible node under a point given in `node`'s parent coordinates.
fn hit(node: &Node, x: f64, y: f64) -> Option<NodeId> {
    if !node.visible {
        return None;
    }
    let (lx, ly) = to_local(node, x, y)?;
    match node.kind {
        NodeKind::Group if node.clip && node.layout_box().is_some() => {
            if !inside(node, lx, ly) {
                return None;
            }
            node.children.iter().rev().find_map(|c| hit(c, lx, ly))
        }
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
/// How close (document pixels) a moving box must come to an edge or center to snap to it.
pub const SNAP_DISTANCE: f64 = 6.0;

impl Rect {
    pub fn right(self) -> f64 {
        self.x + self.width
    }

    pub fn bottom(self) -> f64 {
        self.y + self.height
    }

    /// True when the two rectangles share area (touching edges do not count).
    pub fn overlaps(self, o: Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

/// The direct children of `frame` that a marquee `(x, y, w, h)` (frame coordinates, any corner
/// first) overlaps, in stacking order. Hidden nodes and groups without visible content are
/// skipped; a group counts as one node. A rectangle with no area selects nothing.
pub fn children_in_rect(
    doc: &Document,
    frame: &NodeId,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
) -> Vec<NodeId> {
    let rect = Rect {
        x: x.min(x + w),
        y: y.min(y + h),
        width: w.abs(),
        height: h.abs(),
    };
    if rect.width == 0.0 || rect.height == 0.0 {
        return Vec::new();
    }
    let Some(node) = doc.node(frame) else {
        return Vec::new();
    };
    node.children
        .iter()
        .filter(|c| c.visible)
        .filter(|c| bounds_in_frame(doc, frame, &c.id).is_some_and(|b| b.overlaps(rect)))
        .map(|c| c.id.clone())
        .collect()
}

/// A smart guide: a line at `pos` on `axis` ("x" is a vertical line at that x) from `from` to
/// `to` along the other axis.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Guide {
    pub axis: &'static str,
    pub pos: f64,
    pub from: f64,
    pub to: f64,
}

/// A moving box after snapping, with the guides to draw.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snap {
    pub x: f64,
    pub y: f64,
    pub guides: Vec<Guide>,
}

/// The edges and center of `r` along one axis.
fn stops(start: f64, len: f64) -> [f64; 3] {
    [start, start + len / 2.0, start + len]
}

/// The shift (within `SNAP_DISTANCE`) that brings one of `moving`'s stops onto one of the
/// candidates; the smallest shift wins and the first candidate wins a tie.
fn best_shift(moving: [f64; 3], candidates: &[f64]) -> f64 {
    let mut best: Option<f64> = None;
    for &c in candidates {
        for m in moving {
            let d = c - m;
            if d.abs() <= SNAP_DISTANCE && best.is_none_or(|b| d.abs() < b.abs()) {
                best = Some(d);
            }
        }
    }
    best.unwrap_or(0.0)
}

/// Snaps a box `moving` (frame coordinates) to the edges and centers of the visible direct
/// children of `frame` that are not in `exclude`, and to the frame's own edges and center. X and
/// Y snap independently. The guides cover every candidate line the snapped box now sits on.
pub fn snap_move(doc: &Document, frame: &NodeId, moving: Rect, exclude: &[NodeId]) -> Snap {
    let Some(frame_node) = doc.node(frame) else {
        return Snap {
            x: moving.x,
            y: moving.y,
            guides: Vec::new(),
        };
    };
    let mut others = vec![Rect {
        x: 0.0,
        y: 0.0,
        width: frame_node.width,
        height: frame_node.height,
    }];
    others.extend(
        frame_node
            .children
            .iter()
            .filter(|c| c.visible && !exclude.contains(&c.id))
            .filter_map(|c| bounds_in_frame(doc, frame, &c.id)),
    );
    let xs: Vec<f64> = others.iter().flat_map(|r| stops(r.x, r.width)).collect();
    let ys: Vec<f64> = others.iter().flat_map(|r| stops(r.y, r.height)).collect();
    let snapped = Rect {
        x: moving.x + best_shift(stops(moving.x, moving.width), &xs),
        y: moving.y + best_shift(stops(moving.y, moving.height), &ys),
        ..moving
    };
    Snap {
        x: snapped.x,
        y: snapped.y,
        guides: guides_for(snapped, &others),
    }
}

fn guides_for(moving: Rect, others: &[Rect]) -> Vec<Guide> {
    const EPS: f64 = 1e-6;
    let mut guides: Vec<Guide> = Vec::new();
    let mut add = |guide: Guide| {
        let same = |g: &Guide| g.axis == guide.axis && (g.pos - guide.pos).abs() < EPS;
        match guides.iter_mut().find(|g| same(g)) {
            Some(g) => {
                g.from = g.from.min(guide.from);
                g.to = g.to.max(guide.to);
            }
            None => guides.push(guide),
        }
    };
    for o in others {
        for m in stops(moving.x, moving.width) {
            if stops(o.x, o.width).iter().any(|c| (c - m).abs() < EPS) {
                add(Guide {
                    axis: "x",
                    pos: m,
                    from: moving.y.min(o.y),
                    to: moving.bottom().max(o.bottom()),
                });
            }
        }
        for m in stops(moving.y, moving.height) {
            if stops(o.y, o.height).iter().any(|c| (c - m).abs() < EPS) {
                add(Guide {
                    axis: "y",
                    pos: m,
                    from: moving.x.min(o.x),
                    to: moving.right().max(o.right()),
                });
            }
        }
    }
    guides
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

    fn moving(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn a_moving_box_snaps_to_sibling_edges_and_reports_the_guides() {
        let (s, frame, ..) = build();
        // left edge 8 is 2 from the rect's left (10); top 213 is 3 from the rect's bottom (210)
        let snap = snap_move(s.document(), &frame, moving(8.0, 213.0, 100.0, 50.0), &[]);
        assert_eq!((snap.x, snap.y), (10.0, 210.0));
        let x_line = snap.guides.iter().find(|g| g.axis == "x" && g.pos == 10.0);
        let x_line = x_line.expect("a vertical guide at the shared left edge");
        assert_eq!((x_line.from, x_line.to), (10.0, 260.0));
        assert!(snap.guides.iter().any(|g| g.axis == "y" && g.pos == 210.0));
    }

    #[test]
    fn excluded_nodes_are_not_snap_targets() {
        let (s, frame, back, ..) = build();
        let snap = snap_move(
            s.document(),
            &frame,
            moving(8.0, 213.0, 100.0, 50.0),
            &[back],
        );
        assert_eq!((snap.x, snap.y), (8.0, 213.0));
        assert!(snap.guides.is_empty());
    }

    #[test]
    fn the_frame_edges_and_center_attract_and_the_axes_are_independent() {
        let (s, frame, ..) = build();
        // x: centered on the frame (center 200 vs box center 197); y: far from everything
        let snap = snap_move(s.document(), &frame, moving(147.0, 260.0, 100.0, 20.0), &[]);
        assert_eq!(snap.x, 150.0);
        assert_eq!(snap.y, 260.0);
        assert!(snap.guides.iter().all(|g| g.axis == "x"));
    }

    #[test]
    fn equally_near_candidates_resolve_to_the_first_and_far_boxes_stay_put() {
        let (s, frame, ..) = build();
        // left 5 is 5 from the frame's left (0) and 5 from the rect's left (10): the frame wins
        let tie = snap_move(s.document(), &frame, moving(5.0, 280.0, 0.0, 0.0), &[]);
        assert_eq!(tie.x, 0.0);
        let far = snap_move(s.document(), &frame, moving(33.0, 255.0, 10.0, 10.0), &[]);
        assert_eq!((far.x, far.y), (33.0, 255.0));
        assert!(far.guides.is_empty());
        let missing = NodeId::parse("n_999").unwrap();
        let gone = snap_move(s.document(), &missing, moving(1.0, 2.0, 3.0, 4.0), &[]);
        assert_eq!((gone.x, gone.y, gone.guides.len()), (1.0, 2.0, 0));
    }

    #[test]
    fn a_marquee_picks_direct_children_it_overlaps() {
        let (mut s, frame, back, group, _front) = build();
        let d = s.document();
        // over the group's ellipse only (120..220, 70..170) and the rect (10..210)
        assert_eq!(
            children_in_rect(d, &frame, 150.0, 100.0, 10.0, 10.0),
            vec![back.clone(), group.clone()]
        );
        // dragged up-left gives the same rectangle
        assert_eq!(
            children_in_rect(d, &frame, 160.0, 110.0, -10.0, -10.0),
            vec![back.clone(), group.clone()]
        );
        // only the group is at x 215..300
        assert_eq!(
            children_in_rect(d, &frame, 215.0, 100.0, 80.0, 10.0),
            vec![group.clone()]
        );
        assert!(children_in_rect(d, &frame, 150.0, 100.0, 0.0, 10.0).is_empty());
        s.apply(
            Actor::User,
            None,
            &[Operation::Set {
                id: group,
                prop: crate::Property::Visible(false),
            }],
        )
        .unwrap();
        assert_eq!(
            children_in_rect(s.document(), &frame, 150.0, 100.0, 10.0, 10.0),
            vec![back]
        );
    }
}
