//! Flat, serializable views of a document for panels: the layers list and the frames a user can
//! open. Shells present these; they do not walk the tree themselves.

use serde::Serialize;

use crate::model::{Document, Node, NodeKind};

/// One row of the layers panel. Pages and nodes share the list; a page row has kind `page`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LayerRow {
    pub id: String,
    pub kind: &'static str,
    pub name: String,
    pub depth: usize,
    pub visible: bool,
    pub page: String,
    pub has_children: bool,
}

/// The layers in panel order: pages in order, each node before its children, the topmost layer
/// (last child) first, as design tools list them.
pub fn layers(doc: &Document) -> Vec<LayerRow> {
    let mut rows = Vec::new();
    for page in &doc.pages {
        rows.push(LayerRow {
            id: page.id.to_string(),
            kind: "page",
            name: page.name.clone(),
            depth: 0,
            visible: true,
            page: page.id.to_string(),
            has_children: !page.root.children.is_empty(),
        });
        for child in page.root.children.iter().rev() {
            push_node(child, 1, &page.id.to_string(), &mut rows);
        }
    }
    rows
}

fn push_node(node: &Node, depth: usize, page: &str, rows: &mut Vec<LayerRow>) {
    rows.push(LayerRow {
        id: node.id.to_string(),
        kind: node.kind.as_str(),
        name: node.name.clone(),
        depth,
        visible: node.visible,
        page: page.to_string(),
        has_children: !node.children.is_empty(),
    });
    for child in node.children.iter().rev() {
        push_node(child, depth + 1, page, rows);
    }
}

/// A frame that can be opened on the canvas and exported.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FrameRow {
    pub id: String,
    pub name: String,
    pub page: String,
    pub width: f64,
    pub height: f64,
}

/// Every frame in the document, pages in order, frames in drawing order.
pub fn frames(doc: &Document) -> Vec<FrameRow> {
    fn walk(node: &Node, page: &str, out: &mut Vec<FrameRow>) {
        if node.kind == NodeKind::Frame {
            out.push(FrameRow {
                id: node.id.to_string(),
                name: node.name.clone(),
                page: page.to_string(),
                width: node.width,
                height: node.height,
            });
        }
        for c in &node.children {
            walk(c, page, out);
        }
    }
    let mut out = Vec::new();
    for page in &doc.pages {
        walk(&page.root, &page.id.to_string(), &mut out);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Actor, NewNode, Operation, Session};

    #[test]
    fn layers_list_pages_then_nodes_topmost_first_with_depth() {
        let mut s = Session::new(Document::new("t"));
        let page = s.add_page("Home").unwrap();
        let root = s.document().page(&page).unwrap().root.id.clone();
        let mut frame = NewNode::new(NodeKind::Frame);
        frame.name = Some("Frame".into());
        frame.width = 10.0;
        frame.height = 10.0;
        for name in ["bottom", "top"] {
            let mut r = NewNode::new(NodeKind::Rect);
            r.name = Some(name.into());
            frame.children.push(r);
        }
        s.apply(
            Actor::User,
            None,
            &[Operation::Create {
                parent: root,
                index: None,
                node: frame,
            }],
        )
        .unwrap();
        let rows = layers(s.document());
        let shape: Vec<(&str, usize)> = rows.iter().map(|r| (r.name.as_str(), r.depth)).collect();
        assert_eq!(
            shape,
            vec![("Home", 0), ("Frame", 1), ("top", 2), ("bottom", 2)]
        );
        assert_eq!(rows[0].kind, "page");
        assert!(rows[1].has_children && !rows[2].has_children);
        let f = frames(s.document());
        assert_eq!(f.len(), 1);
        assert_eq!((f[0].name.as_str(), f[0].width), ("Frame", 10.0));
    }
}
