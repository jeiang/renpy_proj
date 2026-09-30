#!/bin/zsh
# Ground truth for the detector: real 8.5.3 unpickle + rollback walk inside a cloned game (probe/truth_load.rpy),
# plus the namemap dump (probe/dump_namemap.rpy). Usage: run_truth.sh <game clone dir> <save dir copy> <out dir> [hide,names]
# Clone the game first with /bin/cp -Rc into corpus/ (gitignored); saves come from scratch/ (a copy, never ~/Library/RenPy).
set -u
here=${0:A:h}; G=$1; S=$2; R=$3; HIDE=${4:-}
SDK=${SDK:?set SDK to the renpy-8.5.3-sdk dir}
snap() { find "$HOME/Library/RenPy" -type f -exec /bin/ls -lT {} + 2>/dev/null | shasum; }
b=$(snap); mkdir -p $R
cp $here/probe/dump_namemap.rpy $G/game/zz_dump_namemap.rpy; cp $here/probe/truth_load.rpy $G/game/zz_truth_load.rpy
until mkdir /tmp/renpy_proj.run.lock 2>/dev/null; do sleep 5; done
SAVESCAN_HIDE=$HIDE SAVESCAN_LIB=$here SAVESCAN_DIR=$S RENPY_PATH_TO_SAVES=$R timeout 240 $SDK/renpy.sh $G lint > $R/lint.out 2>&1
echo rc=$?
pkill -9 -f "$G"; rmdir /tmp/renpy_proj.run.lock; pgrep -f "$G" >/dev/null && echo "SWEEP FAILED"
[ "$b" = "$(snap)" ] && echo "~/Library/RenPy unchanged" || echo "WARNING ~/Library/RenPy changed"
