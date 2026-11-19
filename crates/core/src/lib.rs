//! OpenDesigner core: the document model, validated operations, history and project files.
//!
//! This crate has no UI dependency. The Qt shell, the MCP server and the CLI are front ends over
//! the operations defined here (`docs/wiki/architecture.md`).

pub mod error;
pub mod history;
pub mod limits;
pub mod model;
pub mod ops;
pub mod paint;
pub mod project;

pub use error::{OpError, ProjectError};
pub use history::{Actor, Checkpoint, Session, Transaction, TransactionKind, TxReceipt};
pub use model::{Document, Node, NodeId, NodeKind, Page, Stroke};
pub use ops::{AppliedOp, NewNode, Operation, Property};
pub use paint::{Paint, Rgba};
