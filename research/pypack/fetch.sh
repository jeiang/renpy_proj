#!/bin/sh
# Re-download third-party inputs into gitignored upstream/ (ticket #26).
set -e; cd "$(dirname "$0")"; mkdir -p upstream; cd upstream
TAG=20260924; V=3.12.14
gh release download $TAG -R astral-sh/python-build-standalone -p "cpython-$V+$TAG-aarch64-apple-darwin-pgo+lto-full.tar.zst" --skip-existing
for d in distributions quirks running behavior; do
  curl -sSL -o pbs-$d.rst https://raw.githubusercontent.com/astral-sh/python-build-standalone/main/docs/$d.rst
done
mkdir -p pbs-mac && tar --zstd -xf "cpython-$V+$TAG-aarch64-apple-darwin-pgo+lto-full.tar.zst" -C pbs-mac
# CPython source used for the static-embed probe (same version renpy-build 8.5.3 builds for mac/linux)
[ -d src/Python-3.12.8 ] || { mkdir -p src; curl -sSL https://www.python.org/ftp/python/3.12.8/Python-3.12.8.tar.xz | tar -xJ -C src; }
# PYTHON.json of the other PBS platforms (link mode / object format facts)
for t in x86_64-pc-windows-msvc-pgo x86_64-unknown-linux-gnu-pgo+lto x86_64-apple-darwin-pgo+lto; do
  f="cpython-$V+$TAG-$t-full.tar.zst"; gh release download $TAG -R astral-sh/python-build-standalone -p "$f" --skip-existing
  tar --zstd -xf "$f" -O python/PYTHON.json > PYTHON-$t.json
done
