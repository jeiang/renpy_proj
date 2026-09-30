#!/bin/zsh
# THROWAWAY: decode-only ceiling (no window, no clock; frames transferred to CPU memory then discarded). 1 run each, 8 s.
cd "${0:A:h}"
for spec in ${CEIL_SPECS:-"alice10bit.webm" "alice10bit.webm --hw off" "pc444.webm" "av1_4k60_10bit.mkv" "wa_theora.ogv"}; do
  set -- ${=spec}; clip=$1; shift
  until mkdir /tmp/renpy_proj.run.lock 2>/dev/null; do sleep 5; done
  while (( $(sysctl -n vm.loadavg | awk '{print $2}') > 4.0 )); do sleep 15; done
  l0=$(sysctl -n vm.loadavg | awk '{print $2}')
  line=$(target/release/video-proto scratch/$clip --mode decode --secs 8 --warmup 2 --label "decode $*" "$@" 2>/dev/null)
  l1=$(sysctl -n vm.loadavg | awk '{print $2}')
  rmdir /tmp/renpy_proj.run.lock
  echo "${line%\}},\"load_before\":$l0,\"load_after\":$l1}" | tee -a out/ceiling.jsonl
done
