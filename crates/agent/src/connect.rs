//! How to point each coding agent at the editor's local MCP server
//! (`docs/wiki/agent-integration.md#connecting-an-agent`).
//!
//! The editor never edits an agent's configuration (AGENTS.md invariant 6): it shows what to
//! paste, per agent. No recipe contains the token itself; each reads it from the token file when
//! the agent starts, or names an environment variable to fill from that file.

use serde::Serialize;
use serde_json::json;

/// The environment variable the recipes that cannot read a file use for the token.
pub const TOKEN_ENV: &str = "OPENDESIGNER_MCP_TOKEN";

/// One agent's instructions for connecting to the server.
#[derive(Debug, Clone, Serialize)]
pub struct Recipe {
    pub id: &'static str,
    pub label: &'static str,
    /// Where `text` goes, or what to run.
    pub place: &'static str,
    /// The command or file contents. Shell syntax is for macOS and Linux shells.
    pub text: String,
    /// What was actually run against this server with the real agent, or why it was not.
    pub checked: &'static str,
}

/// Single-quotes `s` for a POSIX shell.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// Recipes for the server at `url` whose bearer token is stored in `token_file`.
pub fn recipes(url: &str, token_file: &str) -> Vec<Recipe> {
    let file = shell_quote(token_file);
    let read_token = format!("$(cat {file})");
    let omp_header = format!("!printf 'Bearer %s' \"{read_token}\"");
    vec![
        Recipe {
            id: "omp",
            label: "omp",
            place: "Save as .omp/mcp.json in your project (or ~/.omp/agent/mcp.json for every project), then /mcp reload",
            text: pretty(&json!({"mcpServers": {"opendesigner": {
                "type": "http", "url": url, "headers": {"Authorization": omp_header}}}})),
            checked: "omp 19.0.0: a real turn called the tools through this file and a project edit was attributed to it.",
        },
        Recipe {
            id: "claude-code",
            label: "Claude Code",
            place: "Run once in a terminal",
            text: format!(
                "claude mcp add --transport http opendesigner {url} --header \"Authorization: Bearer {read_token}\""
            ),
            checked: "Claude Code 2.1.318 connected and listed the tools (no model turn: it was not signed in).",
        },
        Recipe {
            id: "codex",
            label: "Codex CLI",
            place: "Add to ~/.codex/config.toml, and start Codex from a shell that has the variable set",
            text: format!(
                "export {TOKEN_ENV}=\"{read_token}\"\n\n[mcp_servers.opendesigner]\nurl = \"{url}\"\nbearer_token_env_var = \"{TOKEN_ENV}\""
            ),
            checked: "Codex 0.142.0 read this configuration (`codex mcp list`); a connection was not tried (not signed in).",
        },
        Recipe {
            id: "gemini",
            label: "Gemini CLI",
            place: "Run once in a terminal (this copies the token into Gemini's settings file)",
            text: format!(
                "gemini mcp add -s user --transport http --header \"Authorization: Bearer {read_token}\" opendesigner {url}"
            ),
            checked: "Gemini CLI 0.38.2 wrote the entry; a connection was not tried (`gemini mcp list` printed nothing and it was not signed in).",
        },
        Recipe {
            id: "opencode",
            label: "opencode",
            place: "Save as opencode.json in your project, or merge into ~/.config/opencode/opencode.json",
            text: pretty(
                &json!({"$schema": "https://opencode.ai/config.json", "mcp": {"opendesigner": {
                "type": "remote", "url": url,
                "headers": {"Authorization": format!("Bearer {{file:{token_file}}}")}}}}),
            ),
            checked: "opencode 1.19.3: `opencode mcp list` showed it connected.",
        },
        Recipe {
            id: "cursor",
            label: "Cursor",
            place: "Save as .cursor/mcp.json in your project, and start Cursor from a shell that has the variable set",
            text: format!(
                "export {TOKEN_ENV}=\"{read_token}\"\n\n{}",
                pretty(&json!({"mcpServers": {"opendesigner": {
                    "url": url, "headers": {"Authorization": format!("Bearer ${{env:{TOKEN_ENV}}}")}}}}))
            ),
            checked: "Not checked: Cursor is not installed here; this follows its documented format.",
        },
        Recipe {
            id: "other",
            label: "Any MCP client",
            place: "Streamable HTTP server",
            text: format!(
                "URL:    {url}\nHeader: Authorization: Bearer <the contents of {token_file}>"
            ),
            checked: "The server speaks MCP 2025-06-18 over Streamable HTTP; clients that do the same work.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:45790/mcp";
    const FILE: &str = "/home/u/it's here/mcp-token";

    #[test]
    fn every_recipe_names_the_server_and_none_holds_the_token() {
        let all = recipes(URL, FILE);
        assert_eq!(all.len(), 7);
        for r in &all {
            assert!(r.text.contains(URL), "{} lacks the url", r.id);
            assert!(
                r.text.contains("mcp-token"),
                "{} does not read the token file",
                r.id
            );
        }
    }

    #[test]
    fn json_recipes_parse_and_hold_the_url() {
        let all = recipes(URL, FILE);
        for (id, key) in [("omp", "mcpServers"), ("opencode", "mcp")] {
            let r = all.iter().find(|r| r.id == id).unwrap();
            let v: serde_json::Value = serde_json::from_str(&r.text).unwrap();
            assert_eq!(v[key]["opendesigner"]["url"], URL, "{id}");
        }
    }

    #[test]
    fn a_quote_in_the_token_path_cannot_end_the_shell_word() {
        let all = recipes(URL, FILE);
        let claude = all.iter().find(|r| r.id == "claude-code").unwrap();
        assert!(claude.text.contains(r"'/home/u/it'\''s here/mcp-token'"));
    }
}
