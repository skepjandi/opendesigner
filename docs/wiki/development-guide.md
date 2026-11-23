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

### Packaging

Each platform has a script that builds the release binary, bundles Qt, runs `scripts/bundle-licensing.sh` (removes and then fails on any GPL-only Qt module in the bundle; the list mirrors [Qt Desktop](./qt-desktop.md#licensing)), and `scripts/bundle-smoke.sh` (starts the bundled program offscreen and runs the scripted session, `SELFTEST OK`). None signs for distribution.

- **macOS:** `QMAKE="$(brew --prefix qt)/bin/qmake" scripts/package-macos.sh` writes `dist/OpenDesigner.app` (Info.plist, an icon made from `assets/app-icon.png`, `THIRD_PARTY.txt`, the font and project licenses) with `macdeployqt`, refuses a bundle that carries Qt's FFmpeg, and signs ad hoc, so it runs on the machine that built it; other Macs need a right-click Open or `xattr -dr com.apple.quarantine`, until a Developer ID signature and notarization exist. About 375 MB with Qt WebEngine.
- **Windows:** from Git Bash with MSVC and Qt 6 (WebEngine, Multimedia) installed, `QMAKE=.../qmake.exe scripts/package-windows.sh` writes `dist/OpenDesigner/` and `dist/OpenDesigner-<version>-windows-x64.zip` with `windeployqt`, copies the Visual C++ runtime when it finds one, and deletes Qt's FFmpeg backend.
- **Linux:** with Qt 6 from the distribution, ImageMagick, and `linuxdeploy`, `linuxdeploy-plugin-qt` and `appimagetool` on `PATH`, `QMAKE=/usr/lib/qt6/bin/qmake6 scripts/package-linux.sh` writes `dist/OpenDesigner-<version>-linux-x86_64.AppImage`. It needs the glibc of the machine that built it or newer.
- **Release:** bump `version` in `Cargo.toml`, merge, push the tag `v<version>`; `.github/workflows/release.yml` builds the three packages and publishes the GitHub release with `SHA256SUMS.txt`. A pull request that touches the packaging files runs the same builds without publishing.

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
├── assets/                  # logo.svg (mark, copied into the QML module at build time), app-icon.svg/.png (tile icon)
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
