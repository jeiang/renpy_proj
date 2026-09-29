#!/bin/sh
# Re-fetch the third-party Ren'Py sources this directory reads (gitignored: upstream/).
set -e
cd "$(dirname "$0")"; mkdir -p upstream
T=8.5.3.26051504
for f in renpy/compat/fixes.py renpy/python.py; do
  curl -fsSL "https://raw.githubusercontent.com/renpy/renpy/$T/$f" -o "upstream/$(basename $f)"
done
