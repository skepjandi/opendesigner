//! Dry runs and stable error codes for operations. `Workspace::check_ops` applies a batch to a
//! scratch copy of the document and reports what would happen, leaving the history, the
//! document and the files untouched (`docs/wiki/agent-integration.md#dry-run-and-receipts`).

use std::collections::BTreeSet;

use serde::Serialize;

use crate::error::OpError;
use crate::limits;
use crate::model::NodeId;
use crate::ops::{self, Operation};
use crate::workspace::Workspace;

impl OpError {
    /// A stable code for this kind of rejection. The set only grows; text may change, codes do not.
    pub fn code(&self) -> &'static str {
        match self {
            OpError::NoSuchNode(_) => "E_NODE_NOT_FOUND",
            OpError::DuplicateId(_) => "E_DUPLICATE_ID",
            OpError::InvalidId(_) => "E_INVALID_ID",
            OpError::OutOfBounds { .. } => "E_OUT_OF_BOUNDS",
            OpError::Cycle { .. } => "E_CYCLE",
            OpError::BadIndex { .. } => "E_BAD_INDEX",
            OpError::RootImmutable(_) => "E_ROOT_IMMUTABLE",
            OpError::NotApplicable { .. } => "E_NOT_APPLICABLE",
            OpError::NoChildren { .. } => "E_NO_CHILDREN",
            OpError::InvalidPaint(_) => "E_INVALID_PAINT",
            OpError::UnknownToken(_) => "E_UNKNOWN_TOKEN",
            OpError::TokenInUse { .. } => "E_TOKEN_IN_USE",
            OpError::Limit(_) => "E_LIMIT",
            OpError::InternalOnly(_) => "E_INTERNAL_ONLY",
            OpError::NothingTo(_) => "E_NOTHING_TO",
            OpError::NoSuchCheckpoint(_) => "E_CHECKPOINT_NOT_FOUND",
            OpError::NoSuchAnnotation(_) => "E_ANNOTATION_NOT_FOUND",
            OpError::NoSuchPage(_) => "E_PAGE_NOT_FOUND",
            OpError::NoSuchVideo(_) => "E_VIDEO_NOT_FOUND",
            OpError::NoSuchClip(_) => "E_CLIP_NOT_FOUND",
            OpError::FrameInUse { .. } => "E_FRAME_IN_USE",
            OpError::Arrange(_) => "E_ARRANGE",
            OpError::NodeAnimated { .. } => "E_NODE_ANIMATED",
            OpError::Internal(_) => "E_INTERNAL",
        }
    }

    /// What to do about it, for an agent that read the message and is still unsure.
    pub fn hint(&self) -> &'static str {
        match self {
            OpError::NoSuchNode(_)
            | OpError::NoSuchPage(_)
            | OpError::NoSuchVideo(_)
            | OpError::NoSuchClip(_) => {
                "Ids are minted by the document: read them from get_tree, find_nodes or list_videos instead of guessing."
            }
            OpError::DuplicateId(_) | OpError::InvalidId(_) => {
                "Do not supply ids when creating; they are minted for you and listed in the result."
            }
            OpError::OutOfBounds { .. } => {
                "Keep numbers finite and inside the stated range; the message names the limit."
            }
            OpError::Cycle { .. } => "Reparent under a node that is not inside the moved node.",
            OpError::BadIndex { .. } => "Omit index to append, or use 0 up to the child count.",
            OpError::RootImmutable(_) => {
                "A page root cannot change; create or move the nodes inside it."
            }
            OpError::NotApplicable { .. } => {
                "Check the node kind with get_node; set only properties that kind has."
            }
            OpError::NoChildren { .. } => "Only frames and groups hold children.",
            OpError::InvalidPaint(_) => {
                "Use #rrggbb, #rrggbbaa or a {token.name} from list_tokens."
            }
            OpError::UnknownToken(_) => {
                "Define it first with set_token, or pick one from list_tokens."
            }
            OpError::TokenInUse { .. } => {
                "Recolor the nodes that use the token first, or set the token to another color."
            }
            OpError::Limit(_) => {
                "Split the work into smaller batches or remove what is not needed."
            }
            OpError::InternalOnly(_) => {
                "This operation is written by the history; do not issue it."
            }
            OpError::NothingTo(_) => "There is nothing to do; check the current state first.",
            OpError::NoSuchCheckpoint(_) => "List ids with list_checkpoints.",
            OpError::NoSuchAnnotation(_) => "List ids with get_annotations.",
            OpError::FrameInUse { .. } => {
                "Remove or retarget the clip or overlay that shows the frame first (list_videos)."
            }
            OpError::Arrange(_) => "Fix the arrangement request named in the message.",
            OpError::NodeAnimated { .. } => {
                "Remove the animation first: animate the property with no keys."
            }
            OpError::Internal(_) => "This is a bug; report it and do not retry unchanged.",
        }
    }

    pub fn failure(&self) -> OpFailure {
        OpFailure {
            code: self.code(),
            message: self.to_string(),
            hint: self.hint(),
        }
    }
}

/// A rejection in the form agents read: a stable code, the message, and what to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpFailure {
    pub code: &'static str,
    pub message: String,
    pub hint: &'static str,
}

/// What one operation of a dry run did (or why it stopped the batch).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpReport {
    pub index: usize,
    pub op: &'static str,
    pub ok: bool,
    pub touched: Vec<NodeId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<OpFailure>,
}

/// The result of checking a batch without applying it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpsCheck {
    /// Whether the batch would apply. The batch is atomic, so one failure fails it all.
    pub ok: bool,
    pub ops: Vec<OpReport>,
    /// Every node the batch would touch (what a real apply reports as `touched`).
    pub would_change: Vec<NodeId>,
    /// Roots of the nodes, videos, clips and overlays the batch would add.
    pub would_create: Vec<NodeId>,
}

/// The root of what `forward` adds to the document, when it adds something.
pub(crate) fn created_by(forward: &Operation) -> Option<NodeId> {
    match forward {
        Operation::Restore { node, .. } => Some(node.id.clone()),
        Operation::RestoreVideo { video } => Some(video.id.clone()),
        Operation::RestoreClip { clip, .. } => Some(clip.id.clone()),
        Operation::RestoreOverlay { overlay, .. } => Some(overlay.id.clone()),
        _ => None,
    }
}

fn batch_error(ops: &[Operation]) -> Option<OpError> {
    if ops.is_empty() {
        return Some(OpError::NothingTo("apply"));
    }
    (ops.len() > limits::MAX_OPS_PER_TRANSACTION).then(|| {
        OpError::Limit(format!(
            "at most {} operations per transaction",
            limits::MAX_OPS_PER_TRANSACTION
        ))
    })
}

impl Workspace {
    /// Applies `ops` to a scratch copy of the document and reports what each would do, stopping at
    /// the first rejection. The workspace, its history and the project files are not touched; a
    /// real `Session::apply` of the same batch on the same state changes exactly what is reported.
    pub fn check_ops(&self, ops: &[Operation]) -> OpsCheck {
        if let Some(e) = batch_error(ops) {
            return OpsCheck {
                ok: false,
                ops: vec![OpReport {
                    index: 0,
                    op: "batch",
                    ok: false,
                    touched: Vec::new(),
                    error: Some(e.failure()),
                }],
                would_change: Vec::new(),
                would_create: Vec::new(),
            };
        }
        let mut scratch = self.session.document().clone();
        let mut reports = Vec::with_capacity(ops.len());
        let mut changed = BTreeSet::new();
        let mut created = Vec::new();
        let mut ok = true;
        for (index, op) in ops.iter().enumerate() {
            match ops::apply(&mut scratch, op, false) {
                Ok(applied) => {
                    changed.extend(applied.touched.iter().cloned());
                    created.extend(created_by(&applied.forward));
                    reports.push(OpReport {
                        index,
                        op: op.name(),
                        ok: true,
                        touched: applied.touched,
                        error: None,
                    });
                }
                Err(e) => {
                    reports.push(OpReport {
                        index,
                        op: op.name(),
                        ok: false,
                        touched: Vec::new(),
                        error: Some(e.failure()),
                    });
                    ok = false;
                    break;
                }
            }
        }
        OpsCheck {
            ok,
            ops: reports,
            would_change: if ok {
                changed.into_iter().collect()
            } else {
                Vec::new()
            },
            would_create: if ok { created } else { Vec::new() },
        }
    }
}
