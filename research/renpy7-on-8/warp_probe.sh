#!/bin/zsh
# Runtime probe: warp into the story (past `label start`) with auto-forward on, let it run N seconds, report traceback.
# WARP=start skips --warp: the probe file presses Start on the main menu after 4 s.
# Usage: warp_probe.sh CLONE_BASE_DIR SDK_DIR FILE:LINE [SECS=45]  (CLONE_BASE_DIR must be under corpus/; game/ gets zz_probe.rpy)
set -u
source ${0:A:h}/sweep.sh
B=$1; SDK=$2; WARP=$3; SECS=${4:-45}; here=${0:A:h}
snap() { find "$HOME/Library/RenPy" -type f -exec /bin/ls -lT {} + 2>/dev/null | sort; }
snap > /tmp/rp_b_probe.txt
cp $here/probe/zz_probe.rpy $B/game/; rm -f $B/{log,traceback,errors}.txt
S=$(mktemp -d); export RENPY_PATH_TO_SAVES=$S
rm -f $B/probe_progress.txt
if [ "$WARP" = start ]; then
  $SDK/renpy.sh $B > $B/probe.out 2>&1 &
else
  $SDK/renpy.sh $B run --warp $WARP > $B/probe.out 2>&1 &
fi
pid=$!
# Optional screenshot of OUR window only (SHOT=path.png): window id looked up by the engine pid(s) from this clone.
if [ -n "${SHOT:-}" ]; then
  sleep $((SECS - 6))
  pids=$(pgrep -f "py3-mac.*$B" | tr '\n' ' ')
  wid=$(/tmp/winid $=pids | head -1)
  [ -n "$wid" ] && screencapture -x -o -l$wid "$SHOT" && echo "screenshot: $SHOT (window $wid)" || echo "screenshot: no window found for pids $pids"
  sleep 6
else
  sleep $SECS
fi
 kill -0 $pid 2>/dev/null && echo "alive after ${SECS}s" || echo "exited"
sweep
[ -f $B/traceback.txt ] && { echo TRACEBACK; grep -v '^$' $B/traceback.txt | head -12; } || echo "no traceback.txt"
[ -f $B/errors.txt ] && head -8 $B/errors.txt
echo "progress: $(grep -c "^say" $B/probe_progress.txt 2>/dev/null) say lines, $(grep -c "^label" $B/probe_progress.txt 2>/dev/null) labels";  snap > /tmp/rp_a_probe.txt; cmp -s /tmp/rp_b_probe.txt /tmp/rp_a_probe.txt && echo "~/Library/RenPy unchanged" || echo "WARNING ~/Library/RenPy changed"
