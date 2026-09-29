#!/bin/zsh
# Build four Ren'Py 8.5.3 project dirs from a scratch clone of WhiteRussian (7.4.11) for the save-load test.
#   a-src      : game as shipped (.rpy + .rpyc)
#   b-rpyc     : .rpy deleted, original .rpyc only ("released game")
#   c-portfresh: b-rpyc, decompiled with unrpyc, .rpyc deleted (what a one-time offline port produces)
#   d-portmerge: b-rpyc decompiled with unrpyc, original .rpyc kept beside the new .rpy
set -eu
W=${0:A:h:h:h}/corpus; SRC=$W/wr7.app/Contents/Resources/autorun/game
UNRPYC=/Users/aidanp/Projects/renpy_proj/research/rpyc-loading/unrpyc
for v in a-src b-rpyc c-portfresh d-portmerge; do rm -rf $W/wr8-$v; mkdir -p $W/wr8-$v; /bin/cp -Rc $SRC $W/wr8-$v/game; rm -rf $W/wr8-$v/game/cache; rm -f $W/wr8-$v/game/zz_*; done
find $W/wr8-b-rpyc/game $W/wr8-c-portfresh/game $W/wr8-d-portmerge/game -name '*.rpy' -delete
python3 $UNRPYC/unrpyc.py $W/wr8-c-portfresh/game | tail -3
find $W/wr8-c-portfresh/game -name '*.rpyc' -delete
python3 $UNRPYC/unrpyc.py $W/wr8-d-portmerge/game | tail -3
