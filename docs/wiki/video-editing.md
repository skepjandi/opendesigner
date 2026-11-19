# Video Editing

> **Status: Design.** OpenCut and Kino facts come from their public repository and docs as read on 2026-11-19; OpenCut's rewrite is in progress, so its desktop app is a scaffold, not a reference implementation to port. The timeline design is intent.

## What videos are for here

App Store and Google Play app previews, promo and changelog clips, web hero loops, short social cuts of a feature. Short, mostly screen captures and motion graphics, rarely a multi-camera edit. That shapes scope: a precise, scriptable timeline with strong title/overlay tooling beats a full colorist.

## What OpenCut teaches

[OpenCut](https://github.com/opencut-app/opencut) ("a free and open source video editor for web, desktop, and mobile", MIT; the original TypeScript app is archived as `opencut-classic`) is mid-rewrite around ideas we share:

| OpenCut | Us |
|---------|----|
| Business logic moves into a Rust core; apps are UI shells ("the UI framework for any given app is a replaceable detail") | Same rule: [Architecture](./architecture.md#the-central-rule-the-core-is-ui-agnostic) |
| Desktop app in GPUI: `Shell` with `Browser`, `Preview`, `Inspector`, `Timeline` panels (layout 2/3 top row of 25%/50%/25%, 1/3 timeline); semantic OKLCH theme tokens | Same panel set in a Qt shell; we add the agent panel ([Qt Desktop](./qt-desktop.md#window-layout)). OpenCut's GPUI choice is the one thing we do not follow |
| `crates/media`: FFmpeg **8.1.3** pinned in `setup/ffmpeg.json` with SHA-256s and prebuilt targets (Windows/Linux x86_64 and aarch64); a Bun setup script; **shared LGPL only**, `--disable-programs --disable-network --disable-avfilter …`; CI builds prebuilts for six targets and publishes a pre-release | Same *policy*: pinned manifest, checksum-verified, LGPL shared, no network protocols. The *version* is not ours to copy: it must match the FFmpeg major that Qt Multimedia uses ([below](#qt-multimedia-and-ffmpeg)). Write the setup script in Rust (`cargo xtask`) rather than add Bun to the toolchain |
| Plugin-first architecture, an Editor API, MCP server for agents, headless mode, a scripting tab | The operation API plus MCP plus headless CLI are our v1; plugins and scripting are deferred ([Roadmap](./roadmap.md)) |
| moon + proto for monorepo and tool pinning | Plain Cargo workspace first; revisit if a non-Rust toolchain (web docs, store tooling) grows |

Caveats: the OpenCut desktop app is a scaffold (its README says it opens a maximized window with the root `Shell`; the four panels are minimal), and the project says it is not yet taking outside contributions. Treat it as a design reference and a license-compatible source (MIT) for small utilities, not as a dependency.

## What Kino teaches

[Kino](https://kino.ai) is the benchmark for agent-driven video, and its docs show the pattern we want:

- **The agent works on the same timeline the user edits.** "Ask for the cut. The agent reads your footage, makes the edit, and leaves every clip on the timeline for you to move." Clips the agent changed get a **blue outline**. Every agent edit saves a **checkpoint** you can go back to. We copy this exactly ([Document Format: history](./document-format.md#history-and-checkpoints)).
- **Projects are code-like and legible to an agent.** Kino is built on Remotion, so titles and lower thirds are code in your brand, and "Claude Code reads one like a repo". We keep the *legibility* (plain-text project files, tokens for brand) but not the *code*: documents are data ([invariant 3](../../AGENTS.md)).
- **MCP is the integration point,** one local endpoint for many agents, with skills for editing style, and a way to use a Claude or ChatGPT subscription from inside the app ([Agent Integration](./agent-integration.md)).
- **Review and chat as first-class.** Notes on the timeline, references pasted into chat, live progress cards for long jobs. We take the in-app annotation model for video: a note pinned to a time range and a canvas region.

Where we deliberately differ: local-only and open files (Kino stores footage in its cloud and has accounts, plans and credits); a design canvas in the same app, so a store set can contain video and stills; store-aware validation.

## Timeline model (intended)

- A **composition** page has an ordered list of tracks (video, audio, overlay, text), each holding clips with `start`, `duration`, `source in/out`, transform, effects and keyframes. Clips are nodes with stable ids, so the operation set is the same family as design operations plus `trim`, `split`, `retime`, `ripple`.
- A composition is also a **frame**: a video page can embed design frames as overlay clips (a device frame around a screen recording, a headline card), rendered by `render`.
- Time is an integer count of ticks per second (rational, never float seconds) so edits do not drift.
- Preview decodes through `media` (or Qt Multimedia for plain playback, see below) to frames shown in the preview panel at the project frame rate; scrubbing prioritizes latency (nearest keyframe plus decode forward) over exactness, with an exact frame on pause.
- Export encodes H.264/HEVC/ProRes where the platform allows, plus VP9/AV1 where the LGPL build includes them; presets for App Store app previews and Play promo video sit beside the image presets ([Store Assets](./store-assets.md)), with their own validation (resolution, frame rate, duration, size limits) `[INFERENCE: rules to be collected from Apple and Google docs in Phase 5]`.

## Qt Multimedia and FFmpeg

Read from Qt 6.12's Qt Multimedia docs: it is LGPLv3 (or GPLv2) and its **default backend is built on FFmpeg** on every platform except WebAssembly and embedded Linux. The binary packages link FFmpeg (9.0.1 in the 6.12 docs) *dynamically*; an application either bundles those libraries or depends on the system's, and may deploy **its own FFmpeg build provided the major version matches the one Qt uses**. Qt's prebuilt FFmpeg includes only features under permissive licenses. Native backends (AVFoundation on macOS; Windows Media Foundation, deprecated since Qt 6.10) are kept with limited support, and new features land only in the FFmpeg backend. Qt's docs warn that codec and hardware-acceleration behavior differs per platform, and that H.264 and similar codecs may carry patent royalties the Qt license does not cover (a distribution decision for the Phase 7 release work).

What follows for us:

- **One FFmpeg in the process** ([invariant 10](../../AGENTS.md)). Our `media` crate and Qt's backend must share the same major version; if we ship our own FFmpeg build, Qt loads it too. OpenCut's pin (8.1.3) is a different major than Qt's, so it is not directly reusable.
- **Use Qt Multimedia for what it is good at** (plain playback, audio output, `QVideoSink` access to decoded frames, `QVideoFrameInput` to feed frames in) and our `media` crate for what an editor needs (frame-accurate decode, export, thumbnails, waveforms). Where they overlap, pick one in the Phase 0 video spike and delete the other path.
- **Do not depend on the deprecated native backends.**

## Open questions

- Hardware decode paths per platform (VideoToolbox, D3D11/MediaFoundation, VA-API/Vulkan video) versus a software-only first cut.
- Can a video preview item take a platform video surface directly, or does every frame go through a texture upload (`QVideoSink` frames versus our own decode)? Phase 0 spike on macOS first.
- Audio: waveform, mixing, and whether speech-to-text for cutting by transcript is in scope (Kino-style) or left to the agent via a tool.
