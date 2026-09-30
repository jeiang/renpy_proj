#!/bin/sh
# Static CPython 3.12.8 for the player (macOS arm64). Ported from research/pypack/build_cpython_static.sh.
# Usage: build.sh <player-dir>
# The pyo3 config is the committed crates/pyhost/pyo3-config.txt (see there); this script does not write it.
#   source  -> <player>/upstream/src/Python-3.12.8        (fetched here)
#   scratch -> <player>/upstream/cpython-build
#   output  -> <player>/build-out/cpython/{lib,deps,boot,stdlib.zip,pyo3-config.txt,stamp}
# Static third-party libraries come from nix (nixpkgs#pkgsStatic.<pkg>).
# The five faults from research/pypack/README.md section 2 are handled at the marked FAULT lines.
set -eu
HERE=$(cd "$(dirname "$0")" && pwd)
PLAYER=$(cd "$1" && pwd)
PYVER=3.12.8
PYURL=https://www.python.org/ftp/python/$PYVER/Python-$PYVER.tar.xz
# SHA-256 of Python-3.12.8.tar.xz as published by python.org.
PYSHA=c909157bb25ec114e5869124cc2a9c4a4d4c1e957ca4ff553f1edc692101154e
UP=$PLAYER/upstream
SRC=$UP/src/Python-$PYVER
# PYHOST_ARCH=x86_64 builds the Intel slice for a universal binary (packaging/universal.sh): same recipe,
# `clang -arch x86_64` (the configure tests run through Rosetta), x86_64 static libraries from the
# nixpkgs-26.05-darwin pin (the main pin dropped x86_64-darwin), and a separate build and output folder.
ARCH=${PYHOST_ARCH:-arm64}
case "$ARCH" in
  arm64) B=$UP/cpython-build; OUT=$PLAYER/build-out/cpython; NIXPKGS="nixpkgs#pkgsStatic"; TBD_TARGET=arm64-macos; ARCHFLAGS="" ;;
  x86_64) B=$UP/cpython-build-x86_64; OUT=$PLAYER/build-out/cpython-x86_64
          NIXPKGS="github:NixOS/nixpkgs/nixpkgs-26.05-darwin#legacyPackages.x86_64-darwin.pkgsStatic"
          TBD_TARGET=x86_64-macos; ARCHFLAGS="-arch x86_64" ;;
  *) echo "pyhost: PYHOST_ARCH must be arm64 or x86_64" >&2; exit 1 ;;
esac

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) ;;
  Darwin-x86_64) [ "$ARCH" = x86_64 ] || { echo "pyhost: PYHOST_ARCH=arm64 needs an Apple Silicon host" >&2; exit 1; } ;;
  *) echo "pyhost: the static CPython build supports macOS arm64 only (Linux and Windows are M4)" >&2; exit 1 ;;
esac
command -v nix >/dev/null || { echo "pyhost: nix is required for the static libffi, bzip2, xz, expat, zlib and openssl (run inside 'nix develop .#player')" >&2; exit 1; }

# ---- static dependencies from nix (resolved before the environment is cleaned) ----
nixp() { nix build --no-link --print-out-paths "$NIXPKGS.$1" 2>/dev/null | head -1; }
FFI=$(nixp libffi.out); FFI_DEV=$(nixp libffi.dev)
BZ=$(nixp bzip2.out); BZ_DEV=$(nixp bzip2.dev)
XZ=$(nixp xz.out); XZ_DEV=$(nixp xz.dev)
EXP=$(nixp expat.out); EXP_DEV=$(nixp expat.dev)
ZL=$(nixp zlib.out); ZL_DEV=$(nixp zlib.dev)
SSL=$(nixp openssl.out); SSL_DEV=$(nixp openssl.dev)
for v in FFI FFI_DEV BZ BZ_DEV XZ XZ_DEV EXP EXP_DEV ZL ZL_DEV SSL SSL_DEV; do
  eval "p=\${$v}"; [ -n "$p" ] && [ -d "$p" ] || { echo "pyhost: nix build of pkgsStatic dependency $v failed" >&2; exit 1; }
done
PYTHON_HOME_TMP=${TMPDIR:-/tmp}

# ---- clean environment: Apple clang, not the nix cc wrapper ----
run() { env -i HOME="$HOME" TMPDIR="${TMPDIR:-/tmp}" PATH=/usr/bin:/bin:/usr/sbin:/sbin \
  MACOSX_DEPLOYMENT_TARGET=11.0 ${ARCHFLAGS:+CC="/usr/bin/clang $ARCHFLAGS" CXX="/usr/bin/clang++ $ARCHFLAGS" LDFLAGS="$ARCHFLAGS"} "$@"; }

# ---- fetch ----
mkdir -p "$UP/src" "$B" "$OUT"
if [ ! -d "$SRC" ]; then
  echo "pyhost: fetching CPython $PYVER"
  run curl -fsSL -o "$UP/Python-$PYVER.tar.xz" "$PYURL"
  got=$(run shasum -a 256 "$UP/Python-$PYVER.tar.xz" | cut -d' ' -f1)
  [ "$got" = "$PYSHA" ] || { echo "pyhost: checksum mismatch for Python-$PYVER.tar.xz: $got" >&2; rm -f "$UP/Python-$PYVER.tar.xz"; exit 1; }
  run tar -xJf "$UP/Python-$PYVER.tar.xz" -C "$UP/src"
fi

# ---- Setup.local: every extension module builtin ----
INC="-I$FFI_DEV/include -I$BZ_DEV/include -I$XZ_DEV/include -I$EXP_DEV/include -I$ZL_DEV/include -I$SSL_DEV/include"
CORE=-DPy_BUILD_CORE_BUILTIN
cat > "$B/Setup.local.new" <<SETUP
*static*
_asyncio $CORE _asynciomodule.c
_bisect $CORE _bisectmodule.c
_blake2 $CORE _blake2/blake2module.c _blake2/blake2b_impl.c _blake2/blake2s_impl.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_bz2 $CORE $INC _bz2module.c $BZ/lib/libbz2.a
_codecs_cn $CORE cjkcodecs/_codecs_cn.c
_codecs_hk $CORE cjkcodecs/_codecs_hk.c
_codecs_iso2022 $CORE cjkcodecs/_codecs_iso2022.c
_codecs_jp $CORE cjkcodecs/_codecs_jp.c
_codecs_kr $CORE cjkcodecs/_codecs_kr.c
_codecs_tw $CORE cjkcodecs/_codecs_tw.c
_contextvars $CORE _contextvarsmodule.c
_csv $CORE _csv.c
_ctypes $CORE $INC -DUSING_MALLOC_CLOSURE_DOT_C -DUSING_APPLE_OS_LIBFFI -DHAVE_FFI_PREP_CIF_VAR -DHAVE_FFI_PREP_CLOSURE_LOC -DHAVE_FFI_CLOSURE_ALLOC _ctypes/_ctypes.c _ctypes/callbacks.c _ctypes/callproc.c _ctypes/malloc_closure.c _ctypes/stgdict.c _ctypes/cfield.c $FFI/lib/libffi.a
_datetime $CORE _datetimemodule.c
_elementtree $CORE $INC -DUSE_PYEXPAT_CAPI _elementtree.c $EXP/lib/libexpat.a
_hashlib $CORE $INC _hashopenssl.c $SSL/lib/libssl.a $SSL/lib/libcrypto.a
_heapq $CORE _heapqmodule.c
_json $CORE _json.c
_lsprof $CORE _lsprof.c rotatingtree.c
_lzma $CORE $INC _lzmamodule.c $XZ/lib/liblzma.a
_md5 $CORE -I\$(srcdir)/Modules/_hacl/include md5module.c _hacl/Hacl_Hash_MD5.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_multibytecodec $CORE cjkcodecs/multibytecodec.c
_opcode $CORE _opcode.c
_pickle $CORE _pickle.c
_posixsubprocess $CORE _posixsubprocess.c
_queue $CORE _queuemodule.c
_random $CORE _randommodule.c
_scproxy $CORE _scproxy.c -framework SystemConfiguration -framework CoreFoundation
_sha1 $CORE -I\$(srcdir)/Modules/_hacl/include sha1module.c _hacl/Hacl_Hash_SHA1.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_sha2 $CORE -I\$(srcdir)/Modules/_hacl/include sha2module.c _hacl/Hacl_Hash_SHA2.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_sha3 $CORE -I\$(srcdir)/Modules/_hacl/include sha3module.c _hacl/Hacl_Hash_SHA3.c -D_BSD_SOURCE -D_DEFAULT_SOURCE
_socket $CORE socketmodule.c
_ssl $CORE $INC _ssl.c $SSL/lib/libssl.a $SSL/lib/libcrypto.a
_statistics $CORE _statisticsmodule.c
_struct $CORE _struct.c
_typing $CORE _typingmodule.c
_zoneinfo $CORE _zoneinfo.c
array $CORE arraymodule.c
binascii $CORE binascii.c
cmath $CORE cmathmodule.c
fcntl $CORE fcntlmodule.c
grp $CORE grpmodule.c
math $CORE mathmodule.c
mmap $CORE mmapmodule.c
pwd $CORE pwdmodule.c
pyexpat $CORE $INC -DUSE_PYEXPAT_CAPI pyexpat.c $EXP/lib/libexpat.a
resource $CORE resource.c
select $CORE selectmodule.c
termios $CORE termios.c
unicodedata $CORE unicodedata.c
zlib $CORE $INC zlibmodule.c $ZL/lib/libz.a
SETUP

# ---- CPython: configure + make (rebuilt only when the recipe or Setup.local changes) ----
RECIPE=$(cat "$B/Setup.local.new" "$0" | run shasum -a 256 | cut -d' ' -f1)
if [ ! -f "$B/libpython3.12.a" ] || [ "$(cat "$B/recipe" 2>/dev/null)" != "$RECIPE" ]; then
  echo "pyhost: building CPython $PYVER (about 3 minutes)"
  rm -rf "$B/obj" "$B/install"; mkdir -p "$B/obj"; cd "$B/obj"
  cp "$B/Setup.local.new" "$SRC/Modules/Setup.local"
  # FAULT 2: on a new SDK configure detects dup3/pipe2, which fail to link against the 11.0 deployment target.
  run "$SRC/configure" --prefix="$B/install" ${ARCHFLAGS:+--build=x86_64-apple-darwin} --disable-shared --without-ensurepip --disable-test-modules \
      ac_cv_func_dup3=no ac_cv_func_pipe2=no >"$B/configure.log" 2>&1
  # FAULT 1: an out-of-tree build reads Modules/Setup.local from the build dir, and Modules/config.c is
  # regenerated only when Setup.local is newer. Without this every module stays shared.
  cp "$B/Setup.local.new" "$B/obj/Modules/Setup.local"
  touch "$B/obj/Modules/Setup.local"
  rm -f "$B/obj/Modules/config.c"
  run make Modules/config.c >"$B/makefile.log" 2>&1
  grep -q PyInit_zlib "$B/obj/Modules/config.c" || { echo "pyhost: Setup.local was not applied (modules stayed shared)" >&2; exit 1; }
  # FAULT 3: the builtin zlib needs libz. The Xcode libz.tbd is unreadable by nix's ld, so every
  # dependency is linked here as an explicit nix static archive path (in Setup.local above), and
  # pyhost/build.rs links the same archives copied to build-out/cpython/deps.
  run make -j8 >"$B/make.log" 2>&1
  run make install >"$B/install.log" 2>&1
  cp "$B/obj/libpython3.12.a" "$B/libpython3.12.a"
  echo "$RECIPE" > "$B/recipe"
fi

# ---- outputs ----
mkdir -p "$OUT/lib" "$OUT/deps" "$OUT/boot"
cp "$B/libpython3.12.a" "$OUT/lib/libpython3.12.a"
rm -f "$OUT"/deps/*.a
cp "$FFI/lib/libffi.a" "$BZ/lib/libbz2.a" "$XZ/lib/liblzma.a" "$EXP/lib/libexpat.a" "$ZL/lib/libz.a" \
   "$SSL/lib/libssl.a" "$SSL/lib/libcrypto.a" "$OUT/deps/"
chmod u+w "$OUT"/deps/*.a
PY=$B/install/bin/python3.12
LIBDIR=$B/install/lib/python3.12
run "$PY" "$HERE/mkboot.py" "$LIBDIR" "$OUT/boot"
run "$PY" "$HERE/mkzip.py" "$LIBDIR" "$OUT/stdlib.zip"

# smoke.zip: the example package (examples/smokepkg), same format as any embedded package zip
EMPTY=$B/empty-lib; mkdir -p "$EMPTY"
run "$PY" "$HERE/mkzip.py" "$EMPTY" "$OUT/smoke.zip" "$HERE/../examples/smokepkg"
# data files are stored as they are (package resources)
(cd "$HERE/../examples" && run "$PY" -c "
import zipfile,sys
z=zipfile.ZipFile(sys.argv[1],'a',zipfile.ZIP_DEFLATED); z.write('smokepkg/data.txt'); z.close()" "$OUT/smoke.zip")

# FAULT 4: the nix devshell puts its own libiconv on the link path (Rust std links -liconv on macOS), and
# hardened-runtime library validation rejects it at load. build.rs puts this directory first on the link
# search path: a stub for the system /usr/lib/libiconv.2.dylib (no exported symbols are needed; the
# Xcode libiconv.tbd cannot be read by nix's older ld).
mkdir -p "$OUT/sysdeps"
cat > "$OUT/sysdeps/libiconv.tbd" <<TBD
--- !tapi-tbd
tbd-version:     4
targets:         [ $TBD_TARGET ]
install-name:    '/usr/lib/libiconv.2.dylib'
current-version: 7
compatibility-version: 7
...
TBD
date -u +%FT%TZ > "$OUT/stamp"
echo "pyhost: CPython $PYVER ready in $OUT"
