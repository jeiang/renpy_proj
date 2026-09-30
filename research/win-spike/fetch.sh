#!/bin/sh
# Re-download third-party inputs into gitignored upstream/ (ticket #32). Needs gh, zstd, zip.
set -e; cd "$(dirname "$0")"; mkdir -p upstream; cd upstream
F="cpython-3.12.14+20260924-x86_64-pc-windows-msvc-pgo-full.tar.zst"
gh release download 20260924 -R astral-sh/python-build-standalone -p "$F" --skip-existing
mkdir -p pbs && zstd -dc "$F" | tar -x -C pbs
# subset shipped to the VM (C:\spike\dl\pbs.zip): objects, headers, pure-Python Lib
(cd pbs/python && zip -qr ../../pbs.zip PYTHON.json build install/include install/Lib -x 'install/Lib/test/*' -x 'install/Lib/*/test/*' -x 'install/Lib/*/tests/*' -x '*/__pycache__/*' -x 'install/Lib/idlelib/*')
# Ren'Py 8.5.3 sources (only the parts the VM build needs), from research/version-drift/renpy-src
git -C ../../version-drift/renpy-src archive --format=tar.gz -o "$PWD/renpy-src.tgz" HEAD -- renpy src scripts setup.py renpy.py
