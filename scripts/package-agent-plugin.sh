#!/usr/bin/env bash
# Regenerates the agent plugin (the OpenDesigner skill and the Claude Code and Codex manifests)
# from the tool schema into plugins/opendesigner and .claude-plugin, so the plugin cannot drift
# from what the MCP server accepts. Run it after changing a tool, a playbook or the version, and
# commit the result; the test `plugin::tests::the_committed_plugin_is_exactly_what_the_generator_writes`
# (and `opendesigner package-plugin --check`) fail while the committed files are stale.
#
#   scripts/package-agent-plugin.sh           # write
#   scripts/package-agent-plugin.sh --check   # report only, exit 2 when stale
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
exec cargo run --quiet --locked -p opendesigner-cli -- package-plugin --out "$root" "$@"
