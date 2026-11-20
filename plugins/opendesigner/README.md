# OpenDesigner agent plugin

This plugin teaches a coding agent how to work with [OpenDesigner](https://github.com/skepjandi/opendesigner),
the desktop editor for App Store, Google Play and web assets and videos. It ships **the skill and nothing
else**: no MCP endpoint, no token, no hooks, and nothing that edits your agent's configuration. The editor's
installer (`opendesigner skill install`, or the Connect tab in the app) stays the explicit way to put the skill
in `~/.claude/skills` or `~/.agents/skills`; this plugin is the same skill for agents that install through a
plugin marketplace.

The files under `skills/opendesigner/` are generated from the editor's tool schema
(`scripts/package-agent-plugin.sh`) and a test fails when they differ from the generator, so the skill always
matches the tools the app serves.

## 1. Install the plugin

Claude Code:

```text
/plugin marketplace add skepjandi/opendesigner
/plugin install opendesigner@opendesigner
```

Codex reads `.codex-plugin/plugin.json` in this folder; add this repository as a plugin source the way Codex's
plugin documentation describes. [INFERENCE] Neither flow has been run against a live Claude Code or Codex
here; the manifests follow the published formats.

## 2. Connect the editor's MCP server (required, and yours to do)

The skill is only instructions. The tools it talks about come from the editor's local MCP server, which needs the
app running and a **per-install bearer token** that exists only on your machine. The plugin cannot and does not
carry it.

1. Start the OpenDesigner app (or, headless, `opendesigner serve-mcp <project>`). The server listens on
   `127.0.0.1` only.
2. Open the **Connect** tab, pick your agent, and copy the command it shows. For Claude Code it looks like:

   ```text
   claude mcp add --transport http opendesigner http://127.0.0.1:<port>/mcp --header "Authorization: Bearer <your token>"
   ```

   `opendesigner serve-mcp` prints the same command (add `--show-token` to print the token itself).
3. Start your agent. `get_guidance` and `apply_ops` should be listed as tools of the `opendesigner` server. If
   they are not, the skill tells the agent to ask you to start the editor and connect it.

Rotate the token any time (`opendesigner serve-mcp --rotate-token`); agents must then be reconnected.

## Maintaining

```bash
scripts/package-agent-plugin.sh          # regenerate the skill and manifests
scripts/package-agent-plugin.sh --check  # exit 2 when the committed files are stale
```
