#!/usr/bin/env bash
# Usage: environment.sh regenerate.sh OLD_REPO OUTPUT SCRATCH [games]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$1" && pwd)
out=$2
scratch=$3
games=${4:-story,text,view,media,tl,loop,py7}
mkdir -p "$out" "$scratch"
python3 "$here/shaders/capture.py" --repo "$repo" --out "$out/shaders" \
    --renpy-source "$repo/research/test-corpus/sdk/renpy-8.5.3-sdk"
python3 "$here/capture.py" --repo "$repo" --binary "$repo/player/target/release/urp" \
    --out "$out" --scratch "$scratch" --games "$games" --probes
python3 - "$repo" "$out" <<'PY'
import hashlib, json, os, pathlib, platform, subprocess, sys
repo, out = map(pathlib.Path, sys.argv[1:])
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
inputs = {}
for root in ('harness/testgames', 'harness/rpy', 'harness/plans'):
    for p in sorted((repo / root).rglob('*')):
        if p.is_file() and ('build' not in p.relative_to(repo / root).parts and '__pycache__' not in p.parts):
            inputs[str(p.relative_to(repo))] = sha(p)
value = {'source_commit': subprocess.check_output(['git', '-C', str(repo), 'rev-parse', 'HEAD'], text=True).strip(),
         'binary_sha256': sha(repo / 'player/target/release/urp'), 'flake_lock_sha256': sha(repo / 'flake.lock'),
         'host': platform.node(), 'platform': platform.platform(), 'architecture': platform.machine(),
         'renderer': 'wgpu-vulkan-lavapipe', 'vulkan_icd_sha256': sha(pathlib.Path(os.environ['VK_ICD_FILENAMES'])),
         'vulkan_icd': os.environ['VK_ICD_FILENAMES'], 'lp_num_threads': os.environ.get('LP_NUM_THREADS'),
         'inputs': inputs}
(out / 'provenance.json').write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
PY
