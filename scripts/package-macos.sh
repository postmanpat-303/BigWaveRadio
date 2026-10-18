#!/usr/bin/env bash
# Build "BigWave Radio.app" (universal: Apple Silicon + Intel), ad-hoc sign it,
# and package it. Runs on macOS only (CI: .github/workflows/macos.yml).
#   dist/BigWaveRadio-<ver>-macos-universal.dmg   (app + Applications link)
#   dist/BigWaveRadio-<ver>-macos-universal.zip   (the .app)
# Usage: scripts/package-macos.sh 0.9.7
set -euo pipefail
VER="${1:?version}"
cd "$(dirname "$0")/.."
grep -q "^version = \"$VER\"" Cargo.toml || { echo "Cargo.toml [workspace.package] version is not $VER" >&2; exit 1; }
export MACOSX_DEPLOYMENT_TARGET=11.0
for t in aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release --locked -p bwr-app --target "$t"
done
mkdir -p dist
APP="dist/BigWave Radio.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/BigWaveRadio" \
  target/aarch64-apple-darwin/release/bwr-app \
  target/x86_64-apple-darwin/release/bwr-app
lipo -info "$APP/Contents/MacOS/BigWaveRadio"
sed "s/@VERSION@/$VER/g" packaging/macos/Info.plist.in > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
cp assets/BigWaveRadio.icns "$APP/Contents/Resources/BigWaveRadio.icns"
printf 'APPL????' > "$APP/Contents/PkgInfo"
# Unsigned for Gatekeeper, but ad-hoc signed so Apple Silicon will run it.
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict --verbose=2 "$APP"

NAME="BigWaveRadio-$VER-macos-universal"
rm -f "dist/$NAME.zip" "dist/$NAME.dmg"
ditto -c -k --sequesterRsrc --keepParent "$APP" "dist/$NAME.zip"
DMGDIR="$(mktemp -d)"
cp -R "$APP" "$DMGDIR/"
ln -s /Applications "$DMGDIR/Applications"
cp packaging/macos/README.txt "$DMGDIR/Read me first.txt"
hdiutil create -volname "BigWave Radio $VER" -srcfolder "$DMGDIR" -fs HFS+ -format UDZO -ov "dist/$NAME.dmg"
rm -rf "$DMGDIR"
ls -la dist/"$NAME".*
