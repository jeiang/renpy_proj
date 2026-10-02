#!/usr/bin/env bash
# Regenerates RUST_DEPENDENCIES.md (licence texts of every Rust dependency) with cargo-about.
# Run it after Cargo.lock changes. Config: player/about.toml. Template: player/about.hbs.
#
#   player/packaging/licences/generate.sh
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
player="$(cd "$here/../.." && pwd)"
cd "$player"
run=(cargo about)
command -v cargo-about >/dev/null || run=(nix shell nixpkgs#cargo-about nixpkgs#cargo nixpkgs#rustc -c cargo about)
"${run[@]}" generate --offline --locked -c about.toml about.hbs -o "$here/RUST_DEPENDENCIES.md"
echo "wrote $here/RUST_DEPENDENCIES.md"
