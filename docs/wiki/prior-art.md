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

## Video-agent sources (2026-11-22)

Read for ideas only; no code, text or assets were copied. Each row says what the source does (from its own page, as read that day) and what we did with it. Remotion's licence is "Other" (a company licence is required in some cases); FrameScript's licence was not read; none of their code is used.

| Source | What it does | What we took, and what we did not |
|--------|--------------|-----------------------------------|
| [Remotion](https://github.com/remotion-dev/remotion) (README, `docs/audio/visualization`, `docs/noise`, `docs/transitions/presentations`, `docs/transparent-videos`, `docs/render-as-gif`, `docs/dynamic-metadata`; read 2026-11-22) | Videos as React code: agentic, interactive and programmatic; Agent Skills shipped inside the repository; Lambda, Vercel and client-side rendering; Player and Editor Starter; captions, effects, sound effects, shapes. `visualizeAudio()` gives a spectrum for the current frame. `@remotion/noise` has `noise2D/3D/4D`. Transition presentations: blurSlide, bookFlip, clockWipe, crosswarp, crossZoom, cube, dissolve, dreamyZoom, fade, filmBurn, flip, iris, linearBlur, none, pushCut, ripple, slide, swap, wipe, zoomBlur, zoomInOut. Transparent video is VP8 or VP9 with `yuva420p` in a WebM, or ProRes 4444 with `yuva444p10le`, from PNG frames. GIFs have `everyNthFrame` and a loop count. `calculateMetadata()` makes a composition as long as the media it plays | **Took** audio-driven motion as `audio_reactive` ([Video Editing](./video-editing.md#audio-reactive-motion)); seeded smooth noise as `animate_noise` ([noise motion](./video-editing.md#noise-motion)); `flip` and `clock_wipe` transitions; transparent WebM and ProRes MOV, GIF, `every_nth_frame` and a `from_ms` to `to_ms` part in `export_video` ([formats](./video-editing.md#formats-and-parts)); `fit_clip_to_media` for `calculateMetadata`'s job ([fit to media](./video-editing.md#fit-to-media)). **Not taken:** React or any code in the document (invariant 3), cloud rendering (invariant 1), the rest of the presentations (bookFlip, cube, crosswarp, crossZoom, dreamyZoom, filmBurn, linearBlur, pushCut, ripple, swap, zoomBlur, zoomInOut: they need perspective, warps or light leaks we do not draw), `noise2D/3D/4D` as separate calls (`props` gives each property its own stream), light leaks, Lottie and Rive import |
| [FrameScript](https://github.com/frame-script/FrameScript) (README, `docs/media.md`, `docs/checking.md`) | TSX videos drawn by a native engine with no browser; a frame is a function of its number; agents install it from `AGENTS.md` and must render frames and look before saying done. `<LipSync>` swaps an open and a closed picture while the sound is louder than a threshold; `audioLevels(src)` is loudness every 10 ms. Its checking guide: the first frame, one a second, frames 3 to 5 apart across a move, then `ffprobe`, a tiled sheet and `silencedetect` on the export | **Took** the lip flap as a recipe over `audio_reactive` on a mouth node (video-direction playbook). **Already had** render-and-look (`video_strip`, `video_contact_sheet`, `pixel_verified`), a sheet of the whole video and silence detection. **Not built:** a probe of the exported file |
| [OpenFilm](https://www.scriptbyai.com/openfilm-ai-video-agent/) (an article of 2026-11-09; the repository was not read) | A film is a folder of `film.html` plus media; the agent and a local Studio edit the same files; `openfilm look` gives a contact sheet or one frame and reports missing fonts, missing audio, text outside the picture and frames that differ between repeated renders; exports MP4 up to 4K, transparent video, GIF, stills, subtitles, audio stems and timelines for Final Cut Pro, Premiere Pro and DaVinci Resolve | **Took** timeline and subtitle export: `export_timeline` writes a CMX 3600 EDL, SRT or WebVTT ([Video Editing](./video-editing.md#timeline-export)). **Already had** the self-check (`review_video`, lint, `video_strip`). **Not taken:** HTML scenes (invariant 3). **Built later the same day:** transparent video (WebM, ProRes MOV) and GIF ([Formats and parts](./video-editing.md#formats-and-parts)). **Not built:** audio stems, FCPXML, a render-twice consistency check |
| [ELLMPEG](https://arxiv.org/html/2602.00028v1) (arXiv 2602.00028, January 2026) | An edge agent that writes FFmpeg and VVenC commands: choose the tool, retrieve that tool's documentation from its own vector store, generate, self-critique, refine, extract and run the command. Retrieval helped small models most on the tool they know least (VVenC: about 15% base accuracy); more than three self-reflection rounds made the model over-correct valid output | **Agrees with** what we do: no free-form ffmpeg from an agent (typed operations; ffmpeg only runs inside our own tools), per-topic documents on demand (`get_playbook`), and at most three review rounds, now with the reason in the [review playbook](../../crates/agent/skill/review-playbook.md). **Not built:** embedding retrieval; the playbooks are small enough to read by topic |
| [Shotstack guide](https://shotstack.io/learn/how-to-build-an-ai-video-agent/) (2026-04, updated 2026-10) | The LLM picks a template and fills merge fields; a render API produces the file deterministically. Tool descriptions are prompts; an enum keeps template ids out of the model; a merge field whose name does not match is left unreplaced with no error; renders are polled with a cap; a failing payload must not be resubmitted blindly | **Agrees with** variables and `batch_export` (merge fields, but a row naming a variable that does not exist is an error here, not silence), validated operations, typed errors with a hint. **Took** "do not resend a failing call unchanged" into the review playbook. **Not taken:** a hosted render farm, destinations, webhooks |
| [Dataiku video processing](https://doc.dataiku.com/dss/14/unstructured-data/video/video-processing.html) (DSS 14 plugin, unsupported tier) | A "Watch Video" agent tool: up to 10 evenly spaced frames per video, resized to 512 px wide, sent with a question to a vision LLM | **Took** the shape as `watch_footage`: up to 10 pictures from the middle of equal slices, one labeled grid, the question repeated beside it; the agent is the vision model and nothing is sent anywhere |
| [invideo](https://invideo.io/) (marketing page for "Agent Two") | An editing agent that "watches every clip and logs it in memory, so you find any moment by describing it"; one direction applied to every shot; a real multitrack timeline the user can take over; storyboards and scripts; custom agent roles; your own agent (Claude, ChatGPT Codex, Grok, Gemini) connected to the editor | **Took** footage memory: `index_footage` (shots by scene score, an optional transcript from the user's own whisper, the agent's notes per shot) and `search_footage` (find a moment by words, answered with a ready `add_clip` object) ([Video Editing](./video-editing.md#footage-memory)). **Already had** bring-your-own-agent over MCP and one transaction over many nodes. **Not taken:** accounts, credits, multiplayer, hosted models |

`[INFERENCE]` No claim about the accuracy or speed of any of these products was measured here. invideo's page is marketing; what its agent does internally was not read.

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
