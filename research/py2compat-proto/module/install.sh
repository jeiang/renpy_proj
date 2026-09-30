#!/bin/zsh
# Copies the module into a scratch 8.5.3 SDK copy (engine side; no game directory is touched).
# usage: install.sh [SDK_DIR]   default: <worktree>/corpus/sdk (an APFS clone of the 8.5.3 SDK)
here=${0:A:h}; root=${here:h:h:h}
sdk=${1:-$root/corpus/sdk}
[ -d $sdk/renpy/common ] || { echo "no SDK at $sdk"; exit 1; }
cp $here/renpy/py2compat.py $sdk/renpy/py2compat.py
cp $here/renpy/common/00py2compat.rpy $sdk/renpy/common/00py2compat.rpy
rm -f $sdk/renpy/common/00py2compat.rpyc
echo "installed into $sdk"
