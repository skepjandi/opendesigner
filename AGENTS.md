# OpenDesigner — Agent Rules

> Rules for AI coding agents working in this repository.

OpenDesigner is a native desktop editor (macOS, Windows, Linux): a Rust core under a Qt 6 Quick (QML) shell. It is a vector design canvas plus a video timeline for producing App Store, Google Play and web assets. Its defining feature is that the **GUI is the guidance channel for AI agents**: the user's selection, annotations and hand edits reach the agent, and every agent edit is visible and reversible. Installing the app installs an Agent Skill and a local MCP endpoint for the agents on the user's machine. **No server of ours is needed to use the app.**

## Read the Wiki First

**Before touching any code, read the relevant wiki page(s).** The wiki (`docs/wiki/`) is the canonical reference for architecture, the document format, the agent bridge, and the security model.

- Starting anything? → read `docs/wiki/architecture.md` and `docs/wiki/roadmap.md` (which phase are we in?)
- Touching the document model, operations, history, project files? → read `docs/wiki/document-format.md`
- Touching the MCP server, the skill installer, selection/annotation context, in-app chat? → read `docs/wiki/agent-integration.md`
- Touching windows, panels, QML, theme, the Qt/Rust bridge, canvas rendering, Qt licensing? → read `docs/wiki/qt-desktop.md`
- Touching exports, store presets, device frames, panoramas? → read `docs/wiki/store-assets.md`
- Touching the timeline, decoding, FFmpeg, video export? → read `docs/wiki/video-editing.md`
- Touching anything an agent, a file, or the network can reach? → read `docs/wiki/security.md`
- Wondering why a decision was made or what competitors do? → `docs/wiki/roadmap.md` (decision log) and `docs/wiki/prior-art.md`

If the wiki is wrong or missing, fix it first — then make your code change. See the wiki accuracy rule below.

## Worktrees (ALWAYS)

**All agent work — fixes, features, refactors, even small edits — happens inside a git worktree. Never edit files in the main worktree's working tree directly.**

Multiple agent sessions may run concurrently on different branches. The main worktree is shared; editing it directly causes races where one agent's uncommitted changes block or corrupt another's work.

```bash
# From the repo root (the main worktree):
git fetch origin
git worktree add -b <type>/<scope>-<slug> .worktrees/<scope>-<slug> origin/main
# e.g.  git worktree add -b feat/core-ops .worktrees/core-ops origin/main

# Always under .worktrees/ at the repo root (gitignored) — never /tmp or ~.
```

- **Main must stay clean.** No uncommitted changes, no staged files. Remove the worktree as soon as its branch is merged (`git worktree remove <path>`), then confirm `git status` in main is empty.
- **One worktree per branch.** Always branch from `origin/main`, never from another worktree's branch.
- **Build output is per-worktree and large.** Rust `target/` directories live in the worktree (the Qt shell's first build compiles `cxx-qt` and takes minutes); set `CARGO_TARGET_DIR` to a shared directory outside the repo if disk is tight. Never commit it.
- **Coordinate before touching shared files** (`Cargo.toml` workspace manifest, `crates/core` operation and file-format types, `skills/`). Check `git worktree list` and the other worktrees' `git status` first.

## Testing

All checks must pass before pushing:

```bash
# Everything that needs no Qt (core, render, store, agent, cli): the default workspace members
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check

# The Qt shell (`app` crate). Needs Qt 6 and QMAKE, e.g. QMAKE=$(brew --prefix qt)/bin/qmake
cargo clippy -p opendesigner-app --all-targets -- -D warnings
cargo test -p opendesigner-app   # unit tests, plus the offscreen scripted session of the real window
```

- **Never push failing checks.** The commands above are the contract; CI runs the same.
- New module → tests beside the code (`#[cfg(test)]` in Rust); integration tests in each crate's `tests/`. The shell's logic lives in `app/src/state.rs` (no Qt) and is tested there; `qml/SelfTest.qml` drives the real window offscreen (`crates/app/tests/selftest.rs`). QML holds presentation only.
- **The renderer is tested against golden images** compared programmatically (per-pixel tolerance), and the file format against round-trip fixtures. Store export presets are tested against the size, format and alpha rules they encode.
- Mock the network. Agent and MCP tests use recorded request/response fixtures; never call a real model provider in tests and never put a real key or token in a fixture.
- Do not use screenshots or LLM visual inspection as test evidence. Verify behavior programmatically. Manual on-device checks (each OS, each agent) are the user's; state clearly in the PR what remains unverified.

## Self-Documenting Code

- Clear names; no comments explaining obvious behavior.
- Rust: no `unwrap()`/`expect()` outside tests; propagate typed errors (`thiserror`). `clippy -D warnings` is zero-tolerance.
- No `unsafe` without a `// SAFETY:` comment naming the invariant. The `app` crate is the only one that may contain `unsafe` (the `cxx-qt` macros expand to FFI code); every other crate forbids it.
- QML is declarative presentation: bindings, layout, animation, input forwarding. No document logic, no business rules, no parsing beyond reading the JSON the editor object publishes; if you need it, the Rust core is missing an operation. Name properties and invokables so they cannot collide with Qt's own (for example `setProperty`, or the `setX` setter of a property `x`).
- **UI icons are vector icons from the vendored set, never emoji or Unicode symbols.** Every icon in the QML (buttons, tree disclosure, visibility eye, status marks) is an SVG from `crates/app/qml/icons/` drawn through `Icon.qml` (or `Btn`'s `iconName`), so it follows the theme and renders the same on every OS. A new icon comes from the same licensed set (Lucide, ISC) with the file copied unchanged and the licence kept (`qml/icons/LICENSE`); another set needs a decision-log entry. Icon-only buttons carry a tooltip and an accessible name (`Btn.tip`). Typographic punctuation in text (`·`, `×`, `…`, `—`) and key names (`⌘`, `⇧`) are text, not icons.
- Keep functions short and focused. Use the existing patterns in `crates/` and `app/` as reference.
- **Never keep deprecated code.** When Qt, a crate or an API deprecates something, migrate in the same change. `cxx-qt` is pre-1.0 and Qt deprecates across minors: pin versions exactly and bump them deliberately, in their own PR.
- **Zero tolerance for lint-blocking issues.** Unused imports/vars/functions, empty blocks, needless clones: fix them on every change. Prefix intentionally unused parameters with `_` (Rust) or leave them unnamed (C++).

## Dependencies

**Use only popular, well-maintained packages and libraries.** Prefer the standard library, Qt and what the repository already depends on over a new dependency. A new crate, C++ library, QML module or build tool must meet all of these, and the PR states the evidence:

- **Popular.** Widely used in its ecosystem (high download count, many dependents, an established upstream such as the Rust project, Qt, KDAB, Mozilla, Microsoft, or a foundation). A package nobody else uses is a package nobody else audits.
- **Actively maintained.** A release or substantive commit within the last 12 months, issues and security reports answered, more than one maintainer or an organization behind it. Archived, abandoned, or single-maintainer-and-quiet projects are out.
- **License-compatible.** Permissive (MIT, Apache-2.0, BSD, ISC, Zlib), MPL-2.0, or LGPL linked dynamically. No GPL/AGPL-only code (see invariant 12). `cargo deny` enforces the allowlist.
- **Worth its weight.** Small purpose-built crates for a few lines of code, and transitive trees that dwarf the feature, are written in-house instead.
- **Pinned and audited.** Lockfiles committed; exact versions for pre-1.0 and FFI-heavy dependencies; `cargo deny` (advisories, licenses, sources) and Dependabot on.

An exception (a pre-1.0 or niche dependency with no better option, such as the Rust–Qt bridge) needs a decision-log entry in `docs/wiki/roadmap.md` naming the alternatives considered, the maintenance evidence, and the exit plan if it is abandoned. Never add a dependency to work around a missing core operation or a bug you could fix here.

## Architecture Invariants

Violating any of these is a bug regardless of whether tests pass. Details in `docs/wiki/security.md` and `docs/wiki/architecture.md`.

1. **Local-first; no backend; no telemetry.** No analytics, no crash reporting, no identifiers that leave the device. The only network destinations are (a) the model endpoint or agent the user configured, (b) the sites the user chose to fetch an asset from, (c) the update channel, if the user enabled it. Adding any other destination needs a `security.md` update and explicit justification in the PR.
2. **The core is UI-agnostic.** All document logic (model, operations, history, file format, rendering to pixels, export, store presets) lives in Rust crates with no Qt dependency. The Qt app (the `app` crate and its QML), the MCP server and the headless CLI are three front ends over the same operations. The `app` crate and QML hold presentation and input only; document logic in either is a bug. If a feature works only through the GUI, the core is missing an operation.
3. **Agent output is untrusted input.** Agents change a document only through typed operations that `core` validates (schema, bounds, ids that exist, size limits) before applying. Documents are data: no embedded scripts, no code execution from a project file or from an agent. If motion or templating ever needs code, it is a separate, sandboxed, user-approved mechanism with its own wiki page.
4. **Every agent edit is attributed, checkpointed and reversible.** Operations carry an actor (user or a named agent); each agent turn writes a checkpoint; the UI outlines what an agent changed; undo and restore always work. No operation may bypass the history.
5. **The MCP endpoint is local and authenticated.** It binds `127.0.0.1` only, requires a per-install bearer token, and checks `Host` and `Origin` (a web page must not be able to drive the editor through the user's browser). It is off until the user turns it on or accepts the skill installer's offer. Tools write only inside the project directory (`media/`, `exports/` and the document) and, as plain text notes with strict name, symlink and size rules, in `<repo>/.opendesigner/memory/` (`docs/wiki/security.md#agent-memory`); the one read outside it is the user-enabled, read-only picture and video search and copy described in `docs/wiki/security.md#device-access`, which has an off switch.
6. **Installing the skill is explicit, previewed and reversible.** The installer shows every path it will write, writes only the skill's own directory (`opendesigner/`) under each agent's skills location, never overwrites a skill it did not create without consent, records the version it wrote, and has an uninstall that removes exactly those files. It never edits an agent's config, credentials or history.
7. **We never hold the user's agent credentials.** In-app chat drives an agent the user has already signed in to (ACP/MCP). A direct provider mode, if built, stores keys only in the OS secure store, reads them only in Rust, and never logs them or writes them to projects, exports or fixtures.
8. **Projects are open files.** Plain, diffable, documented formats in a folder; every asset exports to open formats (SVG, PNG, JPEG, MP4, …). Nothing locks a project into the app.
9. **Export presets are correct or they are flagged.** Each store preset records its source URL and the date it was last checked; an export is validated against the rule it claims to meet (size, format, no alpha, text share, …) and fails loudly instead of producing a file a store will reject.
10. **Media tooling is pinned and single-sourced.** Anything FFmpeg that the app links or ships is one LGPL build, shared-linked, from a pinned manifest with checksums, matching the major version Qt Multimedia uses; the app never links or ships a GPL build. Until in-process decoding exists, video is encoded by running an `ffmpeg` program (the user's, or the pinned bundled one) as a separate process, fed raw frames over a pipe; see the decision log. Imported media is never uploaded anywhere by the app.
11. **Performance is a feature with a budget.** The budgets in `docs/wiki/roadmap.md` (canvas pan/zoom frame time, cold start, scrub latency) are checked by benchmarks. A change that blows a budget needs a decision-log entry, not a quiet regression.
12. **Qt stays LGPL.** Use only Qt modules offered under LGPLv3. The GPL-only modules listed in `docs/wiki/qt-desktop.md#licensing` (for example Qt Canvas Painter, Qt Quick 3D, Qt Graphs, Qt Quick Timeline, Qt Qml Compiler) are banned, as is any GPL-only third-party library (for example KDDockWidgets). Qt is linked dynamically and shipped with its license texts and a relinking path. The linked-module list in `crates/app/build.rs` is the allowlist, and `scripts/package-macos.sh` refuses a bundle containing a banned module; adding a module needs a licensing check in the PR.

## Second-Order Thinking Before Every Change

**Never fix something without understanding its ripples.**

- **What does this code talk to?** Find callers and consumers. An operation type is consumed by history, the GUI, the MCP tool schema, the skill text, the file format and the golden tests.
- **What does this code touch?** The operation log, the render pipeline, the MCP server and the skill installer are high-coupling areas. Trace the full call chain first.
- **Symptom or cause?** If an agent keeps needing a workaround, the tool surface is probably missing a primitive. Fix the surface.
- **Does it work on every platform?** Qt renders through Metal, Direct3D and Vulkan (or OpenGL) and uses Cocoa, Win32 and X11/Wayland; text, fonts, file dialogs, IME and codecs differ. State which platforms you ran.
- **Is it reversible?** File-format changes need a version bump and a migration; users' saved projects must keep loading.

If uncertain about impact — **stop and ask**.

## Git Discipline

- **Never commit or push directly to `main`.** Every change goes through a branch and a pull request.
- Atomic commits, imperative mood, conventional-commit prefixes (`feat(core):`, `fix(mcp):`, `docs(wiki):`). Scopes: `core`, `render`, `ui`, `app`, `canvas`, `timeline`, `media`, `mcp`, `skill`, `store`, `cli`, `build`, `repo`.
- No `target/`, `.DS_Store`, `.env`, build artifacts, signing material or captured personal media.
- Branch per feature, rebase before merging.

## Wiki documentation

Two documentation surfaces:

- **`CHANGELOG.md`** (repo root) — single-PR fixes and narrow entries, grouped by date descending: title, area (conventional-commit prefix), 2–4 line summary, PR reference. Default location for any new fix or feature note.
- **GitHub Wiki** ([skepjandi/opendesigner/wiki](https://github.com/skepjandi/opendesigner/wiki)) — architecture, formats, security model, agent integration, roadmap. A new page is appropriate when a change crosses architectural boundaries or teaches contributors something about the system.

Editing the wiki: write the page in `docs/wiki/<name>.md` (source-controlled), add it to `docs/wiki/index.md`, open a PR. The sync workflow (`.github/workflows/sync-wiki.yml`) mirrors `docs/wiki/` → GitHub Wiki on every merge to `main`. Manual edits on the GitHub Wiki are overwritten on the next sync.

**Status banners.** While code does not exist, wiki pages describe the *intended* design and carry a `Status: Design` banner. When the code lands, the PR that lands it flips the banner to `Status: Implemented`, replaces intent with fact (real crate names, real signatures), and records deviations in the roadmap decision log. Claims that were not verified are labelled `[INFERENCE]`.

**Wiki accuracy is a living concern.** If a page is wrong — wrong crate, signature, schema field, or behavior — fix it as part of your current work. This applies to every PR.

Architecture-level changes (new crate, new operation family, new MCP tool, new agent target, new platform path) are not done until the wiki is updated.

## Data Safety

- **Never commit secrets:** API keys, `.env`, MCP tokens, signing certificates and notarization credentials, updater private keys.
- `.env` is gitignored; use `.env.example` for templates.
- Fixtures and sample projects must contain only media the project may redistribute; scrub personal content (faces, names, handles, messages) from anything captured.
- Check `git diff` before committing.
- **The repository is public (open source).** Everything committed, every PR body, issue, changelog entry and doc is world-readable and permanent in history. Never include passwords, tokens, keys, certificates, store-console credentials, or personal data (emails, home-directory usernames, private company names). Reference secrets by variable name only. Write docs and scripts so a stranger can follow them: no machine-specific paths except documented defaults.
