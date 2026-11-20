# Development Guide

> **Status: Implemented for what exists.** Everything below was run on macOS arm64 (Qt 6.12 from Homebrew). Windows and Linux builds are not verified. There is no CMake project and one hand-written C++ file (the image provider in `crates/app/cpp/`): Cargo builds the whole app, and `cxx-qt-build` generates and compiles the Qt glue.

## Prerequisites

| Tool | Needed for |
|------|------------|
| Rust stable ≥ 1.85 with `rustfmt` and `clippy` (`rust-toolchain.toml` pins `stable`) | Everything |
| `cargo-deny` | Licenses, advisories, sources |
| Qt 6 with Qt Quick Controls 2 and Qt Quick Dialogs, and its `qmake` (`brew install qt`) | The `app` crate only |
| A C++17 compiler: Xcode command line tools, MSVC, or GCC/Clang | `cxx-qt` compiles generated C++ |

The `app` crate is a workspace member but **not** a default member, so `cargo build`, `cargo test` and `cargo clippy` on a machine without Qt skip it. Build the shell with `-p opendesigner-app` and point the build at Qt:

```bash
export QMAKE="$(brew --prefix qt)/bin/qmake"     # macOS with Homebrew; any Qt 6 qmake elsewhere
```

## Commands

```bash
# no Qt needed: core, render, store, agent, cli
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test                          # 80+ tests, including the MCP server and CLI end to end
cargo deny check                    # advisories, bans, licenses, sources (deny.toml)
cargo run --release -p opendesigner-render --example bench   # render / export / transaction timings vs the roadmap budgets

# the editor (needs QMAKE; the first build compiles cxx-qt and takes a few minutes)
cargo run -p opendesigner-app
cargo clippy -p opendesigner-app --all-targets -- -D warnings
cargo test -p opendesigner-app      # unit tests + the scripted offscreen session of the real window

# the headless editor
cargo run -p opendesigner-cli -- new demo.od
cargo run -p opendesigner-cli -- serve-mcp demo.od      # agents connect over MCP
cargo run -p opendesigner-cli -- skill status           # where the agent skill would go
cargo run -p opendesigner-cli -- export demo.od --frame n_3 --preset apple-iphone-6.9 --out shot.png
```

`opendesigner --help` lists every subcommand. `export` writes nothing and exits with code 2 when the image does not meet its preset.

Run the editor offscreen, the way the test does: `QT_QPA_PLATFORM=offscreen target/debug/opendesigner-app --selftest=/some/empty/folder` prints `SELFTEST OK` and leaves `window.png` (a grab of the window) next to the project it built. Use a throwaway `HOME` and `OPENDESIGNER_CONFIG_DIR` so the session does not touch your real agent folders or token.

### Packaging (macOS)

`QMAKE="$(brew --prefix qt)/bin/qmake" scripts/package-macos.sh` builds the release binary, assembles `dist/OpenDesigner.app` (Info.plist, an icon made from `assets/logo.svg`, `THIRD_PARTY.txt`, the font and project licenses), runs `macdeployqt` with the QML folder so only the Qt modules the app imports are bundled, **fails if any GPL-only Qt module is in the bundle** (the list in the script mirrors [Qt Desktop](./qt-desktop.md#licensing)), signs it ad hoc, and starts the bundled binary offscreen to run the scripted session (`SELFTEST OK`). The build takes about 5 minutes the first time. The bundle is signed ad hoc, so it runs on the machine that built it; distributing it needs a Developer ID signature and notarization, which the script does not do. There are no Windows or Linux packaging scripts.

## Testing

- Rust: unit tests beside the code (`#[cfg(test)]`); integration tests in each crate's `tests/`.
- **MCP server:** protocol tests without a socket (`crates/agent/tests/mcp.rs`), socket-level security tests (token, `Host`, `Origin`, methods, sizes: `tests/server.rs`) and a test that starts the real binary and connects as an agent (`crates/cli/tests/serve.rs`).
- **Skill installer:** snapshots a temporary home folder and asserts only the skill folders change (`crates/agent/tests/skill.rs`).
- **The editor:** the logic is in `crates/app/src/state.rs` and unit-tested without Qt; `crates/app/tests/selftest.rs` runs `qml/SelfTest.qml` against the real window offscreen.
- **Golden images** for `render` are not in place yet; renderer tests assert pixels at known points.
- Screenshots and LLM visual inspection are not test evidence ([AGENTS.md](../../AGENTS.md)).

## Repository layout

```
opendesigner/
├── AGENTS.md  CHANGELOG.md  README.md  CONTRIBUTING.md  SECURITY.md  LICENSE
├── Cargo.toml  Cargo.lock  deny.toml  clippy.toml  rustfmt.toml  rust-toolchain.toml
├── assets/logo.svg          # app logo and brand source (copied into the QML module at build time)
├── crates/
│   ├── core/                # model, operations, history, project I/O, annotations, geometry, views
│   ├── render/              # CPU rasterizer and PNG export
│   ├── store/               # store presets and validation
│   ├── agent/               # MCP server, tools, token, skill generator and installer
│   ├── cli/                 # the `opendesigner` command
│   └── app/                 # the Qt Quick editor: src/{editor,state}.rs, qml/, tests/
└── docs/wiki/               # this wiki
```

Not built yet (design only): `media` (video), the in-app chat, a Windows/Linux CI for the shell.

## Conventions

- Work in a worktree under `.worktrees/` (see `AGENTS.md`). Set `CARGO_TARGET_DIR` to a shared directory if disk is tight; the Qt shell's build output is large.
- Conventional-commit prefixes: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `perf`, with scope = area (`core`, `render`, `ui`, `canvas`, `timeline`, `media`, `mcp`, `skill`, `store`, `cli`, `build`, `repo`).
- `cxx-qt` and Qt versions are pinned; bumping either is its own PR with a changelog entry. New dependencies follow the [dependency policy](../../AGENTS.md#dependencies); exceptions are logged in the [roadmap decision log](./roadmap.md#decision-log).
- An operation type, an MCP tool or a preset changes together with: the schema, the skill text generated from it, the wiki page, and the tests.
- Wiki and changelog rules: [AGENTS.md](../../AGENTS.md#wiki-documentation).
