# Security Policy

## Reporting a vulnerability

Do **not** open a public issue. Report privately through [GitHub private vulnerability reporting](https://github.com/skepjandi/opendesigner/security/advisories/new).

Include affected version/platform, reproduction steps, and impact. Expect an acknowledgement within 7 days.

## Scope

The threat model is in [docs/wiki/security.md](docs/wiki/security.md). Of particular interest: anything that lets a web page or another local process drive the MCP endpoint, agent output that bypasses operation validation, the skill installer writing outside its own directory, malicious project or media files (parser bugs, path traversal), and credential exposure.

## Supported versions

Only the latest release and `main`.
