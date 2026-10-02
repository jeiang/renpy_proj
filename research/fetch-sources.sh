#!/bin/sh
# Fetch the third-party source trees that research notes read. Everything lands in gitignored folders of the main
# checkout. Pinned to the tags and commits that the notes cite.
#
#   research/fetch-sources.sh [name...]     names: rpyc-loading python-embedding gpu-media version-drift mesa
#                                           (default: all except mesa, which is large; ask for it by name)
#
#   rpyc-loading     research/rpyc-loading/{unrpyc,unrpa,renpy}
#   python-embedding research/python-embedding/{renpy,renpy-build,rustpython}
#   gpu-media        research/gpu-media/renpy-src
#   version-drift    research/version-drift/renpy-src   (tags only, blobs on demand)
#   mesa             research/mesa-src                  (Mesa 26.2.3, the anisotropy investigation)
#
# Other research folders have their own fetch.sh (engine-anatomy, lan-streaming, licence, perf-baseline, py2compat-facts,
# pypack, renderer, renpy7-differences, renpy7-on-8, win-spike). SDKs: research/fetch-sdks.sh.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=${REPO_ROOT:-$(dirname "$(git -C "$here" rev-parse --path-format=absolute --git-common-dir)")}
RENPY_TAG=8.5.3.26051504
RENPY_URL=https://github.com/renpy/renpy.git

clone() { # url dest [git clone args...]
    url=$1; dest=$2; shift 2
    [ -d "$dest/.git" ] && { echo "fetch-sources: $dest exists"; return 0; }
    mkdir -p "$(dirname "$dest")"
    echo "fetch-sources: $url -> $dest"
    git clone -q "$@" "$url" "$dest"
}

clone_at() { # url dest commit: full history is small for these repos, then check out the pinned commit
    clone "$1" "$2"
    git -C "$2" checkout -q "$3"
}

rpyc_loading() {
    d=$root/research/rpyc-loading
    clone_at https://github.com/CensoredUsername/unrpyc.git "$d/unrpyc" 3ae8334ed71a05535927dcc559663d3aca51215b
    clone_at https://github.com/Lattyware/unrpa.git "$d/unrpa" 005b10abec590db374f23fd8d4b111963792a15a
    clone "$RENPY_URL" "$d/renpy" --depth 1 --branch "$RENPY_TAG"
    echo "fetch-sources: the SDK for this folder comes from research/fetch-sdks.sh"
}

python_embedding() {
    d=$root/research/python-embedding
    clone "$RENPY_URL" "$d/renpy" --depth 1 --branch "$RENPY_TAG"
    if [ ! -d "$d/renpy-build/.git" ]; then
        clone https://github.com/renpy/renpy-build.git "$d/renpy-build" --depth 1
        git -C "$d/renpy-build" fetch -q --depth 1 origin tag "renpy-$RENPY_TAG"
        git -C "$d/renpy-build" checkout -q "renpy-$RENPY_TAG"
    fi
    clone https://github.com/RustPython/RustPython.git "$d/rustpython" --depth 1
}

gpu_media() {
    clone "$RENPY_URL" "$root/research/gpu-media/renpy-src" --depth 1 --branch "$RENPY_TAG"
}

version_drift() {
    d=$root/research/version-drift/renpy-src
    clone "$RENPY_URL" "$d" --filter=blob:none
    git -C "$d" fetch -q --tags
    git -C "$d" checkout -q "$RENPY_TAG"
}

mesa() {
    clone https://gitlab.freedesktop.org/mesa/mesa.git "$root/research/mesa-src" --depth 1 --branch mesa-26.2.3
}

names=${*:-"rpyc-loading python-embedding gpu-media version-drift"}
for n in $names; do
    case "$n" in
        rpyc-loading) rpyc_loading ;;
        python-embedding) python_embedding ;;
        gpu-media) gpu_media ;;
        version-drift) version_drift ;;
        mesa) mesa ;;
        *) echo "fetch-sources: unknown name $n" >&2; exit 2 ;;
    esac
done
