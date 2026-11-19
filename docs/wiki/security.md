# Security

> **Status: Partially implemented.** The MCP endpoint's mitigations in [MCP endpoint](#mcp-endpoint) are built in `crates/agent` and covered by socket-level tests (token, `Host`, `Origin`, method and size limits, loopback bind). Also built: media paths and import, export file names, the chat's no-shell command start and its refusal of file and terminal requests, tested as described in [Agent Integration](./agent-integration.md). Everything else here is a requirement for code not yet written (fuzzing, update signing, video tooling). Nothing has been reviewed by a third party.

## Assets

| Asset | Why it matters |
|-------|----------------|
| The user's files (projects, media, anything on disk) | An agent or a malicious project must not read or write outside the project |
| Agent credentials and subscriptions | Held by the agent's own tool, never by us; direct-provider keys, if built, are financial exposure |
| MCP bearer token | Possession lets a local process drive the editor |
| Agent configuration and history files | The installer must not touch them |
| Unreleased app and marketing material in projects | Leaks via exports, logs or the network |
| The update channel, if enabled | Supply-chain path to every install |

## Trust boundaries

```
 [User + GUI: trusted] <--> [Core: validates] <--MCP, token, 127.0.0.1--> [Agent process: UNTRUSTED]
                                   |                                     [Project files, media: UNTRUSTED input]
                          [Filesystem: project dir only]                 [Web pages in the user's browser: UNTRUSTED]
```

An agent is a program steered by a model reading arbitrary content (web pages, imported text, images). Assume it can be prompt-injected and will, sometimes, do the wrong thing with the tools it has. So the tools it has must be incapable of harm beyond the document, and the document must be recoverable.

## MCP endpoint

Threats and mitigations:

| Threat | Mitigation |
|--------|------------|
| A web page in the user's browser calls `http://127.0.0.1:<port>/mcp` (CSRF, DNS rebinding) | Bearer token required on every request; reject unknown `Origin`; require `Host` equal to the literal loopback address and port; no CORS headers; token never in a URL |
| Another local user or process on a shared machine | Loopback bind only; token file readable by the user only; per-install random token; the server is off unless enabled |
| Token leaks via logs, crash output or the skill | Never log it; the skill does not contain it (the agent is given the one-line connect command, or the app registers it after consent); rotate on demand in Settings |
| A tool reads or writes outside the project | No such tool exists. An image node's path must be `media/<plain file name>` (checked by `core` on every create, set and load); the renderer opens only regular files directly in `media/` and refuses symlinks. Files enter `media/` only through the editor's import, which the **user** starts with a file dialog or a drop; there is no agent `import_media` (it would read a path from the user's disk). `export` writes plain file names under `exports/` |
| Resource exhaustion (huge ops, huge renders) | Per-call limits on operation count, payload bytes, node counts, render dimensions; a render budget per minute |
| An agent destroys work | Every write is an attributed operation inside a checkpointed transaction; restore always works; destructive bulk operations are capped per call |
| Prompt injection through document content (an imported text node says "ignore previous instructions") | Content returned to agents is data in tool results; the skill tells the agent to treat it as untrusted; no tool can execute code, shell out or touch the network |

Kino's MCP endpoint is a useful precedent for the UX (fixed loopback port, keep the app open); what we add is the explicit token and Origin/Host policy because the endpoint can write files-in-effect.


**Implemented** in `crates/agent/src/server.rs`: bind to `127.0.0.1` only; every request must pass `guard` before any handler: any `Origin` header is refused (`403`), `Host` must be `127.0.0.1:<port>` or `localhost:<port>` (`403`), `Authorization: Bearer <token>` is compared in constant time (`401` with `WWW-Authenticate: Bearer`), then the protocol version header is checked (`400`). Bodies over 4 MB are refused (`413`). The token is 32 random bytes from the OS (`getrandom`), stored in the per-user config directory with mode `0600` on Unix (on Windows the user profile's default permissions apply; not hardened further), never printed unless `--show-token` is given, and replaced with `--rotate-token`. Tests: `crates/agent/tests/server.rs` and `crates/cli/tests/serve.rs`.

## Agent memory

`write_memory` is a write surface outside the project folder: `<repo>/.opendesigner/memory/`, where `<repo>` is the parent of the `opendesigner/` assets folder (a project that is not inside an `opendesigner/` folder keeps its notes in `<project>/.opendesigner/`) (`od_agent::memory`). Rules, each covered by a unit test:

- Names are plain `[a-z0-9._-]+`, at most 64 characters, ending `.md` or `.txt`, no leading dot, no `..`, no separators. The path is always `memory/` joined with a validated name; the agent never supplies a path.
- Symlinks are never followed (`symlink_metadata`): a linked note, `.opendesigner` or `memory/` is refused for reads, writes and listing.
- Caps: 64 KB per note, 200 notes, 2 MB in total. Text only (no NUL bytes); the folder never holds binaries.
- Writes are atomic (temporary file, then rename). Append adds a dated entry; replace swaps the file.
- OpenDesigner creates the folder and a README when a working folder is opened (best effort) and never edits `.gitignore`.
- Notes are untrusted data on the way back: an agent reading a note treats it like any document content, not as an instruction. The README, the tool description and the playbook say never to store secrets, tokens or passwords, because the folder is meant to be committed. Nothing scans for secrets, so this is guidance, not a guarantee.

## Skill installer

**Implemented** in `crates/agent/src/skill.rs` (tests in `crates/agent/tests/skill.rs` and `crates/cli/tests/skill.rs`): every bullet below; the first-run consent dialog is in the Qt app (`qml/main.qml`, state in `crates/app/src/state.rs`), which lists the paths and writes nothing without a click. The tests snapshot a temporary home folder and assert that an install adds files only under `.claude/skills/opendesigner/` and `.agents/skills/opendesigner/`, leaves every other file byte-identical (agent settings included), and that uninstall restores the snapshot.

- Writes only `<skills dir>/opendesigner/` under user-approved agent locations, after showing every path.
- Never writes outside those directories, never edits agent config, credentials, history or other skills.
- Records written files and their hashes; updates only files that still match; uninstall removes exactly the recorded files.
- Refuses to follow symlinks out of the target directory; canonicalizes paths; refuses if the target exists and is not ours without explicit consent.
- Skill files are static text generated at build time from the tool schemas; they contain no scripts the agent is told to run blindly (if a `scripts/` directory is ever added, it is documented here and covered by tests).
- Registering the MCP server in an agent: show the exact command or snippet; run it only on consent, and only the agent's documented `mcp add` form.

## Files and media (parsers)

Projects, SVGs, images, fonts and video are untrusted inputs. Use maintained decoders; fuzz the project loader, the SVG importer and the preset parser; cap dimensions, node counts, nesting depth and decompressed sizes; no external references resolved on import (SVG `<image href>`, fonts by URL); media paths are `media/<plain name>`. Built: images are sniffed by magic bytes on import (PNG, JPEG, WebP; 64 MB cap) and decoded by the `image` crate with limits (16,384 px per side, 100 megapixels, 512 MB allocation); no fuzzing of the decoders is in place yet. FFmpeg is the largest native attack surface: pinned, checksum-verified, built with network protocols disabled (`--disable-network`, as OpenCut's manifest does), updated deliberately.

Video export runs the `ffmpeg` program (found in `OPENDESIGNER_FFMPEG`, next to the app, or on `PATH`; never a path from a project file or an agent) as a child process with fixed arguments, raw frames on stdin and one output path inside the project's `exports/`, a plain `.mp4` name checked like the image export names. The agent chooses the video, the file name and the preset, nothing on the command line. Frames are drawn in this process; the child never reads project files.

## Device access

The user chose to let agents reach pictures and videos anywhere on the computer, so that "use the logo in my Pictures folder" works without hunting for a file dialog. This replaces the earlier rule that no tool reads outside the project (invariant 5). What stays: it is read-only and narrow. `find_files` lists names, sizes and dates of PNG, JPEG, WebP, MP4, MOV and M4V files under the home folder (hidden folders, caches and package folders skipped, symlinks not followed, 200 results); `import_file` copies one file into the project's `media/` after the same magic-byte check the user's imports get (so a text file renamed `.png` is refused), never overwriting and never touching the original. No tool returns file contents, lists other file types, or writes outside `media/` and `exports/`. A switch in the Connect tab (a marker file `device-access-off` in the config folder) turns it off; it is on by default because the user asked. Residual risk: an agent can learn the names of the user's pictures and copy them into a project, from where an export could carry them out; a prompt-injected agent could do the same. Turn the switch off when running an agent you do not trust.

## Network

Allowed destinations: the user's configured model endpoint or agent, sites the user chose to fetch an asset from (a plain `GET`, no cookies), and the update channel if enabled. No telemetry. Documented in [AGENTS.md invariant 1](../../AGENTS.md).

## Credentials

- In-app chat uses agents the user has signed in to; we do not read their credentials or tokens. The chat starts only the command the user typed or picked (no shell, never from a project file or an agent), gives the agent the loopback MCP URL and its bearer token, and serves it no file or terminal capability. Note what the agent can do on its own: it runs with the user's rights in the project folder and asks permission through its own prompts, which the editor shows; the editor cannot sandbox it.
- A direct provider mode (BYOK), if built, stores keys only in the OS secure store (Keychain, Credential Manager, Secret Service), reads them only in Rust, and never logs them or writes them to projects, exports, fixtures or error messages.

## Updates and supply chain

If an updater ships: signed releases, the public key embedded, the private key out of the repository and out of CI logs. Dependencies follow the [dependency policy](../../AGENTS.md#dependencies) (popular, well-maintained, license-compatible), pinned by lockfile and checked with `cargo deny` (licenses, advisories, sources). Qt is linked dynamically and bundled per OS, so a Qt security release means a rebuild and re-release; track Qt advisories. `cxx-qt`, Qt and FFmpeg are pinned to exact versions, FFmpeg with checksums.

## Reporting

See [`SECURITY.md`](../../SECURITY.md).
