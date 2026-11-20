//! The agent plugin: the generated Agent Skill and the manifests that let Claude Code and Codex
//! install it from a marketplace (`docs/wiki/agent-integration.md#agent-plugin`).
//!
//! The plugin carries the skill only (AGENTS.md invariant 6): no MCP endpoint, no token, no hooks,
//! nothing that edits an agent's configuration. Connecting the editor's MCP server stays the
//! explicit step the app shows. Every file is generated from the same schema as the installed
//! skill, and a test fails when the committed files differ from [`generate`].

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::error::AgentError;
use crate::skill::{self, SKILL_NAME};

/// Where the plugin lives, relative to the repository root.
pub const PLUGIN_DIR: &str = "plugins/opendesigner";
const MARKETPLACE_PATH: &str = ".claude-plugin/marketplace.json";
const AUTHOR: &str = "OpenDesigner contributors";
const DESCRIPTION: &str = "Teaches coding agents to design store assets, images and videos in the OpenDesigner editor over MCP. Ships the skill only; connect the editor's MCP server as the README describes.";
const KEYWORDS: [&str; 5] = ["design", "video", "app-store", "mcp", "opendesigner"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginFile {
    /// Relative to the repository root, forward slashes.
    pub path: String,
    pub contents: String,
}

fn json_file(path: String, value: &Value) -> PluginFile {
    let mut contents = serde_json::to_string_pretty(value).unwrap_or_default();
    contents.push('\n');
    PluginFile { path, contents }
}

fn common() -> serde_json::Map<String, Value> {
    let repository = env!("CARGO_PKG_REPOSITORY");
    let Value::Object(map) = json!({
        "name": SKILL_NAME,
        "version": env!("CARGO_PKG_VERSION"),
        "description": DESCRIPTION,
        "author": {"name": AUTHOR},
        "homepage": repository,
        "repository": repository,
        "license": env!("CARGO_PKG_LICENSE"),
        "keywords": KEYWORDS,
    }) else {
        return serde_json::Map::new();
    };
    map
}

fn claude_manifest() -> PluginFile {
    json_file(
        format!("{PLUGIN_DIR}/.claude-plugin/plugin.json"),
        &Value::Object(common()),
    )
}

fn codex_manifest() -> PluginFile {
    let mut manifest = common();
    manifest.insert("skills".into(), json!("./skills/"));
    json_file(
        format!("{PLUGIN_DIR}/.codex-plugin/plugin.json"),
        &Value::Object(manifest),
    )
}

fn marketplace() -> PluginFile {
    json_file(
        MARKETPLACE_PATH.to_string(),
        &json!({
            "name": SKILL_NAME,
            "owner": {"name": AUTHOR},
            "plugins": [{
                "name": SKILL_NAME,
                "source": format!("./{PLUGIN_DIR}"),
                "description": DESCRIPTION,
                "version": env!("CARGO_PKG_VERSION"),
                "category": "design",
                "keywords": KEYWORDS,
            }],
        }),
    )
}

/// Every generated file of the plugin: the manifests and the skill, relative to the repository
/// root. `PLUGIN_DIR/README.md` is hand-written and is not generated.
pub fn generate() -> Vec<PluginFile> {
    let mut files = vec![marketplace(), claude_manifest(), codex_manifest()];
    files.extend(skill::generate().into_iter().map(|f| PluginFile {
        path: format!("{}/{}", skill_dir(), f.path),
        contents: f.contents,
    }));
    files
}

fn skill_dir() -> String {
    format!("{PLUGIN_DIR}/skills/{SKILL_NAME}")
}

/// Regular files under `dir`, relative to `root`, without following symlinks (a symlink counts as
/// a file, so a stale one is reported and removed rather than read through).
fn existing(root: &Path, dir: &Path) -> Result<BTreeSet<String>, AgentError> {
    let mut found = BTreeSet::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(found);
    };
    for entry in entries {
        let entry = entry.map_err(|e| AgentError::io(dir, e))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| AgentError::io(&path, e))?;
        if kind.is_dir() {
            found.extend(existing(root, &path)?);
        } else if let Ok(rel) = path.strip_prefix(root) {
            found.insert(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(found)
}

/// How the files under `root` differ from [`generate`]: `missing`, `differs` and `extra` (files in
/// the skill folder that the generator no longer writes). Empty means the plugin is current.
pub fn check(root: &Path) -> Result<Vec<String>, AgentError> {
    let files = generate();
    let mut problems = Vec::new();
    for f in &files {
        match fs::read_to_string(root.join(&f.path)) {
            // a Windows checkout may have turned line feeds into CR LF
            Ok(text) if text.replace("\r\n", "\n") == f.contents => {}
            Ok(_) => problems.push(format!("differs: {}", f.path)),
            Err(_) => problems.push(format!("missing: {}", f.path)),
        }
    }
    let wanted: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for path in existing(root, &root.join(skill_dir()))? {
        if !wanted.contains(path.as_str()) {
            problems.push(format!("extra: {path}"));
        }
    }
    Ok(problems)
}

/// Writes every generated file under `root` and removes skill files the generator no longer
/// writes. Returns the paths written.
pub fn write(root: &Path) -> Result<Vec<PathBuf>, AgentError> {
    let files = generate();
    let wanted: BTreeSet<&str> = files.iter().map(|f| f.path.as_str()).collect();
    for path in existing(root, &root.join(skill_dir()))? {
        if !wanted.contains(path.as_str()) {
            let stale = root.join(&path);
            fs::remove_file(&stale).map_err(|e| AgentError::io(&stale, e))?;
        }
    }
    let mut written = Vec::with_capacity(files.len());
    for f in files {
        let path = root.join(&f.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| AgentError::io(parent, e))?;
        }
        fs::write(&path, f.contents).map_err(|e| AgentError::io(&path, e))?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn the_committed_plugin_is_exactly_what_the_generator_writes() {
        let problems = check(&repo_root()).unwrap();
        assert!(
            problems.is_empty(),
            "the plugin is stale; run scripts/package-agent-plugin.sh and commit the result:\n{}",
            problems.join("\n")
        );
    }

    #[test]
    fn manifests_carry_the_app_version_and_never_a_token_or_an_endpoint() {
        for f in generate().iter().filter(|f| f.path.ends_with(".json")) {
            let v: Value = serde_json::from_str(&f.contents).unwrap();
            assert!(f.contents.contains(env!("CARGO_PKG_VERSION")), "{}", f.path);
            let lower = f.contents.to_lowercase();
            for banned in [
                "token",
                "bearer",
                "127.0.0.1",
                "localhost",
                "mcpservers",
                "hooks",
            ] {
                assert!(!lower.contains(banned), "{} mentions {banned}", f.path);
            }
            assert!(v["name"] == SKILL_NAME || v["plugins"][0]["name"] == SKILL_NAME);
        }
    }

    #[test]
    fn the_plugin_holds_the_skill_and_manifests_only() {
        for f in generate() {
            let ok = f.path.starts_with(&format!("{}/", skill_dir()))
                || f.path.ends_with("plugin.json")
                || f.path == MARKETPLACE_PATH;
            assert!(ok, "unexpected plugin file {}", f.path);
        }
        assert!(
            generate()
                .iter()
                .any(|f| f.path.ends_with("skills/opendesigner/SKILL.md"))
        );
    }

    #[test]
    fn write_then_check_is_clean_and_stale_skill_files_are_removed() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path()).unwrap();
        assert!(check(tmp.path()).unwrap().is_empty());
        let stale = tmp.path().join(skill_dir()).join("references/old.md");
        fs::write(&stale, "x").unwrap();
        fs::write(tmp.path().join(skill_dir()).join("SKILL.md"), "edited").unwrap();
        let problems = check(tmp.path()).unwrap();
        assert!(problems.contains(
            &"extra: plugins/opendesigner/skills/opendesigner/references/old.md".to_string()
        ));
        assert!(
            problems.contains(
                &"differs: plugins/opendesigner/skills/opendesigner/SKILL.md".to_string()
            )
        );
        write(tmp.path()).unwrap();
        assert!(!stale.exists());
        assert!(check(tmp.path()).unwrap().is_empty());
    }
}
