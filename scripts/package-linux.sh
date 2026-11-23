#!/usr/bin/env bash
# Builds OpenDesigner for Linux (release) as an AppImage with Qt bundled, checks the licensing
# rules, and proves the bundle runs offscreen.
#
#   QMAKE=/usr/lib/qt6/bin/qmake6 scripts/package-linux.sh [output-dir]
#
# Linux x86_64 with Qt 6 (the distribution's packages: base, declarative, multimedia, WebEngine),
# ImageMagick, and `linuxdeploy`, `linuxdeploy-plugin-qt` and `appimagetool` on PATH (the release
# workflow installs pinned copies). The AppImage runs on distributions with the glibc of the system
# that built it or newer.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/dist}"
qmake="${QMAKE:-$(command -v qmake6 || command -v qmake || true)}"
[ -n "$qmake" ] || { echo "set QMAKE to Qt 6's qmake" >&2; exit 1; }
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
appdir="$out/OpenDesigner.AppDir"
image="$out/OpenDesigner-$version-linux-x86_64.AppImage"

echo "==> building (release)"
QMAKE="$qmake" cargo build --release -p opendesigner-app --manifest-path "$root/Cargo.toml"

echo "==> assembling $appdir"
rm -rf "$appdir"
mkdir -p "$appdir/usr/share/doc/opendesigner"
cp "$root/packaging/THIRD_PARTY.txt" "$root/crates/render/fonts/OFL.txt" "$root/LICENSE" \
   "$appdir/usr/share/doc/opendesigner/"

echo "==> bundling Qt"
icon="$(mktemp -d)/opendesigner.png"
convert "$root/assets/app-icon.png" -resize 512x512 "$icon"
# the SVG image plug-in (the logo is an SVG) and Qt Multimedia's backend are loaded at run time,
# so the deploy plug-in does not see them in the executable's imports
export QMAKE="$qmake" QML_SOURCES_PATHS="$root/crates/app/qml" NO_STRIP=1 EXTRA_QT_PLUGINS="svg;multimedia;imageformats"
# GStreamer stays the system's: a bundled libgstreamer cannot find the system's plug-ins (the
# queue, decodebin and sink elements Qt Multimedia needs), and the program then crashes.
linuxdeploy --appdir "$appdir" --plugin qt --exclude-library "libgst*" \
  --executable "$root/target/release/opendesigner-app" \
  --desktop-file "$root/packaging/linux/opendesigner.desktop" \
  --icon-file "$icon" --icon-filename opendesigner
# The offscreen platform plug-in is only there so the smoke test below can run without a display.
plugins="$("$qmake" -query QT_INSTALL_PLUGINS)"
mkdir -p "$appdir/usr/plugins/platforms"
cp --update=none "$plugins/platforms/libqoffscreen.so" "$appdir/usr/plugins/platforms/"

echo "==> licensing check"
"$root/scripts/bundle-licensing.sh" "$appdir" strip-ffmpeg

echo "==> smoke test: the bundled app runs its scripted session offscreen"
"$root/scripts/bundle-smoke.sh" "$appdir/AppRun"

echo "==> $image"
ARCH=x86_64 appimagetool "$appdir" "$image"
du -sh "$image"
