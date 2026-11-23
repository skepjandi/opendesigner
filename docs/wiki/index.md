<p align="center">
  <img src="https://raw.githubusercontent.com/skepjandi/opendesigner/main/assets/logo.svg" alt="OpenDesigner logo" height="96">
</p>

# OpenDesigner Wiki

> Project knowledge base for contributors and AI agents. All pages are automatically synced to the [GitHub Wiki](https://github.com/skepjandi/opendesigner/wiki) on every merge to `main`.

**Project status: design.** No application code exists yet. Every page describes the intended design and carries a `Status: Design` banner until the code lands; claims not yet checked are labelled `[INFERENCE]`.

OpenDesigner is a native desktop editor: a Rust core under a Qt 6 Quick (QML) shell. It is a vector design canvas plus a video timeline for producing App Store, Google Play and web assets with an AI agent. The GUI is how you guide the agent: your selection, pinned notes and hand edits reach it, and every agent edit is outlined and reversible. Installing the app installs an Agent Skill and a local MCP endpoint for the agents on your machine. No account, no analytics, no server of ours.

## Pages

| Page | Description |
|------|-------------|
| [Architecture](./architecture.md) | Crates, trust zones, data flow, the "core is UI-agnostic" rule |
| [Document Format](./document-format.md) | Project folder, node model, operations, history and checkpoints |
| [Agent Integration](./agent-integration.md) | The skill installer, the local MCP server, selection and annotation context, in-app chat |
| [Qt Desktop](./qt-desktop.md) | Why Qt, the QML / `Editor` object / `EditorState` layers, Qt licensing (LGPL-only rule), window layout, canvas rendering, theme and brand tokens, platform requirements |
| [Store Assets](./store-assets.md) | What GitNotēs and OpenMedia do today, store specs, the asset studio that replaces it |
| [Video Editing](./video-editing.md) | Timeline model, preview, FFmpeg, audio, what is built and what is not |
| [Scene Spec](./scene-spec.md) | The SceneSpec an agent writes before it builds, the locked build passes, the review ledger and the correction limits |
| [Security](./security.md) | Assets, trust boundaries, the MCP threat model, skill installer rules |
| [Prior Art](./prior-art.md) | Penpot, Figma, HyperFrames: what they do, what we copy, where we differ |
| [Roadmap](./roadmap.md) | Phases with exit criteria, Phase 0 spikes, performance budgets, decision log |
| [Development Guide](./development-guide.md) | Toolchain, commands, repository layout, conventions |

## AI Agent Note

If you are an AI agent modifying this repository, read [Architecture](./architecture.md) and [Security](./security.md) before making significant changes, and find the current phase in the [Roadmap](./roadmap.md). See `AGENTS.md` for the wiki-first rule and the architecture invariants.

## Documentation Philosophy

- **For contributors:** `docs/wiki/` is the source of truth; `CHANGELOG.md` records individual changes.
- **For AI agents:** names, paths, and schema fields are precise. Intent is labelled as intent.
- **Wiki sync:** edit `docs/wiki/*.md` and open a PR. `.github/workflows/sync-wiki.yml` mirrors changes to the GitHub Wiki.

## See Also

- [GitHub Wiki](https://github.com/skepjandi/opendesigner/wiki)
- [CHANGELOG.md](../../CHANGELOG.md)
- [AGENTS.md](../../AGENTS.md)
