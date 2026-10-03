#!/usr/bin/env bash
# Fails when a file in a package names a Nix store path that the program can load code from (the bytes of the
# files, not only the load commands).
#   check-no-nix-store.sh <package-dir>
# A static library can hardcode a store path in a string (the nixpkgs macOS libffi did, for the dylib that
# holds its closure trampolines). The path does not exist on a user's machine, so the feature breaks there.
#
# Two classes of hit:
#   - a path that ends in .dylib, .so, .a or .dll: a library that can be loaded. This fails the check.
#   - any other path (a default config, locale, include or data folder in OpenSSL, GnuTLS, FFmpeg and others):
#     a missing folder is skipped by those libraries. It is listed and counted, and does not fail the check.
# NIX_STORE_ALLOW is an extended regular expression for "<file name> <path>" pairs that are known and accepted.
# Text in the licenses folder is not checked.
set -euo pipefail
dir=${1:?usage: check-no-nix-store.sh <package-dir>}
[ -d "$dir" ] || { echo "check-no-nix-store: $dir is not a directory" >&2; exit 1; }
allow=${NIX_STORE_ALLOW:-^$}
bad=0 info=0
while IFS= read -r f; do
    hits=$(strings -a "$f" | grep -Eo '/nix/store/[a-z0-9]{32}-[^ "<>]*' | sort -u || true)
    [ -n "$hits" ] || continue
    name=$(basename "$f")
    while IFS= read -r p; do
        if [[ "$p" =~ \.(dylib|so|a|dll)$ ]]; then
            if [[ "$name $p" =~ $allow ]]; then
                echo "check-no-nix-store: accepted: $name names $p" >&2
            else
                echo "check-no-nix-store: $name names the library $p" >&2
                bad=1
            fi
        else
            info=$((info + 1))
        fi
    done <<<"$hits"
done < <(find "$dir" -type f -not -path '*/licenses/*')
[ "$bad" = 0 ] || exit 1
echo "check-no-nix-store: no /nix/store library path in $dir ($info data or config folder strings, not loadable)"
