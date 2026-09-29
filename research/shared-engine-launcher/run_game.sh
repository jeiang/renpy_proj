#!/bin/sh
# usage: run_game.sh <sdk-dir(abs)> <scratch-project> <seconds> [renpy args...]
# Runs SDK on scratch project; reports whether the process is still alive after <seconds>, then kills it. No screenshots.
SDK=$1; P=$2; T=$3; shift 3
"$SDK/renpy.sh" "$P" "$@" > "$P.stdout.log" 2>&1 &
PID=$!
sleep "$T"
if kill -0 $PID 2>/dev/null; then echo "ALIVE after ${T}s"; else wait $PID; echo "EXITED rc=$?"; fi
pkill -P $PID 2>/dev/null; kill $PID 2>/dev/null; sleep 1
pkill -f "$SDK/lib" 2>/dev/null
exit 0
