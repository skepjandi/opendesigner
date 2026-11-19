# Security

> **Status: Design.** The invariants are requirements for the implementation. Nothing here has been built or verified.

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
| A tool reads or writes outside the project | No such tool exists; `import_media` takes a path the **user** approved in the GUI or a path inside the project; paths are canonicalized and checked against the project root, symlinks resolved |
| Resource exhaustion (huge ops, huge renders) | Per-call limits on operation count, payload bytes, node counts, render dimensions; a render budget per minute |
| An agent destroys work | Every write is an attributed operation inside a checkpointed transaction; restore always works; destructive bulk operations are capped per call |
| Prompt injection through document content (an imported text node says "ignore previous instructions") | Content returned to agents is data in tool results; the skill tells the agent to treat it as untrusted; no tool can execute code, shell out or touch the network |

Kino's MCP endpoint is a useful precedent for the UX (fixed loopback port, keep the app open); what we add is the explicit token and Origin/Host policy because the endpoint can write files-in-effect.

## Skill installer

- Writes only `<skills dir>/opendesigner/` under user-approved agent locations, after showing every path.
- Never writes outside those directories, never edits agent config, credentials, history or other skills.
- Records written files and their hashes; updates only files that still match; uninstall removes exactly the recorded files.
- Refuses to follow symlinks out of the target directory; canonicalizes paths; refuses if the target exists and is not ours without explicit consent.
- Skill files are static text generated at build time from the tool schemas; they contain no scripts the agent is told to run blindly (if a `scripts/` directory is ever added, it is documented here and covered by tests).
- Registering the MCP server in an agent: show the exact command or snippet; run it only on consent, and only the agent's documented `mcp add` form.

## Files and media (parsers)

Projects, SVGs, images, fonts and video are untrusted inputs. Use maintained decoders; fuzz the project loader, the SVG importer and the preset parser; cap dimensions, node counts, nesting depth and decompressed sizes; no external references resolved on import (SVG `<image href>`, fonts by URL); media paths are project-relative and canonicalized. FFmpeg is the largest native attack surface: pinned, checksum-verified, built with network protocols disabled (`--disable-network`, as OpenCut's manifest does), updated deliberately.

## Network

Allowed destinations: the user's configured model endpoint or agent, sites the user chose to fetch an asset from (a plain `GET`, no cookies), and the update channel if enabled. No telemetry. Documented in [AGENTS.md invariant 1](../../AGENTS.md).

## Credentials

- In-app chat uses agents the user has signed in to; we do not read their credentials or tokens.
- A direct provider mode (BYOK), if built, stores keys only in the OS secure store (Keychain, Credential Manager, Secret Service), reads them only in Rust, and never logs them or writes them to projects, exports, fixtures or error messages.

## Updates and supply chain

If an updater ships: signed releases, the public key embedded, the private key out of the repository and out of CI logs. Dependencies follow the [dependency policy](../../AGENTS.md#dependencies) (popular, well-maintained, license-compatible), pinned by lockfile and checked with `cargo deny` (licenses, advisories, sources). Qt is linked dynamically and bundled per OS, so a Qt security release means a rebuild and re-release; track Qt advisories. `cxx-qt`, Qt and FFmpeg are pinned to exact versions, FFmpeg with checksums.

## Reporting

See [`SECURITY.md`](../../SECURITY.md).
