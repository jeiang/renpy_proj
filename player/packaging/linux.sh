#!/usr/bin/env bash
# Linux x86_64 package: release build, shared-library bundling, rpath check, tarball.
#
#   player/packaging/linux.sh [--no-build]     (run inside `nix develop .#player`)
#
# Output (gitignored): player/build-out/linux/player-linux-x86_64/{player,lib/} and player-linux-x86_64.tar.gz
#   - The binary is glibc-dynamic with a static libpython. Its interpreter is the generic
#     /lib64/ld-linux-x86-64.so.2 and its rpath is $ORIGIN/lib.
#   - lib/ holds the LGPL FFmpeg shared libraries, libva (the VA-API driver, for example radeonsi_drv_video.so, comes from the host: set LIBVA_DRIVERS_PATH where the host path differs from the one libva was built with) and the libraries they need that the system does not
#     provide (every rpath is $ORIGIN).
#   - Left to the host: glibc, libgcc_s, the Vulkan loader and Mesa, the audio stack (ALSA, PipeWire),
#     libudev, and the window-system libraries (opened with dlopen at run time).
#   - The check at the end fails when ldd names any other library outside the package.
# Note: the binary needs the glibc that the nix toolchain linked against (symbol versions). Build in an
# old-glibc image to run on older distributions.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
PLAYER="$(cd "$HERE/.." && pwd)"
[ "$(uname -s)-$(uname -m)" = Linux-x86_64 ] || { echo "linux.sh: Linux x86_64 only" >&2; exit 1; }
command -v patchelf >/dev/null || { echo "linux.sh: patchelf missing (use 'nix develop .#player')" >&2; exit 1; }
[ "${1:-}" = --no-build ] || (cd "$PLAYER" && cargo build --release -p player)

NAME=player-linux-x86_64
OUT="$PLAYER/build-out/linux"
PKG="$OUT/$NAME"
rm -rf "$PKG"; mkdir -p "$PKG/lib"
cp "$PLAYER/target/release/player" "$PKG/player"
chmod u+w "$PKG/player"

# Libraries the host provides. Everything else that ldd resolves is bundled.
SYSTEM='^(linux-vdso|ld-linux.*|libc|libm|libdl|libpthread|librt|libutil|libresolv|libgcc_s|libasound|libudev|libvulkan|libwayland-.*|libxkbcommon.*|libX.*|libxcb.*|libpipewire.*|libdrm.*|libGL.*|libEGL.*)\.so'
bundle() { # copy the non-system libraries of $1 (recursively) into lib/
  ldd "$1" | awk '/=> \//{print $1, $3}' | while read -r soname path; do
    soname=${soname##*/}
    if [[ "$soname" =~ $SYSTEM ]]; then continue; fi
    if [ ! -e "$PKG/lib/$soname" ]; then
      cp -L "$path" "$PKG/lib/$soname"; chmod u+wx "$PKG/lib/$soname"
      bundle "$PKG/lib/$soname"
    fi
  done
}
bundle "$PKG/player"
for so in "$PKG"/lib/*.so*; do patchelf --set-rpath '$ORIGIN' "$so"; done
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 --set-rpath '$ORIGIN/lib' "$PKG/player"
strip --strip-unneeded "$PKG/player" "$PKG"/lib/*.so* 2>/dev/null || true

# Check: ldd of the packaged binary. Outside NixOS the host's own library path applies. On NixOS the
# generic interpreter is nix-ld, which reads NIX_LD_LIBRARY_PATH (the devshell sets LD_LIBRARY_PATH).
echo "== ldd =="
LDD=$(NIX_LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}" ldd "$PKG/player")
echo "$LDD"
bad=$(echo "$LDD" | awk '/=> \//{print $3}' | grep -v "^$PKG/lib/" | grep -Ev '/(libc|libm|libdl|libpthread|librt|libutil|libresolv|libgcc_s|libasound|libudev|libvulkan|libwayland-[a-z-]*|libxkbcommon[a-z-]*|libX[a-z0-9]*|libxcb[a-z-]*)\.so' || true)
if [ -n "$bad" ]; then echo "linux.sh: unexpected dependencies:" >&2; echo "$bad" >&2; exit 1; fi
(cd "$OUT" && tar -czf "$NAME.tar.gz" "$NAME")
echo "linux.sh: $OUT/$NAME.tar.gz"
