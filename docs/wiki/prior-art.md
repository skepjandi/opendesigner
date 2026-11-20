# Prior Art

> **Status: Research** (as of 2026-11-19). Facts come from each product's own public pages; inferences are labelled. **Not re-checked:** pricing, star counts beyond what is quoted, and anything about Penpot and Figma beyond their MCP pages.

## Kino ([kino.ai](https://kino.ai))

"AI video editor for teams and agents." Footage storage, editing and review in one place; replaces Premiere/Resolve/Final Cut/Descript in its own copy. Desktop apps for macOS (Apple Silicon and Intel), Windows and Ubuntu.

- **Agents on the timeline.** A chat panel with a built-in "Kino Agent" (billed in Kino credits) or Claude Code / Codex using the user's own subscription. Clips the agent changed get a blue outline; every agent edit saves a checkpoint.
- **MCP server in the desktop app.** `http://127.0.0.1:45680/mcp` (loopback; the app must stay open); free tier 100 metered MCP calls a week, unlimited on paid tiers; install buttons for Claude Desktop, ChatGPT, Cursor, VS Code, one-line commands for Claude Code and Codex CLI. Marketing says "40 tools".
- **Built on Remotion;** "Kino projects are code, so Claude Code reads one like a repo"; skills (a brand-clone skill is shown) steer the agent's editing.
- Cloud: team storage, share/review links with comments that "auto-apply", cloud agents, a headless mode for Linux servers.

| Observation | For OpenDesigner |
|-------------|------------------|
| Agent and human edit the same timeline; agent changes are visibly marked and checkpointed | Copy: outline plus checkpoint ([Document Format](./document-format.md#history-and-checkpoints)) |
| One local MCP endpoint serves every agent | Copy, with token and Origin/Host checks ([Security](./security.md#mcp-endpoint)) |
| Skills carry editing taste | Ship a skill; generate it from the tool schema ([Agent Integration](./agent-integration.md#the-skill)) |
| Guidance is chat text plus @-references | **Our differentiator:** selection, pins and hand edits as structured guidance |
| Cloud storage, accounts, credits | Not copied: local files, no accounts ([AGENTS.md](../../AGENTS.md)) |
| Code-defined graphics (Remotion) | Not copied: documents are data ([invariant 3](../../AGENTS.md)) |

## OpenCut ([opencut-app/opencut](https://github.com/opencut-app/opencut))

Open-source video editor ("the open-source CapCut alternative"), MIT. The original TypeScript web app is archived (`opencut-classic`); the repository is a **rewrite**: Rust core, plugin-first, one codebase for desktop/mobile/browser, MCP server, headless mode, scripting tab. Monorepo: `apps/web` (TanStack Start, React 19), `apps/api` (Elysia on Cloudflare Workers), `apps/desktop` (GPUI), `crates/media` (pinned FFmpeg), orchestrated with moon and proto. Not taking outside contributions yet. Details and what we take: [Video Editing](./video-editing.md#what-opencut-teaches).

| Observation | For OpenDesigner |
|-------------|------------------|
| GPUI desktop with panel entities and OKLCH tokens | The scaffold shows a workable `Shell`/`panels`/`theme` split, which we reuse in Qt; we chose Qt over GPUI ([Qt Desktop](./qt-desktop.md#why-qt)) |
| FFmpeg pinned with checksums, LGPL shared, CI prebuilts | Same toolchain policy |
| Logic in Rust, UI replaceable | Core rule ([Architecture](./architecture.md)) |
| Desktop app is still a scaffold | Do not depend on it; read it, do not port it |

## HyperFrames ([heygen-com/hyperframes](https://github.com/heygen-com/hyperframes))

Open-source (Apache-2.0) framework that renders HTML, CSS and seekable animation (GSAP, Lottie, Three.js, WebGL shaders) to deterministic MP4 through headless Chrome and FFmpeg, built for coding agents. Read 2026-11-20 from its README and docs; its source was not read. A free Studio desktop app (HeyGen account sign-in; a paid Pro plan adds HeyGen models) and a browser editor sit on top; a hosted MCP connector, AWS Lambda and Cloud Run rendering, and share links are HeyGen cloud.

- **Agents on the picture.** Studio has Select, Draw and Comment tools, a "Pending video edits" queue sent to the agent as one batch, comments pinned to a moment, and a per-run Undo. Pointing as guidance is therefore not unique to us for video.
- **Catalog and skills.** About 450 installable blocks, nine skills shipped through plugin marketplaces (Claude Code, Codex, Cursor, Copilot).
- **Templates.** Typed variables with `portrays` metadata, `--variables` and `--batch rows.json`.
- **Agent rails.** A linter with stable rule codes, `can()` dry-run, receipts (`dispatched`, `saved`, `verified`), a frame-capture tool so the agent sees its result.

| Observation | For OpenDesigner |
|-------------|------------------|
| Any web content is a layer; documents are HTML | Not copied: documents are data ([invariant 3](../../AGENTS.md)); motion is typed keyframes, blocks and looks instead |
| Block catalog | Built smaller: 31 blocks that expand into ordinary operations ([Store Assets](./store-assets.md)) |
| Lint codes, dry-run, receipts, variables, batch | Built: [Lint Rules](./lint-rules.md), `apply_ops` `dry_run`, `batch_export` |
| Preview and render share one runtime | Ours: preview and export share `render`; preview is a cached, reduced-size draw |
| Account, cloud render, Pro models | Not copied: local files, no account |

## Penpot ([penpot.app](https://penpot.app))

Open-source design tool. Ships an official **MCP server** that works through Penpot's Plugin API ("data queries, transformation and creation operations") for any MCP-capable AI client, free.

| Observation | For OpenDesigner |
|-------------|------------------|
| Agents reach the design through a plugin API and MCP | Our MCP is native to the core, not a plugin layer |
| Browser-based, server-backed | We are a local native app with files |
| Strong layout model (flex/grid auto layout), components, tokens | Reference for our node model ([Document Format](./document-format.md)) |

`[INFERENCE]` Penpot's storage and rendering internals were not read for this page.

## Figma ([figma.com](https://www.figma.com))

The reference product for UX. Its remote **MCP server** (`https://mcp.figma.com/mcp`) gives agents design context and, in beta, lets them **write to the canvas** (frames, components, variables, auto layout); it requires a supported client from Figma's catalog and certain paid plans for write access.

| Observation | For OpenDesigner |
|-------------|------------------|
| Agents read and write the canvas over MCP | Same capability, local and open to any MCP client |
| Guidance is still the agent's prompt text | Selection and pins as structured input |
| Closed, account-based, plan-gated | Open files, no account |

## The two reference projects (our own)

[GitNotēs](https://github.com/skepjandi/gitnotes) and [OpenMedia](https://github.com/skepjandi/openmedia) are the first users. Their asset pipelines, the pain points and the specs they encode are in [Store Assets](./store-assets.md). Their documentation system (wiki-first `AGENTS.md`, worktrees, `CHANGELOG.md` for single-PR changes, `docs/wiki/` mirrored to the GitHub Wiki, status banners) is the one this repository follows. GitNotēs' `gitnotes-bridge` is the precedent for driving agents over the Agent Client Protocol ([Agent Integration](./agent-integration.md#in-app-chat)).

## Gap this project fills

Design tools have MCP servers; video editors have MCP servers; agents can already drive both, and Hyperframes Studio lets the user point, draw and comment on a video for the agent. What none does is use the same structured pointing for stills *and* video, through typed, validated, attributed operations on open data files, produce store-compliant asset sets with validated presets, and run as a native app with no account. That intersection is the product.
