#!/usr/bin/env bash
# Fetch Ren'Py 8.5.3 (tag 8.5.3.26051504) into player/upstream/renpy-8.5.3/ (gitignored).
# RENPY_GIT_URL overrides the clone source (a local clone or a mirror also works).
set -euo pipefail

TAG="8.5.3.26051504"
COMMIT_PREFIX="39895c1e"
URL="${RENPY_GIT_URL:-https://github.com/renpy/renpy}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="${HERE}/../upstream/renpy-8.5.3"

# Pure-Python packages that Ren'Py imports and the official build bundles (savetoken.py needs ecdsa, which needs six).
# They are wheels, checked by SHA-256, unpacked by build.py into the layer zip.
WHEELS="${HERE}/../upstream/pywheels"
mkdir -p "${WHEELS}"
fetch_wheel() { # name file url sha256
    local file="${WHEELS}/$1"
    if [ -f "${file}" ] && [ "$(shasum -a 256 "${file}" | cut -d' ' -f1)" = "$3" ]; then return 0; fi
    curl -fsSL -o "${file}.tmp" "$2"
    [ "$(shasum -a 256 "${file}.tmp" | cut -d' ' -f1)" = "$3" ] || { echo "checksum mismatch for $1" >&2; rm -f "${file}.tmp"; exit 1; }
    mv "${file}.tmp" "${file}"
}
fetch_wheel ecdsa-0.19.2-py2.py3-none-any.whl \
    https://files.pythonhosted.org/packages/51/79/119091c98e2bf49e24ed9f3ae69f816d715d2904aefa6a2baa039a2ba0b0/ecdsa-0.19.2-py2.py3-none-any.whl \
    840f5dc5e375c68f36c1a7a5b9caad28f95daa65185c9253c0c08dd952bb7399
fetch_wheel six-1.17.0-py2.py3-none-any.whl \
    https://files.pythonhosted.org/packages/b7/ce/149a00dd41f10bc29e5921b496af8b574d8413afcd5e30dfa0ed46c2cc5e/six-1.17.0-py2.py3-none-any.whl \
    4721f391ed90541fddacab5acf947aa0d3dc7d27b2e1e8eda2be8970586c3274

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
