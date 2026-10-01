#!/bin/sh
# Fetch what the synthetic games need and git does not carry (everything lands in gitignored folders):
#   - the Ren'Py SDKs: 8.5.3 (stock engine of the Ren'Py 8 games) and 7.4.11 (compiles and runs the Ren'Py 7 games),
#     into research/test-corpus/sdk/ of the main checkout; 8.5.3 is also linked where corpus.toml `sdk-853` looks for it;
#   - the OFL fonts of the text game, into harness/testgames/fonts/.
# Usage: harness/testgames/fetch.sh [sdk|fonts]   (default: both). Needs curl, tar with bzip2 support.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here" && dirname "$(git rev-parse --path-format=absolute --git-common-dir)")
what=${1:-all}

sdk() {
    dir="$root/research/test-corpus/sdk"
    mkdir -p "$dir"
    for v in 8.5.3 7.4.11; do
        if [ ! -x "$dir/renpy-$v-sdk/renpy.sh" ]; then
            echo "fetch: Ren'Py $v SDK"
            curl -fL "https://www.renpy.org/dl/$v/renpy-$v-sdk.tar.bz2" | tar -xj -C "$dir"
        fi
    done
    link="$root/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk"
    if [ ! -e "$link" ]; then
        mkdir -p "$(dirname "$link")"
        ln -s "$dir/renpy-8.5.3-sdk" "$link"
    fi
}

fonts() {
    dir="$here/fonts"
    mkdir -p "$dir"
    g=https://github.com/google/fonts/raw/main/ofl
    [ -s "$dir/OpenSans-Variable.ttf" ] || curl -fL -o "$dir/OpenSans-Variable.ttf" "$g/opensans/OpenSans%5Bwdth%2Cwght%5D.ttf"
    [ -s "$dir/NotoNaskhArabic-Variable.ttf" ] || curl -fL -o "$dir/NotoNaskhArabic-Variable.ttf" "$g/notonaskharabic/NotoNaskhArabic%5Bwght%5D.ttf"
}

case "$what" in
    sdk) sdk ;;
    fonts) fonts ;;
    all) sdk; fonts ;;
    *) echo "usage: $0 [sdk|fonts]" >&2; exit 2 ;;
esac
