#!/bin/sh
# Reproduce the image-loading measurements (needs the corpus clones and the perf-baseline / shared-engine-launcher SDKs).
# Tools: nix develop (cargo), nix shell nixpkgs#cmake (turbojpeg/mozjpeg build), nixpkgs#libavif (avifenc), nixpkgs#dav1d(.dev) + pkg-config (avifbench).
set -eu
cd "$(dirname "$0")"
python3 list_rpa.py "/Users/aidanp/Projects/renpy_proj/.worktrees/perf-baseline/corpus/wa-853/game/archive.rpa" "/Users/aidanp/Projects/renpy_proj/.worktrees/perf-baseline/corpus/rip-853/game/images.rpa" > /tmp/rpa_list.tsv
python3 pick.py                                  # seeded sample -> scratch/ (gitignored)
SDL_VIDEODRIVER=dummy ../shared-engine-launcher/sdk/renpy-8.5.3-sdk/lib/py3-mac-universal/python baseline_sdl.py
(cd bench && nix develop ../../.. -c nix shell nixpkgs#cmake -c cargo build --release --features extra,bc7)
./bench/target/release/imgbench scratch 25 > data/decode.tsv
./bench/target/release/imgbench scratch 1 par > data/parallel.tsv
./bench/target/release/imgbench scratch 1 post > data/post.tsv
for f in si_png_1 rip_jpg_0 astral_webp_0; do ./bench/target/release/bc7 scratch/$f.*; done > data/bc7.tsv
python3 summarize.py > data/summary.tsv
# AVIF: encode 3 samples with `avifenc -q 60 -s 8`, then build/run avifbench (PKG_CONFIG_PATH must point at dav1d's .dev output).
