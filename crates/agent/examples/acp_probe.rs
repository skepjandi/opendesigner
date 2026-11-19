//! Starts a real ACP agent and runs the session setup of the in-app chat against it, to check
//! that the agent and this client understand each other.
//!
//! ```text
//! cargo run -p opendesigner-agent --example acp_probe -- [--url=URL] [--token-file=FILE]
//!     [--cwd=DIR] [--prompt=TEXT] <agent command> [args...]
//! ```
//!
//! Without `--prompt` nothing is sent to the model. With it, one turn runs and every event is
//! printed; permission requests are rejected. Start `opendesigner serve-mcp <project>` first and
//! pass its `--url` and `--token-file` to see the agent use the editor's tools.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

use od_agent::acp::{AcpSession, AgentCommand, ChatEvent, McpEndpoint};

#[tokio::main]
async fn main() {
    let mut url = "http://127.0.0.1:45790/mcp".to_string();
    let mut token = "probe-token".to_string();
    let mut cwd = std::env::temp_dir();
    let mut prompt = None;
    let mut rest = Vec::new();
    for arg in std::env::args().skip(1) {
        if !rest.is_empty() {
            rest.push(arg);
        } else if let Some(v) = arg.strip_prefix("--url=") {
            url = v.to_string();
        } else if let Some(v) = arg.strip_prefix("--token-file=") {
            token = std::fs::read_to_string(v).unwrap().trim().to_string();
        } else if let Some(v) = arg.strip_prefix("--cwd=") {
            cwd = v.into();
        } else if let Some(v) = arg.strip_prefix("--prompt=") {
            prompt = Some(v.to_string());
        } else {
            rest.push(arg);
        }
    }
    if rest.is_empty() {
        println!(
            "usage: acp_probe [--url=] [--token-file=] [--cwd=] [--prompt=] <agent command> [args...]"
        );
        return;
    }
    let command = AgentCommand {
        program: rest.remove(0),
        args: rest,
        cwd,
    };
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let session =
        match AcpSession::start(&command, &McpEndpoint { url, token }, &[], tx.clone()).await {
            Ok(session) => std::sync::Arc::new(session),
            Err(e) => {
                println!("setup failed: {e}");
                return;
            }
        };
    println!("session opened");
    if let Some(text) = prompt {
        session.prompt(&text, tx);
        while let Some(event) = rx.recv().await {
            println!("{event:?}");
            match event {
                ChatEvent::Permission { request, .. } => session.answer_permission(request, None),
                ChatEvent::TurnEnded(_) | ChatEvent::Failed(_) | ChatEvent::Exited(_) => break,
                _ => {}
            }
        }
    }
    session.shutdown().await;
}
