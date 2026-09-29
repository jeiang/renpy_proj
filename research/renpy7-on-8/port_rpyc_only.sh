#!/bin/zsh
# Decompile + "recompile" port of an rpyc-only Ren'Py 7 game (default: Lucky_Paradox), then lint and a timed launch on 8.5.3.
# Usage: port_rpyc_only.sh [GAME_DIRNAME_UNDER_~/Games]   (run outside nix develop; system python3 >= 3.9)
# Needs: unrpyc clone at $UNRPYC (default: main checkout's research/rpyc-loading/unrpyc, commit 3ae8334), SDK 8.5.3.
set -u
source ${0:A:h}/sweep.sh
G=${1:-Lucky_Paradox-v0.10.4-pc}
here=${0:A:h}; repo=${here:h:h}; W=$repo/corpus/port/$G
UNRPYC=${UNRPYC:-/Users/aidanp/Projects/renpy_proj/research/rpyc-loading/unrpyc}
SDK=${SDK:-/Users/aidanp/Projects/renpy_proj/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk}
snap() { find "$HOME/Library/RenPy" -type f -exec /bin/ls -lT {} + 2>/dev/null | sort; }
snap > /tmp/rp_before.txt
rm -rf $W; mkdir -p $W/rpyc $W/saves
python3 $here/rpa_extract.py $W/rpyc .rpyc,.rpymc,.rpym ~/Games/$G/game/*.rpa      # newest patch archive wins, as in the engine
python3 $UNRPYC/unrpyc.py $W/rpyc | tail -4                                     # dir mode: .rpyc -> .rpy next to it
for m in $(cd $W/rpyc && find . -name '*.rpymc'); do python3 $UNRPYC/unrpyc.py --clobber $W/rpyc/$m | tail -2; done  # modules: -> .rpym
/bin/cp -Rc ~/Games/$G $W/game-port
rm -f $W/game-port/{log,traceback,errors}.txt
python3 $here/strip_rpyc_from_rpa.py $W/game-port/game                             # else old+new scripts both load ("translation already exists")
(cd $W/rpyc && find . \( -name '*.rpy' -o -name '*.rpym' \) | while read f; do mkdir -p $W/game-port/game/${f:h}; cp $f $W/game-port/game/$f; done)
export RENPY_PATH_TO_SAVES=$W/saves
$SDK/renpy.sh $W/game-port lint > $W/lint.out 2>&1; echo "lint rc=$?"; sweep
ls $W/game-port/traceback.txt 2>/dev/null
rm -f $W/game-port/log.txt
$SDK/renpy.sh $W/game-port > $W/launch.out 2>&1 &
pid=$!; sleep 20; kill -0 $pid 2>/dev/null && echo "launch: alive after 20s" || echo "launch: exited"
sweep
ls $W/game-port/traceback.txt 2>/dev/null; tail -3 $W/game-port/log.txt
snap > /tmp/rp_after.txt; cmp -s /tmp/rp_before.txt /tmp/rp_after.txt && echo "~/Library/RenPy unchanged (file listing incl. mtimes)" || { echo "WARNING ~/Library/RenPy changed"; diff /tmp/rp_before.txt /tmp/rp_after.txt | head; }
