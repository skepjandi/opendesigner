# Prior Art

> **Status: Research** (as of 2026-11-19). Facts come from each product's own public pages; inferences are labelled. **Not re-checked:** pricing, star counts beyond what is quoted, and anything about Penpot and Figma beyond their MCP pages.

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

## Ideas taken from open projects (2026-11-26)

Read for ideas only; no code, text or assets were copied, and everything is rebuilt as data operations. Each project's licence was read on its repository page.

| Project | Idea we took |
|---------|--------------|
| [Rough.js](https://github.com/rough-stuff/rough) (MIT) | A seeded hand-drawn outline: wobble, bowing, strokes drawn twice, hachure and cross-hatch fills: the `sketch` block |
| [p5.brush](https://github.com/acamposuribe/p5.brush) (MIT) | Watercolour washes built from stacked deformed layers, hatching, strokes along a vector field: `watercolor_wash`, `flow_strokes` |
| [Whisper](https://github.com/openai/whisper) (MIT) and [rembg](https://github.com/danielgatis/rembg) (MIT) | Word timestamps and background removal, run as the user's own installed programs: `transcribe`, `remove_background` |
| [drawn-by-code](https://github.com/illodev/drawn-by-code) (licence "Other": ideas only) | The self-review loop: contact sheets, frame strips, 1:1 crops, a layering pass, flash safety, beat alignment, a harsh-critic rule |
| [ClaudeAnimationBase](https://github.com/JohnHeibel/ClaudeAnimationBase) (MIT) | Acting with many expressions and painted marks instead of text; animating on twos; timing as "reads" the viewer must take in |
| [claude-animated-shorts](https://github.com/mromidev/claude-animated-shorts) (MIT; reference frames for study only) | The short-form script shape (paradox hook, pivot, ironic payoff), ease-out camera bursts, loudness to -14 LUFS |
| GSAP (from a guide) | The ease family (power, expo, circ, sine, steps) and stagger from edges or a grid |

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
