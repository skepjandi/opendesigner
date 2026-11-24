#!/usr/bin/env bash
# Builds OpenDesigner.app (release) with Qt bundled, checks that no GPL-only Qt module went in,
# ad-hoc signs it, starts it offscreen to prove the bundle runs, and wraps it in a .dmg.
#
#   QMAKE="$(brew --prefix qt)/bin/qmake" scripts/package-macos.sh [output-dir]
#
# macOS only. The result is signed ad hoc: it runs on this machine, but distributing it needs a
# Developer ID signature and notarization, which this script does not do.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/dist}"
qmake="${QMAKE:-$(command -v qmake || true)}"
[ -n "$qmake" ] || { echo "set QMAKE to Qt 6's qmake" >&2; exit 1; }
qt_bin="$(dirname "$qmake")"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
app="$out/OpenDesigner.app"

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
"$root/scripts/bundle-licensing.sh" "$app" fail-on-ffmpeg
echo "frameworks:"; ls "$app/Contents/Frameworks" | sed 's/^/  /'

echo "==> signing (ad hoc)"
codesign --force --deep --sign - "$app"

echo "==> smoke test: the bundled app runs its scripted session offscreen"
"$root/scripts/bundle-smoke.sh" "$app/Contents/MacOS/opendesigner-app"

echo "==> disk image"
dmg="$out/OpenDesigner-$version-macos-$(uname -m).dmg"
stage="$(mktemp -d)"
ditto "$app" "$stage/OpenDesigner.app"
ln -s /Applications "$stage/Applications"
rm -f "$dmg"
hdiutil create -quiet -volname "OpenDesigner $version" -srcfolder "$stage" -format UDZO -ov "$dmg"
rm -rf "$stage"

echo "==> $app"
du -sh "$app" "$dmg"
