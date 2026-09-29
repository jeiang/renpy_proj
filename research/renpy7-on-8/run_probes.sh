#!/bin/zsh
# Runtime probes of every game on 8.5.3 and the 8.x twin, 45 s each, SIGKILL sweep after each.
# Usage: run_probes.sh warp|start
#   warp : --warp past `label start` with auto-forward (fast into the story; skips start-label setup code)
#   start: normal launch, the probe file presses Start after 4 s, auto-forward on; screenshot of our window at ~39 s
# Unported originals where they load; ported clones (from run_ports.sh) otherwise. Output: out/probes-<mode>.txt,
# screenshots corpus/shots/<tag>.<engine>.png (gitignored: game art). Outside nix develop.
set -u
MODE=${1:?warp|start}; ONLY=${2:-.}   # optional 2nd arg: regex on the probe tag (appends to the output file)
here=${0:A:h}; R=${here:h:h}; P=$R/corpus/probe; mkdir -p $P $R/corpus/shots $here/out/probes
[ -x /tmp/winid ] || swiftc -O $here/winid.swift -o /tmp/winid
SD=/Users/aidanp/Projects/renpy_proj/research
declare -A SDK=( [8.0.1]=$SD/shared-engine-launcher/sdk/renpy-8.0.1-sdk [8.1.1]=$here/sdk/renpy-8.1.1-sdk [8.3.2]=$here/sdk/renpy-8.3.2-sdk [8.5.3]=$SD/shared-engine-launcher/sdk/renpy-8.5.3-sdk )
OUTF=$here/out/probes-$MODE.txt; [ $ONLY = . ] && : > $OUTF
probe() { # tag src-dir base-rel-dir engine warp-spec
  local tag=$1 src=$2 rel=$3 eng=$4 spec=$5 d=$P/$1__$4
  [[ $1 =~ $ONLY ]] || return 0
  rm -rf $d; /bin/cp -Rc $src $d; [ ${src:e} = app ] && xattr -dr com.apple.quarantine $d
  echo "== $tag on $eng ($MODE ${spec})" | tee -a $OUTF
  [ $MODE = start ] && export SHOT=$R/corpus/shots/$tag.$eng.png || unset SHOT
  $here/warp_probe.sh $d/$rel ${SDK[$eng]} $spec 45 2>&1 | tee -a $OUTF
  rm -f $here/out/probes/$tag.$eng.$MODE.*
  cp $d/$rel/traceback.txt $here/out/probes/$tag.$eng.$MODE.traceback.txt 2>/dev/null
  cp $d/$rel/errors.txt $here/out/probes/$tag.$eng.$MODE.errors.txt 2>/dev/null
  cp $d/$rel/probe_progress.txt $here/out/probes/$tag.$eng.$MODE.progress.txt 2>/dev/null
  rm -rf $d
}
G=~/Games; A=Contents/Resources/autorun; PORT=$R/corpus/port
w() { [ $MODE = warp ] && echo $1 || echo start; }
for e in 8.5.3 8.0.1; do
  probe interim $G/InterimDomain-0.99.0-pc . $e $(w script.rpy:34)
  probe maid $G/MaidandMaidens.app $A $e $(w script.rpy:300)
  probe white $G/WhiteRussian.app $A $e $(w script.rpy:60)
  probe lucky-orig $G/Lucky_Paradox-v0.10.4-pc . $e $(w script.rpy:899)
done
probe lucky-port $PORT/Lucky_Paradox-v0.10.4-pc/game-port . 8.5.3 $(w script.rpy:899)
for pair in "rift-port $PORT/rift/game-port 8.5.3 8.1.1 scripts/quests/story/0_intro/intro_start.rpy:10" \
            "harem-port $PORT/harem/game-port 8.5.3 8.0.1 script.rpy:559" \
            "astral-port $PORT/astral/game-port 8.5.3 8.3.2 events/special/prologue.rpy:40"; do
  set -- ${=pair}
  probe $1 $2 . $3 $(w $5)
  probe $1 $2 . $4 $(w $5)
done
if [ $MODE = start ]; then   # unported originals: what the user would see today
  probe rift-orig $G/AHouseInTheRift-0.8.02r3-pc . 8.5.3 start
  probe harem-orig $G/Harem_Hotel-v0.19-pc . 8.5.3 start
  probe astral-orig $G/AstralLust-0.3.1c.4K-pc . 8.5.3 start
fi
echo done >> $OUTF
