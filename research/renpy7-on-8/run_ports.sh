#!/bin/zsh
# Re-run every manual/auto port and keep small artefacts in out/ports/. Outside nix develop. ~6 min.
here=${0:A:h}; W=${here:h:h}/corpus/port; O=$here/out/ports; mkdir -p $O
SDKR=/Users/aidanp/Projects/renpy_proj/research/shared-engine-launcher/sdk
keep() { # tag dir
  cp $W/$2/lint.out $O/$1.lint.out 2>/dev/null
  for f in lint.traceback.txt game-port/traceback.txt game-port/errors.txt; do [ -f $W/$2/$f ] && cp $W/$2/$f $O/$1.${f:t}; done
}
rm -f $O/*
$here/port_source.sh AHouseInTheRift-0.8.02r3-pc rift > $O/rift-8.5.3.log 2>&1; keep rift-8.5.3 rift
$here/port_source.sh Harem_Hotel-v0.19-pc harem > $O/harem-8.5.3.log 2>&1; keep harem-8.5.3 harem
$here/port_source.sh Harem_Hotel-v0.19-pc harem $SDKR/renpy-8.0.1-sdk > $O/harem-8.0.1.log 2>&1; keep harem-8.0.1 harem
$here/port_source.sh AstralLust-0.3.1c.4K-pc astral > $O/astral-8.5.3.log 2>&1; keep astral-8.5.3 astral
$here/port_rpyc_only.sh > $O/lucky-8.5.3.log 2>&1; cp $W/Lucky_Paradox-v0.10.4-pc/lint.out $O/lucky-8.5.3.lint.out
echo done > $O/DONE
