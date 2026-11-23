#!/usr/bin/env bash
# Starts a packaged editor offscreen and runs its scripted session (`--selftest`) to prove the
# bundle runs with the Qt it carries. Shared by the per-platform package scripts.
#
#   scripts/bundle-smoke.sh <path-to-the-packaged-executable>
#
# The program runs with an empty HOME and its own config folder, so it never touches the real ones.
set -euo pipefail

exe="${1:?usage: bundle-smoke.sh <packaged-executable>}"
work="$(mktemp -d)"
home="$(mktemp -d)"
log="$work/log.txt"
native() { if command -v cygpath >/dev/null; then cygpath -w "$1"; else printf '%s' "$1"; fi; }

# an agent the session can find (the scripted session expects the skill offer for it)
mkdir -p "$home/.claude"

# a 1x1 PNG for the session's image import step
printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -d > "$work/fixture.png"

run() {
  HOME="$home" USERPROFILE="$(native "$home")" OPENDESIGNER_CONFIG_DIR="$(native "$work/cfg")" QT_QPA_PLATFORM=offscreen QT_ASSUME_STDERR_HAS_CONSOLE=1 \
    "$exe" "--selftest=$(native "$work")" >"$log" 2>&1
}
status=0
run || status=$?
if [ "$status" -ne 0 ]; then
  echo "the scripted session exited with status $status; its output:" >&2
  cat "$log" >&2
  GST_DEBUG=2 run || true
  echo "again with GStreamer warnings:" >&2
  grep -E "ERROR|WARN" "$log" | head -40 >&2 || true
  exit 3
fi
if grep -q "implemented in both" "$log"; then
  echo "Qt is loaded twice (the bundle and another install):" >&2
  grep "implemented in both" "$log" | head -3 >&2
  exit 4
fi
grep -q "SELFTEST OK" "$log" || { cat "$log" >&2; exit 3; }
echo "SELFTEST OK from the bundle"
rm -rf "$work" "$home"
