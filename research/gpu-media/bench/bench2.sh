#!/bin/sh
b() { label="$1"; shift; printf '%-52s' "$label"; ffmpeg -hide_banner -benchmark -nostats -loglevel info "$@" -f null - 2>&1 | grep -E '^bench: (utime|rtime)' | tr '\n' ' '; echo; }
F=scale=flags=neighbor+full_chroma_inp+full_chroma_int,format=rgba
b "AV1 libaom (Ren'Py's decoder) sw"          -c:v libaom-av1 -i av1_4k.mkv
b "AV1 libdav1d sw"                            -c:v libdav1d -i av1_4k.mkv
b "AV1 libaom + 1-thread sws rgba"             -c:v libaom-av1 -filter_threads 1 -i av1_4k.mkv -vf $F
b "VP9 sw decode, 1 thread"                    -threads 1 -i vp9_4k.webm
b "VP9 sw decode 1 thread + 1-thread sws rgba" -threads 1 -filter_threads 1 -i vp9_4k.webm -vf $F
b "sws-only cost: VT nv12 download + 1-thread sws" -hwaccel videotoolbox -filter_threads 1 -i vp9_4k.webm -vf $F
