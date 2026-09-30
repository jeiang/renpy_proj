#!/bin/sh
# Points at the Ren'Py 8.5.3 source tree used by the other research tickets.
# Default: the main checkout's research/version-drift/renpy-src (tag 8.5.3.26051504).
# Otherwise: git clone --depth 1 --branch 8.5.3.26051504 https://github.com/renpy/renpy upstream/renpy
set -e
cd "$(dirname "$0")"
if [ ! -d upstream/renpy ]; then
  git clone --depth 1 --branch 8.5.3.26051504 https://github.com/renpy/renpy upstream/renpy
fi
