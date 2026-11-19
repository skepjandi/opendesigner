<p align="center">
  <img src="assets/logo.svg" alt="OpenDesigner logo" height="220">
</p>

<h1 align="center">OpenDesigner</h1>

<p align="center">
  Design and video assets for your apps, guided by pointing, not by typing.<br>
  A native design and video editor your coding agents can drive.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MPL--2.0-blue.svg" alt="MPL-2.0"></a>
  <img src="https://img.shields.io/badge/UI-Qt%206-41cd52.svg" alt="Qt 6">
  <img src="https://img.shields.io/badge/core-Rust-f2ca83.svg" alt="Rust core">
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey.svg" alt="macOS | Windows | Linux">
  <img src="https://img.shields.io/badge/status-design-lightgrey.svg" alt="Status: design">
</p>

OpenDesigner is an open-source desktop editor: a Rust core under a Qt 6 Quick (QML) shell. It is a vector design canvas in the spirit of Penpot and Figma plus a video timeline in the spirit of [Kino](https://kino.ai) and [OpenCut](https://github.com/opencut-app/opencut), made for one job: producing App Store, Google Play and web assets with an AI agent while you stay in control.

Today that work happens in a terminal: you describe every pixel in text, the agent writes HTML and a render script, and you read the result and describe the next fix. OpenDesigner puts a GUI in that loop. You select, pin a note on the exact spot, nudge the result by hand, and the agent sees all of it.

## Why

- **Guide with your hands.** Selection, annotations and manual edits are context the agent receives, so "make this bigger" means *this*.
- **Every agent edit is visible and reversible.** Changed layers are outlined, every edit is a checkpoint you can go back to.
- **Store-aware.** Export presets encode the App Store, Google Play and web size and format rules, and validate the files before you upload them.
- **Bring your own agent.** Installing the app installs a skill and a local MCP endpoint for the agents on your machine: Claude Code, Codex and any other that reads Agent Skills or speaks MCP. No account of ours.
- **Files, not a database.** Projects are plain, diffable files in a folder you can put in Git.
- **Native and fast.** A Rust core and a Qt Quick UI, no webview and no headless browser in the loop.

## Planned highlights

- Vector design canvas: frames, shapes, text, paths, auto layout, components, device frames
- Store asset studio: panoramic screenshot sets sliced per store slot, localized variants, feature graphics, spec validation
- Video timeline: tracks, trimming, text and motion, FFmpeg export
- Agent bridge: Agent Skill installer for every detected agent, local MCP server, optional in-app chat over the Agent Client Protocol
- Headless mode: the same operations from the command line for CI and batch renders
- No telemetry

## Status

**Early development.** The editor runs, and it is built for an agent to do the making: the agent creates the frames, shapes, text, images and videos (and picks their names and sizes) through MCP, and you point and correct, selecting and moving what it made, editing properties, pinning notes, undoing, and exporting a frame to a PNG that is checked against App Store and Google Play rules. Coding agents connect over MCP (omp, Claude Code, Codex, Gemini CLI, opencode, Cursor and any MCP client; the Connect tab shows the exact setup for each) and get your selection, your notes and your corrections; what they change is outlined and can be restored. The app installs an Agent Skill for the agents it finds. Device frames, store sets and panoramas, translations, design tokens and an in-app chat with your agent exist too. **Not there yet:** video; a real agent turn has been run only with omp (opencode connects; Claude Code connects; Codex and Gemini CLI were not signed in), and only macOS has been run (`scripts/package-macos.sh` builds an app bundle there). See the [roadmap](docs/wiki/roadmap.md); each wiki page says what is built and what is design.

## Try it

```bash
# the editor (needs Qt 6; see the development guide)
export QMAKE="$(brew --prefix qt)/bin/qmake"
cargo run -p opendesigner-app

# the headless side, no Qt needed
cargo run -p opendesigner-cli -- new demo.od --name Demo
cargo run -p opendesigner-cli -- serve-mcp demo.od
cargo run -p opendesigner-cli -- skill status
```

## Stack

Rust (edition 2024) core · Qt 6 Quick (QML) shell built with Cargo and `cxx-qt` (Qt used under LGPLv3, dynamically linked) · FFmpeg (LGPL, shared, pinned; not used yet) · Model Context Protocol · Agent Skills. Dependencies are limited to popular, well-maintained packages ([policy](AGENTS.md#dependencies)).

## Build and contribute

Development commands and prerequisites are in the [development guide](docs/wiki/development-guide.md). Read [AGENTS.md](AGENTS.md), [CONTRIBUTING.md](CONTRIBUTING.md) and the [wiki](docs/wiki/index.md) before changing code; the rules apply to humans and AI agents alike.

For bugs or feature requests, [open an issue](https://github.com/skepjandi/opendesigner/issues/new/choose).

## License

[MPL-2.0](LICENSE). Qt is used under the LGPLv3; see [Qt Desktop: Licensing](docs/wiki/qt-desktop.md#licensing).
