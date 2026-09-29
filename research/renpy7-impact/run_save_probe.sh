#!/bin/zsh
# Usage: run_save_probe.sh MODE CLONE_BASE ENGINE_CMD SAVEDIR [SECS=40]
#   MODE save|load; ENGINE_CMD = executable that takes the base dir (or nothing for a .app) ; SAVEDIR = scratch save dir.
# Prints progress lines, traceback head, and whether ~/Library/RenPy changed. Kills every process under CLONE_BASE afterwards.
set -u
MODE=$1; B=$2; ENGINE=$3; S=$4; SECS=${5:-40}; here=${0:A:h}
snap() { find "$HOME/Library/RenPy" -type f -exec /bin/ls -lT {} + 2>/dev/null | sort; }
snap > /tmp/rp15_b.txt
G=$B/game; [ -d "$B/Contents/Resources/autorun/game" ] && G=$B/Contents/Resources/autorun/game
BASE=$(dirname $G)
cp $here/probe/save_probe.rpy $G/zz_probe.rpy; echo $MODE > $G/zz_mode.txt
rm -f $BASE/probe_progress.txt $BASE/traceback.txt $BASE/errors.txt
mkdir -p $S; export RENPY_PATH_TO_SAVES=$S PROBE_ACCEPT_TOKEN=${PROBE_ACCEPT_TOKEN:-0} PROBE_SAVE_AT=${PROBE_SAVE_AT:-8}
if [ "$MODE" = save ]; then rm -f $S/*/probe-LT1.save; fi
$=ENGINE --savedir $S > $BASE/probe.out 2>&1 &
sleep $SECS
pkill -9 -f "$B"; sleep 1; pgrep -f "$B" >/dev/null && { echo "SWEEP FAILED"; pgrep -fl "$B"; }
[ -f $BASE/traceback.txt ] && { echo TRACEBACK; grep -v '^$' $BASE/traceback.txt | head -14; } || echo "no traceback.txt"
[ -f $BASE/errors.txt ] && head -5 $BASE/errors.txt
echo "--- progress (interactions: $(grep -c '^interact' $BASE/probe_progress.txt 2>/dev/null))"; head -12 $BASE/probe_progress.txt 2>/dev/null
find $S -name '*.save' -o -name persistent | head
snap > /tmp/rp15_a.txt; cmp -s /tmp/rp15_b.txt /tmp/rp15_a.txt && echo "~/Library/RenPy unchanged" || echo "WARNING ~/Library/RenPy changed"
