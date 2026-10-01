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
fetch_wheel() { # name file url sha256
    local file="${WHEELS}/$1"
    if [ -f "${file}" ] && [ "$(sha256 "${file}")" = "$3" ]; then return 0; fi
    curl -fsSL -o "${file}.tmp" "$2"
    [ "$(sha256 "${file}.tmp")" = "$3" ] || { echo "checksum mismatch for $1" >&2; rm -f "${file}.tmp"; exit 1; }
    mv "${file}.tmp" "${file}"
}
fetch_wheel ecdsa-0.19.2-py2.py3-none-any.whl \
    https://files.pythonhosted.org/packages/51/79/119091c98e2bf49e24ed9f3ae69f816d715d2904aefa6a2baa039a2ba0b0/ecdsa-0.19.2-py2.py3-none-any.whl \
    840f5dc5e375c68f36c1a7a5b9caad28f95daa65185c9253c0c08dd952bb7399
fetch_wheel six-1.17.0-py2.py3-none-any.whl \
    https://files.pythonhosted.org/packages/b7/ce/149a00dd41f10bc29e5921b496af8b574d8413afcd5e30dfa0ed46c2cc5e/six-1.17.0-py2.py3-none-any.whl \
    4721f391ed90541fddacab5acf947aa0d3dc7d27b2e1e8eda2be8970586c3274
fetch_wheel requests-2.34.2-py3-none-any.whl \
    https://files.pythonhosted.org/packages/a0/f4/c67b0b3f1b9245e8d266f0f112c500d50e5b4e83cb6f3b71b6528104182a/requests-2.34.2-py3-none-any.whl \
    2a0d60c172f83ac6ab31e4554906c0f3b3588d37b5cb939b1c061f4907e278e0
fetch_wheel urllib3-2.8.0-py3-none-any.whl \
    https://files.pythonhosted.org/packages/92/9d/c4e665119135114480843e7ab388fa94d8480650450e6f8e26b70d323a4c/urllib3-2.8.0-py3-none-any.whl \
    0cf3cae568d36aa9576b28dfb35f11328f1cb974ca7647d9475ebb86c75ac6e3
fetch_wheel idna-3.20-py3-none-any.whl \
    https://files.pythonhosted.org/packages/58/a2/bb081bab032533a855d44de1d56f8e8426114ff1ba5d1f07a438a0a654f8/idna-3.20-py3-none-any.whl \
    ab7ae7122974553370f0bdb919e1a960b2cd1bc1ef0276416d896db81c14582c
fetch_wheel certifi-2026.7.22-py3-none-any.whl \
    https://files.pythonhosted.org/packages/0b/a7/71ac2cff56fec219ed242bb11b8efb69fcc4bec75db06fb7bfe35de520e6/certifi-2026.7.22-py3-none-any.whl \
    62f22742b58a1a33014a2b6b706588a8d7e2a88ae7bd1a6ebe8c992928483775
fetch_wheel charset_normalizer-3.5.2-py3-none-any.whl \
    https://files.pythonhosted.org/packages/fc/ad/d07d7862a62ffa6d79d68074d14823243dd235a77c45262acbf6adeb28bf/charset_normalizer-3.5.2-py3-none-any.whl \
    b6b751274acb69d77b3323d6b7dbaa3c7fdfc1eb829b7eb61d262f32e1af9685

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
