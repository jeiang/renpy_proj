#!/usr/bin/env bash
# Read-only survey of Ren'Py games under a directory (default ~/Games). Prints one TSV row per game.
# Loose files and RPA-3.0 archive contents are both counted ("loose+archived").
# Run inside `nix develop` (needs ffprobe, python3).
set -uo pipefail
root=${1:-$HOME/Games}
here=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
vre='.*\.\(webm\|mkv\|ogv\|mp4\|avi\|mov\|mpg\|mpeg\)'
printf 'game\tformat\trenpy\tpython\tscript_version\trpy\trpyc\tnon_rpc2_rpyc\trpa\trpa_headers\timages\tvideos\tlargest_video\tshader_rpy\tgame_size\tengine_size\n'
for d in "$root"/*/; do
  d=${d%/}; name=$(basename "$d")
  if [[ $name == *.app ]]; then
    base=$d/Contents/Resources/autorun; lib=$d/Contents/Resources/lib; fmt=mac
  else
    base=$d; lib=$d/lib; fmt=pc
  fi
  [[ -d $base/renpy && -d $base/game ]] || { printf '%s\tnot-renpy\n' "$name"; continue; }
  g=$base/game
  py=$(ls -d "$lib"/python* "$lib"/*/lib/python* "$lib"/*/python* 2>/dev/null | grep -oE 'python[0-9]+\.[0-9]+' | sort -u | tr '\n' ' ')
  # Ren'Py 7.5/8.0 list a Py2 and a Py3 version_tuple in one file; the Py3 one (last) applies to python3 builds.
  pick=(head -n1); [[ $py == *python3* ]] && pick=(tail -n1)
  ver=$(grep -h -oE 'version_tuple = \([0-9, ]+' "$base/renpy/__init__.py" 2>/dev/null | "${pick[@]}" | sed 's/.*(//; s/, */./g; s/\.$//')
  [[ -z $ver ]] && ver=$(grep -h -m1 -oE "^version = u?['\"][^'\"]+" "$base/renpy/vc_version.py" 2>/dev/null | sed "s/.*['\"]//")
  sv=$(tr -d '\r\n' < "$g/script_version.txt" 2>/dev/null)
  rpas=(); while IFS= read -r -d '' f; do rpas+=("$f"); done < <(find "$g" -name '*.rpa' -print0)
  a=(0 0 0 0 0 - 0 0)
  if (( ${#rpas[@]} )); then
    read -r -a a < <(python3 "$here/rpa_scan.py" --dump-largest-video "$tmp/v" "${rpas[@]}")
  fi
  rpah=$(for f in "${rpas[@]}"; do head -c 7 "$f"; echo; done | sort | uniq -c | awk 'NF{printf "%s×%s ", $2, $1}')
  rpy=$(find "$g" -name '*.rpy' | wc -l | tr -d ' ')
  rpyc=$(find "$g" -name '*.rpyc' | wc -l | tr -d ' ')
  bad=$(find "$g" -name '*.rpyc' -exec sh -c 'for f; do [ "$(head -c 10 "$f")" = "RENPY RPC2" ] || echo x; done' _ {} + | wc -l | tr -d ' ')
  imgs=$(find "$g" -iregex '.*\.\(png\|jpg\|jpeg\|webp\|avif\)' | wc -l | tr -d ' ')
  vids=$(find "$g" -iregex "$vre" | wc -l | tr -d ' ')
  big=$(find "$g" -iregex "$vre" -print0 | xargs -0 ls -S 2>/dev/null | head -n1)
  bigsize=0; [[ -n $big ]] && bigsize=$(wc -c < "$big" | tr -d ' ')
  if (( ${a[6]} > bigsize )); then big=$tmp/v; fi
  maxv=""
  [[ -n $big ]] && maxv=$(ffprobe -v error -select_streams v:0 -show_entries stream=codec_name,width,height,r_frame_rate -of csv=p=0 "$big" 2>/dev/null | head -n1)
  rm -f "$tmp/v"
  sh=$(grep -rlE 'register_shader|gl_FragColor' --include='*.rpy' "$g" 2>/dev/null | wc -l | tr -d ' ')
  gs=$(du -sh "$g" | cut -f1)
  es=$(du -shc "$base/renpy" "$lib" 2>/dev/null | tail -n1 | cut -f1)
  printf '%s\t%s\t%s\t%s\t%s\t%s+%s\t%s+%s\t%s\t%s\t%s\t%s+%s\t%s+%s\t%s\t%s\t%s\t%s\n' \
    "$name" "$fmt" "$ver" "${py% }" "$sv" "$rpy" "${a[2]}" "$rpyc" "${a[1]}" "$bad" "${#rpas[@]}" "${rpah:--}" \
    "$imgs" "${a[3]}" "$vids" "${a[4]}" "${maxv:--}" "$sh" "$gs" "$es"
done
