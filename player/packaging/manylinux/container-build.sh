#!/usr/bin/env bash
# Runs INSIDE quay.io/pypa/manylinux_2_28_x86_64 (AlmaLinux 8, glibc 2.28, gcc-toolset 14). Started by build.sh.
#   /src    the repository clone (read-write; build outputs go to gitignored player/target, player/upstream, player/build-out)
#   /cache  persistent cache: downloads, static deps, FFmpeg, rustup, cargo, pip
# Result: /src/player/build-out/manylinux/player-linux-x86_64/{player,lib/} and player-linux-x86_64.tar.gz
set -euo pipefail
export LC_ALL=C
SRC=/src; PLAYER=$SRC/player; CACHE=/cache
HERE=$PLAYER/packaging/manylinux
DL=$CACHE/dl; DEPS=$CACHE/deps; FF=$CACHE/ffmpeg; WORK=$CACHE/work
OUT=$PLAYER/build-out/manylinux; NAME=player-linux-x86_64
JOBS=$(nproc)
mkdir -p "$DL" "$DEPS" "$FF" "$WORK" "$OUT"
log() { printf '== manylinux: %s\n' "$*"; }

# ---- Pinned inputs: one table. url, then SHA-256 of the tarball. ----------------------------------------
# Checked against the publisher's own checksum where one exists: openssl and dav1d (.sha256 files next to the
# tarball), zlib, bzip2 and libffi (the values in their release notes and in distro packaging). CPython's pin
# lives in crates/pyhost/cpython/build_linux.sh and is read from there.
#   name      version  sha256                                                            url
PINS='
zlib      1.3.1    9a93b2b7dfdac77ceba5a558a580e74667dd6fede4585b91eefb60f03b72df23  https://zlib.net/fossils/zlib-1.3.1.tar.gz
bzip2     1.0.8    ab5a03176ee106d3f0fa90e381da478ddae405918153cca248e682cd0c4a2269  https://sourceware.org/pub/bzip2/bzip2-1.0.8.tar.gz
xz        5.8.1    0b54f79df85912504de0b14aec7971e3f964491af1812d83447005807513cd9e  https://github.com/tukaani-project/xz/releases/download/v5.8.1/xz-5.8.1.tar.xz
expat     2.7.1    354552544b8f99012e5062f7d570ec77f14b412a3ff5c7d8d0dae62c0d217c30  https://github.com/libexpat/libexpat/releases/download/R_2_7_1/expat-2.7.1.tar.xz
libffi    3.4.6    b0dea9df23c863a7a50e825440f3ebffabd65df1497108e5d437747843895a4e  https://github.com/libffi/libffi/releases/download/v3.4.6/libffi-3.4.6.tar.gz
openssl   3.0.18   d80c34f5cf902dccf1f1b5df5ebb86d0392e37049e5d73df1b3abae72e4ffe8b  https://github.com/openssl/openssl/releases/download/openssl-3.0.18/openssl-3.0.18.tar.gz
SDL2      2.32.10  5f5993c530f084535c65a6879e9b26ad441169b3e25d789d83287040a9ca5165  https://github.com/libsdl-org/SDL/releases/download/release-2.32.10/SDL2-2.32.10.tar.gz
dav1d     1.5.1    401813f1f89fa8fd4295805aa5284d9aed9bc7fc1fdbe554af4292f64cbabe21  https://downloads.videolan.org/pub/videolan/dav1d/1.5.1/dav1d-1.5.1.tar.xz
ffmpeg    7.1.1    733984395e0dbbe5c046abda2dc49a5544e7e0e1e2366bba849222ae9e3a03b1  https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz
'
# Build tools from PyPI (exact versions) and the Rust toolchain.
PIP_TOOLS="Cython==3.2.5 meson==1.7.0 ninja==1.11.1.4 patchelf==0.17.2.4"
RUST_TOOLCHAIN=stable   # rustup, installed into /cache/rustup

pin() { awk -v n="$1" -v f="$2" '$1==n{print $f}' <<<"$PINS"; }
fetch() { # name -> extracted dir $WORK/<name>-<ver>
  local n=$1 v sha url f got
  v=$(pin "$n" 2); sha=$(pin "$n" 3); url=$(pin "$n" 4); f=$DL/$(basename "$url")
  if [ ! -f "$f" ]; then curl -fsSL -o "$f.tmp" "$url"; mv "$f.tmp" "$f"; fi
  got=$(sha256sum "$f" | cut -d' ' -f1)
  [ "$got" = "$sha" ] || { echo "manylinux: checksum mismatch for $f: $got" >&2; rm -f "$f"; exit 1; }
  rm -rf "$WORK/$n-$v"; tar -xf "$f" -C "$WORK"
  echo "$WORK/$n-$v"
}
stamped() { [ -f "$1/.built-$2" ]; }
mark() { : > "$1/.built-$2"; }

# ---- system packages and tools ----------------------------------------------------------------------------
log "dnf packages"
dnf install -y --enablerepo=powertools clang clang-devel clang-libs nasm libva-devel libdrm-devel pkgconfig perl-IPC-Cmd perl-Time-Piece systemd-devel alsa-lib-devel >/dev/null
export PATH=/opt/python/cp312-cp312/bin:$PATH
export PIP_CACHE_DIR=$CACHE/pip PIP_DISABLE_PIP_VERSION_CHECK=1
log "pip tools: $PIP_TOOLS"
pip install -q $PIP_TOOLS
python3.12 --version
if [ ! -x "$CACHE/cargo/bin/cargo" ]; then
  log "rustup"
  export RUSTUP_HOME=$CACHE/rustup CARGO_HOME=$CACHE/cargo
  curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain "$RUST_TOOLCHAIN" --no-modify-path >/dev/null
fi
export RUSTUP_HOME=$CACHE/rustup CARGO_HOME=$CACHE/cargo PATH=$CACHE/cargo/bin:$PATH
rustc --version; cargo --version

# ---- static libraries for CPython (PIC, so they link into the PIE player) ----------------------------------
export CFLAGS="-O2 -fPIC"
if ! stamped "$DEPS" static; then
  log "static deps: zlib bzip2 xz expat libffi openssl"
  d=$(fetch zlib);    (cd "$d" && ./configure --prefix="$DEPS" --static >/dev/null && make -j"$JOBS" >/dev/null && make install >/dev/null)
  d=$(fetch bzip2);   (cd "$d" && make -j"$JOBS" CFLAGS="-O2 -fPIC -D_FILE_OFFSET_BITS=64" libbz2.a >/dev/null && make install PREFIX="$DEPS" >/dev/null)
  d=$(fetch xz);      (cd "$d" && ./configure --prefix="$DEPS" --disable-shared --enable-static --disable-xz --disable-xzdec --disable-lzmadec --disable-lzmainfo --disable-scripts --disable-doc --disable-nls >/dev/null && make -j"$JOBS" >/dev/null && make install >/dev/null)
  d=$(fetch expat);   (cd "$d" && ./configure --prefix="$DEPS" --disable-shared --enable-static --without-docbook --without-examples --without-tests >/dev/null && make -j"$JOBS" >/dev/null && make install >/dev/null)
  d=$(fetch libffi);  (cd "$d" && ./configure --prefix="$DEPS" --disable-shared --enable-static --disable-docs --with-pic >/dev/null && make -j"$JOBS" >/dev/null && make install >/dev/null)
  d=$(fetch openssl); (cd "$d" && ./Configure linux-x86_64 no-shared no-tests --prefix="$DEPS" --libdir=lib -fPIC >/dev/null && make -j"$JOBS" build_libs >/dev/null && make install_dev >/dev/null)
  # libffi installs into lib or lib64 depending on the host; flatten so one path serves every library.
  for f in "$DEPS"/lib64/*.a; do [ -e "$f" ] && cp "$f" "$DEPS/lib/"; done
  mark "$DEPS" static
fi
for a in libz.a libbz2.a liblzma.a libexpat.a libffi.a libssl.a libcrypto.a; do [ -f "$DEPS/lib/$a" ] || { echo "manylinux: missing $DEPS/lib/$a" >&2; exit 1; }; done

# ---- SDL2 headers (the engine build compiles a pygame.locals constant probe against them; nothing links SDL2) ----
SDL=$CACHE/sdl2
if ! stamped "$SDL" hdrs; then
  log "SDL2 headers"
  d=$(fetch SDL2); mkdir -p "$SDL"
  (cd "$d" && ./configure --prefix="$SDL" --disable-shared >/dev/null && make install-hdrs install-data >/dev/null)
  mark "$SDL" hdrs
fi

# ---- CPython 3.12.8, static: the recipe of crates/pyhost/cpython/build_linux.sh -------------------------------
# Setup.local, the version and the checksum are read from that script, so the two cannot drift. Only the
# static libraries ($FFI etc.) point at the container's copies instead of nix store paths.
BL=$PLAYER/crates/pyhost/cpython/build_linux.sh
eval "$(grep -E '^(PYVER|PYURL|PYSHA)=' "$BL")"
CPY=$PLAYER/build-out/cpython
RECIPE=$( { cat "$BL"; echo "$PINS"; echo "$CFLAGS"; } | sha256sum | cut -d' ' -f1)
if [ "$(cat "$CPY/.manylinux-recipe" 2>/dev/null)" != "$RECIPE" ]; then
  log "CPython $PYVER"
  FFI=$DEPS FFI_DEV=$DEPS BZ=$DEPS BZ_DEV=$DEPS XZ=$DEPS XZ_DEV=$DEPS EXP=$DEPS EXP_DEV=$DEPS ZL=$DEPS ZL_DEV=$DEPS SSL=$DEPS SSL_DEV=$DEPS
  eval "$(grep -E '^(INC|CORE)=' "$BL")"
  body=$(sed -n '/^cat > "\$B\/Setup.local.new" <<SETUP$/,/^SETUP$/p' "$BL" | sed '1d;$d')
  [ -n "$body" ] || { echo "manylinux: Setup.local heredoc not found in $BL" >&2; exit 1; }
  UP=$PLAYER/upstream; B=$UP/cpython-build-manylinux; mkdir -p "$UP/src" "$B"
  PYSRC=$UP/src/Python-$PYVER
  if [ ! -d "$PYSRC" ]; then
    f=$DL/Python-$PYVER.tar.xz
    [ -f "$f" ] || curl -fsSL -o "$f" "$PYURL"
    [ "$(sha256sum "$f" | cut -d' ' -f1)" = "$PYSHA" ] || { echo "manylinux: CPython checksum mismatch" >&2; exit 1; }
    tar -xJf "$f" -C "$UP/src"
  fi
  eval "cat > \"\$B/Setup.local.new\" <<SETUP
$body
SETUP"
  rm -rf "$B/obj" "$B/install" "$CPY"; mkdir -p "$B/obj" "$CPY"; cd "$B/obj"
  cp "$B/Setup.local.new" "$PYSRC/Modules/Setup.local"
  # PKG_CONFIG_PATH is cleared: CPython must not find system libraries (all dependencies are the static ones above).
  PKG_CONFIG_PATH= "$PYSRC/configure" --prefix="$B/install" --disable-shared --without-ensurepip --disable-test-modules >"$B/configure.log" 2>&1
  cp "$B/Setup.local.new" "$B/obj/Modules/Setup.local"; touch "$B/obj/Modules/Setup.local"; rm -f "$B/obj/Modules/config.c"
  make Modules/config.c >"$B/makefile.log" 2>&1
  grep -q PyInit_zlib "$B/obj/Modules/config.c" || { echo "manylinux: Setup.local was not applied" >&2; exit 1; }
  make -j"$JOBS" >"$B/make.log" 2>&1
  make install >"$B/install.log" 2>&1
  mkdir -p "$CPY/lib" "$CPY/deps" "$CPY/boot" "$CPY/sysdeps" "$CPY/include"
  cp "$B/obj/libpython3.12.a" "$CPY/lib/"
  cp "$DEPS"/lib/{libffi,libbz2,liblzma,libexpat,libz,libssl,libcrypto}.a "$CPY/deps/"
  cp -r "$B/install/include/python3.12" "$CPY/include/"
  PY=$B/install/bin/python3.12; LIBDIR=$B/install/lib/python3.12; CP=$PLAYER/crates/pyhost/cpython
  "$PY" "$CP/mkboot.py" "$LIBDIR" "$CPY/boot"
  "$PY" "$CP/mkzip.py" "$LIBDIR" "$CPY/stdlib.zip"
  mkdir -p "$B/empty-lib"
  "$PY" "$CP/mkzip.py" "$B/empty-lib" "$CPY/smoke.zip" "$CP/../examples/smokepkg"
  (cd "$CP/../examples" && "$PY" -c "
import zipfile,sys
z=zipfile.ZipFile(sys.argv[1],'a',zipfile.ZIP_DEFLATED); z.write('smokepkg/data.txt'); z.close()" "$CPY/smoke.zip")
  date -u +%FT%TZ > "$CPY/stamp"
  echo "$RECIPE" > "$CPY/.manylinux-recipe"
  cd "$PLAYER"
fi

# ---- LGPL FFmpeg 7.1 (shared) with dav1d, no GPL/version3/nonfree components -----------------------------------
# Same selection as flake.nix ffmpegLgpl: no x264, x265, xvid, aom, svt-av1, vpx; dav1d on; VA-API on (libva is bundled).
FFREC=$(sha256sum "$HERE/container-build.sh" | cut -d' ' -f1)
if ! stamped "$FF" "$(pin ffmpeg 2)-$(pin dav1d 2)"; then
  log "dav1d $(pin dav1d 2)"
  d=$(fetch dav1d)
  (cd "$d" && meson setup build --prefix="$FF" --libdir=lib --buildtype=release -Ddefault_library=shared -Denable_tools=false -Denable_tests=false >/dev/null && ninja -C build >/dev/null && ninja -C build install >/dev/null)
  log "FFmpeg $(pin ffmpeg 2)"
  d=$(fetch ffmpeg)
  (cd "$d" && PKG_CONFIG_PATH=$FF/lib/pkgconfig ./configure --prefix="$FF" --libdir="$FF/lib" \
      --enable-shared --disable-static --enable-pic --disable-programs --disable-doc --disable-debug \
      --enable-libdav1d --enable-vaapi --enable-zlib --disable-bzlib --disable-lzma --disable-iconv \
      --disable-vulkan --disable-xlib --disable-libxcb --disable-sdl2 --disable-alsa --disable-sndio --disable-libdrm >"$WORK/ffmpeg-configure.log" 2>&1 || { tail -30 "$WORK/ffmpeg-configure.log"; exit 1; }
   # Licence check: the configuration must be plain LGPL.
   for k in GPL VERSION3 NONFREE; do
     v=$(sed -n "s/^#define CONFIG_$k \([01]\)$/\1/p" config.h); [ "$v" = 0 ] || { echo "manylinux: FFmpeg CONFIG_$k=$v (must be 0)" >&2; exit 1; }
   done
   grep -E '^#define CONFIG_(LIBDAV1D|VAAPI|LIBX264|LIBX265|LIBVPX|GPL|VERSION3|NONFREE) ' config.h
   make -j"$JOBS" >/dev/null && make install >/dev/null)
  mark "$FF" "$(pin ffmpeg 2)-$(pin dav1d 2)"
fi

# ---- player ------------------------------------------------------------------------------------------------------
log "cargo build --release -p player"
export PKG_CONFIG_PATH=$FF/lib/pkgconfig:$SDL/lib/pkgconfig:/usr/lib64/pkgconfig:/usr/share/pkgconfig
export LD_LIBRARY_PATH=$FF/lib
export LIBCLANG_PATH=/usr/lib64
unset CFLAGS
# engine/build.py compiles with a bare `cc` and ignores CFLAGS. The nix gcc wrapper makes PIE objects by
# default; this image's gcc does not, and the Rust linker builds a PIE. A `cc` shim adds -fPIC.
mkdir -p "$CACHE/shim"
printf '#!/bin/sh\nexec /usr/bin/gcc -fPIC "$@"\n' > "$CACHE/shim/cc"; chmod +x "$CACHE/shim/cc"
export PATH=$CACHE/shim:$PATH
rm -rf "$PLAYER/build-out/engine/stamp.txt"
(cd "$PLAYER" && cargo build --release -p player)
if [ "${PLAYER_CHECKS:-0}" = 1 ]; then
  log "cargo clippy, cargo test"
  rustup component add clippy >/dev/null
  (cd "$PLAYER" && cargo clippy --workspace --all-targets --release -- -D warnings && cargo test --release --workspace)
fi

# ---- package: the steps of packaging/linux.sh, with the same host-provided set ----------------------------------------
log "package"
PKG=$OUT/$NAME
rm -rf "$PKG" "$OUT/$NAME.tar.gz"; mkdir -p "$PKG/lib"
cp "$PLAYER/target/release/player" "$PKG/player"; chmod u+w "$PKG/player"
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
bundle "$PKG/player"
for so in "$PKG"/lib/*.so*; do patchelf --set-rpath '$ORIGIN' "$so"; done
patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 --set-rpath '$ORIGIN/lib' "$PKG/player"
strip --strip-unneeded "$PKG/player" "$PKG"/lib/*.so* 2>/dev/null || true
echo "== direct dependencies (NEEDED) =="
bad=""
for f in "$PKG/player" "$PKG"/lib/*.so*; do
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
