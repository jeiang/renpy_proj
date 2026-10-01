#!/usr/bin/env bash
# check-glibc.sh <dir-or-file>...: the manylinux_2_28 symbol-version policy (auditwheel style).
# Lists the newest GLIBC, GLIBCXX, CXXABI and GCC symbol version that every ELF file under the arguments
# needs, from `objdump -T`, and fails when one exceeds the limit. Needs objdump (binutils).
set -euo pipefail
LIM_GLIBC=2.28; LIM_GLIBCXX=3.4.25; LIM_CXXABI=1.3.11; LIM_GCC=8.0.0   # manylinux_2_28 policy
newer() { [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | tail -1)" = "$1" ]; }
rc=0; declare -A MAX=([GLIBC]=0 [GLIBCXX]=0 [CXXABI]=0 [GCC]=0)
while IFS= read -r f; do
  head -c4 "$f" 2>/dev/null | grep -q 'ELF' || continue
  line="$(basename "$f"):"
  for fam in GLIBC GLIBCXX CXXABI GCC; do
    lim=LIM_$fam; lim=${!lim}
    # only versions the file REQUIRES: undefined dynamic symbols carry "(GLIBC_x.y)" after the section column
    max=$(objdump -T "$f" 2>/dev/null | awk '$2!="" && /\*UND\*/' | grep -o "[(]${fam}_[0-9.]*[)]" | tr -d '()' | sed "s/^${fam}_//" | sort -V | tail -1 || true)
    [ -n "$max" ] || max=-
    line="$line $fam=$max"
    if [ "$max" != - ]; then
      newer "$max" "${MAX[$fam]}" && MAX[$fam]=$max
      if newer "$max" "$lim"; then line="$line(EXCEEDS $lim)"; rc=1; fi
    fi
  done
  echo "$line"
done < <(find "$@" -type f \( -name player -o -name '*.so*' \))
for k in GLIBC GLIBCXX CXXABI GCC; do [ "${MAX[$k]}" != 0 ] || MAX[$k]=none; done
echo "max: GLIBC=${MAX[GLIBC]} (limit $LIM_GLIBC) GLIBCXX=${MAX[GLIBCXX]} (limit $LIM_GLIBCXX) CXXABI=${MAX[CXXABI]} (limit $LIM_CXXABI) GCC=${MAX[GCC]} (limit $LIM_GCC)"
[ $rc = 0 ] && echo "check-glibc: OK (manylinux_2_28)" || echo "check-glibc: FAILED" >&2
exit $rc
