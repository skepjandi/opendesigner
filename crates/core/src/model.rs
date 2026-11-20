use std::collections::{BTreeMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::{self, finite_in, text_within};
use crate::paint::{Paint, valid_token_name};

/// Version of the on-disk project format. A loader refuses a newer one. Version 2 added 3D
/// devices and keyframe animations; every version 1 project is a valid version 2 project.
pub const FORMAT_VERSION: u32 = 2;

/// Stable identifier of a node or page. Never reused within a document.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct NodeId(String);

impl NodeId {
    pub fn parse(s: &str) -> Result<NodeId, OpError> {
        let ok = !s.is_empty()
            && s.len() <= 64
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if ok {
            Ok(NodeId(s.to_string()))
        } else {
            Err(OpError::InvalidId(s.to_string()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn numbered(prefix: char, n: u64) -> NodeId {
        NodeId(format!("{prefix}_{n}"))
    }

    /// The counter part of ids minted by the document (`n_12` gives 12).
    pub(crate) fn number(&self) -> Option<u64> {
        self.0.split_once('_').and_then(|(_, n)| n.parse().ok())
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for NodeId {
    type Error = OpError;
    fn try_from(s: String) -> Result<Self, OpError> {
        NodeId::parse(&s)
    }
}

impl From<NodeId> for String {
    fn from(id: NodeId) -> String {
        id.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A container with a size and clipping; the unit that exports.
    Frame,
    /// Transform-only grouping.
    Group,
    Rect,
    Ellipse,
    Text,
    /// A raster image from the project's `media/` folder.
    Image,
    /// A device in 3D (phone, tablet, laptop, monitor) drawn in the node's box.
    Device3d,
}

impl NodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Frame => "frame",
            NodeKind::Group => "group",
            NodeKind::Rect => "rect",
            NodeKind::Ellipse => "ellipse",
            NodeKind::Text => "text",
            NodeKind::Image => "image",
            NodeKind::Device3d => "device3d",
        }
    }

    pub fn is_container(self) -> bool {
        matches!(self, NodeKind::Frame | NodeKind::Group)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    pub paint: Paint,
    pub width: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// How an image fills its node's box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFit {
    /// Scale to cover the box and crop the overflow (the default).
    Cover,
    /// Scale to fit inside the box, keeping the whole image.
    Contain,
    /// Stretch to the box.
    Fill,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub visible: bool,
    #[serde(default = "default_one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Paint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<Stroke>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub corner_radius: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Family name; unset means the bundled default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// 100 to 900; unset means 400.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<f64>,
    /// Unset means left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_align: Option<TextAlign>,
    /// Multiple of the font size; unset means 1.2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Unset means cover.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_fit: Option<ImageFit>,
    /// The 3D device a `device3d` node draws (present exactly on those nodes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device3d: Option<crate::device3d::Device3d>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Node>,
}

fn default_true() -> bool {
    true
}
fn is_true(v: &bool) -> bool {
    *v
}
fn default_one() -> f64 {
    1.0
}
fn is_one(v: &f64) -> bool {
    *v == 1.0
}
fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Node {
    pub(crate) fn blank(id: NodeId, kind: NodeKind, name: String) -> Node {
        Node {
            id,
            kind,
            name,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            visible: true,
            opacity: 1.0,
            fill: None,
            stroke: None,
            corner_radius: 0.0,
            text: (kind == NodeKind::Text).then(String::new),
            font_size: (kind == NodeKind::Text).then_some(16.0),
            font_family: None,
            font_weight: None,
            text_align: None,
            line_height: None,
            image: None,
            image_fit: None,
            device3d: (kind == NodeKind::Device3d).then(crate::device3d::Device3d::default),
            children: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Node::count).sum::<usize>()
    }

    /// Depth of the deepest descendant below this node (a leaf is 0).
    pub fn height_below(&self) -> usize {
        self.children
            .iter()
            .map(|c| 1 + c.height_below())
            .max()
            .unwrap_or(0)
    }

    pub fn ids(&self, out: &mut Vec<NodeId>) {
        out.push(self.id.clone());
        for c in &self.children {
            c.ids(out);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Page {
    pub id: NodeId,
    pub name: String,
    /// An immutable group holding the page's top-level nodes.
    pub root: Node,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub format: u32,
    pub name: String,
    /// Counter for minting ids. Only grows, so ids are never reused.
    pub next_id: u64,
    /// Named colors, referenced by paints as `{name}`.
    pub tokens: BTreeMap<String, String>,
    /// Translations of text nodes: language tag, then node id to text. A text node without an
    /// entry shows its own `text` in that language.
    pub locales: BTreeMap<String, BTreeMap<NodeId, String>>,
    pub pages: Vec<Page>,
    /// Timelines of clips that show frames of the pages.
    pub videos: Vec<crate::video::Video>,
}

/// Where a node sits: the page index and the child indices from the page root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub page: usize,
    pub path: Vec<usize>,
}

impl Document {
    pub fn new(name: &str) -> Document {
        Document {
            format: FORMAT_VERSION,
            name: name.to_string(),
            next_id: 1,
            tokens: BTreeMap::new(),
            locales: BTreeMap::new(),
            pages: Vec::new(),
            videos: Vec::new(),
        }
    }

    pub fn alloc_id(&mut self, prefix: char) -> NodeId {
        let id = NodeId::numbered(prefix, self.next_id);
        self.next_id += 1;
        id
    }

    pub fn add_page(&mut self, name: &str) -> Result<NodeId, OpError> {
        if self.pages.len() >= limits::MAX_PAGES {
            return Err(OpError::Limit(format!(
                "at most {} pages",
                limits::MAX_PAGES
            )));
        }
        text_within("page name", name, limits::MAX_NAME_LEN)?;
        let id = self.alloc_id('p');
        let root_id = self.alloc_id('n');
        self.pages.push(Page {
            id: id.clone(),
            name: name.to_string(),
            root: Node::blank(root_id, NodeKind::Group, "Root".to_string()),
        });
        Ok(id)
    }

    pub fn set_token(&mut self, name: &str, value: &str) -> Result<(), OpError> {
        if !valid_token_name(name) {
            return Err(OpError::UnknownToken(name.to_string()));
        }
        let value = crate::paint::Rgba::parse_hex(value)?.to_hex();
        if self.tokens.len() >= limits::MAX_TOKENS && !self.tokens.contains_key(name) {
            return Err(OpError::Limit(format!(
                "at most {} tokens",
                limits::MAX_TOKENS
            )));
        }
        self.tokens.insert(name.to_string(), value);
        Ok(())
    }

    /// Sets (or with `None` removes) the translation of text node `id` in `locale`; returns the
    /// previous one. A language with no translations left disappears.
    pub fn set_locale_text(
        &mut self,
        locale: &str,
        id: &NodeId,
        text: Option<&str>,
    ) -> Result<Option<String>, OpError> {
        limits::locale_tag(locale)?;
        let node = self
            .node(id)
            .ok_or_else(|| OpError::NoSuchNode(id.to_string()))?;
        if node.kind != NodeKind::Text {
            return Err(OpError::NotApplicable {
                property: "translation",
                kind: node.kind.as_str(),
            });
        }
        if let Some(t) = text {
            text_within("text", t, limits::MAX_TEXT_LEN)?;
            if !self.locales.contains_key(locale) && self.locales.len() >= limits::MAX_LOCALES {
                return Err(OpError::Limit(format!(
                    "at most {} languages",
                    limits::MAX_LOCALES
                )));
            }
        }
        let old = match text {
            Some(t) => self
                .locales
                .entry(locale.to_string())
                .or_default()
                .insert(id.clone(), t.to_string()),
            None => {
                let old = self.locales.get_mut(locale).and_then(|m| m.remove(id));
                if self.locales.get(locale).is_some_and(BTreeMap::is_empty) {
                    self.locales.remove(locale);
                }
                old
            }
        };
        Ok(old)
    }

    /// The document as it reads in `locale`: every translated text node shows its translation.
    /// `None` when the language has no translations.
    pub fn localized(&self, locale: &str) -> Option<Document> {
        fn walk(n: &mut Node, texts: &BTreeMap<NodeId, String>) {
            if let Some(t) = texts.get(&n.id) {
                n.text = Some(t.clone());
            }
            for c in &mut n.children {
                walk(c, texts);
            }
        }
        let texts = self.locales.get(locale)?;
        let mut doc = self.clone();
        for page in &mut doc.pages {
            walk(&mut page.root, texts);
        }
        Some(doc)
    }

    /// Ids of the nodes whose fill or stroke refers to the token `name`.
    pub fn token_users(&self, name: &str) -> Vec<NodeId> {
        fn uses(p: &crate::paint::Paint, name: &str) -> bool {
            matches!(p, crate::paint::Paint::Token(t) if t == name)
        }
        fn walk(n: &Node, name: &str, out: &mut Vec<NodeId>) {
            let fill = n.fill.as_ref().is_some_and(|p| uses(p, name));
            let stroke = n.stroke.as_ref().is_some_and(|s| uses(&s.paint, name));
            if fill || stroke {
                out.push(n.id.clone());
            }
            for c in &n.children {
                walk(c, name, out);
            }
        }
        let mut out = Vec::new();
        for page in &self.pages {
            walk(&page.root, name, &mut out);
        }
        out
    }

    pub fn page(&self, id: &NodeId) -> Option<&Page> {
        self.pages.iter().find(|p| &p.id == id)
    }

    pub fn locate(&self, id: &NodeId) -> Option<Location> {
        for (page, p) in self.pages.iter().enumerate() {
            let mut path = Vec::new();
            if find_path(&p.root, id, &mut path) {
                return Some(Location { page, path });
            }
        }
        None
    }

    pub fn node_at(&self, loc: &Location) -> Option<&Node> {
        let mut node = &self.pages.get(loc.page)?.root;
        for &i in &loc.path {
            node = node.children.get(i)?;
        }
        Some(node)
    }

    pub fn node_at_mut(&mut self, loc: &Location) -> Option<&mut Node> {
        let mut node = &mut self.pages.get_mut(loc.page)?.root;
        for &i in &loc.path {
            node = node.children.get_mut(i)?;
        }
        Some(node)
    }

    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.locate(id).and_then(|loc| self.node_at(&loc))
    }

    pub fn node_count(&self) -> usize {
        self.pages.iter().map(|p| p.root.count()).sum()
    }

    /// Checks every invariant a loaded or edited document must hold.
    pub fn validate(&self) -> Result<(), OpError> {
        if self.pages.len() > limits::MAX_PAGES {
            return Err(OpError::Limit(format!(
                "at most {} pages",
                limits::MAX_PAGES
            )));
        }
        if self.tokens.len() > limits::MAX_TOKENS {
            return Err(OpError::Limit(format!(
                "at most {} tokens",
                limits::MAX_TOKENS
            )));
        }
        if self.locales.len() > limits::MAX_LOCALES {
            return Err(OpError::Limit(format!(
                "at most {} languages",
                limits::MAX_LOCALES
            )));
        }
        for (locale, texts) in &self.locales {
            limits::locale_tag(locale)?;
            for text in texts.values() {
                text_within("text", text, limits::MAX_TEXT_LEN)?;
            }
        }
        for (name, value) in &self.tokens {
            if !valid_token_name(name) {
                return Err(OpError::UnknownToken(name.clone()));
            }
            crate::paint::Rgba::parse_hex(value)?;
        }
        let mut seen = HashSet::new();
        let mut total = 0usize;
        for page in &self.pages {
            if !seen.insert(page.id.clone()) {
                return Err(OpError::DuplicateId(page.id.to_string()));
            }
            if let Some(n) = page.id.number() {
                self.check_counter(n)?;
            }
            text_within("page name", &page.name, limits::MAX_NAME_LEN)?;
            if page.root.kind != NodeKind::Group {
                return Err(OpError::RootImmutable(page.id.to_string()));
            }
            self.validate_tree(&page.root, 0, &mut seen, &mut total)?;
        }
        self.validate_videos()
    }

    pub(crate) fn check_counter_of(&self, id: &NodeId) -> Result<(), OpError> {
        match id.number() {
            Some(n) => self.check_counter(n),
            None => Ok(()),
        }
    }

    fn check_counter(&self, n: u64) -> Result<(), OpError> {
        if n >= self.next_id {
            return Err(OpError::Limit(format!(
                "next_id {} is not past id number {n}",
                self.next_id
            )));
        }
        Ok(())
    }

    fn validate_tree(
        &self,
        node: &Node,
        depth: usize,
        seen: &mut HashSet<NodeId>,
        total: &mut usize,
    ) -> Result<(), OpError> {
        *total += 1;
        if *total > limits::MAX_NODES {
            return Err(OpError::Limit(format!(
                "at most {} nodes",
                limits::MAX_NODES
            )));
        }
        if depth > limits::MAX_DEPTH {
            return Err(OpError::Limit(format!(
                "nesting deeper than {}",
                limits::MAX_DEPTH
            )));
        }
        if !seen.insert(node.id.clone()) {
            return Err(OpError::DuplicateId(node.id.to_string()));
        }
        if let Some(n) = node.id.number() {
            self.check_counter(n)?;
        }
        validate_node_fields(node, &self.tokens)?;
        if !node.kind.is_container() && !node.children.is_empty() {
            return Err(OpError::NoChildren {
                parent_kind: node.kind.as_str(),
            });
        }
        for child in &node.children {
            self.validate_tree(child, depth + 1, seen, total)?;
        }
        Ok(())
    }
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

/// Field-level checks shared by loading and by `Restore`.
pub(crate) fn validate_node_fields(
    node: &Node,
    tokens: &BTreeMap<String, String>,
) -> Result<(), OpError> {
    text_within("name", &node.name, limits::MAX_NAME_LEN)?;
    finite_in("x", node.x, -limits::MAX_COORD, limits::MAX_COORD)?;
    finite_in("y", node.y, -limits::MAX_COORD, limits::MAX_COORD)?;
    finite_in("width", node.width, 0.0, limits::MAX_SIZE)?;
    finite_in("height", node.height, 0.0, limits::MAX_SIZE)?;
    finite_in("opacity", node.opacity, 0.0, 1.0)?;
    finite_in("corner_radius", node.corner_radius, 0.0, limits::MAX_SIZE)?;
    if let Some(size) = node.font_size {
        finite_in("font_size", size, 1.0, limits::MAX_FONT_SIZE)?;
    }
    if let Some(family) = &node.font_family {
        limits::family_name(family)?;
    }
    if let Some(w) = node.font_weight {
        finite_in(
            "font_weight",
            w,
            limits::MIN_FONT_WEIGHT,
            limits::MAX_FONT_WEIGHT,
        )?;
    }
    if let Some(h) = node.line_height {
        finite_in(
            "line_height",
            h,
            limits::MIN_LINE_HEIGHT,
            limits::MAX_LINE_HEIGHT,
        )?;
    }
    if let Some(text) = &node.text {
        text_within("text", text, limits::MAX_TEXT_LEN)?;
    }
    if let Some(path) = &node.image {
        crate::media::reference_name(path)?;
    }
    if let Some(fill) = &node.fill {
        fill.resolve(tokens)?;
    }
    if let Some(stroke) = &node.stroke {
        stroke.paint.resolve(tokens)?;
        finite_in("stroke width", stroke.width, 0.0, limits::MAX_STROKE_WIDTH)?;
    }
    match (&node.device3d, node.kind) {
        (Some(d), NodeKind::Device3d) => d.check()?,
        (None, NodeKind::Device3d) => {
            return Err(OpError::OutOfBounds {
                what: "device3d",
                detail: format!("node {} is a device3d without its device", node.id),
            });
        }
        (Some(_), kind) => {
            return Err(OpError::NotApplicable {
                property: "device3d",
                kind: kind.as_str(),
            });
        }
        (None, _) => {}
    }
    Ok(())
}
