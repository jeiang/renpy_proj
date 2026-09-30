#!/usr/bin/env bash
# Encoder probe on synthetic 1080p30 content (no game assets). Run: nix develop -c research/streaming/encode_probe.sh
# Compares macOS VideoToolbox H.264/HEVC on: static scene, static + 1 s of motion in the middle (a "transition"), and full motion.
# Only wall time and bytes are measured (10 s of content each). Output to out/ (gitignored).
set -euo pipefail
cd "$(dirname "$0")"; mkdir -p out
W=1920; H=1080
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=s=${W}x${H}:r=30" -frames:v 1 out/still.png
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=s=${W}x${H}:r=30" -t 1 -pix_fmt yuv420p -c:v ffv1 out/burst.mkv
# static 4.5 s + 1 s burst + static 4.5 s
ffmpeg -hide_banner -loglevel error -y -loop 1 -framerate 30 -t 4.5 -i out/still.png -i out/burst.mkv -loop 1 -framerate 30 -t 4.5 -i out/still.png \
  -filter_complex "[0][1][2]concat=n=3:v=1:a=0,format=yuv420p" -c:v ffv1 out/mixed.mkv
ffmpeg -hide_banner -loglevel error -y -loop 1 -framerate 30 -t 10 -i out/still.png -pix_fmt yuv420p -c:v ffv1 out/static.mkv
ffmpeg -hide_banner -loglevel error -y -f lavfi -i "testsrc2=s=${W}x${H}:r=30" -t 10 -pix_fmt yuv420p -c:v ffv1 out/motion.mkv
for c in h264_videotoolbox hevc_videotoolbox; do
  for s in static mixed motion; do
    /usr/bin/time -p ffmpeg -hide_banner -loglevel error -y -i out/$s.mkv -c:v $c -realtime 1 -prio_speed 1 -b:v 8M -g 60 -bf 0 out/${s}_$c.mp4 2> out/time.txt
    printf "%-8s %-20s %6s s wall  %9d bytes\n" "$s" "$c" "$(awk '/real/{print $2}' out/time.txt)" "$(wc -c < out/${s}_$c.mp4)"
  done
done
