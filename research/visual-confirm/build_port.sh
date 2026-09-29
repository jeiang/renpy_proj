#!/bin/zsh
# Build a ported clone of a Ren'Py 7 game (no game process is started): same steps as renpy7-on-8/port_source.sh minus lint/launch.
# Usage: build_port.sh GAME_DIRNAME PATCHES_NAME   -> corpus/port/<PATCHES_NAME>/game-port. Run outside nix develop.
set -u
G=$1; P=$2
here=${0:A:h}; repo=${here:h:h}; o7=/Users/aidanp/Projects/renpy_proj/research/renpy7-on-8; W=$repo/corpus/port/$P
UNRPYC=/Users/aidanp/Projects/renpy_proj/research/rpyc-loading/unrpyc
rm -rf $W; mkdir -p $W/src
python3 $o7/rpa_extract.py $W/src .rpy,.rpym,.rpyc,.rpymc ~/Games/$G/game/*.rpa(N)
python3 $UNRPYC/unrpyc.py $W/src | tail -3
find $W/src \( -name '*.rpyc' -o -name '*.rpymc' \) -delete
/bin/cp -Rc ~/Games/$G $W/game-port; rm -f $W/game-port/{log,traceback,errors}.txt
python3 $o7/strip_rpyc_from_rpa.py $W/game-port/game .rpy,.rpym,.rpyc,.rpymc
cp -Rn $W/src/. $W/game-port/game/
python3 $UNRPYC/unrpyc.py $W/game-port/game | tail -3
( cd $W/game-port/game && source $o7/patches/$P.sh )
echo built $W/game-port
