#!/usr/bin/env bash
# Run archive_survey.py over every .tar.zst in the Played and Unplayed folders.
# Runs on artemis under nix shell (python3 + ffmpeg-headless), two archives at a time.
# Usage (on artemis): nix shell nixpkgs#python3 nixpkgs#ffmpeg-headless -c bash ~/urp-work/Survey/bin/run_survey.sh
set -euo pipefail

BIN="$HOME/urp-work/Survey/bin"
OUT="$HOME/urp-work/Survey/out"
SCR="$HOME/urp-work/Survey/scratch"
mkdir -p "$OUT" "$SCR"

one() {
    # Own scratch dir per archive: parallel runs must not share (or delete) each other's files.
    local scr="$SCR/$(basename "$1" .tar.zst)"
    python3 "$BIN/archive_survey.py" "$1" "$OUT" "$scr" || echo "FAIL $1"
}
export -f one
export BIN OUT SCR

find "$HOME/Downloads/Played" "$HOME/Downloads/Unplayed" -maxdepth 1 -name '*.tar.zst' -print0 \
    | nice -n 10 ionice -c3 xargs -0 -P2 -I{} bash -c 'one "$1"' _ {}
echo "DONE $(ls "$OUT"/*.json | wc -l) json files"
