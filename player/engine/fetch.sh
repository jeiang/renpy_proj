#!/usr/bin/env bash
# Fetch Ren'Py 8.5.3 (tag 8.5.3.26051504) into player/upstream/renpy-8.5.3/ (gitignored).
# RENPY_GIT_URL overrides the clone source (a local clone or a mirror also works).
set -euo pipefail

TAG="8.5.3.26051504"
COMMIT_PREFIX="39895c1e"
URL="${RENPY_GIT_URL:-https://github.com/renpy/renpy}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${HERE}/../upstream/renpy-8.5.3"

if [ -d "${DEST}/.git" ] && [ "$(git -C "${DEST}" describe --tags --exact-match 2>/dev/null || true)" = "${TAG}" ]; then
    echo "renpy ${TAG} already present in ${DEST}"
    exit 0
fi

mkdir -p "$(dirname "${DEST}")"
rm -rf "${DEST}.tmp"
git -c advice.detachedHead=false clone --quiet --depth 1 --branch "${TAG}" "${URL}" "${DEST}.tmp"
head="$(git -C "${DEST}.tmp" rev-parse HEAD)"
case "${head}" in
    "${COMMIT_PREFIX}"*) ;;
    *) echo "unexpected commit ${head} for tag ${TAG}" >&2; rm -rf "${DEST}.tmp"; exit 1 ;;
esac
rm -rf "${DEST}"
mv "${DEST}.tmp" "${DEST}"
echo "fetched renpy ${TAG} (${head}) into ${DEST}"
