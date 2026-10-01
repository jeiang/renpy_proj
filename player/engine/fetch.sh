#!/usr/bin/env bash
# Fetch Ren'Py 8.5.3 (tag 8.5.3.26051504) into player/upstream/renpy-8.5.3/ (gitignored).
# RENPY_GIT_URL overrides the clone source (a local clone or a mirror also works).
set -euo pipefail

TAG="8.5.3.26051504"
COMMIT_PREFIX="39895c1e"
URL="${RENPY_GIT_URL:-https://github.com/renpy/renpy}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${HERE}/../upstream/renpy-8.5.3"

# Pure-Python packages that Ren'Py imports and the official build bundles (savetoken.py needs ecdsa, which needs six; games use requests).
# They are wheels, checked by SHA-256, unpacked by build.py into the layer zip.
WHEELS="${HERE}/../upstream/pywheels"
mkdir -p "${WHEELS}"
# macOS has shasum; Git for Windows and Linux have sha256sum.
sha256() { if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d' ' -f1; else sha256sum "$1" | cut -d' ' -f1; fi; }
fetch_wheel() { # file url sha256
    local file="${WHEELS}/$1"
    if [ -f "${file}" ] && [ "$(sha256 "${file}")" = "$3" ]; then return 0; fi
    curl -fsSL -o "${file}.tmp" "$2"
    [ "$(sha256 "${file}.tmp")" = "$3" ] || { echo "checksum mismatch for $1" >&2; rm -f "${file}.tmp"; exit 1; }
    mv "${file}.tmp" "${file}"
}
while read -r name url sum; do
    case "${name}" in ''|'#'*) continue ;; esac
    fetch_wheel "${name}" "${url}" "${sum}"
done < "${HERE}/wheels.txt"

if [ -d "${DEST}/.git" ] && [ "$(git -C "${DEST}" describe --tags --exact-match 2>/dev/null || true)" = "${TAG}" ]; then
    echo "renpy ${TAG} already present in ${DEST}"
    exit 0
fi

mkdir -p "$(dirname "${DEST}")"
rm -rf "${DEST}.tmp"
git -c advice.detachedHead=false -c core.autocrlf=false clone --quiet --depth 1 --branch "${TAG}" "${URL}" "${DEST}.tmp"
head="$(git -C "${DEST}.tmp" rev-parse HEAD)"
case "${head}" in
    "${COMMIT_PREFIX}"*) ;;
    *) echo "unexpected commit ${head} for tag ${TAG}" >&2; rm -rf "${DEST}.tmp"; exit 1 ;;
esac
rm -rf "${DEST}"
mv "${DEST}.tmp" "${DEST}"
echo "fetched renpy ${TAG} (${head}) into ${DEST}"
