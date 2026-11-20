//! Multi-selection and the arrange commands the canvas, the inspector and the menus call. The
//! decisions live in `od_core::arrange` and `od_core::geometry`; this applies them as user edits
//! (one command, one transaction, one undo step) and keeps the selection pointing at the result.

use od_core::Document;
use od_core::arrange::{self, AlignKind, Axis, Clip};
use od_core::error::OpError;

use super::*;

/// The union of the selection's bounds in the active frame, or `null`.
pub(super) fn union_bounds_json(doc: &Document, frame: Option<&NodeId>, sel: &[NodeId]) -> String {
    let union = frame.and_then(|f| {
        sel.iter()
            .filter_map(|id| geometry::bounds_in_frame(doc, f, id))
            .reduce(Rect::union)
    });
    rect_json(union).to_string()
}

/// One `{id, x, y, w, h}` per selected node that is inside the active frame.
pub(super) fn rects_json(doc: &Document, frame: Option<&NodeId>, sel: &[NodeId]) -> String {
    let rows: Vec<Value> = frame
        .map(|f| {
            sel.iter()
                .filter_map(|id| {
                    let r = geometry::bounds_in_frame(doc, f, id)?;
                    Some(json!({"id": id, "x": r.x, "y": r.y, "w": r.width, "h": r.height}))
                })
                .collect()
        })
        .unwrap_or_default();
    Value::Array(rows).to_string()
}

fn parse_ids(json: &str) -> Res<Vec<NodeId>> {
    let names: Vec<String> = serde_json::from_str(json).map_err(|e| format!("ids: {e}"))?;
    names.iter().map(|n| parse_id(n)).collect()
}

fn op_err(e: OpError) -> String {
    e.to_string()
}

impl EditorState {
    pub fn select_many(&mut self, ids: &str) -> Res<()> {
        let ids = parse_ids(ids)?;
        self.with_ws(|ws| ws.select(ids).map_err(op_err))
    }

    /// Shift-click: adds the node to the selection, or removes it when it is already in it.
    pub fn toggle_select(&mut self, id: &str) -> Res<()> {
        let id = parse_id(id)?;
        self.with_ws(|ws| {
            let mut sel = ws.selection();
            match sel.iter().position(|s| s == &id) {
                Some(at) => {
                    sel.remove(at);
                }
                None => sel.push(id),
            }
            ws.select(sel).map_err(op_err)
        })
    }

    /// Selects every visible direct child of the active frame.
    pub fn select_all(&mut self) -> Res<()> {
        let Some(frame) = self.active_frame.clone() else {
            return Ok(());
        };
        self.with_ws(|ws| {
            let ids = ws
                .session
                .document()
                .node(&frame)
                .map(|f| {
                    f.children
                        .iter()
                        .filter(|c| c.visible)
                        .map(|c| c.id.clone())
                })
                .map(Iterator::collect)
                .unwrap_or_default();
            ws.select(ids).map_err(op_err)
        })
    }

    /// Marquee: selects the active frame's direct children (a group is one node) that the
    /// rectangle overlaps; `additive` keeps the current selection too.
    pub fn select_in_rect(&mut self, x: f64, y: f64, w: f64, h: f64, additive: bool) -> Res<()> {
        let Some(frame) = self.active_frame.clone() else {
            return Ok(());
        };
        self.with_ws(|ws| {
            let hit = geometry::children_in_rect(ws.session.document(), &frame, x, y, w, h);
            let mut sel = if additive { ws.selection() } else { Vec::new() };
            sel.extend(
                hit.into_iter()
                    .filter(|id| !sel.contains(id))
                    .collect::<Vec<_>>(),
            );
            ws.select(sel).map_err(op_err)
        })
    }

    /// Plans an arrangement of the selection and applies it as one edit. Nothing to do is not an
    /// error. Returns the nodes the edit created.
    fn arrange_selection(
        &mut self,
        plan: impl FnOnce(&Document, &[NodeId]) -> Result<Vec<Operation>, OpError>,
    ) -> Res<Vec<NodeId>> {
        let ops = self.with_ws(|ws| {
            let sel = ws.selection();
            plan(ws.session.document(), &sel).map_err(op_err)
        })?;
        if ops.is_empty() {
            return Ok(Vec::new());
        }
        self.apply_user(&ops)
    }

    pub fn align_selected(&mut self, kind: &str) -> Res<()> {
        let kind = AlignKind::parse(kind).map_err(op_err)?;
        self.arrange_selection(|doc, sel| arrange::align(doc, sel, kind))
            .map(|_| ())
    }

    pub fn distribute_selected(&mut self, axis: &str) -> Res<()> {
        let axis = Axis::parse(axis).map_err(op_err)?;
        self.arrange_selection(|doc, sel| arrange::distribute(doc, sel, axis))
            .map(|_| ())
    }

    /// One-shot layout of the selection in a row or a column with `gap` between nodes.
    pub fn stack_selected(&mut self, dir: &str, gap: f64) -> Res<()> {
        let axis = match dir {
            "row" => Axis::Horizontal,
            "column" => Axis::Vertical,
            other => return Err(format!("unknown direction {other:?}; use row or column")),
        };
        self.arrange_selection(|doc, sel| arrange::stack(doc, sel, axis, gap))
            .map(|_| ())
    }

    pub fn move_selected(&mut self, dx: f64, dy: f64) -> Res<()> {
        self.arrange_selection(|doc, sel| arrange::translate(doc, sel, dx, dy))
            .map(|_| ())
    }

    /// Moves the whole selection `delta` places through the stacking order.
    pub fn bring_selected(&mut self, delta: i32) -> Res<()> {
        self.arrange_selection(|doc, sel| Ok(arrange::bring(doc, sel, delta)))
            .map(|_| ())
    }

    pub fn duplicate_selected(&mut self) -> Res<()> {
        let mut created = self.arrange_selection(arrange::duplicate)?;
        created.reverse();
        self.select_created(created)
    }

    pub fn group_selected(&mut self) -> Res<()> {
        let created = self.arrange_selection(arrange::group)?;
        self.select_created(created)
    }

    pub fn ungroup_selected(&mut self) -> Res<()> {
        let (ops, freed) = self.with_ws(|ws| {
            let sel = ws.selection();
            arrange::ungroup(ws.session.document(), &sel).map_err(op_err)
        })?;
        self.apply_user(&ops)?;
        self.with_ws(|ws| ws.select(freed).map_err(op_err))
    }

    pub fn copy_selected(&mut self) -> Res<()> {
        let json = self.with_ws(|ws| {
            let sel = ws.selection();
            let clip = arrange::copy(ws.session.document(), &sel).map_err(op_err)?;
            clip.to_json().map_err(op_err)
        })?;
        self.clipboard = Some(json);
        Ok(())
    }

    pub fn paste_clipboard(&mut self) -> Res<()> {
        let Some(json) = self.clipboard.clone() else {
            return Err("nothing has been copied".to_string());
        };
        let clip = Clip::from_json(&json).map_err(op_err)?;
        let frame = self.active_frame.clone();
        let created =
            self.arrange_selection(|doc, _| arrange::paste(doc, &clip, frame.as_ref()))?;
        self.select_created(created)
    }

    /// Deletes every selected node in one edit; the open frame goes with a deleted frame.
    pub fn delete_selected(&mut self) -> Res<()> {
        let (ids, gone_frame) = self.with_ws(|ws| {
            let doc = ws.session.document();
            let ids = arrange::top_level(doc, &ws.selection());
            let frame = self.active_frame.as_ref();
            let gone = frame.is_some_and(|f| {
                ids.iter()
                    .any(|id| id == f || geometry::bounds_in_frame(doc, id, f).is_some())
            });
            Ok((ids, gone))
        })?;
        if ids.is_empty() {
            return Ok(());
        }
        let ops: Vec<Operation> = ids.into_iter().map(|id| Operation::Delete { id }).collect();
        self.apply_user(&ops)?;
        if gone_frame {
            self.active_frame = None;
        }
        Ok(())
    }

    /// Selects what an edit created, and opens it when it is a frame.
    fn select_created(&mut self, created: Vec<NodeId>) -> Res<()> {
        let first_frame = self.with_ws(|ws| {
            let doc = ws.session.document();
            Ok(created
                .iter()
                .find(|id| doc.node(id).is_some_and(|n| n.kind == NodeKind::Frame))
                .cloned())
        })?;
        if let Some(frame) = first_frame {
            self.active_frame = Some(frame);
        }
        self.with_ws(|ws| ws.select(created).map_err(op_err))
    }

    /// Snaps a box being dragged in the active frame; `exclude` is the JSON array of the ids
    /// being dragged. Returns `{x, y, guides: [{axis, pos, from, to}]}`.
    pub fn snap_move(&self, x: f64, y: f64, w: f64, h: f64, exclude: &str) -> String {
        let exclude = parse_ids(exclude).unwrap_or_default();
        let moving = Rect {
            x,
            y,
            width: w,
            height: h,
        };
        let snap = self.active_frame.as_ref().and_then(|frame| {
            self.with_ws(|ws| {
                Ok(geometry::snap_move(
                    ws.session.document(),
                    frame,
                    moving,
                    &exclude,
                ))
            })
            .ok()
        });
        match snap {
            Some(s) => serde_json::to_string(&s).unwrap_or_default(),
            None => json!({"x": x, "y": y, "guides": []}).to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> (EditorState, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let mut s = EditorState::new();
        s.new_project(&tmp.path().join("p.od"), "Store", 400.0, 300.0)
            .unwrap();
        (s, tmp)
    }

    fn ids(s: &EditorState) -> Vec<String> {
        serde_json::from_str(&s.snapshot().selection).unwrap()
    }

    /// Three rects: a (10,10,50x50), b (100,10,50x50), c (200,100,50x50).
    fn three() -> (EditorState, tempfile::TempDir, [String; 3]) {
        let (mut s, t) = state();
        let a = s.create_node("rect", 10.0, 10.0, 50.0, 50.0).unwrap();
        let b = s.create_node("rect", 100.0, 10.0, 50.0, 50.0).unwrap();
        let c = s.create_node("rect", 200.0, 100.0, 50.0, 50.0).unwrap();
        (s, t, [a, b, c])
    }

    fn x_of(s: &EditorState, id: &str) -> f64 {
        let doc_x =
            |ws: &mut Workspace| Ok(ws.session.document().node(&parse_id(id)?).map(|n| n.x));
        s.with_ws(doc_x).unwrap().unwrap()
    }

    fn children(s: &EditorState) -> Vec<String> {
        let frame = s.active_frame.clone().unwrap();
        s.with_ws(|ws| {
            Ok(ws
                .session
                .document()
                .node(&frame)
                .unwrap()
                .children
                .iter()
                .map(|c| c.id.to_string())
                .collect())
        })
        .unwrap()
    }

    #[test]
    fn selecting_many_toggling_and_the_marquee_publish_one_union() {
        let (mut s, _t, [a, b, c]) = three();
        s.select_many(&format!("[\"{a}\",\"{c}\"]")).unwrap();
        assert_eq!(ids(&s), vec![a.clone(), c.clone()]);
        let snap = s.snapshot();
        let union: Value = serde_json::from_str(&snap.selection_bounds).unwrap();
        assert_eq!(
            (
                union["x"].as_f64(),
                union["y"].as_f64(),
                union["width"].as_f64(),
                union["height"].as_f64()
            ),
            (Some(10.0), Some(10.0), Some(240.0), Some(140.0))
        );
        let rects: Vec<Value> = serde_json::from_str(&snap.selection_rects).unwrap();
        assert_eq!(rects.len(), 2);
        assert_eq!(
            (rects[1]["id"].as_str(), rects[1]["x"].as_f64()),
            (Some(c.as_str()), Some(200.0))
        );

        s.toggle_select(&b).unwrap();
        assert_eq!(ids(&s), vec![a.clone(), c.clone(), b.clone()]);
        s.toggle_select(&a).unwrap();
        assert_eq!(ids(&s), vec![c.clone(), b.clone()]);

        s.select_in_rect(0.0, 0.0, 120.0, 40.0, false).unwrap();
        assert_eq!(ids(&s), vec![a.clone(), b.clone()]);
        s.select_in_rect(190.0, 90.0, 20.0, 20.0, true).unwrap();
        assert_eq!(ids(&s), vec![a.clone(), b.clone(), c.clone()]);
        s.select_in_rect(300.0, 250.0, 10.0, 10.0, false).unwrap();
        assert!(ids(&s).is_empty());
        s.select_all().unwrap();
        assert_eq!(ids(&s), vec![a, b, c]);
    }

    #[test]
    fn the_inspector_and_a_single_selection_still_follow_the_first_node() {
        let (mut s, _t, [a, b, _c]) = three();
        s.select_many(&format!("[\"{b}\",\"{a}\"]")).unwrap();
        let insp: Value = serde_json::from_str(&s.snapshot().inspector).unwrap();
        assert_eq!(insp["id"], b.as_str());
        s.select(Some(&a)).unwrap();
        let rects: Vec<Value> = serde_json::from_str(&s.snapshot().selection_rects).unwrap();
        assert_eq!(rects.len(), 1);
        let snap = s.snapshot();
        assert!(snap.can_group && !snap.can_ungroup && !snap.can_paste);
    }

    #[test]
    fn align_distribute_stack_and_move_are_one_undo_step_each() {
        let (mut s, _t, [a, b, c]) = three();
        s.select_many(&format!("[\"{a}\",\"{b}\",\"{c}\"]"))
            .unwrap();
        s.align_selected("left").unwrap();
        assert_eq!(
            (x_of(&s, &a), x_of(&s, &b), x_of(&s, &c)),
            (10.0, 10.0, 10.0)
        );
        s.undo().unwrap();
        assert_eq!((x_of(&s, &b), x_of(&s, &c)), (100.0, 200.0));

        s.distribute_selected("h").unwrap();
        assert_eq!(x_of(&s, &b), 105.0);
        s.stack_selected("row", 10.0).unwrap();
        assert_eq!((x_of(&s, &b), x_of(&s, &c)), (70.0, 130.0));
        s.move_selected(5.0, 0.0).unwrap();
        assert_eq!(x_of(&s, &a), 15.0);
        s.undo().unwrap();
        assert_eq!(x_of(&s, &a), 10.0);
        assert!(s.align_selected("middle").is_err());
        assert!(s.stack_selected("diagonal", 1.0).is_err());
    }

    #[test]
    fn duplicate_group_ungroup_and_order_act_on_the_whole_selection() {
        let (mut s, _t, [a, b, c]) = three();
        s.select_many(&format!("[\"{a}\",\"{b}\"]")).unwrap();
        s.duplicate_selected().unwrap();
        let copies = ids(&s);
        assert_eq!(copies.len(), 2);
        assert!(copies.iter().all(|id| ![&a, &b, &c].contains(&id)));
        assert_eq!(x_of(&s, &copies[0]), 30.0);
        s.undo().unwrap();
        assert_eq!(children(&s).len(), 3);

        s.select_many(&format!("[\"{a}\",\"{b}\"]")).unwrap();
        s.group_selected().unwrap();
        let group = ids(&s);
        assert_eq!(group.len(), 1);
        assert_eq!(children(&s), vec![group[0].clone(), c.clone()]);
        assert!(s.snapshot().can_ungroup);
        s.ungroup_selected().unwrap();
        assert_eq!(ids(&s), vec![a.clone(), b.clone()]);
        assert_eq!(children(&s), vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(x_of(&s, &b), 100.0);

        s.select_many(&format!("[\"{c}\"]")).unwrap();
        s.bring_selected(-1000).unwrap();
        assert_eq!(children(&s), vec![c, a, b]);
    }

    #[test]
    fn copy_and_paste_use_the_clipboard_and_fail_clearly_without_one() {
        let (mut s, _t, [a, ..]) = three();
        assert!(s.paste_clipboard().is_err());
        s.select(Some(&a)).unwrap();
        s.copy_selected().unwrap();
        assert!(s.snapshot().can_paste);
        s.paste_clipboard().unwrap();
        let pasted = ids(&s);
        assert_eq!(pasted.len(), 1);
        assert_ne!(pasted[0], a);
        assert_eq!(x_of(&s, &pasted[0]), 30.0);
        s.undo().unwrap();
        assert_eq!(children(&s).len(), 3);
        s.select(None).unwrap();
        assert!(s.copy_selected().is_err());
    }

    #[test]
    fn deleting_removes_the_whole_selection_in_one_undo_step() {
        let (mut s, _t, [a, b, c]) = three();
        s.select_many(&format!("[\"{a}\",\"{c}\"]")).unwrap();
        s.delete_selected().unwrap();
        assert_eq!(children(&s), vec![b.clone()]);
        assert!(ids(&s).is_empty());
        s.undo().unwrap();
        assert_eq!(children(&s), vec![a, b, c]);
    }

    #[test]
    fn deleting_the_open_frame_closes_it() {
        let (mut s, _t) = state();
        let frame = s.snapshot().active_frame;
        s.select(Some(&frame)).unwrap();
        s.delete_selected().unwrap();
        assert!(s.active_frame.is_none());
    }

    #[test]
    fn snapping_reports_the_snapped_position_and_guides() {
        let (s, _t, [a, ..]) = three();
        let none = serde_json::to_string(&[&a]).unwrap();
        let out: Value = serde_json::from_str(&s.snap_move(98.0, 40.0, 50.0, 50.0, "[]")).unwrap();
        assert_eq!(out["x"], 100.0);
        assert!(
            out["guides"]
                .as_array()
                .unwrap()
                .iter()
                .any(|g| g["axis"] == "x")
        );
        let own: Value = serde_json::from_str(&s.snap_move(12.0, 12.0, 50.0, 50.0, &none)).unwrap();
        assert_eq!(own["x"], 12.0, "a dragged node does not snap to itself");
    }
}
