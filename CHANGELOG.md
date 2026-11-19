# Changelog

All notable changes to OpenDesigner are documented here.

> **Format**: loosely based on [Keep a Changelog](https://keepachangelog.com/), grouped by date descending. Each entry references the originating PR (when available) and names the area from the conventional-commit prefix.
>
> **Wiki**: the [GitHub Wiki](https://github.com/skepjandi/opendesigner/wiki) is the public-facing main wiki — architecture, document format, agent integration, security model, roadmap. Source-controlled editing surface is `docs/wiki/` in this repo; CI (`.github/workflows/sync-wiki.yml`) mirrors it to the GitHub Wiki on every merge to `main`. New single-PR changes belong here, not as new wiki pages.

## 2026-11-19

### feat(mcp): local MCP server, tools and token for coding agents

**Area:** `feat(mcp)`, `feat(cli)`

**Problem:** The guidance model had no way to reach an agent. Without a tool surface an agent could only use the CLI.

**Change:** New `agent` crate. `mcp.rs` speaks MCP (JSON-RPC 2.0, Streamable HTTP, protocol versions 2025-06-18, 2025-03-26, 2024-11-05) with sessions and per-agent attribution; `tools.rs` exposes 17 tools (`get_guidance`, `apply_ops` with a checkpoint before each turn, `render_preview` returning an image, store-validated `export`, checkpoints and restore, and more); `server.rs` is the loopback transport with token, `Host` and `Origin` checks and size limits; `token.rs` creates the 0600 token file. `opendesigner serve-mcp <project>` runs it and prints the Claude Code connect command. Dependencies: axum, tokio, base64, getrandom (evidence in the roadmap decision log).

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (63 tests, including socket-level security tests and a test that starts the real binary, connects as an agent and checks the edit landed on disk with the agent's name in the history), `cargo deny check`, on macOS arm64. **Not verified:** Claude Code or Codex actually connecting (run by hand); Windows and Linux; token file permissions on Windows; behavior under concurrent agents beyond the mutex that serializes calls.

**PR:** n/a

### feat(core): selection, annotations and the guidance context

**Area:** `feat(core)`, `feat(cli)`

**Problem:** The point of the editor is that the user guides an agent by pointing, but the core had no model for what the user points at.

**Change:** `core` gains `Annotations` (pins on a node or a page region, open/resolved, attributed, persisted in `annotations.json`) and `Workspace` (session + annotations + selection + project folder) with `guidance()`: pages, selected nodes, open notes with an orphan flag, and the user's edits since the agent's last turn. The CLI gains `annotate`, `annotations`, `resolve` and `guidance`.

**Verified:** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test --workspace` (43 tests, including a CLI test of pin → read → act → resolve). **Not verified:** Windows and Linux; selection is in memory only (a front end holds it); the MCP and Qt front ends that will use it are not built yet.

**PR:** n/a

### feat(core): headless core, renderer, store presets and CLI

**Area:** `feat(core)`, `feat(render)`, `feat(store)`, `feat(cli)`

**Problem:** The project had rules and a design but no code, and the central claim (agents edit through validated, reversible operations) was unproven.

**Change:** A Cargo workspace with four crates. `core`: node model (frames, groups, rectangles, ellipses, text, image), the operations `create`/`set`/`move`/`reparent`/`delete` with bounds validation and inverses, an append-only history with atomic transactions, undo/redo, checkpoints and `restore`, and project folders (`od.toml`, `pages/*.json`, history log) that save deterministically and reject hostile input. `render`: CPU rasterization of frames, groups, rectangles and ellipses (fills, strokes, rounded corners, clipping, opacity) and PNG export as RGB or RGBA. `store`: nine App Store and Google Play presets with size, format, alpha and count validation. `cli`: the `opendesigner` binary (`new`, `show`, `apply`, `checkpoint`, `checkpoints`, `restore`, `export`, `validate`, `presets`). Also `deny.toml`, `clippy.toml` (no `unwrap`/`expect`/`panic` outside tests), `rust-toolchain.toml`, a Rust CI workflow, and wiki updates marking what is implemented. Dependencies (all popular and maintained, licenses checked by `cargo deny`): serde, serde_json, thiserror, toml, clap, tiny-skia, png, tempfile (tests).

**Verified:** `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test --workspace` (36 tests, including an end-to-end CLI test that applies an agent edit, exports a 1320x2868 App Store PNG, validates it, rejects a wrong preset without writing a file, and restores a checkpoint), `cargo deny check`, all on macOS arm64. **Not verified:** Windows and Linux; the CI workflow (never run); text and image rendering (not implemented, reported as warnings); the store preset rules against Apple's and Google's own pages (copied from a reference project); performance against the roadmap budgets (no benchmarks yet).

**PR:** n/a

### docs(repo): project rules, wiki and design for the Qt + Rust editor

**Area:** `docs(repo)`

**Problem:** GitNotēs and OpenMedia produce their App Store, Google Play and web assets by describing every detail to an agent in a terminal and re-reading the output. There is no surface to select, annotate or adjust, so the agent is guided by text alone.

**Change:** Repository bootstrap with no application code. `AGENTS.md` (wiki-first rule, worktrees, a dependency policy limiting the project to popular, well-maintained packages, and 12 architecture invariants for a local-first editor whose agents work through validated, checkpointed operations, including that Qt stays LGPL), this changelog, the wiki under `docs/wiki/` (architecture, document format, agent integration, Qt desktop, store assets, video editing, security, prior art, roadmap, development guide) with its sync workflow, `README.md`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, issue and PR templates, `.editorconfig`, `.clang-format`, and a Rust/Qt/CMake `.gitignore`. `assets/logo.svg` is the app logo. Toolkit: Qt 6 (C++/QML) over a Rust core, chosen over GPUI; see the roadmap decision log.

**Verified:** nothing run; documentation only. Research inputs: the two reference repositories' own docs and store-asset scripts, Kino's public docs, OpenCut's repository and DeepWiki pages, Qt 6.12's licensing and Qt Multimedia docs, `cxx-qt`'s repository and crates.io page, KDDockWidgets' license, the Agent Skills specification. **Not verified:** canvas and video-preview performance on Qt, the `cxx-qt` build on all three OSes, the per-agent skill directories other than the ones present on the maintainer's machine, whether the QML build flow touches the GPL-only Qt Qml Compiler module, and every `[INFERENCE]` on the wiki pages; these are the Phase 0 spikes in the [roadmap](docs/wiki/roadmap.md).

**PR:** n/a
