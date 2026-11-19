//! Annotations: notes the user pins on a node or a canvas region. They are the structured half of
//! the guidance an agent receives ("make *this* bigger"); an agent reads the open ones, works them
//! and resolves them (`docs/wiki/agent-integration.md#guidance-context`).

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::history::Actor;
use crate::limits::{self, finite_in, text_within};
use crate::model::{Document, NodeId};

pub const MAX_NOTE_LEN: usize = 2_000;
pub const MAX_ANNOTATIONS: usize = 5_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnnotationTarget {
    /// A pin on a node.
    Node { id: NodeId },
    /// A box drawn on a page, in page coordinates.
    Region {
        page: NodeId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: u64,
    pub target: AnnotationTarget,
    pub note: String,
    pub status: AnnotationStatus,
    pub author: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<Actor>,
    /// What the resolver did, in the user's terms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotations {
    next_id: u64,
    items: Vec<Annotation>,
}

impl Default for Annotations {
    fn default() -> Self {
        Annotations {
            next_id: 1,
            items: Vec::new(),
        }
    }
}

impl Annotations {
    pub fn all(&self) -> &[Annotation] {
        &self.items
    }

    pub fn open(&self) -> impl Iterator<Item = &Annotation> {
        self.items
            .iter()
            .filter(|a| a.status == AnnotationStatus::Open)
    }

    pub fn get(&self, id: u64) -> Option<&Annotation> {
        self.items.iter().find(|a| a.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Result<&mut Annotation, OpError> {
        self.items
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(OpError::NoSuchAnnotation(id))
    }

    pub fn add(
        &mut self,
        doc: &Document,
        target: AnnotationTarget,
        note: &str,
        author: Actor,
    ) -> Result<u64, OpError> {
        if self.items.len() >= MAX_ANNOTATIONS {
            return Err(OpError::Limit(format!(
                "at most {MAX_ANNOTATIONS} annotations"
            )));
        }
        text_within("note", note, MAX_NOTE_LEN)?;
        if note.trim().is_empty() {
            return Err(OpError::OutOfBounds {
                what: "note",
                detail: "is empty".to_string(),
            });
        }
        let target = validate_target(doc, target)?;
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Annotation {
            id,
            target,
            note: note.to_string(),
            status: AnnotationStatus::Open,
            author,
            resolved_by: None,
            resolution: None,
        });
        Ok(id)
    }

    pub fn resolve(&mut self, id: u64, by: Actor, resolution: Option<&str>) -> Result<(), OpError> {
        if let Some(r) = resolution {
            text_within("resolution", r, MAX_NOTE_LEN)?;
        }
        let item = self.get_mut(id)?;
        item.status = AnnotationStatus::Resolved;
        item.resolved_by = Some(by);
        item.resolution = resolution.map(str::to_string);
        Ok(())
    }

    pub fn reopen(&mut self, id: u64) -> Result<(), OpError> {
        let item = self.get_mut(id)?;
        item.status = AnnotationStatus::Open;
        item.resolved_by = None;
        item.resolution = None;
        Ok(())
    }

    pub fn remove(&mut self, id: u64) -> Result<(), OpError> {
        let before = self.items.len();
        self.items.retain(|a| a.id != id);
        if self.items.len() == before {
            return Err(OpError::NoSuchAnnotation(id));
        }
        Ok(())
    }

    /// Checks a loaded set: bounded, unique ids, a counter past every id.
    pub fn validate(&self) -> Result<(), OpError> {
        if self.items.len() > MAX_ANNOTATIONS {
            return Err(OpError::Limit(format!(
                "at most {MAX_ANNOTATIONS} annotations"
            )));
        }
        let mut seen = std::collections::HashSet::new();
        for a in &self.items {
            if !seen.insert(a.id) || a.id >= self.next_id {
                return Err(OpError::Internal(format!(
                    "annotation id {} is duplicated or not below next_id {}",
                    a.id, self.next_id
                )));
            }
            text_within("note", &a.note, MAX_NOTE_LEN)?;
        }
        Ok(())
    }
}

fn validate_target(doc: &Document, target: AnnotationTarget) -> Result<AnnotationTarget, OpError> {
    match target {
        AnnotationTarget::Node { id } => {
            if doc.node(&id).is_none() {
                return Err(OpError::NoSuchNode(id.to_string()));
            }
            Ok(AnnotationTarget::Node { id })
        }
        AnnotationTarget::Region {
            page,
            x,
            y,
            width,
            height,
        } => {
            if doc.page(&page).is_none() {
                return Err(OpError::NoSuchPage(page.to_string()));
            }
            Ok(AnnotationTarget::Region {
                page,
                x: finite_in("region x", x, -limits::MAX_COORD, limits::MAX_COORD)?,
                y: finite_in("region y", y, -limits::MAX_COORD, limits::MAX_COORD)?,
                width: finite_in("region width", width, 0.0, limits::MAX_COORD)?,
                height: finite_in("region height", height, 0.0, limits::MAX_COORD)?,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> (Document, NodeId, NodeId) {
        let mut d = Document::new("t");
        let page = d.add_page("p").unwrap();
        let root = d.page(&page).unwrap().root.id.clone();
        (d, page, root)
    }

    #[test]
    fn pins_on_nodes_and_regions_open_and_resolve() {
        let (d, page, root) = doc();
        let mut a = Annotations::default();
        let pin = a
            .add(
                &d,
                AnnotationTarget::Node { id: root },
                "headline too small",
                Actor::User,
            )
            .unwrap();
        let region = a
            .add(
                &d,
                AnnotationTarget::Region {
                    page,
                    x: 10.0,
                    y: 10.0,
                    width: 50.0,
                    height: 20.0,
                },
                "more space here",
                Actor::User,
            )
            .unwrap();
        assert_eq!(a.open().count(), 2);
        a.resolve(
            pin,
            Actor::agent("claude-code").unwrap(),
            Some("raised to 96"),
        )
        .unwrap();
        assert_eq!(a.open().map(|x| x.id).collect::<Vec<_>>(), vec![region]);
        assert_eq!(
            a.get(pin).unwrap().resolution.as_deref(),
            Some("raised to 96")
        );
        a.reopen(pin).unwrap();
        assert_eq!(a.open().count(), 2);
        a.remove(region).unwrap();
        assert!(matches!(
            a.remove(region),
            Err(OpError::NoSuchAnnotation(_))
        ));
        a.validate().unwrap();
    }

    #[test]
    fn rejects_bad_targets_and_notes() {
        let (d, page, _) = doc();
        let mut a = Annotations::default();
        let missing = NodeId::parse("n_999").unwrap();
        assert!(matches!(
            a.add(&d, AnnotationTarget::Node { id: missing }, "x", Actor::User),
            Err(OpError::NoSuchNode(_))
        ));
        assert!(matches!(
            a.add(
                &d,
                AnnotationTarget::Region {
                    page: NodeId::parse("p_999").unwrap(),
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0
                },
                "x",
                Actor::User
            ),
            Err(OpError::NoSuchPage(_))
        ));
        let nan_region = AnnotationTarget::Region {
            page,
            x: f64::NAN,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        };
        assert!(a.add(&d, nan_region, "x", Actor::User).is_err());
        let root = d.pages[0].root.id.clone();
        assert!(
            a.add(
                &d,
                AnnotationTarget::Node { id: root.clone() },
                "   ",
                Actor::User
            )
            .is_err()
        );
        let long = "x".repeat(MAX_NOTE_LEN + 1);
        assert!(
            a.add(&d, AnnotationTarget::Node { id: root }, &long, Actor::User)
                .is_err()
        );
    }
}
