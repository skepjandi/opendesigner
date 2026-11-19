//! Operations: the only way a document changes. Each is validated, applied, and returns the
//! resolved forward operation plus its inverse so history can undo, redo and restore.

use std::collections::BTreeMap;
use std::mem::replace;

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::{self, finite_in, text_within};
use crate::model::{Document, Node, NodeId, NodeKind, Stroke, TextAlign, validate_node_fields};
use crate::paint::Paint;

/// One settable property with its new value. Serialized as `{"name": "x", "value": 10}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum Property {
    Name(String),
    X(f64),
    Y(f64),
    Width(f64),
    Height(f64),
    Visible(bool),
    Opacity(f64),
    Fill(Option<Paint>),
    Stroke(Option<Stroke>),
    CornerRadius(f64),
    Text(String),
    FontSize(f64),
    FontFamily(Option<String>),
    FontWeight(Option<f64>),
    TextAlign(Option<TextAlign>),
    LineHeight(Option<f64>),
    Image(Option<String>),
}

impl Property {
    pub fn name(&self) -> &'static str {
        match self {
            Property::Name(_) => "name",
            Property::X(_) => "x",
            Property::Y(_) => "y",
            Property::Width(_) => "width",
            Property::Height(_) => "height",
            Property::Visible(_) => "visible",
            Property::Opacity(_) => "opacity",
            Property::Fill(_) => "fill",
            Property::Stroke(_) => "stroke",
            Property::CornerRadius(_) => "corner_radius",
            Property::Text(_) => "text",
            Property::FontSize(_) => "font_size",
            Property::FontFamily(_) => "font_family",
            Property::FontWeight(_) => "font_weight",
            Property::TextAlign(_) => "text_align",
            Property::LineHeight(_) => "line_height",
            Property::Image(_) => "image",
        }
    }

    fn applies_to(&self, kind: NodeKind) -> bool {
        use NodeKind::*;
        match self {
            Property::Name(_)
            | Property::X(_)
            | Property::Y(_)
            | Property::Visible(_)
            | Property::Opacity(_) => true,
            Property::Width(_) | Property::Height(_) => kind != Group,
            Property::Fill(_) => matches!(kind, Frame | Rect | Ellipse | Text),
            Property::Stroke(_) => matches!(kind, Frame | Rect | Ellipse),
            Property::CornerRadius(_) => matches!(kind, Frame | Rect),
            Property::Text(_)
            | Property::FontSize(_)
            | Property::FontFamily(_)
            | Property::FontWeight(_)
            | Property::TextAlign(_)
            | Property::LineHeight(_) => kind == Text,
            Property::Image(_) => kind == Image,
        }
    }
}

/// Sets `prop` on `node` and returns the previous value as a property that undoes it.
fn apply_property(
    node: &mut Node,
    prop: Property,
    tokens: &BTreeMap<String, String>,
) -> Result<Property, OpError> {
    if !prop.applies_to(node.kind) {
        return Err(OpError::NotApplicable {
            property: prop.name(),
            kind: node.kind.as_str(),
        });
    }
    let coord = |what, v| finite_in(what, v, -limits::MAX_COORD, limits::MAX_COORD);
    let size = |what, v| finite_in(what, v, 0.0, limits::MAX_SIZE);
    Ok(match prop {
        Property::Name(v) => {
            text_within("name", &v, limits::MAX_NAME_LEN)?;
            Property::Name(replace(&mut node.name, v))
        }
        Property::X(v) => Property::X(replace(&mut node.x, coord("x", v)?)),
        Property::Y(v) => Property::Y(replace(&mut node.y, coord("y", v)?)),
        Property::Width(v) => Property::Width(replace(&mut node.width, size("width", v)?)),
        Property::Height(v) => Property::Height(replace(&mut node.height, size("height", v)?)),
        Property::Visible(v) => Property::Visible(replace(&mut node.visible, v)),
        Property::Opacity(v) => {
            let v = finite_in("opacity", v, 0.0, 1.0)?;
            Property::Opacity(replace(&mut node.opacity, v))
        }
        Property::Fill(v) => {
            if let Some(p) = &v {
                p.resolve(tokens)?;
            }
            Property::Fill(replace(&mut node.fill, v))
        }
        Property::Stroke(v) => {
            let v = match v {
                Some(s) => {
                    s.paint.resolve(tokens)?;
                    let width = finite_in("stroke width", s.width, 0.0, limits::MAX_STROKE_WIDTH)?;
                    Some(Stroke {
                        paint: s.paint,
                        width,
                    })
                }
                None => None,
            };
            Property::Stroke(replace(&mut node.stroke, v))
        }
        Property::CornerRadius(v) => {
            let v = size("corner_radius", v)?;
            Property::CornerRadius(replace(&mut node.corner_radius, v))
        }
        Property::Text(v) => {
            text_within("text", &v, limits::MAX_TEXT_LEN)?;
            Property::Text(node.text.replace(v).unwrap_or_default())
        }
        Property::FontSize(v) => {
            let v = finite_in("font_size", v, 1.0, limits::MAX_FONT_SIZE)?;
            Property::FontSize(node.font_size.replace(v).unwrap_or(16.0))
        }
        Property::FontFamily(v) => {
            if let Some(f) = &v {
                limits::family_name(f)?;
            }
            Property::FontFamily(replace(&mut node.font_family, v))
        }
        Property::FontWeight(v) => {
            let v = v
                .map(|w| {
                    finite_in(
                        "font_weight",
                        w,
                        limits::MIN_FONT_WEIGHT,
                        limits::MAX_FONT_WEIGHT,
                    )
                })
                .transpose()?;
            Property::FontWeight(replace(&mut node.font_weight, v))
        }
        Property::TextAlign(v) => Property::TextAlign(replace(&mut node.text_align, v)),
        Property::LineHeight(v) => {
            let v = v
                .map(|h| {
                    finite_in(
                        "line_height",
                        h,
                        limits::MIN_LINE_HEIGHT,
                        limits::MAX_LINE_HEIGHT,
                    )
                })
                .transpose()?;
            Property::LineHeight(replace(&mut node.line_height, v))
        }
        Property::Image(v) => {
            if let Some(p) = &v {
                text_within("image path", p, limits::MAX_PATH_LEN)?;
            }
            Property::Image(replace(&mut node.image, v))
        }
    })
}

/// A node to create. Ids are minted by the document, never supplied by the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewNode {
    pub kind: NodeKind,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub opacity: Option<f64>,
    #[serde(default)]
    pub fill: Option<Paint>,
    #[serde(default)]
    pub stroke: Option<Stroke>,
    #[serde(default)]
    pub corner_radius: Option<f64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub font_size: Option<f64>,
    #[serde(default)]
    pub font_family: Option<String>,
    #[serde(default)]
    pub font_weight: Option<f64>,
    #[serde(default)]
    pub text_align: Option<TextAlign>,
    #[serde(default)]
    pub line_height: Option<f64>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub children: Vec<NewNode>,
}

impl NewNode {
    pub fn new(kind: NodeKind) -> NewNode {
        NewNode {
            kind,
            name: None,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            visible: None,
            opacity: None,
            fill: None,
            stroke: None,
            corner_radius: None,
            text: None,
            font_size: None,
            font_family: None,
            font_weight: None,
            text_align: None,
            line_height: None,
            image: None,
            children: Vec::new(),
        }
    }

    fn count(&self) -> usize {
        1 + self.children.iter().map(NewNode::count).sum::<usize>()
    }

    fn build(
        &self,
        next: &mut u64,
        tokens: &BTreeMap<String, String>,
        depth: usize,
    ) -> Result<Node, OpError> {
        if depth > limits::MAX_DEPTH {
            return Err(OpError::Limit(format!(
                "nesting deeper than {}",
                limits::MAX_DEPTH
            )));
        }
        if !self.kind.is_container() && !self.children.is_empty() {
            return Err(OpError::NoChildren {
                parent_kind: self.kind.as_str(),
            });
        }
        let id = NodeId::parse(&format!("n_{next}"))?;
        *next += 1;
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| self.kind.as_str().to_string());
        let mut node = Node::blank(id, self.kind, String::new());
        let mut set = |prop: Property| apply_property(&mut node, prop, tokens).map(|_| ());
        set(Property::Name(name))?;
        set(Property::X(self.x))?;
        set(Property::Y(self.y))?;
        if self.kind != NodeKind::Group || self.width != 0.0 {
            set(Property::Width(self.width))?;
        }
        if self.kind != NodeKind::Group || self.height != 0.0 {
            set(Property::Height(self.height))?;
        }
        if let Some(v) = self.visible {
            set(Property::Visible(v))?;
        }
        if let Some(v) = self.opacity {
            set(Property::Opacity(v))?;
        }
        if self.fill.is_some() {
            set(Property::Fill(self.fill.clone()))?;
        }
        if self.stroke.is_some() {
            set(Property::Stroke(self.stroke.clone()))?;
        }
        if let Some(v) = self.corner_radius {
            set(Property::CornerRadius(v))?;
        }
        if let Some(v) = &self.text {
            set(Property::Text(v.clone()))?;
        }
        if let Some(v) = self.font_size {
            set(Property::FontSize(v))?;
        }
        if self.font_family.is_some() {
            set(Property::FontFamily(self.font_family.clone()))?;
        }
        if self.font_weight.is_some() {
            set(Property::FontWeight(self.font_weight))?;
        }
        if self.text_align.is_some() {
            set(Property::TextAlign(self.text_align))?;
        }
        if self.line_height.is_some() {
            set(Property::LineHeight(self.line_height))?;
        }
        if self.image.is_some() {
            set(Property::Image(self.image.clone()))?;
        }
        for child in &self.children {
            node.children.push(child.build(next, tokens, depth + 1)?);
        }
        Ok(node)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Operation {
    /// Add a node under `parent` at `index` (append when omitted).
    Create {
        parent: NodeId,
        #[serde(default)]
        index: Option<usize>,
        node: NewNode,
    },
    /// Set one property.
    Set { id: NodeId, prop: Property },
    /// Move a node to `x`, `y` in its parent's coordinates.
    Move { id: NodeId, x: f64, y: f64 },
    /// Move a node to another parent (append when `index` is omitted).
    Reparent {
        id: NodeId,
        parent: NodeId,
        #[serde(default)]
        index: Option<usize>,
    },
    /// Delete a node and its subtree.
    Delete { id: NodeId },
    /// Put back an existing subtree with its original ids. Written by the history when it
    /// resolves a `Create` or inverts a `Delete`; front ends cannot issue it.
    Restore {
        parent: NodeId,
        index: usize,
        node: Node,
    },
}

impl Operation {
    pub fn name(&self) -> &'static str {
        match self {
            Operation::Create { .. } => "create",
            Operation::Set { .. } => "set",
            Operation::Move { .. } => "move",
            Operation::Reparent { .. } => "reparent",
            Operation::Delete { .. } => "delete",
            Operation::Restore { .. } => "restore",
        }
    }
}

/// An applied operation: what ran (ids resolved), how to undo it, and which nodes it touched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedOp {
    pub forward: Operation,
    pub inverse: Operation,
    pub touched: Vec<NodeId>,
}

fn node_loc(doc: &Document, id: &NodeId) -> Result<crate::model::Location, OpError> {
    doc.locate(id)
        .ok_or_else(|| OpError::NoSuchNode(id.to_string()))
}

fn immutable_if_root(doc: &Document, loc: &crate::model::Location) -> Result<(), OpError> {
    if loc.path.is_empty() {
        let page = doc.pages.get(loc.page).map(|p| p.id.to_string());
        return Err(OpError::RootImmutable(page.unwrap_or_default()));
    }
    Ok(())
}

fn container_loc(doc: &Document, id: &NodeId) -> Result<crate::model::Location, OpError> {
    let loc = node_loc(doc, id)?;
    let kind = doc
        .node_at(&loc)
        .map(|n| n.kind)
        .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
    if !kind.is_container() {
        return Err(OpError::NoChildren {
            parent_kind: kind.as_str(),
        });
    }
    Ok(loc)
}

fn check_insert_index(parent: &NodeId, index: usize, len: usize) -> Result<(), OpError> {
    if index > len {
        return Err(OpError::BadIndex {
            parent: parent.to_string(),
            index,
            len,
        });
    }
    Ok(())
}

fn check_count(doc: &Document, added: usize) -> Result<(), OpError> {
    if doc.node_count() + added > limits::MAX_NODES {
        return Err(OpError::Limit(format!(
            "at most {} nodes",
            limits::MAX_NODES
        )));
    }
    Ok(())
}

fn insert(
    doc: &mut Document,
    parent: &NodeId,
    index: Option<usize>,
    node: Node,
) -> Result<usize, OpError> {
    let loc = container_loc(doc, parent)?;
    let target = doc
        .node_at_mut(&loc)
        .ok_or_else(|| OpError::NoSuchNode(parent.to_string()))?;
    let index = index.unwrap_or(target.children.len());
    check_insert_index(parent, index, target.children.len())?;
    target.children.insert(index, node);
    Ok(index)
}

/// Validates and applies one operation. `allow_internal` lets the history issue `Restore`.
pub fn apply(
    doc: &mut Document,
    op: &Operation,
    allow_internal: bool,
) -> Result<AppliedOp, OpError> {
    match op {
        Operation::Create {
            parent,
            index,
            node,
        } => {
            let ploc = container_loc(doc, parent)?;
            check_count(doc, node.count())?;
            let mut next = doc.next_id;
            let built = node.build(&mut next, &doc.tokens, ploc.path.len() + 1)?;
            let at = insert(doc, parent, *index, built.clone())?;
            doc.next_id = next;
            let mut touched = Vec::new();
            built.ids(&mut touched);
            Ok(AppliedOp {
                inverse: Operation::Delete {
                    id: built.id.clone(),
                },
                forward: Operation::Restore {
                    parent: parent.clone(),
                    index: at,
                    node: built,
                },
                touched,
            })
        }
        Operation::Restore {
            parent,
            index,
            node,
        } => {
            if !allow_internal {
                return Err(OpError::InternalOnly("restore"));
            }
            let ploc = container_loc(doc, parent)?;
            check_count(doc, node.count())?;
            if ploc.path.len() + 1 + node.height_below() > limits::MAX_DEPTH {
                return Err(OpError::Limit(format!(
                    "nesting deeper than {}",
                    limits::MAX_DEPTH
                )));
            }
            let mut ids = Vec::new();
            node.ids(&mut ids);
            for id in &ids {
                if doc.locate(id).is_some() {
                    return Err(OpError::DuplicateId(id.to_string()));
                }
            }
            validate_subtree(node, &doc.tokens)?;
            insert(doc, parent, Some(*index), node.clone())?;
            Ok(AppliedOp {
                forward: op.clone(),
                inverse: Operation::Delete {
                    id: node.id.clone(),
                },
                touched: ids,
            })
        }
        Operation::Set { id, prop } => {
            let loc = node_loc(doc, id)?;
            immutable_if_root(doc, &loc)?;
            let tokens = doc.tokens.clone();
            let node = doc
                .node_at_mut(&loc)
                .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
            let old = apply_property(node, prop.clone(), &tokens)?;
            let resolved = current_value(node, prop);
            Ok(AppliedOp {
                forward: Operation::Set {
                    id: id.clone(),
                    prop: resolved,
                },
                inverse: Operation::Set {
                    id: id.clone(),
                    prop: old,
                },
                touched: vec![id.clone()],
            })
        }
        Operation::Move { id, x, y } => {
            let loc = node_loc(doc, id)?;
            immutable_if_root(doc, &loc)?;
            let x = finite_in("x", *x, -limits::MAX_COORD, limits::MAX_COORD)?;
            let y = finite_in("y", *y, -limits::MAX_COORD, limits::MAX_COORD)?;
            let node = doc
                .node_at_mut(&loc)
                .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
            let (old_x, old_y) = (replace(&mut node.x, x), replace(&mut node.y, y));
            Ok(AppliedOp {
                forward: Operation::Move {
                    id: id.clone(),
                    x,
                    y,
                },
                inverse: Operation::Move {
                    id: id.clone(),
                    x: old_x,
                    y: old_y,
                },
                touched: vec![id.clone()],
            })
        }
        Operation::Reparent { id, parent, index } => reparent(doc, id, parent, *index),
        Operation::Delete { id } => {
            let loc = node_loc(doc, id)?;
            immutable_if_root(doc, &loc)?;
            let (&index, parent_path) = loc
                .path
                .split_last()
                .ok_or_else(|| OpError::Internal("empty path after root check".to_string()))?;
            let parent_loc = crate::model::Location {
                page: loc.page,
                path: parent_path.to_vec(),
            };
            let parent_node = doc
                .node_at_mut(&parent_loc)
                .ok_or_else(|| OpError::Internal("parent vanished".to_string()))?;
            let parent_id = parent_node.id.clone();
            let removed = parent_node.children.remove(index);
            let mut touched = Vec::new();
            removed.ids(&mut touched);
            Ok(AppliedOp {
                forward: op.clone(),
                inverse: Operation::Restore {
                    parent: parent_id,
                    index,
                    node: removed,
                },
                touched,
            })
        }
    }
}

/// The value a `Set` actually stored (rounded), so redo reproduces the exact bytes.
fn current_value(node: &Node, prop: &Property) -> Property {
    match prop {
        Property::Name(_) => Property::Name(node.name.clone()),
        Property::X(_) => Property::X(node.x),
        Property::Y(_) => Property::Y(node.y),
        Property::Width(_) => Property::Width(node.width),
        Property::Height(_) => Property::Height(node.height),
        Property::Visible(_) => Property::Visible(node.visible),
        Property::Opacity(_) => Property::Opacity(node.opacity),
        Property::Fill(_) => Property::Fill(node.fill.clone()),
        Property::Stroke(_) => Property::Stroke(node.stroke.clone()),
        Property::CornerRadius(_) => Property::CornerRadius(node.corner_radius),
        Property::Text(_) => Property::Text(node.text.clone().unwrap_or_default()),
        Property::FontSize(_) => Property::FontSize(node.font_size.unwrap_or(16.0)),
        Property::FontFamily(_) => Property::FontFamily(node.font_family.clone()),
        Property::FontWeight(_) => Property::FontWeight(node.font_weight),
        Property::TextAlign(_) => Property::TextAlign(node.text_align),
        Property::LineHeight(_) => Property::LineHeight(node.line_height),
        Property::Image(_) => Property::Image(node.image.clone()),
    }
}

fn validate_subtree(node: &Node, tokens: &BTreeMap<String, String>) -> Result<(), OpError> {
    validate_node_fields(node, tokens)?;
    if !node.kind.is_container() && !node.children.is_empty() {
        return Err(OpError::NoChildren {
            parent_kind: node.kind.as_str(),
        });
    }
    node.children
        .iter()
        .try_for_each(|c| validate_subtree(c, tokens))
}

fn reparent(
    doc: &mut Document,
    id: &NodeId,
    parent: &NodeId,
    index: Option<usize>,
) -> Result<AppliedOp, OpError> {
    let loc = node_loc(doc, id)?;
    immutable_if_root(doc, &loc)?;
    let target = container_loc(doc, parent)?;
    if target.page == loc.page && target.path.starts_with(&loc.path) {
        return Err(OpError::Cycle {
            node: id.to_string(),
            target: parent.to_string(),
        });
    }
    let node_height = doc
        .node_at(&loc)
        .map(Node::height_below)
        .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
    if target.path.len() + 1 + node_height > limits::MAX_DEPTH {
        return Err(OpError::Limit(format!(
            "nesting deeper than {}",
            limits::MAX_DEPTH
        )));
    }
    let (&old_index, old_parent_path) = loc
        .path
        .split_last()
        .ok_or_else(|| OpError::Internal("empty path after root check".to_string()))?;
    let old_parent_loc = crate::model::Location {
        page: loc.page,
        path: old_parent_path.to_vec(),
    };
    let old_parent_id = doc
        .node_at(&old_parent_loc)
        .map(|n| n.id.clone())
        .ok_or_else(|| OpError::Internal("parent vanished".to_string()))?;
    let target_len = doc.node_at(&target).map_or(0, |n| n.children.len());
    let len_after_removal = if &old_parent_id == parent {
        target_len - 1
    } else {
        target_len
    };
    let new_index = index.unwrap_or(len_after_removal);
    check_insert_index(parent, new_index, len_after_removal)?;

    let removed = doc
        .node_at_mut(&old_parent_loc)
        .map(|p| p.children.remove(old_index))
        .ok_or_else(|| OpError::Internal("parent vanished".to_string()))?;
    insert(doc, parent, Some(new_index), removed)?;
    Ok(AppliedOp {
        forward: Operation::Reparent {
            id: id.clone(),
            parent: parent.clone(),
            index: Some(new_index),
        },
        inverse: Operation::Reparent {
            id: id.clone(),
            parent: old_parent_id.clone(),
            index: Some(old_index),
        },
        touched: vec![id.clone(), old_parent_id, parent.clone()],
    })
}
