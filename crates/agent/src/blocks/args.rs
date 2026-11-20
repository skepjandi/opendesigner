//! Block parameters: each block declares typed, bounded parameters with defaults, and the caller's
//! JSON is checked against that declaration before the block runs.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use od_core::geometry::{self, Rect};
use od_core::{Document, NodeId};
use serde_json::{Map, Value, json};

use super::BlockError;
use crate::assets::index;

const MAX_DATA_BYTES: usize = 16 * 1024;
const MAX_NODES_PER_CALL: usize = 40;

pub enum Kind {
    Text {
        default: &'static str,
        max: usize,
    },
    Number {
        default: f64,
        min: f64,
        max: f64,
    },
    Count {
        default: i64,
        min: i64,
        max: i64,
    },
    Choice {
        default: &'static str,
        options: &'static [&'static str],
    },
    Flag {
        default: bool,
    },
    /// JSON the block validates itself (charts, tables, lists), as text for its default.
    Data {
        default: &'static str,
    },
    /// A node id in the target frame, or `{x, y, width, height}` in the frame's coordinates.
    Region,
    /// Ids of existing nodes.
    Nodes,
    /// A `media/` image path.
    Image,
    /// A bundled Lucide icon name.
    Icon {
        default: &'static str,
    },
}

pub struct Param {
    pub name: &'static str,
    pub about: &'static str,
    pub kind: Kind,
}

impl Param {
    pub const fn text(
        name: &'static str,
        about: &'static str,
        default: &'static str,
        max: usize,
    ) -> Param {
        Param {
            name,
            about,
            kind: Kind::Text { default, max },
        }
    }

    pub const fn number(
        name: &'static str,
        about: &'static str,
        default: f64,
        min: f64,
        max: f64,
    ) -> Param {
        Param {
            name,
            about,
            kind: Kind::Number { default, min, max },
        }
    }

    pub const fn count(
        name: &'static str,
        about: &'static str,
        default: i64,
        min: i64,
        max: i64,
    ) -> Param {
        Param {
            name,
            about,
            kind: Kind::Count { default, min, max },
        }
    }

    pub const fn choice(
        name: &'static str,
        about: &'static str,
        default: &'static str,
        options: &'static [&'static str],
    ) -> Param {
        Param {
            name,
            about,
            kind: Kind::Choice { default, options },
        }
    }

    pub const fn flag(name: &'static str, about: &'static str, default: bool) -> Param {
        Param {
            name,
            about,
            kind: Kind::Flag { default },
        }
    }

    pub const fn data(name: &'static str, about: &'static str, default: &'static str) -> Param {
        Param {
            name,
            about,
            kind: Kind::Data { default },
        }
    }

    pub const fn region(name: &'static str, about: &'static str) -> Param {
        Param {
            name,
            about,
            kind: Kind::Region,
        }
    }

    pub const fn nodes(name: &'static str, about: &'static str) -> Param {
        Param {
            name,
            about,
            kind: Kind::Nodes,
        }
    }

    pub const fn image(name: &'static str, about: &'static str) -> Param {
        Param {
            name,
            about,
            kind: Kind::Image,
        }
    }

    pub const fn icon(name: &'static str, about: &'static str, default: &'static str) -> Param {
        Param {
            name,
            about,
            kind: Kind::Icon { default },
        }
    }

    pub fn describe(&self) -> Value {
        let (kind, extra) = match &self.kind {
            Kind::Text { default, max } => ("text", json!({"default": default, "max_length": max})),
            Kind::Number { default, min, max } => (
                "number",
                json!({"default": default, "min": min, "max": max}),
            ),
            Kind::Count { default, min, max } => (
                "integer",
                json!({"default": default, "min": min, "max": max}),
            ),
            Kind::Choice { default, options } => {
                ("choice", json!({"default": default, "options": options}))
            }
            Kind::Flag { default } => ("flag", json!({"default": default})),
            Kind::Data { default } => (
                "json",
                json!({"default": serde_json::from_str::<Value>(default).unwrap_or(Value::Null)}),
            ),
            Kind::Region => (
                "region",
                json!({"accepts": "a node id in the frame, or {x, y, width, height} in frame coordinates"}),
            ),
            Kind::Nodes => (
                "node_ids",
                json!({"required": true, "max": MAX_NODES_PER_CALL}),
            ),
            Kind::Image => ("image", json!({"accepts": "a media/ path from list_media"})),
            Kind::Icon { default } => (
                "icon",
                json!({"default": default, "accepts": "a Lucide icon name from search_assets"}),
            ),
        };
        let mut out = json!({"name": self.name, "type": kind, "about": self.about});
        if let (Some(o), Some(e)) = (out.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        out
    }
}

pub enum Val {
    Text(String),
    Num(f64),
    Flag(bool),
    Data(Value),
    Region(Option<Rect>),
    Nodes(Vec<NodeId>),
    Image(Option<String>),
}

/// The resolved parameters of one call. A block asking for a name it never declared is a bug in
/// the block; it is reported instead of answered with a made-up value.
pub struct Args {
    block: &'static str,
    vals: BTreeMap<&'static str, Val>,
    asked: RefCell<BTreeSet<&'static str>>,
    unknown: Cell<Option<&'static str>>,
}

fn bad(block: &str, param: &str, why: impl std::fmt::Display) -> BlockError {
    BlockError::Param(format!("{block}.{param}: {why}"))
}

fn region_of(
    v: &Value,
    doc: &Document,
    frame: &NodeId,
    block: &str,
    name: &str,
) -> Result<Rect, BlockError> {
    if let Some(id) = v.as_str() {
        let id = NodeId::parse(id).map_err(|e| bad(block, name, e))?;
        return geometry::bounds_in_frame(doc, frame, &id).ok_or_else(|| {
            bad(
                block,
                name,
                format!("node {id} is not inside the target frame"),
            )
        });
    }
    let num = |k: &str, required: bool| -> Result<f64, BlockError> {
        match v.get(k) {
            Some(n) => n
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| bad(block, name, format!("{k} must be a number"))),
            None if required => Err(bad(block, name, format!("needs {k}"))),
            None => Ok(0.0),
        }
    };
    if !v.is_object() {
        return Err(bad(block, name, "give a node id or {x, y, width, height}"));
    }
    let (x, y) = (num("x", true)?, num("y", true)?);
    let (width, height) = (num("width", false)?, num("height", false)?);
    if width < 0.0 || height < 0.0 {
        return Err(bad(block, name, "width and height cannot be negative"));
    }
    Ok(Rect {
        x,
        y,
        width,
        height,
    })
}

fn text_of(v: &Value, max: usize, block: &str, name: &str) -> Result<String, BlockError> {
    let s = v.as_str().ok_or_else(|| bad(block, name, "must be text"))?;
    if s.chars().count() > max {
        return Err(bad(block, name, format!("longer than {max} characters")));
    }
    if s.chars().any(|c| c.is_control() && c != '\n') {
        return Err(bad(block, name, "contains control characters"));
    }
    Ok(s.to_string())
}

fn icon_of(v: &Value, block: &str, name: &str) -> Result<String, BlockError> {
    let s = v
        .as_str()
        .ok_or_else(|| bad(block, name, "must be an icon name"))?;
    if s.is_empty() {
        return Ok(String::new());
    }
    index::resolve(Path::new(""), &format!("lucide:{s}")).map_err(|_| {
        bad(
            block,
            name,
            format!("no Lucide icon {s:?}; search_assets finds names"),
        )
    })?;
    Ok(s.to_string())
}

fn resolve_one(
    p: &Param,
    given: Option<&Value>,
    doc: &Document,
    frame: &NodeId,
    block: &str,
) -> Result<Val, BlockError> {
    let name = p.name;
    match (&p.kind, given) {
        (Kind::Text { default, max }, v) => Ok(Val::Text(match v {
            Some(v) => text_of(v, *max, block, name)?,
            None => (*default).to_string(),
        })),
        (Kind::Number { default, min, max }, v) => {
            let n = match v {
                Some(v) => v
                    .as_f64()
                    .ok_or_else(|| bad(block, name, "must be a number"))?,
                None => *default,
            };
            if !n.is_finite() || n < *min || n > *max {
                return Err(bad(block, name, format!("{n} is not in {min}..={max}")));
            }
            Ok(Val::Num(n))
        }
        (Kind::Count { default, min, max }, v) => {
            let n = match v {
                Some(v) => v
                    .as_i64()
                    .ok_or_else(|| bad(block, name, "must be a whole number"))?,
                None => *default,
            };
            if n < *min || n > *max {
                return Err(bad(block, name, format!("{n} is not in {min}..={max}")));
            }
            Ok(Val::Num(n as f64))
        }
        (Kind::Choice { default, options }, v) => {
            let s = match v {
                Some(v) => v.as_str().ok_or_else(|| bad(block, name, "must be text"))?,
                None => default,
            };
            if !options.contains(&s) {
                return Err(bad(
                    block,
                    name,
                    format!("{s:?} is not one of {}", options.join(", ")),
                ));
            }
            Ok(Val::Text(s.to_string()))
        }
        (Kind::Flag { default }, v) => Ok(Val::Flag(match v {
            Some(v) => v
                .as_bool()
                .ok_or_else(|| bad(block, name, "must be true or false"))?,
            None => *default,
        })),
        (Kind::Data { default }, v) => {
            let value = match v {
                Some(v) => v.clone(),
                None => serde_json::from_str(default)
                    .map_err(|e| bad(block, name, format!("default is not JSON: {e}")))?,
            };
            if value.to_string().len() > MAX_DATA_BYTES {
                return Err(bad(
                    block,
                    name,
                    format!("larger than {MAX_DATA_BYTES} bytes"),
                ));
            }
            Ok(Val::Data(value))
        }
        (Kind::Region, v) => Ok(Val::Region(
            v.map(|v| region_of(v, doc, frame, block, name))
                .transpose()?,
        )),
        (Kind::Nodes, v) => {
            let list = v
                .and_then(Value::as_array)
                .filter(|l| !l.is_empty())
                .ok_or_else(|| bad(block, name, "give a list of node ids"))?;
            if list.len() > MAX_NODES_PER_CALL {
                return Err(bad(
                    block,
                    name,
                    format!("at most {MAX_NODES_PER_CALL} nodes"),
                ));
            }
            let mut ids = Vec::new();
            for item in list {
                let id = item
                    .as_str()
                    .ok_or_else(|| bad(block, name, "ids are text"))
                    .and_then(|s| NodeId::parse(s).map_err(|e| bad(block, name, e)))?;
                if doc.node(&id).is_none() {
                    return Err(bad(block, name, format!("node {id} does not exist")));
                }
                ids.push(id);
            }
            Ok(Val::Nodes(ids))
        }
        (Kind::Image, v) => Ok(Val::Image(match v {
            Some(v) => {
                let s = v
                    .as_str()
                    .ok_or_else(|| bad(block, name, "must be a media/ path"))?;
                od_core::media::reference_name(s).map_err(|e| bad(block, name, e))?;
                Some(s.to_string())
            }
            None => None,
        })),
        (Kind::Icon { default }, v) => Ok(Val::Text(match v {
            Some(v) => icon_of(v, block, name)?,
            None => (*default).to_string(),
        })),
    }
}

impl Args {
    pub fn resolve(
        block: &'static str,
        params: &'static [Param],
        mut given: Map<String, Value>,
        doc: &Document,
        frame: &NodeId,
    ) -> Result<Args, BlockError> {
        let mut vals = BTreeMap::new();
        for p in params {
            let v = given.remove(p.name);
            vals.insert(p.name, resolve_one(p, v.as_ref(), doc, frame, block)?);
        }
        if let Some(extra) = given.keys().next() {
            let known: Vec<&str> = params.iter().map(|p| p.name).collect();
            return Err(BlockError::Param(format!(
                "{block} has no parameter {extra:?}; its parameters are {}",
                known.join(", ")
            )));
        }
        Ok(Args {
            block,
            vals,
            asked: RefCell::default(),
            unknown: Cell::new(None),
        })
    }

    fn get(&self, name: &'static str) -> Option<&Val> {
        let v = self.vals.get(name);
        if v.is_none() {
            self.unknown.set(Some(name));
        } else {
            self.asked.borrow_mut().insert(name);
        }
        v
    }

    pub fn text(&self, name: &'static str) -> &str {
        match self.get(name) {
            Some(Val::Text(s)) => s,
            _ => "",
        }
    }

    pub fn num(&self, name: &'static str) -> f64 {
        match self.get(name) {
            Some(Val::Num(n)) => *n,
            _ => 0.0,
        }
    }

    pub fn flag(&self, name: &'static str) -> bool {
        matches!(self.get(name), Some(Val::Flag(true)))
    }

    pub fn data(&self, name: &'static str) -> &Value {
        match self.get(name) {
            Some(Val::Data(v)) => v,
            _ => &Value::Null,
        }
    }

    pub fn region(&self, name: &'static str) -> Option<Rect> {
        match self.get(name) {
            Some(Val::Region(r)) => *r,
            _ => None,
        }
    }

    pub fn nodes(&self, name: &'static str) -> &[NodeId] {
        match self.get(name) {
            Some(Val::Nodes(n)) => n,
            _ => &[],
        }
    }

    pub fn image(&self, name: &'static str) -> Option<&str> {
        match self.get(name) {
            Some(Val::Image(i)) => i.as_deref(),
            _ => None,
        }
    }

    /// A block read a parameter it does not declare.
    pub fn check(&self) -> Result<(), BlockError> {
        match self.unknown.get() {
            Some(name) => Err(BlockError::Internal(format!(
                "{} reads parameter {name:?} it never declared",
                self.block
            ))),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    pub fn unread(&self) -> Vec<&'static str> {
        let asked = self.asked.borrow();
        self.vals
            .keys()
            .filter(|k| !asked.contains(*k))
            .copied()
            .collect()
    }
}
