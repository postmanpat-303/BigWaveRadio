#!/usr/bin/env bash
# Build the Windows x64 release with Microsoft's toolchain (MSVC, static CRT):
#   dist/BigWaveRadio-<ver>-windows-x64.zip   portable: exe + README + licences
#   dist/BigWaveRadio-<ver>-setup-x64.exe     NSIS installer (installer/bigwave.nsi)
#   dist/SHA256SUMS-windows.txt               sha256sum lines for both
# The exe is self-contained: no libunwind.dll, no vcruntime (crt-static).
# Runs in Git Bash on Windows (CI: .github/workflows/windows.yml on windows-latest).
# Usage: scripts/package-windows.sh 0.9.7
# Needs: Visual Studio Build Tools (MSVC + Windows SDK), rustup target
#        x86_64-pc-windows-msvc, python, NSIS 3 (makensis) for the setup exe.
set -euo pipefail
VER="${1:?version}"
cd "$(dirname "$0")/.."
TARGET=x86_64-pc-windows-msvc
export RUSTFLAGS="${RUSTFLAGS:--C target-feature=+crt-static}"
PY=$(command -v python3 || command -v python)
export PYTHONUTF8=1   # licence texts are UTF-8 (Windows Python defaults to cp1252)
# [workspace.package] version (shared by every crate; the update checker compares it with the tag)
grep -q "^version = \"$VER\"" Cargo.toml || { echo "Cargo.toml [workspace.package] version is not $VER" >&2; exit 1; }
cargo build --release --locked --target "$TARGET" -p bwr-app
rm -rf dist/pkg && mkdir -p dist/pkg
cp "target/$TARGET/release/bwr-app.exe" dist/pkg/BigWaveRadio.exe
sed "1s/.*/BigWave Radio $VER - Windows x64/" dist/README.txt > dist/pkg/README.txt
cp LICENSE dist/pkg/LICENSE.txt
"$PY" scripts/third_party_licenses.py "$TARGET" > dist/pkg/THIRD_PARTY_LICENSES.txt
# Windows line endings for Notepad users
for f in dist/pkg/*.txt; do sed -i 's/\r*$/\r/' "$f"; done
ZIP="dist/BigWaveRadio-$VER-windows-x64.zip"
rm -f "$ZIP"
"$PY" - "$ZIP" <<'PYEOF'
import sys, zipfile
with zipfile.ZipFile(sys.argv[1], "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
    for f in ["BigWaveRadio.exe", "README.txt", "LICENSE.txt", "THIRD_PARTY_LICENSES.txt"]:
        z.write("dist/pkg/" + f, f)
PYEOF
echo "$ZIP"
OUT=("BigWaveRadio-$VER-windows-x64.zip")
MAKENSIS=$(command -v makensis || true)
for c in "/c/Program Files (x86)/NSIS/makensis.exe" "/c/Program Files/NSIS/makensis.exe"; do
  [ -z "$MAKENSIS" ] && [ -x "$c" ] && MAKENSIS="$c"
done
if [ -n "$MAKENSIS" ]; then
  # NSIS VIProductVersion needs four numbers (0.9.7 -> 0.9.7.0)
  VI=$(echo "$VER" | awk -F. '{printf "%d.%d.%d.%d", $1, $2, $3, $4}')
  # makensis accepts -D switches on Windows too (no MSYS path mangling).
  (cd installer && "$MAKENSIS" -V2 -DVERSION="$VER" -DVIVERSION="$VI" -DPKGDIR=../dist/pkg \
    -DOUTFILE="../dist/BigWaveRadio-$VER-setup-x64.exe" bigwave.nsi)
  echo "dist/BigWaveRadio-$VER-setup-x64.exe"
  OUT+=("BigWaveRadio-$VER-setup-x64.exe")
else
  echo "makensis not found: setup exe skipped" >&2
fi
# "<hash>  <name>" like the release's SHA256SUMS.txt (Git Bash sha256sum adds a "*" binary marker)
(cd dist && sha256sum "${OUT[@]}" | sed 's/ \*/  /' > SHA256SUMS-windows.txt && cat SHA256SUMS-windows.txt)
