#!/bin/sh
# Install BigWave Radio from this folder for the current user (no root):
#   ~/.local/bin/bigwave-radio, desktop entry and icons under ~/.local/share.
# Uninstall: ./install.sh --uninstall   (your stations and settings in
# ~/.local/share/BigWaveRadio are kept)
set -e
cd "$(dirname "$0")"
BIN="${XDG_BIN_HOME:-$HOME/.local/bin}"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
files="applications/bigwave-radio.desktop metainfo/bigwave-radio.metainfo.xml"
for s in 16 32 48 64 128 256 512; do files="$files icons/hicolor/${s}x${s}/apps/bigwave-radio.png"; done
if [ "${1:-}" = "--uninstall" ]; then
  rm -f "$BIN/bigwave-radio"
  for f in $files; do rm -f "$DATA/$f"; done
  echo "BigWave Radio removed."
  exit 0
fi
install -Dm755 bigwave-radio "$BIN/bigwave-radio"
for f in $files; do install -Dm644 "share/$f" "$DATA/$f"; done
sed -i "s|^Exec=bigwave-radio|Exec=$BIN/bigwave-radio|" "$DATA/applications/bigwave-radio.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$DATA/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$DATA/icons/hicolor" 2>/dev/null || true
echo "BigWave Radio installed: $BIN/bigwave-radio (and in your applications menu)."
