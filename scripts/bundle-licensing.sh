#!/usr/bin/env bash
# Licensing gate for a deployed Qt bundle (AGENTS.md invariants 10 and 12), shared by the
# per-platform package scripts.
#
#   scripts/bundle-licensing.sh <bundle-dir> [fail-on-ffmpeg|strip-ffmpeg]
#
# 1. Removes every file or folder named after a Qt module offered under the GPL only
#    (docs/wiki/qt-desktop.md#licensing): the Qt deploy tools copy plug-ins that sit beside the
#    modules the app imports, including some of these, and then fails if any remain.
# 2. Handles Qt's FFmpeg backend (a separate FFmpeg build from the user's `ffmpeg`, which the app
#    runs as a program): `fail-on-ffmpeg` (default) refuses a bundle that carries it,
#    `strip-ffmpeg` deletes the libraries and the Qt Multimedia plug-in that loads them, so Qt
#    Multimedia falls back to the platform's own backend.
set -euo pipefail

dir="${1:?usage: bundle-licensing.sh <bundle-dir> [fail-on-ffmpeg|strip-ffmpeg]}"
mode="${2:-fail-on-ffmpeg}"

banned='qt3d|quick3d|canvaspainter|coap|graphs|grpc|httpserver|lottie|mqtt|networkauth|qmlcompiler|quicktimeline|/timeline|virtualkeyboard|waylandcompositor'
ffmpeg='(^|/)(lib)?(avcodec|avformat|avutil|swscale|swresample|postproc|avfilter|avdevice)([-._][^/]*)?$|ffmpegmediaplugin'

[ -d "$dir" ] || { echo "no such bundle directory: $dir" >&2; exit 1; }

remove_matching() { # pattern label: delete what matches; no match is not an error
  (cd "$dir" && { find . -depth | grep -iE "$1" || true; } | while read -r path; do
    echo "removing $2: $path"
    rm -rf "$path"
  done)
}

remove_matching "$banned" "GPL-only Qt part"

case "$mode" in
  strip-ffmpeg)
    remove_matching "$ffmpeg" "Qt's FFmpeg backend"
    left="$(cd "$dir" && find . | grep -iE "$ffmpeg" || true)"
    if [ -n "$left" ]; then
      echo "Qt's FFmpeg backend is still in the bundle:" >&2
      echo "$left" >&2
      exit 2
    fi
    ;;
  fail-on-ffmpeg)
    found="$(cd "$dir" && find . | grep -iE "$ffmpeg" || true)"
    if [ -n "$found" ]; then
      echo "the bundle carries Qt's FFmpeg backend:" >&2
      echo "$found" >&2
      exit 2
    fi
    ;;
  *) echo "unknown mode: $mode" >&2; exit 1 ;;
esac

left="$(cd "$dir" && find . | grep -iE "$banned" || true)"
if [ -n "$left" ]; then
  echo "GPL-only Qt modules are still in the bundle:" >&2
  echo "$left" >&2
  exit 2
fi
echo "licensing check passed"
