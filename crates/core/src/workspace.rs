//! A workspace is what a front end holds open: the session (document and history), the
//! annotations, the user's current selection, and where it all lives on disk. `guidance()` is the
//! structured "what the user is pointing at" that an agent receives before it acts.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::annotations::{Annotation, Annotations};
use crate::error::{OpError, ProjectError};
use crate::history::{Actor, Session, TransactionKind};
use crate::model::{Node, NodeId};
use crate::ops::Operation;
use crate::project;

const MAX_SELECTION: usize = 1_000;
const MAX_RECENT_USER_EDITS: usize = 20;

pub struct Workspace {
    pub session: Session,
    pub annotations: Annotations,
    selection: Vec<NodeId>,
    dir: Option<PathBuf>,
}

/// A selected node with its data, without its subtree (the ids of its children are listed).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SelectedNode {
    pub page: NodeId,
    pub parent: Option<NodeId>,
    pub node: Node,
    pub child_ids: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnnotationView {
    #[serde(flatten)]
    pub annotation: Annotation,
    /// False when the node the note was pinned to has since been deleted.
    pub target_exists: bool,
}

/// An edit the user made after the agent's last turn: the agent can learn from the correction.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UserEdit {
    pub transaction: u64,
    pub ops: Vec<Operation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Guidance {
    pub project: String,
    pub pages: Vec<PageSummary>,
    pub selection: Vec<SelectedNode>,
    /// Open annotations, oldest first.
    pub annotations: Vec<AnnotationView>,
    pub recent_user_edits: Vec<UserEdit>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageSummary {
    pub id: NodeId,
    pub name: String,
    pub root: NodeId,
}

impl Workspace {
    pub fn new(session: Session) -> Workspace {
        Workspace {
            session,
            annotations: Annotations::default(),
            selection: Vec::new(),
            dir: None,
        }
    }

    pub fn create(dir: &Path, name: &str) -> Result<Workspace, ProjectError> {
        let session = project::create(dir, name)?;
        Ok(Workspace {
            session,
            annotations: Annotations::default(),
            selection: Vec::new(),
            dir: Some(dir.to_path_buf()),
        })
    }

    pub fn open(dir: &Path) -> Result<Workspace, ProjectError> {
        let session = project::load(dir)?;
        let annotations = project::load_annotations(dir)?;
        annotations
            .validate()
            .map_err(|e| ProjectError::Invalid(e.to_string()))?;
        Ok(Workspace {
            session,
            annotations,
            selection: Vec::new(),
            dir: Some(dir.to_path_buf()),
        })
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn save(&self) -> Result<(), ProjectError> {
        let dir = self
            .dir
            .as_deref()
            .ok_or_else(|| ProjectError::Invalid("the workspace has no project folder".into()))?;
        project::save(dir, &self.session)?;
        project::save_annotations(dir, &self.annotations)
    }

    pub fn select(&mut self, ids: Vec<NodeId>) -> Result<(), OpError> {
        if ids.len() > MAX_SELECTION {
            return Err(OpError::Limit(format!(
                "at most {MAX_SELECTION} selected nodes"
            )));
        }
        for id in &ids {
            if self.session.document().node(id).is_none() {
                return Err(OpError::NoSuchNode(id.to_string()));
            }
        }
        self.selection = ids;
        Ok(())
    }

    /// The selection, minus nodes deleted since it was made.
    pub fn selection(&self) -> Vec<NodeId> {
        self.selection
            .iter()
            .filter(|id| self.session.document().node(id).is_some())
            .cloned()
            .collect()
    }

    pub fn guidance(&self) -> Guidance {
        let doc = self.session.document();
        let selection = self
            .selection()
            .iter()
            .filter_map(|id| {
                let loc = doc.locate(id)?;
                let node = doc.node_at(&loc)?;
                let parent = loc.path.split_last().and_then(|(_, rest)| {
                    doc.node_at(&crate::model::Location {
                        page: loc.page,
                        path: rest.to_vec(),
                    })
                    .map(|p| p.id.clone())
                });
                let mut shallow = node.clone();
                let child_ids = shallow.children.drain(..).map(|c| c.id).collect();
                Some(SelectedNode {
                    page: doc.pages.get(loc.page)?.id.clone(),
                    parent,
                    node: shallow,
                    child_ids,
                })
            })
            .collect();
        let annotations = self
            .annotations
            .open()
            .map(|a| AnnotationView {
                target_exists: match &a.target {
                    crate::annotations::AnnotationTarget::Node { id } => doc.node(id).is_some(),
                    crate::annotations::AnnotationTarget::Region { page, .. } => {
                        doc.page(page).is_some()
                    }
                },
                annotation: a.clone(),
            })
            .collect();
        Guidance {
            project: doc.name.clone(),
            pages: doc
                .pages
                .iter()
                .map(|p| PageSummary {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    root: p.root.id.clone(),
                })
                .collect(),
            selection,
            annotations,
            recent_user_edits: self.recent_user_edits(),
        }
    }

    fn recent_user_edits(&self) -> Vec<UserEdit> {
        let log = self.session.log();
        let start = log
            .iter()
            .rposition(|tx| matches!(tx.actor, Actor::Agent(_)))
            .map_or(0, |i| i + 1);
        let mut edits: Vec<UserEdit> = log[start..]
            .iter()
            .filter(|tx| tx.actor == Actor::User && tx.kind == TransactionKind::Edit)
            .map(|tx| UserEdit {
                transaction: tx.id,
                ops: tx.ops.iter().map(|a| a.forward.clone()).collect(),
            })
            .collect();
        if edits.len() > MAX_RECENT_USER_EDITS {
            edits.drain(..edits.len() - MAX_RECENT_USER_EDITS);
        }
        edits
    }
}
