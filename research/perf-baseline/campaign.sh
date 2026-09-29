#!/bin/zsh
# Full measurement campaign (about 2 h). Every run takes the machine lock itself (perf.py), so the sibling can interleave.
cd "${0:A:h}"
P() { python3 perf.py run "$@"; }
R=images/Videos/anim_e7a86.webm
phase=${1:-all}
video() {
  P si-801 video vid --arg images/ch14/p1/b3/cafeteria/cafeteria43_v6.webm --fps 60 --secs 20
  P si-853 video vid --arg images/ch14/p1/b3/cafeteria/cafeteria43_v6.webm --fps 60 --secs 20
  P rip-own video vid-e3 --arg images/Gallery/extras_E3.webm --fps 60 --secs 20
  P rip-853 video vid-e3 --arg images/Gallery/extras_E3.webm --fps 60 --secs 20
  P rip-853 video vid-e7a86 --arg images/Videos/anim_e7a86.webm --fps 60 --secs 15
  P wa-823 video vid --arg images/tennis_d21helen42.ogv --fps 60 --secs 20
  P wa-853 video vid --arg images/tennis_d21helen42.ogv --fps 60 --secs 20
  P astral-782 video vid-menu --arg images/brand/main_menu.webm --fps 60 --secs 20
  P astral-853 video vid-menu --arg images/brand/main_menu.webm --fps 60 --secs 20
  P astral-782 video vid-alice --arg "images/Ev/Alice/alice nun 11.webm" --fps 60 --secs 12
  P astral-853 video vid-alice --arg "images/Ev/Alice/alice nun 11.webm" --fps 60 --secs 12
  P si-853 video vid-cut --kind cutscene --arg images/ch14/p1/b3/cafeteria/cafeteria43_v6.webm --fps 60 --secs 8
  P astral-853 video vid-menu-cut --kind cutscene --arg images/brand/main_menu.webm --fps 60 --secs 20
}
sample() {
  P si-853 video smp --arg images/ch14/p1/b3/cafeteria/cafeteria43_v6.webm --fps 60 --secs 14 --sample 8 --reps 1
  P rip-853 video smp-e3 --arg images/Gallery/extras_E3.webm --fps 60 --secs 14 --sample 8 --reps 1
  P wa-853 video smp --arg images/tennis_d21helen42.ogv --fps 60 --secs 14 --sample 8 --reps 1
  P astral-853 video smp-menu --arg images/brand/main_menu.webm --fps 60 --secs 14 --sample 8 --reps 1
  P astral-782 video smp-menu --arg images/brand/main_menu.webm --fps 60 --secs 14 --sample 8 --reps 1
}
startup() {
  for e in si-801 si-853 rip-own rip-853 wa-823 wa-853 astral-782 astral-853; do
    P $e startup cold --cold --reps 1 --timeout 400
    P $e startup warm --reps 3 --timeout 200
  done
}
saveload() {
  local S=$PWD/out/scenes
  for e in si-801 si-853 rip-own rip-853 wa-823 wa-853 astral-782 astral-853; do
    P $e saveload sl --secs 45 --timeout 200
  done
}
scenes() {
  local S=$PWD/out/scenes
  for pair in si-801:si si-853:si rip-own:rip rip-853:rip wa-823:wa wa-853:wa astral-782:astral astral-853:astral; do
    P ${pair%%:*} scenes sc --arg $S/${pair##*:}_L3.json --timeout 300
  done
  for pair in si-853:si rip-853:rip wa-853:wa astral-853:astral astral-782:astral; do
    P ${pair%%:*} stack stk --arg $S/${pair##*:}_L6.json --secs 12 --timeout 150
  done
}
case $phase in
  all) video; sample; startup; saveload; scenes;;
  *) $phase;;
esac
echo CAMPAIGN_DONE $phase
