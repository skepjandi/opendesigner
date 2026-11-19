//! History: an append-only log of transactions, checkpoints at positions in it, and undo/redo
//! that append transactions instead of rewriting the log (`docs/wiki/document-format.md`).

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::OpError;
use crate::limits;
use crate::model::{Document, NodeId};
use crate::ops::{self, AppliedOp, Operation};

/// Who made a change. Agent edits are attributed so the UI can outline them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum Actor {
    User,
    Agent(String),
}

impl Actor {
    pub fn agent(name: &str) -> Result<Actor, OpError> {
        let ok = !name.is_empty()
            && name.len() <= 64
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '));
        if ok {
            Ok(Actor::Agent(name.to_string()))
        } else {
            Err(OpError::OutOfBounds {
                what: "agent name",
                detail: "use 1-64 characters from A-Z a-z 0-9 space . _ -".to_string(),
            })
        }
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Actor::User => f.write_str("user"),
            Actor::Agent(n) => write!(f, "agent:{n}"),
        }
    }
}

impl TryFrom<String> for Actor {
    type Error = OpError;
    fn try_from(s: String) -> Result<Self, OpError> {
        match s.as_str() {
            "user" => Ok(Actor::User),
            _ => match s.strip_prefix("agent:") {
                Some(name) => Actor::agent(name),
                None => Err(OpError::OutOfBounds {
                    what: "actor",
                    detail: format!("{s:?} is neither \"user\" nor \"agent:<name>\""),
                }),
            },
        }
    }
}

impl From<Actor> for String {
    fn from(a: Actor) -> String {
        a.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TransactionKind {
    Edit,
    Undo { of: u64 },
    Redo { of: u64 },
    Restore { checkpoint: u64 },
}

/// A group of operations applied together: one drag, one agent turn, one undo step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    /// Position in the log.
    pub id: u64,
    pub actor: Actor,
    /// Groups the transactions of one agent response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn: Option<String>,
    pub kind: TransactionKind,
    pub ops: Vec<AppliedOp>,
}

/// A named point in the log. Restoring one appends the inverse of everything after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: u64,
    pub name: String,
    pub actor: Actor,
    /// Number of transactions applied when the checkpoint was taken.
    pub position: u64,
}

/// What a front end learns from a committed transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct TxReceipt {
    pub id: u64,
    /// The operations as applied, with ids resolved.
    pub ops: Vec<Operation>,
    /// Every node the transaction touched.
    pub touched: Vec<NodeId>,
    /// Roots of nodes added by an edit's `create` operations.
    pub created: Vec<NodeId>,
}

pub struct Session {
    doc: Document,
    log: Vec<Transaction>,
    checkpoints: Vec<Checkpoint>,
    undo_stack: Vec<u64>,
    redo_stack: Vec<u64>,
}

impl Session {
    pub fn new(doc: Document) -> Session {
        Session {
            doc,
            log: Vec::new(),
            checkpoints: Vec::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    /// Rebuilds a session from saved parts. Undo and redo stacks are not persisted.
    pub fn from_parts(
        doc: Document,
        log: Vec<Transaction>,
        checkpoints: Vec<Checkpoint>,
    ) -> Result<Session, OpError> {
        doc.validate()?;
        for (i, tx) in log.iter().enumerate() {
            if tx.id != i as u64 {
                return Err(OpError::Internal(format!("log entry {i} has id {}", tx.id)));
            }
        }
        for cp in &checkpoints {
            if cp.position > log.len() as u64 {
                return Err(OpError::Internal(format!(
                    "checkpoint {} is past the end of the log",
                    cp.id
                )));
            }
        }
        Ok(Session {
            doc,
            log,
            checkpoints,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        })
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// Sets a design token. Setup only: not recorded in the history.
    pub fn set_token(&mut self, name: &str, value: &str) -> Result<(), OpError> {
        self.doc.set_token(name, value)
    }

    /// Adds an empty page. Setup only: not recorded in the history.
    pub fn add_page(&mut self, name: &str) -> Result<NodeId, OpError> {
        self.doc.add_page(name)
    }

    pub fn log(&self) -> &[Transaction] {
        &self.log
    }

    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Applies `ops` atomically as one transaction: all succeed or the document is unchanged.
    pub fn apply(
        &mut self,
        actor: Actor,
        turn: Option<String>,
        ops: &[Operation],
    ) -> Result<TxReceipt, OpError> {
        if ops.is_empty() {
            return Err(OpError::NothingTo("apply"));
        }
        if ops.len() > limits::MAX_OPS_PER_TRANSACTION {
            return Err(OpError::Limit(format!(
                "at most {} operations per transaction",
                limits::MAX_OPS_PER_TRANSACTION
            )));
        }
        if let Some(t) = &turn {
            limits::text_within("turn id", t, 100)?;
        }
        let id = self.commit(actor, turn, TransactionKind::Edit, ops, false)?;
        self.undo_stack.push(id);
        self.redo_stack.clear();
        Ok(self.receipt(id))
    }

    pub fn undo(&mut self, actor: Actor) -> Result<TxReceipt, OpError> {
        let of = self.undo_stack.pop().ok_or(OpError::NothingTo("undo"))?;
        let ops = self.inverse_of(of as usize..=of as usize);
        match self.commit(actor, None, TransactionKind::Undo { of }, &ops, true) {
            Ok(id) => {
                self.redo_stack.push(of);
                Ok(self.receipt(id))
            }
            Err(e) => {
                self.undo_stack.push(of);
                Err(e)
            }
        }
    }

    pub fn redo(&mut self, actor: Actor) -> Result<TxReceipt, OpError> {
        let of = self.redo_stack.pop().ok_or(OpError::NothingTo("redo"))?;
        let ops: Vec<Operation> = self.log[of as usize]
            .ops
            .iter()
            .map(|a| a.forward.clone())
            .collect();
        match self.commit(actor, None, TransactionKind::Redo { of }, &ops, true) {
            Ok(id) => {
                self.undo_stack.push(of);
                Ok(self.receipt(id))
            }
            Err(e) => {
                self.redo_stack.push(of);
                Err(e)
            }
        }
    }

    /// Names the current state. Agents take one at the end of every turn.
    pub fn checkpoint(&mut self, actor: Actor, name: &str) -> Result<u64, OpError> {
        limits::text_within("checkpoint name", name, limits::MAX_NAME_LEN)?;
        let id = self.checkpoints.len() as u64 + 1;
        self.checkpoints.push(Checkpoint {
            id,
            name: name.to_string(),
            actor,
            position: self.log.len() as u64,
        });
        Ok(id)
    }

    /// Returns the document to a checkpoint by appending the inverse of everything since it.
    pub fn restore(&mut self, actor: Actor, checkpoint: u64) -> Result<TxReceipt, OpError> {
        let position = self
            .checkpoints
            .iter()
            .find(|c| c.id == checkpoint)
            .map(|c| c.position as usize)
            .ok_or(OpError::NoSuchCheckpoint(checkpoint))?;
        if position >= self.log.len() {
            return Err(OpError::NothingTo("restore"));
        }
        let ops = self.inverse_of(position..=self.log.len() - 1);
        if ops.is_empty() {
            return Err(OpError::NothingTo("restore"));
        }
        let id = self.commit(
            actor,
            None,
            TransactionKind::Restore { checkpoint },
            &ops,
            true,
        )?;
        self.undo_stack.push(id);
        self.redo_stack.clear();
        Ok(self.receipt(id))
    }

    /// Nodes touched by transactions at or after `position` that still exist: what the UI outlines
    /// after an agent turn. Pass a checkpoint's `position`.
    pub fn changed_since(&self, position: u64) -> Vec<NodeId> {
        let mut set = BTreeSet::new();
        for tx in self.log.iter().skip(position as usize) {
            for op in &tx.ops {
                set.extend(op.touched.iter().cloned());
            }
        }
        set.into_iter()
            .filter(|id| self.doc.node(id).is_some())
            .collect()
    }

    fn inverse_of(&self, range: std::ops::RangeInclusive<usize>) -> Vec<Operation> {
        self.log[range]
            .iter()
            .rev()
            .flat_map(|tx| tx.ops.iter().rev().map(|a| a.inverse.clone()))
            .collect()
    }

    fn commit(
        &mut self,
        actor: Actor,
        turn: Option<String>,
        kind: TransactionKind,
        ops: &[Operation],
        allow_internal: bool,
    ) -> Result<u64, OpError> {
        let mut applied: Vec<AppliedOp> = Vec::with_capacity(ops.len());
        for op in ops {
            match ops::apply(&mut self.doc, op, allow_internal) {
                Ok(a) => applied.push(a),
                Err(e) => {
                    self.rollback(&applied)?;
                    return Err(e);
                }
            }
        }
        let id = self.log.len() as u64;
        self.log.push(Transaction {
            id,
            actor,
            turn,
            kind,
            ops: applied,
        });
        Ok(id)
    }

    fn rollback(&mut self, applied: &[AppliedOp]) -> Result<(), OpError> {
        for a in applied.iter().rev() {
            ops::apply(&mut self.doc, &a.inverse, true)
                .map_err(|e| OpError::Internal(format!("rollback failed: {e}")))?;
        }
        Ok(())
    }

    fn receipt(&self, id: u64) -> TxReceipt {
        let tx = &self.log[id as usize];
        let mut touched = BTreeSet::new();
        let mut created = Vec::new();
        for a in &tx.ops {
            touched.extend(a.touched.iter().cloned());
        }
        if tx.kind == TransactionKind::Edit {
            for a in &tx.ops {
                if let Operation::Restore { node, .. } = &a.forward {
                    created.push(node.id.clone());
                }
            }
        }
        TxReceipt {
            id,
            ops: tx.ops.iter().map(|a| a.forward.clone()).collect(),
            touched: touched.into_iter().collect(),
            created,
        }
    }
}
