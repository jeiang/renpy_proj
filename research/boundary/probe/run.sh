#!/bin/sh
# THROWAWAY probes for ticket #21 (research/boundary). Needs: nix (repo devshell: python312, cc) and
# `nix shell nixpkgs#python312Packages.cython` (Cython 3.2.5) for probe 2.
set -e
cd "$(dirname "$0")"
PYI=$(nix develop ../../.. -c python3.12-config --includes)
LD=$(nix develop ../../.. -c python3.12-config --ldflags --embed)
HOME_=$(nix develop ../../.. -c python3.12 -c 'import sys;print(sys.prefix)')
echo "== probe 1: dotted-name inittab module shadows .py, Python subclasses it, buffer export"
nix develop ../../.. -c cc host.c -o host $PYI $LD && PYTHONHOME=$HOME_ ./host
echo "== probe 2: unmodified Cython cimport of PySurface_AsSurface against a non-Cython module via __pyx_capi__"
cd shim
nix shell nixpkgs#python312Packages.cython -c cython -3 shimpkg/consumer.pyx
nix develop ../../../.. -c cc -shared -fPIC -undefined dynamic_lookup -I. $PYI shimpkg/consumer.c -o shimpkg/consumer.cpython-312-darwin.so
nix develop ../../../.. -c cc host.c -o host -I. $PYI $LD && PYTHONHOME=$HOME_ ./host
rm -f host shimpkg/*.so shimpkg/consumer.c ../host
