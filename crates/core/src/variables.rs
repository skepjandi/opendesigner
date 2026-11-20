//! Template variables: typed, document-level slots whose value is written into the properties of
//! the nodes they are bound to (`docs/wiki/document-format.md#template-variables`).
//!
//! A variable never renders by itself. Changing its value is one operation that rewrites every
//! bound node property through the same validated `set` path as a hand edit, so history, undo,
//! redo and attribution apply unchanged. Per-language text (`Document.locales`) is a different
//! thing: it is keyed by node and language and wins over a variable-written default.

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits::{self, text_within};
use crate::model::{Document, NodeId};
use crate::ops::{self, AppliedOp, Operation, Property};
use crate::paint::Paint;

pub const MAX_VARIABLES: usize = 100;
pub const MAX_BINDINGS: usize = 64;
pub const MAX_ENUM_VALUES: usize = 32;
pub const MAX_ENUM_VALUE_LEN: usize = 64;
pub const MAX_DESCRIPTION_LEN: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VariableKind {
    String,
    Number,
    Color,
    Bool,
    Enum,
    Image,
    Font,
}

impl VariableKind {
    pub fn as_str(self) -> &'static str {
        match self {
            VariableKind::String => "string",
            VariableKind::Number => "number",
            VariableKind::Color => "color",
            VariableKind::Bool => "bool",
            VariableKind::Enum => "enum",
            VariableKind::Image => "image",
            VariableKind::Font => "font",
        }
    }

    /// The node properties a variable of this kind may be bound to.
    pub fn bindable(self) -> &'static [&'static str] {
        match self {
            VariableKind::String => &["text", "name"],
            VariableKind::Number => &[
                "x",
                "y",
                "width",
                "height",
                "opacity",
                "corner_radius",
                "font_size",
                "font_weight",
                "line_height",
                "rotate_x",
                "rotate_y",
                "rotate_z",
                "zoom",
                "fov",
                "light_angle",
                "shadow",
                "glare",
                "screen_scroll",
            ],
            VariableKind::Color => &["fill"],
            VariableKind::Bool => &["visible"],
            VariableKind::Enum => &["text", "text_align", "image_fit", "model"],
            VariableKind::Image => &["image"],
            VariableKind::Font => &["font_family"],
        }
    }
}

/// What a slot stands for in the brand or subject of the piece, so an agent knows which values
/// carry identity (and must not be invented) when it fills a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Portrays {
    SubjectName,
    SubjectDomain,
    SubjectTagline,
    SubjectLogo,
    HostIdentity,
}

/// A variable's value. Strings carry colors, enum choices, media references and font families.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VarValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

impl VarValue {
    fn to_json(&self) -> serde_json::Value {
        match self {
            VarValue::Bool(b) => serde_json::Value::Bool(*b),
            VarValue::Number(n) => serde_json::json!(n),
            VarValue::Text(t) => serde_json::Value::String(t.clone()),
        }
    }
}

/// One node property that follows a variable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub node: NodeId,
    pub prop: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variable {
    pub id: NodeId,
    pub kind: VariableKind,
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portrays: Option<Portrays>,
    pub default: VarValue,
    /// What the bound nodes currently show.
    pub value: VarValue,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binds: Vec<Binding>,
}

/// A variable to declare. Its value starts at `default`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewVariable {
    pub id: String,
    pub kind: VariableKind,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub portrays: Option<Portrays>,
    pub default: VarValue,
    #[serde(default)]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub binds: Vec<Binding>,
}

/// One change to a declared variable. Serialized as `{"name": "label", "value": "Headline"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "value", rename_all = "snake_case")]
pub enum VariableProperty {
    Label(String),
    Description(String),
    Portrays(Option<Portrays>),
    Default(VarValue),
    EnumValues(Vec<String>),
    /// The bound properties; newly bound nodes take the current value at once.
    Binds(Vec<Binding>),
}

/// A node property to put back, as the history writes it when undoing a variable operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub node: NodeId,
    pub prop: Property,
}

fn bad(detail: impl Into<String>) -> OpError {
    OpError::OutOfBounds {
        what: "variable",
        detail: detail.into(),
    }
}

impl Variable {
    fn check_value(&self, value: &VarValue) -> Result<(), OpError> {
        let id = &self.id;
        match (self.kind, value) {
            (VariableKind::String, VarValue::Text(t)) => {
                text_within("variable value", t, limits::MAX_TEXT_LEN)
            }
            (VariableKind::Number, VarValue::Number(n)) if n.is_finite() => Ok(()),
            (VariableKind::Bool, VarValue::Bool(_)) => Ok(()),
            (VariableKind::Color, VarValue::Text(t)) => Paint::parse(t).map(|_| ()),
            (VariableKind::Enum, VarValue::Text(t)) if self.enum_values.contains(t) => Ok(()),
            (VariableKind::Enum, VarValue::Text(t)) => Err(bad(format!(
                "{t:?} is not one of the choices of {id}: {}",
                self.enum_values.join(", ")
            ))),
            (VariableKind::Image, VarValue::Text(t)) => crate::media::reference_name(t).map(|_| ()),
            (VariableKind::Font, VarValue::Text(t)) => limits::family_name(t),
            (kind, _) => Err(bad(format!(
                "variable {id} is a {} and does not take that value",
                kind.as_str()
            ))),
        }
    }

    /// Shape checks that need no document: sizes, kind and value agreement, bindable properties.
    pub fn check(&self) -> Result<(), OpError> {
        text_within("variable label", &self.label, limits::MAX_NAME_LEN)?;
        if self.label.trim().is_empty() {
            return Err(bad(format!("variable {} needs a label", self.id)));
        }
        text_within(
            "variable description",
            &self.description,
            MAX_DESCRIPTION_LEN,
        )?;
        self.check_choices()?;
        self.check_value(&self.default)?;
        self.check_value(&self.value)?;
        self.check_binds()
    }

    fn check_choices(&self) -> Result<(), OpError> {
        if self.kind != VariableKind::Enum {
            return if self.enum_values.is_empty() {
                Ok(())
            } else {
                Err(bad(format!(
                    "variable {} is a {}: only an enum has enum_values",
                    self.id,
                    self.kind.as_str()
                )))
            };
        }
        if self.enum_values.is_empty() || self.enum_values.len() > MAX_ENUM_VALUES {
            return Err(bad(format!(
                "an enum has 1 to {MAX_ENUM_VALUES} enum_values"
            )));
        }
        for (i, v) in self.enum_values.iter().enumerate() {
            if v.is_empty() {
                return Err(bad("an enum value cannot be empty"));
            }
            text_within("enum value", v, MAX_ENUM_VALUE_LEN)?;
            if self.enum_values[..i].contains(v) {
                return Err(bad(format!("enum value {v:?} is listed twice")));
            }
        }
        Ok(())
    }

    fn check_binds(&self) -> Result<(), OpError> {
        if self.binds.len() > MAX_BINDINGS {
            return Err(OpError::Limit(format!(
                "at most {MAX_BINDINGS} bindings per variable"
            )));
        }
        let allowed = self.kind.bindable();
        for (i, b) in self.binds.iter().enumerate() {
            if !allowed.contains(&b.prop.as_str()) {
                return Err(bad(format!(
                    "a {} variable cannot drive {:?}; it can drive: {}",
                    self.kind.as_str(),
                    b.prop,
                    allowed.join(", ")
                )));
            }
            if self.binds[..i].contains(b) {
                return Err(bad(format!("{}.{} is bound twice", b.node, b.prop)));
            }
        }
        Ok(())
    }

    /// Whether this variable's current value can be written to `prop` of `node`, without writing.
    pub(crate) fn writes(
        &self,
        node: &crate::model::Node,
        tokens: &std::collections::BTreeMap<String, String>,
        prop: &str,
    ) -> Result<(), String> {
        let property = self
            .property(prop, &self.value)
            .map_err(|e| e.to_string())?;
        let mut copy = node.clone();
        ops::set_property(&mut copy, property, tokens).map_err(|e| e.to_string())
    }

    fn property(&self, prop: &str, value: &VarValue) -> Result<Property, OpError> {
        let named = serde_json::json!({"name": prop, "value": value.to_json()});
        serde_json::from_value(named)
            .map_err(|e| bad(format!("variable {} cannot set {prop}: {e}", self.id)))
    }
}

impl NewVariable {
    fn into_variable(self) -> Result<Variable, OpError> {
        let var = Variable {
            id: NodeId::parse(&self.id)?,
            kind: self.kind,
            label: self.label,
            description: self.description,
            portrays: self.portrays,
            value: self.default.clone(),
            default: self.default,
            enum_values: self.enum_values,
            binds: self.binds,
        };
        var.check()?;
        Ok(var)
    }
}

impl Document {
    pub fn variable(&self, id: &NodeId) -> Option<&Variable> {
        self.variables.iter().find(|v| &v.id == id)
    }

    /// Checks every declared variable (a loaded project or a restore).
    pub(crate) fn validate_variables(&self) -> Result<(), OpError> {
        if self.variables.len() > MAX_VARIABLES {
            return Err(OpError::Limit(format!("at most {MAX_VARIABLES} variables")));
        }
        for (i, v) in self.variables.iter().enumerate() {
            if self.variables[..i].iter().any(|o| o.id == v.id) {
                return Err(OpError::DuplicateId(v.id.to_string()));
            }
            v.check()?;
        }
        Ok(())
    }
}

fn find(doc: &Document, id: &NodeId) -> Result<usize, OpError> {
    doc.variables
        .iter()
        .position(|v| &v.id == id)
        .ok_or_else(|| bad(format!("variable {id} does not exist")))
}

/// Writes `value` into the nodes `binds` names, as `set` operations; returns the operations that
/// put the old values back and the nodes touched. A bound node that no longer exists is skipped
/// (the linter reports it), so deleting a node never blocks the other bindings.
fn write_bound(
    doc: &mut Document,
    var: &Variable,
    binds: &[Binding],
    value: &VarValue,
) -> Result<(Vec<Assignment>, Vec<NodeId>), OpError> {
    let mut undo = Vec::new();
    let mut touched = Vec::new();
    for b in binds {
        if doc.node(&b.node).is_none() {
            continue;
        }
        let prop = var.property(&b.prop, value)?;
        let applied = ops::apply(
            doc,
            &Operation::Set {
                id: b.node.clone(),
                prop,
            },
            false,
        );
        match applied {
            Ok(a) => {
                if let Operation::Set { prop, .. } = a.inverse {
                    undo.push(Assignment {
                        node: b.node.clone(),
                        prop,
                    });
                }
                touched.push(b.node.clone());
            }
            Err(e) => {
                put_back(doc, &undo)?;
                return Err(bad(format!(
                    "variable {} cannot change {}.{}: {e}",
                    var.id, b.node, b.prop
                )));
            }
        }
    }
    undo.reverse();
    Ok((undo, touched))
}

/// Applies `assignments`; the operations that undo them are returned in the order to apply.
fn put_back(doc: &mut Document, assignments: &[Assignment]) -> Result<Vec<Assignment>, OpError> {
    let mut inverse = Vec::with_capacity(assignments.len());
    for a in assignments {
        let applied = ops::apply(
            doc,
            &Operation::Set {
                id: a.node.clone(),
                prop: a.prop.clone(),
            },
            true,
        )?;
        if let Operation::Set { prop, .. } = applied.inverse {
            inverse.push(Assignment {
                node: a.node.clone(),
                prop,
            });
        }
    }
    inverse.reverse();
    Ok(inverse)
}

pub(crate) fn apply(
    doc: &mut Document,
    op: &Operation,
    allow_internal: bool,
) -> Result<AppliedOp, OpError> {
    match op {
        Operation::DeclareVariable { variable } => declare(doc, op, variable.clone()),
        Operation::RemoveVariable { id } => {
            let at = find(doc, id)?;
            let old = doc.variables.remove(at);
            Ok(AppliedOp {
                forward: op.clone(),
                inverse: Operation::RestoreVariable {
                    id: id.clone(),
                    index: at,
                    variable: Some(old),
                    sets: Vec::new(),
                },
                touched: Vec::new(),
            })
        }
        Operation::SetVariable { id, prop } => set_variable(doc, op, id, prop),
        Operation::SetVariableValue { id, value } => set_value(doc, op, id, value),
        Operation::RestoreVariable {
            id,
            index,
            variable,
            sets,
        } => {
            if !allow_internal {
                return Err(OpError::InternalOnly("restore_variable"));
            }
            restore(doc, id, *index, variable, sets)
        }
        _ => Err(OpError::Internal(format!(
            "{} is not a variable operation",
            op.name()
        ))),
    }
}

fn declare(doc: &mut Document, op: &Operation, new: NewVariable) -> Result<AppliedOp, OpError> {
    if doc.variables.len() >= MAX_VARIABLES {
        return Err(OpError::Limit(format!("at most {MAX_VARIABLES} variables")));
    }
    let var = new.into_variable()?;
    if doc.variable(&var.id).is_some() {
        return Err(OpError::DuplicateId(var.id.to_string()));
    }
    check_nodes(doc, &var.binds)?;
    let (sets, touched) = write_bound(doc, &var, &var.binds, &var.default)?;
    let id = var.id.clone();
    let index = doc.variables.len();
    doc.variables.push(var);
    Ok(AppliedOp {
        forward: op.clone(),
        inverse: Operation::RestoreVariable {
            id,
            index,
            variable: None,
            sets,
        },
        touched,
    })
}

fn set_variable(
    doc: &mut Document,
    op: &Operation,
    id: &NodeId,
    prop: &VariableProperty,
) -> Result<AppliedOp, OpError> {
    let at = find(doc, id)?;
    let old = doc.variables[at].clone();
    let mut var = old.clone();
    match prop {
        VariableProperty::Label(l) => var.label = l.clone(),
        VariableProperty::Description(d) => var.description = d.clone(),
        VariableProperty::Portrays(p) => var.portrays = *p,
        VariableProperty::Default(d) => var.default = d.clone(),
        VariableProperty::EnumValues(v) => var.enum_values = v.clone(),
        VariableProperty::Binds(b) => var.binds = b.clone(),
    }
    var.check()?;
    let (sets, touched) = match prop {
        VariableProperty::Binds(binds) => {
            let fresh: Vec<Binding> = binds
                .iter()
                .filter(|b| !old.binds.contains(b))
                .cloned()
                .collect();
            check_nodes(doc, &fresh)?;
            write_bound(doc, &var, &fresh, &var.value)?
        }
        _ => (Vec::new(), Vec::new()),
    };
    doc.variables[at] = var;
    Ok(AppliedOp {
        forward: op.clone(),
        inverse: Operation::RestoreVariable {
            id: id.clone(),
            index: at,
            variable: Some(old),
            sets,
        },
        touched,
    })
}

fn set_value(
    doc: &mut Document,
    op: &Operation,
    id: &NodeId,
    value: &VarValue,
) -> Result<AppliedOp, OpError> {
    let at = find(doc, id)?;
    let old = doc.variables[at].clone();
    old.check_value(value)?;
    let (sets, touched) = write_bound(doc, &old, &old.binds, value)?;
    let mut var = old.clone();
    var.value = value.clone();
    doc.variables[at] = var;
    Ok(AppliedOp {
        forward: op.clone(),
        inverse: Operation::RestoreVariable {
            id: id.clone(),
            index: at,
            variable: Some(old),
            sets,
        },
        touched,
    })
}

fn restore(
    doc: &mut Document,
    id: &NodeId,
    index: usize,
    variable: &Option<Variable>,
    sets: &[Assignment],
) -> Result<AppliedOp, OpError> {
    let before = doc.variables.iter().position(|v| &v.id == id);
    let before_var = before.map(|at| doc.variables[at].clone());
    let inverse_sets = put_back(doc, sets)?;
    match (before, variable) {
        (Some(at), Some(v)) => doc.variables[at] = v.clone(),
        (Some(at), None) => {
            doc.variables.remove(at);
        }
        (None, Some(v)) => doc
            .variables
            .insert(index.min(doc.variables.len()), v.clone()),
        (None, None) => {}
    }
    let touched: Vec<NodeId> = sets.iter().map(|a| a.node.clone()).collect();
    Ok(AppliedOp {
        forward: Operation::RestoreVariable {
            id: id.clone(),
            index,
            variable: variable.clone(),
            sets: sets.to_vec(),
        },
        inverse: Operation::RestoreVariable {
            id: id.clone(),
            index: before.unwrap_or(0),
            variable: before_var,
            sets: inverse_sets,
        },
        touched,
    })
}

fn check_nodes(doc: &Document, binds: &[Binding]) -> Result<(), OpError> {
    match binds.iter().find(|b| doc.node(&b.node).is_none()) {
        Some(b) => Err(OpError::NoSuchNode(b.node.to_string())),
        None => Ok(()),
    }
}
