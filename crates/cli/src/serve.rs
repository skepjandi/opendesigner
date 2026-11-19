use std::path::Path;
use std::sync::{Arc, Mutex};

use od_agent::{Handler, token};
use od_core::Workspace;

use crate::CliError;

/// Runs the MCP server for a project until Ctrl-C.
pub fn run(
    dir: &Path,
    port: u16,
    show_token: bool,
    rotate: bool,
    agent: &str,
) -> Result<(), CliError> {
    let workspace = Workspace::open(dir)?;
    let token_file = token::token_path()?;
    let secret = if rotate {
        token::rotate(&token_file)?
    } else {
        token::load_or_create(&token_file)?
    };
    let handler = Arc::new(Handler::new(Arc::new(Mutex::new(workspace))));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| CliError::Invalid(format!("cannot start the runtime: {e}")))?;
    runtime.block_on(async move {
        let server = od_agent::start(handler, secret.clone(), port).await?;
        let url = server.url();
        println!("OpenDesigner MCP server for {} on {url}", dir.display());
        println!(
            "Token file: {} (readable by you only)",
            token_file.display()
        );
        if show_token {
            println!("Token: {secret}");
        }
        println!();
        println!(
            "{}",
            connection_help(&url, &token_file.display().to_string(), agent)?
        );
        println!("Every change an agent makes is attributed to it, checkpointed and reversible.");
        println!("Press Ctrl-C to stop.");
        tokio::signal::ctrl_c()
            .await
            .map_err(|e| CliError::Invalid(format!("cannot wait for Ctrl-C: {e}")))?;
        server.stop().await;
        Ok::<(), CliError>(())
    })
}

/// The instructions for `agent`, or an error naming the agents that have some.
fn connection_help(url: &str, token_file: &str, agent: &str) -> Result<String, CliError> {
    let recipes = od_agent::connect::recipes(url, token_file);
    match recipes.iter().find(|r| r.id == agent) {
        Some(r) => Ok(format!(
            "Connect {} ({}):\n{}\n\nOther agents: --agent {}",
            r.label,
            r.place,
            r.text,
            recipes.iter().map(|r| r.id).collect::<Vec<_>>().join(" | ")
        )),
        None => Err(CliError::Invalid(format!(
            "unknown agent {agent:?}; choose one of: {}",
            recipes.iter().map(|r| r.id).collect::<Vec<_>>().join(", ")
        ))),
    }
}
