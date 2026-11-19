//! Starts a real ACP agent and runs the session setup of the in-app chat against it, to check
//! that the agent and this client understand each other. It sends no prompt.
//!
//! `cargo run -p opendesigner-agent --example acp_probe -- gemini --acp`
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

use od_agent::acp::{AcpSession, AgentCommand, McpEndpoint};

#[tokio::main]
async fn main() {
    let mut args = std::env::args().skip(1);
    let Some(program) = args.next() else {
        println!("usage: acp_probe <agent command> [args...]");
        return;
    };
    let command = AgentCommand {
        program,
        args: args.collect(),
        cwd: std::env::temp_dir(),
    };
    let mcp = McpEndpoint {
        url: "http://127.0.0.1:45790/mcp".into(),
        token: "probe-token".into(),
    };
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    match AcpSession::start(&command, &mcp, tx).await {
        Ok(session) => {
            println!("session opened");
            session.shutdown().await;
        }
        Err(e) => println!("setup failed: {e}"),
    }
}
