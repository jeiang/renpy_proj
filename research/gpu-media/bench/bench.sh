#!/bin/sh
# Throughput probe: 180-frame 4K clips, max speed (no -re). fps = 180 / rtime.
run() { label="$1"; shift; printf '%-46s' "$label"; ffmpeg -hide_banner -benchmark -loglevel info -nostats "$@" -f null - 2>&1 | grep -E '^bench: (utime|rtime)|Unsupported|Failed|Error' | tr '\n' ' '; echo; }
for f in vp9_4k.webm av1_4k.mkv h264_4k.mp4; do
  echo "== $f"
  run "sw decode only"                    -i $f
  run "sw decode + rgba (SWS_POINT, Ren'Py-like)" -i $f -vf scale=flags=neighbor+full_chroma_inp+full_chroma_int,format=rgba
  run "VideoToolbox decode, frames stay on GPU" -hwaccel videotoolbox -hwaccel_output_format videotoolbox_vld -i $f
  run "VideoToolbox decode + download nv12"   -hwaccel videotoolbox -i $f
  run "VideoToolbox decode + download + rgba" -hwaccel videotoolbox -i $f -vf scale=flags=neighbor+full_chroma_inp+full_chroma_int,format=rgba
done
