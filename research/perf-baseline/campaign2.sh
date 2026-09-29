#!/bin/zsh
# Follow-up runs after the main campaign showed 10-bit 4K VP9 is the failing case: the game's 4:4:4 4K file, and decode hotspots for it.
cd "${0:A:h}"
P() { python3 perf.py run "$@"; }
P astral-853 video vid-444 --arg "images/Obj/PC/a pc wal Feb 2021 1.webm" --fps 60 --secs 15
P astral-782 video vid-444 --arg "images/Obj/PC/a pc wal Feb 2021 1.webm" --fps 60 --secs 15
P astral-853 video smp-alice --arg "images/Ev/Alice/alice nun 11.webm" --fps 60 --secs 10 --sample 6 --reps 1
P astral-782 video smp-alice --arg "images/Ev/Alice/alice nun 11.webm" --fps 60 --secs 10 --sample 6 --reps 1
P astral-853 video smp-444 --arg "images/Obj/PC/a pc wal Feb 2021 1.webm" --fps 60 --secs 10 --sample 6 --reps 1
P rip-853 video vid-e9a191 --arg images/Videos/anim_e9a191.webm --fps 60 --secs 20
echo CAMPAIGN2_DONE
