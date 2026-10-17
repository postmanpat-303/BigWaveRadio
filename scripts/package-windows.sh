#!/usr/bin/env bash
# Build the Windows x64 zip (llvm-mingw cross-compile): exe + libunwind.dll + README + licences.
# Also builds the NSIS setup exe when makensis is installed. Same flow as ChipWaveTracker.
# Usage: scripts/package-windows.sh 0.9.5
# Needs: rust target x86_64-pc-windows-gnullvm, llvm-mingw (LLVM_MINGW=/path, default below), makensis, zip.
set -euo pipefail
VER="${1:?version}"
cd "$(dirname "$0")/.."
LLVM_MINGW="${LLVM_MINGW:-/workspace/llvm-mingw-20261006-ucrt-ubuntu-22.04-x86_64}"
export PATH="$LLVM_MINGW/bin:$PATH"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=x86_64-w64-mingw32-clang
export CC_x86_64_pc_windows_gnullvm=x86_64-w64-mingw32-clang
export AR_x86_64_pc_windows_gnullvm=llvm-ar
export WINDRES=x86_64-w64-mingw32-windres
# [workspace.package] version (shared by every crate; the update checker compares it with the tag)
grep -q "^version = \"$VER\"" Cargo.toml || { echo "Cargo.toml [workspace.package] version is not $VER" >&2; exit 1; }
cargo build --release --target x86_64-pc-windows-gnullvm -p bwr-app
rm -rf dist/pkg && mkdir -p dist/pkg
cp target/x86_64-pc-windows-gnullvm/release/bwr-app.exe dist/pkg/BigWaveRadio.exe
cp "$LLVM_MINGW/x86_64-w64-mingw32/bin/libunwind.dll" dist/pkg/
sed "1s/.*/BigWave Radio $VER - Windows x64/" dist/README.txt > dist/pkg/README.txt
cp LICENSE dist/pkg/LICENSE.txt
python3 scripts/third_party_licenses.py x86_64-pc-windows-gnullvm > dist/pkg/THIRD_PARTY_LICENSES.txt
# Windows line endings for Notepad users
for f in dist/pkg/*.txt; do sed -i 's/\r*$/\r/' "$f"; done
rm -f "dist/BigWaveRadio-$VER-windows-x64.zip"
(cd dist/pkg && zip -9 -q "../BigWaveRadio-$VER-windows-x64.zip" BigWaveRadio.exe libunwind.dll README.txt LICENSE.txt THIRD_PARTY_LICENSES.txt)
echo "dist/BigWaveRadio-$VER-windows-x64.zip"
if command -v makensis >/dev/null; then
  # NSIS VIProductVersion needs four numbers (0.9.5 -> 0.9.5.0)
  VI=$(echo "$VER" | awk -F. '{printf "%d.%d.%d.%d", $1, $2, $3, $4}')
  (cd installer && makensis -V2 -DVERSION="$VER" -DVIVERSION="$VI" -DPKGDIR=../dist/pkg \
    -DOUTFILE="../dist/BigWaveRadio-$VER-setup-x64.exe" bigwave.nsi)
  echo "dist/BigWaveRadio-$VER-setup-x64.exe"
fi
