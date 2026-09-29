#!/bin/sh
# usage: make_scratch.sh <name>  -> scratch/<name>/game : real copies of script-ish dirs, symlinks for heavy assets
set -e
SRC=${SRC:-$HOME/Games/SecretIsland-0.18.8.0-pc}
D=$(dirname "$0")/scratch/$1
rm -rf "$D"; mkdir -p "$D/game"
for f in scripts tl cache script_version.txt; do cp -R "$SRC/game/$f" "$D/game/"; done
for f in images audio gui fonts presplash_background.png presplash_foreground.png; do ln -s "$SRC/game/$f" "$D/game/$f"; done
# optional: RELEASED=1 -> drop .rpy so only 8.0.1-compiled .rpyc remain (a "released game" layout)
if [ -n "$RELEASED" ]; then find "$D/game" -name '*.rpy' -delete; fi
