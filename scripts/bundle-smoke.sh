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

# a 1x1 PNG for the session's image import step
printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -d > "$work/fixture.png"

if ! HOME="$home" OPENDESIGNER_CONFIG_DIR="$(native "$work/cfg")" QT_QPA_PLATFORM=offscreen \
     "$exe" "--selftest=$(native "$work")" >"$log" 2>&1; then
  cat "$log" >&2
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
