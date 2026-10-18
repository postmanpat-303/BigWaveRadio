#!/usr/bin/env bash
# Build the Linux packages from target/release/bwr-app (build it first, on the
# oldest glibc you want to support: CI uses ubuntu-22.04).
#   dist/BigWaveRadio-<ver>-linux-<arch>.tar.gz    portable (binary + desktop file + icons + install.sh)
#   dist/BigWaveRadio-<ver>-linux-<arch>.deb       cargo deb        (skipped if cargo-deb is missing)
#   dist/BigWaveRadio-<ver>-linux-<arch>.rpm       cargo generate-rpm (skipped if missing)
#   dist/BigWaveRadio-<ver>-linux-<arch>.AppImage  linuxdeploy      (skipped if LINUXDEPLOY is unset/missing)
# The Arch package is built from packaging/arch/bigwave-radio-bin/PKGBUILD (CI: archlinux container).
# Usage: scripts/package-linux.sh 0.9.8
set -euo pipefail
VER="${1:?version}"
cd "$(dirname "$0")/.."
grep -q "^version = \"$VER\"" Cargo.toml || { echo "Cargo.toml [workspace.package] version is not $VER" >&2; exit 1; }
ARCH="$(uname -m)"                 # x86_64 / aarch64
BIN=target/release/bwr-app
[ -x "$BIN" ] || cargo build --release -p bwr-app --locked
mkdir -p dist
NAME="BigWaveRadio-$VER-linux-$ARCH"

# --- tar.gz -----------------------------------------------------------------
STAGE="dist/$NAME"
rm -rf "$STAGE" && mkdir -p "$STAGE/share/applications" "$STAGE/share/metainfo"
install -m755 "$BIN" "$STAGE/bigwave-radio"
install -m644 packaging/linux/bigwave-radio.desktop "$STAGE/share/applications/"
install -m644 packaging/linux/bigwave-radio.metainfo.xml "$STAGE/share/metainfo/"
for s in 16 32 48 64 128 256 512; do
  install -Dm644 "assets/icons/bigwave-radio_$s.png" "$STAGE/share/icons/hicolor/${s}x${s}/apps/bigwave-radio.png"
done
install -m644 LICENSE "$STAGE/LICENSE"
sed "1s/.*/BigWave Radio $VER - Linux $ARCH/" packaging/linux/README.txt > "$STAGE/README.txt"
install -m755 packaging/linux/install.sh "$STAGE/install.sh"
tar -C dist -czf "dist/$NAME.tar.gz" "$NAME"
echo "dist/$NAME.tar.gz"

# --- .deb -------------------------------------------------------------------
if cargo deb --version >/dev/null 2>&1; then
  cargo deb -p bwr-app --no-build --no-strip --output "dist/$NAME.deb"
  echo "dist/$NAME.deb"
fi

# --- .rpm -------------------------------------------------------------------
if cargo generate-rpm --version >/dev/null 2>&1; then
  cargo generate-rpm -p crates/bwr-app --output "dist/$NAME.rpm"
  echo "dist/$NAME.rpm"
fi

# --- AppImage ---------------------------------------------------------------
if [ -n "${LINUXDEPLOY:-}" ] && [ -x "$LINUXDEPLOY" ]; then
  APPDIR="dist/AppDir"
  rm -rf "$APPDIR"
  mkdir -p "$APPDIR/usr/share/metainfo"
  install -m644 packaging/linux/bigwave-radio.metainfo.xml "$APPDIR/usr/share/metainfo/bigwave-radio.appdata.xml"
  for s in 16 32 48 64 128 256 512; do
    install -Dm644 "assets/icons/bigwave-radio_$s.png" "$APPDIR/usr/share/icons/hicolor/${s}x${s}/apps/bigwave-radio.png"
  done
  install -Dm755 "$BIN" "$APPDIR/usr/bin/bigwave-radio"
  # linuxdeploy bundles the non-system libraries (e.g. OpenSSL) next to the binary;
  # GL, X11/Wayland, ALSA and glibc come from the host, as AppImage expects.
  APPIMAGE_EXTRACT_AND_RUN=1 ARCH="$ARCH" LDAI_OUTPUT="dist/$NAME.AppImage" "$LINUXDEPLOY" \
    --appdir "$APPDIR" \
    --executable "$APPDIR/usr/bin/bigwave-radio" \
    --desktop-file packaging/linux/bigwave-radio.desktop \
    --icon-file assets/icons/bigwave-radio_256.png --icon-filename bigwave-radio \
    --output appimage
  chmod +x "dist/$NAME.AppImage"
  echo "dist/$NAME.AppImage"
fi
