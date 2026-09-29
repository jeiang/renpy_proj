#!/bin/zsh
# Question B: static silent-semantics census over all 17 Ren'Py 7 games in ~/Games (read-only). Outside nix develop
# except the python: nix shell nixpkgs#python312.  Writes census.json.
here=${0:A:h}; G=~/Games
$here/fetch.sh
nix shell nixpkgs#python312 -c python3 $here/census.py $here/census.json \
 $G/A_World_Between_Us-0.2.8-pc/game $G/AlexsVantasticAdventure-1.0-pc/game $G/BlackRose-Public-0.4.1-win/game \
 $G/BloomWar-0.19-pc/game $G/BraveheartAcademy-2.1-pc/game $G/CabinByTheLake_FantasticFacts-1.0-pc/game \
 $G/DFraction-0.01-pc/game $G/DTRemake-0.4-0.4-pc/game $G/Dreamscape-v0.2R1-pc/game \
 "$G/Bumpkin Boy's Bizzare Adventure/Bumpkin_Boy's_Bizarre_Adventures-0.14-pc/game" \
 $G/AHouseInTheRift-0.8.02r3-pc/game $G/AstralLust-0.3.1c.4K-pc/game $G/Harem_Hotel-v0.19-pc/game \
 $G/InterimDomain-0.99.0-pc/game $G/Lucky_Paradox-v0.10.4-pc/game \
 $G/MaidandMaidens.app/Contents/Resources/autorun/game $G/WhiteRussian.app/Contents/Resources/autorun/game
