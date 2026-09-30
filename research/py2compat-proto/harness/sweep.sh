#!/bin/zsh
# Other Ren'Py 7 games with the compat module only: menu, New Game, ~40 s of auto-advanced dialogue, shots.
# usage: sweep.sh   (outside nix develop). Results: evidence/<tag>.run.txt, summary in evidence/sweep.tsv
cd ${0:A:h:h}
E=$(cd ../..; pwd)/corpus/sdk/renpy.sh; G=~/Games
r() { python3 harness/run.py "$@" > /dev/null 2>&1; }
r harem $G/Harem_Hotel-v0.19-pc . $E {base} --plan harness/plans/short2.plan
r interim $G/InterimDomain-0.99.0-pc . $E {base} --plan harness/plans/short2.plan
r rift $G/AHouseInTheRift-0.8.02r3-pc . $E {base} --plan harness/plans/short2.plan
r maid $G/MaidandMaidens.app Contents/Resources/autorun $E {base} --plan harness/plans/short2.plan
r white $G/WhiteRussian.app Contents/Resources/autorun $E {base} --plan harness/plans/short2.plan
r aworld $G/A_World_Between_Us-0.2.8-pc . $E {base} --plan harness/plans/short2.plan
r alex $G/AlexsVantasticAdventure-1.0-pc . $E {base} --plan harness/plans/short2.plan
r bloom $G/BloomWar-0.19-pc . $E {base} --plan harness/plans/short2.plan
r brave $G/BraveheartAcademy-2.1-pc . $E {base} --plan harness/plans/short2.plan
r dfraction $G/DFraction-0.01-pc . $E {base} --plan harness/plans/short2.plan
r dtremake $G/DTRemake-0.4-0.4-pc . $E {base} --plan harness/plans/short2.plan
r dreamscape $G/Dreamscape-v0.2R1-pc . $E {base} --plan harness/plans/short2.plan
r cabin $G/CabinByTheLake_FantasticFacts-1.0-pc . $E {base} --plan harness/plans/short2.plan
echo done > evidence/sweep.done
