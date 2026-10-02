#!/bin/bash
# M6 campaign on artemis: phase 1 deep runs (parallel workers, gamemoderun stops llm-server), phase 2 `player upgrade` (llm-server must be up).
#   harness/tools/m6_campaign.sh deep    [workers]     # all 23 games, 3 seeds x 30 min each
#   harness/tools/m6_campaign.sh upgrade               # every game with patchable python2 errors, one after another
# Env: PLAYER_BIN (absolute), OUT (default harness/out/m6), DATA (default harness/out/m6-data).
set -u
HERE=$(cd "$(dirname "$0")/.." && pwd)
PLAYER_BIN=${PLAYER_BIN:?set PLAYER_BIN to the absolute path of the player binary}
OUT=${OUT:-$HERE/out/m6}
DATA=${DATA:-$HERE/out/m6-data}
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}
SHELL_NIX=(nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg nixpkgs#sway nixpkgs#xwayland -c)
case "${1:-}" in
deep)
  "${SHELL_NIX[@]}" python3 "$HERE/deep_all.py" --player-bin "$PLAYER_BIN" --out "$OUT" --workers "${2:-6}" --stage-scale 3
  ;;
upgrade)
  export PLAYER_UPGRADE_BASE_URL=${PLAYER_UPGRADE_BASE_URL:-http://127.0.0.1:8080/v1}
  export PLAYER_UPGRADE_MODEL=${PLAYER_UPGRADE_MODEL:-Qwen3.6-35B-A3B-MTP-UD-Q4_K_XL.gguf}
  export PLAYER_HARNESS_DIR=$HERE
  export PLAYER_PYTHON=python3
  for d in "$OUT"/*/errors; do
    [ -d "$d" ] || continue
    g=$(basename "$(dirname "$d")")
    echo "[upgrade] $g $(date +%H:%M:%S)"
    "${SHELL_NIX[@]}" "$PLAYER_BIN" upgrade "$g" --errors "$d" --data "$DATA"
    echo "[upgrade] $g rc $?"
  done
  ;;
*) sed -n 2,6p "$0"; exit 2;;
esac
