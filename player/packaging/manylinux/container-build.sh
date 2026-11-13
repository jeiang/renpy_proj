#!/usr/bin/env bash
# Runs INSIDE quay.io/pypa/manylinux_2_28_x86_64 (AlmaLinux 8, glibc 2.28, gcc-toolset 14). Started by build.sh.
#   /src    the repository clone (read-write; build outputs go to gitignored player/target, player/upstream, player/build-out)
#   /cache  persistent cache: downloads, static deps, FFmpeg, rustup, cargo, pip
# Result: /src/player/build-out/manylinux/urp-linux-x86_64/{urp,lib/} and urp-linux-x86_64.tar.gz
set -euo pipefail
export LC_ALL=C
SRC=/src; PLAYER=$SRC/player; CACHE=/cache
HERE=$PLAYER/packaging/manylinux
DL=$CACHE/dl; DEPS=$CACHE/deps; FF=$CACHE/ffmpeg; WORK=$CACHE/work
OUT=$PLAYER/build-out/manylinux; NAME=urp-linux-x86_64
JOBS=$(nproc)
mkdir -p "$DL" "$DEPS" "$FF" "$WORK" "$OUT"
log() { printf '== manylinux: %s\n' "$*"; }

# Dependency recipes are separate so packaging edits do not invalidate the inputs cache.
source "$HERE/build-inputs.sh"

# ---- player ------------------------------------------------------------------------------------------------------
log "cargo build --release -p player"
export PKG_CONFIG_PATH=$FF/lib/pkgconfig:$SDL/lib/pkgconfig:/usr/lib64/pkgconfig:/usr/share/pkgconfig
export LD_LIBRARY_PATH=$FF/lib
export LIBCLANG_PATH=/usr/lib64
unset CFLAGS
# engine/build.py compiles with a bare `cc` and ignores CFLAGS. The nix gcc wrapper makes PIE objects by
# default; this image's gcc does not, and the Rust linker builds a PIE. A `cc` shim adds -fPIC. It must be the
# gcc-toolset gcc that is first on PATH, not /usr/bin/gcc (GCC 8): `c++` (the UnRAR build) is the toolset g++,
# and only the toolset driver links the libstdc++_nonshared.a its newer symbols need.
mkdir -p "$CACHE/shim"
printf '#!/bin/sh\nexec %s -fPIC "$@"\n' "$(command -v gcc)" > "$CACHE/shim/cc"; chmod +x "$CACHE/shim/cc"
export PATH=$CACHE/shim:$PATH
(cd "$PLAYER" && cargo build --release -p player)
if [ "${PLAYER_CHECKS:-0}" = 1 ]; then
  log "cargo clippy, cargo test"
  rustup component add clippy >/dev/null
  (cd "$PLAYER" && cargo clippy --workspace --all-targets --release -- -D warnings && cargo test --release --workspace)
  log "engine tests"
  URP_TESTS_REQUIRE_TREE=1 URP_HOST_TREE=$PLAYER/build-out/engine/common-rpyc/host \
    python3.12 -m unittest discover -s "$PLAYER/engine/python/tests" -v
fi

# ---- package: the steps of packaging/linux.sh, with the same host-provided set ----------------------------------------
log "package"
PKG=$OUT/$NAME
rm -rf "$PKG" "$OUT/$NAME.tar.gz"; mkdir -p "$PKG/lib"
cp "$PLAYER/target/release/urp" "$PKG/urp"; chmod u+w "$PKG/urp"
SYSTEM='^(linux-vdso|ld-linux.*|libc|libm|libdl|libpthread|librt|libutil|libresolv|libmvec|libnsl|libanl|libnss_[a-z]*|libthread_db|libBrokenLocale|libgcc_s|libstdc\+\+|libdrm.*|libasound|libudev|libvulkan|libwayland-.*|libxkbcommon.*|libX.*|libxcb.*|libpipewire.*|libdrm.*|libGL.*|libEGL.*)\.so'
needed() { objdump -p "$1" | awk '/NEEDED/{print $2}'; }   # direct dependencies only
bundle() { # copy the non-host libraries that $1 needs directly, then recurse into the copies
  local soname path
  for soname in $(needed "$1"); do
    if [[ "$soname" =~ $SYSTEM ]]; then continue; fi
    if [ ! -e "$PKG/lib/$soname" ]; then
      path=$(ldd "$1" | awk -v s="$soname" '$1==s && /=> \//{print $3}')
      [ -n "$path" ] || { echo "manylinux: cannot resolve $soname for $1" >&2; exit 1; }
      cp -L "$path" "$PKG/lib/$soname"; chmod u+wx "$PKG/lib/$soname"
      bundle "$PKG/lib/$soname"
    fi
  done
}
bundle "$PKG/urp"
for so in "$PKG"/lib/*.so*; do patchelf --set-rpath '$ORIGIN' "$so"; done
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 --set-rpath '$ORIGIN/lib' "$PKG/urp"
strip --strip-unneeded "$PKG/urp" "$PKG"/lib/*.so* 2>/dev/null || true
echo "== direct dependencies (NEEDED) =="
bad=""
for f in "$PKG/urp" "$PKG"/lib/*.so*; do
  for soname in $(needed "$f"); do
    echo "$(basename "$f"): $soname"
    if [ -e "$PKG/lib/$soname" ] || [[ "$soname" =~ $SYSTEM ]]; then continue; fi
    bad="$bad $(basename "$f"):$soname"
  done
done
[ -z "$bad" ] || { echo "manylinux: dependencies that are neither bundled nor host-provided:$bad" >&2; exit 1; }
# Licence notices (see packaging/licences/stage.sh).
bash "$PLAYER/packaging/licences/stage.sh" "$PKG/licenses"
bash "$PLAYER/packaging/check-no-nix-store.sh" "$PKG"
(cd "$OUT" && tar -czf "$NAME.tar.gz" "$NAME")
log "glibc policy check"
bash "$HERE/check-glibc.sh" "$PKG"
log "done: $OUT/$NAME.tar.gz"
