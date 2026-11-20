//! A drawn frame kept between edits, redrawn tile by tile.
//!
//! The frame is cut into horizontal tiles of [`TILE_ROWS`] rows. Each tile is drawn on its own
//! by the same function a full draw uses, so a tile's pixels depend only on the document and the
//! tile's position: redrawing only the tiles an edit reaches gives the exact bytes a full draw
//! gives. Which tiles an edit reaches comes from the ids it touched: each node's rows (with
//! stroke and text overflow) are recorded at every draw, and an edit dirties the rows its nodes
//! covered before and cover now. When that cannot be told (a different size, scale or token set,
//! a root frame whose own look changed, a 3D device) the caller draws the frame again.
//!
//! Warnings (images that could not be drawn, text overflow) are not tracked here; the editor
//! reports them from the document, not from the drawing.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path as FsPath;

use od_core::{Document, Node, NodeId, NodeKind};
use tiny_skia::Pixmap;

use crate::{RenderError, draw_rows, frame_geometry, text};

/// How many tiles a frame is cut into. The rows of a tile follow from the frame's height: a
/// frame is walked once per tile, so few tiles keep the first draw cheap, and an edit that
/// touches one node redraws the one or two tiles it reaches.
pub const TILES: u32 = 8;
/// Rows added around every dirty span: anti-aliasing reaches a pixel past a path's bounds.
const SPAN_PAD: i32 = 2;

type Rows = (i32, i32);
const NO_ROWS: Rows = (i32::MAX, i32::MIN);

/// What [`Retained::update`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    /// The edits touched no pixel.
    Unchanged,
    /// `redrawn` of `tiles` tiles were drawn again.
    Redrawn { redrawn: usize, tiles: usize },
    /// The change cannot be told from the touched ids: draw the frame again with [`Retained::render`].
    Stale,
}

struct Item {
    id: NodeId,
    /// Rows the node and everything below it can paint.
    rows: Rows,
    /// A container's own look without its children, to tell a resize from a reorder.
    shallow: Option<Box<Node>>,
    kids: Vec<usize>,
}

struct Snapshot {
    /// The node's position in `items`, by the hash of its id (the id itself is in the item, so a
    /// hash that lands on another node is caught).
    index: HashMap<u64, usize>,
    items: Vec<Item>,
    has_device: bool,
}

pub struct Retained {
    frame: NodeId,
    scale: f32,
    pixmap: Pixmap,
    tokens: BTreeMap<String, String>,
    snapshot: Snapshot,
}

impl Retained {
    /// Draws `frame` at `scale`. `None` when the frame holds a 3D device, which is drawn as a
    /// whole and is not kept in tiles.
    pub fn render(
        doc: &Document,
        frame: &NodeId,
        scale: f32,
        project_dir: Option<&FsPath>,
    ) -> Result<Option<Retained>, RenderError> {
        let (node, (w, h)) = frame_geometry(doc, frame, scale)?;
        let snapshot = Snapshot::capture(node, scale);
        if snapshot.has_device {
            return Ok(None);
        }
        let mut pixmap = Pixmap::new(w, h).ok_or(RenderError::TooLarge {
            width: u64::from(w),
            height: u64::from(h),
        })?;
        let all = tile_tops(h);
        draw_tiles(doc, node, scale, project_dir, &mut pixmap, &all)?;
        Ok(Some(Retained {
            frame: frame.clone(),
            scale,
            pixmap,
            tokens: doc.tokens.clone(),
            snapshot,
        }))
    }

    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }

    pub fn frame(&self) -> &NodeId {
        &self.frame
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Brings the picture up to date with `doc`. `changed` must hold every node id the edits
    /// since the last draw touched, deleted nodes included.
    pub fn update(
        &mut self,
        doc: &Document,
        changed: &[NodeId],
        project_dir: Option<&FsPath>,
    ) -> Result<Refresh, RenderError> {
        let Ok((node, (w, h))) = frame_geometry(doc, &self.frame, self.scale) else {
            return Ok(Refresh::Stale);
        };
        if (w, h) != (self.pixmap.width(), self.pixmap.height()) || doc.tokens != self.tokens {
            return Ok(Refresh::Stale);
        }
        let next = Snapshot::capture(node, self.scale);
        if next.has_device {
            return Ok(Refresh::Stale);
        }
        let Some(spans) = self.snapshot.dirty_rows(&next, node, changed) else {
            return Ok(Refresh::Stale);
        };
        let tiles = tiles_of(&spans, h);
        if !tiles.is_empty() {
            draw_tiles(doc, node, self.scale, project_dir, &mut self.pixmap, &tiles)?;
        }
        self.snapshot = next;
        Ok(match tiles.len() {
            0 => Refresh::Unchanged,
            redrawn => Refresh::Redrawn {
                redrawn,
                tiles: tile_tops(h).len(),
            },
        })
    }
}

/// The rows in one tile of a frame `height` rows tall.
fn tile_rows(height: u32) -> u32 {
    height.div_ceil(TILES).max(1)
}

/// The first row of every tile of a frame `height` rows tall.
fn tile_tops(height: u32) -> Vec<u32> {
    let rows = tile_rows(height);
    (0..TILES)
        .map(|n| n * rows)
        .filter(|top| *top < height)
        .collect()
}

/// The first row of every tile the row `spans` touch, in order.
fn tiles_of(spans: &[Rows], height: u32) -> Vec<u32> {
    let last = height as i32 - 1;
    let rows = tile_rows(height);
    let mut tiles: Vec<u32> = spans
        .iter()
        .filter_map(|&(a, b)| {
            let (a, b) = (a.max(0), b.min(last));
            (a <= b).then(|| (a as u32 / rows)..=(b as u32 / rows))
        })
        .flatten()
        .map(|tile| tile * rows)
        .collect();
    tiles.sort_unstable();
    tiles.dedup();
    tiles
}

/// Draws `tiles` into `pixmap`, on as many threads as there are cores.
fn draw_tiles(
    doc: &Document,
    node: &Node,
    scale: f32,
    project_dir: Option<&FsPath>,
    pixmap: &mut Pixmap,
    tiles: &[u32],
) -> Result<(), RenderError> {
    let (w, h) = (pixmap.width(), pixmap.height());
    let devices = HashMap::new();
    let draw = |top: u32| -> Result<(u32, Vec<u8>), RenderError> {
        let rows = tile_rows(h);
        draw_rows(doc, node, scale, (w, h), project_dir, &devices, top, rows)
            .map(|(bytes, _)| (top, bytes))
    };
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(tiles.len());
    let drawn: Vec<Result<(u32, Vec<u8>), RenderError>> = if threads <= 1 {
        tiles.iter().map(|&t| draw(t)).collect()
    } else {
        std::thread::scope(|scope| {
            let draw = &draw;
            let handles: Vec<_> = (0..threads)
                .map(|lane| {
                    scope.spawn(move || {
                        tiles
                            .iter()
                            .skip(lane)
                            .step_by(threads)
                            .map(|&t| draw(t))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| {
                    h.join().unwrap_or_else(|_| {
                        vec![Err(RenderError::Encode("a render thread failed".into()))]
                    })
                })
                .collect()
        })
    };
    for part in drawn {
        let (top, bytes) = part?;
        let at = top as usize * w as usize * 4;
        pixmap.data_mut()[at..at + bytes.len()].copy_from_slice(&bytes);
    }
    Ok(())
}

impl Snapshot {
    fn capture(frame: &Node, scale: f32) -> Snapshot {
        let mut snapshot = Snapshot {
            index: HashMap::with_capacity(1024),
            items: Vec::new(),
            has_device: false,
        };
        snapshot.visit(frame, (0.0, 0.0), scale, true, false);
        snapshot
    }

    fn visit(
        &mut self,
        node: &Node,
        parent_origin: (f64, f64),
        scale: f32,
        is_root: bool,
        hidden: bool,
    ) -> usize {
        let origin = if is_root {
            (0.0, 0.0)
        } else {
            (parent_origin.0 + node.x, parent_origin.1 + node.y)
        };
        let hidden = hidden || !node.visible;
        self.has_device |= node.kind == NodeKind::Device3d;
        let at = self.items.len();
        self.index.insert(hash_of(&node.id), at);
        self.items.push(Item {
            id: node.id.clone(),
            rows: NO_ROWS,
            shallow: node
                .kind
                .is_container()
                .then(|| Box::new(painted_look(node, is_root))),
            kids: Vec::with_capacity(node.children.len()),
        });
        let mut rows = if hidden {
            NO_ROWS
        } else {
            own_rows(node, origin.1, f64::from(scale))
        };
        for child in &node.children {
            let kid = self.visit(child, origin, scale, false, hidden);
            rows = union(rows, self.items[kid].rows);
            self.items[at].kids.push(kid);
        }
        self.items[at].rows = rows;
        at
    }

    /// The item of the node with that id, or `None` when the node is not in the snapshot.
    fn item_of(&self, id: &NodeId) -> Option<usize> {
        let at = *self.index.get(&hash_of(id))?;
        (self.items[at].id == *id).then_some(at)
    }

    fn rows_of(&self, id: &NodeId) -> Rows {
        self.item_of(id).map_or(NO_ROWS, |i| self.items[i].rows)
    }

    /// The row spans an edit touching `changed` repaints, or `None` when they cannot be told.
    fn dirty_rows(&self, next: &Snapshot, frame: &Node, changed: &[NodeId]) -> Option<Vec<Rows>> {
        let touched: HashSet<&NodeId> = changed.iter().collect();
        let mut spans = Vec::new();
        for id in changed {
            let (old, new) = (self.item_of(id), next.item_of(id));
            match (old, new) {
                (Some(o), Some(n)) => {
                    let (before, after) = (&self.items[o], &next.items[n]);
                    if before.shallow != after.shallow {
                        if id == &frame.id {
                            return None;
                        }
                        spans.extend([before.rows, after.rows]);
                    } else if after.shallow.is_some() {
                        self.reordered(next, o, n, &touched, &mut spans);
                    } else {
                        spans.extend([before.rows, after.rows]);
                    }
                }
                (Some(o), None) => spans.push(self.items[o].rows),
                (None, Some(n)) => spans.push(next.items[n].rows),
                (None, None) => {}
            }
        }
        Some(merge(spans))
    }

    /// Dirties the children of a container whose own look is unchanged but whose children may
    /// have been added, removed or reordered: what lies between the common start and end of the
    /// old and new child lists, unless it differs only by touched nodes (those dirty themselves).
    fn reordered(
        &self,
        next: &Snapshot,
        old: usize,
        new: usize,
        touched: &HashSet<&NodeId>,
        spans: &mut Vec<Rows>,
    ) {
        let (old_ids, new_ids): (Vec<&NodeId>, Vec<&NodeId>) = (
            self.items[old]
                .kids
                .iter()
                .map(|&k| &self.items[k].id)
                .collect(),
            next.items[new]
                .kids
                .iter()
                .map(|&k| &next.items[k].id)
                .collect(),
        );
        let prefix = old_ids
            .iter()
            .zip(&new_ids)
            .take_while(|(a, b)| a == b)
            .count();
        let suffix = old_ids[prefix..]
            .iter()
            .rev()
            .zip(new_ids[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let old_mid = &old_ids[prefix..old_ids.len() - suffix];
        let new_mid = &new_ids[prefix..new_ids.len() - suffix];
        let untouched = |ids: &[&NodeId]| -> Vec<NodeId> {
            ids.iter()
                .filter(|id| !touched.contains(**id))
                .map(|id| (*id).clone())
                .collect()
        };
        if untouched(old_mid) == untouched(new_mid) {
            return;
        }
        spans.extend(old_mid.iter().map(|id| self.rows_of(id)));
        spans.extend(new_mid.iter().map(|id| next.rows_of(id)));
    }
}

/// A container as far as drawing it goes: no children, no name, and for the root frame no
/// position (the exported frame always sits at the origin).
fn painted_look(node: &Node, is_root: bool) -> Node {
    let mut look = Node {
        children: Vec::new(),
        name: String::new(),
        ..node.clone()
    };
    if is_root {
        (look.x, look.y) = (0.0, 0.0);
    }
    look
}

/// The rows `node` paints on its own, with `top` its absolute top in document units.
fn own_rows(node: &Node, top: f64, scale: f64) -> Rows {
    let stroke = node.stroke.as_ref().map_or(0.0, |s| s.width * scale / 2.0);
    let margin = stroke + 1.0;
    let mut bottom = top + node.height;
    let mut ink_above = 0.0;
    if node.kind == NodeKind::Text {
        if let Some((_, text_h)) = text::measure(node) {
            bottom = bottom.max(top + f64::from(text_h));
        }
        ink_above = node.font_size.unwrap_or(16.0) * scale;
    }
    (
        (top * scale - margin - ink_above).floor() as i32,
        (bottom * scale + margin).ceil() as i32,
    )
}

/// The hash of a node's id: the index is keyed by it, so the ids are not copied twice.
fn hash_of(id: &NodeId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    id.as_str().hash(&mut hasher);
    hasher.finish()
}

fn union(a: Rows, b: Rows) -> Rows {
    (a.0.min(b.0), a.1.max(b.1))
}

/// Spans padded, empty ones dropped, overlapping ones joined.
fn merge(spans: Vec<Rows>) -> Vec<Rows> {
    let mut spans: Vec<Rows> = spans
        .into_iter()
        .filter(|(a, b)| a <= b)
        .map(|(a, b)| (a.saturating_sub(SPAN_PAD), b.saturating_add(SPAN_PAD)))
        .collect();
    spans.sort_unstable();
    let mut merged: Vec<Rows> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(last) if span.0 <= last.1 => last.1 = last.1.max(span.1),
            _ => merged.push(span),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use od_core::{Actor, NewNode, Operation, Paint, Property, Session, Stroke};

    struct Scene {
        session: Session,
        frame: NodeId,
        inner: NodeId,
        shapes: Vec<NodeId>,
        text: NodeId,
    }

    fn create(s: &mut Session, parent: &NodeId, node: NewNode) -> NodeId {
        let op = Operation::Create {
            parent: parent.clone(),
            index: None,
            node,
        };
        s.apply(Actor::User, None, &[op]).unwrap().created[0].clone()
    }

    /// A tall frame of strokes, rounded shapes, a clipping inner frame and wrapping text.
    fn scene() -> Scene {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("p").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut big = NewNode::new(NodeKind::Frame);
        big.width = 400.0;
        big.height = 1200.0;
        big.fill = Some(Paint::parse("#101820").unwrap());
        let frame = create(&mut s, &root, big);
        let mut inner = NewNode::new(NodeKind::Frame);
        inner.x = 20.0;
        inner.y = 500.0;
        inner.width = 300.0;
        inner.height = 300.0;
        inner.corner_radius = Some(30.0);
        inner.fill = Some(Paint::parse("#223344").unwrap());
        let inner = create(&mut s, &frame, inner);
        let mut shapes = Vec::new();
        for i in 0..60_u32 {
            let mut n = NewNode::new(if i % 3 == 0 {
                NodeKind::Ellipse
            } else {
                NodeKind::Rect
            });
            n.x = f64::from(i % 9) * 40.0 - 10.0;
            n.y = f64::from(i) * 19.7;
            n.width = 37.0;
            n.height = 23.0;
            n.opacity = Some(0.85);
            n.fill = Some(Paint::parse(["#f2ca83", "#33aa77", "#aa3377"][i as usize % 3]).unwrap());
            if i % 4 == 0 {
                n.stroke = Some(Stroke {
                    paint: Paint::parse("#ffffff").unwrap(),
                    width: 4.0,
                });
            }
            let parent = if i % 5 == 0 { &inner } else { &frame };
            shapes.push(create(&mut s, parent, n));
        }
        let mut t = NewNode::new(NodeKind::Text);
        t.x = 30.0;
        t.y = 100.0;
        t.width = 120.0;
        t.height = 20.0;
        t.font_size = Some(30.0);
        t.text = Some("words that wrap over several rows".into());
        t.fill = Some(Paint::parse("#fcf1d9").unwrap());
        let text = create(&mut s, &frame, t);
        Scene {
            session: s,
            frame,
            inner,
            shapes,
            text,
        }
    }

    /// Applies `ops`, updates the retained picture and checks it equals a fresh draw exactly.
    fn edit_and_compare(scene: &mut Scene, retained: &mut Retained, ops: &[Operation]) -> Refresh {
        let position = scene.session.log().len() as u64;
        scene.session.apply(Actor::User, None, ops).unwrap();
        catch_up(scene, retained, position)
    }

    fn catch_up(scene: &Scene, retained: &mut Retained, position: u64) -> Refresh {
        let doc = scene.session.document();
        let changed = scene.session.touched_since(position);
        let refresh = retained.update(doc, &changed, None).unwrap();
        if refresh == Refresh::Stale {
            return refresh;
        }
        let fresh = Retained::render(doc, &scene.frame, retained.scale(), None)
            .unwrap()
            .unwrap();
        assert!(
            retained.pixmap().data() == fresh.pixmap().data(),
            "the incremental picture differs from a full draw after {refresh:?}"
        );
        refresh
    }

    fn start(scene: &Scene, scale: f32) -> Retained {
        Retained::render(scene.session.document(), &scene.frame, scale, None)
            .unwrap()
            .unwrap()
    }

    fn redrawn(refresh: Refresh) -> usize {
        match refresh {
            Refresh::Redrawn { redrawn, .. } => redrawn,
            other => panic!("expected a partial redraw, got {other:?}"),
        }
    }

    #[test]
    fn a_full_draw_matches_the_renderer_within_band_rounding() {
        let scene = scene();
        let retained = start(&scene, 1.0);
        let whole = crate::render_frame(scene.session.document(), &scene.frame, 1.0, None).unwrap();
        let worst = retained
            .pixmap()
            .data()
            .iter()
            .zip(whole.pixmap.data())
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        assert!(worst <= 64, "worst difference {worst}");
    }

    #[test]
    fn moving_a_node_redraws_only_its_tiles() {
        let mut scene = scene();
        for scale in [0.3, 1.0, 1.7] {
            let mut retained = start(&scene, scale);
            let id = scene.shapes[7].clone();
            let refresh = edit_and_compare(
                &mut scene,
                &mut retained,
                &[Operation::Move {
                    id,
                    x: 200.5,
                    y: 333.3,
                }],
            );
            let Refresh::Redrawn { redrawn, tiles } = refresh else {
                panic!("{refresh:?}");
            };
            assert!(
                redrawn < tiles,
                "{redrawn} of {tiles} tiles at scale {scale}"
            );
        }
    }

    #[test]
    fn resizing_text_edits_and_style_changes_match_a_full_draw() {
        let mut scene = scene();
        let mut retained = start(&scene, 1.0);
        let (shape, text) = (scene.shapes[3].clone(), scene.text.clone());
        let steps: Vec<Vec<Operation>> = vec![
            vec![Operation::Set {
                id: shape.clone(),
                prop: Property::Width(180.0),
            }],
            vec![Operation::Set {
                id: shape.clone(),
                prop: Property::Stroke(Some(Stroke {
                    paint: Paint::parse("#ff0000").unwrap(),
                    width: 9.0,
                })),
            }],
            vec![Operation::Set {
                id: text.clone(),
                prop: Property::Text(
                    "a much longer line of text that wraps over many many more rows than before"
                        .into(),
                ),
            }],
            vec![Operation::Set {
                id: text.clone(),
                prop: Property::FontSize(12.0),
            }],
            vec![Operation::Set {
                id: shape,
                prop: Property::Visible(false),
            }],
        ];
        for ops in steps {
            assert!(redrawn(edit_and_compare(&mut scene, &mut retained, &ops)) > 0);
        }
    }

    #[test]
    fn z_order_reparenting_creating_and_deleting_match_a_full_draw() {
        let mut scene = scene();
        let mut retained = start(&scene, 1.0);
        let (a, b, inner, frame) = (
            scene.shapes[1].clone(),
            scene.shapes[2].clone(),
            scene.inner.clone(),
            scene.frame.clone(),
        );
        let to_front = Operation::Reparent {
            id: a.clone(),
            parent: frame.clone(),
            index: None,
        };
        let to_back = Operation::Reparent {
            id: b.clone(),
            parent: frame.clone(),
            index: Some(0),
        };
        let into_inner = Operation::Reparent {
            id: a.clone(),
            parent: inner,
            index: Some(0),
        };
        let mut fresh = NewNode::new(NodeKind::Rect);
        fresh.x = 50.0;
        fresh.y = 700.0;
        fresh.width = 100.0;
        fresh.height = 60.0;
        fresh.fill = Some(Paint::parse("#ffffff").unwrap());
        let create_op = Operation::Create {
            parent: frame,
            index: Some(4),
            node: fresh,
        };
        for ops in [
            vec![to_front],
            vec![to_back],
            vec![into_inner],
            vec![create_op],
            vec![Operation::Delete { id: b }],
        ] {
            edit_and_compare(&mut scene, &mut retained, &ops);
        }
    }

    #[test]
    fn undo_redo_and_a_batch_of_edits_match_a_full_draw() {
        let mut scene = scene();
        let mut retained = start(&scene, 1.0);
        let position = scene.session.log().len() as u64;
        let ops: Vec<Operation> = (0..8)
            .map(|i| Operation::Move {
                id: scene.shapes[i * 5 + 1].clone(),
                x: 10.0 * i as f64,
                y: 80.0 * i as f64,
            })
            .collect();
        scene.session.apply(Actor::User, None, &ops).unwrap();
        catch_up(&scene, &mut retained, position);
        let position = scene.session.log().len() as u64;
        scene.session.undo(Actor::User).unwrap();
        catch_up(&scene, &mut retained, position);
        let position = scene.session.log().len() as u64;
        scene.session.redo(Actor::User).unwrap();
        catch_up(&scene, &mut retained, position);
    }

    #[test]
    fn what_cannot_be_told_from_the_touched_ids_asks_for_a_full_draw() {
        let mut scene = scene();
        let mut retained = start(&scene, 1.0);
        let frame = scene.frame.clone();
        let position = scene.session.log().len() as u64;
        scene
            .session
            .apply(
                Actor::User,
                None,
                &[Operation::Set {
                    id: frame.clone(),
                    prop: Property::Fill(Some(Paint::parse("#000000").unwrap())),
                }],
            )
            .unwrap();
        let changed = scene.session.touched_since(position);
        let doc = scene.session.document();
        assert_eq!(
            retained.update(doc, &changed, None).unwrap(),
            Refresh::Stale
        );

        let mut retained = start(&scene, 1.0);
        let position = scene.session.log().len() as u64;
        scene
            .session
            .apply(
                Actor::User,
                None,
                &[Operation::Set {
                    id: frame,
                    prop: Property::Height(900.0),
                }],
            )
            .unwrap();
        let changed = scene.session.touched_since(position);
        let doc = scene.session.document();
        assert_eq!(
            retained.update(doc, &changed, None).unwrap(),
            Refresh::Stale
        );

        let mut retained = start(&scene, 1.0);
        scene.session.set_token("brand", "#112233").unwrap();
        let doc = scene.session.document();
        assert_eq!(retained.update(doc, &[], None).unwrap(), Refresh::Stale);
    }

    #[test]
    fn no_touched_nodes_means_no_redraw_and_a_device_is_not_retained() {
        let scene = scene();
        let mut retained = start(&scene, 1.0);
        let doc = scene.session.document();
        assert_eq!(retained.update(doc, &[], None).unwrap(), Refresh::Unchanged);

        let mut with_device = scene;
        let mut device = NewNode::new(NodeKind::Device3d);
        device.width = 100.0;
        device.height = 200.0;
        let frame = with_device.frame.clone();
        create(&mut with_device.session, &frame, device);
        let doc = with_device.session.document();
        assert!(Retained::render(doc, &frame, 1.0, None).unwrap().is_none());
    }

    #[test]
    fn tiles_cover_the_spans_and_stay_inside_the_frame() {
        assert_eq!(tiles_of(&[(-50, -1)], 300), Vec::<u32>::new());
        assert_eq!(tiles_of(&[(0, 0)], 300), vec![0]);
        // 300 rows in 8 tiles of 38 rows
        assert_eq!(tiles_of(&[(30, 33), (63, 70)], 300), vec![0, 38]);
        assert_eq!(tiles_of(&[(299, 999)], 300), vec![266]);
        assert_eq!(tile_tops(300), vec![0, 38, 76, 114, 152, 190, 228, 266]);
        assert_eq!(
            merge(vec![(10, 20), (22, 30), (100, 110)]),
            vec![(8, 32), (98, 112)]
        );
    }
}
