#!/bin/zsh
# Manual-patch port of a Ren'Py 7 game whose .rpy ships (loose or inside RPAs) to an 8.x engine.
# Usage: port_source.sh GAME_DIRNAME PATCHES_NAME [SDK_DIR]   e.g. port_source.sh AHouseInTheRift-0.8.02r3-pc rift
# Extracts the archived .rpy (decompiling rpyc-only files with unrpyc), strips .rpy/.rpyc from the clone's RPA copies, applies patches/<name>.sh (sed edits on the
# extracted source), then lint + 20 s launch on 8.5.3. Prints the first traceback of each. Run outside nix develop.
set -u
source ${0:A:h}/sweep.sh
G=$1; P=$2
here=${0:A:h}; repo=${here:h:h}; W=$repo/corpus/port/$P
SDK=${3:-${SDK:-/Users/aidanp/Projects/renpy_proj/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk}}
snap() { find "$HOME/Library/RenPy" -type f -exec /bin/ls -lT {} + 2>/dev/null | sort; }
snap > /tmp/rp_before_$P.txt
rm -rf $W; mkdir -p $W/src $W/saves
UNRPYC=${UNRPYC:-/Users/aidanp/Projects/renpy_proj/research/rpyc-loading/unrpyc}
python3 $here/rpa_extract.py $W/src .rpy,.rpym,.rpyc,.rpymc ~/Games/$G/game/*.rpa
python3 $UNRPYC/unrpyc.py $W/src | tail -3     # decompile archived rpyc that have no .rpy (existing .rpy are skipped)
find $W/src \( -name '*.rpyc' -o -name '*.rpymc' \) -delete
/bin/cp -Rc ~/Games/$G $W/game-port; rm -f $W/game-port/{log,traceback,errors}.txt
python3 $here/strip_rpyc_from_rpa.py $W/game-port/game .rpy,.rpym,.rpyc,.rpymc
cp -Rn $W/src/. $W/game-port/game/   # never clobber a shipped loose .rpy with a decompiled one
python3 $UNRPYC/unrpyc.py $W/game-port/game | tail -3   # loose rpyc without .rpy
( cd $W/game-port/game && source $here/patches/$P.sh )
export RENPY_PATH_TO_SAVES=$W/saves
$SDK/renpy.sh $W/game-port lint > $W/lint.out 2>&1; echo "lint rc=$?"; sweep
grep -m1 -A0 -E "Exception|Error" $W/game-port/traceback.txt 2>/dev/null; grep -c "" $W/game-port/traceback.txt 2>/dev/null
mv $W/game-port/traceback.txt $W/lint.traceback.txt 2>/dev/null
$SDK/renpy.sh $W/game-port > $W/launch.out 2>&1 &
pid=$!; sleep 20; kill -0 $pid 2>/dev/null && echo "launch: alive after 20s" || echo "launch: exited"
sweep   # SIGKILL by path; SIGTERM would only pop Ren'Py's quit-confirmation window
[ -f $W/game-port/traceback.txt ] && grep -m3 -B4 -E "^\w*(Error|Exception)" $W/game-port/traceback.txt | head -20
snap > /tmp/rp_after_$P.txt; cmp -s /tmp/rp_before_$P.txt /tmp/rp_after_$P.txt && echo "~/Library/RenPy unchanged" || echo "WARNING ~/Library/RenPy changed"
