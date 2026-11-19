# Agent Integration

> **Status: Partially implemented.** The MCP server and the skill generator/installer (`crates/agent`, `opendesigner skill …`) are built and tested. The Qt app runs the MCP server in-process, has a Connect tab that previews and installs the skill, and a Chat tab (see [In-app chat](#in-app-chat)); an automatic first-run offer to install the skill is not built, and nothing here has been tried against a real Claude Code, Codex, Gemini CLI or opencode. Skill locations for the supported agents were verified against each agent's own docs on 2026-11-19 (see the target table).

The point of OpenDesigner is that the user guides an agent *with the GUI*. Three mechanisms carry that, and the app installs the first two on first run.

| Mechanism | What it is | Who uses it |
|-----------|------------|-------------|
| **Skill** | An [Agent Skill](https://agentskills.io/specification) (`SKILL.md` plus references) teaching an agent how to work with OpenDesigner | CLI agents the user already runs |
| **MCP server** | A local HTTP MCP endpoint exposing the editor's operations and the user's guidance context | The same agents, after the skill points them at it |
| **In-app chat** | A chat panel that drives an agent over the Agent Client Protocol | Users who would rather not leave the app |

## Why a skill and an MCP server

Kino shows the shape: an MCP server inside the desktop app (`http://127.0.0.1:45680/mcp`, "keep the desktop app open while the agent works"; marketed as 40 tools) that Claude Code, Codex, ChatGPT, Cursor and others connect to with one command, and skills such as a brand-clone skill that tell the agent *how* to edit well. Penpot and Figma both ship MCP servers too (Penpot's runs through its Plugin API; Figma's remote server can write to the canvas). A tool surface alone leaves the agent to guess conventions; a skill alone cannot touch the document. We ship both, and add what none of them has as a first-class idea: the user's **pointing** as input ([Guidance context](#guidance-context)).

## The skill

Layout, following the Agent Skills specification (`name` equals the directory name, lowercase and hyphens; `description` says when to use it):

```
opendesigner/
├── SKILL.md                     # when to use OpenDesigner; connect, read guidance first, work in ops
└── references/
    ├── tools.md                 # the MCP tool list with examples
    ├── store-specs.md           # generated from presets: sizes and rules per store slot
    └── style-guide.md           # how to write headlines, spacing, contrast for store art
```

`SKILL.md` is short (the guidance is tier 2 of progressive disclosure, under ~5000 tokens) and tells the agent to: confirm the app is running and the MCP server answers; call `get_guidance` before anything else; prefer small operations over regenerating a frame; end every turn by describing what changed in the user's terms; never export without running `validate_export`. The text is generated from the same tool schemas and presets the app uses, so it cannot drift from what the server accepts. The skill carries the app version that wrote it.

### The installer

On first run (and from Settings → Agents) the app detects agents and offers to install.

1. **Detect.** Look for each agent's configuration or skills directory. Detection reads directory existence only.
2. **Preview.** Show every path that will be written, per agent, and let the user uncheck any.
3. **Write.** Write only `<skills dir>/opendesigner/`. Write the cross-client directory once, and native directories for agents that do not scan it.
4. **Record.** Store a manifest of written files and hashes in the app's data directory.
5. **Update and uninstall.** On app update, rewrite files whose hash still matches the manifest; if the user edited a file, leave it and say so. Uninstall removes exactly the manifest's files.

It never edits an agent's config, credentials or history, never overwrites a skill named `opendesigner` it did not create without consent, and registers the MCP endpoint only by showing the user the one-line command or settings snippet for their agent, or by running that agent's documented `mcp add` command after consent (see [Security](./security.md#skill-installer)).

### Target table

| Target id | Directory (under the user's home) | Read by | Verified against |
|-----------|-----------------------------------|---------|------------------|
| `agents` | `~/.agents/skills/` | Codex CLI, Gemini CLI, opencode | Codex docs list `$HOME/.agents/skills` as the user location; Gemini CLI docs list `~/.agents/skills/` as an alias of `~/.gemini/skills/`; opencode reads `~/.agents/skills` for compatibility (all read 2026-11-19) |
| `claude-code` | `~/.claude/skills/` | Claude Code | Claude Code docs: personal skills at `~/.claude/skills/<skill-name>/SKILL.md` (read 2026-11-19). Claude Code does not list `~/.agents/skills`, so it needs its own row |
| Cursor, VS Code, others | not supported | | Not verified. VS Code reads project-level `.agents/skills/` per the Agent Skills quickstart; a project-level install is not built |

The installer writes one directory per target, so an install for Codex, Gemini CLI and opencode is a single `~/.agents/skills/opendesigner/`. Gemini CLI's docs say unpaid-tier and Google One users moved to Antigravity CLI on 2026-06-18; where Antigravity looks for skills was not checked.

### Implemented

`crates/agent/src/skill.rs` and `opendesigner skill status|install|uninstall`:

- **Generated, not hand-written:** `SKILL.md` (Agent Skills frontmatter: `name`, `description` under 1024 characters, `license`, `compatibility`, `metadata.version`), `references/tools.md` (from the live tool definitions), `references/store-specs.md` (from the presets, including which are not yet checked against their source) and `references/style-guide.md`. Tests fail if a tool or preset is missing from the references.
- **Previewed:** `install` and `uninstall` print every path and write nothing without `--yes`. Targets default to agents it detects (by config folder).
- **Reversible and conservative:** a manifest (`.opendesigner-install.json`, files and SHA-256) lives inside the skill folder. Updates rewrite only files that still match it; a file the user edited is kept and reported. Uninstall removes exactly the recorded, unedited files. A folder named `opendesigner` that has no manifest is refused unless `--overwrite-foreign`; a symlinked skill folder or file is never written through.
- **Does not connect the agent:** it never edits an agent's config. `opendesigner serve-mcp` prints the one-line connect command instead.

Not built: the first-run offer inside the app, project-level installs, an update check when the app updates.

Each row is data in `agent`'s target table, not code, so adding an agent is a one-row change with a test that the installer writes only inside that row's directory.

## The MCP server

### Implemented

`crates/agent`: `mcp.rs` (JSON-RPC 2.0 and the MCP lifecycle, transport-free, so it is tested without a socket), `tools.rs` (the tools), `server.rs` (the loopback HTTP transport), `token.rs` (the token and its file). `opendesigner serve-mcp <project>` runs it over a project folder and prints the connect command for Claude Code.

- **Transport:** Streamable HTTP as in the 2025-06-18 spec, JSON responses only. `POST /mcp` carries one JSON-RPC message; notifications get `202`; `GET` is `405` (no server-to-client stream); `DELETE` ends a session. Versions `2025-06-18`, `2025-03-26` and `2024-11-05` are negotiated, and an unsupported `MCP-Protocol-Version` header is `400`. Default port 45790, falling forward when taken.
- **Sessions and attribution:** `initialize` mints a session id (`Mcp-Session-Id`) and names the agent from `clientInfo.name`; every change the session makes is attributed to `agent:<name>` in the history.
- **Turns and checkpoints:** the first `apply_ops` of a turn (the `turn` argument, or one per call when omitted) first takes a checkpoint named `before: <summary>`; later calls in the same turn reuse it, and the result reports `checkpoint_before`. This is what lets the user restore "the state before the agent's turn".
- **Tools:** `get_guidance`, `get_selection`, `get_annotations`, `resolve_annotation`, `list_pages`, `get_node`, `get_tree`, `find_nodes`, `list_tokens`, `apply_ops`, `list_checkpoints`, `diff_checkpoint`, `restore_checkpoint`, `list_export_presets`, `list_media`, `list_locales`, `device_template`, `render_preview` (returns a PNG image block), `export`, `export_set`, `export_panorama`, `validate_export`. Arguments are closed types (`deny_unknown_fields`); a rejected call returns `isError: true` with the reason and changes nothing. Translations are `set_localized_text` operations in `apply_ops`; `export`, `export_set` and `export_panorama` take a `locale`.
- **Writes stay in the project:** `export`, `export_set`, `export_panorama` and `validate_export` take plain file names (or a name stem for sets and panoramas: letters, digits, `. _ -`) under `<project>/exports/`; separators, leading dots and `..` are refused. No tool takes another path. A language tag is `en`, `ja`, `pt-BR` (letters and digits, at most 20 characters), so it is safe inside a file name.
- **Media:** an image node's `image` must be `media/<plain file name>` (`.png`, `.jpg`, `.jpeg`, `.webp`); `core` refuses anything else, and the renderer refuses symlinks. Files get into `media/` only when the **user** imports them (the editor's Image button or a drop on the canvas copies the file and sniffs its first bytes). There is deliberately no agent `import_media`: a tool that copies a path from the user's disk would read outside the project (invariant 5). `list_media` shows the agent what is there, with pixel sizes, so it can size and place nodes; to use a new image it asks the user to add it.
- **Change hook:** `Handler::with_change_hook` reports the nodes an agent touched, for the editor to outline them. Reads do not fire it.

Deviations: `add_annotation` is not offered to agents, and there is no `transcribe` or timeline tool. Design tokens change through `apply_ops` with `set_token` (attributed, checkpointed and undoable like any edit); there is no separate token tool.

### Design (as originally specified)

- Streamable HTTP on `127.0.0.1`, fixed default port with fallback, **bearer token** per install, `Host` and `Origin` checks, off until enabled. See [Security](./security.md#mcp-endpoint).
- Tools are thin wrappers over `core` operations; the schema is generated from the operation types. Read tools are free; write tools return the changed node ids and the new checkpoint id.
- Limits per call: operations count, payload size, rendered-image size. Every rejection carries a reason.

### Tool surface (intended)

| Group | Tools |
|-------|-------|
| Guidance | `get_guidance` (selection, annotations, viewport, active page), `get_selection`, `get_annotations`, `resolve_annotation` |
| Read | `list_pages`, `get_node`, `get_tree`, `find_nodes`, `render_preview` (PNG of a node or region, so the agent can *look*), `list_tokens`, `list_media`, `list_fonts` |
| Write | `apply_ops`, `create_page`, `set_tokens` (no `import_media`: the user imports files, see above) |
| History | `list_checkpoints`, `diff_checkpoints`, `restore_checkpoint` |
| Store | `list_export_presets`, `export`, `export_set`, `export_panorama`, `device_template`, `validate_export` ([Store Assets](./store-assets.md)) |
| Video | `get_timeline`, `apply_timeline_ops`, `render_frame`, `transcribe` `[INFERENCE: scope decided in Phase 5]` ([Video Editing](./video-editing.md)) |

## Guidance context

This is the feature the whole app exists for. The editor maintains, and exposes to agents, what the user is pointing at:

- **Selection.** Node ids, with each node's data and a rendered crop, so "make this bigger" is unambiguous.
- **Annotations.** The user drops a pin or draws a box on the canvas (or a timeline range) and types a note. An annotation is `{ id, target (node id or canvas rect), note, status }`. The agent reads open annotations, works them, and marks them resolved; the user sees pins close.
- **Hand edits as signal.** When the user fixes what the agent did, the operations are in the log with `actor: user`; the agent can read them to learn the correction (`get_guidance` includes the last user operations after the agent's last checkpoint).
- **Reference.** The user can attach a reference image or another project's frame ("make it feel like this") that the agent can fetch.

The agent's output is shown, not applied silently: changed nodes are outlined, a one-line summary of each turn sits in the chat/history panel, and a checkpoint exists to restore.

## In-app chat

> **Status: Partially implemented.** Built: an ACP v1 client (`od_agent::acp`), the Chat tab, and tests against a scripted agent. **Not verified against any real agent** (Claude Code, Codex or Gemini CLI adapters were not installed or signed in here), so treat the presets below as unproven.

A chat panel (Kino's "chat panel in your project") that drives an agent over the **Agent Client Protocol**: the user picks the agent they already use and are signed in to (their own subscription); OpenDesigner starts it as a child process and shows its turns, tool calls and permission requests inline.

- **Protocol.** ACP version 1: newline-delimited JSON-RPC 2.0 over the child's stdio (`initialize`, `session/new`, `session/prompt`, `session/cancel`, `session/update` notifications, `session/request_permission`). An agent that answers with another version is refused with a message saying so. Lines that are not JSON are ignored (some agents print a banner). ACP is changing (a v2 draft exists, and adapter package names have been renamed more than once), so this is the part most likely to need updating.
- **What the agent is given.** `session/new` carries the project folder as the working directory and one MCP server: the editor's own, over HTTP, with the bearer token in an `Authorization` header (the agent must advertise `mcpCapabilities.http`, or the chat says it cannot connect). The agent therefore edits the design through `apply_ops` like any other agent: attributed, checkpointed, outlined. The first prompt of a conversation carries a short preamble telling it to call `get_guidance`, which returns the selection, the pinned notes and the user's recent hand edits.
- **What the client serves.** Nothing: no file-system or terminal capability is declared, and a request for either is answered with "method not supported". Agents do their own file and shell work in their own process; their own permission prompts arrive as `session/request_permission` and are shown with the agent's options (Allow, Reject, ...) for the user to pick, or Cancel.
- **What starts.** Only the command the user typed or picked in the panel (a program and arguments, split on spaces with quotes, **no shell**); it is remembered next to the connection token. No project file and no agent can set it. Presets in the panel: `npx -y @agentclientprotocol/claude-agent-acp`, `npx -y @agentclientprotocol/codex-acp`, `gemini --acp` (from the adapters' published names and Gemini CLI's documentation; unchecked here).
- **Credentials.** The agent signs in on its own; OpenDesigner never sees its credentials. The agent does receive the local MCP token, which only opens the loopback server of this app.
- **Not built:** plans, images in prompts, session resume and a session list, several conversations, per-turn checkpoints named after the chat (the checkpoint comes from the MCP `apply_ops` turn as for any agent).

## Headless

`opendesigner` CLI runs the same operations against a project folder without a window: `export`, `validate`, `apply` (a file of operations), `serve-mcp`. Used in CI and by agents on machines with no display (Kino documents headless use on Linux servers; OpenCut lists a headless mode in its plans).
