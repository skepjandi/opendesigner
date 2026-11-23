//! Code mode: a project that is a real codebase. This crate holds what is true about it (mode,
//! the stack, the run plan), what the user approved, and the tmux sessions that run the plan
//! (`docs/wiki/code-mode.md`). No Qt and no network: the app, the MCP tools and the CLI are
//! front ends over it.

pub mod approval;
pub mod detect;
pub mod plan;
pub mod project;
pub mod runner;
pub mod stack;
pub mod tmux;

use thiserror::Error;

#[derive(Debug, Error)]
#[error("{0}")]
pub struct CodeError(pub String);

pub(crate) fn fail<T>(message: impl Into<String>) -> Result<T, CodeError> {
    Err(CodeError(message.into()))
}

pub(crate) fn io(what: &str, source: std::io::Error) -> CodeError {
    CodeError(format!("{what}: {source}"))
}

/// Text from an untrusted source: no control characters, not longer than `max` characters.
pub(crate) fn plain_text(
    what: &str,
    text: &str,
    max: usize,
    allow_newlines: bool,
) -> Result<(), CodeError> {
    if text.chars().count() > max {
        return fail(format!("{what} is longer than {max} characters"));
    }
    let bad = |c: char| c.is_control() && !(allow_newlines && matches!(c, '\n' | '\t'));
    if text.chars().any(bad) {
        return fail(format!("{what} holds a control character"));
    }
    Ok(())
}
