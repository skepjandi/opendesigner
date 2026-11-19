# Agent Integration

> **Status: Partially implemented.** The MCP server, the skill generator/installer (`crates/agent`, `opendesigner skill …`), per-agent connection instructions (`od_agent::connect`) and the Chat tab are built and tested; the Qt app runs the MCP server in-process and has a Connect tab (a picker of agents with the exact thing to paste, and the skill install) and a Chat tab (see [In-app chat](#in-app-chat)). On the first start with an agent present and no skill installed, a dialog lists every file it would write and offers Install, Not now or Don't ask again (remembered in the config folder). The Connect tab also holds the MCP servers the user adds for the chat (see [Extra MCP servers](#extra-mcp-servers)). The design is agent-agnostic: MCP over Streamable HTTP and ACP are open protocols, and the skill goes where each agent reads skills. **Checked against real agents (2026-11-19):** with **omp 19.0.0**, signed in, a full turn through the in-app chat's own ACP client (`omp acp`) called `get_guidance`, `list_pages` and `apply_ops` on this server, created a frame and a text node, and the edit is attributed `agent:omp-coding-agent` in the project history; the same server was also read from a project `.omp/mcp.json` by `omp -p`. **opencode 1.19.3** listed the server as connected from an `opencode.json`, and `opencode acp` opened a chat session. Claude Code 2.1.318 connected over HTTP with the bearer token and listed the tools; Gemini CLI 0.38.2, the Claude adapter and the Codex adapter answered the chat's ACP handshake (Gemini and Codex with a sign-in error, which the chat reports). **Not checked:** a model turn with Claude Code, Codex or Gemini CLI (not signed in here), Codex, Gemini and Cursor connecting to MCP, and any of it on Windows or Linux. Skill locations were verified against each agent's docs on 2026-11-19 (see the target table).

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

On first run the app detects agents and, if one reads skills and ours is not there, shows a dialog with every path it would write and three choices: Install, Not now (asks again next start) and Don't ask again (`skill-offer-answered` in the config folder). The Connect tab does the same on demand. The offer never fires when the skill is already installed, and nothing is written without the click.

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
| `omp` | `~/.omp/agent/skills/` | omp | omp's skills doc: the native provider reads `~/.omp/agent/skills/<name>/SKILL.md`; it also reads `~/.agents/skills` and `~/.claude/skills` and drops an identical same-named copy, so installing both is harmless (read in omp's bundled docs 2026-11-19; not run against an installed copy) |
| `claude-code` | `~/.claude/skills/` | Claude Code | Claude Code docs: personal skills at `~/.claude/skills/<skill-name>/SKILL.md` (read 2026-11-19). Claude Code does not list `~/.agents/skills`, so it needs its own row |
| Cursor, VS Code, others | not supported | | Not verified. VS Code reads project-level `.agents/skills/` per the Agent Skills quickstart; a project-level install is not built |

The installer writes one directory per target, so an install for Codex, Gemini CLI and opencode is a single `~/.agents/skills/opendesigner/`. Gemini CLI's docs say unpaid-tier and Google One users moved to Antigravity CLI on 2026-06-18; where Antigravity looks for skills was not checked.

### Implemented

`crates/agent/src/skill.rs` and `opendesigner skill status|install|uninstall`:

- **Generated, not hand-written:** `SKILL.md` (Agent Skills frontmatter: `name`, `description` under 1024 characters, `license`, `compatibility`, `metadata.version`), `references/tools.md` (from the live tool definitions), `references/store-specs.md` (from the presets, including which are not yet checked against their source) and `references/style-guide.md`. Tests fail if a tool or preset is missing from the references.
- **Previewed:** `install` and `uninstall` print every path and write nothing without `--yes`. Targets default to agents it detects (by config folder).
- **Reversible and conservative:** a manifest (`.opendesigner-install.json`, files and SHA-256) lives inside the skill folder. Updates rewrite only files that still match it; a file the user edited is kept and reported. Uninstall removes exactly the recorded, unedited files. A folder named `opendesigner` that has no manifest is refused unless `--overwrite-foreign`; a symlinked skill folder or file is never written through.
- **Does not connect the agent:** it never edits an agent's config. `opendesigner serve-mcp` prints the one-line connect command instead.

Not built: project-level installs, an update check when the app updates.

Each row is data in `agent`'s target table, not code, so adding an agent is a one-row change with a test that the installer writes only inside that row's directory.

## The MCP server

### Implemented

`crates/agent`: `mcp.rs` (JSON-RPC 2.0 and the MCP lifecycle, transport-free, so it is tested without a socket), `tools.rs` (the tools), `server.rs` (the loopback HTTP transport), `token.rs` (the token and its file). `opendesigner serve-mcp <project>` runs it over a project folder and prints the connect command for Claude Code.

- **Transport:** Streamable HTTP as in the 2025-06-18 spec, JSON responses only. `POST /mcp` carries one JSON-RPC message; notifications get `202`; `GET` is `405` (no server-to-client stream); `DELETE` ends a session. Versions `2025-06-18`, `2025-03-26` and `2024-11-05` are negotiated, and an unsupported `MCP-Protocol-Version` header is `400`. Default port 45790, falling forward when taken.
- **Sessions and attribution:** `initialize` mints a session id (`Mcp-Session-Id`) and names the agent from `clientInfo.name`; every change the session makes is attributed to `agent:<name>` in the history.
- **Turns and checkpoints:** the first `apply_ops` of a turn (the `turn` argument, or one per call when omitted) first takes a checkpoint named `before: <summary>`; later calls in the same turn reuse it, and the result reports `checkpoint_before`. This is what lets the user restore "the state before the agent's turn".
- **Tools:** `get_guidance` (also returns the workspace's `brief`), `get_playbook`, `review_frame`, `list_formats`, `get_brief`, `set_format`, `get_selection`, `get_annotations`, `resolve_annotation`, `list_pages`, `get_node`, `get_tree`, `find_nodes`, `list_tokens`, `apply_ops`, `list_checkpoints`, `diff_checkpoint`, `restore_checkpoint`, `list_export_presets`, `list_media`, `list_memory`, `read_memory`, `write_memory`, `list_locales`, `device_template`, `render_preview` (returns a PNG image block), `export`, `export_set`, `export_panorama`, `validate_export`. Arguments are closed types (`deny_unknown_fields`); a rejected call returns `isError: true` with the reason and changes nothing. Translations are `set_localized_text` operations in `apply_ops`; `export`, `export_set` and `export_panorama` take a `locale`.
- **Writes stay in the project:** `export`, `export_set`, `export_panorama` and `validate_export` take plain file names (or a name stem for sets and panoramas: letters, digits, `. _ -`) under `<project>/exports/`; separators, leading dots and `..` are refused. No tool takes another path. A language tag is `en`, `ja`, `pt-BR` (letters and digits, at most 20 characters), so it is safe inside a file name. The one write outside the project folder is the agent's memory (see [Agent memory](#agent-memory)): text notes in `.opendesigner/memory/`, next to the assets folder, with their own name, symlink and size rules.
- **Media:** an image node's `image` must be `media/<plain file name>` (`.png`, `.jpg`, `.jpeg`, `.webp`); `core` refuses anything else, and the renderer refuses symlinks. Files get into `media/` only when the **user** imports them (the editor's Image button or a drop on the canvas copies the file and sniffs its first bytes). There is deliberately no agent `import_media`: a tool that copies a path from the user's disk would read outside the project (invariant 5). `list_media` shows the agent what is there, with pixel sizes, so it can size and place nodes; to use a new image it asks the user to add it.
- **Tools:** `get_guidance` (also returns the workspace's `brief`), `get_playbook`, `review_frame`, `list_formats`, `get_brief`, `set_format`, `get_selection`, `get_annotations`, `resolve_annotation`, `list_pages`, `get_node`, `get_tree`, `find_nodes`, `list_tokens`, `apply_ops`, `list_checkpoints`, `diff_checkpoint`, `restore_checkpoint`, `list_export_presets`, `list_media`, `list_locales`, `device_template`, `render_preview` (returns a PNG image block), `export`, `export_set`, `export_panorama`, `validate_export`, `list_videos`, `render_video_frame`, `export_video`, `list_video_presets`, `list_footage`, `find_files`, `import_file`, `search_sounds`, `import_sound`, `list_sounds`. Arguments are closed types (`deny_unknown_fields`); a rejected call returns `isError: true` with the reason and changes nothing. Translations are `set_localized_text` operations in `apply_ops`; `export`, `export_set` and `export_panorama` take a `locale`.
- **Writes stay in the project:** `export`, `export_set`, `export_panorama` and `validate_export` take plain file names (or a name stem for sets and panoramas: letters, digits, `. _ -`) under `<project>/exports/`; separators, leading dots and `..` are refused. No tool takes another path. A language tag is `en`, `ja`, `pt-BR` (letters and digits, at most 20 characters), so it is safe inside a file name.
- **Media:** an image node's `image` must be `media/<plain file name>` (`.png`, `.jpg`, `.jpeg`, `.webp`); `core` refuses anything else, and the renderer refuses symlinks. A video's sound track names a file the same way (`.mp3`, `.wav`, `.m4a`, `.aac`, `.ogg`): `set_video` with `prop {name: "audio", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}` sets it, `null` takes it away, and the export mixes it under the picture. Files get into `media/` only when the **user** imports them (the editor's Image button or a drop on the canvas copies the file and sniffs its first bytes). There is deliberately no agent `import_media`: a tool that copies a path from the user's disk would read outside the project (invariant 5). `list_media` shows the agent what is there, with pixel sizes, so it can size and place nodes; to use a new image it asks the user to add it.
- **Change hook:** `Handler::with_change_hook` reports the nodes an agent touched, for the editor to outline them. Reads do not fire it.

- **Sounds from Freesound.** `search_sounds {query, duration_max_ms?, page?}` asks Freesound for music and sound effects; `import_sound {id, name?}` downloads the preview into the project's `media/` folder and writes `media/<name>.freesound.json` beside it; `list_sounds` shows what is imported. They need the user's free Freesound token, which the user pastes into the Connect tab; without it every call answers "no Freesound token: ask the user to add one in the Connect tab; they can get one at https://freesound.org/apiv2/apply/". The token is a credential: it is stored in the per-user config folder and sent only to freesound.org ([Security](./security.md#network)). The sounds keep their authors' licenses, so the tool descriptions tell the agent to show the attribution it is given; see [Video Editing](./video-editing.md#finding-sounds-freesound).

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

### Connecting an agent

The Connect tab (and `opendesigner serve-mcp <project> --agent <id>`) shows, per agent, the exact thing to run or save; it is `od_agent::connect::recipes`. The editor never edits an agent's configuration (invariant 6), and no recipe contains the token: each reads the token file when the agent starts (`$(cat 'file')`, an `!` command header for omp, `{file:…}` for opencode) or names an environment variable to fill from it.

| Agent | What to do | Checked against the real agent |
|-------|-----------|--------------------------------|
| omp | `.omp/mcp.json` (project) or `~/.omp/agent/mcp.json` with an `http` server and an `Authorization` header of `!printf 'Bearer %s' "$(cat 'token-file')"` | a real turn listed the pages through it |
| Claude Code | `claude mcp add --transport http …` | connected, tools listed (no model turn) |
| Codex CLI | `[mcp_servers.opendesigner]` with `url` and `bearer_token_env_var` in `~/.codex/config.toml` | `codex mcp list` parsed it; no connection tried |
| Gemini CLI | `gemini mcp add -s user --transport http --header …` (copies the token into Gemini's settings) | entry written; no connection tried |
| opencode | `opencode.json` with a `remote` server and `Bearer {file:token-file}` | `opencode mcp list` showed it connected |
| Cursor | `.cursor/mcp.json` with `${env:OPENDESIGNER_MCP_TOKEN}` | not installed here; documented format only |
| any other client | the URL and `Authorization: Bearer <token>` | the server speaks MCP 2025-06-18 over Streamable HTTP |

The shell forms are for macOS and Linux shells; Windows was not tried.

## Making agents good

Agents without direction produce the same centred title on a gradient, and fades for everything. Three things push every agent above that, **whether or not the skill is installed** (`crates/agent/src/playbook.rs`, `skill/*.md`):

1. **The quality bar** (`playbook::QUALITY_BAR`, about 2 KB) is sent in the MCP `initialize` result's `instructions` (every MCP client reads it) and prepended to the first prompt of the in-app chat. It says: look at the real product first; read the playbooks; build a system (tokens, type scale, grid, named layers) before pictures; build one hero frame, look, fix; check with `render_preview` (also thumbnail size), `review_frame` and `render_video_frame`; iterate at least once; use every tool; never invent facts; say what you decided.
2. **Playbooks** as the `get_playbook` tool (and as `references/` files of the installed skill, which also gets a description that triggers on video and motion work): `design` (process, hierarchy, type scale, banned defaults, layout recipes), `style` (eight named looks with rules: Swiss kinetic type, flat explainer, dark keynote, living screencast, data story, warm editorial, soft product, neo-brutalist), `video` (kinds of video, script and beat sheet first, hook / problem / proof / payoff arc, captions, editing footage, quality gate), `motion` (timing table, easing rules, stagger, transition meaning, overlay recipes, pre-export checks) and `formats` (sizes, safe zones). Distilled from public skills (frontend-design and canvas-design by Anthropic, ParthJadhav's app-store-screenshots, the ECC video-editing skill, kajisho5/ffmpeg-skill, arhen/motion-film, buildwithhanif/claude-animation-skill, cth9191/animate, iart-ai/explainer-video-skills, Barty-Bart/motion-graphics, artemnovitckii/claude-animation-skills, awesomeclaude.ai's motion-graphics styles) and platform documentation (Meta's ad safe-zone guide, TikTok and Google templates, Apple's app preview pages), rewritten for what OpenDesigner can draw; nothing is copied. Safe-zone numbers were read on 2026-11-19 from published guides and platforms change them, so the text says so.
3. **`review_frame`** measures what an art director would flag: WCAG contrast of text against the fills under it, text under 2.2% of the short side, text within 4% of an edge or past it, overlapping text boxes, wrapped overflow, near-miss left edges, more than two font families, more than six hard-coded colors (use tokens), and a size that differs from the workspace's format. `error` means fix; `warn` means fix or have a reason. It does not change the design and does not judge taste: the playbooks and a look at the preview do. `export`, `export_set` and `export_panorama` add `before_you_call_it_done` to their result when a frame was never reviewed, or changed (anything inside it) after its last review, so an agent that skips the check is told at the moment it matters; the export still goes through (a nudge, not a gate).

**Format and permission.** A workspace may have a format (`opendesigner/.../brief.json`: size, aspect ratio, frame rate, who chose it). The user may choose one when starting (sections: Video, Store images, Social images, Web and icons, or their own size) or from the toolbar; choosing nothing leaves it to the agent, which picks from `list_formats` and records it with `set_format` (applies at once, shown in the toolbar as agent-chosen). A format the user chose is kept: `create_video` and `set_video` at another size or frame rate are refused, and `set_format` does not change it but leaves a request (with the agent's reason) that the window shows with **Allow** / **Keep mine**; the agent reads the answer with `get_brief`. Images are guidance only (`review_frame` notes a different size), because a panorama is several slides wide.

## Direction picker

Before the agent starts, the user can say what the work should look like, and the picks become part
of the prompt (`od_agent::direction`, the **Direction** button in the chat and on the start screen):

| what | choices |
|---|---|
| Look | 12 named looks (Swiss kinetic type, flat explainer, dark keynote, living screencast, data story, warm editorial, soft product, neo-brutalist, luxury minimal, playful, risograph, soft gradient); each carries its rules, which are the ones in `references/style-directions.md` |
| Colors | 16 palettes (background, surface, text, accent, muted) or colors the user types |
| Type | 9 pairings, all fonts macOS ships (the renderer falls back to the bundled Lato for a family the machine lacks, so the preview has the last word) |
| Mood | calm, energetic, premium, friendly, technical, bold, playful, trustworthy, urgent, nostalgic |
| Motion | none, subtle, standard, kinetic |
| Layout | airy, balanced, dense |
| Imagery | real app screens in device frames, typography only, abstract shapes, photography the user has, icons and badges |
| Free text | who it is for and what it should do, anything else |

Three switches change how the agent works: **look for good skills and references first** (it is told to list its skills and use the best matches, and to search when it can), **show me three small directions first** (it builds three hero frames in that direction and waits), and **remember these choices** (it writes them into the agent memory).

## The chat's settings and commands

The agent's own Agent Client Protocol session offers settings (the model, the mode, how hard it
thinks) and slash commands. The chat shows them as they arrive (`session/new` returns
`configOptions`; agents that only speak the older `modes` and `models` are read the same way) and
the user can change them without leaving the panel: `session/set_config_option` (or the older
`session/set_mode` and `session/set_model`). The commands the agent reports (`available_commands`)
are listed behind the `/` button and typed into the message field like any other text.

The agent's words are shown in text edits, so they can be selected and copied.
## Agent memory

Agents start every task blank. `.opendesigner/` next to the assets folder (the parent of `opendesigner/`, so `<repo>/.opendesigner/` for a repository of workspaces) holds small text notes that carry what was learned from one task to the next. It is separate from `opendesigner/`, which holds the generated assets (media, exports, pages, videos) and may be gitignored; **track `.opendesigner/` in git**. It contains only Markdown or plain text, never binaries. A project that is not inside an `opendesigner/` folder keeps its notes in `<project>/.opendesigner/`, so nothing is written outside the folder the user picked.

- Layout: `.opendesigner/README.md` (what the folder is; written once, never overwritten) and `.opendesigner/memory/<name>.md|.txt`. The app creates both when a working folder is opened; the tools create them on first write. OpenDesigner does not edit `.gitignore`.
- Tools: `list_memory` (name, bytes, title, modified), `read_memory {name}`, `write_memory {name, text, mode?}` (`append`, the default, adds a dated `### YYYY-MM-DD` entry; `replace` rewrites the file). `get_guidance` carries a `memory` field with the index (names and titles, no bodies) and the instruction to read what is relevant before building and write lessons back before finishing.
- What goes in: brand facts (colors, fonts, copy) and their source, what the user corrected or liked, what worked, mistakes to avoid, decisions; conventionally `brand.md`, `lessons.md`, `decisions.md`, `style.md`. Never secrets or tokens.
- Rules and limits (names, no symlinks, 64 KB per note, 200 notes, 2 MB total, atomic writes): [Security](./security.md#agent-memory).
- The Memory panel lists the notes under **Notes for the agent** (read-only, copyable); edit them with any editor.

## Extra MCP servers

The chat can hand the agent more MCP servers than the editor's own: programs the user installed on
this computer (a speech model server, a search API, a database). They are listed in the
**Connect** tab and are given to the agent with the session, so the agent can call them while it
works.

### What the user adds

| Field | What it is |
|-------|------------|
| Name | How the server is called in the chat: letters, digits, spaces, `-` and `_`, at most 40 characters. It is the name the agent sees. |
| Command | The program that starts the server and its arguments (`npx -y minimax-mcp`), split on spaces with quotes, **no shell**. |
| Working directory | Where the server runs; the session's folder (the project) when it is empty. |
| Environment variables | `KEY=value` per line, given to that server when it starts. |

The list lives in the app's config folder (`mcp-servers.json`, next to the connection token;
`OPENDESIGNER_CONFIG_DIR` overrides the folder). The file is readable by the user only, because a
value may be a key:

```json
[
  {
    "name": "minimax-mcp",
    "command": "npx",
    "args": ["-y", "minimax-mcp"],
    "env": [{ "name": "MINIMAX_API_KEY", "value": "…" }]
  }
]
```

Only the user writes this list, through the app. No MCP tool adds or changes a server, and no
agent can.

### The MiniMax example

`minimax-mcp` is the MiniMax MCP server: it gives the agent speech models (text to speech and
transcription), so the agent can voice a video or write down what a clip says. Install it the way
MiniMax documents — `npx -y minimax-mcp` or `uvx minimax-mcp`, their README has the current
command — then add it in the Connect tab with the name `minimax-mcp` and the command you
installed. Put your `MINIMAX_API_KEY` in the environment variables: it is kept in the config
folder, given to that server only, and never shown again (the window lists the names of the
variables; to keep a value, write its name on a line of its own).

### What goes on the wire

The ACP `session/new` request carries the editor's own MCP server first (over HTTP, with the
bearer token) and then the user's, in the ACP stdio form:

```json
{
  "name": "minimax-mcp",
  "command": "/opt/homebrew/bin/npx",
  "args": ["-y", "minimax-mcp"],
  "env": [{ "name": "MINIMAX_API_KEY", "value": "…" }]
}
```

What was checked (2026-11-20): the ACP v1 schema (`https://agentclientprotocol.com/protocol/v1/schema`)
defines `McpServer` as a union of an HTTP variant (`{"type": "http", …}`), an SSE variant and a
stdio variant with `name`, `command`, `args` and `env` (`[{name, value}]`); the session-setup page
shows the same stdio example. **The v1 stdio variant has no `type` field** — the discriminator
`"type": "stdio"` was added in the v2 draft, so it is not sent: the shape above is exactly the v1
one, and the session's working directory is the project folder, so no extra field is needed.
The program is sent as the path to the executable (a name on `PATH` is resolved when the session
starts), because the spec asks for an absolute path.

### When a server does not start

The agent starts the servers itself and says so when it cannot start one: ACP v1 has a live
`notice` update for that (`{"sessionUpdate": "notice", "severity": "warning", …}`), and the client
asks for it (`clientCapabilities.session.notices`). The window shows the notice in the chat and
the session carries on without the server. Nothing the app does can start a server the user did
not install, so a failure is the user's to fix (the notice says which server).

Not verified: no real MCP server was started on this machine, and no agent other than the scripted
one was asked to start one.

## Guidance context

This is the feature the whole app exists for. The editor maintains, and exposes to agents, what the user is pointing at:

- **Selection.** Node ids, with each node's data and a rendered crop, so "make this bigger" is unambiguous.
- **Annotations.** The user drops a pin or draws a box on the canvas (or a timeline range) and types a note. An annotation is `{ id, target (node id or canvas rect), note, status }`. The agent reads open annotations, works them, and marks them resolved; the user sees pins close.
- **Hand edits as signal.** When the user fixes what the agent did, the operations are in the log with `actor: user`; the agent can read them to learn the correction (`get_guidance` includes the last user operations after the agent's last checkpoint).
- **Reference.** The user can attach a reference image or another project's frame ("make it feel like this") that the agent can fetch.

The agent's output is shown, not applied silently: changed nodes are outlined, a one-line summary of each turn sits in the chat/history panel, and a checkpoint exists to restore.

## In-app chat

> **Status: Partially implemented.** Built: an ACP v1 client (`od_agent::acp`), the Chat tab, tests against a scripted agent, and `cargo run -p opendesigner-agent --example acp_probe -- [--prompt=TEXT] <agent command>` to run the session setup (and one turn) against a real agent. With **omp** a real turn worked end to end (see the status above). Setup was checked with opencode, Gemini CLI, the Claude adapter and the Codex adapter; a prompt turn was not, because those three are not signed in here. The setup timeout is 4 minutes because the first `npx` start of an adapter downloads it (over a minute here).

A chat panel (Kino's "chat panel in your project") that drives an agent over the **Agent Client Protocol**: the user picks the agent they already use and are signed in to (their own subscription); OpenDesigner starts it as a child process and shows its turns, tool calls and permission requests inline.

- **Protocol.** ACP version 1: newline-delimited JSON-RPC 2.0 over the child's stdio (`initialize`, `session/new`, `session/prompt`, `session/cancel`, `session/update` notifications, `session/request_permission`). An agent that answers with another version is refused with a message saying so. Lines that are not JSON are ignored (some agents print a banner). ACP is changing (a v2 draft exists, and adapter package names have been renamed more than once), so this is the part most likely to need updating.
- **What the agent is given.** `session/new` carries the project folder as the working directory and one MCP server: the editor's own, over HTTP, with the bearer token in an `Authorization` header (the agent must advertise `mcpCapabilities.http`, or the chat says it cannot connect). The MCP servers the user added come after it (see [Extra MCP servers](#extra-mcp-servers)). The agent therefore edits the design through `apply_ops` like any other agent: attributed, checkpointed, outlined. The first prompt of a conversation carries a short preamble telling it to call `get_guidance`, which returns the selection, the pinned notes and the user's recent hand edits.
- **What the client serves.** Nothing: no file-system or terminal capability is declared, and a request for either is answered with "method not supported". Agents do their own file and shell work in their own process; their own permission prompts arrive as `session/request_permission` and are shown with the agent's options (Allow, Reject, ...) for the user to pick, or Cancel.
- **What starts.** Only the command the user typed or picked in the panel (a program and arguments, split on spaces with quotes, **no shell**); it is remembered next to the connection token. No project file and no agent can set it. Presets in the panel, each marked "found" when the programs it needs are on `PATH`: `omp acp`, `npx -y @agentclientprotocol/claude-agent-acp`, `npx -y @agentclientprotocol/codex-acp`, `gemini --acp`, `opencode acp`, or any command you type (any ACP agent works). The MCP servers the user added are not started by the app either: the agent starts them (see [Extra MCP servers](#extra-mcp-servers)).
- **Credentials.** The agent signs in on its own; OpenDesigner never sees its credentials. The agent does receive the local MCP token, which only opens the loopback server of this app.
- **Plans.** An ACP `plan` update (the agent's whole plan; each one replaces the last) is shown as a checklist above the conversation and cleared when the user sends the next message. Checked with real omp: its `todo` tool arrives as `plan` updates the client parses.
- **Not built:** images in prompts, session resume and a session list, several conversations, per-turn checkpoints named after the chat (the checkpoint comes from the MCP `apply_ops` turn as for any agent).

## Headless

`opendesigner` CLI runs the same operations against a project folder without a window: `export`, `validate`, `apply` (a file of operations), `serve-mcp`. Used in CI and by agents on machines with no display (Kino documents headless use on Linux servers; OpenCut lists a headless mode in its plans).
