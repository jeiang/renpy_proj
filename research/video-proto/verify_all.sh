#!/bin/zsh
# THROWAWAY: colour check of GPU output vs swscale. Tools: nix shell nixpkgs#(python3.withPackages numpy) + system ffmpeg (reference only).
cd "${0:A:h}"
nix shell --impure --expr 'with import (builtins.getFlake "nixpkgs") {}; python3.withPackages (p: [p.numpy])' -c zsh -c '
for p in "si1080.webm d_si1080" "alice10bit.webm d_alice10bit" "pc444.webm d_pc444" "wa_theora.ogv d_wa_theora" "av1_4k60_10bit.mkv d_av1_4k60_10bit"; do set -- ${=p}; python3 verify_colour.py scratch/$1 out/$2.ppm 100 140; done; python3 verify_colour.py scratch/awbu_h264_10bit.mp4 out/d_h10.ppm 100 125'
