#!/usr/bin/env bash
# Builds OpenDesigner.app (release) with Qt bundled, checks that no GPL-only Qt module went in,
# ad-hoc signs it, and starts it offscreen to prove the bundle runs.
#
#   QMAKE="$(brew --prefix qt)/bin/qmake" scripts/package-macos.sh [output-dir]
#
# macOS only. The result is signed ad hoc: it runs on this machine, but distributing it needs a
# Developer ID signature and notarization, which this script does not do.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/dist}"
tmp_gpl="$(mktemp)"
qmake="${QMAKE:-$(command -v qmake || true)}"
[ -n "$qmake" ] || { echo "set QMAKE to Qt 6's qmake" >&2; exit 1; }
qt_bin="$(dirname "$qmake")"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
app="$out/OpenDesigner.app"

# Qt modules offered under the GPL only (docs/wiki/qt-desktop.md#licensing), as a case-insensitive
# pattern over file and folder names. macdeployqt copies every plug-in it finds next to the
# modules the app imports, including some of these (Qt Quick Timeline, Qt Virtual Keyboard), so
# they are removed from the bundle and the result is checked.
banned='qt3d|quick3d|canvaspainter|coap|graphs|grpc|httpserver|lottie|mqtt|networkauth|qmlcompiler|quicktimeline|/timeline|virtualkeyboard|waylandcompositor'

echo "==> building (release)"
QMAKE="$qmake" cargo build --release -p opendesigner-app --manifest-path "$root/Cargo.toml"

echo "==> assembling $app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$root/target/release/opendesigner-app" "$app/Contents/MacOS/"
sed "s/@VERSION@/$version/" "$root/packaging/Info.plist.in" > "$app/Contents/Info.plist"
cp "$root/packaging/THIRD_PARTY.txt" "$root/crates/render/fonts/OFL.txt" "$root/LICENSE" "$app/Contents/Resources/"

echo "==> icon"
tmp="$(mktemp -d)"
iconset="$tmp/OpenDesigner.iconset"
mkdir -p "$iconset"
cp "$root/assets/app-icon.png" "$tmp/square.png"
for size in 16 32 128 256 512; do
  sips -z $size $size "$tmp/square.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) "$tmp/square.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/OpenDesigner.icns"
rm -rf "$tmp"

echo "==> bundling Qt"
plugins="$("$qmake" -query QT_INSTALL_PLUGINS)"
"$qt_bin/macdeployqt" "$app" -qmldir="$root/crates/app/qml"
# The offscreen platform plug-in is only there so the smoke test below can run without a display.
cp "$plugins/platforms/libqoffscreen.dylib" "$app/Contents/PlugIns/platforms/"
offscreen="$app/Contents/PlugIns/platforms/libqoffscreen.dylib"
install_name_tool -add_rpath "@loader_path/../../Frameworks" "$offscreen"
# no path outside the bundle may resolve a Qt library: a stray Homebrew copy would load twice
install_name_tool -delete_rpath /opt/homebrew/lib "$app/Contents/MacOS/opendesigner-app" 2>/dev/null || true

echo "==> licensing check"
(cd "$app" && find . -depth | grep -iE "$banned" | while read -r path; do
  echo "removing GPL-only Qt part: $path"
  rm -rf "$path"
done)
# Qt's FFmpeg backend is a GPL build; the app runs the user's own ffmpeg instead, so a bundle
# that carries Qt's FFmpeg libraries would break invariant 10.
(cd "$app" && find . | grep -iE "lib(avcodec|avformat|avutil|swscale|swresample|postproc)" || true) > "$tmp_gpl"
if [ -s "$tmp_gpl" ]; then
  echo "the bundle carries Qt's FFmpeg libraries (GPL):" >&2
  cat "$tmp_gpl" >&2
  exit 2
fi
left="$(cd "$app" && find . | grep -iE "$banned" || true)"
if [ -n "$left" ]; then
  echo "GPL-only Qt modules are still in the bundle:" >&2
  echo "$left" >&2
  exit 2
fi
echo "frameworks:"; ls "$app/Contents/Frameworks" | sed 's/^/  /'

echo "==> signing (ad hoc)"
codesign --force --deep --sign - "$app"

echo "==> smoke test: the bundled app runs its scripted session offscreen"
work="$(mktemp -d)"
home="$(mktemp -d)"
log="$work/log.txt"
# a 1x1 PNG for the session's image import step
printf 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==' | base64 -D > "$work/fixture.png"
if ! HOME="$home" OPENDESIGNER_CONFIG_DIR="$work/cfg" QT_QPA_PLATFORM=offscreen \
     "$app/Contents/MacOS/opendesigner-app" "--selftest=$work" >"$log" 2>&1; then
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

rm -f "$tmp_gpl"
echo "==> $app"
du -sh "$app"
