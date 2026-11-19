//! Lets coding agents drive OpenDesigner: a local MCP server over the shared workspace, the tools
//! it exposes, and the per-install token that guards it (`docs/wiki/agent-integration.md`).

pub mod acp;
pub mod connect;
pub mod error;
pub mod export;
pub mod mcp;
pub mod server;
pub mod skill;
pub mod token;
pub mod tools;

pub use error::AgentError;
pub use mcp::{ChangeEvent, Handler, Outcome};
pub use server::{DEFAULT_PORT, RunningServer, start};
