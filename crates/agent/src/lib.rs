//! Lets coding agents drive OpenDesigner: a local MCP server over the shared workspace, the tools
//! it exposes, and the per-install token that guards it (`docs/wiki/agent-integration.md`).

pub mod acp;
pub mod assets;
pub mod connect;
pub mod device;
pub mod direction;
pub mod error;
pub mod export;
pub mod formats;
pub mod freesound;
pub mod http;
pub mod mcp;
pub mod mcp_servers;
pub mod memory;
pub mod playbook;
pub mod program;
pub mod review;
pub mod server;
pub mod skill;
pub mod synth;
pub mod token;
pub mod tools;
pub mod video_export;
pub mod video_preview;
pub mod workspaces;

pub use error::AgentError;
pub use mcp::{ChangeEvent, Handler, Outcome};
pub use server::{DEFAULT_PORT, RunningServer, start};
