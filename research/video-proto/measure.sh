#!/bin/zsh
# THROWAWAY: 3 runs per clip under the machine lock (one hold per run); appends JSON lines to out/runs.jsonl.
# Waits (inside the lock) until the 1-min load average is < 4, and records loadavg before/after each run.
# usage: ./measure.sh LABEL CLIP [extra video-proto args...]   (run inside `nix develop ./research/video-proto -c`)
set -u
cd "${0:A:h}"
label=$1; clip=$2; shift 2
mkdir -p out
bin=target/release/video-proto
load1() { sysctl -n vm.loadavg | awk '{print $2}'; }
for i in 1 2 3; do
  until mkdir /tmp/renpy_proj.run.lock 2>/dev/null; do sleep 5; done
  while (( $(load1) > 4.0 )); do sleep 15; done
  l0=$(load1)
  line=$(/usr/bin/time -l -o out/time_${label}_$i.txt $bin "scratch/$clip" --label "$label" "$@" 2> out/err_${label}_$i.txt)
  l1=$(load1)
  pkill -9 -f "video-proto/target/release/video-proto" 2>/dev/null
  rmdir /tmp/renpy_proj.run.lock
  echo "${line%\}},\"load_before\":$l0,\"load_after\":$l1}" | tee -a out/runs.jsonl
  sleep 2
done
