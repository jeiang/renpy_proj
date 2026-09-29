#!/bin/zsh
# Question A experiment: run the hookprobe game (copy under corpus/) on 8.5.3 for 45 s, print hook_log.txt.
# Usage: run_hookprobe.sh [lint|run]   Holds the machine run lock for the run; outside nix develop.
set -u
here=${0:A:h}; R=${here:h:h}; source $here/sweep.sh
SDK=/Users/aidanp/Projects/renpy_proj/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk
MODE=${1:-run}
snap() { find "$HOME/Library/RenPy" -type f -exec /bin/ls -lT {} + 2>/dev/null | sort | shasum; }
b=$(snap)
D=$R/corpus/hookprobe; rm -rf $D; mkdir -p $D; cp -R $here/hookprobe/game $D/game
S=$(mktemp -d); export RENPY_PATH_TO_SAVES=$S
until mkdir /tmp/renpy_proj.run.lock 2>/dev/null; do sleep 5; done
if [ $MODE = lint ]; then
  $SDK/renpy.sh $D lint > $D/probe.out 2>&1; echo "lint rc=$?"
else
  $SDK/renpy.sh $D > $D/probe.out 2>&1 & pid=$!; sleep 45
  kill -0 $pid 2>/dev/null && echo "alive after 45s" || echo "exited"
fi
sweep; rmdir /tmp/renpy_proj.run.lock
echo "--- hook_log.txt"; cat $D/hook_log.txt 2>/dev/null
echo "--- traceback.txt"; grep -v '^$' $D/traceback.txt 2>/dev/null | head -20
echo "--- errors.txt"; head -20 $D/errors.txt 2>/dev/null
[ "$b" = "$(snap)" ] && echo "~/Library/RenPy unchanged" || echo "WARNING ~/Library/RenPy changed"
