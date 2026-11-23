# Agent Integration

> **Status: Partially implemented.** The MCP server, the skill generator/installer (`crates/agent`, `opendesigner skill …`), per-agent connection instructions (`od_agent::connect`) and the Chat tab are built and tested; the Qt app runs the MCP server in-process and has a Connect tab (a picker of agents with the exact thing to paste, and the skill install) and a Chat tab (see [In-app chat](#in-app-chat)). On the first start with an agent present and no skill installed, a dialog lists every file it would write and offers Install, Not now or Don't ask again (remembered in the config folder). The Connect tab also holds the MCP servers the user adds for the chat (see [Extra MCP servers](#extra-mcp-servers)). The design is agent-agnostic: MCP over Streamable HTTP and ACP are open protocols, and the skill goes where each agent reads skills. **Checked against real agents (2026-11-19):** with **omp 19.0.0**, signed in, a full turn through the in-app chat's own ACP client (`omp acp`) called `get_guidance`, `list_pages` and `apply_ops` on this server, created a frame and a text node, and the edit is attributed `agent:omp-coding-agent` in the project history; the same server was also read from a project `.omp/mcp.json` by `omp -p`. **opencode 1.19.3** listed the server as connected from an `opencode.json`, and `opencode acp` opened a chat session. Claude Code 2.1.318 connected over HTTP with the bearer token and listed the tools; Gemini CLI 0.38.2, the Claude adapter and the Codex adapter answered the chat's ACP handshake (Gemini and Codex with a sign-in error, which the chat reports). **Not checked:** a model turn with Claude Code, Codex or Gemini CLI (not signed in here), Codex, Gemini and Cursor connecting to MCP, and any of it on Windows or Linux. Skill locations were verified against each agent's docs on 2026-11-19 (see the target table).

The point of OpenDesigner is that the user guides an agent *with the GUI*. Three mechanisms carry that, and the app installs the first two on first run.

| Mechanism | What it is | Who uses it |
|-----------|------------|-------------|
| **Skill** | An [Agent Skill](https://agentskills.io/specification) (`SKILL.md` plus references) teaching an agent how to work with OpenDesigner | CLI agents the user already runs |
| **MCP server** | A local HTTP MCP endpoint exposing the editor's operations and the user's guidance context | The same agents, after the skill points them at it |
| **In-app chat** | A chat panel that drives an agent over the Agent Client Protocol | Users who would rather not leave the app |

## Why a skill and an MCP server

The shape: an MCP server inside the desktop app (`http://127.0.0.1:45680/mcp`, with the app kept open while the agent works) that Claude Code, Codex, ChatGPT, Cursor and others connect to with one command, and skills that tell the agent *how* to edit well. Penpot and Figma both ship MCP servers too (Penpot's runs through its Plugin API; Figma's remote server can write to the canvas). A tool surface alone leaves the agent to guess conventions; a skill alone cannot touch the document. We ship both, and add what none of them has as a first-class idea: the user's **pointing** as input ([Guidance context](#guidance-context)).

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
- **Tools:** `get_guidance` (also returns the workspace's `brief`), `get_playbook`, `review_frame`, `review_video`, `list_formats`, `get_brief`, `set_format`, `get_selection`, `get_annotations`, `resolve_annotation`, `list_pages`, `get_node`, `get_tree`, `find_nodes`, `list_tokens`, `apply_ops`, `list_checkpoints`, `diff_checkpoint`, `restore_checkpoint`, `list_export_presets`, `list_media`, `list_memory`, `read_memory`, `write_memory`, `list_locales`, `device_template`, `render_preview` (returns a PNG image block), `export`, `export_set`, `export_panorama`, `validate_export`. Arguments are closed types (`deny_unknown_fields`); a rejected call returns `isError: true` with the reason and changes nothing. Nodes turn, scale, blur, cast shadows and clip through `set`/`create` properties and a video's camera and looping tracks through `animate` (`repeat`, `camera_*` with the video as `node`); see [Document Format](./document-format.md#transform-and-effects). Translations are `set_localized_text` operations in `apply_ops`; `export`, `export_set` and `export_panorama` take a `locale`.
- **Writes stay in the project:** `export`, `export_set`, `export_panorama` and `validate_export` take plain file names (or a name stem for sets and panoramas: letters, digits, `. _ -`) under `<project>/exports/`; separators, leading dots and `..` are refused. No tool takes another path. A language tag is `en`, `ja`, `pt-BR` (letters and digits, at most 20 characters), so it is safe inside a file name. The one write outside the project folder is the agent's memory (see [Agent memory](#agent-memory)): text notes in `.opendesigner/memory/`, next to the assets folder, with their own name, symlink and size rules.
- **Media:** an image node's `image` must be `media/<plain file name>` (`.png`, `.jpg`, `.jpeg`, `.webp`); `core` refuses anything else, and the renderer refuses symlinks. Files get into `media/` only when the **user** imports them (the editor's Image button or a drop on the canvas copies the file and sniffs its first bytes). There is deliberately no agent `import_media`: a tool that copies a path from the user's disk would read outside the project (invariant 5). `list_media` shows the agent what is there, with pixel sizes, so it can size and place nodes; to use a new image it asks the user to add it.
- **Tools:** `get_guidance` (also returns the workspace's `brief`), `get_playbook`, `review_frame`, `review_video`, `list_formats`, `get_brief`, `set_format`, `get_selection`, `get_annotations`, `resolve_annotation`, `list_pages`, `get_node`, `get_tree`, `find_nodes`, `list_tokens`, `apply_ops`, `list_checkpoints`, `diff_checkpoint`, `restore_checkpoint`, `list_export_presets`, `list_media`, `list_locales`, `device_template`, `render_preview` (returns a PNG image block), `export`, `export_set`, `export_panorama`, `validate_export`, `list_videos`, `render_video_frame`, `video_contact_sheet`, `video_strip`, `video_crop`, `frame_grid`, `video_diff`, `watch_footage`, `index_footage`, `search_footage`, `export_timeline`, `audio_reactive`, `animate_noise`, `fit_clip_to_media`, `animate_preset`, `sequence`, `spring_to`, `list_blocks`, `get_block`, `add_block`, `device3d_template`, `export_video`, `list_video_presets`, `list_footage`, `find_files`, `import_file`, `search_sounds`, `import_sound`, `list_sounds`, `make_sound`, `show_preview`, `offer_variants`, `get_variants`, `clear_variants`. Arguments are closed types (`deny_unknown_fields`); a rejected call returns `isError: true` with the reason and changes nothing. Translations are `set_localized_text` operations in `apply_ops`; `export`, `export_set` and `export_panorama` take a `locale`.
- **Writes stay in the project:** `export`, `export_set`, `export_panorama` and `validate_export` take plain file names (or a name stem for sets and panoramas: letters, digits, `. _ -`) under `<project>/exports/`; separators, leading dots and `..` are refused. No tool takes another path. A language tag is `en`, `ja`, `pt-BR` (letters and digits, at most 20 characters), so it is safe inside a file name.
- **Media:** an image node's `image` must be `media/<plain file name>` (`.png`, `.jpg`, `.jpeg`, `.webp`); `core` refuses anything else, and the renderer refuses symlinks. A video's sound track names a file the same way (`.mp3`, `.wav`, `.m4a`, `.aac`, `.ogg`): `set_video` with `prop {name: "audio", value: {file, start_ms, gain, fade_in_ms, fade_out_ms}}` sets it, `null` takes it away, and the export mixes it under the picture. Files get into `media/` only when the **user** imports them (the editor's Image button or a drop on the canvas copies the file and sniffs its first bytes). There is deliberately no agent `import_media`: a tool that copies a path from the user's disk would read outside the project (invariant 5). `list_media` shows the agent what is there, with pixel sizes, so it can size and place nodes; to use a new image it asks the user to add it.
- **Change hook:** `Handler::with_change_hook` reports the nodes an agent touched, for the editor to outline them. Reads do not fire it.

- **Sounds without Freesound.** `make_sound {kind, duration_ms?, seed?, pitch_hz?, bpm?, bars?, stereo?, name?}` synthesizes effects and simple loops into `media/<name>.wav` (no licence, sidecars `.synth.json` and `.license.json`); the kinds are `click`, `pop`, `tick`, `whoosh`, `swoosh_up`, `swoosh_down`, `riser`, `hit`, `chime`, `drone`, `beat`, `stinger`, `key` (a typing click), `blip` (a UI blip), `pad` (a slow chord bed), `impact` (a heavy thump with a crack and a tail) and `scratch` (a record scratch). `import_file` also takes the user's audio (`find_files` `kind: "audio"`; mp3, wav, m4a, aac, ogg, sniffed by their first bytes); `list_sounds` lists all audio in `media/` with length, source and licence. See [Audio sources](./video-editing.md#audio-sources).
- **Telling the agent what it has.** The in-app chat's first message ends with the user's extra MCP servers (names only, never commands or environment values; known names get a hint, `minimax-mcp` is MiniMax speech for voice-overs, ask before spending credits on long text) and a line on the sound tools. The MCP `initialize` instructions cannot know the user's servers, so `get_guidance` returns them as `extra_mcp_servers` (`[{name, about?}]`) plus a `sound` line; the app gives the handler the names with `Handler::set_extra_servers` each time the chat starts.
- **Sound tracks, captions, pauses and waveforms** (`crates/agent/src/audio_tools.rs`; the same tools run headless as `opendesigner sound ...` and `opendesigner captions ...`). Changing tools apply one `set_video` through `apply_ops`, so each is one attributed, checkpointed, undoable transaction and returns the receipt with extra fields. `add_sound_track {video, file, role?, start_ms?, in_ms?, out_ms?, gain?, fade_in_ms?, fade_out_ms?, pan?, duck?}` (up to 8; returns `track`), `set_sound_track {video, track, ...}` (only given fields change; `out_ms: null` and `duck: null` clear), `remove_sound_track {video, track}`; `detect_pauses {file, noise_db?, min_ms?}` (read-only; ffmpeg `silencedetect`), `remove_pauses {video, clip, noise_db?, min_ms?, keep_ms?}` (replaces a footage clip by the stretches with sound, one transaction), `get_waveform {file, buckets?}` (read-only; 50-2000 peaks from an ffmpeg PCM decode); `import_captions {video, format: srt|vtt|words_json, text, offset_ms?}`, `captions_from_script {video, script, start_ms?, end_ms?, words_per_cue?}` (an estimate by word length), `add_caption_cue`, `remove_caption_cue`, `clear_captions`, `set_caption_style {video, preset?, ...}`. **Speech-to-text is not built in** (it needs a model or a network service, which would break local-first): the transcript comes from a speech MCP server the user added in the Connect tab, or from the user, and the tool descriptions, the skill and the `video` playbook all say so. `list_videos` rows carry `tracks`, `captions {style, cues}` and story lines for both.
- **Sounds from the libraries.** `search_sounds {query, provider?, source?, duration_max_ms?, page?}` searches four libraries and `import_sound {provider, id, name?}` brings one into the project's `media/` folder with its licence written beside it as `media/<name>.license.json`. The `provider` is `openverse` (the default when the user has no Freesound token), `freesound`, `incompetech` (Kevin MacLeod's music, all CC BY 4.0) or `ccmixter`; `source` narrows an Openverse search to one hosting site (`freesound`, `jamendo`, `wikimedia_audio`). Each result carries its id, title, creator, length, licence (SPDX-style, with its url and whether it needs a credit), the ready-made attribution and a preview to listen to. Freesound needs the user's free token, which the user pastes into the Connect tab; without it every Freesound call answers "no Freesound token: ask the user to add one in the Connect tab; they can get one at https://freesound.org/apiv2/apply/". The token is a credential: it is stored in the per-user config folder and sent only to freesound.org ([Security](./security.md#network)). The other three need no key. `list_sounds` shows what is imported.
- **Licences, credits and the `commercial` setting.** A project is commercial by default (`sound-settings.json` in the project folder). While it is, the search leaves out sounds under a non-commercial, a no-derivatives or an unknown licence (the answer counts them in `hidden_by_license`) and the import refuses them with the message to use `sound_settings` instead. `sound_settings {commercial?, reason?}` reads the setting and changes it: a change to `false` needs the user's own words as the reason, and the setting never changes to get a sound through. `sound_credits {video}` writes `exports/<video>-credits.txt` with the credit line of every sound the video's tracks use (and what the video changed, as CC BY asks), and lists the sounds with no licence record and the ShareAlike ones; the agent shows that text with the video. The licences and the destinations are in [Security](./security.md#network).

- **Assets.** `list_asset_sources`, `search_assets`, `make_asset`, `import_asset`, `list_asset_packs`, `install_asset_pack`, `unload_asset_pack` give the agent icons (Lucide built in; more sets and Kenney CC0 packs as downloads), generated blobs, gradients, auroras, grain, patterns and shapes, and sounds, written into `media/` as PNG (an SVG is recolored and drawn at the size asked for) or copied as is, each with a `media/<name>.asset.json` sidecar. `install_asset_pack` only asks the user (the Connect tab); the download is a job once approved. The MCP servers for 3D assets the user named are optional external servers added in the Connect tab. Full description, sources and licences: [Store Assets](./store-assets.md#asset-packs-icons-shapes-backgrounds-sounds).

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
2. **Playbooks** as the `get_playbook` tool (and as `references/` files of the installed skill, which also gets a description that triggers on video and motion work): `design` (process, hierarchy, type scale, banned defaults, layout recipes), `style` (eight named looks with rules: Swiss kinetic type, flat explainer, dark keynote, living screencast, data story, warm editorial, soft product, neo-brutalist), `sketch` (hand-made looks: sketch, boil, watercolor, paper grain, flow strokes), `explainer` (animated 2D scenes: the whole craft of a Kurzgesagt-style film, see below), `scenery` (layered flat-vector scenes, parallax, scene moves and camera presets), `review` (looking at your own video harshly: the self-critique loop, the checklist, the inspection tools, see below), `video` (kinds of video, script and beat sheet first, hook / problem / proof / payoff arc, captions, editing footage, quality gate), `motion` (timing table, easing rules, stagger, transition meaning, overlay recipes, pre-export checks), `3d` (3D devices: the models, the pose that reads as a product shot, capturing from the iOS simulator, the Android emulator or a browser, and the named moves), `formats` (sizes, safe zones) and `blocks` (the block catalog: when to reach for a ready-made title, overlay, background or chart, how to customize it and how to check the result) and `characters` (flat-vector characters with a named rig, speech and thought bubbles and animated props: the actions, the idle life, timing against the voice). Distilled from public skills (frontend-design and canvas-design by Anthropic, ParthJadhav's app-store-screenshots, the ECC video-editing skill, kajisho5/ffmpeg-skill, arhen/motion-film, buildwithhanif/claude-animation-skill, cth9191/animate, iart-ai/explainer-video-skills, Barty-Bart/motion-graphics, artemnovitckii/claude-animation-skills, awesomeclaude.ai's motion-graphics styles) and platform documentation (Meta's ad safe-zone guide, TikTok and Google templates, Apple's app preview pages), rewritten for what OpenDesigner can draw; nothing is copied. Safe-zone numbers were read on 2026-11-19 from published guides and platforms change them, so the text says so.
3. **`review_frame`** measures what an art director would flag: WCAG contrast of text against the fills under it, text under 2.2% of the short side, text within 4% of an edge or past it, overlapping text boxes, wrapped overflow, near-miss left edges, more than two font families, more than six hard-coded colors (use tokens), and a size that differs from the workspace's format. `error` means fix; `warn` means fix or have a reason. It does not change the design and does not judge taste: the playbooks and a look at the preview do. `export`, `export_set` and `export_panorama` add `before_you_call_it_done` to their result when a frame was never reviewed, or changed (anything inside it) after its last review, so an agent that skips the check is told at the moment it matters; the export still goes through (a nudge, not a gate).

**Video starts with an inventory.** An agent once made a video without touching the bundled assets because nothing told it to: the skill, the quality bar and the `video` playbook never named the asset tools. Now the quality bar (item 5), the skill body, the first chat message, the `video` playbook (*First: take inventory*, *Using assets in a video*) and the descriptions of `list_media`, `list_videos`, `get_playbook`, `search_assets`, `make_asset` and `import_asset` all say the same: `list_media`, `list_footage`, `list_sounds`, `list_asset_sources`, `search_assets` first. There is no separate asset-to-video operation: an asset is a PNG or sound in `media/`, a PNG becomes an `image` node in a design frame, and that frame is a clip (`add_clip`) or an overlay (`add_overlay`); sounds go to `set_video` `audio`. All of it is validated and undoable through the existing ops. Tests pin the tool names in the guidance against the registry and drive the path end to end.

**Animated work starts with `explainer`.** A user asked for a "2D animated scene like Kurzgesagt" and got a JPEG backdrop with animated text on top. The engine could already draw vector paths, gradients, shadows, loops and a camera (format version 4), but nothing told agents to use it. Now the quality bar (item 2), the `video` playbook (*Animated, illustrated or explainer?*), the installed skill body and the `get_playbook` and `export_video` descriptions route any request with the words animated, animation, scene, explainer, motion graphics, cartoon, 2D, illustration, Kurzgesagt or In a Nutshell to `get_playbook explainer` first, and say it outright: an image or JPG backdrop plus text is never an acceptable animated scene. Status: **Built** (`crates/agent/skill/illustrated-explainer.md`, registered in `playbook::TOPICS`; verified by `playbook::tests::animated_requests_are_routed_to_the_explainer_playbook`). The playbook teaches the whole craft with only what the tools offer: script and storyboard first, a 4 to 6 token palette (`set_token`), depth layers with `depth` and `clip`, the block kit and hand-drawn `path` nodes or `import_svg_vector`, a motion language (ambient loops, anticipation and overshoot, stagger, camera moves, transitions by continuity), captions as the minority of the screen, sound design, a per-scene checklist, three worked scene recipes (space, biology, a machine) and a final gate. The block ids and `animate_preset` names it uses are the reserved names of the characters and scenery slices of the same wave `[INFERENCE: not all of them were merged when this was written; get_block and the preset enum are the truth]`.

**Format and permission.** A workspace may have a format (`opendesigner/.../brief.json`: size, aspect ratio, frame rate, who chose it). The user may choose one when starting (sections: Video, Store images, Social images, Web and icons, or their own size) or from the toolbar; choosing nothing leaves it to the agent, which picks from `list_formats` and records it with `set_format` (applies at once, shown in the toolbar as agent-chosen). A format the user chose is kept: `create_video` and `set_video` at another size or frame rate are refused, and `set_format` does not change it but leaves a request (with the agent's reason) that the window shows with **Allow** / **Keep mine**; the agent reads the answer with `get_brief`. Images are guidance only (`review_frame` notes a different size), because a panorama is several slides wide.

## Style packs

Agents without a committed look produce the same centred title on the same gradient. A **style pack**
is that commitment, as data (`crates/agent/src/style_pack.rs`, `style_pack/packs.rs`): six colours, a
type scale, a shape posture, a motion personality, the rules that hold them together and the
anti-patterns that keep them out of the slop. There is nothing to load at run time and no file to go
missing.

Eight packs ship, named by mood rather than after any product: `deep-space-flat`, `paper-craft`,
`swiss-kinetic`, `neon-night`, `soft-clay`, `editorial-ink`, `sunny-explainer`, `mono-terminal`. A
unit test checks every pack's WCAG contrast (body text at least 4.5:1 on the ground and on the
surface, the accent at least 3:1) and that the eight differ from each other in OKLab.

- **`list_style_packs`** — id, name, mood, the swatches and the motion personality of each pack.
- **`get_style_pack {id}`** — the whole pack: the colours, the type, the shapes, the motion and the
  rules written as guidance an agent can follow.
- **`apply_style_pack {id, overrides?}`** — commits the design to the pack. It writes the palette
  into the document as the tokens `bg`, `surface`, `fg`, `muted`, `border`, `accent` and the two
  tints derived from them, `accent_2` and `shadow`; adds the token `palette_locked`; records the
  pack's id and motion personality in the workspace brief (`get_brief`, and the brief inside
  `get_guidance`); and returns the rules. It is **one attributed, checkpointed, undoable
  transaction**: undoing it takes the palette and the lock with it. `overrides` changes single
  colours, so a user's own brand colour can replace the accent without breaking the pack.

**The palette lock.** `palette_locked` is the marker the linter reads: with it, `OD092`
(`palette-lock`) flags every fill and stroke whose colour is neither a token, nor within 0.04 of a
palette colour in OKLab, nor a blend of two palette colours at 25, 50 or 75 percent. Without it the
rule is off, so a project that never chose a pack behaves exactly as before
([Lint rules](./lint-rules.md#the-locked-palette)). The blocks the agent adds afterwards read the
same tokens, so a `title_card` drawn after `apply_style_pack` is painted in the pack's colours.

The `packs` playbook (`skill/style-packs.md`, also `references/style-packs.md` of the installed
skill) says how to choose a pack from the brief, that the agent must say which one it chose and
why, that two packs are never mixed, and how the motion personality maps to eases, durations and
stagger.

## Showing work: Preview and Variants

The canvas is where the agent works, not where the user judges, so two panels show results (`crates/agent/src/showcase.rs`, the file `showcase.json` next to `brief.json`; a missing, over 64 KB or invalid file reads as empty).

- **Preview** (`show_preview {kind, target, title?, note?}`; kind `frame`, `video` or `image`, target a frame id, a video id or a `media/` path) is the end product, drawn clean (no selection, handles, pins or agent outlines). The agent calls it when there is a deliverable and again when it improves it. The user can look at something else (a selector in the panel; session only), and **Let the agent decide** hands the choice back. With no agent choice the panel shows the newest video, else the open frame.
- **Variants** (`offer_variants {title, question?, options:[{label, kind, target, note?}]}`, 2 to 8 options, each a different frame, video or image) replaces any open set, counts rounds, and shows one card per option with **Pick this one**, **Show on canvas** and **Ask for different options**. The agent builds each option as its own named frame or video, offers them, tells the user to pick in the panel, then waits. A pick is stored and, when the chat is connected, sent to it as `I pick variant <label>. Continue with it.`; `get_variants` returns `status` `none`, `waiting` or `picked` with the picked option, `clear_variants` closes the set. The direction picker's "show me three small directions first" line tells the agent to do this.

Targets are checked when the agent offers or shows them (a missing frame, video or file is refused with the reason and nothing changes). A new offer or `show_preview` reveals its panel once and toasts, without moving keyboard focus. Calls are attributed like every other tool (`by` is stored). The Preview plays a video through the same background preview the Video panel uses, which holds one video at a time: showing different videos in both panels at once makes them take turns.

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

Three switches change how the agent works: **look for good skills and references first** (it is told to list its skills and use the best matches, and to search when it can), **show me three small directions first** (it builds three hero frames in that direction, offers them in the Variants panel with `offer_variants` and waits for your pick), and **remember these choices** (it writes them into the agent memory).

## The chat's settings and commands

The agent's own Agent Client Protocol session offers settings (the model, the mode, how hard it
thinks) and slash commands. The chat shows them as they arrive (`session/new` returns
`configOptions`; agents that only speak the older `modes` and `models` are read the same way) and
the user can change them without leaving the panel: `session/set_config_option` (or the older
`session/set_mode` and `session/set_model`). The commands the agent reports (`available_commands`)
are listed above the message field while a `/` command is being typed (or from the options button).
Up and Down choose, Tab or Enter fills the command in, Enter on a whole command sends it, and Esc,
the close button or a click elsewhere closes the list; a list closed by hand stays closed until the
text stops being a command. A slash command is sent to the agent exactly as typed, because agents
only recognize `/name` at the start of the prompt: the chat's first-message preamble is held back
until the first message that is not a command. `/model`, `/effort` and `/mode` open the in-app
chooser instead of going to the agent.

The agent's words are shown in text edits, so they can be selected and copied.
## Code mode tools

`get_code_project`, `set_stack`, `propose_run_plan`, `start_services`, `stop_services`, `restart_service` `service_logs`, `list_simulators`, `boot_simulator`, `install_app`, `launch_app` and `capture_simulator`, the `code` playbook, and the `code_mode` note in `get_guidance`. Behavior, the approval rule and the tmux sessions are in [Code Mode](./code-mode.md); the threat model is in [Security](./security.md#running-processes).

## Sharing files: `export_share`

`export_share` renders frames to a file made to be sent rather than uploaded to a store, in `exports/`: `.jpg` (one frame), `.pdf` (one page per frame, in the order given, pages at 96 pixels to the inch) or `.html` (one self-contained page: each frame as an embedded PNG with its text as alt text, no scripts, no external files). All are raster output at the frame's size times `scale`; transparency is flattened onto white in JPEG and PDF. Store files still go through `export`, which checks the store's rules and writes PNG. Code: `od_render::share` (`encode_jpeg`, `encode_pdf`), `crates/agent/src/share_export.rs`.

## Agent memory

Agents start every task blank. `.opendesigner/` next to the assets folder (the parent of `opendesigner/`, so `<repo>/.opendesigner/` for a repository of workspaces) holds small text notes that carry what was learned from one task to the next. **Notes belong to one workspace:** a named workspace keeps them in `<repo>/.opendesigner/workspaces/<name>/memory/` and never reads another workspace's notes or the legacy project's; the legacy project (`main`, directly in `opendesigner/`) keeps `<repo>/.opendesigner/memory/`. Two similar workspaces therefore do not share what was learned in either. The in-app chat starts the agent in the workspace's own folder, so anything the agent keeps per working directory stays with the workspace too. It is separate from `opendesigner/`, which holds the generated assets (media, exports, pages, videos) and may be gitignored; **track `.opendesigner/` in git**. It contains only Markdown or plain text, never binaries. A project that is not inside an `opendesigner/` folder keeps its notes in `<project>/.opendesigner/`, so nothing is written outside the folder the user picked.

- Layout: `README.md` (what the folder is; written once, never overwritten) and `memory/<name>.md|.txt` in the workspace's folder. The app creates both when a working folder is opened; the tools create them on first write. OpenDesigner does not edit `.gitignore`.
- Tools: `list_memory` (name, bytes, title, modified), `read_memory {name}`, `write_memory {name, text, mode?}` (`append`, the default, adds a dated `### YYYY-MM-DD` entry; `replace` rewrites the file). `get_guidance` carries a `memory` field with the index (names and titles, no bodies) and the instruction to read what is relevant before building and write lessons back before finishing.
- What goes in: brand facts (colors, fonts, copy) and their source, what the user corrected or liked, what worked, mistakes to avoid, decisions; conventionally `brand.md`, `lessons.md`, `decisions.md`, `style.md`. Never secrets or tokens.
- Rules and limits (names, no symlinks, 64 KB per note, 200 notes, 2 MB total, atomic writes): [Security](./security.md#agent-memory).
- The Memory panel lists the notes under **Notes for the agent** (read-only, copyable); edit them with any editor.

## Extra MCP servers

The chat can hand the agent more MCP servers than the editor's own: programs the user installed on
this computer (a speech model server, a search API, a database). They are listed in the
**Connect** tab and are given to the agent with the session, so the agent can call them while it
works.

### Recommended servers

Below the asset packs the Connect tab lists three servers worth adding for 3D assets (`3dassets`, `sozai`, `asset-gate`), each with a one-click **Add** that saves the exact command read from its README (`npx -y 3dassets-mcp`, `uvx --from git+https://github.com/kanaworksai-hue/sozai-search-mcp sozai`, `npx -y github:codepawl/asset-gate`; none needs a key). The click does what the user could type in the form: the app still never installs or starts them. What each is, its licence and what could not be verified are in [Store Assets](./store-assets.md#asset-packs-icons-shapes-backgrounds-sounds).

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

A chat panel that drives an agent over the **Agent Client Protocol**: the user picks the agent they already use and are signed in to (their own subscription); OpenDesigner starts it as a child process and shows its turns, tool calls and permission requests inline.

- **Protocol.** ACP version 1: newline-delimited JSON-RPC 2.0 over the child's stdio (`initialize`, `session/new`, `session/prompt`, `session/cancel`, `session/update` notifications, `session/request_permission`). An agent that answers with another version is refused with a message saying so. Lines that are not JSON are ignored (some agents print a banner). ACP is changing (a v2 draft exists, and adapter package names have been renamed more than once), so this is the part most likely to need updating.
- **What the agent is given.** `session/new` carries the project folder as the working directory and one MCP server: the editor's own, over HTTP, with the bearer token in an `Authorization` header (the agent must advertise `mcpCapabilities.http`, or the chat says it cannot connect). The MCP servers the user added come after it (see [Extra MCP servers](#extra-mcp-servers)). The agent therefore edits the design through `apply_ops` like any other agent: attributed, checkpointed, outlined. The first prompt of a conversation carries a short preamble telling it to call `get_guidance`, which returns the selection, the pinned notes and the user's recent hand edits.
- **What the client serves.** Nothing: no file-system or terminal capability is declared, and a request for either is answered with "method not supported". Agents do their own file and shell work in their own process; their own permission prompts arrive as `session/request_permission` and are shown with the agent's options (Allow, Reject, ...) for the user to pick, or Cancel.
- **What starts.** Only the command the user typed or picked in the panel (a program and arguments, split on spaces with quotes, **no shell**); it is remembered next to the connection token. No project file and no agent can set it. Presets in the panel, each marked "found" when the programs it needs are on `PATH`: `omp acp`, `npx -y @agentclientprotocol/claude-agent-acp`, `npx -y @agentclientprotocol/codex-acp`, `gemini --acp`, `opencode acp`, or any command you type (any ACP agent works). The MCP servers the user added are not started by the app either: the agent starts them (see [Extra MCP servers](#extra-mcp-servers)).
- **Credentials.** The agent signs in on its own; OpenDesigner never sees its credentials. The agent does receive the local MCP token, which only opens the loopback server of this app.
- **Plans.** An ACP `plan` update (the agent's whole plan; each one replaces the last) is shown as a checklist above the conversation and cleared when the user sends the next message. Checked with real omp: its `todo` tool arrives as `plan` updates the client parses.
- **Not built:** images in prompts, session resume and a session list, several conversations, per-turn checkpoints named after the chat (the checkpoint comes from the MCP `apply_ops` turn as for any agent).

## Document tools

> **Status: Implemented** (`crates/core/src/{lint,dryrun,variables}.rs`, `crates/agent/src/{doctools,batch,plugin}.rs`, `crates/cli/src/doccmd.rs`). Verified on macOS; nothing here is platform-specific except the `ffmpeg` the video batch runs.

### Lint: `validate_document`

`od_core::lint::lint(&Document, &LintOptions) -> Vec<Finding>` finds what makes a design or a video wrong without making it invalid, with **stable rule codes** (`OD001`...). The full table, what each rule means and what it does not know is in [Lint rules](./lint-rules.md). A finding is `{code, severity: error|warn|info, node?, frame?, video?, message, hint}`. The core has no fonts or file access beyond `media/`, so the caller supplies text measuring (the renderer's, through the `TextMeasure` trait), the project folder and the export targets (`LintOptions`); a rule without its input is skipped, not guessed.

- **MCP:** `validate_document {min_severity?}` (read-only) returns `{ok, errors, warnings, infos, findings}`; `ok` is `errors == 0`. The skill's loop, the server's connect-time instructions and the tool descriptions tell agents to call it before they finish a turn and to fix every error and warning or say why one was left. `review_frame` stays the picture-level review of one frame; the linter covers the whole document and the videos.
- **CLI:** `opendesigner lint <project> [--json] [--fail-on warn|error] [--preset <id>]...` (default `--fail-on error`; exit code 2 when a finding is at or above it). `--preset` adds a store preset's size rule as an export target for `OD041`.

### Dry run and receipts: `apply_ops`

`Workspace::check_ops(&[Operation]) -> OpsCheck` applies a batch to a scratch copy of the document and reports, per operation, whether it would apply, the nodes it would touch and, for a rejection, a stable error code, the message and a hint. The history, the checkpoints, the document and the files are untouched. A real `Session::apply` of the same batch on the same state changes exactly what was reported (`would_change` equals the receipt's `touched`, `would_create` its `created`); a test pins this.

- **MCP:** `apply_ops {ops, dry_run: true}` returns `{stage: "dry_run", ok, would_change, would_create, ops: [{index, op, ok, touched, error?: {code, message, hint}}]}`. No pre-turn checkpoint is taken. A real call returns `{stage: "applied", saved, transaction, turn, checkpoint_before, created, touched}`, so an agent can tell an accepted change (a transaction and a checkpoint exist) from a checked one. `saved` says the project folder was written. A rejection leads with the code: `E_NODE_NOT_FOUND: node n_9 does not exist` then `hint: ...`.
- **CLI:** `opendesigner apply <project> <ops.json> --dry-run` prints the same JSON and exits 2 when the batch would be rejected; a real apply prints `stage: "applied"` and leads a rejection with the code.
- **Codes** (`OpError::code`, added on top of the typed errors without changing their text): `E_NODE_NOT_FOUND`, `E_DUPLICATE_ID`, `E_INVALID_ID`, `E_OUT_OF_BOUNDS`, `E_CYCLE`, `E_BAD_INDEX`, `E_ROOT_IMMUTABLE`, `E_NOT_APPLICABLE`, `E_NO_CHILDREN`, `E_INVALID_PAINT`, `E_UNKNOWN_TOKEN`, `E_TOKEN_IN_USE`, `E_LIMIT`, `E_INTERNAL_ONLY`, `E_NOTHING_TO`, `E_CHECKPOINT_NOT_FOUND`, `E_ANNOTATION_NOT_FOUND`, `E_PAGE_NOT_FOUND`, `E_VIDEO_NOT_FOUND`, `E_CLIP_NOT_FOUND`, `E_FRAME_IN_USE`, `E_ARRANGE`, `E_NODE_ANIMATED`, `E_INTERNAL`. The set only grows. The match over `OpError` is exhaustive, so a new error variant must name its code.
- **Not built:** a `verified` stage (render and check after the apply). Today an agent verifies with `render_preview` and `validate_document`.

### Template variables and batch export

Variables are described in [Document Format](./document-format.md#template-variables). `list_variables` shows an agent the slots, their kinds, what they `portray` and where they are bound.

`opendesigner export <project> --batch rows.json (--frame <id> | --video <id>) [--preset <id>] [--out-template 'exports/{name}.png'] [--strict-variables] [--scale n]` and the `batch_export` tool render the project once per row. The rows file is a JSON array (or `{"rows": [...]}`) of `{name, variables: {<id>: string|number|boolean}, locale?}`.

- **The project is never changed.** Each row is applied to a scratch copy of the document through `set_variable_value` on a throwaway session (`Workspace::scratch`), then rendered with the ordinary still or video export, so the row is validated against `--preset` like any export. The only files written are the exports.
- **All or nothing per row.** A row exports completely and validates, or reports its error and writes nothing; other rows still run. The report is `{ok, exported, failed, rows: [{name, ok, files, warnings, error}]}`; the CLI prints it and exits 2 when any row failed.
- **Untrusted input.** The rows file is capped at 1 MiB, 100 rows (20 for video: each is a whole encode) and 100 variables per row; unknown keys, wrong types, bad names and duplicate names are refused before anything renders. Names are `[A-Za-z0-9_-]{1,64}`.
- **Strict mode.** By default an unknown variable or a value that does not fit its variable is skipped with a warning in the row's report and the variable keeps its value. `--strict-variables` (`strict_variables` in the tool) fails the row instead.
- **Output confinement.** `--out-template` must start with `exports/`; `{name}` and `{index}` (from 1) are replaced; what remains must be a plain file name directly in `exports/` ending `.png` (`.mp4` for a video). `..`, an absolute path, separators, a leading dot and a collision between rows are refused up front. An `exports/` folder or an output file that is a symlink is never written through.
- **Videos** go through the existing export path (`video_export::export_video`, needs `ffmpeg`), one MP4 per row.

### Vector import

> **Status: Implemented** (`crates/agent/src/{svg_vector,vector_tools}.rs`, `crates/cli/src/vector.rs`). Verified on macOS.

`import_svg_vector {parent, svg | file | asset, name?, x?, y?, width?, index?, turn?, summary?}` (CLI: `opendesigner import-svg-vector <project> --parent <id> (--svg <path> | --file media/<name>.svg | --asset <id>) [--name --x --y --width --index --agent]`) turns an SVG illustration into one **group** of shape nodes, as a single `create` operation: one transaction, one checkpoint, one undo. The nodes are meant to be found and changed afterwards (`find_nodes`, `apply_ops`, `animate`).

- **One node per shape.** Every shape is a `path` node (its data relative to its own bounds, so its box is exactly where it was drawn), except a plain axis-aligned rectangle, which becomes a `rect`, and a circle or ellipse, which becomes an `ellipse`. Transforms are flattened into the shapes (`usvg` resolves them), and `width` scales the whole drawing. An `<svg>` fill or stroke keeps its color, opacity, gradient (linear or radial, in any units, resampled onto the node's box), cap, join and dash; stroke width and dashes are scaled with the transform. `fill-rule: evenodd` is kept on paths.
- **Names.** A shape takes its SVG `id` as its name (`shape N` when it has none); an SVG group with an `id` or an `opacity` below 1 stays a group node named by its id, with its children placed relative to it, so an arm or an eye can be moved or animated on its own. Any other group was only a transform and is flattened. Classes are not read: `usvg` resolves CSS before the importer sees the tree. The top group is named `name`, else the SVG's id, else `illustration`, and sits at `x`, `y`.
- **The answer** lists `group`, `width`, `height`, `nodes`, every part as `{id, name, kind}`, and `notes` for what the nodes cannot carry: text, clip paths, masks, filters, pattern fills, reflecting or repeating gradients, radial gradients stretched into ellipses (drawn as circles), gradients with more than 8 stops (reduced), shapes over the path limits (skipped). An asset's `credit` is included and its SVG and sidecar are kept in `media/`.
- **Refused** (with the reason): scripts, event handlers, pictures, any reference that leaves the file, nested `<use>` chains, more than 2,000 shape elements or 500 nodes, more than 4 MB. See [Security](./security.md#svg-vector-import-and-path-data).
- **Not the same as `import_svg`**, which rasterizes a logo or icon to a PNG and keeps the SVG for `export_icon_set`.

### Animation review: `review_video`

> **Status: Implemented** (`crates/agent/src/review_video.rs`, `crates/core/src/scene.rs`, `crates/cli/src/review_video.rs`). Read-only; verified on macOS by the unit tests in `review_video.rs`.

`review_video {video?, samples?, bpm?, allow_unevaluated?}` (MCP) and `opendesigner review-video <project> [--video <id>] [--samples 8..16] [--bpm 30..300] [--json]` (CLI) check a whole video the way `review_frame` checks one frame, aimed at the failure of an image with text on it. It reads each design-frame scene from the document (`od_core::scene::scene_stats`: shapes by kind, how much of the frame images and vectors cover on a 32 x 32 grid, which nodes the video animates, which tracks loop) and **renders 8 to 16 evenly spaced moments** (12 by default) at 192 px on the long side with `od_render`'s `VideoRenderer`, each against the picture 250 ms later, so how much of the picture really moves is measured, camera included. It is deterministic and never changes the project. A video with footage clips is not sampled (needs a decoder), and one with no video, or only footage, answers with a plain message instead of numbers.

The answer has `summary` (plain words: the verdict and the three fixes that matter most), `score` (0 to 100, 70 or more is ready), `verdict` (`ready`, `needs work` or `not ready`), `ready` (false while any gate is unevaluated, unless `allow_unevaluated` says otherwise), `gates` (what each numeric gate found: `pass`, `fail` or `unevaluated`, with the reason), `findings` (stable codes, the scenes they are about, a message and the fix), `scenes` (per scene: vector shapes, image, raster and vector share of the frame, animated nodes, ambient loops, moving layers) and `metrics` (moving layers, share of pixels moving in 250 ms, raster against vector share, loops, camera, sound tracks, caption share, tokens against hard-coded colors).

| code | name | fires when |
|---|---|---|
| `RV001` | image-backdrop | one image covers over 60% of a scene |
| `RV002` | text-only-motion | the only animated nodes of a scene are text and rectangles |
| `RV003` | still-stretch | under 2% of pixels change for over 2.5 s (the last 2 s may hold) |
| `RV004` | no-ambient-loops | a scene of 2.5 s or more has no looping track that changes |
| `RV005` | no-camera-move | a video over 15 s never moves the camera |
| `RV006` | few-vector-shapes | a scene has fewer than 12 paths, rects and ellipses |
| `RV007` | repeated-composition | two scenes have the same layout of at least 8 nodes |
| `RV008` | captions-cover-the-frame | the widest cue is estimated to cover over 25% of the frame |
| `RV009` | no-sound | the video has no sound track |
| `RV010` | same-sound-every-scene | 3 or more scenes and no effect or voice differs between them |
| `RV011` | palette-not-from-tokens | over 12 distinct hard-coded colors and most paints are not tokens, or no tokens and over 6 colors |
| `RV012` | few-moving-layers | fewer than 3 animated layers move on average in a scene of 2.5 s or more |
| `RV013` | flash-safety | **error.** More than 3 large changes of brightness in any 1 s (see below) |
| `RV014` | cuts-off-the-beat | `bpm` given, at least 2 cuts and 40% of cuts farther than 70 ms from a beat or half beat |
| `RV015` | text-too-brief | a text is readable (opacity 0.5 or more) for less than 1 s plus 0.4 s per short word (a word over 6 letters counts for more) |
| `RV016` | text-in-unsafe-zone | vertical video (height at least 1.3 times the width): a text box more than 30% inside the top 150 px or bottom 250 px of a 1080x1920 picture, scaled by height |
| `RV017` | thin-stretch | a 2 s window with fewer than 3 distinct moving layers (animated nodes of the scenes on screen, and the camera as one) |
| `RV018` | identical-scene-repeated | two scenes of 1 s or more with at least 3 nodes and exactly the same content (kinds, boxes, text, images, paths, fills) |
| `RV019` | no-final-hold | a one-shot animation or a clip zoom is still under way in the last 0.8 s (looping ambient motion may continue) |
| `RV020` | track-drives-nothing | **error.** An animation track drives nothing: its node does not exist, the property does not apply to that kind of node, its scene is not on screen while the track plays, the node is hidden or at opacity 0, the track starts at or after the end of the video, or the value the renderer draws is not the value the track says (see `verify_motion` below). A track whose keyframes all hold one value is the same finding as a warning |

Each finding takes points off by its weight, scaled by the share of scenes it covers; the score is 100 minus the sum. The document-only parts of RV001 and RV002 are also lint rules `OD090` and `OD091` ([Lint Rules](./lint-rules.md)), so `validate_document` says the same without rendering. `export_video`'s description, the `explainer` playbook's final gate and the quality bar tell agents to call it and fix findings before exporting. `[INFERENCE]` The weights and thresholds are judgment, not measured against real films; tune them when real explainers are scored.

**Gates that could not look.** The report names the gates it could not evaluate and says why: the beat (`RV014`) without a `bpm`, the flash scan (`RV013`) of a video with footage or under a second, the still-picture measure (`RV003`) of a video with no pictures to sample, and the track check (`RV020`). An unevaluated gate is never a pass: `ready` is false while one stands, and `verdict` says `needs work` rather than `ready`. A caller that knows the missing input cannot matter (a silent video, a cut without music) passes `allow_unevaluated: true` and the gate stands as it is.

### Motion and scene gates: `verify_motion`, `scene_check`, `hit_test`

> **Status: Implemented** (`crates/agent/src/scene_checks/`). Read-only; verified on macOS by the unit tests in `scene_checks/tests.rs`.

Three tools measure what the document says about motion and geometry, in numbers. Every gate answers `pass`, `fail` or `unevaluated` with its reasons: a gate that could not look at its input says so, and an unevaluated gate is never a pass. Every length is a fraction of the frame height, never an absolute unit, so the same scene measured at two sizes reads the same, and every threshold says how far it can be trusted (`calibration`: `measured`, `derived` or `uncalibrated`; an uncalibrated threshold can only warn).

- **`verify_motion {video?, samples?, classify?, declared_loops?, gait?, expect?, ambient?}`**: for every animation track of the video (the camera's too) it samples at least 5 moments (default 7: the start, the middle and the end) and compares the value the track says (`Animation::value_at`) with the value the renderer's pipeline draws (`Document::at_time`). The difference must be within 2^-23 of the value, plus the rounding the pipeline does (4 decimals, `od_core::limits::round4`): that is G1, the track reaches the node. G2 checks the values are finite and legal (the scale product is not 0, the trim is 0 <= trim_start <= trim_end <= 1, the stroke width is not negative). G3 checks a track that repeats, or that the plan declares a loop, ends where it starts: rotation 0.5 degrees (modulo a turn), position 0.01 of the frame height, scale and opacity 0.005 (that last one is **uncalibrated**, so it only warns). A loop the plan declares as unknown (`loop: null`) is treated as a one-shot and the gate is `unevaluated`.
  A track is also checked for reachability: its node exists, the property applies to that kind of node, its scene is on screen while the track plays, the node is visible and its opacity is above 0 somewhere in that window, and the track starts before the video ends. A track that fails this drives nothing: that is finding `RV020`.
  Each track reports `id`, `node`, `prop`, `samples`, `max_delta`, `status`, `reasons` and `warnings`.
  With `classify` it also measures every animated node: `duration_s`, `travel_h`, `rise_h`, `speed_h_per_s`, `rot_range_deg`, `scale_delta`, `pose_return_deg`, `return_h`, the labels and the primary one. The labels are `idle`, `in-place`, `walk`, `run`, `dash`, `jump`, `leap`, `planted` and `gesture`; a gait label needs the node to have travelled as well as to be fast, because a node that vibrates in place is not running. Ambient life (every track of the node loops, or the node is named in `ambient`) must read as `idle` (travel and rise under 0.02 of the frame height) or it warns `ambient-too-large`. A class that names an intent (`strike`, `guard`) is reported as `intent_inferred`: no kinematic feature tells a strike from a stumble.
  `gait` checks the foot slide: while a foot is in a declared stance it may not slide more than 0.01 of the character height (the ground's own scroll is subtracted). A gait with no stance is `unevaluated`.
- **`scene_check {video?, layer_plan?, details?, camera?, freeze?, added_by?, freeze_snapshot?, footprint?}`**: the geometry gates of the plan, each `pass`, `fail` or `unevaluated` with reasons, run at the union of the keyframe times of the chains involved and 12 even moments of the video.
  - `bleed`: a layer flagged `bleed:true` must cover the frame under every camera pose. The camera pans the scene by `camera_x * depth` and magnifies it by `camera_zoom ^ depth` about the frame's centre (`od_core::transform::Camera::affine`, which is what the renderer does), so a layer at depth `d` has to reach `camera_x * d` beyond the edge of the frame, and further when the camera zooms out.
  - `attach`: a child with a `parent` in the plan must stay within `attach.gap_tolerance_px` (2 px at 1080p, scaled by the frame height) of the parent's box at every sampled moment.
  - `chirality`: a left and a right part are a reflection of each other (the lateral axis negated, the turn mirrored), never a half turn; the left one is on the viewer's right when the character faces front. When the sides were assigned by convention the sign is `unevaluated`.
  - `coverage`: every plan entry is a node (a missing one fails), no two entries are the same node (two layers fused into one fails), nodes no entry names warn, and details that map to nothing warn.
  - `motion_roster`: every layer that expects motion has a track.
  - `freeze`: the shapes frozen after the shapes pass have not changed. The hash of a node covers its kind, position, size, path, view box, fill rule and its children in order, over the numbers' bit patterns and not their text, so a change in the last decimal is still a change. `freeze_snapshot` returns the hashes and the attributes to store; a later pass may only add the nodes it names in `added_by`.
- **`hit_test {video?, x, y, t?}`**: the node ids under a point at a moment, topmost first: the discriminating test for "what is covering this?". The point is in the frame's own units, as the design shows it at that moment; a moving camera is not applied.

### Optional programs: `get_capabilities`, `transcribe`, `remove_background`

> **Status: Implemented** (`crates/agent/src/external_tools.rs`, `crates/media/src/{external,speech,cutout}.rs`, `crates/cli/src/external.rs`). Security: [External programs](./security.md#external-programs).

Three tools drive programs the user installed (nothing is bundled, installed or uploaded by OpenDesigner). **Call `get_capabilities` first** (read-only; MCP, and `opendesigner doctor [--json]` for the user): it lists `ffmpeg`, `whisper-cli`, `whisper` and `rembg` with path, version (`--version`, 15 s timeout), what each is for, the override variable and the exact install hint when absent, plus `can {export_video, transcribe, remove_background}` and `missing`. When something is missing the agent tells the user what to install instead of failing mid-task. `get_guidance` carries one line (`optional_programs`) naming what is missing, without running anything.

- **`transcribe {file | video, language?, model?, apply?, offset_ms?, min_pause_ms?, timeout_s?}`** (and `opendesigner transcribe <project> ...`). `file` is a `media/` audio or footage file; `video` transcribes that video's own sound mix (`preview_audio::prepare`), so times are timeline times. ffmpeg first writes a 16 kHz mono WAV into a scratch folder; then whichever speech program is found (`OPENDESIGNER_WHISPER`, then `whisper-cli`, `whisper-cpp`, `whisper`): whisper.cpp needs a model file (`OPENDESIGNER_WHISPER_MODEL`, or `model` as a plain file name in the app's `models/` folder or `media/`), the Python whisper takes a model name (default `base`). Each program's JSON becomes the same word list (`od_media::speech`); the answer has `text`, `words` (`{text, start_ms, end_ms}`, up to 3,000), `pauses` (gaps between words of at least `min_pause_ms`, shaped like `detect_pauses`) and, with `apply: true` (`video` required; `offset_ms` with `file`), `captions`: the words go through the existing `import_captions` (`words_json`), one attributed, undoable change that replaces the cues.
- **`remove_background {file, model?, mask_only?, timeout_s?}`** (and `opendesigner remove-background <project> --file media/x.png [--model] [--mask-only]`). Runs `rembg i` on a `media/` png/jpg/webp (25 MB cap) and writes `media/<name>-cutout.png` (or `-mask.png`; `-2`, `-3` when the name is taken) plus `<name>-cutout.rembg.json` {source, tool, model, "processed locally"}. The result is an ordinary image asset for an image node.
- Tests run the code against fake programs (shell scripts printing each tool's canned JSON): detection order, arguments, both JSON formats, missing program or model, timeout, cancel flag, output cap, no write outside the scratch folder and the project. A real whisper.cpp, OpenAI whisper or rembg has **not** been run here `[INFERENCE: argument spellings follow each tool's documented command line]`; word splitting for whisper.cpp uses `-ml 1 -sow`.
- Not built: a progress job for these calls (they hold the editor until they finish), speaker labels, translating, streaming.
**Flash safety (RV013).** A video with footage cannot be scanned (it needs a decoder); otherwise the whole video is drawn at 48 px on the long side, 15 pictures a second (at most 1,800 pictures, so the first 120 s of a longer video), and the mean relative luminance (sRGB linearized) of each picture is taken. A *change* is a run of brightening or darkening, ended by a reversal, that moves the luminance by at least 0.1 with the darker end below 0.8; a fade is one change, a cut to white and back is two. Four changes within 1 s (`max_per_second` in `metrics.flash_changes_per_second_max`) is the finding. It is the only `error`: its `severity` is `error` in the JSON, the verdict is `not ready` and the score cannot exceed 54. `[INFERENCE]` This is a screening heuristic modelled on the common three-flashes-per-second guideline (WCAG 2.3.1 uses relative luminance change over a screen area, not the whole-frame mean); it is not a certified photosensitivity test.

**Tempo (RV014).** A document has no tempo, so `bpm` is an argument (30 to 300; anything else is refused). The beat grid starts at the first `music` track's `start_ms` (0 without one) and has a line every half beat; a cut is the start of the clip, or the middle of its transition.

**Overlap pass.** `review_video` also answers `overlap`: `{regions, total, note}`. For each scene, at 25%, 50% and 75% of its length, the direct children of the frame (the layers, as blocks and characters are groups) are looked at in drawing order, skipping any that cover 60% or more of the frame (the backdrop) or under 0.1% (specks). Each pair whose boxes overlap by at least 1% of the frame is a region with the scene, the moment, the rect in frame units, `back_to_front` (the lower layer's name, then the upper's), the node ids and a `check` line telling the agent to `video_crop` it. A pair is listed once, at its largest overlap; at most 12 regions are returned, largest first (`total` counts them all). It is information, not a finding: it takes no points off. Boxes are bounding boxes in the frame (camera not applied), so a region may overlap only in its corner; the crop is the judge.

### Inspection tools: `video_strip`, `video_crop`, `frame_grid`, `video_diff`

> **Status: Implemented** (`crates/agent/src/inspect.rs`, `inspect/{draw,geometry}.rs`, `crates/cli/src/inspect.rs`). Read-only; verified on macOS by the unit tests in `inspect/tests.rs` and `crates/cli/tests/inspect.rs`.

Agents judge their own animation from pictures, so these return one PNG image block and a line of text, like `video_contact_sheet` (the MCP image content type). None changes the project.

- **`video_strip {video, from_ms, to_ms, every?, cell_side?, locale?}`**: every frame (every `every`th) from `from_ms` to `to_ms` included, at the video's frame rate, in a grid (6 to a row, 9 for a portrait video), each picture labeled with its time in milliseconds by a built-in 3 x 5 pixel font. At most 36 pictures: a longer range is refused with the `every` that fits. `cell_side` 64 to 512 (default 192). Times must be inside the video.
- **`video_crop {video, time_ms, x, y, width, height | node, padding?, zoom?, locale?}`**: the picture is drawn at the video's full size and the rectangle cut out, so it is 1:1. With `node`, the rectangle is the node's box at that moment plus `padding` (default 24, at most 512 video pixels): its box in the frame (rotation and scale included), moved by the camera as far as its depth (times its parents') says, scaled by how the clip covers the video and by the clip's slow zoom (`inspect/geometry.rs`). It is refused when the node is hidden or no clip or overlay holds its frame at that time. A transition's slide or fade is not part of the box. The result is at most 2,048 px on a side; `zoom` 2 to 4 enlarges each pixel without smoothing.
- **`frame_grid {video, time_ms, spacing?, max_side?, locale?}`**: the picture (longest side `max_side`, 256 to 1536, default 768) with lines every `spacing` video pixels (default: the first of 10, 20, 25, 50, 100, 200, 250, 500, 1000, 2000 that gives at most 12 lines along the long side), numbered in video pixels, x along the top and y down the left, so what the agent reads can go straight into `video_crop` or `apply_ops`.
- **`video_diff {video, time_ms, other_time_ms?, other_video?, mode?, threshold?, max_side?, locale?}`**: the picture at `time_ms`, the picture at `other_time_ms` (of `other_video` when given, which must be the same size), and a dimmed copy of the first with every pixel that differs by more than `threshold` (default 12 of 255 in any channel) painted magenta; `mode` `overlay` returns only the last. The text gives the share of pixels changed and their bounding box in video pixels. The same time of the same video is refused. It compares two moments or two videos, not two states of one video over time: to compare an edit, keep the earlier version as a second video, or compare the moment before and after the thing you changed. `[INFERENCE]` A before/after of the same video would need a saved reference picture; none is kept.

`opendesigner inspect <project> strip|crop|grid|diff --video <id> ... [--out <name>.png]` runs the same code and writes the PNG to `exports/` (a plain `.png` name, an `exports/` folder or file that is a symbolic link is refused, nothing is written elsewhere) and prints the path and the text. Footage videos need `ffmpeg` found as for `render_video_frame`; nothing else leaves the process.

The `review` playbook (`get_playbook review`, `crates/agent/skill/review-playbook.md`) is the process around them: a contact sheet, strips of every key motion, crops of every face and contact, at most three automatic rounds and then ask the user, harsh critique (a flaw is what you see, at which second, why), and a checklist (legibility, clarity, density, motion, photosensitivity, rhythm, origin and physics, continuity, composition, ending as poster, detail at 1:1). The `explainer` final gate and the `video` quality gate point to it.

### Footage, timeline export and audio-driven motion: `watch_footage`, `index_footage`, `search_footage`, `export_timeline`, `audio_reactive`

> **Status: Implemented** (`crates/agent/src/{footage,timeline_export,audio_reactive}.rs`; mechanics in `crates/media/src/footage.rs` and `crates/core/src/interchange.rs`). Details, numbers and limits are in [Video Editing](./video-editing.md#footage-memory), [timeline export](./video-editing.md#timeline-export) and [audio-reactive motion](./video-editing.md#audio-reactive-motion). Ideas from the sources in [Prior Art](./prior-art.md#video-agent-sources-2026-11-22).

- **`watch_footage {file, question?, frames?, from_ms?, to_ms?, cell_width?}`** (read-only): up to 10 evenly spaced pictures of a `media/` recording as one labeled grid, to look at before writing a note or choosing a cut. Needs ffmpeg.
- **`index_footage {file, threshold?, min_shot_ms?, rescan?, transcribe?, language?, model?, timeout_s?, notes?}`**: cuts a recording into shots, optionally adds a word-timed transcript from the user's own whisper (call `get_capabilities` first), keeps the agent's `notes: [{shot, note, tags?}]` and stores it all in `footage-index/`. Not a document change: no checkpoint, no `pixel_verified` effect.
- **`search_footage {query?, file?, limit?}`** (read-only): finds shots by words against notes, tags and transcript; each result carries a `clip` for `add_clip`. Needs neither ffmpeg nor a speech program.
- **`export_timeline {video, format, file?}`**: a CMX 3600 EDL, SRT or WebVTT in `exports/`; the answer lists what an EDL cannot hold in `warnings`, to be told to the user. It is a hand-over like `export_video`: it warns about changes that were never looked at.
- **`audio_reactive {video, node, prop, file, band?, min?, max?, start_ms?, from_ms?, to_ms?, keys_per_second?, floor?, attack?, release?, additive?}`**: keyframes that follow the loudness (or the bass, mid or treble) of a sound, as one attributed, undoable `animate`. Look at the result with `video_strip` (the answer says so).

### Formats, noise and fit to media: `export_video`, `animate_noise`, `fit_clip_to_media`

> **Status: Implemented** (`crates/agent/src/{video_export,noise_tool,fit_media}.rs`). Numbers and limits: [Video Editing](./video-editing.md#formats-and-parts), [noise motion](./video-editing.md#noise-motion), [fit to media](./video-editing.md#fit-to-media). Ideas from Remotion's docs ([Prior Art](./prior-art.md#video-agent-sources-2026-11-22)).

- **`export_video {video, file, preset?, locale?, loudness?, from_ms?, to_ms?, every_nth_frame?, transparent?, width?, gif_loops?}`**: by the file's ending an H.264 MP4, a GIF, a VP9 WebM or a ProRes MOV; `from_ms` and `to_ms` write a part (and its sound, in an MP4); `every_nth_frame` thins the frames (the sound is dropped); `transparent` keeps uncovered ground clear in a GIF, WebM or MOV. The answer adds `format`, `transparent`, `from_ms`, `to_ms` and `every_nth_frame`.
- **`animate_noise {video, node, prop | props, amplitude, ...}`**: seeded smooth noise as keys, additive by default; `loop` for a seamless loop.
- **`fit_clip_to_media {video, clip, file?, pad_ms?}`**: one `set_clip` that makes a clip as long as a file (or the rest of its own footage).
- **Transitions** `flip` and `clock_wipe` join the list `set_clip` and `add_clip` take.

### Depth layers from a picture: `layer_image`

> **Status: Built** (`crates/agent/src/layer_image.rs`, `layer_image/planes.rs`). `[INFERENCE]` Tested on synthetic pictures only; not tried on real photographs or with a real depth model.

`layer_image {file, mask | depth, bands?, invert_depth?, mask_threshold?, grow?, feather?, parent, name?, x?, y?, width?, overscan?}` cuts one picture in `media/` into depth layers for a camera move (2.5D parallax). The grey plane that says what is near comes from the user's own programs, as `transcribe` and `remove_background` do: a `mask` (a subject mask or a transparent cut-out, from `remove_background` with `mask_only`) makes two layers, a `depth` map (brighter is nearer, or `invert_depth`) makes `bands` layers (2 to 6) of equal area. Equal depths share a band, so a flat map is refused ("only one level").

Each band becomes `media/<name>-layer-<n>.png` (never overwriting; a `<name>-layers.json` records the source, plane and settings) and an image node of one group, far layer first, with `depth` from 0.15 (far) to 1 (near), so `camera_pan`, `camera_push_in` and `camera_drift` move the near layers further. A layer holds its own pixels, plus the colours grown out of its edge under the nearer bands (`grow`, default 40 px, a breadth-first fill from the band's own pixels, so nothing is invented), so a move does not uncover a hole; the lowest layer is grown to fill the picture. The edge towards a farther band is softened over `feather` pixels (default 1). Layers are overscanned by 6 percent so a camera move has room. Pictures over 24 million pixels are refused. One attributed, undoable turn.

Licences: the weights of the programs the user runs have their own terms. `rembg`'s newer default model is CC BY-NC 4.0, so for a commercial project name `isnet-general-use` or `u2net`; Depth Anything V2 Small is Apache-2.0 and the larger sizes are non-commercial. Nothing is downloaded or uploaded by OpenDesigner.

### Kinetic type: `split_text`, `text_reveal`

Status: **Built** (`crates/agent/src/text_split.rs`, the layout read is `od_render::text_lines`). Both tools write ordinary nodes and ordinary keyframes in one attributed transaction, so one undo puts the text back.

- **`split_text {node, by, wrap?, mode?, turn?, summary?}`**: `by` is `chars`, `words` or `lines`. The text node becomes a group (the original's box, transform, opacity, blur and shadow) of one text node per piece, named `<name>.c0..`, `.w0..` or `.l0..` in reading order, with the original font, size, weight, line height, fill and stroke. Spaces are not nodes. Each piece sits where its glyphs were drawn: x and the baseline row come from the renderer's own shaping and wrapping (the lines are the rows the box width really breaks to, not a guess), rounded to whole pixels. `wrap: true` puts every piece in a clipping group `<name>.<piece>.mask` (the piece's line box with a small side pad) so it can rise or slide in from outside the box; the animated node is then the text inside the mask. `mode: replace` (default) deletes the original and the group takes its name; `keep` hides the original (`visible: false`) and names the group `<name> split`. The result lists `ids` (the nodes to animate, in order, for `animate_preset` stagger or `sequence`), `masks`, and each piece's box. At most 300 pieces and the document's node limit; a bigger split is refused with the cut that fits.
- **`text_reveal {video, node, style, by?, wrap?, mode?, each_ms?, at_ms?, duration_ms?, ease?, turn?, summary?}`**: splits the node as above (or animates a group `split_text` made earlier as it is) and writes the keys. `style` and its defaults: `rise` (450 ms, `power4_out`, `y` from one line below the mask to rest; masked by default; unmasked it also fades in over 60% of the time), `fade` (400 ms, `ease_out`), `pop` (350 ms, `back_out(1.2)`, `scale` 0.5 to 1 and a fade over the first 40%), `slide` (500 ms, `power4_out`, `x` from one font size to the left, or from the mask's width when masked), `typewriter` (each piece's opacity jumps 0 to 1 within 1 ms). Stagger between pieces: 40 ms per character, 90 per word, 140 per line (typewriter 33, 120, 400). `by` defaults to characters for a one-line headline of 12 characters or fewer, else words (lines past 4 lines or 40 words); typewriter defaults to characters up to 80 of them. The tracks replace any a piece already has. It answers with the resolved timing (every piece's `start_ms` and `end_ms`, `ends_at_ms`), `hold_ms` (1 s plus 0.4 s per short word, the `RV015` rule, over the whole text) and `leave_no_earlier_than_ms`, plus `warnings` when characters are revealed one by one on more than 12 characters, or `each_ms` is outside 30 to 50 ms (characters) or 80 to 120 ms (words).

What it cannot do: kerning between two pieces is lost when splitting by characters (words and lines keep theirs inside the piece); a ligature (`fi`) is one glyph and so one character piece; pieces are left-aligned at their drawn position, so centre or right alignment is baked into the positions and a later edit of the box width does not re-wrap them; right-to-left and mixed-direction text is ordered by logical position and only pure runs are checked `[INFERENCE]`; a text bound to a template variable or a localised string is split from the text it shows now and the pieces no longer follow it; position is rounded to whole pixels, so an at-rest split matches the original to within sub-pixel anti-aliasing, not bit for bit (the test allows a few percent of ink pixels to differ for characters, fewer for words and lines).

### Layers and layout moves: `animate_to`, `snapshot_layout`, `transition_layout`

Status: **Built** (`crates/agent/src/layering.rs`, `crates/agent/src/layout_transition.rs`; semantics in [Video editing](./video-editing.md#additive-layers-and-implicit-from)). All three write ordinary `animate` operations in one attributed, undoable turn.

- **`animate_to {video, node?, prop, to, at_ms, duration_ms, ease?, keep_later?, turn?, summary?}`**: starts from the value at `at_ms` (the node's track, else the node's own value) and replaces the old track's keys from there. `to` is a number, or a `'#rrggbb[aa]'` string for `fill_color`; the camera props need no `node`. Returns `from`, `to`, `keys`, `replaced_keys` and the usual receipt. Refused: a looping track, a value it is already at, a prop the node lacks.
- **`snapshot_layout {frame, name?}`** and **`transition_layout {video, snapshot, at_ms, duration_ms?, ease?, stagger?, turn?, summary?}`**: record where the nodes of a frame are, edit, then animate from the record to the live document (moved, resized, entering and leaving nodes are listed in the result's `table`). Snapshots are JSON files in the project's `layout-snapshots/` folder with plain names, no links, size and count caps.
- **Additive layers** need no tool of their own: `apply_ops` `animate` takes `additive: true`, and `animate_preset` takes `layer: "additive"`.

Not verified: that agents choose `animate_to` over rewriting keys by hand, and the transition of turned or scaled nodes `[INFERENCE]`.

### Paths and morphing: `animate_arc`, `animate_keyframes`, `text_on_path`, `morph_to`

Status: **Built** (`crates/agent/src/path_motion.rs`; semantics in [Video editing](./video-editing.md#paths-and-morphing), the shape data in [Document format](./document-format.md#vector-shapes)). Each is one attributed, undoable turn; the first two write ordinary keys.

- **`animate_arc {video, node, from?, to, at_ms, duration_ms, ease?, arc?, orient?, turn?, summary?}`**: a quadratic-Bezier travel sampled into x/y keys every ~33 ms; `arc` is `{strength, peak, direction auto|cw|ccw}`; `orient` adds a tangent-following rotation (unwrapped). `animate_preset` takes the same `arc` object for scene moves that travel in x or y (`slide_in_*`, `bounce_in`, `fade_up`) and refuses it for the rest.
- **`animate_keyframes {video, node?, keyframes, at_ms?, duration_ms, ease?, allow_over?, turn?, summary?}`**: `{"0%": {"x": 0}, "40%": {"x": 200, "ease": "ease_out"}, "100%": {"x": 400}}` over several properties; a keyframe's ease shapes the move that leaves it; the result says when an ease is unused.
- **`text_on_path {text_node, path_node, start_offset?, align?, mode?, reveal?, turn?, summary?}`**: one text node per glyph standing on the path's curve, rotated to its tangent, in a group; `reveal {video, preset?, each_ms?, at_ms?, duration_ms?, ease?}` staggers a scene move over the letters. Reports `ids`, `count` and `off_the_end` (glyphs that fell off an open path).
- **`morph_to {video, node, to_path, at_ms, duration_ms, ease?, reset?, turn?, summary?}`**: sets the node's `path_morph` (equal-length point lists of both shapes) and a `morph_progress` track; call it again after the first ends to continue to a third shape. The result lists `points`, `contours`, `stops` and the progress keys.

Not verified: that agents reach for `animate_arc` and `morph_to` rather than hand-written keys or swapped path nodes `[INFERENCE]`.

### Scene seeds: `list_seeds`, `apply_seed`

Status: **Built** (`crates/agent/src/seeds.rs`, the data in `seeds/`, playbook topic `seeds`). Six starter scenes, each a deterministic recipe over the blocks `add_block` already has (`apply_seed` calls the same code path), plus the 3D phone node and `animate_preset`:

| Seed | Builds | Length |
|---|---|---|
| `explainer-opener` | `sky_gradient`, `clouds`, `parallax_hills`, `particles`, `eyebrow_label`, `kinetic_headline`, `character` (waving), `speech_bubble` | 9 s |
| `product-reveal` | `sky_gradient`, `particles`, `eyebrow_label`, `kinetic_headline`, a `device3d` phone (`hero_reveal`, then `float`), `badge_pill`, `cta_button` | 9 s |
| `kinetic-quote` | `sky_gradient`, `particles`, `eyebrow_label`, `kinetic_headline`, `lower_third` | 8 s |
| `stats-callout` | `sky_gradient`, `grid_floor`, `particles`, `eyebrow_label`, `stat_callout`, `donut_chart`, `highlight_ring` | 9 s |
| `before-after` | `sky_gradient`, `clouds`, `particles`, two `badge_pill`, two `progress_bar`, `connector`, `stat_callout`, `highlight_ring` | 10 s |
| `logo-sting` | `starfield`, `particles`, `logo_sting`, a `camera_push_in` | 6 s |

- **`list_seeds {}`** returns each seed's id, what it is for, its length, its blocks, its beats and its layout notes (the `seeds/layouts/<id>.md` file: where each element sits as a fraction of the frame, and what to swap).
- **`apply_seed {id, video?, brand?}`** is one attributed turn (one `turn` id, one "before" checkpoint, so one undo or one `restore_checkpoint` removes the whole scene). It creates a frame (placed beside existing frames), a video in the workspace's chosen video format (else 1280x720 at 30 fps) or uses the empty `video` you name (its size is used; a video with clips is refused), one clip of the seed's length, then the blocks, an ambient `camera_drift` (or a `camera_push_in`) and a SceneSpec in `scene_specs/<id>.json` (`<id>-2`, `<id>-3` for later applications, so applying twice never collides). Block colors are the applied style pack's tokens (`{bg}`, `{fg}`, `{accent}`, `{muted}`) when the workspace brief names a pack and the document has them, else the block defaults; `brand.accent`, `brand.background` and `brand.foreground` (`#rrggbb`) override either, and `brand.name`, `headline`, `tagline`, `cta` replace the seed's copy. If any step fails the turn is restored to its checkpoint and the error says so.
- **The spec** has `seed`, the palette (the pack's eight tokens, else bg, fg, accent, muted), `layer_plan` (a layer per block with its node id, level, role, depth and `expects_motion`), `motion_plan`, `detail_inventory`, two feature targets, `audio_plan` (a music track and a cue per beat that names one) and a `beat_map` of 3 to 4 beats, all `built`. Its routing is resolved from the seed's kind.
- **The result** returns `layers` (layer id to node id), `beat_times`, the `layouts` notes, `p0_gates` (`validate_document` and `scene_spec_validate` run at build time; the strict check too, which every seed passes, any open finding listed; `review_video`, the contact sheet, the copy swap and the sound as to do) and the `scene_status` (`null` without a project folder, where the spec is not saved either).

The seeds use no picture backdrop on purpose: `aurora_backdrop`, `gradient_wash`, `grid_backdrop` and `noise_backdrop` are full-frame pictures and trip `RV001`; the seeds take vector scenery (`sky_gradient`, `grid_floor`, `clouds`) instead.

What it does not do: it does not render or look at the scene, add sound, or import images (a `device3d` has no capture until you set one). `review_video` is not run inside the tool, because it renders a sample of frames; the tests run it on every seed and require no RV001 and no RV002. [INFERENCE] A seed's motion is calm and generic: it is a starting point, and a scene with a brief of its own should change its beats.

### Pixel-art assets: `export_atlas`, `quantize_image`

`export_atlas` writes a sprite sheet or atlas (PNG plus JSON manifest) from a grid frame or a list of frames; `quantize_image` snaps a `media/` picture to a palette. The tools `render_preview`, `export`, `export_set` and `export_panorama` also accept `pixel_art: true`. Schemas and limits: [Store assets](./store-assets.md#pixel-art-game-assets). CLI: `opendesigner export-atlas`, `opendesigner quantize-image`, `opendesigner export --pixel-art`.

### Agent plugin

`plugins/opendesigner/` is the skill packaged for agents that install through a plugin marketplace: `.claude-plugin/plugin.json` (Claude Code), `.codex-plugin/plugin.json` (Codex) and `skills/opendesigner/` (the output of `skill::generate()`), with `.claude-plugin/marketplace.json` at the repository root pointing at it. `scripts/package-agent-plugin.sh` (or `opendesigner package-plugin [--check]`) regenerates every file from the tool schema; `plugin::tests::the_committed_plugin_is_exactly_what_the_generator_writes` fails while the committed files differ, so run the script and commit after changing a tool, a playbook or the version.

The plugin holds the skill and nothing else (invariant 6): no `mcpServers`, no token, no endpoint, no hooks. The MCP connect step stays the explicit one the app shows (Connect tab, or `opendesigner serve-mcp`), because the endpoint needs the app running and a per-install token; the plugin's README explains the flow. The app's installer remains the explicit install path. `[INFERENCE]` The manifests follow the published Claude Code and Codex plugin formats; neither has been run against a live install here.

## Headless

`opendesigner` CLI runs the same operations against a project folder without a window: `export` (one frame, or `--batch` rows of template variables), `validate`, `apply` (a file of operations, `--dry-run` to check them first), `lint` ([rules](./lint-rules.md)), `package-plugin`, `serve-mcp`. Used in CI and by agents on machines with no display.
