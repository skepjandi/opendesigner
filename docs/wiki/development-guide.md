# Development Guide

> **Status: Design.** There is no Cargo workspace or CMake project yet; the commands below are the contract the first implementation PR must satisfy. Prerequisites for the Rust–Qt bridge come from `cxx-qt`'s getting-started page (CMake 3.24+, a C++ compiler, Rust, Qt 6) and Qt 6.12's documentation.

## Prerequisites

| Tool | Needed for |
|------|------------|
| Qt 6 (exact version pinned in Phase 0; the docs read were 6.12) with Qt Quick, Quick Controls, Multimedia, and the Qt Test / Quick Test modules | The app shell. Only allowlisted LGPL modules ([Qt Desktop: Licensing](./qt-desktop.md#licensing)) |
| CMake ≥ 3.24, Ninja | Build (`cxx-qt` needs 3.24+) |
| A C++20 compiler: Apple Clang (Xcode command line tools), MSVC, GCC or Clang | The shell |
| Rust stable ≥ 1.85 (edition 2024) with `rustfmt`, `clippy` | Everything in `crates/` (`cxx-qt`'s MSRV is 1.85) |
| `clang-format`, `clang-tidy` | C++ style and lint |
| `cargo-deny` | Licenses, advisories, sources |
| FFmpeg shared libraries matching the major Qt Multimedia uses, fetched and checksum-verified by `cargo xtask media-setup` | Phase 5 onward ([Video Editing](./video-editing.md#qt-multimedia-and-ffmpeg)) |
| Linux: the Qt platform plugin dependencies (X11/Wayland, fontconfig, Vulkan or OpenGL drivers) | Running and packaging on Linux |

## Commands (intended)

```bash
# configure and build the shell and the Rust crates it links
cmake --preset dev
cmake --build --preset dev
./build/dev/app/opendesigner         # launch the editor (path set by the preset)

# Rust crates on their own (no Qt needed except for `bridge`)
cargo check --workspace
cargo run -p cli -- export <project> --preset <id>

# checks (the contract in AGENTS.md)
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace               # unit, integration, golden-image tests
ctest --preset dev --output-on-failure   # Qt Test and Qt Quick Test
clang-format --dry-run --Werror $(git ls-files '*.cpp' '*.h')
cargo deny check                     # licenses, advisories, sources
cargo xtask check-qt-licensing       # fails on a GPL-only Qt module or dependency

cargo bench -p render                # performance budgets from the roadmap
cargo xtask media-setup              # fetch the pinned FFmpeg (Phase 5)
```

`CMakePresets.json` is checked in; `CMakeUserPresets.json` (your local paths) is gitignored.

## Testing

- Rust: unit tests beside the code (`#[cfg(test)]`); integration tests in each crate's `tests/`.
- **Golden images:** `render` output compared to reference PNGs in `crates/render/tests/golden/` with a per-pixel tolerance; goldens are regenerated only by an explicit command and reviewed in the PR diff as data (the tolerance and the max-differing-pixels count are asserted, not eyeballed).
- **C++ and QML:** Qt Test for items and the bridge, Qt Quick Test for QML behavior, run by `ctest` (offscreen platform in CI, `[INFERENCE]` to confirm). They cover presentation and bridge behavior; document logic is tested in Rust.
- **File format:** round-trip fixtures; a format-version bump needs a migration test.
- **Store presets:** each preset is tested against the rule it encodes (size, format, alpha, count).
- **Agent bridge:** recorded MCP request/response fixtures; the installer tested against a temp home directory; never a real provider, key or token.
- **Licensing guard:** a seeded GPL-only dependency must fail the check.
- Screenshots and LLM visual inspection are not test evidence ([AGENTS.md](../../AGENTS.md)).

## Repository layout (intended)

```
opendesigner/
├── AGENTS.md  CHANGELOG.md  README.md  CONTRIBUTING.md  SECURITY.md  LICENSE
├── CMakeLists.txt  CMakePresets.json  Cargo.toml  Cargo.lock  deny.toml  .clang-format
├── assets/logo.svg          # app logo and brand source
├── app/                     # C++/QML shell (the only non-bridge code that touches Qt)
│   ├── src/                 #   bootstrap, canvas and timeline items, platform integration
│   ├── qml/                 #   panels, inspector, agent panel, theme
│   └── tests/               #   Qt Test, Qt Quick Test
├── crates/
│   ├── core/                # model, operations, history, project I/O
│   ├── render/              # rasterizer, text shaping, goldens
│   ├── store/               # presets, device frames, panorama, validation
│   ├── media/               # decode/encode, FFmpeg manifest
│   ├── agent/               # MCP server, skill installer, ACP client
│   ├── bridge/              # cxx-qt: core exposed to QML (the only Rust crate that depends on Qt)
│   └── cli/
├── skills/opendesigner/     # skill source (generated parts are checked in)
├── xtask/                   # build helpers (media setup, golden regen, icon set, licensing check)
└── docs/wiki/               # this wiki
```

## Conventions

- Work in a worktree under `.worktrees/` (see `AGENTS.md`). Set `CARGO_TARGET_DIR` to a shared directory if disk is tight; Rust and CMake build directories are large.
- Conventional-commit prefixes: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, with scope = area (`core`, `render`, `ui`, `bridge`, `canvas`, `timeline`, `media`, `mcp`, `skill`, `store`, `cli`, `build`, `repo`).
- Qt and `cxx-qt` are pinned to exact versions; bumping either is its own PR with a changelog entry. New dependencies follow the [dependency policy](../../AGENTS.md#dependencies); exceptions are logged in the [roadmap decision log](./roadmap.md#decision-log).
- An operation type, an MCP tool or a preset changes together with: the schema, the skill text generated from it, the wiki page, and the tests.
- Wiki and changelog rules: [AGENTS.md](../../AGENTS.md#wiki-documentation).
