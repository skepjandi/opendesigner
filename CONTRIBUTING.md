# Contributing to OpenDesigner

Read [AGENTS.md](AGENTS.md) first. It applies to humans and AI agents alike.

1. Read the relevant [wiki](docs/wiki/index.md) page before changing code.
2. Branch from `origin/main` (ideally in a git worktree under `.worktrees/`). Never commit to `main`.
3. Use conventional-commit prefixes (`feat(core):`, `fix(mcp):`, `docs(wiki):`).
4. All checks must pass before opening a PR:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo deny check
# the Qt editor (needs Qt 6; see the development guide)
QMAKE="$(brew --prefix qt)/bin/qmake" cargo clippy -p opendesigner-app --all-targets -- -D warnings
QMAKE="$(brew --prefix qt)/bin/qmake" cargo test -p opendesigner-app
```

5. Add a `CHANGELOG.md` entry; update the wiki if behavior or architecture changes.
6. Adding a dependency? Only popular, well-maintained, license-compatible packages; put the evidence in the PR ([policy](AGENTS.md#dependencies)). Adding a Qt module? It must be LGPLv3 and on the allowlist ([licensing](docs/wiki/qt-desktop.md#licensing)).
7. Never commit secrets, and use only media you may redistribute in fixtures.

Prerequisites and platform notes are in the [development guide](docs/wiki/development-guide.md).

## License

By contributing you agree your contributions are licensed under [MPL-2.0](LICENSE). Each source file is covered by the license via the repository `LICENSE`; no per-file header is required.
