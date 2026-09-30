#!/bin/sh
# Probe: static CPython 3.12.8 (release, no PGO) on macOS arm64, builtin extension modules (ticket #26).
# Needs upstream/src/Python-3.12.8 (see fetch.sh). Uses Apple clang from /usr/bin, not the nix cc wrapper.
set -e; cd "$(dirname "$0")"; ROOT=$PWD
export PATH=/usr/bin:/bin:/usr/sbin:/sbin
export MACOSX_DEPLOYMENT_TARGET=11.0
B=$ROOT/upstream/build; mkdir -p $B; cd $B
cat > $ROOT/upstream/src/Python-3.12.8/Modules/Setup.local <<'SETUP'
*static*
_asyncio _asynciomodule.c
_bisect _bisectmodule.c
_contextvars _contextvarsmodule.c
_csv _csv.c
_datetime _datetimemodule.c
_heapq _heapqmodule.c
_json _json.c
_opcode _opcode.c
_pickle _pickle.c
_queue _queuemodule.c
_random _randommodule.c
_statistics _statisticsmodule.c
_struct _struct.c
_zoneinfo _zoneinfo.c
array arraymodule.c
binascii binascii.c
cmath cmathmodule.c
math mathmodule.c
mmap mmapmodule.c
select selectmodule.c
unicodedata unicodedata.c
zlib zlibmodule.c -lz
_socket socketmodule.c
fcntl fcntlmodule.c
grp grpmodule.c
pwd pwdmodule.c
resource resource.c
termios termios.c
_posixsubprocess _posixsubprocess.c
SETUP
$ROOT/upstream/src/Python-3.12.8/configure --prefix=$B/install --disable-shared --without-ensurepip \
   --disable-test-modules ac_cv_func_dup3=no ac_cv_func_pipe2=no >configure.log 2>&1
# out-of-tree builds read Modules/Setup.local from the BUILD dir
cp $ROOT/upstream/src/Python-3.12.8/Modules/Setup.local $B/Modules/Setup.local
touch $B/Modules/Setup.local; make Makefile >makefile.log 2>&1  # regenerate Modules/config.c with the static list
make -j8 >make.log 2>&1
make install >install.log 2>&1
ls -la $B/install/lib/libpython3.12.a
