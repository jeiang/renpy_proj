#!/usr/bin/env bash
# Host-side driver: runs container-build.sh in quay.io/pypa/manylinux_2_28_x86_64 with rootless podman.
#   player/packaging/manylinux/build.sh [CACHE_DIR]
# Run it from a checkout of the repository on a Linux x86_64 host. Output (gitignored):
#   player/build-out/manylinux/player-linux-x86_64/ and player-linux-x86_64.tar.gz
# CACHE_DIR (default ~/.cache/renpy-manylinux) keeps downloads, static deps, FFmpeg and the Rust toolchain.
# PLAYER_CHECKS=1 also runs `cargo clippy -- -D warnings` and `cargo test --release` in the container (CI does).
# Needs `podman` (rootless: newuidmap, newgidmap, subuid and subgid entries). Without podman on PATH it uses
# `nix shell nixpkgs#podman nixpkgs#fuse-overlayfs`. Without ~/.config/containers/{policy,registries,storage}.conf
# the script writes minimal ones. PODMAN_GRAPHROOT puts the image store somewhere else (default: podman's own).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
CACHE="$(mkdir -p "${1:-$HOME/.cache/renpy-manylinux}" && cd "${1:-$HOME/.cache/renpy-manylinux}" && pwd)"
# Image pinned by digest (tag manylinux_2_28_x86_64, pulled 2026-10-01).
IMAGE="${MANYLINUX_IMAGE:-quay.io/pypa/manylinux_2_28_x86_64@sha256:c2261579b9c2e5d45aa93312f73e2a302182e3e977b558581a1838d6fed3d8e6}"
[ "$(uname -s)-$(uname -m)" = Linux-x86_64 ] || { echo "build.sh: Linux x86_64 only" >&2; exit 1; }
export PATH=/run/wrappers/bin:$PATH   # NixOS setuid newuidmap and newgidmap

CONF="${XDG_CONFIG_HOME:-$HOME/.config}/containers"; mkdir -p "$CONF"
[ -f "$CONF/policy.json" ] || echo '{"default":[{"type":"insecureAcceptAnything"}]}' > "$CONF/policy.json"
[ -f "$CONF/registries.conf" ] || echo 'unqualified-search-registries = ["quay.io"]' > "$CONF/registries.conf"
if [ -n "${PODMAN_GRAPHROOT:-}" ] && [ ! -f "$CONF/storage.conf" ]; then
  mkdir -p "$PODMAN_GRAPHROOT"
  printf '[storage]\ndriver = "overlay"\ngraphroot = "%s"\nrunroot = "/run/user/%s/containers"\n' "$PODMAN_GRAPHROOT" "$(id -u)" > "$CONF/storage.conf"
fi

run=(podman)
command -v podman >/dev/null || run=(nix shell nixpkgs#podman nixpkgs#fuse-overlayfs --command podman)
# Container root maps to the calling user, so build outputs in the clone belong to the user.
exec "${run[@]}" run --rm --name "renpy-manylinux-$$" \
  -e PLAYER_CHECKS="${PLAYER_CHECKS:-0}" \
  -v "$REPO:/src" -v "$CACHE:/cache" \
  "$IMAGE" bash /src/player/packaging/manylinux/container-build.sh
