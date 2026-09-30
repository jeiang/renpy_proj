#!/bin/sh
# Reproduce the macOS arm64 embed probe (ticket #26). Needs: ./fetch.sh done, `nix` (devshell rustc), Xcode clang.
set -e; cd "$(dirname "$0")"; ROOT=$PWD
./build_cpython_static.sh                                   # static CPython 3.12.8 -> upstream/build
B=$ROOT/upstream/build
$B/python.exe mkboot.py $B/install/lib/python3.12 upstream/boot
$B/python.exe mkzip.py $B/install/lib/python3.12 upstream/stdlib.zip probe_main.py probe_pkg
cat > upstream/pyo3.cfg <<CFG
implementation=CPython
version=3.12
shared=false
abi3=false
lib_name=python3.12
lib_dir=$B/install/lib
pointer_width=64
executable=$B/python.exe
build_flags=
suppress_build_script_link_lines=false
CFG
ZLIB=$(nix build nixpkgs#zlib.static --no-link --print-out-paths | tail -1)/lib
(cd embed-probe && ZLIB_LIB_DIR=$ZLIB PYO3_CONFIG_FILE=$ROOT/upstream/pyo3.cfg nix develop ../../.. -c cargo build --release)
cp embed-probe/target/release/embed-probe upstream/probe
# nix links libiconv from the store; repoint to the system one (what a non-nix build gets), then hardened-runtime sign ad hoc
install_name_tool -change "$(otool -L upstream/probe | awk '/nix\/store.*libiconv/{print $1}')" /usr/lib/libiconv.2.dylib upstream/probe
codesign --force -s - -o runtime upstream/probe
codesign --verify --strict --verbose=2 upstream/probe
cd /tmp && env -i PROBE_MODS="$(cat $ROOT/mods.txt)" $ROOT/upstream/probe
