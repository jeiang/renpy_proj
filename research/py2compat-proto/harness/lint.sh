#!/bin/zsh
# Headless lint of a clone of a game with the compat module (scratch SDK). usage: lint.sh TAG SRC_GAME_DIR [ENV=VAL...]
# Holds the machine lock for the run. Output: corpus/run/TAG.lint.out
set -u
here=${0:A:h}; R=${here:h:h:h}; tag=$1; src=$2; shift 2
run=$R/corpus/run/$tag; rm -rf $run; mkdir -p $R/corpus/run; /bin/cp -Rc $src $run
rm -f $run/{log,traceback,errors}.txt
home=$R/corpus/p2c-home/$tag; rm -rf $home; mkdir -p $home
saves=$(mktemp -d)
until mkdir /tmp/renpy_proj.run.lock 2>/dev/null; do sleep 0.2; done
env RENPY_PATH_TO_SAVES=$saves PY2COMPAT_HOME=$home "$@" $R/corpus/sdk/renpy.sh $run lint > $run.lint.out 2>&1; echo "lint rc=$?"
pkill -9 -f "$run" ; sleep 1; rmdir /tmp/renpy_proj.run.lock
echo "--- p2c log"; head -30 $home/run.log 2>/dev/null
echo "--- traceback"; grep -v '^$' $run/traceback.txt 2>/dev/null | head -20
