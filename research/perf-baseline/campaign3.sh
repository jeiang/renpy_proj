#!/bin/zsh
cd "${0:A:h}"
P() { python3 perf.py run "$@"; }
S=$PWD/out/scenes
P astral-853 saveload sl --secs 45 --timeout 200
P si-853 stack stk16 --arg $S/si_L16.json --secs 12 --timeout 150
P astral-853 stack stk16 --arg $S/astral_L16.json --secs 12 --timeout 150
P astral-782 stack stk16 --arg $S/astral_L16.json --secs 12 --timeout 150
echo CAMPAIGN3_DONE
