//! The MCP servers the user added for the in-app chat: what the window shows, what the user
//! types, and what is written to the app's config folder
//! (`docs/wiki/agent-integration.md#extra-mcp-servers`).
//!
//! Only the user writes this list. An agent cannot add or change a server: the MCP tools know
//! nothing about it.

use od_agent::mcp_servers::{self, EnvVar, McpServer};
use od_agent::token;
use serde_json::{Value, json};

use crate::chat::split_command;

type Res<T> = Result<T, String>;

/// The servers as the window shows them: the variables by name, never by value.
pub fn list_json() -> String {
    Value::Array(
        load()
            .unwrap_or_default()
            .iter()
            .map(|server| {
                json!({
                    "name": server.name,
                    "command": command_line(server),
                    "env": server.env_names(),
                })
            })
            .collect(),
    )
    .to_string()
}

/// The command of a server as one line, the way the user typed it.
fn command_line(server: &McpServer) -> String {
    let mut parts = vec![server.command.clone()];
    parts.extend(server.args.iter().map(|arg| {
        if arg.contains(char::is_whitespace) {
            format!("\"{arg}\"")
        } else {
            arg.clone()
        }
    }));
    parts.join(" ")
}

/// The server the user described, checked before it is written.
pub fn parse(name: &str, command: &str, env: &str, stored: &[EnvVar]) -> Res<McpServer> {
    if command.trim().is_empty() {
        return Err("type the command that starts the server".into());
    }
    let (program, args) = split_command(command)?;
    let server = McpServer {
        name: name.trim().to_string(),
        command: program,
        args,
        env: mcp_servers::parse_env(env, stored)?,
    };
    mcp_servers::validate(&server)?;
    Ok(server)
}

/// Adds the server, or replaces the one with that name. `previous` is the name it had before, so
/// a rename does not leave the old one behind.
pub fn save(previous: &str, name: &str, command: &str, env: &str) -> Res<()> {
    let dir = config_dir()?;
    let previous = previous.trim();
    let stored = mcp_servers::load(&dir).map_err(|e| e.to_string())?;
    let kept = stored
        .iter()
        .find(|server| server.name == previous)
        .map(|server| server.env.clone())
        .unwrap_or_default();
    let server = parse(name, command, env, &kept)?;
    let renamed = !previous.is_empty() && previous != server.name;
    mcp_servers::upsert(&dir, server).map_err(|e| e.to_string())?;
    if renamed {
        mcp_servers::remove(&dir, previous).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Removes the server with that name.
pub fn remove(name: &str) -> Res<()> {
    let dir = config_dir()?;
    mcp_servers::remove(&dir, name.trim()).map_err(|e| e.to_string())?;
    Ok(())
}

/// The servers the user added.
pub fn load() -> Res<Vec<McpServer>> {
    let dir = config_dir()?;
    mcp_servers::load(&dir).map_err(|e| e.to_string())
}

/// The app's own config folder.
fn config_dir() -> Res<std::path::PathBuf> {
    token::config_dir().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_is_read_from_what_the_user_typed() {
        let server = parse(
            "minimax-mcp",
            "npx -y minimax-mcp",
            "MINIMAX_API_KEY=secret",
            &[],
        )
        .unwrap();
        assert_eq!(server.name, "minimax-mcp");
        assert_eq!(server.command, "npx");
        assert_eq!(
            server.args,
            vec!["-y".to_string(), "minimax-mcp".to_string()]
        );
        assert_eq!(server.env[0].name, "MINIMAX_API_KEY");
        assert_eq!(server.env[0].value, "secret");
    }

    #[test]
    fn a_bad_name_or_command_is_refused() {
        assert!(parse("bad/name", "npx", "", &[]).is_err());
        assert!(parse("", "npx", "", &[]).is_err());
        assert!(parse("fine", "", "", &[]).is_err());
    }

    #[test]
    fn a_variable_keeps_the_value_that_was_stored() {
        let stored = vec![EnvVar {
            name: "MINIMAX_API_KEY".into(),
            value: "secret".into(),
        }];
        let server = parse("minimax-mcp", "npx", "MINIMAX_API_KEY", &stored).unwrap();
        assert_eq!(server.env[0].value, "secret");
    }

    #[test]
    fn the_command_is_shown_as_one_line() {
        let server = McpServer {
            name: "one".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "a server".into()],
            env: Vec::new(),
        };
        assert_eq!(command_line(&server), "npx -y \"a server\"");
    }
}
