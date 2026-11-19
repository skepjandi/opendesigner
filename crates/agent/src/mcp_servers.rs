//! The MCP servers the user adds for the in-app chat: programs of their own that the agent may
//! call (speech models, a search API, a database). The list lives in the app's config folder and
//! is sent to the agent with the session, in the ACP stdio form
//! (`docs/wiki/agent-integration.md#extra-mcp-servers`).
//!
//! Only the user writes this list: the editor never lets an agent add or change a server, and the
//! MCP tools know nothing about it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::AgentError;

/// The file in the config folder that holds the list.
pub const FILE_NAME: &str = "mcp-servers.json";

/// The longest name a server may have.
const MAX_NAME: usize = 40;

/// One MCP server the user added.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServer {
    /// How the server is called in the chat: letters, digits, spaces, `-` and `_`.
    pub name: String,
    /// The program that starts it.
    pub command: String,
    /// The arguments it is started with.
    #[serde(default)]
    pub args: Vec<String>,
    /// The variables it is started with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<EnvVar>,
}

/// One environment variable for a server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvVar {
    pub name: String,
    pub value: String,
}

impl McpServer {
    /// The names of the variables, never their values: the window shows the names only.
    pub fn env_names(&self) -> Vec<&str> {
        self.env.iter().map(|v| v.name.as_str()).collect()
    }
}

/// The file the list lives in.
pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE_NAME)
}

/// Reads the list. A file that is not there is an empty list; a file that cannot be read is an
/// error the user is told about.
pub fn load(config_dir: &Path) -> Result<Vec<McpServer>, AgentError> {
    let file = path(config_dir);
    let text = match fs::read_to_string(&file) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(AgentError::io(&file, e)),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&text).map_err(|e| AgentError::Config(format!("{}: {e}", file.display())))
}

/// Writes the list. The file may hold a key, so only the user can read it.
pub fn save(config_dir: &Path, servers: &[McpServer]) -> Result<(), AgentError> {
    let file = path(config_dir);
    fs::create_dir_all(config_dir).map_err(|e| AgentError::io(config_dir, e))?;
    let text =
        serde_json::to_string_pretty(servers).map_err(|e| AgentError::Config(e.to_string()))?;
    // written beside the file and moved into place, so a crash cannot leave half a list
    let temp = file.with_extension("json.tmp");
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut handle = options.open(&temp).map_err(|e| AgentError::io(&temp, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        handle
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| AgentError::io(&temp, e))?;
    }
    handle
        .write_all(text.as_bytes())
        .map_err(|e| AgentError::io(&temp, e))?;
    handle.sync_all().map_err(|e| AgentError::io(&temp, e))?;
    fs::rename(&temp, &file).map_err(|e| AgentError::io(&file, e))?;
    Ok(())
}

/// Adds the server, or replaces the one with the same name.
pub fn upsert(config_dir: &Path, server: McpServer) -> Result<Vec<McpServer>, AgentError> {
    validate(&server).map_err(AgentError::Config)?;
    let mut servers = load(config_dir)?;
    match servers.iter_mut().find(|s| s.name == server.name) {
        Some(slot) => *slot = server,
        None => servers.push(server),
    }
    save(config_dir, &servers)?;
    Ok(servers)
}

/// Removes the server with that name.
pub fn remove(config_dir: &Path, name: &str) -> Result<Vec<McpServer>, AgentError> {
    let mut servers = load(config_dir)?;
    servers.retain(|s| s.name != name);
    save(config_dir, &servers)?;
    Ok(servers)
}

/// Checks what the user typed; the message is what they are told.
pub fn validate(server: &McpServer) -> Result<(), String> {
    let name = server.name.trim();
    if name.is_empty() {
        return Err("give the server a name".into());
    }
    if name.chars().count() > MAX_NAME {
        return Err(format!("the name is longer than {MAX_NAME} characters"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-' || c == '_')
    {
        return Err("the name may hold letters, digits, spaces, - and _ only".into());
    }
    if server.command.trim().is_empty() {
        return Err("type the command that starts the server".into());
    }
    if server.command.contains(['\n', '\r', '\0']) {
        return Err("the command has a line break in it".into());
    }
    for arg in &server.args {
        if arg.contains(['\n', '\0']) {
            return Err("an argument has a line break in it".into());
        }
    }
    for var in &server.env {
        if !is_env_name(&var.name) {
            return Err(format!(
                "{:?} is not a valid environment variable name",
                var.name
            ));
        }
        if var.value.contains('\0') {
            return Err(format!("the value of {} has a null byte in it", var.name));
        }
    }
    Ok(())
}

/// Reads the variables from the lines the user typed, one `KEY=value` per line. A line with only
/// a name keeps the value already stored for it; a line with a name and a value sets it.
pub fn parse_env(lines: &str, stored: &[EnvVar]) -> Result<Vec<EnvVar>, String> {
    let mut out: Vec<EnvVar> = Vec::new();
    for line in lines.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (name, value) = match line.split_once('=') {
            Some((name, value)) => (name.trim(), Some(value.trim())),
            None => (line, None),
        };
        if !is_env_name(name) {
            return Err(format!("{name:?} is not a valid environment variable name"));
        }
        if out.iter().any(|v| v.name == name) {
            return Err(format!("{name} is listed twice"));
        }
        let value = match value {
            Some(value) => value.to_string(),
            None => stored
                .iter()
                .find(|v| v.name == name)
                .map(|v| v.value.clone())
                .ok_or_else(|| format!("{name} has no value yet: write {name}=..."))?,
        };
        out.push(EnvVar {
            name: name.to_string(),
            value,
        });
    }
    Ok(out)
}

/// The server as the agent's session is told about it: the ACP stdio transport
/// (`{"type": "stdio", "name", "command", "args", "env"}`).
pub fn to_acp(server: &McpServer) -> serde_json::Value {
    // The v1 shape of a stdio server: name, command, args and env, and nothing else. (The v2
    // draft adds "type": "stdio" and a working directory; this client speaks v1, and the
    // session's working directory is the project folder anyway.)
    serde_json::json!({
        "name": server.name,
        "command": server.command,
        "args": server.args,
        "env": server.env.iter().map(|v| serde_json::json!({
            "name": v.name, "value": v.value,
        })).collect::<Vec<_>>(),
    })
}

/// Whether a name may be used for an environment variable.
fn is_env_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(name: &str) -> McpServer {
        McpServer {
            name: name.to_string(),
            command: "npx".into(),
            args: vec!["-y".into(), "minimax-mcp".into()],
            env: Vec::new(),
        }
    }

    #[test]
    fn the_list_round_trips_through_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        assert_eq!(load(dir).unwrap(), vec![]);

        let mut with_env = server("minimax-mcp");
        with_env.env = vec![EnvVar {
            name: "MINIMAX_API_KEY".into(),
            value: "secret".into(),
        }];
        let saved = upsert(dir, with_env.clone()).unwrap();
        assert_eq!(saved, vec![with_env.clone()]);
        assert_eq!(load(dir).unwrap(), vec![with_env]);

        // a second server, then the first one again: the list is replaced, not duplicated
        upsert(dir, server("other")).unwrap();
        assert_eq!(load(dir).unwrap().len(), 2);
        upsert(dir, server("minimax-mcp")).unwrap();
        let list = load(dir).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "minimax-mcp");

        assert_eq!(remove(dir, "other").unwrap().len(), 1);
        assert_eq!(load(dir).unwrap()[0].name, "minimax-mcp");
    }

    #[test]
    fn the_file_is_only_the_user_who_can_read_it() {
        let tmp = tempfile::tempdir().unwrap();
        upsert(tmp.path(), server("one")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(path(tmp.path())).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn bad_names_and_commands_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        for bad in ["", "   ", "a/b", "name?", "x".repeat(41).as_str()] {
            let mut s = server(bad);
            s.name = bad.to_string();
            assert!(validate(&s).is_err(), "{bad:?} should be refused");
            assert!(
                upsert(tmp.path(), s).is_err(),
                "{bad:?} should not be saved"
            );
        }
        assert!(load(tmp.path()).unwrap().is_empty());

        let mut s = server("fine");
        s.command = "  ".into();
        assert!(validate(&s).is_err());
        s.command = "npx".into();
        s.env = vec![EnvVar {
            name: "BAD NAME".into(),
            value: "x".into(),
        }];
        assert!(validate(&s).is_err());
    }

    #[test]
    fn the_variables_are_read_one_per_line_and_keep_what_was_stored() {
        let stored = vec![EnvVar {
            name: "MINIMAX_API_KEY".into(),
            value: "secret".into(),
        }];
        let parsed = parse_env("MINIMAX_API_KEY\nOTHER=1\n", &stored).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].value, "secret");
        assert_eq!(parsed[1].value, "1");

        assert!(parse_env("MINIMAX_API_KEY", &[]).is_err());
        assert!(parse_env("BAD NAME=1", &[]).is_err());
        assert!(parse_env("A=1\nA=2", &[]).is_err());
        assert_eq!(parse_env("# a comment\n\n", &[]).unwrap(), vec![]);
    }

    #[test]
    fn the_agent_is_told_about_a_server_in_the_stdio_form() {
        let mut s = server("minimax-mcp");
        s.env = vec![EnvVar {
            name: "MINIMAX_API_KEY".into(),
            value: "secret".into(),
        }];
        let value = to_acp(&s);
        // the v1 shape: no "type" field (that came with the v2 draft)
        assert!(value.get("type").is_none());
        assert_eq!(value["name"], "minimax-mcp");
        assert_eq!(value["command"], "npx");
        assert_eq!(value["args"][1], "minimax-mcp");
        assert_eq!(value["env"][0]["name"], "MINIMAX_API_KEY");
        assert_eq!(value["env"][0]["value"], "secret");
        assert!(value.get("cwd").is_none());
    }
}
