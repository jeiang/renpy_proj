#!/usr/bin/env bash
# Turns the manylinux package directory into urp-linux-x86_64.AppImage.
#
#   player/packaging/appimage.sh [PACKAGE_DIR] [OUT_FILE]
#
# PACKAGE_DIR defaults to player/build-out/manylinux/urp-linux-x86_64 (output of manylinux/build.sh: `urp`, `lib/`,
# `licenses/`). OUT_FILE defaults to player/build-out/manylinux/urp-linux-x86_64.AppImage.
#
# AppDir layout (the binary keeps its RUNPATH $ORIGIN/lib, so bin/ and lib/ stay side by side):
#   AppRun                                  execs usr/bin/urp
#   urp.desktop, urp.png, .DirIcon          Name=Universal Ren'Py Player, Exec=urp, Categories=Game;
#   usr/bin/urp, usr/bin/lib/*.so*          the package
#   usr/share/licenses/urp/                 licence notices, UnRAR included
#
# appimagetool is pinned by release tag and SHA-256 (APPIMAGETOOL_BIN overrides it, e.g. nixpkgs `appimagetool`).
# Its type2 runtime is the static one that the tool embeds: the AppImage needs no libfuse2. Hosts without FUSE at
# all run it with `./urp-linux-x86_64.AppImage --appimage-extract-and-run [args]`.
# Needs python3 (draws the icon; stdlib only), curl, sha256sum.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
player_dir="$(cd "$here/.." && pwd)"
pkg="$(cd "${1:-$player_dir/build-out/manylinux/urp-linux-x86_64}" && pwd)"
out="${2:-$player_dir/build-out/manylinux/urp-linux-x86_64.AppImage}"
[ "$(uname -m)" = x86_64 ] || { echo "appimage.sh: x86_64 only" >&2; exit 1; }
[ -x "$pkg/urp" ] && [ -d "$pkg/lib" ] && [ -f "$pkg/licenses/UnRAR-LICENSE.txt" ] \
    || { echo "appimage.sh: $pkg is not a urp package (need urp, lib/, licenses/UnRAR-LICENSE.txt)" >&2; exit 1; }

TOOL_TAG=1.9.1
TOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
tool="${APPIMAGETOOL_BIN:-}"
run_tool=("$tool")
if [ -z "$tool" ]; then
    cache="${XDG_CACHE_HOME:-$HOME/.cache}/urp-appimage"
    tool="$cache/appimagetool-$TOOL_TAG-x86_64.AppImage"
    if [ ! -x "$tool" ]; then
        mkdir -p "$cache"
        curl -fsSL -o "$tool.tmp" "https://github.com/AppImage/appimagetool/releases/download/$TOOL_TAG/appimagetool-x86_64.AppImage"
        echo "$TOOL_SHA256  $tool.tmp" | sha256sum -c - >/dev/null || { echo "appimage.sh: appimagetool checksum mismatch" >&2; rm -f "$tool.tmp"; exit 1; }
        chmod +x "$tool.tmp"; mv "$tool.tmp" "$tool"
    fi
    # The pinned tool is itself an AppImage: extract-and-run keeps it independent of FUSE (CI runners have none).
    run_tool=("$tool" --appimage-extract-and-run)
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
app="$work/urp.AppDir"
mkdir -p "$app/usr/bin" "$app/usr/share/licenses"
cp -a "$pkg/urp" "$pkg/lib" "$app/usr/bin/"
cp -a "$pkg/licenses" "$app/usr/share/licenses/urp"

cat >"$app/AppRun" <<'RUN'
#!/bin/sh
# The binary finds its FFmpeg libraries through RUNPATH $ORIGIN/lib: exec it by its real path.
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/urp" "$@"
RUN
chmod 755 "$app/AppRun"

cat >"$app/urp.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=Universal Ren'Py Player
Comment=Play and manage Ren'Py games
Exec=urp
Icon=urp
Categories=Game;
Terminal=false
DESKTOP

# A 256x256 icon drawn here: dark rounded square with a light play triangle. Own artwork, no third-party art.
python3 - "$app/urp.png" <<'PY'
import struct, sys, zlib
n = 256
def inside_round(x, y, r=48):
    cx = min(max(x, r), n - 1 - r); cy = min(max(y, r), n - 1 - r)
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r
def in_tri(x, y):  # triangle (88,64) (88,192) (200,128)
    if x < 88 or x > 200: return False
    half = (200 - x) * 64 / 112
    return abs(y - 128) <= half
rows = bytearray()
for y in range(n):
    rows.append(0)
    for x in range(n):
        if not inside_round(x, y): px = (0, 0, 0, 0)
        elif in_tri(x, y): px = (245, 245, 250, 255)
        else: px = (45, 55, 95, 255)
        rows += bytes(px)
def chunk(t, d):
    c = struct.pack(">I", len(d)) + t + d
    return c + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0)) \
    + chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
PY
ln -s urp.png "$app/.DirIcon"

mkdir -p "$(dirname "$out")"
rm -f "$out"
ARCH=x86_64 "${run_tool[@]}" --no-appstream "$app" "$out"
chmod +x "$out"
echo "appimage.sh: $out ($(du -h "$out" | cut -f1))"
