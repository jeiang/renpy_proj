#!/bin/zsh
# THROWAWAY: the measured campaign (3 runs each, 3 s warm-up + 20 s window).
cd "${0:A:h}"
./measure.sh alice_4k60_10bit alice10bit.webm --secs 20
./measure.sh pc_4k60_444 pc444.webm --secs 20
./measure.sh si_1080p60_vp9 si1080.webm --secs 20
./measure.sh wa_1440p60_theora wa_theora.ogv --secs 20
./measure.sh awbu_1080p60_h264 awbu_h264.mp4 --secs 20
./measure.sh awbu_1080p15_h264_hi10 awbu_h264_10bit.mp4 --secs 20
./measure.sh gen_1080p60_av1 av1_1080p60.mkv --secs 20
./measure.sh gen_4k60_10bit_av1 av1_4k60_10bit.mkv --secs 20
# extras: software-only decode of the two failing clips (VideoToolbox off), for the CPU comparison
./measure.sh alice_swonly alice10bit.webm --secs 20 --hw off
./measure.sh pc_swonly pc444.webm --secs 20 --hw off
