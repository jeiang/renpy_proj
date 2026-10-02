#!/bin/sh
# Fetch the stock Ren'Py SDKs that harness/corpus.toml and the research scripts read. Everything lands in
# gitignored folders of the main checkout:
#   research/test-corpus/sdk/renpy-<v>-sdk/             7.4.5 7.4.8 7.4.11 7.5.3 7.6.1 7.7.3 7.8.2 8.2.3 8.5.3
#   research/shared-engine-launcher/sdk/renpy-<v>-sdk/  8.0.1 (a real download), 8.5.3 (a link to the one above)
# Ren'Py 7 SDKs hold x86_64 Python 2: on an Apple Silicon Mac they run under Rosetta. 8.0.1 is x86_64 only.
# The harness/testgames/fetch.sh script fetches only 8.5.3 and 7.4.11. This one fetches the full set.
#
#   research/fetch-sdks.sh [<version>...]     (default: every version above)
#
# Needs curl and tar with bzip2 support. Downloads come from https://www.renpy.org/dl/. No checksum list is
# pinned: renpy.org publishes md5 and sha1 only (research/shared-engine-launcher/README.md has two SHA-256 values
# that were checked by hand: 8.0.1 a2a58082..., 8.5.3 eb0a9be7...).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=${REPO_ROOT:-$(dirname "$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)")}
shared="7.4.5 7.4.8 7.4.11 7.5.3 7.6.1 7.7.3 7.8.2 8.2.3 8.5.3"
versions=${*:-"$shared 8.0.1"}

fetch() { # version dest-dir
    [ -x "$2/renpy-$1-sdk/renpy.sh" ] && return 0
    mkdir -p "$2"
    echo "fetch-sdks: Ren'Py $1 -> $2"
    curl -fL "https://www.renpy.org/dl/$1/renpy-$1-sdk.tar.bz2" | tar -xj -C "$2"
}

for v in $versions; do
    case " $shared " in
        *" $v "*) fetch "$v" "$root/research/test-corpus/sdk" ;;
    esac
    case "$v" in
        8.0.1) fetch 8.0.1 "$root/research/shared-engine-launcher/sdk" ;;
        8.5.3)
            link="$root/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk"
            if [ ! -e "$link" ]; then
                mkdir -p "$(dirname "$link")"
                ln -s "$root/research/test-corpus/sdk/renpy-8.5.3-sdk" "$link"
            fi ;;
    esac
    case " $shared 8.0.1 " in
        *" $v "*) ;;
        *) echo "fetch-sdks: unknown version $v (known: $shared 8.0.1)" >&2; exit 2 ;;
    esac
done
