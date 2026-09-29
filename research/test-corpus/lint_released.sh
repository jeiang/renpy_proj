#!/bin/sh
# Lint each released corpus copy with its matching engine (no window; lint is a non-display command).
# RENPY_PATH_TO_SAVES moves saves, persistent AND the global tokens dir (security_keys.txt, upgraded.txt)
# to scratch; --savedir alone leaves ~/Library/RenPy/tokens writable. ~/Library/RenPy is compared before/after.
# Run from anywhere, outside nix develop. Needs sdk/renpy-8.2.3-sdk (see README) and the 8.0.1 SDK from
# research/shared-engine-launcher.
set -u
here=$(cd "$(dirname "$0")" && pwd); corpus=$here/../../corpus; out=$here/scratch
rm -rf "$out/saves"; mkdir -p "$out/saves"
export RENPY_PATH_TO_SAVES=$out/saves
snap() { find "$HOME/Library/RenPy" -type f -exec /bin/ls -lT {} + 2>/dev/null | shasum; }
before=$(snap)
lint() { # name, command...
  name=$1; shift
  "$@" lint > "$out/$name.lint.out" 2>&1
  echo "$name: rc=$? $(grep -m1 -E 'Statistics|rror|Traceback' "$out/$name.lint.out")"
}
rip=$corpus/Ripples-released.app/Contents
lint Ripples "$rip/MacOS/Ripples" "$rip/Resources/autorun"
lint SecretIsland "$here/../shared-engine-launcher/sdk/renpy-8.0.1-sdk/renpy.sh" "$corpus/SecretIsland-0.18.8.0-pc-released"
lint WaifuAcademy "$here/sdk/renpy-8.2.3-sdk/renpy.sh" "$corpus/WaifuAcademy-0.13.5-pc-released"
[ "$before" = "$(snap)" ] && echo "~/Library/RenPy unchanged" || echo "WARNING: ~/Library/RenPy changed"
