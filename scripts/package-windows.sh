#!/usr/bin/env bash
# Builds OpenDesigner for Windows (release) with Qt bundled, checks the licensing rules, proves the
# bundle runs offscreen, and writes a zip and an installer (dist/OpenDesigner-<version>-windows-x64-setup.exe).
#
#   QMAKE=C:/Qt/6.8.3/msvc2022_64/bin/qmake.exe scripts/package-windows.sh [output-dir]
#
# Run from Git Bash on Windows with the MSVC toolchain, Qt 6 (with Qt WebEngine and Qt
# Multimedia) and Inno Setup 6 installed. The executable is not code-signed: Windows SmartScreen warns on first run.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="${1:-$root/dist}"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
qmake="${QMAKE:-$(command -v qmake || true)}"
[ -n "$qmake" ] || { echo "set QMAKE to Qt 6's qmake.exe" >&2; exit 1; }
qt_bin="$(dirname "$qmake")"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)"
name="OpenDesigner-$version-windows-x64"
dir="$out/OpenDesigner"

echo "==> building (release)"
QMAKE="$qmake" cargo build --release -p opendesigner-app --manifest-path "$root/Cargo.toml"

echo "==> assembling $dir"
rm -rf "$dir"
mkdir -p "$dir"
cp "$root/target/release/opendesigner-app.exe" "$dir/"
cp "$root/packaging/THIRD_PARTY.txt" "$root/crates/render/fonts/OFL.txt" "$root/LICENSE" "$dir/"

echo "==> bundling Qt"
"$qt_bin/windeployqt.exe" --release --no-translations --qmldir "$(cygpath -w "$root/crates/app/qml")" \
  "$(cygpath -w "$dir/opendesigner-app.exe")"
# The offscreen platform plug-in is only there so the smoke test below can run without a display.
plugins="$("$qmake" -query QT_INSTALL_PLUGINS)"
cp "$(cygpath -u "$plugins")/platforms/qoffscreen.dll" "$dir/platforms/"

# The Microsoft C++ runtime the program and Qt link dynamically (vcruntime140*.dll, msvcp140*.dll),
# so the bundle runs on a machine that has not installed the Visual C++ Redistributable.
crt=""
for candidate in "/c/Program Files/Microsoft Visual Studio/"*/*/VC/Redist/MSVC/*/x64/Microsoft.VC14*.CRT; do
  [ -d "$candidate" ] && crt="$candidate"
done
if [ -n "$crt" ]; then
  echo "==> C++ runtime from $crt"
  cp "$crt"/*.dll "$dir/"
else
  echo "warning: no Visual C++ runtime found; the bundle needs the Visual C++ Redistributable installed" >&2
fi

echo "==> licensing check"
"$root/scripts/bundle-licensing.sh" "$dir" strip-ffmpeg

echo "==> smoke test: the bundled app runs its scripted session offscreen"
"$root/scripts/bundle-smoke.sh" "$dir/opendesigner-app.exe"

echo "==> installer"
iscc="$(command -v ISCC.exe || true)"
for candidate in "/c/Program Files (x86)/Inno Setup 6/ISCC.exe" "/c/Program Files/Inno Setup 6/ISCC.exe"; do
  [ -n "$iscc" ] || { [ -x "$candidate" ] && iscc="$candidate"; }
done
[ -n "$iscc" ] || { echo "Inno Setup 6 (ISCC.exe) is needed for the installer" >&2; exit 1; }
icon_args=()
if command -v magick >/dev/null; then
  magick "$root/assets/app-icon.png" -define icon:auto-resize=256,128,64,48,32,16 "$out/app.ico"
  icon_args=("/DIconFile=$(cygpath -w "$out/app.ico")")
else
  echo "warning: ImageMagick not found; the installer has no icon" >&2
fi
MSYS2_ARG_CONV_EXCL='*' "$iscc" "/DAppVersion=$version" "/DSourceDir=$(cygpath -w "$dir")" \
  "/DOutputDir=$(cygpath -w "$out")" "${icon_args[@]}" "$(cygpath -w "$root/packaging/windows/opendesigner.iss")"
rm -f "$out/app.ico"

echo "==> $out/$name.zip"
rm -f "$out/$name.zip"
(cd "$out" && 7z a -tzip -mx=6 "$name.zip" OpenDesigner >/dev/null)
du -sh "$dir" "$out/$name.zip" "$out/OpenDesigner-$version-windows-x64-setup.exe"
