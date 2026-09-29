#!/bin/sh
# Re-fetch the sources measured by measure.py into ./src (gitignored).
set -e
cd "$(dirname "$0")"
mkdir -p src && cd src
# 8.5.3 final = annotated tag 8.5.3.26051504 -> commit 39895c1e017f0b36ffea2447d97eccd69d76ee1c
[ -d renpy ] || git clone -q --depth 1 --branch 8.5.3.26051504 https://github.com/renpy/renpy.git
# pygame_sdl2 standalone repo: tag renpy-8.5.3.26051504 -> 1c8de534e0de2b521900e92ed27337c28f9bc5ee (last commit 2025-06-02)
[ -d pygame_sdl2 ] || git clone -q --depth 1 --branch renpy-8.5.3.26051504 https://github.com/renpy/pygame_sdl2.git
