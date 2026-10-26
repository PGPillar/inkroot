#!/usr/bin/env bash
# Builds libwdi's wdi-simple.exe (the WinUSB driver installer Inkroot bundles on Windows)
# from the commit pinned in tools/binaries.lock.json, using libwdi's own MinGW recipe
# reduced to WinUSB only (no libusb0 / libusbK).
#
# Run from an MSYS2 MINGW64 shell with node on PATH:
#   bash tools/build-wdi.sh
# Needs: mingw-w64-x86_64-toolchain base-devel autotools git (as libwdi's CI installs).
# Output: the lock's libwdi.build path (tools/.build/windows-x86_64/wdi-simple.exe).
set -euo pipefail

die() { echo "build-wdi: $*" >&2; exit 1; }

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOCK="$REPO/tools/binaries.lock.json"
SRC="$REPO/tools/.build/src/libwdi"
MSI="$REPO/tools/.build/wdk-redist.msi"

[ "${MSYSTEM:-}" = "MINGW64" ] || die "run this from an MSYS2 MINGW64 shell (MSYSTEM=${MSYSTEM:-unset})"
command -v node >/dev/null || die "node is required to read $LOCK"

lock() { node -e 'const l=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")); const v=process.argv[2].split(".").reduce((o,k)=>o[k],l); if(typeof v!=="string") process.exit(1); process.stdout.write(v)' "$LOCK" "$1"; }
REPO_URL="https://github.com/pbatard/libwdi"
COMMIT="$(lock libwdi.commit)"
WDK_URL="$(lock libwdi.wdkRedist.url)"
WDK_SHA="$(lock libwdi.wdkRedist.sha256)"
OUT="$REPO/$(lock libwdi.build)"

# 1. WDK redistributable (WDF coinstaller), hash-verified against the lock
mkdir -p "$(dirname "$MSI")"
echo "downloading $WDK_URL"
curl -fsSL --retry 3 -o "$MSI" "$WDK_URL"
ACTUAL="$(sha256sum "$MSI" | cut -d' ' -f1)"
if [[ "$WDK_SHA" == FILL_IN* ]]; then
  die "libwdi.wdkRedist.sha256 is not pinned yet. Downloaded hash: $ACTUAL (verify it, write it into $LOCK and commit)"
fi
[ "$ACTUAL" = "$WDK_SHA" ] || die "WDK redistributable hash mismatch: expected $WDK_SHA, actual $ACTUAL"
echo "WDK redistributable OK sha256=$ACTUAL"

# 2. libwdi at the pinned commit, clean tree
if [ ! -d "$SRC/.git" ]; then
  mkdir -p "$(dirname "$SRC")"
  git clone "$REPO_URL" "$SRC"
elif ! git -C "$SRC" cat-file -e "$COMMIT^{commit}" 2>/dev/null; then
  git -C "$SRC" fetch origin "$COMMIT"
fi
git -C "$SRC" checkout --force --detach "$COMMIT"
git -C "$SRC" clean -ffdxq
HEAD="$(git -C "$SRC" rev-parse HEAD)"
[ "$HEAD" = "$COMMIT" ] || die "libwdi checkout is at $HEAD, expected $COMMIT"

# 3. Extract the MSI into <checkout>/wdk (libwdi embeds ../wdk/... relative to its libwdi/ dir)
MSYS2_ARG_CONV_EXCL='*' msiexec /a "$(cygpath -w "$MSI")" /qn TARGETDIR="$(cygpath -w "$SRC/wdk")"
WDK_DIR="wdk/Windows Kits/8.0"
[ -f "$SRC/$WDK_DIR/redist/wdf/x64/WdfCoInstaller01011.dll" ] \
  || die "WDK extraction did not produce $WDK_DIR/redist/wdf/x64/WdfCoInstaller01011.dll"

# 4. Build (WinUSB only, 64-bit only, static)
cd "$SRC"
./bootstrap.sh
./configure --build=x86_64-w64-mingw32 --host=x86_64-w64-mingw32 \
  --enable-examples-build --disable-debug --disable-shared --disable-32bit \
  --with-wdkdir="$WDK_DIR" --with-wdfver=1011
make

# 5. Copy to the build path
[ -f examples/wdi-simple.exe ] || die "make did not produce examples/wdi-simple.exe"
mkdir -p "$(dirname "$OUT")"
cp examples/wdi-simple.exe "$OUT"

# 6. Smoke test (captured first so a closed pipe can't fail the run under pipefail)
HELP="$("$OUT" --help 2>&1)" || die "wdi-simple.exe --help exited non-zero"
grep -qi usage <<<"$HELP" || die "wdi-simple.exe --help printed no usage text"
echo "wdi-simple.exe OK sha256=$(sha256sum "$OUT" | cut -d' ' -f1)"
