#!/usr/bin/env bash
# Copies the licence files of a binary package into <dest> (created if needed):
#   LICENSE-MIT, LICENSE-APACHE, THIRD_PARTY.md, and every file of player/packaging/licences
#   except the scripts. Used by macos.sh, linux.sh and manylinux/container-build.sh.
#   windows.ps1 does the same with PowerShell.
#
#   player/packaging/licences/stage.sh <dest>
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../../.." && pwd)"
dest="${1:?usage: stage.sh <dest>}"
mkdir -p "$dest"
for f in LICENSE-MIT LICENSE-APACHE THIRD_PARTY.md; do
    [ -f "$repo/$f" ] || { echo "stage.sh: missing $repo/$f" >&2; exit 1; }
    cp "$repo/$f" "$dest/$f"
done
for f in "$here"/*.txt "$here"/RUST_DEPENDENCIES.md; do
    [ -f "$f" ] || { echo "stage.sh: missing $f" >&2; exit 1; }
    cp "$f" "$dest/$(basename "$f")"
done
