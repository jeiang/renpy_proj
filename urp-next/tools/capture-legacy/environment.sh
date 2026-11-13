#!/usr/bin/env bash
# Run from the old repository root. Pin software Vulkan to that repository's flake.lock.
set -euo pipefail
mesa=$(nix build --impure --no-link --print-out-paths --expr 'let f = builtins.getFlake (toString ./.); p = import f.inputs.nixpkgs { system = "x86_64-linux"; }; in p.mesa')
export VK_ICD_FILENAMES="$mesa/share/vulkan/icd.d/lvp_icd.x86_64.json"
if [[ ! -f "$VK_ICD_FILENAMES" ]]; then
    export VK_ICD_FILENAMES="$mesa/share/vulkan/icd.d/lvp_icd.json"
fi
[[ -f "$VK_ICD_FILENAMES" ]]
export WGPU_BACKEND=vulkan
export WGPU_ADAPTER_NAME=llvmpipe
export LP_NUM_THREADS=1
exec nix develop .#player -c "$@"
