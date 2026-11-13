#!/usr/bin/env bash
# Usage, from the old repository root: environment.sh regenerate-all.sh OLD_REPO WORK DEST
# Captures twice in separate player processes, then packages run 1 with the measured differences.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$1
work=$2
dest=$3
for n in 1 2; do
    "$here/regenerate.sh" "$repo" "$work/run$n" "$work/scratch$n"
done
python3 "$here/package.py" "$work/run1" "$work/run2" "$dest"
